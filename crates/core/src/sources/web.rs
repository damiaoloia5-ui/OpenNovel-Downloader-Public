use crate::{Error, Result};
use reqwest::{Client, Url};
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn interval_from(value: Option<&str>) -> Duration {
    let ms = value
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| (250..=60000).contains(v))
        .unwrap_or(500);
    Duration::from_millis(ms)
}
fn request_interval() -> Duration {
    interval_from(std::env::var("OPENNOVEL_HTTP_INTERVAL_MS").ok().as_deref())
}

pub(super) struct WebClient {
    client: Client,
    hosts: &'static [&'static str],
    pub(super) next_request: Mutex<Instant>,
    cooldown: Mutex<Option<Instant>>,
}

enum RequestFailure {
    Transient(Error),
    Stop(Error),
}

fn retry_after_seconds(headers: &reqwest::header::HeaderMap) -> u64 {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.trim().parse::<u64>().ok().or_else(|| {
                httpdate::parse_http_date(s).ok().map(|date| {
                    let delay = date
                        .duration_since(std::time::SystemTime::now())
                        .unwrap_or_default();
                    delay.as_secs() + u64::from(delay.subsec_nanos() > 0)
                })
            })
        })
        .unwrap_or(60)
        .min(86400)
}

pub(super) fn allowed(url: &Url, hosts: &[&str]) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| hosts.contains(&host))
}

