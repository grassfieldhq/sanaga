#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| sanaga_fuzz::import::fuzz_one(data));
