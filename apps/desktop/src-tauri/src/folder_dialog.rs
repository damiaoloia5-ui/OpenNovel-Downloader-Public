//! Windows folder picker. No shell commands or external runtime are required.
#[cfg(windows)]
mod native {
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
                FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
                SIGDN_FILESYSPATH,
            },
        },
        core::{HRESULT, HSTRING},
    };

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    pub fn run(owner: isize, initial: String, show: bool) -> Result<Option<String>, String> {
        // A dedicated thread prevents accidentally inheriting an MTA worker.
        std::thread::spawn(move || unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            let _apartment = Apartment;
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
            let flags = dialog.GetOptions().map_err(|e| e.to_string())?
                | FOS_PICKFOLDERS
                | FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_NOCHANGEDIR;
            dialog.SetOptions(flags).map_err(|e| e.to_string())?;
            dialog
                .SetTitle(&HSTRING::from("选择这本书的保存文件夹"))
                .map_err(|e| e.to_string())?;
            if std::path::Path::new(&initial).is_dir() {
                let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(&initial), None)
                    .map_err(|e| e.to_string())?;
                dialog.SetFolder(&item).map_err(|e| e.to_string())?;
            }
            if !show {
                return Ok(None);
            }
            if let Err(error) = dialog.Show(Some(HWND(owner as *mut std::ffi::c_void))) {
                return if error.code() == HRESULT::from_win32(1223) {
                    Ok(None)
                } else {
                    Err(format!("文件夹选择失败：{error}"))
                };
            }
            let item = dialog.GetResult().map_err(|e| e.to_string())?;
            let pointer = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(|e| e.to_string())?;
            let text = pointer.to_string().map_err(|e| e.to_string());
            CoTaskMemFree(Some(pointer.0.cast()));
            text.map(Some)
        })
        .join()
        .map_err(|_| "文件夹选择线程退出".to_string())?
    }
}

pub fn pick(owner: isize, initial: &str) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        native::run(owner, initial.into(), true)
    }
    #[cfg(not(windows))]
    {
        let _ = (owner, initial);
        Err("此系统请展开“输入文件夹路径”手动设置保存位置".into())
    }
}

#[cfg(windows)]
pub fn check(initial: &std::path::Path) -> Result<(), String> {
    native::run(0, initial.to_string_lossy().into_owned(), false).map(|_| ())
}
