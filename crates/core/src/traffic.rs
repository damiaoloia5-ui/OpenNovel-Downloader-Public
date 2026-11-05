//! Cooperative scheduling across processes and local profiles of one OS user.
//! This is not a platform authorization, machine-wide firewall or security boundary.
use crate::{Error, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn platform(value: &str) -> Result<&'static str> {
    match value {
        "fanqie" | "fanqienovel.com" | "www.fanqienovel.com" => Ok("fanqie"),
        "qidian" => Ok("qidian"),
        "shuqi" | "www.shuqi.com" | "shuqi.com" | "c13.shuqireader.com" => Ok("shuqi"),
        "qimao" | "www.qimao.com" | "qimao.com" => Ok("qimao"),
        "jinjiang" | "www.jjwxc.net" => Ok("jinjiang"),
        "zongheng" | "book.zongheng.com" | "read.zongheng.com" => Ok("zongheng"),
        "17k" | "www.17k.com" => Ok("17k"),
        "ciweimao" | "www.ciweimao.com" => Ok("ciweimao"),
        "faloo" | "b.faloo.com" => Ok("faloo"),
        "migu" | "n.cmread.com" => Ok("migu"),
        _ => Err(Error::Invalid("不支持的平台请求预算".into())),
    }
}

#[derive(Clone)]
pub struct TrafficGate {
    root: PathBuf,
    platform: &'static str,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    next_ms: u64,
    cooldown_ms: u64,
}
pub struct TrafficLease {
    _lock: File,
    path: PathBuf,
    state: Mutex<State>,
}
fn now_ms() -> Result<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Invalid("系统时间异常，暂停访问".into()))?
            .as_millis(),
    )
    .map_err(|_| Error::Invalid("系统时间超出范围".into()))
}
impl TrafficGate {
    pub fn for_user(platform: &str) -> Result<Self> {
        Self::at(
            crate::profiles::user_data_root()?.join("traffic-v1"),
            platform,
        )
    }
    pub fn at(root: impl AsRef<Path>, key: &str) -> Result<Self> {
        if !root.as_ref().is_absolute() {
            return Err(Error::Invalid("调度目录必须为绝对路径".into()));
        }
        Ok(Self {
            root: root.as_ref().to_owned(),
            platform: platform(key)?,
        })
    }
    pub fn acquire(&self) -> Result<TrafficLease> {
        std::fs::create_dir_all(&self.root)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join(format!("{}.lock", self.platform)))?;
        FileExt::try_lock_exclusive(&file).map_err(|_| {
            Error::Conflict(
                "该平台正被另一个程序或本地用户访问，请等待或暂停原任务；进度保留".into(),
            )
        })?;
        let path = self.root.join(format!("{}.json", self.platform));
        let state = match std::fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(e.into()),
            Ok(m) if m.len() <= 256 => serde_json::from_slice::<State>(&std::fs::read(&path)?)?,
            Ok(_) => return Err(Error::Invalid("平台调度记录异常，已停止访问".into())),
        };
        let lease = TrafficLease {
            _lock: file,
            path,
            state: Mutex::new(state),
        };
        lease.check()?;
        Ok(lease)
    }
}
impl TrafficLease {
    fn persist(&self, state: &State) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(self.path.parent().expect("fixed parent"))?;
        file.write_all(&serde_json::to_vec(state)?)?;
        file.as_file().sync_all()?;
        file.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
    fn cooling(state: &State, now: u64) -> Result<()> {
        if state.cooldown_ms > now {
            return Err(Error::Network(format!(
                "来源仍在限流或验证冷却中，请至少等待 {} 秒再继续；切换用户或重启不会取消等待",
                (state.cooldown_ms - now).div_ceil(1000)
            )));
        }
        Ok(())
    }
    pub fn check(&self) -> Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::Conflict("调度记录不可用".into()))?;
        Self::cooling(&state, now_ms()?)
    }
    /// Zero means a request slot was durably reserved; otherwise wait and retry.
    pub fn reserve(&self, interval: Duration) -> Result<Duration> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Conflict("调度记录不可用".into()))?;
        let now = now_ms()?;
        Self::cooling(&state, now)?;
        if state.next_ms > now {
            return Ok(Duration::from_millis((state.next_ms - now).min(50)));
        }
        state.next_ms = now
            + u64::try_from(interval.as_millis())
                .unwrap_or(60000)
                .clamp(250, 60000);
        self.persist(&state)?;
        Ok(Duration::ZERO)
    }
    pub fn wait(&self, interval: Duration, cancelled: &AtomicBool) -> Result<()> {
        loop {
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            let delay = self.reserve(interval)?;
            if delay.is_zero() {
                return Ok(());
            }
            std::thread::sleep(delay);
        }
    }
    pub fn defer(&self, seconds: u64) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::Conflict("调度记录不可用".into()))?;
        state.cooldown_ms = state
            .cooldown_ms
            .max(now_ms()? + seconds.clamp(1, 86400) * 1000);
        state.next_ms = state.next_ms.max(state.cooldown_ms);
        self.persist(&state)
    }
}
