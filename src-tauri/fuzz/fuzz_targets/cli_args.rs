#![no_main]

use libfuzzer_sys::fuzz_target;
use weir_lib::core::cli::parse;

fuzz_target!(|text: &str| {
    let args: Vec<String> = std::iter::once("weir")
        .chain(text.split('\0'))
        .map(str::to_owned)
        .collect();
    let _ = parse(&args);
});
