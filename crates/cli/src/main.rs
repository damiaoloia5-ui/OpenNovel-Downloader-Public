use clap::{Parser, Subcommand};
use opennovel_core::{ExportFormat, Library, MAX_IMPORT_BYTES, sources::Fanqie};
use std::{io::Read, path::PathBuf, sync::atomic::AtomicBool};

#[derive(Parser)]
#[command(version, about = "OpenNovel 下载与本地书库（番茄公开网页实验适配）")]
struct Arguments {
    #[arg(long, default_value = ".artifacts/library.db")]
    database: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 读取番茄作品的真实目录和网页访问状态
    Inspect { url: String },
    /// 下载官网公开网页提供的完整章节；受限内容明确记录
    Download {
        url: String,
        #[arg(long, default_value_t = 1)]
        from: usize,
        #[arg(long)]
        to: Option<usize>,
    },
    /// 导入用户提供的 JSON 书籍
    Import { file: PathBuf },
    /// 查看书库和持久任务
    List,
    /// 恢复中断、取消或失败的导入
    Resume { job_id: String },
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
    let library = Library::open(args.database)?;
    let result = match args.command {
        Command::Inspect { url } => serde_json::to_value(Fanqie::new()?.inspect(&url)?)?,
        Command::Download { url, from, to } => {
            let source = Fanqie::new()?;
            let catalog = source.inspect(&url)?;
            let job =
                library.create_download(&catalog, from, to.unwrap_or(catalog.chapters.len()))?;
            serde_json::to_value(library.run_job(&job, &source, &AtomicBool::new(false))?)?
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
        Command::Resume { job_id } => {
            let job = library.job(&job_id)?;
            serde_json::to_value(if job.source == "local-json" {
                library.run_import(&job_id, &AtomicBool::new(false))?
            } else {
                library.run_job(&job_id, &Fanqie::new()?, &AtomicBool::new(false))?
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
