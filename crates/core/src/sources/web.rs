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
        })
    }

    pub(super) fn get(&self, url: &str, cancelled: &AtomicBool, maximum: usize) -> Result<Vec<u8>> {
        let url = Url::parse(url).map_err(|_| Error::Invalid("来源地址无效".into()))?;
        if !allowed(&url, self.hosts) {
            return Err(Error::Invalid("来源地址超出允许范围".into()));
        }
        {
            let mut next = self
                .next_request
                .lock()
                .map_err(|_| Error::Network("请求调度器不可用".into()))?;
            while Instant::now() < *next {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Error::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if cancelled.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            *next = Instant::now() + request_interval();
        }
        self.request(url, cancelled, maximum)
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
                let mut response=self.client.get(url).send().await.map_err(|_|Error::Network("连接、TLS 或读取超时，请稍后重试".into()))?;
                let status=response.status();
                if status.as_u16()==429 {
                    let seconds=response.headers().get("retry-after").and_then(|v|v.to_str().ok()).and_then(|s|s.parse::<u64>().ok().or_else(||httpdate::parse_http_date(s).ok().map(|date| {
                        let delay=date.duration_since(std::time::SystemTime::now()).unwrap_or_default();
                        delay.as_secs()+u64::from(delay.subsec_nanos()>0)
                    }))).unwrap_or(60).min(86400);
                    if let Ok(mut next)=self.next_request.lock(){*next=Instant::now()+Duration::from_secs(seconds);}
                    return Err(Error::Network(format!("来源限流，请至少等待 {seconds} 秒再重试")));
                }
                if matches!(status.as_u16(),401|403) { return Err(Error::Restricted("官网拒绝访问或需要验证，不进行绕过".into())); }
                if !status.is_success() { return Err(Error::Network(format!("官网返回 HTTP {}",status.as_u16()))); }
                let mut bytes=Vec::new();
                while let Some(chunk)=response.chunk().await.map_err(|_|Error::Network("正文传输中断或超时".into()))? {
                    if bytes.len()+chunk.len()>maximum { return Err(Error::SourceChanged("响应超过大小限制".into())); }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(bytes)
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
