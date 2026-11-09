#![no_main]

use libfuzzer_sys::fuzz_target;
use weir_lib::core::backends::cloudflare::parse_colo;

fuzz_target!(|trace: &str| {
    if let Some(colo) = parse_colo(trace) {
        assert_eq!(colo.len(), 3);
        assert!(colo.bytes().all(|byte| byte.is_ascii_uppercase()));
    }
});
