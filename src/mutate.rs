//! A small, repeatable fuzzer for `cargo test`: files and pages come from
//! anywhere, so every reader of them is fed thousands of damaged versions of
//! real inputs and must never panic, hang or grow without limit. The same
//! entry points (`fuzz_one` in each module) are driven by libFuzzer for
//! longer, coverage-guided runs: see `fuzz/` and docs/FUZZING.md.

/// Pieces that readers of HTML, JSON, filter lists and archives trip over.
const TOKENS: &[&[u8]] = &[
    b"<", b">", b"</", b"<!--", b"-->", b"<script>", b"</script>", b"<a href=", b"&", b"&#", b"&#x", b";", b"\"", b"'", b"\\", b"\\u", b"{", b"}", b"[", b"]", b":", b",", b"/*", b"*/", b"//",
    b"||", b"^", b"$", b"@@", b"|", b"*", b"domain=", b"~", b"##", b"\n", b"\r\n", b"\t", b"\0", b"\xff", b"\xfe\xff", b"\xef\xbb\xbf", b"\xf0\x9f\x91\x8d", b"\xc3", b"PK\x03\x04", b"PK\x01\x02",
    b"PK\x05\x06", b"\xff\xff\xff\xff", b"\x00\x00\x00\x80", b"__MSG_", b"content_scripts", b"manifest.json", b"-1", b"1e999", b"99999999999999999999",
];

/// xorshift: the same damaged inputs at every run (a failure can be found again).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() >> 11) as usize % n }
    }
}

/// One damaged version of `input` (a few changes, as a fuzzer would make).
fn mutate(rng: &mut Rng, input: &[u8], others: &[&[u8]]) -> Vec<u8> {
    let mut v = input.to_vec();
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(v.len() + 1);
        let last = v.len().saturating_sub(1);
        match rng.below(8) {
            // a byte changed, a bit flipped
            0 if !v.is_empty() => v[at.min(last)] = rng.next() as u8,
            1 if !v.is_empty() => v[at.min(last)] ^= 1 << rng.below(8),
            // cut short, or a part removed
            2 => v.truncate(at),
            3 => {
                let end = (at + rng.below(24)).min(v.len());
                v.drain(at..end);
            }
            // a troublesome piece added
            4 | 5 => {
                let t = TOKENS[rng.below(TOKENS.len())];
                v.splice(at..at, t.iter().copied());
            }
            // a part repeated (nesting, long runs)
            6 => {
                let end = (at + 1 + rng.below(16)).min(v.len());
                let part: Vec<u8> = v[at.min(end)..end].to_vec();
                for _ in 0..rng.below(40) {
                    v.splice(at..at, part.iter().copied());
                }
            }
            // a piece of another input
            _ => {
                let o = others[rng.below(others.len())];
                let from = rng.below(o.len() + 1);
                let to = (from + rng.below(64)).min(o.len());
                v.splice(at..at, o[from..to].iter().copied());
            }
        }
        // (inputs stay small: the readers' size limits have their own tests)
        v.truncate(1 << 16);
    }
    v
}

/// Feed `reader` each seed, every cut of it, and `rounds` damaged versions.
pub fn run(seeds: &[&[u8]], rounds: usize, reader: impl Fn(&[u8])) {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for s in seeds {
        reader(s);
        for cut in 0..s.len().min(512) {
            reader(&s[..cut]);
        }
    }
    for _ in 0..rounds {
        let s = seeds[rng.below(seeds.len())];
        reader(&mutate(&mut rng, s, seeds));
    }
}
