//! 复制详情那几行：只列有值的项 · 原话截断 · 时刻写法 · 项名闭集与文案表两向相等。

use super::*;

#[test]
fn only_items_with_values_render_one_per_line() {
    let d = Detail::new()
        .item(Label::At, "2026-10-08 14:32:07 +08:00")
        .item(Label::Path, "   ")
        .maybe(Label::Target, None::<&str>)
        .item(Label::Code, "kill_failed")
        .item(Label::Raw, "  can't find window\n");
    assert_eq!(
        d.render(),
        "时刻：2026-10-08 14:32:07 +08:00\n码：kill_failed\n原话：can't find window"
    );
    assert_eq!(Detail::new().render(), "");
}

#[test]
fn raw_is_cut_at_the_cap_on_a_char_boundary() {
    let long = "错".repeat(RAW_CAP); // 每字 3 字节
    let cut = truncate_raw(&long);
    let tail = copy_text("detail.value.truncated", &[]);
    assert!(cut.ends_with(&tail));
    let body = &cut[..cut.len() - tail.len()];
    assert!(body.len() <= RAW_CAP && body.len() > RAW_CAP - 3);
    assert_eq!(truncate_raw("short"), "short");
}

#[test]
fn a_written_detail_is_kept_whole_and_more_items_follow_it() {
    // 对端写好的一整份（原样一块）后面再接一项：本机壳给远端那份补「本机」那一行。
    let d = Detail::new()
        .block("码：x")
        .item(Label::Local, "cc-monitor 1");
    assert_eq!(d.render(), "码：x\n本机：cc-monitor 1");
    assert_eq!(
        Detail::new().block("").item(Label::Local, "a").render(),
        "本机：a"
    );
    assert_eq!(
        Detail::new()
            .block("码：x")
            .item(Label::Local, " ")
            .render(),
        "码：x"
    );
    assert!(Detail::new().block("码：x").has_block());
    assert!(!Detail::new().block("  ").has_block(), "空的一块不算");
}

/// 补一项按项名次序（`Label::ALL`）插：不拆渲染好的字，原话里恰好有一行以项名打头也插不错位。
#[test]
fn inserting_an_item_goes_by_label_order_not_by_parsing_lines() {
    let raw = "first\n码：not a code line\n命令：not a command line";
    let d = Detail::new()
        .item(Label::At, "t")
        .item(Label::Local, "cc-monitor 1")
        .item(Label::Code, "io_failed")
        .item(Label::Raw, raw)
        .insert(Label::Command, "open_log_dir");
    assert_eq!(
        d.render(),
        format!("时刻：t\n本机：cc-monitor 1\n命令：open_log_dir\n码：io_failed\n原话：{raw}")
    );
    assert!(d.has(Label::Command) && !d.has(Label::Path));
    // 没有后排项 ⇒ 接在末尾；空值不插。
    assert_eq!(
        Detail::new()
            .item(Label::At, "t")
            .insert(Label::Command, "c")
            .render(),
        "时刻：t\n命令：c"
    );
    assert_eq!(
        Detail::new()
            .item(Label::At, "t")
            .insert(Label::Command, " ")
            .render(),
        "时刻：t"
    );
}

/// 通道这一跳断了那一份：时刻 · 机器（没发出去标「未连上」）· 本机（可缺）· 命令 · 断在（可缺）· 码 —— 壳与文件窗口同一份。
#[test]
fn a_channel_detail_is_one_shape() {
    let nc = copy_text("detail.value.notConnected", &[]);
    assert_eq!(
        channel("t", "devbox", true, Some("cc-monitor 1"), "files-ls", Some("1:open NotSent"), "Unreachable").render(),
        format!("时刻：t\n机器：devbox（{nc}）\n本机：cc-monitor 1\n命令：files-ls\n断在：1:open NotSent\n码：Unreachable")
    );
    assert_eq!(
        channel("t", "本机", false, None, "kill", None, "Broken").render(),
        "时刻：t\n机器：本机\n命令：kill\n码：Broken"
    );
}

/// 一行汇总底下几件各自的失败 ⇒ 复制出去的整段（主界面合流 ×N 与文件窗口进度那一行同一排法，跨语言金样 `detail-many.golden.json`）。
#[test]
fn many_segments_follow_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/detail-many.golden.json")).unwrap();
    let cases = g["cases"].as_array().unwrap();
    assert!(cases.len() >= 3);
    for c in cases {
        let segs: Vec<(String, String)> = c["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                (
                    s[0].as_str().unwrap().to_string(),
                    s[1].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let got = many(c["head"].as_str().unwrap(), &segs);
        assert_eq!(got.as_deref(), c["body"].as_str(), "{c}");
    }
}

