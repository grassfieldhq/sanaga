#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| sanaga_fuzz::html::fuzz_one(data));
