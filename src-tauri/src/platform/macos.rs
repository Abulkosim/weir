use crate::core::{diagnose::Wifi, network::Path};
use block2::RcBlock;
use std::ffi::{c_char, c_int, c_void};

type Object = *mut c_void;

const STATUS_SATISFIED: c_int = 1;
const INTERFACE_WIFI: c_int = 1;
const INTERFACE_CELLULAR: c_int = 2;
const INTERFACE_WIRED: c_int = 3;

#[link(name = "Network", kind = "framework")]
extern "C" {
    fn nw_path_monitor_create() -> Object;
    fn nw_path_monitor_set_queue(monitor: Object, queue: Object);
    fn nw_path_monitor_set_update_handler(monitor: Object, handler: &block2::Block<dyn Fn(Object)>);
    fn nw_path_monitor_start(monitor: Object);
    fn nw_path_get_status(path: Object) -> c_int;
    fn nw_path_is_expensive(path: Object) -> bool;
    fn nw_path_is_constrained(path: Object) -> bool;
    fn nw_path_uses_interface_type(path: Object, interface: c_int) -> bool;
}

extern "C" {
    fn dispatch_queue_create(label: *const c_char, attributes: Object) -> Object;
}

/// Reads one `nw_path_t`. Safety: `path` is the live path passed to the update handler.
unsafe fn read(path: Object) -> Path {
    Path {
        online: nw_path_get_status(path) == STATUS_SATISFIED,
        expensive: nw_path_is_expensive(path),
        constrained: nw_path_is_constrained(path),
        cost: None,
        roaming: false,
        over_limit: false,
        wifi: nw_path_uses_interface_type(path, INTERFACE_WIFI),
        wired: nw_path_uses_interface_type(path, INTERFACE_WIRED),
        cellular: nw_path_uses_interface_type(path, INTERFACE_CELLULAR),
    }
}

pub fn watch(on_change: impl Fn(Path) + Send + Sync + 'static) {
    let handler = RcBlock::new(move |path: Object| {
        if !path.is_null() {
            // Network.framework passes a valid path for the duration of the call.
            on_change(unsafe { read(path) });
        }
    });
    // The monitor and queue live for the whole app; Network.framework copies the block.
    unsafe {
        let queue =
            dispatch_queue_create(c"com.abulkosim.weir.path".as_ptr(), std::ptr::null_mut());
        let monitor = nw_path_monitor_create();
        nw_path_monitor_set_queue(monitor, queue);
        nw_path_monitor_set_update_handler(monitor, &handler);
        nw_path_monitor_start(monitor);
    }
}

/// One ICMP echo request. Identifier and checksum are filled in by the kernel
/// for datagram ICMP sockets, but a valid checksum keeps older systems happy.
fn echo_request(sequence: u16) -> [u8; 24] {
    let mut packet = [0_u8; 24];
    packet[0] = 8;
    packet[6..8].copy_from_slice(&sequence.to_be_bytes());
    packet[8..].copy_from_slice(b"weir router ping");
    let mut sum: u32 = packet
        .chunks(2)
        .map(|pair| u32::from(u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)])))
        .sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    packet[2..4].copy_from_slice(&(!(sum as u16)).to_be_bytes());
    packet
}

/// Echo reply payload, skipping the IPv4 header that macOS includes.
fn reply_sequence(reply: &[u8]) -> Option<u16> {
    let icmp = if reply.first().is_some_and(|byte| byte >> 4 == 4) {
        reply.get(usize::from(reply[0] & 0x0f) * 4..)?
    } else {
        reply
    };
    (icmp.len() >= 8 && icmp[0] == 0).then(|| u16::from_be_bytes([icmp[6], icmp[7]]))
}

pub fn ping(target: std::net::Ipv4Addr, count: u32) -> Option<Vec<f64>> {
    use socket2::{Domain, Protocol, Socket, Type};
    use std::{
        mem::MaybeUninit,
        net::SocketAddrV4,
        time::{Duration, Instant},
    };

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(1_000)))
        .ok()?;
    let address = SocketAddrV4::new(target, 0).into();
    let mut times = Vec::new();
    for sequence in 1..=count as u16 {
        let started = Instant::now();
        socket.send_to(&echo_request(sequence), &address).ok()?;
        let mut buffer = [MaybeUninit::<u8>::uninit(); 256];
        while started.elapsed() < Duration::from_millis(1_000) {
            let Ok(size) = socket.recv(&mut buffer) else {
                break;
            };
            // recv initialised the first `size` bytes.
            let bytes: Vec<u8> = buffer[..size]
                .iter()
                .map(|byte| unsafe { byte.assume_init() })
                .collect();
            if reply_sequence(&bytes) == Some(sequence) {
                times.push(started.elapsed().as_secs_f64() * 1_000.0);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    Some(times)
}

pub fn wifi() -> Option<Wifi> {
    use objc2::rc::autoreleasepool;
    use objc2_core_wlan::CWWiFiClient;

    // CoreWLAN returns autoreleased objects; without a pool they leak on this thread.
    autoreleasepool(|_| {
        let interface = unsafe { CWWiFiClient::sharedWiFiClient().interface() }?;
        let rssi = unsafe { interface.rssiValue() } as i32;
        let noise = unsafe { interface.noiseMeasurement() } as i32;
        let rate = unsafe { interface.transmitRate() };
        (rssi != 0).then_some(Wifi {
            rssi_dbm: Some(rssi),
            noise_dbm: (noise != 0).then_some(noise),
            rate_mbps: (rate > 0.0).then_some(rate),
            bars: None,
        })
    })
}

pub fn low_power() -> bool {
    objc2_foundation::NSProcessInfo::processInfo().isLowPowerModeEnabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_request_has_a_valid_checksum() {
        let packet = echo_request(3);
        let mut sum: u32 = packet
            .chunks(2)
            .map(|pair| u32::from(u16::from_be_bytes([pair[0], pair[1]])))
            .sum();
        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        assert_eq!(sum, 0xffff);
        assert_eq!(packet[0], 8);
    }

    #[test]
    #[ignore = "pings this machine's router"]
    fn unprivileged_icmp_reaches_the_router() {
        let gateway = crate::core::probe::local().gateway.expect("a router");
        let replies = ping(gateway, 3).expect("datagram ICMP is allowed");
        println!("ICMP replies {} of 3", replies.len());
        assert!(!replies.is_empty());
    }

    #[test]
    fn replies_are_read_with_or_without_the_ip_header() {
        let mut reply = echo_request(9);
        reply[0] = 0;
        assert_eq!(reply_sequence(&reply), Some(9));
        let mut with_header = vec![0x45; 1];
        with_header.extend_from_slice(&[0; 19]);
        with_header.extend_from_slice(&reply);
        assert_eq!(reply_sequence(&with_header), Some(9));
        assert_eq!(
            reply_sequence(&echo_request(9)),
            None,
            "a request is not a reply"
        );
    }
}
