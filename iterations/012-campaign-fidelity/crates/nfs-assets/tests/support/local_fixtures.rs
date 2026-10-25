//! Recordings derived from the user's own copy of the game.
//!
//! The repository ships no data taken from the original game. Fixtures that were produced from
//! game files live in the gitignored `local/fixtures/` directory and are created by
//! `scripts/research/record-local-fixtures.sh`. Tests that need them skip (with a message) when
//! they are absent. `NFS_LOCAL_FIXTURES` overrides the directory.
#![allow(dead_code)]

use std::ops::Deref;
use std::path::PathBuf;
use std::sync::OnceLock;

pub fn dir() -> PathBuf {
    match std::env::var_os("NFS_LOCAL_FIXTURES") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/fixtures"),
    }
}

pub struct Fixture {
    name: &'static str,
    text: OnceLock<Option<String>>,
}

impl Fixture {
    pub const fn new(name: &'static str) -> Self {
        Fixture { name, text: OnceLock::new() }
    }
    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn available(&self) -> bool {
        self.text.get_or_init(|| std::fs::read_to_string(dir().join(self.name)).ok()).is_some()
    }
}

impl Deref for Fixture {
    type Target = str;
    fn deref(&self) -> &str {
        self.available();
        self.text.get().and_then(|t| t.as_deref()).unwrap_or("")
    }
}

/// Returns from the test (a pass) with a message when a local fixture is missing.
#[allow(unused_macros)]
macro_rules! skip_without_local_fixtures {
    ($($f:expr),+ $(,)?) => {
        $(
            if !$f.available() {
                eprintln!(
                    "SKIPPED: local fixture {} not found; run scripts/research/record-local-fixtures.sh",
                    $f.name()
                );
                return;
            }
        )+
    };
}

/// Like `skip_without_local_fixtures!`, for fixtures read by file name at run time.
#[allow(unused_macros)]
macro_rules! skip_without_local_files {
    ($($f:expr),+ $(,)?) => {
        $(
            if !$crate::local_fixtures::dir().join($f).is_file() {
                eprintln!(
                    "SKIPPED: local fixture {} not found; run scripts/research/record-local-fixtures.sh",
                    $f
                );
                return;
            }
        )+
    };
}
