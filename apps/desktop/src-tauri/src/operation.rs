//! Admit IPC operations atomically; installation waits for admitted work to finish.
use std::sync::Mutex;
use tauri::{
    Runtime, State,
    ipc::{CommandArg, CommandItem, InvokeError},
};

#[derive(Default)]
struct Gate {
    installing: bool,
    active: usize,
}
static GATE: Mutex<Gate> = Mutex::new(Gate {
    installing: false,
    active: 0,
});

pub struct Lease;
impl Lease {
    pub(crate) fn enter() -> Result<Self, String> {
        let mut gate = GATE.lock().map_err(|_| "操作保护不可用")?;
        if gate.installing {
            return Err("正在准备安装更新，请稍候".into());
        }
        gate.active += 1;
        Ok(Self)
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut gate) = GATE.lock() {
            gate.active -= 1;
        }
    }
}
pub struct Maintenance;
impl Maintenance {
    pub fn begin() -> Result<Self, String> {
        let mut gate = GATE.lock().map_err(|_| "操作保护不可用")?;
        if gate.installing {
            return Err("已有更新正在安装".into());
        }
        gate.installing = true;
        Ok(Self)
    }
    pub fn drained(&self) -> Result<bool, String> {
        Ok(GATE.lock().map_err(|_| "操作保护不可用")?.active == 0)
    }
}
impl Drop for Maintenance {
    fn drop(&mut self) {
        if let Ok(mut gate) = GATE.lock() {
            gate.installing = false;
        }
    }
}
pub fn installing() -> bool {
    GATE.lock().map_or(true, |g| g.installing)
}

/// A command's lease lives across its awaits, unlike an invoke-dispatch callback.
pub struct OperationState<'r, T: Send + Sync + 'static> {
    state: State<'r, T>,
    _lease: Lease,
}
impl<T: Send + Sync + 'static> std::ops::Deref for OperationState<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.state.inner()
    }
}
impl<'de, R: Runtime, T: Send + Sync + 'static> CommandArg<'de, R> for OperationState<'de, T> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let lease = Lease::enter().map_err(InvokeError::from)?;
        Ok(Self {
            state: State::from_command(command)?,
            _lease: lease,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn installation_blocks_new_commands_and_waits_for_existing_lease() {
        let lease = super::Lease::enter().unwrap();
        let install = super::Maintenance::begin().unwrap();
        assert!(!install.drained().unwrap());
        assert!(super::Lease::enter().is_err());
        assert!(super::Maintenance::begin().is_err());
        drop(lease);
        assert!(install.drained().unwrap());
        drop(install);
        assert!(super::Lease::enter().is_ok());
    }
}
