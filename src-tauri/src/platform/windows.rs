use crate::core::network::{Cost, Path};
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
