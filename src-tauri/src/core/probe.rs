//! Small, privacy-safe probes for the connection check and the outage watch: the
//! router, the DNS server and Cloudflare. Nothing here is stored or logged.

use super::{
    backends::{Backend, Connection},
    diagnose::{Readings, Router},
    network::Path,
    outage::Signals,
    stats::median,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use netdev::ipnet::{Ipv4Net, Ipv6Net};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, time::timeout};

pub const DNS_NAME: &str = "speed.cloudflare.com";
const DNS_TIMEOUT: Duration = Duration::from_millis(1_500);
const TCP_TIMEOUT: Duration = Duration::from_millis(1_000);
const ROUTER_PROBES: u32 = 5;
/// Router probes per failed outage-watch check: fewer than the connection check sends.
pub const OUTAGE_PROBES: u32 = 2;
const LOCAL_LIMIT: Duration = Duration::from_secs(1);
/// Two ICMP probes (about 2.24 s) and, if both go unanswered, two TCP rounds of 1 s.
const ROUTER_LIMIT: Duration = Duration::from_secs(5);
/// Longest wait for a router probe already running: the connection check's 5 ICMP
/// probes and 5 TCP rounds against a silent router take about 10.6 s.
const ROUTER_QUEUE_LIMIT: Duration = Duration::from_secs(12);
/// Two DNS attempts of 1.5 s each.
const DNS_LIMIT: Duration = Duration::from_millis(3_500);

/// One router probe at a time, so the watch and the connection check never read
/// each other's ICMP replies.
static ROUTER_PROBE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

type Alone = tokio::sync::MutexGuard<'static, ()>;

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

/// True when `reply` is any response to query `id`, whatever its result code. A
/// forwarder that answers "server failure" during an upstream outage is still reachable.
pub fn dns_replies(reply: &[u8], id: u16) -> bool {
    reply.len() >= 12 && reply[..2] == id.to_be_bytes() && reply[2] & 0x80 != 0
}

fn dns_id() -> u16 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.subsec_nanos() as u16)
        ^ std::process::id() as u16
}

async fn dns_once(server: SocketAddr, id: u16) -> Option<f64> {
    dns_exchange(server, id, dns_answers).await
}

async fn dns_exchange(server: SocketAddr, id: u16, accept: fn(&[u8], u16) -> bool) -> Option<f64> {
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
        if accept(&buffer[..size], id) {
            return Some(started.elapsed().as_secs_f64() * 1_000.0);
        }
    }
}

/// Median lookup time over three queries, or `None` when none were answered.
pub async fn dns_ms(server: IpAddr) -> Option<f64> {
    let seed = dns_id();
    let mut times = Vec::new();
    for attempt in 0..3_u16 {
        if let Some(ms) = dns_once((server, 53).into(), seed.wrapping_add(attempt)).await {
            times.push(ms);
        }
    }
    median(&times)
}

/// True on the first reply of any kind within `attempts` queries.
pub async fn dns_reachable(server: SocketAddr, attempts: u16) -> bool {
    let seed = dns_id();
    for attempt in 0..attempts {
        if dns_exchange(server, seed.wrapping_add(attempt), dns_replies)
            .await
            .is_some()
        {
            return true;
        }
    }
    false
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

/// The first common port to answer. Tried together, so ports that drop packets
/// cost one timeout per probe, not one each.
async fn first_port(gateway: Ipv4Addr) -> Option<f64> {
    let mut attempts: FuturesUnordered<_> = [80, 443, 53]
        .into_iter()
        .map(|port| tcp_ms((gateway, port).into()))
        .collect();
    while let Some(answer) = attempts.next().await {
        if answer.is_some() {
            return answer;
        }
    }
    None
}

/// Router round trips for the connection check.
pub async fn router(gateway: Ipv4Addr) -> Router {
    router_with(gateway, ROUTER_PROBES).await
}

/// Router round trips: ICMP where the OS allows it, else TCP connects to common ports.
pub async fn router_with(gateway: Ipv4Addr, probes: u32) -> Router {
    probe_router(ROUTER_PROBE.lock().await, gateway, probes).await
}

/// A short router probe for the outage watch, `None` when the wait for another
/// probe or the probe itself runs out of time.
async fn router_in_time(gateway: Ipv4Addr, probes: u32) -> Option<Router> {
    let alone = timeout(ROUTER_QUEUE_LIMIT, ROUTER_PROBE.lock())
        .await
        .ok()?;
    timeout(ROUTER_LIMIT, probe_router(alone, gateway, probes))
        .await
        .ok()
}

/// Runs blocking `work` with `guard` moved into it, so a caller cut short by a time
/// limit doesn't release the guard while the work still runs.
async fn holding<G, T>(guard: G, work: impl FnOnce() -> T + Send + 'static) -> Option<(T, G)>
where
    G: Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || (work(), guard))
        .await
        .ok()
}

