pub mod cloudflare;

use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const USER_AGENT: &str = concat!(
    "Weir/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Abulkosim/weir)"
);

/// Where a test ran. Holds only the operator and the data-centre code: never
/// the user's address or location, which Cloudflare also sends back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub provider: &'static str,
    pub colo: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendError {
    Offline,
    Refused {
        status: u16,
        retry_after: Option<Duration>,
    },
    ServerError {
        status: u16,
    },
    Interrupted,
    Timeout,
}

impl BackendError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Offline => "No internet",
            Self::Refused { .. } => "Cloudflare refused the test",
            Self::ServerError { .. } => "Cloudflare server error",
            Self::Interrupted => "Connection kept dropping",
            Self::Timeout => "Connection stalled",
        }
    }
}

/// A finished upload request, timed from the client side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sent {
    pub bytes: u64,
    pub duration_ms: f64,
}

/// One HTTP/1.1 connection, reused for every request of a stream.
pub trait Connection: Send + Sync {
    /// Round trip of an empty request in ms, with server time removed.
    fn probe(&self) -> impl Future<Output = Result<f64, BackendError>> + Send;
    /// Download `bytes`, adding each received chunk to `counter`.
    fn download(
        &self,
        bytes: u64,
        counter: Arc<AtomicU64>,
    ) -> impl Future<Output = Result<u64, BackendError>> + Send;
    /// Upload `bytes`, adding each chunk handed to the socket to `counter`.
    fn upload(
        &self,
        bytes: u64,
        counter: Arc<AtomicU64>,
    ) -> impl Future<Output = Result<Sent, BackendError>> + Send;
}

pub trait Backend: Send + Sync {
    type Connection: Connection + 'static;
    fn server(&self) -> impl Future<Output = Result<Server, BackendError>> + Send;
    fn connect(&self) -> Result<Self::Connection, BackendError>;
}

/// Query value that keeps caches and proxies from serving a stored body.
pub fn cache_buster() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.subsec_nanos() as u64);
    nanos ^ (NEXT.fetch_add(1, Ordering::Relaxed) << 32)
}
