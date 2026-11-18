//! Local stand-in for the part of Cloudflare's speed test protocol Weir and
//! `@cloudflare/speedtest` use, plus `/lab/*` counters for the accuracy lab.
//! It shares no code with the engine it judges.

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::{
    body::{Body, Frame, Incoming, SizeHint},
    header::{self, HeaderMap, HeaderValue},
    server::conn::http1,
    service::service_fn,
    Method, Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::net::TcpListener;

const CHUNK: usize = 256 * 1024;
static ZEROS: [u8; CHUNK] = [0; CHUNK];
const MAX_DOWN: u64 = 1_000_000_000;
const MAX_LOG: usize = 200_000;
const WIRE_EVERY: Duration = Duration::from_millis(50);
const TRACE: &str = "fl=lab\nh=lab\nvisit_scheme=http\ncolo=LAB\nhttp=http/1.1\ntls=off\n";

type Reply = Response<BoxBody<Bytes, Infallible>>;

#[derive(Clone, Copy)]
struct Entry {
    kind: &'static str,
    start_ms: f64,
    body_end_ms: f64,
    bytes: u64,
}

#[derive(Clone, Copy)]
struct Sample {
    at_ms: f64,
    rx_bytes: u64,
    rx_packets: u64,
    tx_bytes: u64,
    tx_packets: u64,
}

#[derive(Default)]
struct State {
    down_bytes: AtomicU64,
    up_bytes: AtomicU64,
    requests: AtomicU64,
    unknown: AtomicU64,
    server_ms: AtomicU64,
    up_includes_body: AtomicBool,
    log: Mutex<Vec<Entry>>,
    wire: Mutex<Vec<Sample>>,
    static_dir: Option<PathBuf>,
}

impl State {
    fn record(&self, entry: Entry) -> Option<usize> {
        let mut log = self.log.lock().ok()?;
        if log.len() >= MAX_LOG {
            return None;
        }
        log.push(entry);
        Some(log.len() - 1)
    }

    fn finish(&self, index: Option<usize>, bytes: u64) {
        let Some(index) = index else { return };
        if let Ok(mut log) = self.log.lock() {
            if let Some(entry) = log.get_mut(index) {
                entry.body_end_ms = unix_ms();
                entry.bytes = bytes;
            }
        }
    }
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |time| time.as_secs_f64() * 1_000.0)
}

fn full(body: impl Into<Bytes>) -> BoxBody<Bytes, Infallible> {
    Full::new(body.into()).boxed()
}

fn status(code: StatusCode, text: &'static str) -> Reply {
    let mut response = Response::new(full(text));
    *response.status_mut() = code;
    response
}

fn json(text: String) -> Reply {
    let mut response = Response::new(full(text));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

/// `bytes=N` from a query string, when N is a whole number up to 1 GB.
fn query_bytes(query: Option<&str>) -> Option<u64> {
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix("bytes="))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|bytes| *bytes <= MAX_DOWN)
}

fn query_number(query: Option<&str>, name: &str) -> Option<u64> {
    let prefix = format!("{name}=");
    query?
        .split('&')
        .find_map(|pair| pair.strip_prefix(prefix.as_str()))
        .and_then(|value| value.parse::<u64>().ok())
}

fn server_timing(ms: f64) -> String {
    format!("cfSpeedEdge;dur=0.0, cfSpeedWorker;dur={ms:.3}")
}

fn upload_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// File under `root` for a `/twin/...` path, or `None` for anything that could
/// leave it.
fn static_path(root: &Path, rest: &str) -> Option<PathBuf> {
    let rest = if rest.is_empty() { "index.html" } else { rest };
    let mut path = root.to_path_buf();
    for part in rest.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") | Some("map") => "application/json",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

struct Zeros {
    remaining: u64,
    total: u64,
    state: Arc<State>,
    entry: Option<usize>,
}

impl Body for Zeros {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        if self.remaining == 0 {
            return Poll::Ready(None);
        }
        let size = self.remaining.min(CHUNK as u64);
        self.remaining -= size;
        self.state.down_bytes.fetch_add(size, Ordering::Relaxed);
        if self.remaining == 0 {
            self.state.finish(self.entry, self.total);
        }
        Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(
            &ZEROS[..size as usize],
        )))))
    }

    fn is_end_stream(&self) -> bool {
        self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}

