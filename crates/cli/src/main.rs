use clap::{Parser, Subcommand};
use opennovel_core::{
    ExportFormat, Library, MAX_IMPORT_BYTES,
    sources::{SourceHub, identify, source_infos},
};
use std::{io::Read, path::PathBuf, sync::atomic::AtomicBool};

#[derive(Parser)]
#[command(version, about = "OpenNovel 多来源下载与本地书库")]
struct Arguments {
    #[arg(long, default_value = ".artifacts/library.db")]
    database: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 标签和归档，不修改远端作品字段
    Organize {
        book_id: String,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long)]
        archived: bool,
    },
    /// 创建数据库一致性备份包，目录必须不存在
    Backup { output: PathBuf },
    /// 从备份恢复到全新目录，不覆盖现有书库
    Restore {
        backup: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// 预览或导出不含标题、正文、路径及凭据的诊断
    Diagnose {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// 列出来源能力与支持边界
    Sources,
    /// 读取已接入官网的真实详情与目录
    Inspect { url: String },
    /// 下载官网公开网页提供的完整章节；受限内容明确记录
    Download {
        url: String,
        #[arg(long, default_value_t = 1)]
        from: usize,
        #[arg(long)]
        to: Option<usize>,
        #[arg(long,conflicts_with_all=["from","to"])]
        chapters: Option<String>,
    },
    /// 导入用户提供的 JSON 书籍
    Import { file: PathBuf },
    /// 查看书库和持久任务
    List,
    /// 恢复中断、取消或失败的导入
    Resume { job_id: String },
    /// 显示章节状态、错误分类及可重试项
    Details { job_id: String },
    /// 只重试指定失败章节，不继续其他待下载章节
    Retry {
        job_id: String,
        #[arg(long = "chapter", required = true)]
        chapters: Vec<String>,
    },
    /// 重读官网目录，补齐缺失正文并获取末尾新增章节
    Update { book_id: String },
    /// 暂停尚未执行的持久任务
    Pause { job_id: String },
    /// 显示本地目录及导出记录
    Book { book_id: String },
    /// 离线读取已保存章节
    Read { book_id: String, chapter_id: String },
    /// 仅导出指定任务选中的章节
    ExportJob {
        job_id: String,
        #[arg(long, default_value = "epub")]
        format: String,
        #[arg(long, default_value = ".artifacts/exports")]
        output: PathBuf,
    },
    /// 从本地保存内容导出；现有文件不会被覆盖
    Export {
        book_id: String,
        #[arg(long, default_value = "txt", value_parser = ["txt", "epub"])]
        format: String,
        #[arg(long, default_value = ".artifacts/exports")]
        output: PathBuf,
    },
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args = Arguments::parse();
    if let Command::Restore { backup, output } = &args.command {
        let restored = Library::restore(backup, output)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&restored.diagnostics()?)?
        );
        return Ok(0);
    }
    let library = Library::open(args.database)?;
    let hub = SourceHub::new();
    let result = match args.command {
        Command::Organize {
            book_id,
            tags,
            archived,
        } => serde_json::to_value(library.organize(&book_id, &tags, archived)?)?,
        Command::Backup { output } => serde_json::to_value(library.backup(output)?)?,
        Command::Restore { .. } => unreachable!("handled before opening the active library"),
        Command::Diagnose { output } => serde_json::to_value(if let Some(path) = output {
            library.export_diagnostics(path)?
        } else {
            library.diagnostics()?
        })?,
        Command::Sources => serde_json::to_value(source_infos())?,
        Command::Inspect { url } => serde_json::to_value(hub.inspect(&url)?)?,
        Command::Download {
            url,
            from,
            to,
            chapters,
        } => {
            let source = hub.adapter(identify(&url)?.0)?;
            let catalog = hub.inspect(&url)?;
            let job = if let Some(chapters) = chapters {
                library.create_download_selected(
                    &catalog,
                    &opennovel_core::parse_selection(&chapters, catalog.chapters.len())?,
                )?
            } else {
                library.create_download(&catalog, from, to.unwrap_or(catalog.chapters.len()))?
            };
            serde_json::to_value(library.run_job(&job, source, &AtomicBool::new(false))?)?
        }
        Command::Import { file } => {
            let mut json = String::new();
            std::fs::File::open(file)?
                .take((MAX_IMPORT_BYTES + 1) as u64)
                .read_to_string(&mut json)?;
            let job = library.create_import(&json)?;
            serde_json::to_value(library.run_import(&job, &AtomicBool::new(false))?)?
        }
        Command::List => serde_json::to_value(library.snapshot()?)?,
        Command::Pause { job_id } => {
            library.pause_job(&job_id)?;
            serde_json::to_value(library.job(&job_id)?)?
        }
        Command::Book { book_id } => serde_json::to_value(library.book_details(&book_id)?)?,
        Command::Read {
            book_id,
            chapter_id,
        } => serde_json::to_value(library.read_chapter(&book_id, &chapter_id)?)?,
        Command::ExportJob {
            job_id,
            format,
            output,
        } => serde_json::to_value(library.export_job(
            &job_id,
            output,
            ExportFormat::parse(&format)?,
        )?)?,
        Command::Details { job_id } => serde_json::to_value(library.job_details(&job_id)?)?,
        Command::Retry { job_id, chapters } => {
            let job = library.job(&job_id)?;
            serde_json::to_value(library.retry_chapters(
                &job_id,
                hub.adapter(&job.source)?,
                &AtomicBool::new(false),
                &chapters,
            )?)?
        }
        Command::Update { book_id } => {
            let book = library
                .snapshot()?
                .books
                .into_iter()
                .find(|b| b.id == book_id)
                .ok_or("作品不存在")?;
            let url = book.source_url().ok_or("该来源不支持在线更新")?;
            let source = hub.adapter(identify(&url)?.0)?;
            let catalog = hub.inspect(&url)?;
            let job = library.create_download(&catalog, 1, catalog.chapters.len())?;
            serde_json::to_value(library.run_job(&job, source, &AtomicBool::new(false))?)?
        }
        Command::Resume { job_id } => {
            let job = library.job(&job_id)?;
            serde_json::to_value(if job.source == "local-json" {
                library.run_import(&job_id, &AtomicBool::new(false))?
            } else {
                library.run_job(&job_id, hub.adapter(&job.source)?, &AtomicBool::new(false))?
            })?
        }
        Command::Export {
            book_id,
            format,
            output,
        } => serde_json::to_value(library.export(
            &book_id,
            output,
            ExportFormat::parse(&format)?,
        )?)?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(
        if result
            .get("state")
            .and_then(|state| state.as_str())
            .is_some_and(|state| state != "completed")
        {
            2
        } else {
            0
        },
    )
}
