#![no_main]

use libfuzzer_sys::fuzz_target;
use weir_lib::core::history::{parse, to_text};

fuzz_target!(|text: &str| {
    if let Some((entries, _)) = parse(text) {
        assert!(entries
            .windows(2)
            .all(|pair| pair[0].at_ms() <= pair[1].at_ms()));
        let saved = to_text(&entries).expect("history serializes");
        assert_eq!(parse(&saved), Some((entries, 0)));
    }
});
