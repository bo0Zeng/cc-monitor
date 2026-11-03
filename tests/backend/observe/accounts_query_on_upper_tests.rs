//! 账号清单判据里引到上层的那两条：起会话跟随读清单成品（控制面 `launch_account::pick`）· 换模型那一句（帧面 `session_model_face`）。
//! 观测面是独立的包（`backend-observe`）、够不到上层 ⇒ 挂在主 crate（经 `cross/observe.rs`）；账号库经 `accounts_query::test_support` 造。

use crate::observe::accounts_query::test_support::{
    account_default_golden, fresh_dir, golden_library, write_manifest,
};
use std::fs;
use std::path::Path;

/// ★★ 起会话跟随（`launch_account::pick`）读的就是清单成品：没有上次的号 ⇒ 落默认号（选得了）或不指定号。
/// 期望取自跨三处金样（`tests/__fixtures__/account-default.golden.json`）那一格的 `selectable` · `effectiveDefault`。
#[test]
fn following_with_no_last_account_lands_on_the_default_when_it_is_selectable() {
    use crate::control::launch_account::{pick, AccountAsk, Settled};
    let golden = account_default_golden();
    let cases = golden["cases"].as_array().unwrap();
    assert!(cases.len() >= 6, "金样被删薄了");
    for (i, case) in cases.iter().enumerate() {
        let what = case["what"].as_str().unwrap();
        let (root, accts) = golden_library(&format!("pick{i}"), case);
        let typed = crate::observe::accounts_query::list_product_with(
            &accts,
            None,
            &[],
            &[],
            "claude-code",
            "claude-code",
            false,
        );
        let selectable: Vec<&str> = case["selectable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let picked = pick(&AccountAsk::Follow, &typed, None).expect("跟随没有上次的号不会拒");
        let want_pick = case["effectiveDefault"]
            .as_str()
            .filter(|d| selectable.contains(d));
        match (picked, want_pick) {
            (Settled::Account(a), Some(d)) => assert_eq!(a.name, d, "{what}：跟随落的号"),
            (Settled::Unsaid, None) => {}
            (p, w) => panic!("{what}：跟随落成 {p:?}，金样要 {w:?}"),
        }
        let _ = fs::remove_dir_all(&root);
    }
}

/// ★ 换模型写的那份设置共不共用，按**这条会话用的那个号**判：a · b 两个号链回共享库同一份、c 自己一份 ⇒
/// a · b 说「各号新会话的默认」，c 说「此号」；都还没有这份文件 · 没有账号库 ⇒「此号」。
#[cfg(unix)]
#[test]
fn whether_the_settings_are_shared_is_judged_for_the_session_s_own_account() {
    let root = fresh_dir("shared-settings");
    let accts = root.join("accts");
    let shared = root.join("shared");
    let (a, b, c) = (accts.join("a"), accts.join("b"), accts.join("c"));
    for d in [&shared, &a, &b, &c] {
        fs::create_dir_all(d).unwrap();
    }
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"accounts":[
                {{"name":"a","configDir":"{a}"}},
                {{"name":"b","configDir":"{b}"}},
                {{"name":"c","configDir":"{c}"}}]}}"#,
            a = a.display(),
            b = b.display(),
            c = c.display()
        ),
    );
    let said = |home: &Path, accts: &Path| {
        crate::faces::session_model_face::settled(
            Some(crate::agents::ModelSaid {
                name: "Sonnet 5.5".into(),
                kept: false,
                effort: None,
            }),
            crate::observe::accounts_query::shared_with_other_accounts_in(
                accts,
                home,
                "settings.json",
            ),
        )
        .said
        .unwrap()
    };
    let m = [("model", "Sonnet 5.5")];
    let (each, own) = (
        copy_core::copy_text("beSessionModel.set.savedShared", &m),
        copy_core::copy_text("beSessionModel.set.saved", &m),
    );
    assert_eq!(said(&a, &accts), own, "都还没有这份文件就算共用了");
    fs::write(shared.join("settings.json"), "{}").unwrap();
    for d in [&a, &b] {
        std::os::unix::fs::symlink(shared.join("settings.json"), d.join("settings.json")).unwrap();
    }
    fs::write(c.join("settings.json"), "{}").unwrap();
    assert_eq!(said(&a, &accts), each, "a 与 b 共用一份，a 没说「各号」");
    assert_eq!(said(&b, &accts), each, "a 与 b 共用一份，b 没说「各号」");
    assert_eq!(said(&c, &accts), own, "c 自己一份，却说成了「各号」");
    assert_eq!(said(&a, &root.join("none")), own, "没有账号库也算共用了");
    let _ = fs::remove_dir_all(&root);
}
