//! 判据用的住址（本包那一份；同 monitor `src/frontend/shell/src/guard_support.rs` · 后端 `src/backend/guard_support.rs`，一个 crate 一份）。
#![cfg(test)]

use std::path::{Path, PathBuf};

pub(crate) fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

pub(crate) fn crate_src_root() -> PathBuf {
    crate_root().join("src")
}

pub(crate) fn repo_root() -> PathBuf {
    crate_root()
        .ancestors()
        .nth(3)
        .expect("src/frontend/filewin 的上三级 = 仓根")
        .to_path_buf()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/guard_support_tests.rs"]
mod tests;
