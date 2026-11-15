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
            "sessionOpen.attached.body",
            &[("machine", "devbox"), ("at", "%1")]
        ),
        "devbox · %1"
    );
}

/// 表里没有 ⇒ `〔键〕`（不 panic）。
#[test]
fn a_missing_key_says_its_name_instead_of_panicking() {
    assert_eq!(copy_text("no.such.key", &[]), "〔no.such.key〕");
}

/// 〔少参当场红〕给的参数名少了一个：调试档（测试都在这一档）当场 panic，说出是哪条、少了哪个 ——
/// 不再原样留 `{at}` 让一句半成品混过判据（文件窗口两处判据就是这样漏的）。
#[test]
#[should_panic(expected = "copy_text(sessionOpen.attached.body): missing args at")]
fn a_short_call_panics_on_the_spot() {
    copy_text("sessionOpen.attached.body", &[("machine", "devbox")]);
}

/// 正控：登记的参数都给了 ⇒ 照常出句子（不是「一律 panic」）；多给的不用、也不红（共用一套参数的几处照给全集）。
#[test]
fn a_full_call_still_speaks() {
    assert_eq!(
        copy_text(
            "sessionOpen.attached.body",
            &[("at", "%1"), ("machine", "devbox"), ("host", "x")]
        ),
        "devbox · %1"
    );
}