/// 一件失败复制出去的整段（CLI 的 `--text` 失败那一形）：与界面 `detailBody` 的单件那一支读同一份金样的 `one`。
#[test]
fn one_failure_follows_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/detail-many.golden.json")).unwrap();
    let cases = g["one"].as_array().unwrap();
    assert!(cases.len() >= 2);
    for c in cases {
        let got = one(c["said"].as_str().unwrap(), c["detail"].as_str().unwrap());
        assert_eq!(Some(got.as_str()), c["body"].as_str(), "{c}");
    }
}

#[test]
fn stamp_writes_local_time_with_offset() {
    // 2026-10-08 06:32:07 UTC
    let t = 1_791_440_000 - 1_791_440_000 % 86_400 + 6 * 3600 + 32 * 60 + 7;
    let day = copy_core_day(t);
    assert_eq!(stamp(t, 8 * 3600), format!("{day} 14:32:07 +08:00"));
    assert_eq!(
        stamp(t, -(5 * 3600 + 30 * 60)),
        format!("{day} 01:02:07 -05:30")
    );
    assert_eq!(stamp(0, 0), "1970-01-01 00:00:00 +00:00");
}

fn copy_core_day(t: i64) -> String {
    let (y, m, d) = crate::civil_from_days(t.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

#[test]
fn labels_are_a_closed_set_equal_to_the_copy_table() {
    let table: serde_json::Value = serde_json::from_str(crate::TABLE_JSON).unwrap();
    let mut in_table: Vec<String> = table["entries"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("detail.label."))
        .cloned()
        .collect();
    in_table.sort();
    let mut said: Vec<String> = Label::ALL.iter().map(|l| l.said()).collect();
    let mut want: Vec<String> = in_table
        .iter()
        .map(|k| table["entries"][k]["zh"].as_str().unwrap().to_string())
        .collect();
    said.sort();
    want.sort();
    assert_eq!(said, want, "Label 闭集 ≠ 文案表 detail.label.*");
    assert_eq!(in_table.len(), Label::ALL.len());
}

/// 读回别处写好的详情：项名起项、续行跟上一项；「原话」之后的行一律算原话（原话里以项名打头的行不被拆走）；读回再排一字不差。
#[test]
fn a_written_detail_parses_back_into_items() {
    let w = "时刻：t\n机器：m\n码：x\n原话：line one\n命令：not a command\n码：not a code";
    let d = Detail::parse(w);
    assert_eq!(d.render(), w);
    assert!(d.has(Label::Code) && d.has(Label::Raw) && !d.has(Label::Command));
    assert_eq!(
        d.value(Label::Raw),
        Some("line one\n命令：not a command\n码：not a code")
    );
    let d = d.insert(Label::Command, "c");
    assert_eq!(
        d.render(),
        "时刻：t\n机器：m\n命令：c\n码：x\n原话：line one\n命令：not a command\n码：not a code"
    );
    assert!(Detail::parse("random words\n码：x").has_block());
}

#[test]
fn a_session_target_is_the_machine_and_a_short_sid_never_the_title() {
    // 会话标题常常就是用户的第一句话，详情不含会话内容 ⇒ 对象那一项只写机器 ＋ sid 前 8 位。
    let sid = "0000aaaa-0000-4000-8000-000000000001";
    let d = Detail::new().target(Target::Session {
        machine: "devbox",
        sid,
    });
    assert_eq!(d.render(), "对象：devbox · 0000aaaa");
    assert_eq!(
        Detail::new().target(Target::Account("work")).render(),
        "对象：work"
    );
}

#[test]
fn the_target_item_takes_no_free_text() {
    // 自由文本进不了「对象」：item / insert 拿 Label::Target 当场拒，只能经 Detail::target（只收标识）。
    let title = "帮我把登录页的报错改一下";
    let puts: [fn(&'static str) -> Detail; 3] = [
        |t| Detail::new().item(Label::Target, t),
        |t| Detail::new().insert(Label::Target, t),
        |t| Detail::new().maybe(Label::Target, Some(t)),
    ];
    for put in puts {
        assert!(
            std::panic::catch_unwind(|| put(title)).is_err(),
            "对象那一项收了自由文本"
        );
    }
}
