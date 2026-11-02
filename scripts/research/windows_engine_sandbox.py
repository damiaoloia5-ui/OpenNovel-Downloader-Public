"""Windows AppContainer launcher. Fail closed; never run an ordinary fallback child.

An ephemeral SID receives RX on one copied executable only. No capability SIDs,
network exemptions, user-library ACL changes, or elevated service.
Only stdin/stdout handles are inherited. A kill-on-close job owns the child.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import os
import pathlib
import secrets
import shutil
import subprocess
import tempfile
import time


class SandboxError(RuntimeError):
    pass


def _api(dll, name, result, *args):
    fn = getattr(dll, name)
    fn.restype, fn.argtypes = result, args
    return fn


def _ok(value):
    if not value:
        raise SandboxError(f'Windows isolation failed ({c.get_last_error()})')
    return value


class SecurityCapabilities(c.Structure):
    _fields_ = [('sid', c.c_void_p), ('capabilities', c.c_void_p), ('count', w.DWORD), ('reserved', w.DWORD)]


class Startup(c.Structure):
    _fields_ = [('cb', w.DWORD), ('reserved', w.LPWSTR), ('desktop', w.LPWSTR), ('title', w.LPWSTR),
                ('x', w.DWORD), ('y', w.DWORD), ('xs', w.DWORD), ('ys', w.DWORD),
                ('xc', w.DWORD), ('yc', w.DWORD), ('fill', w.DWORD), ('flags', w.DWORD),
                ('show', w.WORD), ('cbReserved', w.WORD), ('lpReserved', c.c_void_p),
                ('stdin', w.HANDLE), ('stdout', w.HANDLE), ('stderr', w.HANDLE)]


class StartupEx(c.Structure):
    _fields_ = [('startup', Startup), ('attributes', c.c_void_p)]


class ProcessInfo(c.Structure):
    _fields_ = [('process', w.HANDLE), ('thread', w.HANDLE), ('pid', w.DWORD), ('tid', w.DWORD)]


class BasicLimits(c.Structure):
    _fields_ = [('process_time', c.c_longlong), ('job_time', c.c_longlong), ('flags', w.DWORD),
                ('min_working', c.c_size_t), ('max_working', c.c_size_t), ('active', w.DWORD),
                ('affinity', c.c_size_t), ('priority', w.DWORD), ('scheduling', w.DWORD)]


class ExtendedLimits(c.Structure):
    _fields_ = [('basic', BasicLimits), ('io', c.c_ulonglong * 6), ('process_memory', c.c_size_t),
                ('job_memory', c.c_size_t), ('peak_process', c.c_size_t), ('peak_job', c.c_size_t)]


class IsolatedProcess:
    def __init__(self, executable, *, expected_sha256):
        if os.name != 'nt':
            raise SandboxError('Windows AppContainer required')
        import msvcrt
        executable = pathlib.Path(executable)
        if not executable.is_absolute() or executable.is_symlink():
            raise SandboxError('fixed absolute executable required')
        self.job = self.process = None
        self.stdin = self.stdout = None
        self.directory = None
        self.pid = None
        self.profile = None
        k = c.WinDLL('kernel32', use_last_error=True)
        a = c.WinDLL('advapi32', use_last_error=True)
        u = c.WinDLL('userenv', use_last_error=True)
        self.close_handle = _api(k, 'CloseHandle', w.BOOL, w.HANDLE)
        self.wait = _api(k, 'WaitForSingleObject', w.DWORD, w.HANDLE, w.DWORD)
        self.exit_code = _api(k, 'GetExitCodeProcess', w.BOOL, w.HANDLE, c.POINTER(w.DWORD))
        terminate = _api(k, 'TerminateProcess', w.BOOL, w.HANDLE, w.UINT)
        free_sid = _api(a, 'FreeSid', c.c_void_p, c.c_void_p)
        local_free = _api(k, 'LocalFree', c.c_void_p, c.c_void_p)
        create_profile = _api(u, 'CreateAppContainerProfile', c.c_long, w.LPCWSTR, w.LPCWSTR, w.LPCWSTR, c.c_void_p, w.DWORD, c.POINTER(c.c_void_p))
        self.delete_profile = _api(u, 'DeleteAppContainerProfile', c.c_long, w.LPCWSTR)
        sid_string = _api(a, 'ConvertSidToStringSidW', w.BOOL, c.c_void_p, c.POINTER(w.LPWSTR))
        init = _api(k, 'InitializeProcThreadAttributeList', w.BOOL, c.c_void_p, w.DWORD, w.DWORD, c.POINTER(c.c_size_t))
        update = _api(k, 'UpdateProcThreadAttribute', w.BOOL, c.c_void_p, w.DWORD, c.c_size_t, c.c_void_p, c.c_size_t, c.c_void_p, c.c_void_p)
        delete = _api(k, 'DeleteProcThreadAttributeList', None, c.c_void_p)
        create = _api(k, 'CreateProcessW', w.BOOL, w.LPCWSTR, w.LPWSTR, c.c_void_p, c.c_void_p, w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(StartupEx), c.POINTER(ProcessInfo))
        create_job = _api(k, 'CreateJobObjectW', w.HANDLE, c.c_void_p, w.LPCWSTR)
        set_job = _api(k, 'SetInformationJobObject', w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD)
        resume = _api(k, 'ResumeThread', w.DWORD, w.HANDLE)
        open_token = _api(a, 'OpenProcessToken', w.BOOL, w.HANDLE, w.DWORD, c.POINTER(w.HANDLE))
        token_info = _api(a, 'GetTokenInformation', w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
        sid, string, attributes, handles, pi = c.c_void_p(), w.LPWSTR(), None, [], ProcessInfo()
        try:
            # Copy, then verify: the executable actually launched must match the fixed lock.
            self.directory = pathlib.Path(tempfile.mkdtemp(prefix='opennovel-native-'))
            binary = self.directory / 'engine.exe'
            shutil.copyfile(executable, binary)
            if hashlib.sha256(binary.read_bytes()).hexdigest() != expected_sha256:
                raise SandboxError('native executable changed')
            profile = 'OpenNovel.ReadOnly.' + secrets.token_hex(16)
            if create_profile(profile, 'OpenNovel isolated engine', 'Temporary no-network engine', None, 0, c.byref(sid)) < 0:
                raise SandboxError('AppContainer profile unavailable')
            self.profile = profile
            _ok(sid_string(sid, c.byref(string)))
            icacls = pathlib.Path(os.environ['SystemRoot']) / 'System32/icacls.exe'
            for path in (self.directory, binary):
                subprocess.run([str(icacls), str(path), '/grant', '*' + string.value + ':RX'], check=True,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5,
                               creationflags=subprocess.CREATE_NO_WINDOW)
            parent_read, child_write = os.pipe()
            child_read, parent_write = os.pipe()
            handles = [parent_read, child_write, child_read, parent_write]
            os.set_inheritable(child_read, True)
            os.set_inheritable(child_write, True)
            inherited = (w.HANDLE * 2)(msvcrt.get_osfhandle(child_read), msvcrt.get_osfhandle(child_write))
            capabilities = SecurityCapabilities(sid, None, 0, 0)
            no_children = w.DWORD(1)
            self.job = _ok(create_job(None, None))
            limits = ExtendedLimits()
            limits.basic.flags = 0x2000 | 0x8 | 0x100  # kill on close, one process, memory
            limits.basic.active = 1
            limits.process_memory = 256 * 1024 * 1024
            _ok(set_job(self.job, 9, c.byref(limits), c.sizeof(limits)))
            jobs = (w.HANDLE * 1)(self.job)
            length = c.c_size_t()
            init(None, 4, 0, c.byref(length))
            attributes = c.create_string_buffer(length.value)
            _ok(init(attributes, 4, 0, c.byref(length)))
            # Atomic job membership closes the create-then-assign parent-crash gap.
            for key, value in ((0x20009, capabilities), (0x20002, inherited), (0x2000E, no_children), (0x2000D, jobs)):
                _ok(update(attributes, 0, key, c.byref(value), c.sizeof(value), None, None))
            startup = StartupEx()
            startup.startup.cb = c.sizeof(startup)
            startup.startup.flags = 0x100
            startup.startup.stdin, startup.startup.stdout, startup.startup.stderr = inherited[0], inherited[1], inherited[1]
            startup.attributes = c.cast(attributes, c.c_void_p)
            # Only necessary system environment. Never inherit proxy, account, Python or library paths.
            # LOCALAPPDATA is required by CreateProcess for AppContainer profile resolution;
            # Windows redirects the child's storage to that profile. No other user variables.
            env = c.create_unicode_buffer('LOCALAPPDATA=' + os.environ['LOCALAPPDATA'] + '\0SystemRoot=' + os.environ['SystemRoot'] + '\0\0')
            command = c.create_unicode_buffer('"' + str(binary) + '"')
            _ok(create(str(binary), command, None, None, True, 0x08000000 | 0x00080000 | 0x400 | 4,
                       env, str(self.directory), c.byref(startup), c.byref(pi)))
            self.process, self.pid = pi.process, pi.pid
            token = w.HANDLE()
            _ok(open_token(self.process, 8, c.byref(token)))
            try:
                is_container, size = w.DWORD(), w.DWORD()
                _ok(token_info(token, 29, c.byref(is_container), 4, c.byref(size)))
                if is_container.value != 1:
                    raise SandboxError('child token is not AppContainer')
                # TOKEN_GROUPS starts with the count; zero capability groups required.
                capability_data = c.create_string_buffer(4096)
                _ok(token_info(token, 30, capability_data, 4096, c.byref(size)))
                if w.DWORD.from_buffer(capability_data).value != 0:
                    raise SandboxError('unexpected capabilities')
            finally:
                self.close_handle(token)
            if resume(pi.thread) == 0xFFFFFFFF:
                raise SandboxError('cannot resume isolated process')
            self.stdin, self.stdout = os.fdopen(parent_write, 'wb', buffering=0), os.fdopen(parent_read, 'rb')
            handles.remove(parent_write)
            handles.remove(parent_read)
        except BaseException:
            if self.process:
                terminate(self.process, 3)
            self.close()
            raise
        finally:
            for fd in handles:
                os.close(fd)
            if pi.thread:
                self.close_handle(pi.thread)
            if attributes is not None:
                delete(attributes)
            if string:
                local_free(c.cast(string, c.c_void_p))
            if sid:
                free_sid(sid)

    def poll(self):
        if not self.process:
            return 3
        code = w.DWORD()
        _ok(self.exit_code(self.process, c.byref(code)))
        return None if code.value == 259 else code.value

    def close(self):
        # Closing the only non-inherited job handle also handles parent crashes.
        if self.job:
            self.close_handle(self.job)
            self.job = None
        if self.process:
            if self.wait(self.process, 5000) != 0:
                raise SandboxError('owned engine did not exit')
            self.close_handle(self.process)
            self.process = None
        for name in ('stdin', 'stdout'):
            stream = getattr(self, name)
            if stream is not None:
                stream.close()
                setattr(self, name, None)
        try:
            if self.directory is not None:
                # Windows image scanning can briefly retain a mapping after process exit.
                # Retry only deletion of the one owned file, never arbitrary directory trees.
                deadline = time.monotonic() + 3
                while True:
                    try:
                        (self.directory / 'engine.exe').unlink(missing_ok=True)
                        self.directory.rmdir()
                        self.directory = None
                        break
                    except PermissionError:
                        if time.monotonic() >= deadline:
                            raise
                        time.sleep(0.05)
        finally:
            if self.profile is not None:
                if self.delete_profile(self.profile) < 0:
                    raise SandboxError('owned AppContainer profile cleanup failed')
                self.profile = None