/// `copy_static!` 取的是同一条文案、给的是 `&'static str`，同一个调用点取两次是同一块内存（住一辈子，不是每次现造）。
#[test]
fn the_static_form_is_the_same_text_and_lives_forever() {
    fn once() -> &'static str {
        crate::copy_static!("accountChip.menu.refresh")
    }
    assert_eq!(once(), copy_text("accountChip.menu.refresh", &[]));
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
    // 值里带另一个参数的占位符：原样留着（逐个 replace 的话会成 `%1 · %1`）。
    assert_eq!(
        copy_text(
            "sessionOpen.attached.body",
            &[("machine", "{at}"), ("at", "%1")]
        ),
        "{at} · %1"
    );
    // 参数给的顺序反过来也一样（顺序不是语义的一部分）。
    assert_eq!(
        copy_text(
            "sessionOpen.attached.body",
            &[("at", "{machine}"), ("machine", "devbox")]
        ),
        "devbox · {machine}"
    );
    // 值里带它自己的占位符、带花括号残片：都原样。
    assert_eq!(
        copy_text(
            "sessionOpen.attached.body",
            &[("machine", "{machine}"), ("at", "{a{b}")]
        ),
        "{machine} · {a{b}"
    );
    // 正控：两个占位符都真换了（不是「一个都不换」）。
    assert_eq!(
        copy_text(
            "sessionOpen.attached.body",
            &[("machine", "devbox"), ("at", "%1")]
        ),
        "devbox · %1"
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

/// 一句话里称呼一台：本机 ⇒ 本机那一词（与 `local_machine` 同一把键）；远端 ⇒ 它的键本身（远端的键就是给人看的名字）。
#[test]
fn a_machine_is_called_by_one_word() {
    assert_eq!(machine_word(&cmd_wire::Machine::local()), local_machine());
    assert_eq!(machine_word(&cmd_wire::Machine::named("devbox")), "devbox");
    assert_ne!(machine_word(&cmd_wire::Machine::local()), cmd_wire::LOCAL);
}

/// 那台离线那一句称呼那台（全产品只这一处取 `link.machine.offline`，扫描在 `tests/comms/inward/chan/said_tests.rs`）。
#[test]
fn the_offline_sentence_names_the_machine() {
    let s = offline("devbox");
    assert_eq!(
        s,
        copy_text("link.machine.offline", &[("machine", "devbox")])
    );
    assert!(s.contains("devbox"), "{s}");
}

/// 测试按键断言用：`copy_matches` 认一段文字里有没有那一条（占位处任意值、接缝空格可有可无）。
#[test]
fn copy_matches_finds_an_entry_whatever_its_values_are() {
    // 无参：整句在里头才算
    let plain = copy_text("detail.label.local", &[]);
    assert!(copy_matches(
        "detail.label.local",
        &format!("前缀 {plain} 后缀")
    ));
    // 有参：值换成什么都认，值挨着汉字时取文口补的那个空格也认
    let said = copy_text("peerVersion.said.old", &[("machine", "devbox")]);
    assert!(copy_matches("peerVersion.said.old", &said));
    let said_local = copy_text("peerVersion.said.old", &[("machine", &plain)]);
    assert!(copy_matches("peerVersion.said.old", &said_local));
    // 反例：缺了固定字的一截、顺序颠倒、表里没有的键，都不认
    let fixed: Vec<&str> = entries()["peerVersion.said.old"]["zh"]
        .as_str()
        .unwrap()
        .split("{machine}")
        .filter(|s| !s.trim().is_empty())
        .collect();
    let cut = said.replacen(fixed[0].trim(), "", 1);
    assert!(!copy_matches("peerVersion.said.old", &cut));
    assert!(!copy_matches("no.such.key", &said));
}

/// `copy_matches_with`：给了值的那一格要真是那个值，没给的仍是任意值。
#[test]
fn copy_matches_with_pins_the_given_values_only() {
    let said = copy_text("peerVersion.said.old", &[("machine", "devbox")]);
    assert!(copy_matches_with(
        "peerVersion.said.old",
        &[("machine", "devbox")],
        &said
    ));
    assert!(copy_matches_with("peerVersion.said.old", &[], &said));
    assert!(!copy_matches_with(
        "peerVersion.said.old",
        &[("machine", "laptop")],
        &said
    ));
    assert!(!copy_matches_with("no.such.key", &[], &said));
}

/// **时长格式化的对拍**：共用金样 `tests/__fixtures__/duration-format.golden.json` 逐条喂给 [`format_duration`]，
/// 期望是金样里手写的（界面没有这一份的孪生，只此一侧）。
#[test]
fn the_shared_duration_golden_agrees_with_this_reader() {
    let raw = include_str!("../../__fixtures__/duration-format.golden.json");
    let golden: serde_json::Value = serde_json::from_str(raw).expect("金样不是合法 JSON");
    let cases = golden["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 10, "金样只有 {} 条（反空真）", cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let ms = c["ms"].as_u64().expect("ms 不是非负整数");
            let want = c["want"].as_str().expect("want 不是字符串");
            let got = format_duration(ms);
            (got != want).then(|| format!("{ms}: {got:?} ≠ {want:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "时长格式化与金样不符：{wrong:?}");
}

/// **短时长的对拍**：共用金样 `tests/__fixtures__/short-duration.golden.json` 逐条喂给 [`short_duration`]；
/// 桌面那一侧 `tests/copy/short-duration.vitest.ts` 读同一份（`quota-lines.ts::fmtDur`）。
#[test]
fn the_shared_short_duration_golden_agrees_with_this_reader() {
    let raw = include_str!("../../__fixtures__/short-duration.golden.json");
    let golden: serde_json::Value = serde_json::from_str(raw).expect("金样不是合法 JSON");
    let cases = golden["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 10, "金样只有 {} 条（反空真）", cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let ms = c["ms"].as_u64().expect("ms 不是非负整数");
            let want = c["want"].as_str().expect("want 不是字符串");
            let got = short_duration(ms);
            (got != want).then(|| format!("{ms}: {got:?} ≠ {want:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "短时长与金样不符：{wrong:?}");
}

/// **距今的对拍**：同一份金样的 `rel` 段喂给 [`rel_duration`]（核心写 `…RelText` 的那一处）。
#[test]
fn the_shared_rel_golden_agrees_with_this_reader() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/short-duration.golden.json"
    ))
    .unwrap();
    let cases = golden["rel"].as_array().expect("金样缺 rel");
    assert!(cases.len() >= 8, "rel 只有 {} 条（反空真）", cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let ms = c["aheadMs"].as_i64().expect("aheadMs");
            let want = c["want"].as_str().map(str::to_string);
            let got = crate::rel_duration(ms);
            (got != want).then(|| format!("{ms}: {got:?} ≠ {want:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "距今与金样不符：{wrong:?}");
}

/// **大小的金样**：`tests/__fixtures__/size-text.golden.json` 逐条喂给 [`crate::size_text`]（界面照后端写好的那一格放，不另算）。
#[test]
fn the_shared_size_golden_agrees_with_this_reader() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/size-text.golden.json")).unwrap();
    let cases = golden["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 10, "只有 {} 条（反空真）", cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let n = c["bytes"].as_u64().expect("bytes");
            let got = crate::size_text(n);
            (got != c["want"].as_str().unwrap()).then(|| format!("{n}: {got:?} ≠ {}", c["want"]))
        })
        .collect();
    assert!(wrong.is_empty(), "大小与金样不符：{wrong:?}");
}

/// **计数的金样**：`tests/__fixtures__/count-text.golden.json` 逐条喂给 [`crate::count_text`]（界面不另数）。
#[test]
fn the_shared_count_golden_agrees_with_this_reader() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/count-text.golden.json")).unwrap();
    let cases = golden["cases"].as_array().expect("金样缺 cases");
    assert!(cases.len() >= 8, "只有 {} 条（反空真）", cases.len());
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|c| {
            let n = c["n"].as_u64().expect("n");
            let got = crate::count_text(n);
            (got != c["want"].as_str().unwrap()).then(|| format!("{n}: {got:?} ≠ {}", c["want"]))
        })
        .collect();
    assert!(wrong.is_empty(), "计数与金样不符：{wrong:?}");
}

/// 短写的计数（`1% ≈ 2.1M` 那一格）：不满一千写整数；往上按千进位到 K · M · B，不满 10 带一位小数、否则取整（都四舍五入），
/// 舍到 1000 的进一档。
#[test]
fn short_counts_round_into_k_m_b() {
    for (n, want) in [
        (0, "0"),
        (999, "999"),
        (1_000, "1.0K"),
        (9_949, "9.9K"),
        (9_950, "10K"),
        (25_000, "25K"),
        (999_499, "999K"),
        (999_500, "1.0M"),
        (2_100_000, "2.1M"),
        (1_500_000_000, "1.5B"),
        (u64::MAX, "18446744074B"),
    ] {
        assert_eq!(crate::count_short(n), want, "{n}");
    }
}

/// 断着 ⇒ 写类为什么做不了（唯一一处）：那台断着说离线 · 只读；与 cc-monitor 断开说那一段，不说那台离线；连着 ⇒ 不拦。
#[test]
fn a_cut_says_which_stretch_is_down_and_a_live_link_says_nothing() {
    use cmd_wire::files::Cut;
    assert_eq!(
        peer::write_refused(Some(Cut::Monitor)),
        Some(copy_text("rsFilewinChrome.link.severedReadOnly", &[]))
    );
    assert_eq!(
        peer::write_refused(Some(Cut::Machine)),
        Some(copy_text("link.machine.readOnly", &[]))
    );
    assert_ne!(
        peer::write_refused(Some(Cut::Monitor)),
        peer::write_refused(Some(Cut::Machine)),
        "两段断开说的是同一句"
    );
    assert_eq!(peer::write_refused(None), None);
}
