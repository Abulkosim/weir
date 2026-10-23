use super::{cache_buster, Backend, BackendError, Connection, Sent, Server, USER_AGENT};
use crate::core::stats::{probe_rtt, server_time_ms};
use bytes::Bytes;
use reqwest::{header, Client, Response, StatusCode};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const BASE: &str = "https://speed.cloudflare.com";
const TRACE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30);
const CHUNK: usize = 64 * 1024;
static ZEROS: [u8; CHUNK] = [0; CHUNK];

pub struct Cloudflare {
    base: String,
}

impl Default for Cloudflare {
    fn default() -> Self {
        Self::with_base(BASE)
    }
}

impl Cloudflare {
    pub fn with_base(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
        }
    }
}

fn client() -> Result<Client, BackendError> {
    Client::builder()
        .http1_only()
        .user_agent(USER_AGENT)
        .pool_max_idle_per_host(1)
        .connect_timeout(TRACE_TIMEOUT)
        .tcp_nodelay(true)
        .build()
        .map_err(|_| BackendError::Offline)
}

fn transport(error: reqwest::Error) -> BackendError {
    if error.is_connect() {
        BackendError::Offline
    } else if error.is_timeout() {
        BackendError::Timeout
    } else {
        BackendError::Interrupted
    }
}

fn retry_after(response: &Response) -> Option<Duration> {
    response
        .headers()
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn checked(response: Response) -> Result<Response, BackendError> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else if status.is_server_error() {
        Err(BackendError::ServerError {
            status: status.as_u16(),
        })
    } else {
        Err(BackendError::Refused {
            status: status.as_u16(),
            retry_after: (status == StatusCode::TOO_MANY_REQUESTS)
                .then(|| retry_after(&response))
                .flatten(),
        })
    }
}

fn server_time(response: &Response) -> Option<f64> {
    let joined = response
        .headers()
        .get_all("server-timing")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect::<Vec<_>>()
        .join(",");
    server_time_ms(&joined)
}

pub fn parse_colo(trace: &str) -> Option<String> {
    trace
        .lines()
        .find_map(|line| line.strip_prefix("colo="))
        .map(str::trim)
        .filter(|code| code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_uppercase()))
        .map(str::to_owned)
}

impl Backend for Cloudflare {
    type Connection = CloudflareConnection;

    async fn server(&self) -> Result<Server, BackendError> {
        let response = client()?
            .get(format!("{}/cdn-cgi/trace", self.base))
            .timeout(TRACE_TIMEOUT)
            .send()
            .await
            .map_err(|_| BackendError::Offline)?;
        let body = checked(response)?
            .text()
            .await
            .map_err(|_| BackendError::Offline)?;
        Ok(Server {
            provider: "Cloudflare",
            colo: parse_colo(&body),
        })
    }

    fn connect(&self) -> Result<CloudflareConnection, BackendError> {
        Ok(CloudflareConnection {
            client: client()?,
            base: self.base.clone(),
        })
    }
}

pub struct CloudflareConnection {
    client: Client,
    base: String,
}

impl Connection for CloudflareConnection {
    async fn probe(&self) -> Result<f64, BackendError> {
        let started = Instant::now();
        let response = self
            .client
            .get(format!("{}/__down?bytes=0&r={}", self.base, cache_buster()))
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(transport)?;
        let ttfb = started.elapsed().as_secs_f64() * 1_000.0;
        let response = checked(response)?;
        let server = server_time(&response);
        response.bytes().await.map_err(transport)?;
        Ok(probe_rtt(ttfb, server))
    }

    async fn download(&self, bytes: u64, counter: Arc<AtomicU64>) -> Result<u64, BackendError> {
        let mut response = checked(
            self.client
                .get(format!(
                    "{}/__down?bytes={bytes}&r={}",
                    self.base,
                    cache_buster()
                ))
                .timeout(TRANSFER_TIMEOUT)
                .send()
                .await
                .map_err(transport)?,
        )?;
        let mut received = 0;
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            received += chunk.len() as u64;
            counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        }
        if received < bytes {
            return Err(BackendError::Interrupted);
        }
        Ok(received)
    }

    async fn upload(&self, bytes: u64, counter: Arc<AtomicU64>) -> Result<Sent, BackendError> {
        let chunks = (0..bytes.div_ceil(CHUNK as u64)).map(move |index| {
            let size = (bytes - index * CHUNK as u64).min(CHUNK as u64) as usize;
            counter.fetch_add(size as u64, Ordering::Relaxed);
            Ok::<_, std::io::Error>(Bytes::from_static(&ZEROS[..size]))
        });
        let started = Instant::now();
        let response = self
            .client
            .post(format!("{}/__up?r={}", self.base, cache_buster()))
            .header(header::CONTENT_LENGTH, bytes)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(reqwest::Body::wrap_stream(futures_util::stream::iter(
                chunks,
            )))
            .timeout(TRANSFER_TIMEOUT)
            .send()
            .await
            .map_err(transport)?;
        let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
        let response = checked(response)?;
        let server = server_time(&response).unwrap_or(0.0);
        response.bytes().await.map_err(transport)?;
        Ok(Sent {
            bytes,
            duration_ms: (elapsed - server).max(elapsed * 0.5),
        })
    }
}

#[cfg(test)]
mod tests;
