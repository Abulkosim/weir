use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use wiremock::{
    matchers::{header_regex, method, path, query_param},
    Mock, MockServer, Request, ResponseTemplate,
};

/// What Cloudflare attaches to every response about the person testing.
fn identifying(template: ResponseTemplate) -> ResponseTemplate {
    template
        .insert_header("cf-meta-ip", "203.0.113.9")
        .insert_header("cf-meta-city", "Exampletown")
        .insert_header("cf-meta-asn", "64500")
        .insert_header("cf-meta-latitude", "1.2345")
        .insert_header("cf-meta-colo", "FRA")
}

async fn server() -> (MockServer, Cloudflare) {
    let mock = MockServer::start().await;
    let backend = Cloudflare::with_base(&mock.uri());
    (mock, backend)
}

#[tokio::test]
async fn server_reads_only_the_colo_from_the_trace() {
    let (mock, backend) = server().await;
    Mock::given(method("GET"))
        .and(path("/cdn-cgi/trace"))
        .respond_with(identifying(ResponseTemplate::new(200).set_body_string(
            "fl=1\nh=speed.cloudflare.com\nip=203.0.113.9\nts=1\ncolo=FRA\nloc=DE\n",
        )))
        .mount(&mock)
        .await;
    let found = backend.server().await.unwrap();
    assert_eq!(found.provider, "Cloudflare");
    assert_eq!(found.colo.as_deref(), Some("FRA"));
    let shown = format!("{found:?}");
    assert!(!shown.contains("203.0.113") && !shown.contains("Exampletown"));
}

#[test]
fn colo_must_be_an_airport_code() {
    assert_eq!(parse_colo("colo=AMS\n"), Some("AMS".into()));
    assert_eq!(parse_colo("colo=ams\n"), None);
    assert_eq!(parse_colo("colo=<script>\n"), None);
    assert_eq!(parse_colo("ip=1\n"), None);
}

#[tokio::test]
async fn unreachable_server_is_offline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let backend = Cloudflare::with_base(&format!("http://{address}"));
    assert_eq!(backend.server().await, Err(BackendError::Offline));
    let connection = backend.connect().unwrap();
    assert_eq!(connection.probe().await, Err(BackendError::Offline));
}

#[tokio::test]
async fn probe_subtracts_server_time_and_identifies_the_app() {
    let (mock, backend) = server().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .and(query_param("bytes", "0"))
        .and(header_regex("user-agent", r"^Weir/\d+\.\d+\.\d+ "))
        .respond_with(identifying(
            ResponseTemplate::new(200)
                .insert_header("server-timing", "cfSpeedEdge;dur=3, cfSpeedWorker;dur=37")
                .set_delay(Duration::from_millis(80)),
        ))
        .mount(&mock)
        .await;
    let rtt = backend.connect().unwrap().probe().await.unwrap();
    assert!((40.0..70.0).contains(&rtt), "{rtt}");
}

#[tokio::test]
async fn probe_that_never_answers_times_out() {
    let (mock, backend) = server().await;
    Mock::given(path("/__down"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(4)))
        .mount(&mock)
        .await;
    assert_eq!(
        backend.connect().unwrap().probe().await,
        Err(BackendError::Timeout)
    );
}

#[tokio::test]
async fn download_counts_every_byte_received() {
    let (mock, backend) = server().await;
    Mock::given(method("GET"))
        .and(path("/__down"))
        .and(query_param("bytes", "1000000"))
        .respond_with(identifying(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![7_u8; 1_000_000])
                .set_delay(Duration::from_millis(200)),
        ))
        .mount(&mock)
        .await;
    let counter = Arc::new(AtomicU64::new(0));
    let connection = backend.connect().unwrap();
    assert_eq!(
        connection.download(1_000_000, counter.clone()).await,
        Ok(1_000_000)
    );
    assert_eq!(
        connection.download(1_000_000, counter.clone()).await,
        Ok(1_000_000)
    );
    assert_eq!(counter.load(Ordering::Relaxed), 2_000_000);
}

#[tokio::test]
async fn refusals_and_server_errors_are_typed() {
    let (mock, backend) = server().await;
    for (bytes, status, extra) in [
        ("403", 403, None),
        ("429", 429, Some("7")),
        ("503", 503, None),
    ] {
        let mut template = ResponseTemplate::new(status);
        if let Some(seconds) = extra {
            template = template.insert_header("retry-after", seconds);
        }
        Mock::given(path("/__down"))
            .and(query_param("bytes", bytes))
            .respond_with(template)
            .mount(&mock)
            .await;
    }
    let connection = backend.connect().unwrap();
    let counter = Arc::new(AtomicU64::new(0));
    assert_eq!(
        connection.download(403, counter.clone()).await,
        Err(BackendError::Refused {
            status: 403,
            retry_after: None
        })
    );
    assert_eq!(
        connection.download(429, counter.clone()).await,
        Err(BackendError::Refused {
            status: 429,
            retry_after: Some(Duration::from_secs(7))
        })
    );
    assert_eq!(
        connection.download(503, counter.clone()).await,
        Err(BackendError::ServerError { status: 503 })
    );
    assert_eq!(counter.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn body_cut_off_mid_transfer_is_interrupted() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100000\r\n\r\n")
            .await
            .unwrap();
        socket.write_all(&[1_u8; 40_000]).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    let backend = Cloudflare::with_base(&format!("http://{address}"));
    let counter = Arc::new(AtomicU64::new(0));
    let result = backend
        .connect()
        .unwrap()
        .download(100_000, counter.clone())
        .await;
    assert_eq!(result, Err(BackendError::Interrupted));
    assert_eq!(counter.load(Ordering::Relaxed), 40_000);
}

#[tokio::test]
async fn upload_sends_exact_length_and_times_the_request() {
    let (mock, backend) = server().await;
    Mock::given(method("POST"))
        .and(path("/__up"))
        .and(header_regex("content-length", "^300000$"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("server-timing", "cfSpeedEdge;dur=2, cfSpeedWorker;dur=8")
                .set_delay(Duration::from_millis(120)),
        )
        .expect(1)
        .mount(&mock)
        .await;
    let counter = Arc::new(AtomicU64::new(0));
    let sent = backend
        .connect()
        .unwrap()
        .upload(300_000, counter.clone())
        .await
        .unwrap();
    assert_eq!(sent.bytes, 300_000);
    assert!(sent.duration_ms >= 100.0, "{}", sent.duration_ms);
    assert_eq!(counter.load(Ordering::Relaxed), 300_000);
    let received: Vec<Request> = mock.received_requests().await.unwrap();
    assert_eq!(received[0].body.len(), 300_000);
}
