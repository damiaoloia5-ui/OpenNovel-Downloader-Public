//! A serial browser-backed adapter. Only the local main window may schedule it.
use crate::{ActiveTask, AppState, accounts::window_label};
use opennovel_core::{
    Error, ImportChapter, JobSnapshot, Result, SessionChapter,
    sources::{ChapterRef, SourceAdapter},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State};

fn navigation_interval(value: Option<u64>) -> std::result::Result<Duration, String> {
    match value.unwrap_or(3000) {
        ms @ (500 | 1000 | 3000 | 6000) => Ok(Duration::from_millis(ms)),
        _ => Err("保存节奏只支持 500、1000、3000 或 6000 毫秒".into()),
    }
}
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);

struct ResumeTicket {
    generation: String,
    revision: u64,
}

fn check_resume_page(raw: &str) -> std::result::Result<(), String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Page {
        status: String,
    }
    if raw.len() > 1024 {
        return Err("官网状态返回异常，未重新启动任务".into());
    }
    let page: Page = serde_json::from_str(raw).map_err(|_| "无法确认官网状态，未重新启动任务")?;
    match page.status.as_str() {
        "ready" => Ok(()),
        "verification" => {
            Err("官网验证仍未完成，请在原窗口手动完成滑块或验证码，再点继续；进度已保留".into())
        }
        "loading" => Err("官网页面仍在加载，请等待加载完成后再点继续；进度已保留".into()),
        _ => Err("无法确认官网状态，请本人核对官网页面后重试；进度已保留".into()),
    }
}

async fn prepare_resume(
    id: &str,
    app: &tauri::AppHandle,
    state: &AppState,
) -> std::result::Result<ResumeTicket, String> {
    let account = state.library.account(id).map_err(|e| e.to_string())?;
    opennovel_core::session_source(&account.platform).map_err(|e| e.to_string())?;
    let window = app
        .get_webview_window(&window_label(id))
        .ok_or("官网窗口已关闭，请重新登录后继续；保存进度保留")?;
    let (generation, revision, reading) = {
        let sessions = state.sessions.lock().map_err(|_| "账户会话不可用")?;
        let session = sessions.get(id).ok_or("官网会话已关闭，请重新登录后继续")?;
        (
            session.generation.clone(),
            Arc::clone(&session.revision),
            Arc::clone(&session.reading),
        )
    };
    if reading.swap(true, Ordering::AcqRel) {
        return Err("原任务仍在执行或退出，请等待完全停止后再继续".into());
    }
    let _lease = PageLease(Some(reading));
    let before = window.url().map_err(|_| "无法确认官网当前地址")?;
    if !opennovel_core::account_navigation_allowed(&account.platform, &before) {
        return Err("当前页面不是受支持的平台官网页面，请重新登录后继续".into());
    }
    if before.host_str() == Some("passport.qidian.com") {
        let _ = crate::accounts::show_existing(&window);
        return Err("官网要求重新登录，请在原官网窗口登录完成后继续；保存进度保留".into());
    }
    let epoch = revision.load(Ordering::Acquire);
    let (sender, receiver) = std::sync::mpsc::channel();
    window
        .eval_with_callback(include_str!("resume_page.js"), move |raw| {
            let _ = sender.send(raw);
        })
        .map_err(|_| "无法检查官网状态，请稍后重试")?;
    let raw =
        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(8)))
            .await
            .map_err(|_| "官网状态检查中断，未重新启动任务")?
            .map_err(|_| "官网状态检查超时，请核对原窗口后重试")?;
    if state
        .sessions
        .lock()
        .map_err(|_| "账户会话不可用")?
        .get(id)
        .is_none_or(|s| s.generation != generation)
    {
        return Err("官网会话已变化，请重新登录后继续；保存进度保留".into());
    }
    if revision.load(Ordering::Acquire) != epoch
        || window.url().map_err(|_| "无法确认官网地址")? != before
    {
        return Err("官网页面仍在变化，请等登录或验证页面加载完成后再继续；进度已保留".into());
    }
    if let Err(error) = check_resume_page(&raw) {
        let _ = crate::accounts::show_existing(&window);
        return Err(error);
    }
    Ok(ResumeTicket {
        generation,
        revision: epoch,
    })
}

