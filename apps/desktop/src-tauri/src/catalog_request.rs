//! One preparation flow per main window. Cancellation and job creation share a lock.
use opennovel_core::{Error, Result};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Request {
    id: String,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub(crate) struct CatalogRequests {
    active: Mutex<Option<Request>>,
}

impl CatalogRequests {
    pub fn is_active(&self) -> Result<bool> {
        Ok(self.active.lock().map_err(|_| unavailable())?.is_some())
    }

    pub fn begin(&self) -> Result<String> {
        let mut active = self.active.lock().map_err(|_| unavailable())?;
        if let Some(previous) = active.take() {
            previous.stop.store(true, Ordering::Release);
        }
        let id = uuid::Uuid::new_v4().to_string();
        *active = Some(Request {
            id: id.clone(),
            stop: Arc::new(AtomicBool::new(false)),
        });
        Ok(id)
    }

    pub fn stop_flag(&self, id: &str) -> Result<Arc<AtomicBool>> {
        self.active
            .lock()
            .map_err(|_| unavailable())?
            .as_ref()
            .filter(|request| request.id == id)
            .map(|request| Arc::clone(&request.stop))
            .ok_or(Error::Cancelled)
    }

    /// Also used to release a completed read. An old cleanup cannot cancel a new flow.
    pub fn cancel(&self, id: &str) -> Result<bool> {
        let mut active = self.active.lock().map_err(|_| unavailable())?;
        if active.as_ref().is_some_and(|request| request.id == id) {
            active.take().unwrap().stop.store(true, Ordering::Release);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// The lock orders cancellation against the final database transaction. Once
    /// committed, cancellation returns false and the UI must show the created job.
    pub fn commit<T>(&self, id: &str, create: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut active = self.active.lock().map_err(|_| unavailable())?;
        let request = active
            .as_ref()
            .filter(|request| request.id == id)
            .ok_or(Error::Cancelled)?;
        if request.stop.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let result = create();
        active.take().unwrap().stop.store(true, Ordering::Release);
        result
    }
}

fn unavailable() -> Error {
    Error::Conflict("目录请求管理器不可用".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_before_dispatch_blocks_reads_and_commit() {
        let requests = CatalogRequests::default();
        let id = requests.begin().unwrap();
        let stop = requests.stop_flag(&id).unwrap();
        assert!(requests.cancel(&id).unwrap());
        assert!(stop.load(Ordering::Acquire));
        assert!(matches!(requests.stop_flag(&id), Err(Error::Cancelled)));
        assert!(matches!(
            requests.commit::<()>(&id, || panic!("cancelled job created")),
            Err(Error::Cancelled)
        ));
    }

    #[test]
    fn replacement_stops_old_read_and_late_cleanup_preserves_new_request() {
        let requests = CatalogRequests::default();
        let old = requests.begin().unwrap();
        let stop = requests.stop_flag(&old).unwrap();
        let current = requests.begin().unwrap();
        assert!(stop.load(Ordering::Acquire));
        assert!(!requests.cancel(&old).unwrap());
        assert!(
            !requests
                .stop_flag(&current)
                .unwrap()
                .load(Ordering::Acquire)
        );
        assert_eq!(requests.commit(&current, || Ok("job")).unwrap(), "job");
        assert!(!requests.cancel(&current).unwrap());
    }

    #[test]
    fn cancellation_waiting_on_commit_reports_that_job_already_exists() {
        let requests = CatalogRequests::default();
        let id = requests.begin().unwrap();
        let (entered, received) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let commit = scope.spawn(|| {
                requests.commit(&id, || {
                    entered.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    Ok("created")
                })
            });
            received.recv().unwrap();
            assert!(!requests.cancel(&id).unwrap());
            assert_eq!(commit.join().unwrap().unwrap(), "created");
        });
    }
}