async fn probe_router(alone: Alone, gateway: Ipv4Addr, probes: u32) -> Router {
    let mut times = Vec::new();
    let (icmp, _alone) = match holding(alone, move || crate::platform::ping(gateway, probes)).await
    {
        Some((icmp, alone)) => (icmp, Some(alone)),
        None => (None, None),
    };
    let sent = probes;
    match icmp {
        Some(replies) if !replies.is_empty() => times = replies,
        _ => {
            for _ in 0..sent {
                if let Some(ms) = first_port(gateway).await {
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Local {
    pub gateway: Option<Ipv4Addr>,
    pub dns: Option<IpAddr>,
    /// The default route is a VPN or other tunnel, so its gateway is not the home router.
    pub tunnel: bool,
    /// A reply from `dns` means traffic got through the router.
    pub dns_past_router: bool,
}

/// Adapters that some VPNs report as plain Ethernet, by words in their name or description.
const VPN_WORDS: [&str; 14] = [
    "tap-",
    "tap adapter",
    "wintun",
    "wireguard",
    "openvpn",
    "vpn",
    "tunnel",
    "anyconnect",
    "globalprotect",
    "pangp",
    "zscaler",
    "tailscale",
    "zerotier",
    "nordlynx",
];

pub fn named_like_vpn<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    names.into_iter().any(|name| {
        let name = name.to_lowercase();
        VPN_WORDS.iter().any(|word| name.contains(word))
    })
}

fn on_link(address: Ipv6Addr) -> bool {
    address.is_loopback() || address.segments()[0] & 0xffc0 == 0xfe80
}

/// Whether a reply from `dns` says traffic got through the router: it is the
/// router itself, or it sits past it. A server elsewhere on the home network (a
/// Pi-hole, a NAS) or a stub on this computer can answer while the router is down.
pub fn dns_past_router(dns: IpAddr, gateways: &[IpAddr], v4: &[Ipv4Net], v6: &[Ipv6Net]) -> bool {
    if gateways.contains(&dns) {
        return true;
    }
    match dns {
        IpAddr::V4(dns) => !dns.is_loopback() && !v4.iter().any(|net| net.contains(&dns)),
        IpAddr::V6(dns) => !on_link(dns) && !v6.iter().any(|net| net.contains(&dns)),
    }
}

pub fn local() -> Local {
    use netdev::interface::types::InterfaceType;
    let Ok(interface) = netdev::get_default_interface() else {
        return Local::default();
    };
    let tunnel = interface.is_tun()
        || matches!(
            interface.if_type,
            InterfaceType::Tunnel | InterfaceType::Ppp | InterfaceType::ProprietaryVirtual
        )
        || (matches!(
            interface.if_type,
            InterfaceType::Unknown | InterfaceType::UnknownWithValue(_)
        ) && interface.is_point_to_point())
        || named_like_vpn(
            std::iter::once(interface.name.as_str())
                .chain(interface.friendly_name.as_deref())
                .chain(interface.description.as_deref()),
        );
    let gateways: Vec<IpAddr> = interface.gateway.as_ref().map_or_else(Vec::new, |gateway| {
        gateway
            .ipv4
            .iter()
            .copied()
            .map(IpAddr::V4)
            .chain(gateway.ipv6.iter().copied().map(IpAddr::V6))
            .collect()
    });
    let dns = interface.dns_servers.first().copied();
    Local {
        gateway: interface
            .gateway
            .and_then(|gateway| gateway.ipv4.first().copied()),
        dns,
        tunnel,
        dns_past_router: dns
            .is_some_and(|dns| dns_past_router(dns, &gateways, &interface.ipv4, &interface.ipv6)),
    }
}

/// A path whose router is worth asking: Wi-Fi or a cable. Windows doesn't report a
/// cable, so there anything but a cellular-only link counts.
fn home_path(path: Path) -> bool {
    path.wifi || path.wired || (cfg!(windows) && !path.cellular)
}

/// A gateway that can be a home router: a private address. A public one belongs
/// to the provider, so its silence says nothing about home.
fn home_gateway(gateway: Ipv4Addr) -> bool {
    gateway.is_private()
}

/// The home router's address and a DNS server whose reply proves traffic got
/// past it, unless there is no usable router to ask.
async fn home_router(path: Path) -> Option<(Ipv4Addr, Option<IpAddr>)> {
    if !home_path(path) {
        return None;
    }
    let local = timeout(LOCAL_LIMIT, tokio::task::spawn_blocking(local))
        .await
        .ok()?
        .ok()?;
    if local.tunnel {
        return None;
    }
    let gateway = local.gateway.filter(|gateway| home_gateway(*gateway))?;
    Some((gateway, local.dns.filter(|_| local.dns_past_router)))
}

/// Where a failed outage-watch check broke: the router first, the DNS server only
/// when the router is silent. Each stage has its own time limit, and a limit hit
/// reads as "not known", never as silence.
pub async fn where_broken(path: Path) -> Signals {
    let Some((gateway, dns)) = home_router(path).await else {
        return Signals::default();
    };
    let Some(router) = router_in_time(gateway, OUTAGE_PROBES).await else {
        return Signals::default();
    };
    if router.answered > 0 {
        return Signals {
            router: Some(true),
            dns: None,
        };
    }
    let dns = match dns {
        Some(server) => timeout(DNS_LIMIT, dns_reachable((server, 53).into(), 2))
            .await
            .ok(),
        None => None,
    };
    Signals {
        router: Some(false),
        dns,
    }
}

/// The router re-check once the internet is back. `None` when there is no usable
/// router to ask or the probe ran out of time.
pub async fn router_answers(path: Path) -> Option<bool> {
    let (gateway, _) = home_router(path).await?;
    router_in_time(gateway, OUTAGE_PROBES)
        .await
        .map(|router| router.answered > 0)
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
    let local = tokio::task::spawn_blocking(local).await.unwrap_or_default();
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

    #[test]
    fn any_reply_to_the_query_proves_the_server_is_reachable() {
        let mut reply = dns_query(9, DNS_NAME);
        assert!(!dns_replies(&reply, 9), "a query is not a reply");
        reply[2] |= 0x80;
        for code in [0, 2, 3, 5] {
            reply[3] = code;
            assert!(dns_replies(&reply, 9), "rcode {code}");
        }
        assert!(!dns_replies(&reply, 10));
        assert!(!dns_replies(&reply[..11], 9));
    }

    #[tokio::test]
    async fn a_server_failure_still_counts_as_reachable_and_silence_does_not() {
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buffer = [0_u8; 512];
            while let Ok((size, from)) = server.recv_from(&mut buffer).await {
                buffer[2] |= 0x80;
                buffer[3] = (buffer[3] & 0xf0) | 2;
                let _ = server.send_to(&buffer[..size], from).await;
            }
        });
        assert!(dns_reachable(address, 2).await);
        assert_eq!(
            dns_once(address, 44).await,
            None,
            "but it isn't a working lookup"
        );
        let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let started = Instant::now();
        assert!(!dns_reachable(silent.local_addr().unwrap(), 2).await);
        assert!(started.elapsed() < Duration::from_millis(3_200));
    }

    #[test]
    fn only_wifi_or_a_cable_has_a_router_worth_asking() {
        let wifi = Path {
            online: true,
            wifi: true,
            ..Path::default()
        };
        assert!(home_path(wifi));
        let cellular = Path {
            online: true,
            cellular: true,
            ..Path::default()
        };
        assert!(!home_path(cellular));
    }

    #[test]
    fn only_a_dns_server_at_or_past_the_router_vouches_for_it() {
        let v4: Vec<Ipv4Net> = vec!["192.168.1.20/24".parse().unwrap()];
        let v6: Vec<Ipv6Net> = vec!["fd12:3456:789a::20/64".parse().unwrap()];
        let gateways: Vec<IpAddr> =
            vec!["192.168.1.1".parse().unwrap(), "fe80::1".parse().unwrap()];
        let vouches = |dns: &str| dns_past_router(dns.parse().unwrap(), &gateways, &v4, &v6);
        assert!(vouches("192.168.1.1"), "the router itself");
        assert!(vouches("fe80::1"), "the router's link-local address");
        assert!(vouches("1.1.1.1"), "past the router");
        assert!(
            vouches("10.20.0.53"),
            "another network, reached through the router"
        );
        assert!(vouches("2606:4700:4700::1111"));
        assert!(!vouches("192.168.1.2"), "a Pi-hole beside the router");
        assert!(
            !vouches("fd12:3456:789a::53"),
            "a server on the home network"
        );
        assert!(!vouches("fe80::53"), "a neighbour on the link");
        assert!(!vouches("127.0.0.53"), "a stub on this computer");
        assert!(!vouches("::1"));
    }

    #[test]
    fn vpn_adapters_that_look_like_ethernet_are_tunnels() {
        for names in [
            ["Ethernet 2", "TAP-Windows Adapter V9"],
            ["OpenVPN Data Channel Offload", ""],
            ["Local Area Connection", "Wintun Userspace Tunnel"],
            ["NordLynx", ""],
            ["Ethernet 3", "PANGP Virtual Ethernet Adapter"],
            [
                "Ethernet 4",
                "Cisco AnyConnect Secure Mobility Client Virtual Miniport Adapter for Windows x64",
            ],
        ] {
            assert!(named_like_vpn(names), "{names:?}");
        }
        for names in [
            ["Wi-Fi", "Intel(R) Wi-Fi 6 AX201 160MHz"],
            ["Ethernet", "Realtek PCIe GbE Family Controller"],
            ["vEthernet (External)", "Hyper-V Virtual Ethernet Adapter"],
            ["en0", ""],
        ] {
            assert!(!named_like_vpn(names), "{names:?}");
        }
    }

    #[test]
    fn only_a_private_gateway_is_a_home_router() {
        assert!(home_gateway(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(home_gateway(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(home_gateway(Ipv4Addr::new(172, 20, 10, 1)));
        assert!(
            !home_gateway(Ipv4Addr::new(203, 0, 113, 1)),
            "the provider's"
        );
        assert!(
            !home_gateway(Ipv4Addr::new(100, 64, 0, 1)),
            "carrier-grade NAT"
        );
    }

    #[tokio::test]
    async fn a_probe_cut_short_keeps_the_lock_until_its_work_ends() {
        let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let guard = lock.clone().lock_owned().await;
        let work = holding(guard, || std::thread::sleep(Duration::from_millis(300)));
        assert!(timeout(Duration::from_millis(50), work).await.is_err());
        assert!(lock.try_lock().is_err(), "the work still runs");
        assert!(timeout(Duration::from_secs(2), lock.lock()).await.is_ok());
    }

    #[tokio::test]
    async fn no_router_to_ask_is_not_known() {
        let cellular = Path {
            online: true,
            cellular: true,
            ..Path::default()
        };
        assert_eq!(where_broken(cellular).await, Signals::default());
        assert_eq!(router_answers(cellular).await, None);
    }

    #[tokio::test]
    async fn a_short_router_check_answers_on_this_machine() {
        if cfg!(windows) {
            return;
        }
        let router = router_with(Ipv4Addr::LOCALHOST, OUTAGE_PROBES).await;
        assert_eq!(router.sent, OUTAGE_PROBES);
        assert!(router.answered > 0);
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

    #[tokio::test]
    async fn router_ports_answer_with_the_first_reply() {
        if cfg!(windows) {
            return;
        }
        let started = Instant::now();
        assert!(first_port(Ipv4Addr::LOCALHOST).await.is_some());
        assert!(started.elapsed() < TCP_TIMEOUT);
    }
}