struct AccountReader {
    platform: String,
    source: String,
    interval: Duration,
    app: tauri::AppHandle,
    account: String,
    generation: String,
    revision: Arc<AtomicU64>,
    last_navigation: Arc<Mutex<Option<Instant>>>,
}
/// WebView2 starts at about:blank before its first official navigation commits.
/// This is a loading state, never a page whose content may be captured.
fn classify_page_url(platform: &str, url: &tauri::Url) -> Result<Option<String>> {
    if url.as_str() == "about:blank" {
        return Ok(None);
    }
    if !opennovel_core::account_navigation_allowed(platform, url) {
        return Err(Error::Restricted(
            "页面已离开当前平台官网，自动保存已停止".into(),
        ));
    }
    if url.host_str() == Some("passport.qidian.com") {
        return Err(Error::Restricted(
            "官网要求重新登录，请本人登录后继续".into(),
        ));
    }
    opennovel_core::clean_session_url(platform, url.as_str()).map(Some)
}
struct RunLease {
    reading: Arc<AtomicBool>,
    registrations: Arc<Mutex<HashMap<String, ActiveTask>>>,
    job_id: String,
}
impl Drop for RunLease {
    fn drop(&mut self) {
        if let Ok(mut tasks) = self.registrations.lock() {
            tasks.remove(&self.job_id);
        }
        self.reading.store(false, Ordering::Release);
    }
}
struct PageLease(Option<Arc<AtomicBool>>);
impl Drop for PageLease {
    fn drop(&mut self) {
        if let Some(flag) = &self.0 {
            flag.store(false, Ordering::Release);
        }
    }
}

