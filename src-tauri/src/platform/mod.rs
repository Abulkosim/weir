//! OS network-path watchers. They report facts only and send no traffic.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use crate::core::network::Path;

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
