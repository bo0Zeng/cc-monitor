//! ccm 计划判据共用的夹具：这台没有账号库的那一份账号清单成品（本包的判据与挂主 crate 的那几份共用）。

use cmd_wire::accounts::{AccountsList, ListMeta};

/// 这台没有账号库（成品 `meta.enabled = false`、一行都没有）。
pub fn no_library() -> AccountsList {
    AccountsList {
        meta: ListMeta {
            enabled: false,
            accts_dir: "/h/accts".to_string(),
            manifest_path: "/h/accts/accounts.json".to_string(),
            updated_at: None,
            shared_store: None,
            count: 0,
            error: None,
            unsupported: None,
            next_default: None,
            effective_default: None,
        },
        accounts: Vec::new(),
        notice: None,
    }
}