fn wait(duration: Duration, cancelled: &AtomicBool) -> Result<()> {
    let until = Instant::now() + duration;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

/// None means DOM is still loading. Errors never include page text, tokens, or URLs.
fn decode_page_for(
    raw: &str,
    expected: &str,
    chapter: &ChapterRef,
    platform: &str,
    book: &str,
) -> Result<Option<ImportChapter>> {
    if raw.len() > opennovel_core::MAX_CAPTURE_BYTES {
        return Err(Error::SourceChanged(
            "页面超过读取上限，自动保存已停止".into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| Error::SourceChanged("页面返回无效，自动保存已停止".into()))?;
    if let Some(code) = value.get("error").and_then(|v| v.as_str()) {
        return match code {
            "loading" | "reader" => Ok(None),
            "restricted" | "protected" => Err(Error::Restricted(
                "页面要求订阅或含保护内容，自动保存已停止；请自行核对阅读权限".into(),
            )),
            "verification" => Err(Error::Restricted(
                "官网要求验证，自动保存已停止；请本人处理后继续".into(),
            )),
            "word_count" => Err(Error::SourceChanged(
                "官网未提供可核验的章节字数，无法确认正文完整性；本章未保存".into(),
            )),
            "identity" => Err(Error::SourceChanged(
                "官网章节身份无法核对；请核对当前作品及登录状态，本章未保存".into(),
            )),
            _ => Err(Error::SourceChanged(
                "页面结构无法识别，自动保存已停止".into(),
            )),
        };
    }
    let captured = SessionChapter::parse_for(raw, expected, platform, book, &chapter.id)?;
    if captured.chapter.id != chapter.id
        || captured
            .chapter
            .title
            .split_whitespace()
            .collect::<String>()
            != chapter.title.split_whitespace().collect::<String>()
    {
        return Err(Error::SourceChanged(
            "章节身份或标题与已确认目录不一致，自动保存已停止".into(),
        ));
    }
    Ok(Some(captured.chapter))
}

#[cfg(test)]
fn decode_page(raw: &str, expected: &str, chapter: &ChapterRef) -> Result<Option<ImportChapter>> {
    decode_page_for(raw, expected, chapter, "qidian", "")
}
struct StableCapture<'a> {
    platform: &'a str,
    book: &'a str,
    expected: &'a str,
    chapter: &'a ChapterRef,
    previous: Option<(String, ImportChapter)>,
}
impl<'a> StableCapture<'a> {
    #[cfg(test)]
    fn new(expected: &'a str, chapter: &'a ChapterRef) -> Self {
        Self::for_platform(expected, chapter, "qidian", "")
    }
    fn for_platform(
        expected: &'a str,
        chapter: &'a ChapterRef,
        platform: &'a str,
        book: &'a str,
    ) -> Self {
        Self {
            platform,
            book,
            expected,
            chapter,
            previous: None,
        }
    }
    fn observe(&mut self, raw: String) -> Result<Option<ImportChapter>> {
        // The caller rechecks URL and navigation epoch on every observation.
        // Equal bytes have already passed full validation for this exact target.
        if self
            .previous
            .as_ref()
            .is_some_and(|(prior, _)| prior == &raw)
        {
            return Ok(self.previous.take().map(|(_, content)| content));
        }
        self.previous = None;
        if let Some(content) =
            decode_page_for(&raw, self.expected, self.chapter, self.platform, self.book)?
        {
            self.previous = Some((raw, content));
        }
        Ok(None)
    }
}

impl AccountReader {
    fn window(&self, cancelled: &AtomicBool) -> Result<tauri::WebviewWindow> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        let state = self.app.state::<AppState>();
        if state
            .sessions
            .lock()
            .map_err(|_| Error::Conflict("账户会话不可用".into()))?
            .get(&self.account)
            .is_none_or(|s| s.generation != self.generation)
        {
            return Err(Error::Network(
                "账户窗口已关闭或重新打开；请登录后在平台账户继续任务".into(),
            ));
        }
        self.app
            .get_webview_window(&window_label(&self.account))
            .ok_or_else(|| Error::Network("官网窗口已关闭，自动保存已停止".into()))
    }
    fn current_url(&self, window: &tauri::WebviewWindow) -> Result<String> {
        let url = window
            .url()
            .map_err(|_| Error::Network("无法确认官网地址".into()))?;
        classify_page_url(&self.platform, &url)?
            .ok_or_else(|| Error::Network("官网页面尚未加载，保存进度保留".into()))
    }
}
impl SourceAdapter for AccountReader {
    fn source_id(&self) -> &str {
        &self.source
    }
    fn fetch_chapter(
        &self,
        book: &str,
        chapter: &ChapterRef,
        cancelled: &AtomicBool,
    ) -> Result<ImportChapter> {
        if self.platform == "jinjiang" && chapter.title.contains("[锁定]") {
            return Err(Error::Restricted("官网目录标记章节锁定，进度保留".into()));
        }
        let expected = opennovel_core::session_chapter_url(&self.platform, book, &chapter.id)?;
        let expected = if self.platform == "jinjiang" && !chapter.restricted {
            expected.replace(
                "https://my.jjwxc.net/onebook_vip.php?",
                "https://www.jjwxc.net/onebook.php?",
            )
        } else {
            expected
        };
        let delay = self
            .last_navigation
            .lock()
            .map_err(|_| Error::Conflict("页面调度不可用".into()))?
            .map(|t| self.interval.saturating_sub(t.elapsed()))
            .unwrap_or_default();
        wait(delay, cancelled)?;
        let initial_deadline = Instant::now() + PAGE_TIMEOUT;
        let (window, current) = loop {
            let window = self.window(cancelled)?;
            let url = window
                .url()
                .map_err(|_| Error::Network("无法确认官网地址".into()))?;
            if let Some(current) = classify_page_url(&self.platform, &url)? {
                break (window, current);
            }
            if Instant::now() >= initial_deadline {
                return Err(Error::Network(
                    "官网窗口加载超时，请核对网络后继续；进度已保留".into(),
                ));
            }
            wait(Duration::from_millis(100), cancelled)?;
        };
        // Reading normal pages must never turn a login page into an unattended login flow.
        if current != expected {
            window
                .navigate(
                    expected
                        .parse()
                        .map_err(|_| Error::Invalid("章节地址无效".into()))?,
                )
                .map_err(|_| Error::Network("官网章节导航失败".into()))?;
            *self
                .last_navigation
                .lock()
                .map_err(|_| Error::Conflict("页面调度不可用".into()))? = Some(Instant::now());
        }
        let deadline = Instant::now() + PAGE_TIMEOUT;
        let mut arrived = false;
        let mut stable = StableCapture::for_platform(&expected, chapter, &self.platform, book);
        let script = if self.platform == "qidian" {
            include_str!("capture_page.js").replace("__MODE__", "chapter")
        } else {
            include_str!("session_page.js")
                .replace("__PLATFORM__", &self.platform)
                .replace("__BOOK__", book)
        };
        while Instant::now() < deadline {
            let window = self.window(cancelled)?;
            let current = self.current_url(&window)?;
            if current != expected {
                if arrived {
                    return Err(Error::Network(
                        "官网页面被切换，自动保存已停止；请不要操作自动保存窗口".into(),
                    ));
                }
                wait(Duration::from_millis(100), cancelled)?;
                continue;
            }
            arrived = true;
            let epoch = self.revision.load(Ordering::Acquire);
            let (sender, receiver) = std::sync::mpsc::channel();
            window
                .eval_with_callback(script.clone(), move |raw| {
                    let _ = sender.send(raw);
                })
                .map_err(|_| Error::Network("官网页面读取失败".into()))?;
            let raw = loop {
                self.window(cancelled)?;
                if Instant::now() >= deadline {
                    return Err(Error::Network(
                        "页面读取超时，已停止；请核对登录或验证提示后继续".into(),
                    ));
                }
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(raw) => break raw,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err(Error::Network("页面读取中断".into())),
                }
            };
            if self.current_url(&window)? != expected
                || self.revision.load(Ordering::Acquire) != epoch
            {
                return Err(Error::Network(
                    "读取期间页面导航发生变化，自动保存已停止".into(),
                ));
            }
            if let Some(content) = stable.observe(raw)? {
                return Ok(content);
            }
            wait(Duration::from_millis(400), cancelled)?;
        }
        Err(Error::Network(
            "未获得稳定完整正文；可能仍在加载、登录或验证，自动保存已停止".into(),
        ))
    }
}