async fn pause(state: &State) {
    let ms = state.server_ms.load(Ordering::Relaxed);
    if ms > 0 {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

async fn down(state: Arc<State>, query: Option<&str>) -> Reply {
    let started = Instant::now();
    let Some(bytes) = query_bytes(query) else {
        return status(StatusCode::BAD_REQUEST, "bytes must be 0 to 1000000000\n");
    };
    let entry = state.record(Entry {
        kind: "down",
        start_ms: unix_ms(),
        body_end_ms: 0.0,
        bytes: 0,
    });
    pause(&state).await;
    if bytes == 0 {
        state.finish(entry, 0);
    }
    let ms = started.elapsed().as_secs_f64() * 1_000.0;
    let mut response = Response::new(
        Zeros {
            remaining: bytes,
            total: bytes,
            state,
            entry,
        }
        .boxed(),
    );
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("timing-allow-origin", HeaderValue::from_static("*"));
    if let Ok(value) = HeaderValue::from_str(&server_timing(ms)) {
        headers.insert("server-timing", value);
    }
    response
}

async fn up(state: Arc<State>, request: Request<Incoming>) -> Reply {
    if upload_length(request.headers()).is_none() {
        return status(StatusCode::LENGTH_REQUIRED, "content-length required\n");
    }
    let entry = state.record(Entry {
        kind: "up",
        start_ms: unix_ms(),
        body_end_ms: 0.0,
        bytes: 0,
    });
    let mut body = request.into_body();
    let mut received = 0u64;
    let mut first: Option<Instant> = None;
    while let Some(frame) = body.frame().await {
        let Ok(frame) = frame else {
            return status(StatusCode::BAD_REQUEST, "body interrupted\n");
        };
        if let Some(data) = frame.data_ref() {
            first.get_or_insert_with(Instant::now);
            received += data.len() as u64;
            state
                .up_bytes
                .fetch_add(data.len() as u64, Ordering::Relaxed);
        }
    }
    let last = Instant::now();
    state.finish(entry, received);
    pause(&state).await;
    let from = if state.up_includes_body.load(Ordering::Relaxed) {
        first.unwrap_or(last)
    } else {
        last
    };
    let ms = from.elapsed().as_secs_f64() * 1_000.0;
    let mut response = Response::new(full(""));
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("timing-allow-origin", HeaderValue::from_static("*"));
    headers.insert("cf-meta-upload-bytes", HeaderValue::from(received));
    if let Ok(value) = HeaderValue::from_str(&server_timing(ms)) {
        headers.insert("server-timing", value);
    }
    response
}

fn stats(state: &State) -> Reply {
    json(format!(
        "{{\"down_bytes\":{},\"up_bytes\":{},\"requests\":{},\"unknown\":{}}}",
        state.down_bytes.load(Ordering::Relaxed),
        state.up_bytes.load(Ordering::Relaxed),
        state.requests.load(Ordering::Relaxed),
        state.unknown.load(Ordering::Relaxed),
    ))
}

fn requests(state: &State) -> Reply {
    let log = state.log.lock().map(|log| log.clone()).unwrap_or_default();
    let rows: Vec<String> = log
        .iter()
        .map(|entry| {
            format!(
                "{{\"kind\":\"{}\",\"unix_ms_start\":{:.3},\"unix_ms_body_end\":{:.3},\"bytes\":{}}}",
                entry.kind, entry.start_ms, entry.body_end_ms, entry.bytes
            )
        })
        .collect();
    json(format!("[{}]", rows.join(",")))
}

fn wire(state: &State) -> Reply {
    let samples = state
        .wire
        .lock()
        .map(|wire| wire.clone())
        .unwrap_or_default();
    let rows: Vec<String> = samples
        .iter()
        .map(|sample| {
            format!(
                "[{:.3},{},{},{},{}]",
                sample.at_ms,
                sample.rx_bytes,
                sample.rx_packets,
                sample.tx_bytes,
                sample.tx_packets
            )
        })
        .collect();
    json(format!(
        "{{\"columns\":[\"unix_ms\",\"rx_bytes\",\"rx_packets\",\"tx_bytes\",\"tx_packets\"],\"samples\":[{}]}}",
        rows.join(",")
    ))
}

fn reset(state: &State) -> Reply {
    state.down_bytes.store(0, Ordering::Relaxed);
    state.up_bytes.store(0, Ordering::Relaxed);
    state.requests.store(0, Ordering::Relaxed);
    state.unknown.store(0, Ordering::Relaxed);
    if let Ok(mut log) = state.log.lock() {
        log.clear();
    }
    if let Ok(mut wire) = state.wire.lock() {
        wire.clear();
    }
    status(StatusCode::OK, "ok\n")
}

fn configure(state: &State, query: Option<&str>) -> Reply {
    if let Some(ms) = query_number(query, "server_ms") {
        state.server_ms.store(ms.min(10_000), Ordering::Relaxed);
    }
    if let Some(flag) = query_number(query, "up_includes_body") {
        state.up_includes_body.store(flag == 1, Ordering::Relaxed);
    }
    json(format!(
        "{{\"server_ms\":{},\"up_includes_body\":{}}}",
        state.server_ms.load(Ordering::Relaxed),
        state.up_includes_body.load(Ordering::Relaxed)
    ))
}

async fn serve_static(state: &State, rest: &str) -> Reply {
    let Some(path) = state
        .static_dir
        .as_deref()
        .and_then(|root| static_path(root, rest))
    else {
        return status(StatusCode::NOT_FOUND, "not found\n");
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut response = Response::new(full(bytes));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type(&path)),
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(_) => status(StatusCode::NOT_FOUND, "not found\n"),
    }
}

async fn handle(state: Arc<State>, request: Request<Incoming>) -> Result<Reply, Infallible> {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().map(str::to_owned);
    let query = query.as_deref();
    let lab = path.starts_with("/lab/");
    if !lab {
        state.requests.fetch_add(1, Ordering::Relaxed);
    }
    let reply = match (&method, path.as_str()) {
        (&Method::GET, "/cdn-cgi/trace") => {
            let mut response = Response::new(full(TRACE));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
            response
        }
        (&Method::GET, "/__down") => down(state, query).await,
        (&Method::POST, "/__up") => up(state, request).await,
        (&Method::GET, "/lab/stats") => stats(&state),
        (&Method::GET, "/lab/requests") => requests(&state),
        (&Method::GET, "/lab/wire") => wire(&state),
        (&Method::POST, "/lab/reset") => reset(&state),
        (&Method::POST, "/lab/config") => configure(&state, query),
        (&Method::GET, twin) if twin.starts_with("/twin/") => {
            serve_static(&state, &twin["/twin/".len()..]).await
        }
        _ => {
            state.unknown.fetch_add(1, Ordering::Relaxed);
            status(StatusCode::NOT_FOUND, "not found\n")
        }
    };
    Ok(reply)
}

fn read_counter(iface: &str, name: &str) -> u64 {
    std::fs::read_to_string(format!("/sys/class/net/{iface}/statistics/{name}"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn sample_wire(state: Arc<State>, iface: String) {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(WIRE_EVERY);
        loop {
            every.tick().await;
            let sample = Sample {
                at_ms: unix_ms(),
                rx_bytes: read_counter(&iface, "rx_bytes"),
                rx_packets: read_counter(&iface, "rx_packets"),
                tx_bytes: read_counter(&iface, "tx_bytes"),
                tx_packets: read_counter(&iface, "tx_packets"),
            };
            if let Ok(mut wire) = state.wire.lock() {
                if wire.len() < MAX_LOG {
                    wire.push(sample);
                }
            }
        }
    });
}

struct Args {
    listen: String,
    static_dir: Option<PathBuf>,
    wire_iface: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        listen: "127.0.0.1:8080".to_owned(),
        static_dir: None,
        wire_iface: None,
    };
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || iter.next().cloned().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--listen" => parsed.listen = value()?,
            "--static" => parsed.static_dir = Some(PathBuf::from(value()?)),
            "--wire-iface" => parsed.wire_iface = Some(value()?),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(parsed)
}

async fn serve(listener: TcpListener, state: Arc<State>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let _ = stream.set_nodelay(true);
        let state = state.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| handle(state.clone(), request));
            let _ = http1::Builder::new()
                .keep_alive(true)
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("weir-lab-server: {message}");
            std::process::exit(2);
        }
    };
    let state = Arc::new(State {
        static_dir: args.static_dir,
        ..State::default()
    });
    if let Some(iface) = args.wire_iface {
        sample_wire(state.clone(), iface);
    }
    let listener = match TcpListener::bind(&args.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("weir-lab-server: cannot listen: {error}");
            std::process::exit(1);
        }
    };
    eprintln!("weir-lab-server: ready");
    serve(listener, state).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn reads_bytes_from_the_query() {
        assert_eq!(query_bytes(Some("bytes=0")), Some(0));
        assert_eq!(query_bytes(Some("r=12&bytes=25000000")), Some(25_000_000));
        assert_eq!(query_bytes(Some("bytes=1000000000")), Some(MAX_DOWN));
        assert_eq!(query_bytes(Some("bytes=1000000001")), None);
        assert_eq!(query_bytes(Some("bytes=-1")), None);
        assert_eq!(query_bytes(Some("measId=4")), None);
        assert_eq!(query_bytes(None), None);
        assert_eq!(
            query_number(Some("server_ms=30&up_includes_body=1"), "up_includes_body"),
            Some(1)
        );
    }

    #[test]
    fn server_timing_parses_like_cloudflare() {
        assert_eq!(
            server_timing(12.3456),
            "cfSpeedEdge;dur=0.0, cfSpeedWorker;dur=12.346"
        );
    }

    #[test]
    fn static_paths_stay_inside_the_root() {
        let root = Path::new("/srv/twin");
        assert_eq!(static_path(root, ""), Some(root.join("index.html")));
        assert_eq!(
            static_path(root, "assets/main.js"),
            Some(root.join("assets/main.js"))
        );
        assert_eq!(static_path(root, "../etc/passwd"), None);
        assert_eq!(static_path(root, "assets/../../x"), None);
        assert_eq!(static_path(root, "/etc/passwd"), None);
        assert_eq!(static_path(root, "a\\..\\b"), None);
        assert_eq!(content_type(Path::new("x.js")), "text/javascript");
    }

    #[test]
    fn content_length_is_read_from_headers() {
        let mut headers = HeaderMap::new();
        assert_eq!(upload_length(&headers), None);
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from_static("65536"));
        assert_eq!(upload_length(&headers), Some(65_536));
    }

    async fn exchange(port: u16, request: &[u8]) -> String {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("connect");
        stream.write_all(request).await.expect("write");
        let mut buffer = vec![0; 4096];
        let read = stream.read(&mut buffer).await.expect("read");
        String::from_utf8_lossy(&buffer[..read]).into_owned()
    }

    #[tokio::test]
    async fn speaks_the_protocol_over_localhost() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let state = Arc::new(State::default());
        tokio::spawn(serve(listener, state.clone()));

        let chunked = exchange(
            port,
            b"POST /__up HTTP/1.1\r\nhost: lab\r\ntransfer-encoding: chunked\r\n\r\n0\r\n\r\n",
        )
        .await;
        assert!(chunked.starts_with("HTTP/1.1 411"), "{chunked}");

        let trace = exchange(port, b"GET /cdn-cgi/trace HTTP/1.1\r\nhost: lab\r\n\r\n").await;
        assert!(trace.contains("colo=LAB"), "{trace}");

        let down = exchange(
            port,
            b"GET /__down?bytes=10&r=1 HTTP/1.1\r\nhost: lab\r\n\r\n",
        )
        .await;
        assert!(down.contains("content-length: 10"), "{down}");
        assert!(
            down.contains("server-timing: cfSpeedEdge;dur=0.0, cfSpeedWorker;dur="),
            "{down}"
        );

        let up = exchange(
            port,
            b"POST /__up?r=2 HTTP/1.1\r\nhost: lab\r\ncontent-length: 4\r\n\r\nabcd",
        )
        .await;
        assert!(up.contains("cf-meta-upload-bytes: 4"), "{up}");

        let missing = exchange(port, b"GET /__results HTTP/1.1\r\nhost: lab\r\n\r\n").await;
        assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
        assert_eq!(state.unknown.load(Ordering::Relaxed), 1);
        assert_eq!(state.up_bytes.load(Ordering::Relaxed), 4);
        assert_eq!(state.down_bytes.load(Ordering::Relaxed), 10);

        let log = state.log.lock().expect("log").clone();
        assert_eq!(log.len(), 2);
        assert!(log.iter().all(|entry| entry.body_end_ms >= entry.start_ms));
    }
}
