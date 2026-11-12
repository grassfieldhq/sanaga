# Fuzzing Sanaga's readers

Four parts of Sanaga read files or pages that come from anywhere, with code
of Sanaga's own (not the web engine's):

| Reader | File | What it reads |
|---|---|---|
| `html` | `src/html.rs` | pages, for the Light engine and the Reader |
| `webext` | `src/webext.rs` | extensions of other browsers: `.crx` / `.xpi` / `.zip` archives, `manifest.json`, locale files |
| `import` | `src/import.rs` | bookmarks of other browsers: Chrome's `Bookmarks` file, exported HTML |
| `blocklist` | `src/blocklist.rs` | the ad-block lists (EasyList…), turned into WebKit rule lists |

Whatever they are given, they must never panic, run for ever or use memory
without limit — and what they hand on must hold together (a rule list is
valid JSON, an imported address is a web address, an archive never unpacks
past its limit). Each has one entry point, `fuzz_one(data: &[u8])`, which
reads the bytes and checks those promises. Two fuzzers call it.

## 1. With the unit tests (every push)

`src/mutate.rs` is a small, repeatable fuzzer: each reader is fed its sample
inputs, every cut of them, and 30 000 damaged versions (bytes changed, parts
removed or repeated, troublesome pieces inserted). The damage is the same at
every run, so a failure can be found again.

```sh
cargo test          # damaged_pages_do_not_panic, damaged_files_do_not_panic…
```

It needs nothing but the stable compiler and takes under a second, so it
runs at every push in CI.

## 2. With libFuzzer (weekly, and on demand)

`fuzz/` holds the same readers built on their own (the very files of `src/`,
without GTK and WebKitGTK) for [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).
libFuzzer watches which code each input reaches and keeps the inputs that
reach new code: it goes much deeper than random damage.

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cargo +nightly fuzz run html -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run webext
cargo +nightly fuzz run import
cargo +nightly fuzz run blocklist
```

`-timeout=10` makes an input that takes more than ten seconds a finding too.
What it finds is kept in `fuzz/artifacts/<reader>/`; run it again with
`cargo +nightly fuzz run <reader> fuzz/artifacts/<reader>/<file>`.

CI (`.github/workflows/fuzz.yml`) runs each reader for five minutes every
week, and for one minute when a pull request touches a reader.

## When a fuzzer finds something

1. Fix the reader.
2. Add the input (made as small as possible:
   `cargo +nightly fuzz tmin <reader> <file>`) to the reader's unit tests, so
   it never comes back — see `broken_files_end` in `src/import.rs`.

The interface's own text handling (Find in page, the source view, the
address bar, model answers, settings files) cannot be built without GTK, so
it has the repeatable fuzzer only: `src/app/robust.rs`.

## Found so far

- Find in page: a page holding İ, ẞ or the Kelvin sign made Sanaga panic
  when searched (the lowercased text was cut at the original's places).
  Fixed; `find_in_page_cuts_text_on_characters`.

- `import`: a Chrome bookmarks file holding `[}` made the reader run for
  ever (importing never ended, one processor core busy); a broken surrogate
  pair (`\ud83dA`) overflowed a subtraction. Both fixed, both in the
  unit tests.

## Adding a reader

1. Give the module a `#[cfg(any(test, fuzzing))] pub fn fuzz_one(data: &[u8])`.
2. Add a unit test that calls `crate::mutate::run(&seeds, 30_000, fuzz_one)`.
3. Add the module to `fuzz/lib.rs`, a three-line target in
   `fuzz/fuzz_targets/`, a `[[bin]]` in `fuzz/Cargo.toml` and its name to
   the matrix of `.github/workflows/fuzz.yml`.
