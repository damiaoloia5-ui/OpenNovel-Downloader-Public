"""Execute the shipped launcher with an original GUI-subsystem fixture, offline."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'scripts'))
from verify_installer import installation_check_command
FIXTURE = r'''
#![windows_subsystem = "windows"]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--check-installation" { std::process::exit(19); }
    if std::env::var("ORIGINAL_CHECK_FAIL").is_ok() { std::process::exit(7); }
    let path = std::path::Path::new(&args[2]);
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join("original-fixture.txt"), b"Original launcher fixture only").unwrap();
}
'''


@unittest.skipUnless(sys.platform == 'win32', 'Windows batch launcher')
class InstallationLauncherTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = tempfile.TemporaryDirectory(prefix='opennovel-launcher-')
        cls.addClassCleanup(cls.scratch.cleanup)
        cls.root = Path(cls.scratch.name)
        source = cls.root / 'original.rs'
        source.write_text(FIXTURE, encoding='utf-8')
        cls.fixture = cls.root / 'original.exe'
        compiler = shutil.which('rustc')
        if not compiler:
            raise RuntimeError('rustc is required for the original Windows launcher fixture')
        subprocess.run([compiler, str(source), '-o', str(cls.fixture)], check=True,
                       capture_output=True, timeout=120)

    def launch(self, name, exe_name='opennovel-desktop.exe', fail=False, custom=True):
        home = self.root / name
        home.mkdir()
        script = home / '运行安装自检.cmd'
        # Match the CRLF checkout required by .gitattributes on every host.
        script.write_bytes((REPO / 'scripts/windows-installation-check.cmd').read_text(
            encoding='utf-8').replace('\n', '\r\n').encode('utf-8'))
        if exe_name:
            shutil.copy2(self.fixture, home / exe_name)
        output = home / '报告 & 原创 (1)! %sample%'
        env = os.environ.copy()
        env['PATH'] = ''  # No Python, Node, Rust, Docker or shell helpers at runtime.
        env['LOCALAPPDATA'] = str(home / 'isolated-profile')
        if fail:
            env['ORIGINAL_CHECK_FAIL'] = '1'
        command = installation_check_command(script, output if custom else '')
        result = subprocess.run(command,
                                env=env, capture_output=True, timeout=30)
        return result, output if custom else Path(env['LOCALAPPDATA']) / 'OpenNovel-Installation-Checks'

    def test_installed_name_and_special_paths_without_developer_tools(self):
        result, output = self.launch('安装 & 测试 (1)!')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((output / 'original-fixture.txt').read_bytes(), b'Original launcher fixture only')

    def test_portable_name_and_default_output(self):
        result, output = self.launch('portable', exe_name='OpenNovel.exe', custom=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((output / 'original-fixture.txt').is_file())

    def test_missing_executable_does_not_claim_success(self):
        result, output = self.launch('missing', exe_name=None)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertFalse(output.exists())

    def test_gui_process_failure_is_propagated(self):
        result, output = self.launch('failure', fail=True)
        self.assertEqual(result.returncode, 7, result.stdout + result.stderr)
        self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
