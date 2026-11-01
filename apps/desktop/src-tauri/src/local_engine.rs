//! Optional producer bridge. Storage, content validation and TXT remain in core.
use opennovel_core::{
    Error, ImportChapter, Result,
    sources::{ChapterRef, LOCAL_ENGINE_SOURCE, SourceAdapter, parse_engine_chapter},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct EngineControl {
    enabled: AtomicBool,
    remembered: AtomicBool,
    preference: Option<PathBuf>,
    choice_revision: Mutex<u64>,
}

#[derive(Serialize)]
pub struct EngineStatus {
    pub enabled: bool,
    pub ready: bool,
    pub remembered: bool,
    pub installed: bool,
    pub message: String,
}

impl EngineControl {
    pub fn for_directory(directory: &std::path::Path) -> Self {
        let preference = directory.join("local-engine-choice.json");
        let remembered = std::fs::metadata(&preference)
            .ok()
            .filter(|m| m.len() <= 128)
            .and_then(|_| std::fs::read(&preference).ok())
            .and_then(|raw| serde_json::from_slice::<EngineChoice>(&raw).ok())
            .is_some_and(|choice| choice.schema_version == 1 && choice.enabled);
        Self {
            preference: Some(preference),
            remembered: AtomicBool::new(remembered),
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn remembered(&self) -> bool {
        self.remembered.load(Ordering::Acquire)
    }

    pub fn require_ready(&self) -> Result<Runtime> {
        let (ticket, prepare) = self.activation_ticket()?;
        let runtime = Runtime::discover()?;
        if prepare {
            runtime.prepare()?;
        }
        self.finish_activation(ticket)?;
        Ok(runtime)
    }

    fn activation_ticket(&self) -> Result<(u64, bool)> {
        let revision = self.choice_revision.lock().map_err(|_| unavailable())?;
        if !self.remembered() {
            return Err(unavailable());
        }
        Ok((*revision, !self.enabled()))
    }

    fn finish_activation(&self, ticket: u64) -> Result<()> {
        let revision = self.choice_revision.lock().map_err(|_| unavailable())?;
        if *revision != ticket || !self.remembered() {
            return Err(choice_changed());
        }
        self.enabled.store(true, Ordering::Release);
        Ok(())
    }

    pub fn status(&self) -> EngineStatus {
        let runtime = Runtime::discover();
        let installed = runtime.is_ok();
        let mut status = self.checked_status(runtime.and_then(|r| r.check()));
        status.installed = installed;
        status
    }

    fn checked_status(&self, result: Result<()>) -> EngineStatus {
        let _revision = self
            .choice_revision
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let ready = result.is_ok();
        let remembered = self.remembered.load(Ordering::Acquire);
        self.enabled.store(ready && remembered, Ordering::Release);
        EngineStatus {
            enabled: self.enabled(),
            ready,
            remembered,
            installed: false,
            message: if ready {
                if remembered {
                    "本地增强已启用，下次打开软件会自动恢复；粘贴番茄链接即可保存免费章节。".into()
                } else {
                    "本地增强已就绪，首次启用后记住选择，无需再配置运行环境。".into()
                }
            } else {
                format!(
                    "本地增强暂不可用：{}。普通网页保存仍可使用。",
                    result.unwrap_err()
                )
            },
        }
    }

    pub fn set(&self, enabled: bool) -> Result<EngineStatus> {
        if enabled {
            let ticket = self.reserve_choice()?;
            Runtime::discover()?.prepare()?;
            self.commit_choice(ticket, true)?;
        } else {
            self.save_choice(false)?;
        }
        Ok(self.status())
    }

    fn save_choice(&self, enabled: bool) -> Result<()> {
        let ticket = self.reserve_choice()?;
        self.commit_choice(ticket, enabled)
    }

    fn reserve_choice(&self) -> Result<u64> {
        let mut revision = self.choice_revision.lock().map_err(|_| unavailable())?;
        *revision = revision.wrapping_add(1);
        Ok(*revision)
    }

    fn commit_choice(&self, ticket: u64, enabled: bool) -> Result<()> {
        let revision = self.choice_revision.lock().map_err(|_| unavailable())?;
        if *revision != ticket {
            return Err(choice_changed());
        }
        if let Some(path) = &self.preference {
            let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(unavailable)?)?;
            serde_json::to_writer(
                &mut file,
                &EngineChoice {
                    schema_version: 1,
                    enabled,
                },
            )?;
            file.as_file().sync_all()?;
            file.persist(path).map_err(|e| Error::Io(e.error))?;
        }
        self.remembered.store(enabled, Ordering::Release);
        self.enabled.store(enabled, Ordering::Release);
        Ok(())
    }
}

fn choice_changed() -> Error {
    Error::Invalid("本地增强选择已变化，已取消旧的准备结果；请按当前状态操作".into())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineChoice {
    schema_version: u8,
    enabled: bool,
}

#[derive(Clone)]
pub struct Runtime {
    python: PathBuf,
    worker: PathBuf,
    component: Option<PathBuf>,
    #[cfg(test)]
    fixture: bool,
}

impl Runtime {
    fn discover() -> Result<Self> {
        let component = std::env::var_os("OPENNOVEL_ENGINE_ROOT")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe()
                    .ok()
                    .and_then(|exe| Self::adjacent_component(exe.parent()?))
            });
        if let Some(root) = &component
            && root.join("python").exists()
        {
            let python = verified_bundled_python(root)?;
            verify_offline_files(root)?;
            return Self::from_paths(python, component);
        }
        let python = std::env::var_os("OPENNOVEL_ENGINE_PYTHON")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or_else(unavailable)?;
        Self::from_paths(python, component)
    }

    fn adjacent_component(directory: &std::path::Path) -> Option<PathBuf> {
        let bundled = directory.join("opennovel-engine");
        if bundled.is_dir() {
            Some(bundled)
        } else if directory.join("runtime").is_dir() {
            Some(directory.to_owned())
        } else {
            None
        }
    }

    fn from_paths(python: PathBuf, component: Option<PathBuf>) -> Result<Self> {
        if !python.is_absolute() || !python.is_file() {
            return Err(unavailable());
        }
        // Never resolve executable scripts from cwd, frontend input or PATH.
        let scripts = if let Some(root) = &component {
            if !root.is_absolute() {
                return Err(unavailable());
            }
            let scripts = root.join("runtime");
            let entries = std::fs::read_dir(&scripts)?.collect::<std::io::Result<Vec<_>>>()?;
            if entries.len() != COMPONENT_SCRIPTS.len()
                || entries
                    .iter()
                    .any(|e| !e.file_type().is_ok_and(|t| t.is_file()))
            {
                return Err(unavailable());
            }
            // The manifest describes a prepared copy; it cannot authorize code.
            // Compare against the scripts embedded in this application build.
            for (name, expected) in COMPONENT_SCRIPTS {
                if std::fs::read(scripts.join(name))? != *expected {
                    return Err(unavailable());
                }
            }
            scripts
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/research")
        };
        let worker = scripts.join("desktop_engine_worker.py");
        if !worker.is_file() {
            return Err(unavailable());
        }
        Ok(Self {
            python,
            worker,
            component,
            #[cfg(test)]
            fixture: false,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.python);
        command
            .args(["-B", "-u"])
            .arg(&self.worker)
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(root) = &self.component {
            command.env("OPENNOVEL_ENGINE_ROOT", root);
        } else {
            command.env_remove("OPENNOVEL_ENGINE_ROOT");
        }
        #[cfg(test)]
        if self.fixture {
            command.arg("--fixture");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }

    fn check(&self) -> Result<()> {
        self.check_mode(false)
    }

    fn prepare(&self) -> Result<()> {
        self.check_mode(true)
    }

    fn check_mode(&self, prepare: bool) -> Result<()> {
        let mut command = self.command();
        command.arg(if prepare { "--prepare" } else { "--check" });
        let worker = Worker::spawn(command)?;
        let result = worker.receive(
            &AtomicBool::new(false),
            Duration::from_secs(if prepare { 240 } else { 35 }),
        )?;
        if result["ready"] != true {
            return Err(Error::Invalid(
                match result["reason"].as_str() {
                    Some("docker_unavailable") => {
                        "请先启动 Docker Desktop，再点击重新检查；需要 Linux 隔离后端"
                    }
                    Some("images_missing") => {
                        "本地隔离镜像缺失，请按组件安装说明准备；软件不会自动下载"
                    }
                    Some("engine_missing") => "本地隔离引擎构建缺失，请按组件安装说明准备",
                    Some("source_changed") => "组件源码校验失败，请重新安装匹配的完整组件",
                    Some("offline_bundle_pending") => {
                        "首次使用需要准备内置引擎，请点击启用；无需手工下载镜像或构建"
                    }
                    Some("offline_bundle_invalid") => {
                        "内置离线组件校验失败，请重新安装完整一体化版"
                    }
                    Some("offline_import_failed") => {
                        "内置引擎准备失败，请检查 Docker 和可用空间后重试；已存正文保留"
                    }
                    _ => "本地组件检查失败，请重新检查或安装匹配组件",
                }
                .into(),
            ));
        }
        Ok(())
    }
}

fn verified_bundled_python(root: &std::path::Path) -> Result<PathBuf> {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../../third_party/python-embed-lock.json"
    ))?;
    verify_files(
        &root.join("python"),
        manifest["files"].as_object().ok_or_else(unavailable)?,
    )?;
    Ok(root.join("python/python.exe"))
}

fn verify_offline_files(root: &std::path::Path) -> Result<()> {
    verify_offline_files_with_lock(
        root,
        include_bytes!("../../../../scripts/research/engine_bundle_lock.json"),
    )
    .map_err(|_| Error::Invalid("内置离线组件不完整或版本不匹配，请重新安装完整一体化版".into()))
}

fn verify_offline_files_with_lock(root: &std::path::Path, raw: &[u8]) -> Result<()> {
    let directory = root.join("offline-engine");
    if std::fs::read(directory.join("manifest.json"))? != raw {
        return Err(unavailable());
    }
    let lock: Value = serde_json::from_slice(raw)?;
    let files = lock["files"].as_object().ok_or_else(unavailable)?;
    if files.len() != 2
        || !files.contains_key("runtime-image.tar.gz")
        || !files.contains_key("corresponding-source.tar.gz")
    {
        return Err(unavailable());
    }
    let entries = std::fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() != 3
        || entries
            .iter()
            .any(|e| !e.file_type().is_ok_and(|t| t.is_file()))
    {
        return Err(unavailable());
    }
    for (name, expected) in files {
        let path = directory.join(name);
        let bytes = expected["bytes"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 300 * 1024 * 1024)
            .ok_or_else(unavailable)?;
        if std::fs::metadata(&path)?.len() != bytes {
            return Err(unavailable());
        }
        let mut file = std::fs::File::open(path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        if expected["sha256"].as_str() != Some(format!("{:x}", digest.finalize()).as_str()) {
            return Err(unavailable());
        }
    }
    Ok(())
}

fn verify_files(
    directory: &std::path::Path,
    expected: &serde_json::Map<String, Value>,
) -> Result<()> {
    let entries = std::fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() != expected.len()
        || entries
            .iter()
            .any(|e| !e.file_type().is_ok_and(|t| t.is_file()))
    {
        return Err(unavailable());
    }
    for (name, digest) in expected {
        if name.contains(['/', '\\', ':']) || name == "." || name == ".." {
            return Err(unavailable());
        }
        let actual = format!("{:x}", Sha256::digest(std::fs::read(directory.join(name))?));
        if digest.as_str() != Some(actual.as_str()) {
            return Err(unavailable());
        }
    }
    Ok(())
}

const COMPONENT_SCRIPTS: &[(&str, &[u8])] = &[
    (
        "offline_engine_bundle.py",
        include_bytes!("../../../../scripts/research/offline_engine_bundle.py"),
    ),
    (
        "engine_bundle_lock.json",
        include_bytes!("../../../../scripts/research/engine_bundle_lock.json"),
    ),
    (
        "desktop_engine_worker.py",
        include_bytes!("../../../../scripts/research/desktop_engine_worker.py"),
    ),
    (
        "engine_runtime_config.py",
        include_bytes!("../../../../scripts/research/engine_runtime_config.py"),
    ),
    (
        "persistent_engine.py",
        include_bytes!("../../../../scripts/research/persistent_engine.py"),
    ),
    (
        "persistent_read_gate.py",
        include_bytes!("../../../../scripts/research/persistent_read_gate.py"),
    ),
    (
        "relay_read_gate.py",
        include_bytes!("../../../../scripts/research/relay_read_gate.py"),
    ),
    (
        "https_read_gate.py",
        include_bytes!("../../../../scripts/research/https_read_gate.py"),
    ),
    (
        "local_engine_gate.py",
        include_bytes!("../../../../scripts/research/local_engine_gate.py"),
    ),
];

fn unavailable() -> Error {
    Error::Invalid("本地引擎未就绪，请检查组件状态与安装指引".into())
}
fn stopped() -> Error {
    Error::Network("本地引擎已停止；已保存内容保留。处理网络或官网提示后继续原任务".into())
}

struct Worker {
    child: Child,
    responses: mpsc::Receiver<Value>,
    sequence: u64,
}

impl Worker {
    fn spawn(mut command: Command) -> Result<Self> {
        let mut child = command.spawn().map_err(|_| unavailable())?;
        let output = child.stdout.take().ok_or_else(stopped)?;
        let (tx, responses) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = Vec::new();
                let read = Read::by_ref(&mut reader)
                    .take(2 * 1024 * 1024 + 1)
                    .read_until(b'\n', &mut line);
                if read.is_err()
                    || line.is_empty()
                    || line.len() > 2 * 1024 * 1024
                    || line.last() != Some(&b'\n')
                {
                    break;
                }
                let Ok(value) = serde_json::from_slice(&line) else {
                    break;
                };
                if tx.try_send(value).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            responses,
            sequence: 0,
        })
    }

    fn send(&mut self, value: Value) -> Result<()> {
        let input = self.child.stdin.as_mut().ok_or_else(stopped)?;
        let mut raw = serde_json::to_vec(&value)?;
        if raw.len() > 2 * 1024 * 1024 {
            return Err(stopped());
        }
        raw.push(b'\n');
        input
            .write_all(&raw)
            .and_then(|()| input.flush())
            .map_err(|_| stopped())
    }

    fn receive(&self, cancelled: &AtomicBool, timeout: Duration) -> Result<Value> {
        let until = Instant::now() + timeout;
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            if Instant::now() >= until {
                return Err(stopped());
            }
            match self.responses.recv_timeout(Duration::from_millis(50)) {
                Ok(value) => return Ok(value),
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(_) => return Err(stopped()),
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send(json!({"op":"cancel"}));
        self.child.stdin.take();
        let until = Instant::now() + Duration::from_secs(35);
        while Instant::now() < until {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct EngineAdapter {
    runtime: Runtime,
    book: String,
    items: Vec<String>,
    worker: Mutex<Option<Worker>>,
}

impl EngineAdapter {
    pub fn new(runtime: Runtime, book: String, items: Vec<String>) -> Self {
        Self {
            runtime,
            book,
            items,
            worker: Mutex::new(None),
        }
    }
}

impl SourceAdapter for EngineAdapter {
    fn source_id(&self) -> &str {
        LOCAL_ENGINE_SOURCE
    }

    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        cancelled: &AtomicBool,
    ) -> Result<ImportChapter> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if book != self.book || !self.items.contains(&chapter.id) {
            return Err(stopped());
        }
        let mut slot = self.worker.lock().map_err(|_| stopped())?;
        if slot.is_none() {
            let mut worker = Worker::spawn(self.runtime.command())?;
            worker.send(json!({"op":"configure","book_id":self.book,"items":self.items}))?;
            if worker.receive(cancelled, Duration::from_secs(65))?["op"] != "ready" {
                return Err(stopped());
            }
            *slot = Some(worker);
        }
        let worker = slot.as_mut().ok_or_else(stopped)?;
        worker.sequence += 1;
        worker.send(json!({"op":"read","sequence":worker.sequence,"item_id":chapter.id}))?;
        let value = worker.receive(cancelled, Duration::from_secs(65))?;
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if value["op"] == "error" {
            return Err(match value["kind"].as_str() {
                Some("access_challenge") => Error::AccessChallenge(
                    "来源要求验证或拒绝访问，已停止；请核对官网提示后再继续".into(),
                ),
                Some("rate_limited") => {
                    let seconds = value["retry_after_seconds"]
                        .as_u64()
                        .unwrap_or(60)
                        .clamp(1, 86400);
                    Error::Network(format!(
                        "来源限流，请至少等待 {seconds} 秒后继续原任务；进度已保留"
                    ))
                }
                Some("permission_changed") => {
                    Error::AccessChallenge("本章阅读权限变化，未保存；请先核对官网免费状态".into())
                }
                _ => match value["outcome"].as_str() {
                    Some("empty_response") => Error::Network(
                        "官方接口连续返回空内容，已停止；进度保留，可稍后继续原任务".into(),
                    ),
                    Some("transport_failed") => Error::Network(
                        "官方接口连接或传输暂时失败，有限重试后已停止；请稍后继续".into(),
                    ),
                    Some("oversized" | "redirect_refused") => Error::SourceChanged(
                        "官方响应大小或地址不符合约定，已停止；未保存异常正文".into(),
                    ),
                    Some("http_error") => Error::Network(format!(
                        "官方服务返回 HTTP {}，已停止；进度保留",
                        value["http_status"].as_u64().unwrap_or(502)
                    )),
                    _ => stopped(),
                },
            });
        }
        if value["op"] != "chapter" || value["sequence"].as_u64() != Some(worker.sequence) {
            return Err(stopped());
        }
        parse_engine_chapter(
            &serde_json::to_string(&value["envelope"])?,
            book,
            &chapter.id,
            &chapter.title,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_engine_is_disabled_without_a_saved_choice() {
        let control = EngineControl::default();
        assert!(!control.enabled());
        assert!(control.require_ready().is_err());
    }

    #[test]
    fn explicit_choice_survives_restart_and_missing_runtime_without_losing_consent() {
        let root = tempfile::tempdir().unwrap();
        let first = EngineControl::for_directory(root.path());
        assert!(!first.checked_status(Ok(())).enabled);
        first.save_choice(true).unwrap();
        let restarted = EngineControl::for_directory(root.path());
        assert!(!restarted.enabled());
        let missing = restarted.checked_status(Err(unavailable()));
        assert!(!missing.enabled && missing.remembered);
        assert!(restarted.checked_status(Ok(())).enabled);
        restarted.save_choice(false).unwrap();
        assert!(
            !EngineControl::for_directory(root.path())
                .checked_status(Ok(()))
                .enabled
        );
    }

    #[test]
    fn malformed_choices_and_failed_writes_do_not_enable_the_component() {
        let root = tempfile::tempdir().unwrap();
        let choice = root.path().join("local-engine-choice.json");
        for raw in [
            "{\"schema_version\":2,\"enabled\":true}",
            "{\"schema_version\":1,\"enabled\":true,\"extra\":1}",
            "invalid",
        ] {
            std::fs::write(&choice, raw).unwrap();
            assert!(
                !EngineControl::for_directory(root.path())
                    .checked_status(Ok(()))
                    .enabled
            );
        }
        let absent = EngineControl::for_directory(&root.path().join("missing-parent"));
        assert!(absent.save_choice(true).is_err());
        assert!(!absent.checked_status(Ok(())).enabled);
    }

    #[test]
    fn disabling_or_newer_choices_invalidate_late_preparation_results() {
        let root = tempfile::tempdir().unwrap();
        let control = EngineControl::for_directory(root.path());
        control.save_choice(true).unwrap();
        let (ticket, _) = control.activation_ticket().unwrap();
        control.save_choice(false).unwrap();
        assert!(control.finish_activation(ticket).is_err());
        assert!(!control.remembered() && !control.enabled());
        let old_enable = control.reserve_choice().unwrap();
        control.save_choice(false).unwrap();
        assert!(control.commit_choice(old_enable, true).is_err());
        assert!(!EngineControl::for_directory(root.path()).remembered());
    }

    #[test]
    fn bundled_resource_discovery_uses_only_the_application_directory() {
        let root = tempfile::tempdir().unwrap();
        assert!(Runtime::adjacent_component(root.path()).is_none());
        let bundled = root.path().join("opennovel-engine");
        std::fs::create_dir(&bundled).unwrap();
        assert_eq!(
            Runtime::adjacent_component(root.path()),
            Some(bundled.clone())
        );
        assert!(verified_bundled_python(&bundled).is_err());
    }

    #[test]
    fn runtime_integrity_rejects_changed_added_and_nested_python_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("python.exe"), b"original fixture").unwrap();
        let expected = json!({"python.exe":format!("{:x}",Sha256::digest(b"original fixture"))});
        let expected = expected.as_object().unwrap();
        assert!(verify_files(root.path(), expected).is_ok());
        std::fs::write(root.path().join("python.exe"), b"altered").unwrap();
        assert!(verify_files(root.path(), expected).is_err());
        std::fs::write(root.path().join("python.exe"), b"original fixture").unwrap();
        std::fs::create_dir(root.path().join("site-packages")).unwrap();
        assert!(verify_files(root.path(), expected).is_err());
    }

    #[test]
    fn offline_archives_and_manifest_cannot_authorize_altered_or_partial_files() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("offline-engine");
        std::fs::create_dir(&directory).unwrap();
        let mut files = serde_json::Map::new();
        for name in ["runtime-image.tar.gz", "corresponding-source.tar.gz"] {
            let content = format!("original fixture {name}");
            std::fs::write(directory.join(name), &content).unwrap();
            files.insert(name.into(), json!({"bytes":content.len(),"sha256":format!("{:x}",Sha256::digest(content.as_bytes()))}));
        }
        let raw = serde_json::to_vec(&json!({"files":files})).unwrap();
        std::fs::write(directory.join("manifest.json"), &raw).unwrap();
        assert!(verify_offline_files_with_lock(root.path(), &raw).is_ok());
        std::fs::write(directory.join("runtime-image.tar.gz"), "altered").unwrap();
        assert!(verify_offline_files_with_lock(root.path(), &raw).is_err());
        std::fs::write(directory.join("manifest.json"), b"{}").unwrap();
        assert!(verify_offline_files_with_lock(root.path(), &raw).is_err());
    }

    #[test]
    fn prepared_component_requires_this_builds_exact_runtime_inventory() {
        let root = tempfile::tempdir().unwrap();
        let scripts = root.path().join("runtime");
        std::fs::create_dir(&scripts).unwrap();
        let python = root.path().join("python.exe");
        std::fs::write(&python, b"never executed by this discovery test").unwrap();
        for (name, content) in COMPONENT_SCRIPTS {
            std::fs::write(scripts.join(name), content).unwrap();
        }
        assert!(Runtime::from_paths(python.clone(), Some(root.path().to_owned())).is_ok());
        let first = scripts.join(COMPONENT_SCRIPTS[0].0);
        std::fs::write(&first, b"altered script").unwrap();
        assert!(Runtime::from_paths(python.clone(), Some(root.path().to_owned())).is_err());
        std::fs::write(&first, COMPONENT_SCRIPTS[0].1).unwrap();
        std::fs::write(scripts.join("json.py"), b"unexpected import shadow").unwrap();
        assert!(Runtime::from_paths(python.clone(), Some(root.path().to_owned())).is_err());
        assert!(Runtime::from_paths(python, Some(PathBuf::from("relative"))).is_err());
    }

    #[test]
    #[ignore = "Manual real-source acceptance only; explicit public book URL and new output directory required"]
    fn manual_free_book_acceptance() {
        let input =
            std::env::var("OPENNOVEL_ACCEPT_BOOK").expect("explicit public book URL required");
        let directory = PathBuf::from(
            std::env::var_os("OPENNOVEL_ACCEPT_OUTPUT").expect("new acceptance directory required"),
        );
        let resume = std::env::var("OPENNOVEL_ACCEPT_RESUME").as_deref() == Ok("1");
        assert!(
            directory.is_absolute()
                && if resume {
                    directory.is_dir()
                } else {
                    !directory.exists()
                },
            "use a new absolute acceptance directory, or explicitly resume its original job"
        );
        let runtime = Runtime::discover().unwrap();
        runtime.check().unwrap();
        let catalog = opennovel_core::sources::Fanqie::new()
            .unwrap()
            .inspect_engine(&input)
            .unwrap();
        assert!(
            catalog.chapters.len() <= 200,
            "manual acceptance request budget exceeded"
        );
        assert!(
            catalog.chapters.iter().all(|c| !c.restricted),
            "this acceptance case requires an explicitly free catalog"
        );
        std::fs::create_dir_all(&directory).unwrap();
        let library = std::sync::Arc::new(
            opennovel_core::Library::open(directory.join("library.db")).unwrap(),
        );
        let job = if resume {
            let jobs = library.snapshot().unwrap().jobs;
            assert_eq!(
                jobs.len(),
                1,
                "acceptance resume requires exactly one original job"
            );
            let previous = &jobs[0];
            let book = library.book_details(&previous.book_id).unwrap().book;
            assert_eq!(book.source, catalog.source);
            assert_eq!(book.remote_id, catalog.remote_id);
            assert_eq!(
                library
                    .job_details(&previous.id)
                    .unwrap()
                    .items
                    .iter()
                    .map(|i| i.chapter_id.as_str())
                    .collect::<Vec<_>>(),
                catalog
                    .chapters
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>(),
                "catalog changed; preserve the original acceptance instead of silently changing scope"
            );
            previous.id.clone()
        } else {
            library
                .create_engine_download(&catalog, &(1..=catalog.chapters.len()).collect::<Vec<_>>())
                .unwrap()
        };
        let saved_before = library.job(&job).unwrap().saved;
        let automatic = crate::auto_txt::AutoTxtService::start(
            std::sync::Arc::clone(&library),
            directory.join("exports"),
        )
        .unwrap();
        let client = EngineAdapter::new(
            runtime,
            catalog.remote_id.clone(),
            catalog.chapters.iter().map(|c| c.id.clone()).collect(),
        );
        let started = Instant::now();
        let result = library.run_job(&job, &client, &AtomicBool::new(false));
        drop(client);
        let snapshot = library.job(&job).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let receipt = loop {
            if let Some(status) = automatic.snapshot().unwrap().into_iter().find(|s| {
                s.book_id == snapshot.book_id
                    && (s.state == "failed"
                        || (s.state == "empty" && snapshot.saved == 0)
                        || (s.state == "ready"
                            && s.receipt.as_ref().is_some_and(|r| {
                                r.saved_chapters == snapshot.saved
                                    && r.total_chapters == snapshot.total
                            })))
            }) {
                break status;
            }
            assert!(Instant::now() < deadline, "automatic TXT deadline exceeded");
            std::thread::sleep(Duration::from_millis(50));
        };
        let details = library.job_details(&job).unwrap();
        let failures: Vec<_> = details.items.iter().filter(|i| i.state != "completed").map(|i| json!({"position":i.position,"chapter_id":i.chapter_id,"state":i.state,"error_kind":i.error_kind,"error":i.error})).collect();
        let report = json!({"job_id":job,"saved_before":saved_before,"book_id":catalog.remote_id,"title":catalog.title,"source":catalog.source,
            "total":snapshot.total,"saved":snapshot.saved,"blocked":snapshot.blocked,"failed":snapshot.failed,
            "state":snapshot.state,"elapsed_seconds":started.elapsed().as_secs_f64(),"automatic_txt":receipt,
            "run_error":result.err().map(|e|e.to_string()),"missing":failures,"real_source_requested":true,
            "account_used":false,"local_text_imported":false});
        std::fs::write(
            directory.join(if resume {
                format!("acceptance-resume-{}.json", uuid::Uuid::new_v4())
            } else {
                "acceptance.json".into()
            }),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!(
            "{}",
            json!({"saved":snapshot.saved,"total":snapshot.total,"state":snapshot.state})
        );
        drop(automatic);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            snapshot.state, "completed",
            "see acceptance.json; saved content retained"
        );
        assert_eq!(snapshot.saved, snapshot.total);
        assert_eq!(report["automatic_txt"]["state"], "ready");
        assert_eq!(report["automatic_txt"]["receipt"]["partial"], false);
    }

    #[test]
    #[ignore = "Explicit native Python/Docker fixture acceptance; no real platform requests"]
    fn native_worker_reuses_session_and_commits_validated_txt() {
        let root = tempfile::tempdir().unwrap();
        let control = EngineControl::for_directory(root.path());
        control.save_choice(true).unwrap();
        let control = EngineControl::for_directory(root.path());
        let mut runtime = control.require_ready().unwrap();
        assert!(control.enabled());
        runtime.fixture = true;
        let library = std::sync::Arc::new(
            opennovel_core::Library::open(root.path().join("library.db")).unwrap(),
        );
        let automatic = crate::auto_txt::AutoTxtService::start(
            std::sync::Arc::clone(&library),
            root.path().join("exports"),
        )
        .unwrap();
        let catalog = opennovel_core::sources::Catalog {
            source: LOCAL_ENGINE_SOURCE.into(),
            remote_id: "10000000001".into(),
            title: "原创桌面桥接验收".into(),
            author: "".into(),
            description: "".into(),
            chapters: ["10000000002", "10000000003"]
                .into_iter()
                .map(|id| ChapterRef {
                    id: id.into(),
                    title: "原创章".into(),
                    restricted: false,
                })
                .collect(),
        };
        let client = EngineAdapter::new(
            runtime,
            catalog.remote_id.clone(),
            catalog.chapters.iter().map(|c| c.id.clone()).collect(),
        );
        let job = library.create_engine_download(&catalog, &[1, 2]).unwrap();
        let result = library
            .run_job(&job, &client, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(result.state, "completed");
        assert_eq!(result.saved, 2);
        assert_eq!(client.worker.lock().unwrap().as_ref().unwrap().sequence, 2);
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(12);
        let receipt = loop {
            if let Some(status) = automatic
                .snapshot()
                .unwrap()
                .into_iter()
                .find(|s| s.book_id == result.book_id && s.state == "ready")
            {
                assert!(status.export_id.is_some());
                break status.receipt.unwrap();
            }
            assert!(Instant::now() < deadline, "automatic TXT deadline exceeded");
            std::thread::sleep(Duration::from_millis(30));
        };
        let text = std::fs::read_to_string(receipt.path).unwrap();
        assert_eq!(text.matches("山间的小屋亮起一盏灯。").count(), 2);
        assert!(!receipt.partial);
        drop(automatic);
        std::thread::sleep(Duration::from_millis(100));
    }
}
