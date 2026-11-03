//! Explicit, isolated installation checks. Never opens the user's library.
use crate::{auto_txt::AutoTxtService, local_engine::EngineControl};
use opennovel_core::Library;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

fn local_roundtrip(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let library = Arc::new(Library::open(root.join("sample-library.db"))?);
    let chosen = root.join("chosen-book-folder");
    std::fs::create_dir(&chosen)?;
    let destination = opennovel_core::SaveDestination {
        directory: Some(chosen.clone()),
        ..Default::default()
    };
    crate::export_destination::preflight(Some(&destination), &root.join("sample-exports"))?;
    let job = library.create_import_with_destination(
        include_str!("../../../../examples/local-book.json"),
        Some(&destination),
    )?;
    let book = library.job(&job)?.book_id;
    if !library.export_destination(&book, root)?.custom
        || library.book_summary(&book)?.saved_chapters != 0
    {
        return Err("开始保存前的目录选择未落库".into());
    }
    let service = AutoTxtService::start(Arc::clone(&library), root.join("sample-exports"))?;
    library.run_import(&job, &AtomicBool::new(false))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut original: Option<(PathBuf, String)> = None;
    loop {
        if let Some(status) = service.snapshot()?.into_iter().find(|s| s.book_id == book) {
            if status.state == "failed" {
                return Err(status.error.unwrap_or_else(|| "TXT 生成失败".into()).into());
            }
            if let Some(receipt) = status.receipt
                && status.state == "ready"
                && !receipt.partial
                && receipt.saved_chapters == 2
                && Path::new(&receipt.path).is_file()
            {
                let path = PathBuf::from(receipt.path);
                if path.parent() != Some(chosen.as_path())
                    || !path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .contains('－')
                {
                    return Err("本书导出位置或文件名核对失败".into());
                }
                let text = std::fs::read_to_string(&path)?;
                if !text.contains("清晨的阳光落在书架上") || !text.contains("雨停之后")
                {
                    return Err("原创样本 TXT 内容核对失败".into());
                }
                if let Some((expected_path, expected_text)) = &original {
                    if &path != expected_path || &text != expected_text {
                        return Err("原创样本 TXT 恢复核对失败".into());
                    }
                    return Ok(path);
                }
                // Only this newly created original fixture file is removed.
                original = Some((path.clone(), text));
                std::fs::remove_file(path)?;
            }
        }
        if Instant::now() >= deadline {
            return Err("自动 TXT 检查超时".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn word_roundtrip(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let library = Arc::new(Library::open(root.join("word-library.db"))?);
    let folder = root.join("chosen-word-folder");
    std::fs::create_dir(&folder)?;
    let destination = opennovel_core::SaveDestination {
        directory: Some(folder.clone()),
        format: Some("docx".into()),
        selection_only: false,
    };
    let job = library.create_import_with_destination(
        include_str!("../../../../examples/local-book.json"),
        Some(&destination),
    )?;
    let book = library.run_import(&job, &AtomicBool::new(false))?.book_id;
    let service = AutoTxtService::start(Arc::clone(&library), root.join("unused-word-default"))?;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = service.snapshot()?.into_iter().find(|s| s.book_id == book) {
            if status.state == "failed" {
                return Err(status.error.unwrap_or_default().into());
            }
            if let Some(receipt) = status.receipt
                && status.state == "ready"
            {
                let path = PathBuf::from(receipt.path);
                if path.extension().is_none_or(|e| e != "docx")
                    || path.parent() != Some(folder.as_path())
                    || receipt.saved_chapters != 2
                    || receipt.partial
                    || !std::fs::read(&path)?.starts_with(b"PK\x03\x04")
                {
                    return Err("原创 Word 自动导出检查失败".into());
                }
                let selected = library.export_selected(
                    &book,
                    "2",
                    &folder,
                    opennovel_core::ExportFormat::Docx,
                )?;
                if selected.saved_chapters != 1
                    || selected.total_chapters != 1
                    || selected.partial
                    || library.book_summary(&book)?.saved_chapters != 2
                {
                    return Err("Word 选章范围检查失败".into());
                }
                return Ok(path);
            }
        }
        if Instant::now() > until {
            return Err("Word 自动导出检查超时".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn run(output: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if !output.is_absolute() {
        return Err("检查输出目录必须是绝对路径".into());
    }
    std::fs::create_dir_all(output)?;
    let root = output.join(format!("check-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root)?;
    let webview = tauri::webview_version().map_err(|e| e.to_string());
    let local = local_roundtrip(&root).map_err(|e| e.to_string());
    let word = word_roundtrip(&root).map_err(|e| e.to_string());
    #[cfg(windows)]
    let folder_dialog = crate::folder_dialog::check(&root);
    #[cfg(not(windows))]
    let folder_dialog: Result<(), String> = Ok(());
    // Optional readiness only: no enabling, chapter requests, image pulls or installation.
    let engine = EngineControl::default().status();
    let passed = webview.is_ok() && local.is_ok() && word.is_ok() && folder_dialog.is_ok();
    let report = json!({
        "schema_version": 1, "version": env!("CARGO_PKG_VERSION"),
        "required_checks_passed": passed,
        "webview": {"ready": webview.is_ok(), "result": webview},
        "original_sample_txt": {"ready": local.is_ok(), "result": local},
        "original_sample_word": {"ready": word.is_ok(), "result": word},
        "selected_word_range_checked": word.is_ok(),
        "deleted_sample_txt_recovered": local.is_ok(),
        "chosen_directory_and_simple_filename_checked": local.is_ok(),
        "directory_selected_before_save_checked": local.is_ok(),
        "native_folder_dialog_initialization": folder_dialog,
        "optional_local_engine": engine,
        "user_library_opened": false,
        "real_platform_requested": false,
        "native_window_interaction_checked": false
    });
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        root.join("README.txt"),
        format!(
            "OpenNovel {} 安装检查\r\n基础检查：{}\r\n可选番茄组件：{}\r\n下一步：{}\r\n详细结果：report.json\r\n本次仅使用原创两章独立样本；未打开原书库，未请求小说平台。\r\n检查通过不代表真实窗口、全平台或 VIP 验收。\r\n",
            env!("CARGO_PKG_VERSION"),
            if passed {
                "通过"
            } else {
                "失败，请查看报告"
            },
            if report["optional_local_engine"]["ready"] == true {
                "就绪（仍默认关闭）"
            } else {
                "未就绪，普通网页功能不依赖它"
            },
            report["optional_local_engine"]["next_step"]
                .as_str()
                .unwrap_or("请查看报告")
        ),
    )?;
    Ok(passed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn installation_sample_uses_a_new_library_and_recovers_complete_txt() {
        let root = tempfile::tempdir().unwrap();
        let txt = super::local_roundtrip(root.path()).unwrap();
        assert!(txt.starts_with(root.path()));
        assert!(root.path().join("sample-library.db").is_file());
        assert!(
            std::fs::read_to_string(txt)
                .unwrap()
                .contains("第二章 归档")
        );
    }
}
