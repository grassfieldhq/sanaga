//! See Cargo.toml: Sanaga's own readers, built without GTK and WebKit, for
//! libFuzzer. Each module's `fuzz_one` is its entry point.
#![allow(dead_code)]

#[path = "../src/guard.rs"]
pub mod guard;

#[path = "../src/i18n.rs"]
pub mod i18n;

#[path = "../src/html.rs"]
pub mod html;

#[path = "../src/import.rs"]
pub mod import;

#[path = "../src/blocklist.rs"]
pub mod blocklist;

#[path = "../src/webext.rs"]
pub mod webext;

// What those modules ask of the rest of Sanaga, for the parts the fuzzers
// never reach (fetching the lists, installing into the extensions folder).
pub mod keyring {
    pub use crate::guard::js_str;
}

pub mod extensions {
    pub fn prepare_folder() -> std::path::PathBuf {
        std::env::temp_dir().join("sanaga-fuzz-extensions")
    }
}

pub mod net {
    pub fn builder() -> ureq::AgentBuilder {
        ureq::AgentBuilder::new()
    }
}

pub mod store {
    pub fn write_atomic(path: &std::path::Path, text: impl AsRef<[u8]>) -> std::io::Result<()> {
        std::fs::write(path, text)
    }
}
