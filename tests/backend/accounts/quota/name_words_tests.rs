//! 号名 / 位名的字只住一处：号名由 [`account_text_among`] 照号那一家写进每号那一行的 `label`，位名由 `quota-read` 成品带的 `names` 给；界面不认 `_` 与 `5h` / `7d`。
use super::*;

#[test]
fn the_names_cell_writes_both_slots_and_account_text_names_home() {
    let n = serde_json::to_value(names()).unwrap();
    let kind = crate::agents::default_kind();
    let account_text = |name: &str| account_text_of(kind, name);
    // 账号 0 ＝ 不设配置目录的那一份登录（~/.claude）；`_` ＝ 起会话时没说是哪个号（落在哪一份登录说不准，
    //   建了账号库的机器上可能是 shell rc 导出的那个配置根）⇒ 两个名字，`_` 与额度行那一格「未指定账号」同一条键。
    assert_eq!(
        account_text("0"),
        copy_core::home_short(
            &crate::agents::home_of(kind, None, false)
                .unwrap_or_default()
                .to_string_lossy(),
            crate::platform::paths::home_dir().as_deref()
        )
    );
    assert_eq!(account_text("_"), copy_text("acct.tag.home", &[]));
    assert_ne!(account_text("0"), account_text("_"));
    assert!(n.get("accounts").is_none(), "号名不在这张表里：{n}");
    assert_eq!(n["slots"]["5h"], copy_text("acct.slot.fiveHour", &[]));
    assert_eq!(n["slots"]["7d"], copy_text("acct.slot.sevenDay", &[]));
    assert_eq!(account_text("work"), "work");
}

/// 账号 0 叫那一家默认家目录的短形：换一家（家目录换了）名字就跟着换，不是写死的一句。
#[test]
fn account_zero_is_named_after_that_familys_home() {
    let mut row = crate::agents::fake::full_row(|| None);
    let mut local = row.local.expect("替身家有本机布局");
    local.home_at = |_, _| std::path::PathBuf::from("/u/.fake-agent");
    row.local = Some(local);
    let reg = [row];
    let user = std::path::Path::new("/u");
    assert_eq!(
        bare_account_text_among(&reg, Some(crate::agents::fake::AGENT_KIND), Some(user)),
        "~/.fake-agent"
    );
    // 认不出那一家 ⇒ 照实说认不出是哪一家的（不落一个写死的目录，也不退成空串）。
    assert_eq!(
        bare_account_text_among(&reg, Some("nobody"), Some(user)),
        copy_text("rsAgents.family.unknown", &[])
    );
}

/// ★ 那一家认得、却没有本机布局（说不出家目录）：账号 0 的号名照上一层同一个口径照实说，不退成空串。
#[test]
fn account_zero_of_a_family_without_local_layout_says_so() {
    let mut row = crate::agents::fake::full_row(|| None);
    row.local = None;
    let reg = [row];
    let unknown = copy_text("rsAgents.family.unknown", &[]);
    assert_eq!(
        account_text_among(&reg, Some(crate::agents::fake::AGENT_KIND), "0"),
        unknown
    );
    assert_eq!(
        bare_account_text_among(
            &reg,
            Some(crate::agents::fake::AGENT_KIND),
            Some(std::path::Path::new("/u"))
        ),
        unknown
    );
}

/// 号名照那个号那一家问：账号 0 叫**它那一家**默认家目录的短形，不叫账号库那一家的（替身家不归凭据文件）。
#[test]
fn account_zero_text_follows_the_family_it_is_asked_for() {
    let mut row = crate::agents::fake::full_row(|| None);
    let mut local = row.local.expect("替身家有本机布局");
    local.home_at = |_, _| std::path::PathBuf::from("/u/.fake-agent");
    row.local = Some(local);
    let reg = [row];
    let want = bare_account_text_among(
        &reg,
        Some(crate::agents::fake::AGENT_KIND),
        crate::platform::paths::home_dir().as_deref(),
    );
    assert!(!want.is_empty(), "正控：替身家说得出账号 0 叫什么");
    assert_eq!(
        account_text_among(&reg, Some(crate::agents::fake::AGENT_KIND), "0"),
        want
    );
    assert_eq!(
        account_text_among(&reg, Some("nobody"), "0"),
        copy_text("rsAgents.family.unknown", &[])
    );
    assert_eq!(
        account_text_among(&reg, Some(crate::agents::fake::AGENT_KIND), "work"),
        "work"
    );
}

/// ★ 认不出号是哪一家的（`None`）：账号 0 的号名要问那一家的家目录 ⇒ 照实说认不出是哪一家的（判在 `bare_account_text_among`）；
/// 具名号 · `_` 不看家，照常写。
#[test]
fn account_text_of_an_unknown_family_says_so_only_for_account_zero() {
    let reg = [crate::agents::fake::full_row(|| None)];
    assert_eq!(
        account_text_among(&reg, None, "0"),
        copy_text("rsAgents.family.unknown", &[])
    );
    assert_eq!(account_text_among(&reg, None, "work"), "work");
    assert_eq!(
        account_text_among(&reg, None, "_"),
        copy_text("acct.tag.home", &[])
    );
}

/// 后端自己拼号名 / 位名的那几处都走这里（别处不许再写一份 `"5h" =>` / `== "_"` 取字）。
#[test]
fn no_other_backend_module_maps_slots_or_home_to_words() {
    let root = guard_core::addr::backend_src_root();
    let root = root.as_path();
    let mut bad = Vec::new();
    // 摘掉那一处本身（它就是唯一住址）；判据住 tests/、不在被扫的这棵树里。
    for (p, src) in
        guard_core::scan_tree_excluding(root, &["rs"], &["accounts/quota/name_words.rs"])
    {
        let prod = guard_core::production_code(&src);
        for needle in [
            "copy_text(\"acct.slot.fiveHour\"",
            "copy_text(\"acct.slot.sevenDay\"",
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

/// ★ 金样 ＝ 核心写出来的那张位名表（界面不自己写位名）。
#[test]
fn the_names_golden_is_what_the_core_writes() {
    crate::golden::assert_golden(
        "tests/__fixtures__/names.golden.json",
        &format!("{}\n", serde_json::to_string_pretty(&names()).unwrap()),
        "号名 / 位名表与金样不一致",
    );
}
