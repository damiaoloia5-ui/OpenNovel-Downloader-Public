//! Private stdio queue companion. Requires an explicit catalog or existing job ID.
use opennovel_core::{
    Error, ExportFormat, ImportChapter, Library, Result,
    sources::{Catalog, ChapterRef, LOCAL_ENGINE_SOURCE, SourceAdapter, parse_engine_chapter},
};
use std::{
    io::{BufRead, Read, Write},
    sync::atomic::AtomicBool,
};

fn emit(value: serde_json::Value) -> Result<()> {
    println!("{value}");
    std::io::stdout().flush()?;
    Ok(())
}
struct StdioSource;
impl SourceAdapter for StdioSource {
    fn source_id(&self) -> &str {
        LOCAL_ENGINE_SOURCE
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        _: &AtomicBool,
    ) -> Result<ImportChapter> {
        emit(
            serde_json::json!({"op":"read","book_id":book,"chapter_id":chapter.id,"title":chapter.title}),
        )?;
        let mut raw = Vec::new();
        std::io::stdin()
            .lock()
            .take(2 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut raw)?;
        if raw.is_empty() {
            return Err(Error::Cancelled);
        }
        let text = std::str::from_utf8(&raw)
            .map_err(|_| Error::SourceChanged("引擎通信不是 UTF-8".into()))?;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
            && let Some(error) = value.get("error").and_then(|e| e.as_str())
        {
            return Err(match error {
                "cancelled" => Error::Cancelled,
                "restricted" => Error::Restricted("引擎确认章节受限".into()),
                "challenge" => Error::AccessChallenge("引擎会话已停止，处理后显式继续".into()),
                _ => Error::Network("引擎会话失败".into()),
            });
        }
        parse_engine_chapter(text, book, &chapter.id, &chapter.title)
    }
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [db, mode, input, output] = args.as_slice() else {
        return Err("database start|resume catalog-file|job-id output-directory".into());
    };
    let library = Library::open(db)?;
    let job = match mode.as_str() {
        "start" => {
            let file = std::fs::File::open(input)?;
            let mut raw = String::new();
            file.take(8 * 1024 * 1024 + 1).read_to_string(&mut raw)?;
            if raw.len() > 8 * 1024 * 1024 {
                return Err("catalog too large".into());
            }
            let catalog: Catalog = serde_json::from_str(&raw)?;
            library.create_engine_download(
                &catalog,
                &(1..=catalog.chapters.len()).collect::<Vec<_>>(),
            )?
        }
        "resume" => input.clone(),
        _ => return Err("unsupported mode".into()),
    };
    if library.job(&job)?.source != LOCAL_ENGINE_SOURCE {
        return Err("not a local engine job".into());
    }
    emit(serde_json::json!({"op":"job","id":job}))?;
    let result = library.run_job(&job, &StdioSource, &AtomicBool::new(false));
    let snapshot = library.job(&job)?;
    let export = if snapshot.saved > 0 {
        Some(library.export(&snapshot.book_id, output, ExportFormat::Txt)?)
    } else {
        None
    };
    emit(
        serde_json::json!({"op":"result","job":snapshot,"export":export,"run_failed":result.is_err()}),
    )?;
    Ok(())
}
