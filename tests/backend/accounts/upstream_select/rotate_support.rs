//! 换号那几族判据共用的夹具（本包与上层判据经 `test-support` 共用）。

use std::path::{Path, PathBuf};

/// 号表里的一行（`id` 是路由段，`dir` 的末段要与它一致；`dir: None` ＝ 账号 0）。
pub fn book_row(id: &str, dir: Option<PathBuf>, api: bool) -> acct_core::ManifestAccount {
    acct_core::ManifestAccount {
        name: id.to_string(),
        config_dir: dir.map(|d| d.to_string_lossy().into_owned()),
        auth_kind: api.then(|| acct_core::AUTH_KIND_API_KEY.to_string()),
        ..Default::default()
    }
}

/// 只有替身家（`agents::fake`）一家的注册表：默认上游那一格接上它的登录面（账号 0 不续不写），
/// 路由名就是它的 kind；账号身份那一格换成「凭据文件在就读得出」（替身家自己那一格恒读不出，判不到身份之后那几步）。
pub fn fake_registry() -> &'static [crate::agents::Adapter] {
    static REG: std::sync::OnceLock<Vec<crate::agents::Adapter>> = std::sync::OnceLock::new();
    REG.get_or_init(|| {
        let row = crate::agents::fake::full_row(|| None);
        let login = crate::agents::LoginFace {
            identity_in: |p| p.is_file().then(|| "who".to_string()),
            ..row
                .accounts
                .and_then(|f| f.login)
                .expect("替身家声明了登录面")
        };
        vec![crate::agents::Adapter {
            upstream: Some(crate::agents::DefaultUpstream {
                route_id: crate::agents::fake::AGENT_KIND,
                login: Some(login),
                ..crate::agents::fake::upstream::UPSTREAM
            }),
            ..row
        }]
    })
}

/// 替身家那一形的凭据（`stamped_s` ＝ 上次续的那一秒，过期 ＝ 它 ＋ 一小时；换号头点名的 `org` 也在）写进 `dir`。
pub fn fake_creds_in(dir: &Path, stamped_s: u64) {
    std::fs::create_dir_all(dir).expect("mkdir");
    let doc = serde_json::json!({
        "session": {"bearer": "acc", "renewal": "ref", "app": "a", "grants": ["g"], "org": "o"},
        "stamped": stamped_s,
    });
    std::fs::write(dir.join("fake-login.ndjson"), doc.to_string()).expect("creds");
}
