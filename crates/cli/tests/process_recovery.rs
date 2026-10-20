use opennovel_core::Library;
use std::{
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[test]
fn killed_cli_releases_lock_and_resumes_only_unsaved_items() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("library.db");
    let input = directory.path().join("input.json");
    let chapters: Vec<_> = (0..10_000)
        .map(|position| {
            serde_json::json!({
                "id": format!("{position:06}"), "title": format!("Chapter {position}"),
                "paragraphs": ["Original local content for process recovery testing."],
            })
        })
        .collect();
    std::fs::write(&input, serde_json::to_vec(&serde_json::json!({
        "id": "process-recovery", "title": "Process recovery", "author": "OpenNovel tests", "chapters": chapters,
    })).unwrap()).unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_opennovel"))
        .arg("--database")
        .arg(&database)
        .arg("import")
        .arg(&input)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut observed_partial = false;
    while Instant::now() < deadline {
        if let Ok(connection) = rusqlite::Connection::open_with_flags(
            &database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            let count = connection.query_row("SELECT COUNT(*) FROM chapters", [], |row| {
                row.get::<_, u32>(0)
            });
            if let Ok(saved) = count
                && saved > 0
                && saved < 10_000
            {
                observed_partial = true;
                break;
            }
        }
        if process.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if process.try_wait().unwrap().is_none() {
        process.kill().unwrap();
    }
    process.wait().unwrap();
    assert!(
        observed_partial,
        "The test must interrupt a real CLI while chapters are being committed"
    );
    let library = Library::open(&database).unwrap();
    let snapshot = library.snapshot().unwrap();
    let job = &snapshot.jobs[0];
    assert_eq!(job.state, "paused");
    assert!(job.saved > 0 && job.saved < job.total);
    assert_eq!(job.saved, snapshot.books[0].saved_chapters);
    let resumed = library
        .run_import(&job.id, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        (resumed.state.as_str(), resumed.saved, resumed.total),
        ("completed", 10_000, 10_000)
    );
    let connection = rusqlite::Connection::open(&database).unwrap();
    let count: u32 = connection
        .query_row(
            "SELECT COUNT(DISTINCT chapter_id) FROM chapters",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 10_000);
}
