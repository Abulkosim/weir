//! Small, privacy-safe probes for the connection check: the router, the DNS
//! server and Cloudflare. Nothing here is stored or logged.

use super::{
    backends::{Backend, Connection},
    diagnose::{Readings, Router},
    network::Path,
    stats::median,
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, time::timeout};

pub const DNS_NAME: &str = "speed.cloudflare.com";
const DNS_TIMEOUT: Duration = Duration::from_millis(1_500);
const TCP_TIMEOUT: Duration = Duration::from_millis(1_000);
const ROUTER_PROBES: u32 = 5;

/// A standard recursive A query for `name`.
pub fn dns_query(id: u16, name: &str) -> Vec<u8> {
    let mut packet = Vec::with_capacity(18 + name.len());
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.split('.').filter(|label| !label.is_empty()) {
        packet.push(label.len().min(63) as u8);
        packet.extend_from_slice(&label.as_bytes()[..label.len().min(63)]);
    }
    packet.extend_from_slice(&[0, 0, 1, 0, 1]);
    packet
}

/// True when `reply` answers query `id`: response bit set and a result code
/// that means the server worked (no error or no such name).
pub fn dns_answers(reply: &[u8], id: u16) -> bool {
    reply.len() >= 12
        && reply[..2] == id.to_be_bytes()
        && reply[2] & 0x80 != 0
        && matches!(reply[3] & 0x0f, 0 | 3)
}

async fn dns_once(server: SocketAddr, id: u16) -> Option<f64> {
    let bind: SocketAddr = if server.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(bind).await.ok()?;
    socket.connect(server).await.ok()?;
    let started = Instant::now();
    socket.send(&dns_query(id, DNS_NAME)).await.ok()?;
    let mut buffer = [0_u8; 512];
    let deadline = started + DNS_TIMEOUT;
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let size = timeout(left, socket.recv(&mut buffer)).await.ok()?.ok()?;
        if dns_answers(&buffer[..size], id) {
            return Some(started.elapsed().as_secs_f64() * 1_000.0);
        }
    }
}

/// Median lookup time over three queries, or `None` when none were answered.
pub async fn dns_ms(server: IpAddr) -> Option<f64> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.subsec_nanos() as u16)
        ^ std::process::id() as u16;
    let mut times = Vec::new();
    for attempt in 0..3_u16 {
        if let Some(ms) = dns_once((server, 53).into(), seed.wrapping_add(attempt)).await {
            times.push(ms);
        }
    }
    median(&times)
}

/// Round trip of a TCP connect. A refused port still answers, so it still counts.
pub async fn tcp_ms(address: SocketAddr) -> Option<f64> {
    let started = Instant::now();
    match timeout(TCP_TIMEOUT, tokio::net::TcpStream::connect(address)).await {
        Ok(Ok(_)) => Some(started.elapsed().as_secs_f64() * 1_000.0),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            Some(started.elapsed().as_secs_f64() * 1_000.0)
        }
        _ => None,
    }
}

/// Router round trips: ICMP where the OS allows it, else TCP connects to common ports.
pub async fn router(gateway: Ipv4Addr) -> Router {
    let mut times = Vec::new();
    let icmp = tokio::task::spawn_blocking(move || crate::platform::ping(gateway, ROUTER_PROBES))
        .await
        .ok()
        .flatten();
    let sent = ROUTER_PROBES;
    match icmp {
        Some(replies) if !replies.is_empty() => times = replies,
        _ => {
            for _ in 0..sent {
                let mut answered = None;
                for port in [80, 443, 53] {
                    answered = tcp_ms((gateway, port).into()).await;
                    if answered.is_some() {
                        break;
                    }
                }
                if let Some(ms) = answered {
                    times.push(ms);
                }
            }
        }
    }
    Router {
        sent,
        answered: times.len() as u32,
        median_ms: median(&times),
    }
}

