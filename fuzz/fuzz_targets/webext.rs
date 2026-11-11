#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| sanaga_fuzz::webext::fuzz_one(data));
