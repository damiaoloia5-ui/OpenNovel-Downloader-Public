use clap::{Parser, Subcommand};
use opennovel_core::{ExportFormat, Library, MAX_IMPORT_BYTES};
use std::{io::Read, path::PathBuf, sync::atomic::AtomicBool};

#[derive(Parser)]
#[command(version, about = "OpenNovel 本地书库命令行（商业来源尚未接入）")]
struct Arguments {
    #[arg(long, default_value = ".artifacts/library.db")]
    database: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse();
    let library = Library::open(args.database)?;
    let result = match args.command {
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
            serde_json::to_value(library.run_import(&job_id, &AtomicBool::new(false))?)?
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
    Ok(())
}
