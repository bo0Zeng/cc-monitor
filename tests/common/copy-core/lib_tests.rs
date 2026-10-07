//! `copy-core`（对外文案表的 Rust 取文口）的判据。
//!
//! 要求：「**所有对外文案与报错都从一张表来**」；决定 2
//! 「一份文件，两侧各读，零转换」。「表 ↔ 引用」两向相等住 `tests/copy/copy-table.vitest.ts`。

use super::*;

/// 内嵌的就是盘上那一份（同一个文件，不是副本），而且解析得出条目（反空真：0 条时每一句都成了 ``）。
#[test]
fn it_embeds_the_one_table_on_disk() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/copy/table.json"),
    )
    .expect("读不到 src/shared/copy/table.json");
    assert_eq!(raw, TABLE_JSON, "内嵌的那一份与盘上那一份不是同一份");
    assert!(!entries().is_empty(), "表解析出 0 条");
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "devbox"), ("target", "%1")]
        ),
        "预览画面 · [devbox] tmux: %1"
    );
}

/// 表里没有 ⇒ ``（不 panic）；没给的占位符原样留着。
#[test]
fn a_missing_key_says_its_name_instead_of_panicking() {
    assert_eq!(copy_text("no.such.key", &[]), "〔no.such.key〕");
    assert_eq!(
        copy_text("panePreview.head.title", &[("origin", "devbox")]),
        "预览画面 · [devbox] tmux: {target}"
    );
}

/// `copy_static!` 取的是同一条文案、给的是 `&'static str`，同一个调用点取两次是同一块内存（住一辈子，不是每次现造）。
#[test]
fn the_static_form_is_the_same_text_and_lives_forever() {
    fn once() -> &'static str {
        crate::copy_static!("panePreview.head.refresh")
    }
    assert_eq!(once(), copy_text("panePreview.head.refresh", &[]));
    assert!(
        std::ptr::eq(once(), once()),
        "同一个调用点取两次不是同一块内存"
    );
}

/// 〔「前端读口 `copy-table.ts::copyText`；Rust 读口只有一份实现 `copy-core::copy_text`」〕
/// **两个读口的插值对拍**：共用金样 `tests/__fixtures__/copy-interpolation.golden.json` 逐条喂给本读口，
/// 期望是金样里**手写**的（TS 那一侧 `tests/copy/copy-table.vitest.ts` 读同一份跑 `copyText`）⇒ 两侧各对金样，不是彼此对拍。
/// 只收合法插值；两侧有意不同的那几形（缺键 · 参数对不上）登记在金样 `_differences`，不在这里。
/// 「值里含别的占位符」那一形原先也登记在那里（本读口逐个 `replace` 会把值再换一遍）；改成单趟之后两侧一致，挪进 `cases`。
#[test]
fn the_shared_interpolation_golden_agrees_with_this_reader() {
    let raw = include_str!("../../__fixtures__/copy-interpolation.golden.json");
    let g: serde_json::Value = serde_json::from_str(raw).expect("金样不是合法 JSON");
    let cases = g["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 5, "金样只有 {} 条 —— 读坏了", cases.len());
    let mut wrong = Vec::new();
    for c in cases {
        let key = c["key"].as_str().expect("key 是字符串");
        let zh = entries()
            .get(key)
            .and_then(|e| e.get("zh"))
            .and_then(|z| z.as_str())
            .unwrap_or_else(|| panic!("金样里的 {key} 不在表里"));
        assert_eq!(
            zh,
            c["zh"].as_str().expect("zh 是字符串"),
            "{key} 在表里的原文变了 —— 照新句子改金样的 zh 与 want"
        );
        let args: Vec<(String, String)> = c["args"]
            .as_object()
            .expect("args 是对象")
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().expect("参数是字符串").to_string()))
            .collect();
        let pairs: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let got = copy_text(key, &pairs);
        if got != c["want"].as_str().expect("want 是字符串") {
            wrong.push(format!("{key}: {got:?}"));
        }
    }
    assert_eq!(wrong, Vec::<String>::new(), "Rust 读口与插值金样对不上");
}

/// 〔插值不该重新解释值〕**单趟**：值里带的 `{名}` 不再被扫；没给的占位符原样留；
/// 一个不成对 / 不是给了值的 `{` 原样留着、后面的占位符照认。正反各一格（只断「不重扫」的话，把插值焊成「一个都不换」也能绿）。
#[test]
fn the_interpolation_is_one_pass_and_never_rescans_a_value() {
    // 值里带另一个参数的占位符：原样留着（先前逐个 replace ⇒ `[%1] tmux: %1`）。
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "{target}"), ("target", "%1")]
        ),
        "预览画面 · [{target}] tmux: %1"
    );
    // 参数给的顺序反过来也一样（先前的写法只在一个方向上重扫 —— 顺序不该是语义的一部分）。
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("target", "{origin}"), ("origin", "devbox")]
        ),
        "预览画面 · [devbox] tmux: {origin}"
    );
    // 值里带它自己的占位符、带花括号残片：都原样。
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "{origin}"), ("target", "{a{b}")]
        ),
        "预览画面 · [{origin}] tmux: {a{b}"
    );
    // 正控：两个占位符都真换了（不是「一个都不换」）。
    assert_eq!(
        copy_text(
            "panePreview.head.title",
            &[("origin", "devbox"), ("target", "%1")]
        ),
        "预览画面 · [devbox] tmux: %1"
    );
}

/// 因对方版本说不成的两个码各一句：取的是 `table.json` 里那两条（与界面 `chan-caller.ts::peerVersionSaid` 同键），本机说「本机」。
#[test]
fn the_two_peer_version_codes_each_say_their_one_line() {
    let entry = |k: &str| {
        entries()[k]["zh"]
            .as_str()
            .expect("table.json 里缺这一条")
            .to_string()
    };
    assert_eq!(
        backend_old("devbox"),
        entry("peerVersion.said.old").replace("{machine}", "devbox")
    );
    assert_eq!(
        reply_unreadable("devbox"),
        entry("peerVersion.said.unreadable").replace("{machine}", "devbox")
    );
    assert!(
        !reply_unreadable("devbox").contains("版本"),
        "认不出那一句不猜版本"
    );
    assert!(backend_old(&local_machine()).starts_with("本机"));
}