/// Cloudflare round trip on a fresh connection: the second probe, so TLS setup is excluded.
pub async fn internet_ms<B: Backend>(backend: &B) -> Option<f64> {
    let connection = backend.connect().ok()?;
    let first = connection.probe().await.ok()?;
    Some(connection.probe().await.unwrap_or(first))
}

pub struct Local {
    pub gateway: Option<Ipv4Addr>,
    pub dns: Option<IpAddr>,
}

pub fn local() -> Local {
    let Ok(interface) = netdev::get_default_interface() else {
        return Local {
            gateway: None,
            dns: None,
        };
    };
    Local {
        gateway: interface
            .gateway
            .and_then(|gateway| gateway.ipv4.first().copied()),
        dns: interface.dns_servers.first().copied(),
    }
}

/// Runs every probe at once and gathers the readings.
pub async fn readings<B: Backend>(backend: &B, path: Path) -> Readings {
    if !path.online {
        return Readings {
            online: false,
            wired: path.wired,
            wifi: path.wifi.then(crate::platform::wifi).flatten(),
            ..Readings::default()
        };
    }
    let local = tokio::task::spawn_blocking(local).await.unwrap_or(Local {
        gateway: None,
        dns: None,
    });
    let router_check = async {
        match local.gateway {
            Some(gateway) => Some(router(gateway).await),
            None => None,
        }
    };
    let dns_check = async {
        match local.dns {
            Some(server) => Some(dns_ms(server).await),
            None => None,
        }
    };
    let (router, dns_ms, internet, wifi) = tokio::join!(
        router_check,
        dns_check,
        internet_ms(backend),
        tokio::task::spawn_blocking(crate::platform::wifi)
    );
    Readings {
        online: true,
        wired: path.wired,
        wifi: if path.wifi {
            Some(wifi.ok().flatten().unwrap_or_default())
        } else {
            None
        },
        router,
        dns_ms,
        internet_ms: Some(internet),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dns_query_is_a_recursive_a_lookup() {
        let packet = dns_query(0xbeef, "speed.cloudflare.com");
        assert_eq!(&packet[..12], &[0xbe, 0xef, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&packet[12..18], b"\x05speed");
        assert_eq!(&packet[packet.len() - 5..], &[0, 0, 1, 0, 1]);
        assert_eq!(packet.len(), 12 + 22 + 4);
    }

    #[test]
    fn dns_reply_must_match_the_query_and_succeed() {
        let mut reply = dns_query(7, DNS_NAME);
        reply[2] |= 0x80;
        assert!(dns_answers(&reply, 7));
        assert!(!dns_answers(&reply, 8));
        reply[3] = 3;
        assert!(
            dns_answers(&reply, 7),
            "no such name still means the server works"
        );
        reply[3] = 2;
        assert!(!dns_answers(&reply, 7), "server failure");
        assert!(
            !dns_answers(&dns_query(7, DNS_NAME), 7),
            "a query is not a reply"
        );
        assert!(!dns_answers(&[0, 7, 0x80], 7));
    }

    #[tokio::test]
    async fn dns_times_a_local_server_and_gives_up_on_silence() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            while let Ok((size, from)) = server.recv_from(&mut buffer).await {
                buffer[2] |= 0x80;
                let _ = server.send_to(&buffer[..size], from).await;
            }
        });
        let started = Instant::now();
        let ms = dns_once(address, 42).await;
        assert!(ms.is_some_and(|ms| ms < 500.0), "{ms:?}");
        assert!(started.elapsed() < Duration::from_secs(1));
        let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        assert_eq!(dns_once(silent.local_addr().unwrap(), 43).await, None);
    }

    #[tokio::test]
    async fn tcp_counts_open_and_refused_ports_as_answers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(tcp_ms(listener.local_addr().unwrap()).await.is_some());
        // Windows takes about 2 s to refuse a local port, longer than the probe waits.
        if cfg!(windows) {
            return;
        }
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = closed.local_addr().unwrap();
        drop(closed);
        assert!(tcp_ms(address).await.is_some());
    }
}
