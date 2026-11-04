//! Local workspaces, not authentication identities. All paths belong to the OS user.
use crate::{Error, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProfile {
    pub id: String,
    pub name: String,
    // Only explicit legacy-library adoption may set an external database path.
    pub database: Option<PathBuf>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    selected: String,
    profiles: Vec<LocalProfile>,
}
#[derive(Clone)]
pub struct ProfileStore {
    root: PathBuf,
}
#[derive(Clone, Debug)]
pub struct ProfilePaths {
    pub profile: LocalProfile,
    pub directory: PathBuf,
    pub database: PathBuf,
    pub exports: PathBuf,
    pub sessions: PathBuf,
    pub webview: PathBuf,
}

pub fn user_data_root() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("org.opennovel.downloader"))
        .ok_or_else(|| Error::Invalid("无法确定当前系统用户的数据目录".into()))
}

impl ProfileStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        if !root.as_ref().is_absolute() {
            return Err(Error::Invalid("用户数据目录必须为绝对路径".into()));
        }
        Ok(Self {
            root: root.as_ref().to_owned(),
        })
    }
    fn lock(&self) -> Result<File> {
        std::fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("profiles.lock"))?;
        FileExt::lock_exclusive(&lock)?;
        Ok(lock)
    }
    fn read(&self) -> Result<Registry> {
        let path = self.root.join("profiles.json");
        if !path.exists() {
            let legacy = self.root.join("library.db");
            let profile = LocalProfile {
                id: uuid::Uuid::new_v4().to_string(),
                name: if legacy.is_file() {
                    "原有书库"
                } else {
                    "我的书库"
                }
                .into(),
                database: legacy.is_file().then_some(legacy),
            };
            let registry = Registry {
                version: 1,
                selected: profile.id.clone(),
                profiles: vec![profile],
            };
            self.write(&registry)?;
            return Ok(registry);
        }
        if path.metadata()?.len() > 1024 * 1024 {
            return Err(Error::Invalid("用户列表文件过大".into()));
        }
        let r: Registry = serde_json::from_slice(&std::fs::read(path)?)?;
        let mut ids = std::collections::HashSet::new();
        if r.version != 1
            || r.profiles.is_empty()
            || r.profiles.len() > 100
            || !r.profiles.iter().any(|p| p.id == r.selected)
            || r.profiles.iter().any(|p| {
                uuid::Uuid::parse_str(&p.id).is_err()
                    || !ids.insert(&p.id)
                    || validate_name(&p.name).is_err()
                    || p.database.as_ref().is_some_and(|d| !d.is_absolute())
            })
        {
            return Err(Error::Invalid(
                "用户列表无效；原书库未被覆盖，请恢复用户列表备份".into(),
            ));
        }
        Ok(r)
    }
    fn write(&self, registry: &Registry) -> Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(&self.root)?;
        file.write_all(&serde_json::to_vec_pretty(registry)?)?;
        file.as_file_mut().sync_all()?;
        file.persist(self.root.join("profiles.json"))
            .map_err(|e| e.error)?;
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<LocalProfile>> {
        let _lock = self.lock()?;
        Ok(self.read()?.profiles)
    }
    /// Save a consistent registry copy for upgrade recovery; never replaces a previous copy.
    pub fn backup_registry(&self, destination: &Path) -> Result<()> {
        let _lock = self.lock()?;
        let registry = self.read()?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        file.write_all(&serde_json::to_vec_pretty(&registry)?)?;
        file.sync_all()?;
        Ok(())
    }
    pub fn resolve(&self, id: Option<&str>) -> Result<ProfilePaths> {
        let _lock = self.lock()?;
        let r = self.read()?;
        let id = id.unwrap_or(&r.selected);
        let p = r
            .profiles
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| Error::Invalid("本地用户不存在".into()))?;
        let directory = self.root.join("profiles").join(&p.id);
        Ok(ProfilePaths {
            database: p
                .database
                .clone()
                .unwrap_or_else(|| directory.join("library.db")),
            exports: directory.join("exports"),
            sessions: directory.join("session-cache"),
            webview: directory.join("ui-webview"),
            directory,
            profile: p,
        })
    }
    pub fn create(&self, name: &str) -> Result<LocalProfile> {
        validate_name(name)?;
        let _lock = self.lock()?;
        let mut r = self.read()?;
        if r.profiles.len() >= 100 {
            return Err(Error::Invalid("本地用户最多 100 个".into()));
        }
        if r.profiles.iter().any(|p| p.name == name.trim()) {
            return Err(Error::Invalid("用户名称已存在，请换一个名称".into()));
        }
        let p = LocalProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().into(),
            database: None,
        };
        r.profiles.push(p.clone());
        self.write(&r)?;
        Ok(p)
    }
    pub fn select(&self, id: &str) -> Result<()> {
        let _lock = self.lock()?;
        let mut r = self.read()?;
        if !r.profiles.iter().any(|p| p.id == id) {
            return Err(Error::Invalid("本地用户不存在".into()));
        }
        r.selected = id.into();
        self.write(&r)
    }
    /// Register a chosen legacy file in place. Never read, copy, migrate or seed its content.
    pub fn adopt(&self, database: &Path) -> Result<LocalProfile> {
        let database = std::path::absolute(database)?;
        let _lock = self.lock()?;
        let mut r = self.read()?;
        if let Some(p) = r.profiles.iter().find(|p| {
            let path = p
                .database
                .clone()
                .unwrap_or_else(|| self.root.join("profiles").join(&p.id).join("library.db"));
            path.canonicalize().unwrap_or(path)
                == database.canonicalize().unwrap_or_else(|_| database.clone())
        }) {
            return Ok(p.clone());
        }
        if r.profiles.len() >= 100 {
            return Err(Error::Invalid("本地用户最多 100 个".into()));
        }
        let p = LocalProfile {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("原有书库 {}", r.profiles.len()),
            database: Some(database),
        };
        r.profiles.push(p.clone());
        self.write(&r)?;
        Ok(p)
    }

    pub fn startup_marker(&self, token: &str) -> Result<PathBuf> {
        if uuid::Uuid::parse_str(token).is_err() {
            return Err(Error::Invalid("用户切换标记无效".into()));
        }
        Ok(self.root.join("handoff").join(format!("{token}.ready")))
    }
}
fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name.trim().chars().count() > 40
        || name.chars().any(char::is_control)
    {
        return Err(Error::Invalid(
            "用户名称需为 1–40 个字符，不能包含控制字符".into(),
        ));
    }
    Ok(())
}