#[tauri::command]
pub async fn account_download_jobs(
    book_id: String,
    state: State<'_, AppState>,
) -> std::result::Result<Vec<JobSnapshot>, String> {
    let library = Arc::clone(&state.library);
    tauri::async_runtime::spawn_blocking(move || library.session_download_jobs(&book_id))
        .await
        .map_err(|_| "任务读取中断")?
        .map_err(|e| e.to_string())
}
#[tauri::command]
// Keep the existing IPC argument names; app and state are injected by Tauri.
#[allow(clippy::too_many_arguments)]
pub async fn start_account_download(
    id: String,
    no_auto_purchase: bool,
    interval_ms: Option<u64>,
    book_id: String,
    selection: Option<String>,
    destination: Option<opennovel_core::SaveDestination>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    if !no_auto_purchase {
        return Err("开始前请本人确认官网已关闭自动订阅".into());
    }
    launch(
        id,
        Some((book_id, selection, destination)),
        None,
        navigation_interval(interval_ms)?,
        None,
        app,
        &state,
    )
}
#[tauri::command]
pub async fn resume_account_download(
    id: String,
    no_auto_purchase: bool,
    interval_ms: Option<u64>,
    job_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> std::result::Result<String, String> {
    if !no_auto_purchase {
        return Err("继续前请本人确认官网已关闭自动订阅".into());
    }
    let interval = navigation_interval(interval_ms)?;
    let ticket = prepare_resume(&id, &app, &state).await?;
    launch(id, None, Some(job_id), interval, Some(ticket), app, &state)
}
fn launch(
    id: String,
    new: Option<(
        String,
        Option<String>,
        Option<opennovel_core::SaveDestination>,
    )>,
    resume: Option<String>,
    interval: Duration,
    ticket: Option<ResumeTicket>,
    app: tauri::AppHandle,
    state: &AppState,
) -> std::result::Result<String, String> {
    let account = state.library.account(&id).map_err(|e| e.to_string())?;
    opennovel_core::session_source(&account.platform).map_err(|e| e.to_string())?;
    if app.get_webview_window(&window_label(&id)).is_none() {
        return Err("请先打开本账户官网窗口并自行登录".into());
    }
    let (generation, revision, reading, last_navigation) = {
        let sessions = state.sessions.lock().map_err(|_| "账户会话不可用")?;
        let s = sessions.get(&id).ok_or("官网会话已关闭")?;
        if ticket.as_ref().is_some_and(|t| {
            t.generation != s.generation || t.revision != s.revision.load(Ordering::Acquire)
        }) {
            return Err("官网会话或页面已变化，请完成登录或验证后再继续；进度已保留".into());
        }
        (
            s.generation.clone(),
            Arc::clone(&s.revision),
            Arc::clone(&s.reading),
            Arc::clone(&s.last_automatic_navigation),
        )
    };
    if reading.swap(true, Ordering::AcqRel) {
        return Err("该账户正在读取或自动保存，请等待或暂停".into());
    }
    let mut lease = PageLease(Some(Arc::clone(&reading)));
    let mut registrations = state.cancellations.lock().map_err(|_| "任务管理器不可用")?;
    if registrations.values().any(|r| r.account_id.is_some()) {
        return Err("已有官网自动任务执行或停止中，请稍候".into());
    }
    let expected_source =
        opennovel_core::session_source(&account.platform).map_err(|e| e.to_string())?;
    let requested_book = if let Some((book, _, _)) = &new {
        book.clone()
    } else {
        state
            .library
            .job(resume.as_deref().ok_or("缺少任务")?)
            .map_err(|e| e.to_string())?
            .book_id
    };
    if state
        .library
        .session_catalog(&requested_book)
        .map_err(|e| e.to_string())?
        .source
        != expected_source
    {
        return Err("当前账户平台与这本书不一致，未启动任务".into());
    }
    let job_id = if let Some((book, selection, destination)) = new {
        if registrations.values().any(|r| r.book_id == book) {
            return Err("该书有任务正在停止中，请稍候".into());
        }
        crate::export_destination::preflight(destination.as_ref(), &state.exports)
            .map_err(|e| e.to_string())?;
        state
            .library
            .create_session_download_with_destination(
                &book,
                selection.as_deref(),
                destination.as_ref(),
            )
            .map_err(|e| e.to_string())?
    } else {
        let job = resume.ok_or("缺少任务")?;
        if !state
            .library
            .is_session_download(&job)
            .map_err(|e| e.to_string())?
        {
            return Err("这不是官网自动保存任务".into());
        }
        let snapshot = state.library.job(&job).map_err(|e| e.to_string())?;
        if snapshot.state == "completed"
            || registrations
                .values()
                .any(|r| r.book_id == snapshot.book_id)
        {
            return Err("该任务已完成或仍在执行，请稍候".into());
        }
        job
    };
    let book_id = state
        .library
        .job(&job_id)
        .map_err(|e| e.to_string())?
        .book_id;
    let token = Arc::new(AtomicBool::new(false));
    registrations.insert(
        job_id.clone(),
        ActiveTask {
            book_id,
            token: Arc::clone(&token),
            account_id: Some(id.clone()),
        },
    );
    drop(registrations);
    lease.0 = None; // The worker's RunLease now owns the reading flag.
    let lease = RunLease {
        reading,
        registrations: Arc::clone(&state.cancellations),
        job_id: job_id.clone(),
    };
    let reader = AccountReader {
        platform: account.platform,
        source: expected_source.into(),
        interval,
        app,
        account: id,
        generation,
        revision,
        last_navigation,
    };
    let library = Arc::clone(&state.library);
    let requested = job_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        if let Err(e) = library.run_job(&requested, &reader, &token) {
            let _ = library.record_worker_failure(&requested, &e.to_string());
        }
    });
    Ok(job_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_blank_waits_but_login_and_foreign_pages_still_stop() {
        assert_eq!(
            classify_page_url("faloo", &"about:blank".parse().unwrap()).unwrap(),
            None
        );
        assert_eq!(
            classify_page_url(
                "faloo",
                &"https://b.faloo.com/1538594.html".parse().unwrap()
            )
            .unwrap(),
            Some("https://b.faloo.com/1538594.html".into())
        );
        for address in [
            "about:blank?x=1",
            "https://faloo.com.evil.test/",
            "http://b.faloo.com/1538594.html",
            "chrome-error://chromewebdata/",
        ] {
            assert!(matches!(
                classify_page_url("faloo", &address.parse().unwrap()),
                Err(Error::Restricted(_))
            ));
        }
        assert!(matches!(
            classify_page_url("qidian", &"https://passport.qidian.com/".parse().unwrap()),
            Err(Error::Restricted(_))
        ));
    }
    #[test]
    fn resume_checks_only_accept_a_small_ready_status() {
        assert!(check_resume_page(r#"{"status":"ready"}"#).is_ok());
        assert!(
            check_resume_page(r#"{"status":"verification"}"#)
                .unwrap_err()
                .contains("验证仍未完成")
        );
        assert!(
            check_resume_page(r#"{"status":"loading"}"#)
                .unwrap_err()
                .contains("仍在加载")
        );
        for raw in [
            r#"{"status":"unknown"}"#,
            r#"{"status":"ready","cookie":"not accepted"}"#,
            "null",
            "bad json",
        ] {
            assert!(check_resume_page(raw).is_err());
        }
        assert!(check_resume_page(&" ".repeat(1025)).is_err());
    }
    fn original_page() -> (String, ChapterRef) {
        let paragraph = "原创稳定正文".repeat(1000);
        let raw=serde_json::json!({"url":"https://www.qidian.com/chapter/1/2/","title":"原创章节","declared_words":paragraph.chars().count(),"paragraphs":[paragraph]}).to_string();
        (
            raw,
            ChapterRef {
                id: "2".into(),
                title: "原创章节".into(),
                restricted: false,
            },
        )
    }
    #[test]
    fn stable_capture_needs_two_equal_complete_reads_and_resets_on_loading_or_gate() {
        let (raw, chapter) = original_page();
        let url = "https://www.qidian.com/chapter/1/2/";
        let mut stable = StableCapture::new(url, &chapter);
        assert!(stable.observe(raw.clone()).unwrap().is_none());
        assert!(
            stable
                .observe(r#"{"error":"loading"}"#.into())
                .unwrap()
                .is_none()
        );
        assert!(stable.observe(raw.clone()).unwrap().is_none());
        assert!(stable.observe(raw.clone()).unwrap().is_some());
        assert!(stable.observe(raw.clone()).unwrap().is_none());
        assert!(matches!(
            stable.observe(r#"{"error":"restricted"}"#.into()),
            Err(Error::Restricted(_))
        ));
        assert!(stable.observe(raw.clone()).unwrap().is_none());
        let changed = raw.replace("原创稳定正文", "原创变化正文");
        assert!(stable.observe(changed.clone()).unwrap().is_none());
        assert!(stable.observe(changed).unwrap().is_some());
        assert!(
            StableCapture::new("https://www.qidian.com/chapter/1/3/", &chapter)
                .observe(raw)
                .is_err()
        );
    }
    #[test]
    fn stable_capture_never_reuses_prior_body_after_verification_or_oversized_input() {
        let (raw, chapter) = original_page();
        let mut stable = StableCapture::new("https://www.qidian.com/chapter/1/2/", &chapter);
        stable.observe(raw.clone()).unwrap();
        assert!(
            stable
                .observe("x".repeat(opennovel_core::MAX_CAPTURE_BYTES + 1))
                .is_err()
        );
        assert!(stable.observe(raw.clone()).unwrap().is_none());
        assert!(matches!(
            stable.observe(r#"{"error":"verification"}"#.into()),
            Err(Error::Restricted(_))
        ));
        assert!(stable.observe(raw).unwrap().is_none());
    }
    #[test]
    #[ignore = "explicit controlled decoder CPU measurement"]
    fn stable_decode_benchmark() {
        let (raw, chapter) = original_page();
        let url = "https://www.qidian.com/chapter/1/2/";
        let start = Instant::now();
        for _ in 0..500 {
            let a = raw.clone();
            std::hint::black_box(decode_page(&a, url, &chapter).unwrap());
            let b = raw.clone();
            std::hint::black_box(decode_page(&b, url, &chapter).unwrap());
        }
        let duplicate = start.elapsed();
        let start = Instant::now();
        for _ in 0..500 {
            let mut stable = StableCapture::new(url, &chapter);
            assert!(stable.observe(raw.clone()).unwrap().is_none());
            std::hint::black_box(stable.observe(raw.clone()).unwrap());
        }
        println!(
            "500 original chapters x 2 reads: duplicate_decode_ms={:.3}; reuse_validated_ms={:.3}",
            duplicate.as_secs_f64() * 1000.,
            start.elapsed().as_secs_f64() * 1000.
        );
    }
    #[test]
    fn bounded_speed_modes_keep_legacy_default() {
        assert_eq!(navigation_interval(None).unwrap(), Duration::from_secs(3));
        assert_eq!(
            navigation_interval(Some(1000)).unwrap(),
            Duration::from_secs(1)
        );
        assert_eq!(
            navigation_interval(Some(500)).unwrap(),
            Duration::from_millis(500)
        );
        assert_eq!(
            navigation_interval(Some(6000)).unwrap(),
            Duration::from_secs(6)
        );
        for value in [0, 1, 499, 501, 1001, u64::MAX] {
            assert!(navigation_interval(Some(value)).is_err());
        }
    }
    #[test]
    fn cancellation_interrupts_wait_and_loading_never_returns_text() {
        assert!(matches!(
            wait(Duration::from_secs(3), &AtomicBool::new(true)),
            Err(Error::Cancelled)
        ));
        let chapter = ChapterRef {
            id: "2".into(),
            title: "第一章".into(),
            restricted: false,
        };
        let url = "https://www.qidian.com/chapter/1/2/";
        assert!(
            decode_page(r#"{"error":"loading"}"#, url, &chapter)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            decode_page(r#"{"error":"restricted"}"#, url, &chapter),
            Err(Error::Restricted(_))
        ));
        assert!(decode_page(r#"{"cookie":"not accepted"}"#, url, &chapter).is_err());
    }
}
