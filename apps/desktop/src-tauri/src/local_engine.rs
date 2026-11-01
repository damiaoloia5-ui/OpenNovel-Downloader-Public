//! Optional producer bridge. Storage, content validation and TXT remain in core.
use opennovel_core::{
    Error, ImportChapter, Result,
    sources::{ChapterRef, LOCAL_ENGINE_SOURCE, SourceAdapter, parse_engine_chapter},
};
use serde::Serialize;
use serde_json::{Value, json};
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
}

#[derive(Serialize)]
pub struct EngineStatus {
    pub enabled: bool,
    pub ready: bool,
    pub message: String,
}

impl EngineControl {
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn require(&self) -> Result<Runtime> {
        if !self.enabled() {
            return Err(Error::Invalid(
                "本地引擎默认关闭，请在添加小说页主动启用".into(),
            ));
        }
        Runtime::discover()
    }

    pub fn status(&self) -> EngineStatus {
        let ready = Runtime::discover().and_then(|r| r.check()).is_ok();
        if !ready {
            self.enabled.store(false, Ordering::Release);
        }
        EngineStatus {
            enabled: self.enabled(), ready,
            message: if ready {
                "隔离组件可用；仅实验性读取番茄明确免费的章节，每次启动需主动启用。"
            } else {
                "组件未就绪：需要本地 Python、正在运行的 Docker 和已审查的固定引擎构建。普通网页保存不受影响；安装步骤见 docs/local-engine.md。"
            }.into(),
        }
    }

    pub fn set(&self, enabled: bool) -> Result<EngineStatus> {
        if enabled {
            Runtime::discover()?.check()?;
        }
        self.enabled.store(enabled, Ordering::Release);
        Ok(self.status())
    }
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
        let python = std::env::var_os("OPENNOVEL_ENGINE_PYTHON")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or_else(unavailable)?;
        Self::from_paths(
            python,
            std::env::var_os("OPENNOVEL_ENGINE_ROOT").map(PathBuf::from),
        )
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
        let mut command = self.command();
        command.arg("--check");
        let worker = Worker::spawn(command)?;
        if worker.receive(&AtomicBool::new(false), Duration::from_secs(35))?["ready"] != true {
            return Err(unavailable());
        }
        Ok(())
    }
}

const COMPONENT_SCRIPTS: &[(&str, &[u8])] = &[
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
    fn local_engine_is_disabled_at_every_application_start() {
        let control = EngineControl::default();
        assert!(!control.enabled());
        assert!(control.require().is_err());
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
        let mut runtime = Runtime::discover().unwrap();
        runtime.check().unwrap();
        runtime.fixture = true;
        let root = tempfile::tempdir().unwrap();
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
