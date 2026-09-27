//! # 要求住址：`设计/71 §2.2`（ccm argv 的组合规则一条都不许静默忽略）＋ `设计/01 §5 D1` / `D2`
//!
//! 核原文：`设计/71 §2.2` 逐字「后端的纪律是「一条都不许静默忽略」（`control/ccm/argv.rs`）」，同节 V1–V3 三种互斥 / 依赖组合
//! 就是 `the_combination_rules_all_fail_loudly` 逐条断言的那几种。`the_ccm_argv_is_parsed_in_exactly_one_place` 对 `D1`「一个判定只有一个家」；
//! `every_default_lives_only_in_the_defaults_block` 对 `D2`「一个数只有一个住址」。〔JA1 点址 2026-09-24〕

use super::*;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn ok(a: &[&str]) -> Opts {
    match parse(&v(a)).expect("该解析得动") {
        Parsed::Opts(o) => *o,
        other => panic!("期望拿到 Opts，实得 {other:?}"),
    }
}

fn err(a: &[&str]) -> String {
    match parse(&v(a)) {
        Err(Die(m)) => m,
        other => panic!("期望 die，实得 {other:?}"),
    }
}

/// 〔V138〕ccm 只看不吃 `--resume` / `--continue`：几种写法都原样进透传、顺序不变（从前三种写法落成 ccm 的 resume 动作）。
#[test]
fn the_ways_to_say_resume_all_reach_claude_untouched() {
    for a in [
        v(&["--resume", "abc-123"]),
        v(&["--resume=abc-123"]),
        v(&["-r", "abc-123"]),
        v(&["--continue"]),
        v(&["-c"]),
        v(&["resume", "abc-123"]),
    ] {
        assert_eq!(
            ok(&a.iter().map(String::as_str).collect::<Vec<_>>()).passthru,
            a,
            "写法 {a:?} 被 ccm 改了"
        );
    }
    // 壳层选项夹在中间照认，其余按原顺序交出去。
    let o = ok(&[
        "--resume",
        "s1",
        "--ccm-tmux",
        "--model",
        "opus",
        "-p",
        "hi",
    ]);
    assert!(o.use_tmux);
    assert_eq!(
        o.passthru,
        v(&["--resume", "s1", "--model", "opus", "-p", "hi"])
    );
}

/// 〔V138〕`--attach <名>` 取值时不许把下一个旗标吞成名字（从前位置动作 `attach <名>` 那一条的同形）。
#[test]
fn the_attach_option_never_swallows_the_next_flag_as_its_value() {
    assert!(err(&["--attach", "--ccm-tmux"]).contains("--attach"));
    assert!(err(&["--attach"]).contains("--attach"));
    assert_eq!(ok(&["--attach", "cc-foo"]).attach_name, "cc-foo");
}

/// 〔搬自 `ccm-cli`「未知 agent 报错」「--account 与 --base 互斥」〕
#[test]
fn the_combination_rules_all_fail_loudly() {
    assert!(err(&["--ccm-agent", "gemini"]).starts_with("未知 agent: gemini"));
    assert_eq!(
        err(&["--account", "z", "--base"]),
        "--account 与 --base 互斥"
    );
    assert!(err(&["--detach"]).starts_with("--detach 只能和 --ccm-tmux 一起用"));
    assert!(err(&["--tmux-size", "1x1"]).starts_with("--tmux-size 需要配合 --ccm-tmux"));
    assert!(
        err(&["--ccm-tmux=a", "--tmux-base", "b"]).starts_with("--ccm-tmux=<名> 与 --tmux-base")
    );
    assert!(err(&["--ccm-tmux", "--bus-register"]).starts_with("--bus-register 需要配合 --detach"));
    assert!(err(&["--bus-note", "x"]).starts_with("--bus-note 需要配合 --bus-register"));
    // V138：从前报「未知选项 / 多余的位置参数」的这几形，今天原样交给 agent。
    assert_eq!(ok(&["--nope", "foo"]).passthru, v(&["--nope", "foo"]));
    assert_eq!(
        ok(&["--ccm-agent", "codex", "resume", "s"]).passthru,
        v(&["resume", "s"])
    );
    // 用户 09-26：claude 自己的 `--tmux` / `--agent` 原样交出去；首词 `new` 是 ccm 的，别处的 `new` / 位置词 `attach` 交给 claude。
    assert_eq!(
        ok(&["--tmux", "--agent", "x"]).passthru,
        v(&["--tmux", "--agent", "x"])
    );
    assert!(ok(&["new", "--ccm-tmux"]).passthru.is_empty());
    assert_eq!(ok(&["-p", "new"]).passthru, v(&["-p", "new"]));
    assert_eq!(ok(&["attach", "abc"]).passthru, v(&["attach", "abc"]));
}

/// 〔搬自 `ccm-cli`「非法 --tmux-size」那一格 —— 它是一条**注入面**，不是排版〕
#[test]
fn the_size_is_two_plain_decimals_or_it_is_refused() {
    assert_eq!(parse_size("220x50"), Some(("220".into(), "50".into())));
    for bad in [
        "x50", "220x", "1x2x3", "", "22 0x50", "220X50", "-1x2", "a x b",
    ] {
        assert!(parse_size(bad).is_none(), "'{bad}' 不该被当成合法尺寸");
    }
    assert!(err(&["--ccm-tmux", "--tmux-size", "x50"]).starts_with("非法 --tmux-size"));
}