impl WebClient {
    pub(super) fn new(hosts: &'static [&'static str]) -> Result<Self> {
        Self::build(hosts, false)
    }

    pub(super) fn with_obsolete_header_folding(hosts: &'static [&'static str]) -> Result<Self> {
        Self::build(hosts, true)
    }

    fn build(hosts: &'static [&'static str], allow_header_folding: bool) -> Result<Self> {
        let client = Client::builder()
            .http1_allow_obsolete_multiline_headers_in_responses(allow_header_folding)
            .user_agent(concat!(
                "OpenNovelDownloader/",
                env!("CARGO_PKG_VERSION"),
                " (public web reader)"
            ))
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 4 || !allowed(attempt.url(), hosts) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| Error::Network("无法初始化 HTTPS 客户端".into()))?;
        Ok(Self {
            client,
            hosts,
            next_request: Mutex::new(Instant::now()),
            cooldown: Mutex::new(None),
        })
    }

    pub(super) fn get(&self, url: &str, cancelled: &AtomicBool, maximum: usize) -> Result<Vec<u8>> {
        let url = Url::parse(url).map_err(|_| Error::Invalid("来源地址无效".into()))?;
        if !allowed(&url, self.hosts) {
            return Err(Error::Invalid("来源地址超出允许范围".into()));
        }
        self.request(url, cancelled, maximum)
    }

    // Release both locks before waiting so pause/cancel and other tasks stay responsive.
    async fn wait_turn(&self) -> Result<()> {
        loop {
            let now = Instant::now();
            let cooling = *self
                .cooldown
                .lock()
                .map_err(|_| Error::Network("限流调度器不可用".into()))?;
            if let Some(until) = cooling.filter(|until| *until > now) {
                let delay = until.duration_since(now);
                let seconds = delay.as_secs() + u64::from(delay.subsec_nanos() > 0);
                return Err(Error::Network(format!(
                    "来源仍在限流，请至少等待 {seconds} 秒再继续"
                )));
            }
            let wait = {
                let mut next = self
                    .next_request
                    .lock()
                    .map_err(|_| Error::Network("请求调度器不可用".into()))?;
                if *next <= now {
                    *next = now + request_interval();
                    return Ok(());
                }
                next.duration_since(now).min(Duration::from_millis(50))
            };
            tokio::time::sleep(wait).await;
        }
    }

    async fn request_once(
        &self,
        url: &Url,
        maximum: usize,
    ) -> std::result::Result<Vec<u8>, RequestFailure> {
        let mut response = self.client.get(url.clone()).send().await.map_err(|error| {
            let retryable = error.is_timeout() || error.is_connect() || error.is_body();
            let error = Error::Network("连接、TLS 或读取失败，请稍后重试".into());
            if retryable {
                RequestFailure::Transient(error)
            } else {
                RequestFailure::Stop(error)
            }
        })?;
        let status = response.status().as_u16();
        if status == 429 || (status == 503 && response.headers().contains_key("retry-after")) {
            let seconds = retry_after_seconds(response.headers());
            let until = Instant::now() + Duration::from_secs(seconds);
            {
                let mut cooldown = self
                    .cooldown
                    .lock()
                    .map_err(|_| RequestFailure::Stop(Error::Network("限流调度器不可用".into())))?;
                *cooldown = Some(cooldown.map_or(until, |old| old.max(until)));
            }
            if let Ok(mut next) = self.next_request.lock() {
                *next = (*next).max(until);
            }
            return Err(RequestFailure::Stop(Error::Network(format!(
                "来源限流，请至少等待 {seconds} 秒再重试"
            ))));
        }
        if matches!(status, 401 | 403) || response.headers().contains_key("bdturing-verify") {
            return Err(RequestFailure::Stop(Error::AccessChallenge(format!(
                "官网要求验证、登录或拒绝当前访问（HTTP {status}），已停止后续章节请求"
            ))));
        }
        if matches!(status, 408 | 502 | 503 | 504) {
            return Err(RequestFailure::Transient(Error::Network(format!(
                "官网返回临时错误 HTTP {status}"
            ))));
        }
        if !response.status().is_success() {
            return Err(RequestFailure::Stop(Error::Network(format!(
                "官网返回 HTTP {status}"
            ))));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| RequestFailure::Transient(Error::Network("正文传输中断或超时".into())))?
        {
            if bytes.len() + chunk.len() > maximum {
                return Err(RequestFailure::Stop(Error::SourceChanged(
                    "响应超过大小限制".into(),
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub(super) fn text(&self, url: &str, cancelled: &AtomicBool, maximum: usize) -> Result<String> {
        String::from_utf8(self.get(url, cancelled, maximum)?)
            .map_err(|_| Error::SourceChanged("官网响应不是 UTF-8 文本".into()))
    }

    // Callers use get(), which validates the initial URL. Tests can target a local HTTP server here.
    pub(super) fn request(
        &self,
        url: Url,
        cancelled: &AtomicBool,
        maximum: usize,
    ) -> Result<Vec<u8>> {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("HTTP runtime")
        });
        runtime.block_on(async {
            let work=async {
                self.wait_turn().await?;
                // The complete retry sequence shares one budget; retries never triple a 20s timeout.
                tokio::time::timeout(Duration::from_secs(20), async {
                    for attempt in 0..3 {
                        if attempt > 0 {
                            tokio::time::sleep(Duration::from_millis(500 * attempt)).await;
                            self.wait_turn().await?;
                        }
                        match self.request_once(&url, maximum).await {
                            Ok(bytes) => return Ok(bytes),
                            Err(RequestFailure::Stop(error)) => return Err(error),
                            Err(RequestFailure::Transient(error)) if attempt == 2 => return Err(error),
                            Err(RequestFailure::Transient(_)) => {},
                        }
                    }
                    unreachable!("bounded retry sequence")
                }).await.map_err(|_| Error::Network("读取与有限重试超过 20 秒，进度保留，请稍后继续".into()))?
            };
            tokio::pin!(work);
            loop { tokio::select! {
                result=&mut work=>return if cancelled.load(Ordering::Acquire){Err(Error::Cancelled)}else{result},
                _=tokio::time::sleep(Duration::from_millis(25))=>if cancelled.load(Ordering::Acquire){return Err(Error::Cancelled);}
            }}
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::AtomicUsize};

    fn scripted_server(
        responses: Vec<&'static [u8]>,
    ) -> (Url, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&calls);
        let worker = std::thread::spawn(move || {
            // Client initialization can be slow on Windows; retries still share the production budget.
            let mut deadline = Instant::now() + Duration::from_secs(30);
            for response in responses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => return,
                    }
                };
                deadline = Instant::now() + Duration::from_secs(8);
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                    let mut byte = [0];
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        return;
                    }
                    request.push(byte[0]);
                }
                observed.fetch_add(1, Ordering::AcqRel);
                let _ = stream.write_all(response);
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Write);
                // Finish the local response before dropping the Windows socket.
                let _ = std::io::copy(&mut stream.take(8192), &mut std::io::sink());
            }
        });
        (
            Url::parse(&format!("http://{address}/")).unwrap(),
            calls,
            worker,
        )
    }

    const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
    const BUSY: &[u8] =
        b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";

    #[test]
    fn persistent_transient_error_stops_after_three_attempts() {
        let (url, calls, worker) = scripted_server(vec![BUSY, BUSY, BUSY]);
        let client = WebClient::new(&["fanqienovel.com"]).unwrap();
        assert!(matches!(
            client.request(url, &AtomicBool::new(false), 128),
            Err(Error::Network(_))
        ));
        worker.join().unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 3);
    }

    #[test]
    fn rate_limit_does_not_retry_and_repeated_start_reports_remaining_cooldown_immediately() {
        for response in [
            &b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"[..],
            &b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"[..],
        ] {
            let (url, calls, worker) = scripted_server(vec![response]);
            let client = WebClient::new(&["fanqienovel.com"]).unwrap();
            assert!(matches!(client.request(url.clone(), &AtomicBool::new(false), 128), Err(Error::Network(_))));
            worker.join().unwrap();
            let began = Instant::now();
            let error = client.request(url, &AtomicBool::new(false), 128).unwrap_err();
            assert!(error.to_string().contains("仍在限流"));
            assert!(began.elapsed() < Duration::from_millis(200));
            assert_eq!(calls.load(Ordering::Acquire), 1);
        }
    }

    #[test]
    fn verification_header_is_not_a_successful_empty_page() {
        let (url, calls, worker) = scripted_server(vec![b"HTTP/1.1 200 OK\r\nbdturing-verify: controlled\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"]);
        let client = WebClient::new(&["fanqienovel.com"]).unwrap();
        assert!(matches!(
            client.request(url, &AtomicBool::new(false), 128),
            Err(Error::AccessChallenge(_))
        ));
        worker.join().unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn cancellation_interrupts_retry_delay_and_scheduler_wait_without_holding_the_mutex() {
        let (url, calls, worker) = scripted_server(vec![BUSY]);
        let client = WebClient::new(&["fanqienovel.com"]).unwrap();
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let request = scope.spawn(|| client.request(url, &cancel, 128));
            let deadline = Instant::now() + Duration::from_secs(2);
            while calls.load(Ordering::Acquire) == 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(calls.load(Ordering::Acquire), 1);
            cancel.store(true, Ordering::Release);
            assert!(matches!(request.join().unwrap(), Err(Error::Cancelled)));
        });
        worker.join().unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 1);
        *client.next_request.lock().unwrap() = Instant::now() + Duration::from_secs(60);
        cancel.store(false, Ordering::Release);
        let began = Instant::now();
        std::thread::scope(|scope| {
            let request = scope
                .spawn(|| client.request(Url::parse("http://127.0.0.1:1/").unwrap(), &cancel, 128));
            std::thread::sleep(Duration::from_millis(50));
            assert!(client.next_request.try_lock().is_ok());
            cancel.store(true, Ordering::Release);
            assert!(matches!(request.join().unwrap(), Err(Error::Cancelled)));
        });
        assert!(began.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn transient_gateway_error_recovers_without_restarting_the_book() {
        let (url, calls, worker) = scripted_server(vec![BUSY, OK]);
        let client = WebClient::new(&["fanqienovel.com"]).unwrap();
        let result = client.request(url, &AtomicBool::new(false), 128);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), b"ok");
        assert_eq!(calls.load(Ordering::Acquire), 2);
    }

    #[test]
    fn access_denial_is_not_a_skippable_chapter_restriction() {
        let (url, calls, worker) = scripted_server(vec![
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ]);
        let client = WebClient::new(&["fanqienovel.com"]).unwrap();
        let error = client
            .request(url, &AtomicBool::new(false), 128)
            .unwrap_err();
        worker.join().unwrap();
        assert!(
            !matches!(error, Error::Restricted(_)),
            "a site-wide refusal must stop the queue instead of skipping every chapter"
        );
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[test]
    fn http_pacing_has_bounded_fast_override_and_safe_fallback() {
        use std::time::Duration;
        assert_eq!(super::interval_from(None), Duration::from_millis(500));
        for value in ["250", "1000", "60000"] {
            assert_eq!(
                super::interval_from(Some(value)),
                Duration::from_millis(value.parse().unwrap())
            );
        }
        for value in ["0", "249", "60001", "-1", "invalid"] {
            assert_eq!(
                super::interval_from(Some(value)),
                Duration::from_millis(500)
            );
        }
    }

    use super::*;
    #[test]
    fn observed_qimao_header_folding_is_opt_in_and_does_not_disable_url_checks() {
        use std::io::{Read, Write};
        for compatible in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-Timing: 1\r\n content-security-policy: default-src self\r\n x-frame-options: SAMEORIGIN\r\n\r\nok").unwrap();
            });
            let client = if compatible {
                WebClient::with_obsolete_header_folding(&["www.qimao.com"]).unwrap()
            } else {
                WebClient::new(&["www.qimao.com"]).unwrap()
            };
            let response = client.request(
                Url::parse(&format!("http://{address}/")).unwrap(),
                &AtomicBool::new(false),
                128,
            );
            if compatible {
                assert_eq!(response.unwrap(), b"ok");
            } else {
                assert!(matches!(response, Err(Error::Network(_))));
            }
            worker.join().unwrap();
            assert!(
                client
                    .get("https://example.invalid/", &AtomicBool::new(false), 128)
                    .is_err()
            );
        }
    }
}
