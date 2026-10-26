use crate::core::{
    diagnose::Wifi,
    network::{Cost, Path},
};
use std::sync::Arc;
use windows::Networking::Connectivity::{
    NetworkConnectivityLevel, NetworkCostType, NetworkInformation, NetworkStatusChangedEventHandler,
};

fn read() -> Path {
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else {
        return Path::default();
    };
    let online = profile
        .GetNetworkConnectivityLevel()
        .is_ok_and(|level| level == NetworkConnectivityLevel::InternetAccess);
    let cost = profile.GetConnectionCost().ok();
    let flag = |read: fn(
        &windows::Networking::Connectivity::ConnectionCost,
    ) -> windows::core::Result<bool>| {
        cost.as_ref()
            .and_then(|cost| read(cost).ok())
            .unwrap_or(false)
    };
    let kind = cost.as_ref().and_then(|cost| cost.NetworkCostType().ok());
    Path {
        online,
        expensive: false,
        constrained: flag(|cost| cost.BackgroundDataUsageRestricted()),
        cost: Some(match kind {
            Some(NetworkCostType::Unrestricted) => Cost::Unrestricted,
            Some(NetworkCostType::Fixed) => Cost::Fixed,
            Some(NetworkCostType::Variable) => Cost::Variable,
            _ => Cost::Unknown,
        }),
        roaming: flag(|cost| cost.Roaming()),
        over_limit: flag(|cost| cost.OverDataLimit()),
        wifi: profile.IsWlanConnectionProfile().unwrap_or(false),
        wired: false,
        cellular: profile.IsWwanConnectionProfile().unwrap_or(false),
    }
}

pub fn watch(on_change: impl Fn(Path) + Send + Sync + 'static) {
    let on_change = Arc::new(on_change);
    on_change(read());
    let notify = on_change.clone();
    let handler = NetworkStatusChangedEventHandler::new(move |_| {
        notify(read());
        Ok(())
    });
    // The registration lasts for the life of the app, so the token is never used.
    let _ = NetworkInformation::NetworkStatusChanged(&handler);
}

pub fn ping(target: std::net::Ipv4Addr, count: u32) -> Option<Vec<f64>> {
    use windows::Win32::NetworkManagement::IpHelper::{
        IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
    };

    // IcmpSendEcho works for normal users, unlike raw ICMP sockets.
    let handle = unsafe { IcmpCreateFile() }.ok()?;
    let request = *b"weir-router-check";
    let mut reply = vec![0_u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + request.len() + 8];
    let destination = u32::from_ne_bytes(target.octets());
    let mut times = Vec::new();
    for _ in 0..count {
        let answered = unsafe {
            IcmpSendEcho(
                handle,
                destination,
                request.as_ptr().cast(),
                request.len() as u16,
                None,
                reply.as_mut_ptr().cast(),
                reply.len() as u32,
                1_000,
            )
        };
        if answered > 0 {
            // The buffer starts with an ICMP_ECHO_REPLY when the call reports a reply.
            let echo =
                unsafe { std::ptr::read_unaligned(reply.as_ptr().cast::<ICMP_ECHO_REPLY>()) };
            if echo.Status == 0 {
                times.push(f64::from(echo.RoundTripTime.max(1)));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    let _ = unsafe { IcmpCloseHandle(handle) };
    Some(times)
}

pub fn wifi() -> Option<Wifi> {
    let profile = NetworkInformation::GetInternetConnectionProfile().ok()?;
    if !profile.IsWlanConnectionProfile().unwrap_or(false) {
        return None;
    }
    let bars = profile.GetSignalBars().ok()?.Value().ok()?;
    Some(Wifi {
        bars: Some(bars),
        ..Wifi::default()
    })
}

pub fn low_power() -> bool {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` is a valid, writable SYSTEM_POWER_STATUS for the call.
    unsafe { GetSystemPowerStatus(&mut status) }.is_ok() && status.SystemStatusFlag == 1
}
