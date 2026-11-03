//! 换号那几族判据共用的夹具（本包与上层判据经 `test-support` 共用）。

use std::path::PathBuf;

/// 号表里的一行（`id` 是路由段，`dir` 的末段要与它一致；`dir: None` ＝ 账号 0）。
pub fn book_row(id: &str, dir: Option<PathBuf>, api: bool) -> acct_core::ManifestAccount {
    acct_core::ManifestAccount {
        name: id.to_string(),
        config_dir: dir.map(|d| d.to_string_lossy().into_owned()),
        auth_kind: api.then(|| acct_core::AUTH_KIND_API_KEY.to_string()),
        ..Default::default()
    }
}
