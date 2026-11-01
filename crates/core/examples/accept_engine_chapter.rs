//! Isolated acceptance: validate a single engine envelope before creating a fresh sample library.
use opennovel_core::{ExportFormat, ImportDocument, Library, sources::parse_engine_chapter};
use std::{io::Read, path::PathBuf, sync::atomic::AtomicBool};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [book, item, title, output] = args.as_slice() else {
        return Err("expected book item title fresh-output-directory".into());
    };
    let mut raw = String::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_string(&mut raw)?;
    let chapter = parse_engine_chapter(&raw, book, item, title)?;
    let document = ImportDocument {
        id: format!("engine-acceptance:{book}:{item}"),
        title: "本地引擎单章验收（非全书）".into(),
        author: String::new(),
        chapters: vec![chapter],
    };
    let output = PathBuf::from(output);
    std::fs::create_dir(&output)?;
    let library = Library::open(output.join("sample.db"))?;
    let job = library.create_import(&serde_json::to_string(&document)?)?;
    let snapshot = library.run_import(&job, &AtomicBool::new(false))?;
    let export = library.export(&snapshot.book_id, output.join("exports"), ExportFormat::Txt)?;
    println!(
        "{}",
        serde_json::json!({"scope":"single_chapter_acceptance", "saved":snapshot.saved, "export":export})
    );
    Ok(())
}
