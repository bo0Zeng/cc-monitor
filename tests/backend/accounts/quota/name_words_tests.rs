//! 号名 / 位名的字只住一处：四件出号名的成品都带 `names`，界面照它画、不认 `_` 与 `5h` / `7d`。
use super::*;

#[test]
fn the_names_cell_writes_home_and_both_slots() {
    let n = names_cell();
    assert_eq!(n["accounts"]["_"], copy_text("acct.home.name", &[]));
    assert_eq!(
        n["accounts"].as_object().unwrap().len(),
        1,
        "只列与原名不同的号"
    );
    assert_eq!(n["slots"]["5h"], copy_text("acct.slot.fiveHour", &[]));
    assert_eq!(n["slots"]["7d"], copy_text("acct.slot.sevenDay", &[]));
    assert_eq!(account_text("work"), "work");
}

/// 后端自己拼号名 / 位名的那几处都走这里（别处不许再写一份 `"5h" =>` / `== "_"` 取字）。
#[test]
fn no_other_backend_module_maps_slots_or_home_to_words() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bad = Vec::new();
    // 摘掉那一处本身（它就是唯一住址）；判据住 tests/、不在被扫的这棵树里。
    for (p, src) in
        guard_core::scan_tree_excluding(root, &["rs"], &["accounts/quota/name_words.rs"])
    {
        let prod = crate::guard_support::production_code(&src);
        for needle in [
            "copy_text(\"acct.slot.fiveHour\"",
            "copy_text(\"acct.slot.sevenDay\"",
            "copy_text(\"acct.home.name\"",
        ] {
            if prod.contains(needle) {
                bad.push(format!(
                    "{} · {needle}",
                    p.strip_prefix(root).unwrap_or(&p).display()
                ));
            }
        }
    }
    assert!(bad.is_empty(), "号名 / 位名在别处又取了一次字：{bad:#?}");
}

/// ★ 金样 ＝ 核心写出来的那张表（界面的判据照它喂 `takeNames`，不自己写「默认号」那个字）。
/// 重写：`CCM_REGEN_NAMES_GOLDEN=1 cargo test --lib -- name_words`。
#[test]
fn the_names_golden_is_what_the_core_writes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/names.golden.json");
    let got = format!("{}\n", serde_json::to_string_pretty(&names_cell()).unwrap());
    if std::env::var_os("CCM_REGEN_NAMES_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        got == want,
        "号名 / 位名表与金样不一致（CCM_REGEN_NAMES_GOLDEN=1 重写）：\n{got}"
    );
}
