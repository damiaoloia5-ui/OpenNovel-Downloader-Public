use opennovel_core::Library;
use std::time::Instant;

fn seed(path: &std::path::Path) {
    let mut db = rusqlite::Connection::open(path).unwrap();
    let tx = db.transaction().unwrap();
    for book in 0..40 {
        tx.execute("INSERT INTO books(id,source,remote_id,title,author,total) VALUES (?1,'local-json',?1,'原创性能样书','测试',1000)",[format!("b{book}")]).unwrap();
        tx.execute(
            "INSERT INTO jobs(id,book_id,state) VALUES (?1,?2,'queued')",
            [format!("j{book}"), format!("b{book}")],
        )
        .unwrap();
        let mut insert=tx.prepare("INSERT INTO job_items(job_id,chapter_id,position,payload,state) VALUES (?1,?2,?3,'{\"title\":\"原创章\"}','queued')").unwrap();
        for chapter in 0..1000 {
            insert
                .execute(rusqlite::params![
                    format!("j{book}"),
                    format!("c{chapter}"),
                    chapter
                ])
                .unwrap();
        }
    }
    tx.commit().unwrap();
}
#[test]
fn scoped_queries_match_full_snapshot_and_do_not_require_other_books() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("test.db");
    let l = Library::open(&path).unwrap();
    seed(&path);
    let snapshot = l.snapshot().unwrap();
    let target = snapshot.jobs.iter().find(|j| j.id == "j0").unwrap();
    assert_eq!(
        serde_json::to_value(l.job("j0").unwrap()).unwrap(),
        serde_json::to_value(target).unwrap()
    );
    assert_eq!(
        serde_json::to_value(l.book_summary("b0").unwrap()).unwrap(),
        serde_json::to_value(snapshot.books.iter().find(|b| b.id == "b0").unwrap()).unwrap()
    );
    assert!(l.job("missing").is_err());
    assert!(l.book_summary("missing").is_err());
}
#[test]
#[ignore = "explicit local synthetic performance measurement"]
fn scoped_query_benchmark() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("test.db");
    let l = Library::open(&path).unwrap();
    seed(&path);
    let now = Instant::now();
    for _ in 0..30 {
        std::hint::black_box(
            l.snapshot()
                .unwrap()
                .jobs
                .into_iter()
                .find(|j| j.id == "j0")
                .unwrap(),
        );
    }
    let previous = now.elapsed();
    let now = Instant::now();
    for _ in 0..30 {
        std::hint::black_box(l.job("j0").unwrap());
    }
    let scoped = now.elapsed();
    println!(
        "synthetic 40 books x 1000 chapters, 30 reads: previous_full_snapshot_ms={:.3}, scoped_job_ms={:.3}, ratio={:.2}",
        previous.as_secs_f64() * 1000.,
        scoped.as_secs_f64() * 1000.,
        previous.as_secs_f64() / scoped.as_secs_f64()
    );
}
