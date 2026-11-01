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
    #[cfg(test)]
    fixture: bool,
}

impl Runtime {
    fn discover() -> Result<Self> {
        let python = std::env::var_os("OPENNOVEL_ENGINE_PYTHON")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && p.is_file())
            .ok_or_else(unavailable)?;
        // Development component only: never resolve a helper from the working
        // directory, a download, frontend input or the user's PATH.
        let worker = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/research/desktop_engine_worker.py");
        if !worker.is_file() {
            return Err(unavailable());
        }
        Ok(Self {
            python,
            worker,
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
                    Error::Network("来源限流，已停止；请等待后继续原任务".into())
                }
                Some("permission_changed") => {
                    Error::AccessChallenge("本章阅读权限变化，未保存；请先核对官网免费状态".into())
                }
                _ => stopped(),
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
    #[ignore = "Explicit native Python/Docker fixture acceptance; no real platform requests"]
    fn native_worker_reuses_session_and_commits_validated_txt() {
        let mut runtime = Runtime::discover().unwrap();
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
