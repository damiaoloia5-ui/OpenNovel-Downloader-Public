//! Manual, offline baseline: cargo run --release -p opennovel-core --example performance -- <new-directory>
use opennovel_core::{ExportFormat, ImportChapter, ImportDocument, Library};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

fn measure(mut work: impl FnMut(), times: usize) -> serde_json::Value {
    let mut samples = Vec::new();
    for _ in 0..times {
        let start = Instant::now();
        work();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    json!({"samples":times,"p50_ms":samples[times/2],"p95_ms":samples[(times*95).div_ceil(100)-1],"max_ms":samples[times-1]})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass a new output directory")?,
    );
    std::fs::create_dir(&directory)?;
    let library = Library::open(directory.join("library.db"))?;
    let stop = AtomicBool::new(false);
    let start = Instant::now();
    for i in 0..1000 {
        let original = ImportDocument {
            id: format!("benchmark-{i:04}"),
            title: format!("原创基准作品{i:04}"),
            author: "OpenNovel".into(),
            chapters: vec![ImportChapter {
                id: "1".into(),
                title: "原创章节".into(),
                paragraphs: vec!["原创性能样本，不含平台正文。".into()],
            }],
        };
        let job = library.create_import(&serde_json::to_string(&original)?)?;
        library.run_import(&job, &stop)?;
    }
    let small_seed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let original = ImportDocument {
        id: "benchmark-long".into(),
        title: "原创万章基准".into(),
        author: "OpenNovel".into(),
        chapters: (1..=10_000)
            .map(|i| ImportChapter {
                id: i.to_string(),
                title: format!("原创第{i}章"),
                paragraphs: vec!["这是原创压力测试文字，用于测量正文的保存和导出。".repeat(8)],
            })
            .collect(),
    };
    let input = serde_json::to_string(&original)?;
    let start = Instant::now();
    let job = library.create_import(&input)?;
    let book = library.run_import(&job, &stop)?.book_id;
    let long_import_ms = start.elapsed().as_secs_f64() * 1000.0;
    drop(original);
    drop(input);
    let snapshot = measure(
        || {
            assert_eq!(library.snapshot().unwrap().books.len(), 1001);
        },
        30,
    );
    let details = measure(
        || {
            assert_eq!(library.book_details(&book).unwrap().chapters.len(), 10_000);
        },
        30,
    );
    let read = measure(
        || {
            assert_eq!(library.read_chapter(&book, "10000").unwrap().id, "10000");
        },
        50,
    );
    let start = Instant::now();
    let txt = library.export(&book, directory.join("exports"), ExportFormat::Txt)?;
    let txt_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let epub = library.export(&book, directory.join("exports"), ExportFormat::Epub)?;
    let epub_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let backup = directory.join("backup");
    library.backup(&backup)?;
    let backup_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let restored = Library::restore(&backup, directory.join("restored"))?;
    let restore_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(restored.diagnostics()?.saved_chapters, 11_000);
    let result = json!({"version":env!("CARGO_PKG_VERSION"),"schema":library.diagnostics()?.schema,"books":1001,"chapters":11_000,"long_book":book,"small_seed_ms":small_seed_ms,"long_import_ms":long_import_ms,"snapshot":snapshot,"long_details":details,"read_chapter":read,"txt_ms":txt_ms,"epub_ms":epub_ms,"backup_ms":backup_ms,"restore_ms":restore_ms,"txt_bytes":std::fs::metadata(txt.path)?.len(),"epub_bytes":std::fs::metadata(epub.path)?.len(),"database_bytes":std::fs::metadata(directory.join("library.db"))?.len()});
    std::fs::write(
        directory.join("baseline.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
