#![no_main]

use libfuzzer_sys::fuzz_target;
use weir_lib::core::settings::{parse, to_text, Parsed};

fuzz_target!(|text: &str| {
    if let Parsed::Read(settings, _) = parse(text) {
        let saved = to_text(&settings).expect("settings serialize");
        assert_eq!(parse(&saved), Parsed::Read(settings, Vec::new()));
    }
});
