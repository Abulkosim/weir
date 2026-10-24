//! OS network-path watchers. They report facts only and send no traffic.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use crate::core::{diagnose::Wifi, network::Path};
use std::net::Ipv4Addr;

/// ICMP echo round trips in ms, one per answered probe. `None` when the OS
/// does not allow unprivileged ICMP; the caller then falls back to TCP.
pub fn ping(target: Ipv4Addr, count: u32) -> Option<Vec<f64>> {
    #[cfg(target_os = "macos")]
    return macos::ping(target, count);
    #[cfg(windows)]
    return windows::ping(target, count);
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (target, count);
        None
    }
}

/// Wi-Fi signal of the current connection. Never reads the network name.
pub fn wifi() -> Option<Wifi> {
    #[cfg(target_os = "macos")]
    return macos::wifi();
    #[cfg(windows)]
    return windows::wifi();
    #[cfg(not(any(target_os = "macos", windows)))]
    None
}

/// Calls `on_change` with the current path now and on every change.
pub fn watch_path(on_change: impl Fn(Path) + Send + Sync + 'static) {
    #[cfg(target_os = "macos")]
    macos::watch(on_change);
    #[cfg(windows)]
    windows::watch(on_change);
    #[cfg(not(any(target_os = "macos", windows)))]
    on_change(Path {
        online: true,
        ..Path::default()
    });
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "reads this machine's network path"]
    fn prints_the_current_path() {
        let (sender, receiver) = std::sync::mpsc::channel();
        super::watch_path(move |path| {
            let _ = sender.send(path);
        });
        let path = receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        println!("online={} metered={:?}", path.online, path.metered());
    }
}
