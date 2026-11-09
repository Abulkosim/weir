#![no_main]

use libfuzzer_sys::fuzz_target;
use weir_lib::core::stats::server_time_ms;

fuzz_target!(|header: &str| {
    if let Some(ms) = server_time_ms(header) {
        assert!(ms.is_finite() && ms >= 0.0, "server time {ms}");
    }
});