/// 〔搬自 `ccm-cli`「`-- 之后透传给 agent`」〕`--` 之后的壳层选项名也交给 agent（claude 自己的 `--tmux` 走这条）。
#[test]
fn everything_after_the_terminator_goes_to_the_agent_untouched() {
    let o = ok(&["--", "-p", "hi there", "--ccm-tmux"]);
    assert_eq!(o.passthru, v(&["-p", "hi there", "--ccm-tmux"]));
    assert!(
        o.use_tmux == Defaults::USE_TMUX,
        "`--` 之后的 --ccm-tmux 不许被本层认走"
    );
}

/// 🔴 `KR48D4` 的机检：**每个默认值只许有一处住址。**
///
/// 判法不是「数一数」，是**真去比**：`parse(&[])` 的结果必须逐个字段等于
/// [`Defaults`] 声明的那些值。任何人在别处再写一份默认（比如在 `plan.rs` 里
/// `if agent.is_empty() { agent = "claude" }`），只要那份与这里分叉，这条就红。
#[test]
fn every_default_lives_only_in_the_defaults_block() {
    let o = ok(&[]);
    assert_eq!(o.agent, Defaults::AGENT);
    assert_eq!(o.cwd_spec, Defaults::CWD);
    assert_eq!(o.use_tmux, Defaults::USE_TMUX);
    assert_eq!(o.use_base, Defaults::USE_BASE);
    assert_eq!(o.detach, Defaults::DETACH);
    assert_eq!(o.print, Defaults::PRINT);
    assert_eq!(o.bus_register, Defaults::BUS_REGISTER);
    assert!(o.passthru.is_empty() && o.attach_name.is_empty());
    // 反向：把默认值本身换掉，上面那一族必须跟着动 —— 否则它们是自说自话。
    assert_ne!(Defaults::AGENT, "", "默认 agent 是空串的话这条判据就是空真");
}

/// 🔴 `KR48D2` 的机检：**这套 argv 的解析只许在本文件里发生。**
///
/// 失效方向（`KR48D2` 逐字）：「有人再起第二个 argv 解析口、或把某条子命令的行为
/// 在第二处重写一遍」。判法 = 扫本模块**除本文件外**的生产段，
/// 不许出现任何 ccm 旗标的字面量。
#[test]
fn the_ccm_argv_is_parsed_in_exactly_one_place() {
    let others: &[(&str, &str)] = &[
        (
            "control/ccm/mod.rs",
            include_str!("../../../../src/backend/control/ccm/mod.rs"),
        ),
        (
            "control/ccm/plan.rs",
            include_str!("../../../../src/backend/control/ccm/plan.rs"),
        ),
    ];
    let mut hits: Vec<String> = Vec::new();
    for (name, raw) in others {
        let prod = crate::guard_support::production_code(raw);
        if prod.contains("\"--") {
            hits.push((*name).to_string());
        }
    }
    assert!(
        hits.is_empty(),
        "这些文件里出现了 ccm 旗标的字面量：{hits:?}\n\
             `KR48D2` 要的是「一条命令一处实现」——旗标名的住址是 `argv.rs::flag`，\n\
             别处只许 `use` 它。在第二处敲一遍字面量，两份迟早分叉（`K-R50` 的成因）。"
    );
}

/// 〔DUP1 · `INVARIANTS §47` ①〕`--ccm-sid` 是标识符：进容器路那条 shell 串之前先过
/// `shell_quote_core::session_id_ok`（全仓唯一一份，`设计/01 §5` D1）——**正反各一格**（§47「拒过头也算违反」）。
/// 要求住址：`INVARIANTS §47` ①「字符集白名单（闭集，默认拒）＋ 不许 `-` 开头（选项注入）＋ 有长度上界的就钉上界」。
/// 〔V138〕`--resume <sid>` / `--model` 不再是 ccm 的，它们的值交给 claude 自己判（直路不过 shell，容器路走唯一的 quote）。
#[test]
fn a_session_id_is_judged_before_it_goes_anywhere() {
    let uuid = "0473c3a0-1111-2222-3333-444455556666";
    assert_eq!(ok(&["--ccm-sid", uuid]).ccm_sid, uuid);
    for bad in ["a_b", "a;b", "a.b"] {
        let arg = format!("--ccm-sid={bad}");
        assert!(err(&[&arg]).contains("不合形状"), "坏 sid {bad:?} 放行了");
    }
    assert!(err(&["--ccm-sid", &"a".repeat(65)]).contains("不合形状"));
}

/// 〔DUP1 · `INVARIANTS §47` ①〕`--account`：与建账号的那个工具逐字同的那一份判（`shell_quote_core::account_name_ok`），**正反各一格**。
#[test]
fn an_account_name_is_judged_before_it_goes_anywhere() {
    assert_eq!(ok(&["--account", "work"]).account, "work");
    for bad in ["a.b", "_a", "a b"] {
        let arg = format!("--account={bad}");
        assert!(err(&[&arg]).contains("用不了"), "坏账号名 {bad:?} 放行了");
    }
}
