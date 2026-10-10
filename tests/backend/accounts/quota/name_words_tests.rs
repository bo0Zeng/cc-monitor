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
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                if !p.ends_with("target") {
                    stack.push(p);
                }
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) != Some("rs")
                || p.ends_with("accounts/quota/name_words.rs")
            {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap_or_default();
            for needle in [
                "copy_text(\"acct.slot.fiveHour\"",
                "copy_text(\"acct.slot.sevenDay\"",
                "copy_text(\"acct.home.name\"",
            ] {
                if src.contains(needle) {
                    bad.push(format!(
                        "{} · {needle}",
                        p.strip_prefix(root).unwrap().display()
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "号名 / 位名在别处又取了一次字：{bad:#?}");
}
