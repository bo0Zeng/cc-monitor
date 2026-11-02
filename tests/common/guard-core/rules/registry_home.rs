//! 登记表（`*_registry*.rs`）住 `tests/common/guard-core/rules/`，挂在本模块底下，`cargo test -p guard-core` 跑到每一张。
//! 壳那棵只留够得着壳 crate 内部的那几张（[`SHELL_STAYS`]，逐行理由）。
//!
//! 认法：壳 crate 根（`src/frontend/shell/src/lib.rs`）上一行 `#[path = "…"]` 挂的文件名里带 `_registry` ⇒ 挂在壳上；
//! `rules/` 里每一张表头（文件名以 `_registry.rs` 收尾）都要在 `rules/mod.rs` 里挂上。

use super::{equal, rule, RULES_DIR};
use std::collections::BTreeSet;

const SHELL_ROOT: &str = "src/frontend/shell/src/lib.rs";
const SHELL_TESTS: &str = "tests/frontend/shell";

/// 留在壳上的登记表（表头文件名）· 为什么留。
const SHELL_STAYS: &[(&str, &str)] = &[(
    "command_home_registry.rs",
    "读壳 crate 内部两样：`chan::host::HOST_OPS`（由文件窗口契约与 `transfer_relay` 的常数拼成）与 `parity_ledger` 的 `LEDGER` · `Own`（壳那边的测试模块）",
)];

fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// 一张表的表头：`<名>_registry.rs`，名里不再带 `_registry_`（那是从表头拆出去、由表头自己挂的一份）。
fn is_head(file: &str) -> bool {
    file.strip_suffix("_registry.rs")
        .is_some_and(|stem| !stem.contains("_registry_"))
}

#[test]
fn registries_live_under_guard_core_rules() {
    rule(|repo| {
        let stays: BTreeSet<&str> = SHELL_STAYS.iter().map(|(f, _)| *f).collect();
        for (f, why) in SHELL_STAYS {
            assert!(why.chars().count() >= 8, "`{f}` 的理由写得太短：{why:?}");
        }

        let mounted_on_shell: BTreeSet<&str> = repo
            .file(SHELL_ROOT)
            .text()
            .lines()
            .filter_map(|l| crate::path_attr(l.trim()))
            .map(base)
            .filter(|f| f.contains("_registry"))
            .collect();
        equal("壳 crate 根上挂的登记表", mounted_on_shell, stays.clone());

        let left_in_shell: BTreeSet<&str> = repo
            .under(&[SHELL_TESTS], &["rs"])
            .into_iter()
            .map(|f| base(&f.rel))
            .filter(|f| is_head(f))
            .collect();
        equal("壳测试目录里的登记表表头", left_in_shell, stays);

        let rules_dir = RULES_DIR.trim_end_matches('/');
        let heads: BTreeSet<&str> = repo
            .with_rules()
            .under(&[rules_dir], &["rs"])
            .into_iter()
            .map(|f| base(&f.rel))
            .filter(|f| is_head(f))
            .collect();
        let mounted_here: BTreeSet<&str> = repo
            .file(&format!("{RULES_DIR}mod.rs"))
            .text()
            .lines()
            .filter_map(|l| crate::path_attr(l.trim()))
            .filter(|f| is_head(f))
            .collect();
        assert!(
            !heads.is_empty(),
            "`{RULES_DIR}` 里一张登记表都没有 —— 判不了"
        );
        equal(
            "`rules/` 里的登记表 ↔ `rules/mod.rs` 挂上的",
            heads,
            mounted_here,
        );
    });
}
