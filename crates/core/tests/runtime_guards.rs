use opennovel_core::{
    session_directory::{SessionDirectory, cleanup},
    traffic::TrafficGate,
};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(root: &Path, mode: &str) -> ChildGuard {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_process_fixture", "--nocapture"])
        .env("OPENNOVEL_TEST_GUARD_ROOT", root)
        .env("OPENNOVEL_TEST_GUARD_MODE", mode)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join("ready").exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "child terminated before ready"
        );
        assert!(Instant::now() < deadline, "child ready timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}
#[test]
fn child_process_fixture() {
    let Some(root) = std::env::var_os("OPENNOVEL_TEST_GUARD_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let mode = std::env::var("OPENNOVEL_TEST_GUARD_MODE").unwrap();
    if mode == "library" {
        let library = opennovel_core::Library::open(root.join("library.db")).unwrap();
        let job = library
            .create_import(include_str!("../../../examples/local-book.json"))
            .unwrap();
        library
            .run_import_observed(&job, &AtomicBool::new(false), |_| {
                std::fs::write(root.join("ready"), job.as_bytes()).unwrap();
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
            .unwrap();
        return;
    }
    let _traffic = if mode == "traffic" {
        Some(TrafficGate::at(root, "fanqie").unwrap().acquire().unwrap())
    } else {
        None
    };
    let _session = if mode == "session" {
        let session = SessionDirectory::new(root).unwrap();
        std::fs::write(
            session.path().join("original-sample"),
            b"original test session data",
        )
        .unwrap();
        Some(session)
    } else {
        None
    };
    std::fs::write(root.join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn platform_lock_spans_processes_and_recovers_after_forced_exit() {
    let root = tempfile::tempdir().unwrap();
    let child = start(root.path(), "traffic");
    assert!(
        TrafficGate::at(root.path(), "fanqienovel.com")
            .unwrap()
            .acquire()
            .is_err()
    );
    assert!(
        TrafficGate::at(root.path(), "qimao")
            .unwrap()
            .acquire()
            .is_ok()
    );
    drop(child);
    assert!(
        TrafficGate::at(root.path(), "fanqie")
            .unwrap()
            .acquire()
            .is_ok()
    );
}

#[test]
fn actual_worker_exit_keeps_committed_chapter_and_resumes_the_remainder() {
    let root = tempfile::tempdir().unwrap();
    let child = start(root.path(), "library");
    let job = std::fs::read_to_string(root.path().join("ready")).unwrap();
    drop(child);
    let library = opennovel_core::Library::open(root.path().join("library.db")).unwrap();
    let before = library.job(&job).unwrap();
    assert_eq!((before.state.as_str(), before.saved), ("paused", 1));
    let after = library.run_import(&job, &AtomicBool::new(false)).unwrap();
    assert_eq!((after.state.as_str(), after.saved), ("completed", 2));
}

#[test]
fn failed_chapter_write_rolls_back_and_can_resume_without_losing_prior_body() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("library.db");
    let library = opennovel_core::Library::open(&path).unwrap();
    let job = library
        .create_import(include_str!("../../../examples/local-book.json"))
        .unwrap();
    let injector = rusqlite::Connection::open(&path).unwrap();
    // Deterministic write failure, not a claim of physical disk-full testing.
    let failed = library.run_import_observed(&job, &AtomicBool::new(false), |_| {
        injector.execute_batch("CREATE TRIGGER original_failure BEFORE INSERT ON chapters BEGIN SELECT RAISE(ABORT, 'original storage fault'); END;").unwrap();
    });
    assert!(failed.is_err());
    let state = library.job(&job).unwrap();
    assert_eq!(state.saved, 1);
    let completed: i64 = injector
        .query_row(
            "SELECT COUNT(*) FROM job_items WHERE state='completed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let bodies: i64 = injector
        .query_row("SELECT COUNT(*) FROM chapters", [], |r| r.get(0))
        .unwrap();
    assert_eq!((completed, bodies), (1, 1));
    injector
        .execute_batch("DROP TRIGGER original_failure")
        .unwrap();
    drop(library);
    let reopened = opennovel_core::Library::open(&path).unwrap();
    assert_eq!(
        reopened
            .run_import(&job, &AtomicBool::new(false))
            .unwrap()
            .saved,
        2
    );
}
#[test]
fn pacing_and_cooldown_survive_new_handles_and_corruption_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let gate = TrafficGate::at(root.path(), "shuqi").unwrap();
    let lease = gate.acquire().unwrap();
    assert!(lease.reserve(Duration::from_secs(1)).unwrap().is_zero());
    drop(lease);
    let other = TrafficGate::at(root.path(), "c13.shuqireader.com")
        .unwrap()
        .acquire()
        .unwrap();
    assert!(!other.reserve(Duration::from_secs(1)).unwrap().is_zero());
    assert!(
        other
            .wait(Duration::from_secs(1), &AtomicBool::new(true))
            .is_err()
    );
    other.defer(60).unwrap();
    let original = std::fs::read(root.path().join("shuqi.json")).unwrap();
    other.defer(1).unwrap();
    assert_eq!(
        std::fs::read(root.path().join("shuqi.json")).unwrap(),
        original
    );
    drop(other);
    assert!(gate.acquire().is_err());
    std::fs::write(root.path().join("shuqi.json"), b"invalid").unwrap();
    assert!(gate.acquire().is_err());
    assert!(TrafficGate::at(root.path(), "../escape").is_err());
}
#[test]
fn failed_persistence_never_reserves_a_request() {
    let root = tempfile::tempdir().unwrap();
    let gate = TrafficGate::at(root.path(), "qidian").unwrap();
    let lease = gate.acquire().unwrap();
    std::fs::create_dir(root.path().join("qidian.json")).unwrap();
    assert!(lease.reserve(Duration::from_secs(3)).is_err());
    assert!(lease.defer(60).is_err());
}
#[test]
fn cleanup_skips_active_and_unowned_but_reaps_a_crashed_session() {
    let root = tempfile::tempdir().unwrap();
    let unknown = root
        .path()
        .join(format!("session-v1-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&unknown).unwrap();
    std::fs::write(unknown.join("keep.txt"), b"user-owned").unwrap();
    let child = start(root.path(), "session");
    let report = cleanup(root.path()).unwrap();
    assert_eq!(report.active, 1);
    assert_eq!(report.retained, 1);
    assert_eq!(report.removed, 0);
    drop(child);
    let report = cleanup(root.path()).unwrap();
    assert_eq!(report.removed, 1);
    assert!(unknown.join("keep.txt").is_file());
    let session = SessionDirectory::new(root.path()).unwrap();
    let path = session.path().to_owned();
    drop(session);
    assert!(!path.exists());
}
#[test]
fn cleanup_never_follows_a_directory_link() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("keep.txt");
    std::fs::write(&sentinel, b"original outside data").unwrap();
    let session = SessionDirectory::new(root.path()).unwrap();
    let path = session.path().to_owned();
    let link = path.join("linked");
    #[cfg(windows)]
    assert!(
        Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(outside.path())
            .stdout(Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    drop(session);
    assert!(path.exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"original outside data");
    #[cfg(windows)]
    std::fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    std::fs::remove_file(&link).unwrap();
    assert_eq!(cleanup(root.path()).unwrap().removed, 1);
}
