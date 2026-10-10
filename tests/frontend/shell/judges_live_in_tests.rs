//! 壳的生产源码里不放判据：名字是 `*_registry.rs` / `*_guard.rs` / `*_ledger.rs` 的判据住 `tests/frontend/shell/`，
//! 由 `lib.rs` 以 `#[cfg(test)] #[path = "../../../../tests/frontend/shell/<名>.rs"]` 挂进来（10-10 搬过一次，33 份）。
//! 生产源码里名字撞上这三个后缀、却真是产品的，进 [`PRODUCT`] 并写理由。

use std::collections::BTreeSet;

/// 名字带这三个后缀、却是产品的：`(文件名, 为什么是产品)`。
const PRODUCT: &[(&str, &str)] = &[(
    "drift_ledger.rs",
    "运行期的账：远端回来的未知 token 按机器记下，Tauri 命令 `drift_ledger_report` 交给界面",
)];

fn judge_named(name: &str) -> bool {
    ["_registry.rs", "_guard.rs", "_ledger.rs"]
        .iter()
        .any(|s| name.ends_with(s))
}

#[test]
fn no_judge_module_lives_in_the_shell_production_tree() {
    let root = crate::guard_support::repo_root();
    let mut seen = 0usize;
    let mut named: BTreeSet<String> = BTreeSet::new();
    for tree in ["src/frontend/shell/src", "src/frontend/filewin/src"] {
        for f in guard_core::files_under(&root.join(tree)) {
            seen += 1;
            let name = f.rsplit('/').next().unwrap_or(&f).to_string();
            if judge_named(&name) {
                named.insert(name);
            }
        }
    }
    assert!(
        seen > 50,
        "两棵生产树只走到 {seen} 份文件 —— 遍历坏了，本条会零命中地绿"
    );
    let product: BTreeSet<String> = PRODUCT.iter().map(|(n, _)| n.to_string()).collect();
    let stray: Vec<&String> = named.difference(&product).collect();
    assert!(
        stray.is_empty(),
        "壳的生产源码里有判据模块：{stray:?}\n⇒ 搬到 `tests/frontend/shell/`，`lib.rs` 里改成 \
         `#[cfg(test)] #[path = \"../../../../tests/frontend/shell/<名>.rs\"] mod <名>;`；\
         它若真是产品，进本文件的 `PRODUCT` 并写理由"
    );
    let dead: Vec<&String> = product.difference(&named).collect();
    assert!(
        dead.is_empty(),
        "`PRODUCT` 里的 {dead:?} 盘上没有了 —— 删掉那一行"
    );
    // 正控：判名规则认得出三种后缀，也不误认。
    assert!(
        judge_named("a_registry.rs") && judge_named("b_guard.rs") && judge_named("c_ledger.rs")
    );
    assert!(!judge_named("guard_support.rs") && !judge_named("registry.rs"));
}
