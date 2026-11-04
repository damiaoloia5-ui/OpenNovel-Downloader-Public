//! Reap only our marked, unlocked temporary browser directories. Never infer
//! ownership from a prefix alone, and never follow reparse points during cleanup.
use crate::{Error, Result};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const OWNER: &[u8] = b"OPENNOVEL_SESSION_V1\n";

pub struct SessionDirectory {
    root: PathBuf,
    path: PathBuf,
    lease: Option<File>,
}
#[derive(Default, serde::Serialize)]
pub struct CleanupReport {
    pub removed: usize,
    pub active: usize,
    pub retained: usize,
}

fn ordinary(path: &Path, directory: bool) -> Result<bool> {
    let m = std::fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Ok(false);
        }
    }
    Ok(!m.file_type().is_symlink() && if directory { m.is_dir() } else { m.is_file() })
}
fn root_lock(root: &Path) -> Result<File> {
    if !root.is_absolute() {
        return Err(Error::Invalid("会话目录必须为绝对路径".into()));
    }
    std::fs::create_dir_all(root)?;
    if !ordinary(root, true)? {
        return Err(Error::Invalid("会话目录不能是链接".into()));
    }
    let p = root.join(".cleanup.lock");
    if p.symlink_metadata().is_ok() && !ordinary(&p, false)? {
        return Err(Error::Invalid("会话锁不能是链接".into()));
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(p)?;
    FileExt::try_lock_exclusive(&file)
        .map_err(|_| Error::Conflict("会话目录正在维护，请稍后重试".into()))?;
    Ok(file)
}
fn safe_tree(path: &Path, remaining: &mut usize, depth: usize) -> Result<bool> {
    if depth > 64 || !ordinary(path, true)? {
        return Ok(false);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if *remaining == 0 {
            return Ok(false);
        }
        *remaining -= 1;
        let p = entry.path();
        if ordinary(&p, true)? {
            if !safe_tree(&p, remaining, depth + 1)? {
                return Ok(false);
            }
        } else if !ordinary(&p, false)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn remove_owned(root: &Path, path: &Path, remaining: &mut usize) -> Result<bool> {
    let resolved = path.canonicalize()?;
    if resolved.parent() != Some(root.canonicalize()?.as_path()) || !safe_tree(path, remaining, 0)?
    {
        return Ok(false);
    }
    std::fs::remove_dir_all(&resolved)?;
    Ok(true)
}
pub fn cleanup(root: &Path) -> Result<CleanupReport> {
    let _guard = root_lock(root)?;
    let mut report = CleanupReport::default();
    let mut remaining = 10000;
    for entry in std::fs::read_dir(root)?.take(2000) {
        if remaining == 0 || report.removed >= 8 {
            break;
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_prefix("session-v1-") else {
            continue;
        };
        if uuid::Uuid::parse_str(id).is_err() {
            continue;
        }
        let path = entry.path();
        if !ordinary(&path, true)? {
            report.retained += 1;
            continue;
        }
        let owner = root.join(format!("{name}.lock"));
        let attempt = (|| -> Result<Option<bool>> {
            if !ordinary(&owner, false)? {
                return Ok(Some(false));
            }
            let mut file = OpenOptions::new().read(true).write(true).open(&owner)?;
            if FileExt::try_lock_exclusive(&file).is_err() {
                return Ok(None);
            }
            let mut marker = Vec::new();
            (&mut file).take(64).read_to_end(&mut marker)?;
            if marker != OWNER {
                return Ok(Some(false));
            }
            let removed = remove_owned(root, &path, &mut remaining)?;
            drop(file);
            if removed {
                std::fs::remove_file(&owner)?;
            }
            Ok(Some(removed))
        })();
        match attempt {
            Ok(None) => report.active += 1,
            Ok(Some(true)) => report.removed += 1,
            _ => report.retained += 1,
        }
    }
    Ok(report)
}
impl SessionDirectory {
    pub fn new(root: &Path) -> Result<Self> {
        // An unsuccessful cleanup does not authorize deleting unmarked content.
        cleanup(root)?;
        let _guard = root_lock(root)?;
        let name = format!("session-v1-{}", uuid::Uuid::new_v4());
        let mut lease = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(root.join(format!("{name}.lock")))?;
        FileExt::try_lock_exclusive(&lease)?;
        lease.write_all(OWNER)?;
        lease.sync_all()?;
        let path = root.join(name);
        std::fs::create_dir(&path)?;
        Ok(Self {
            root: root.to_owned(),
            path,
            lease: Some(lease),
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for SessionDirectory {
    fn drop(&mut self) {
        drop(self.lease.take());
        // WebView2 may still hold files; preserve the ownership marker and retry
        // on the next startup/open. A crash releases the OS lock automatically.
        let _ = cleanup(&self.root);
    }
}
