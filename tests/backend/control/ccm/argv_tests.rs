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

/// 〔搬自 `tests/e2e/ccm-cli.test.sh`「resume <sid>」「`--resume <sid>` 等价」「`--resume=<sid>`」〕
///
/// 三种写法**必须**落到同一套意图上 —— cc-monitor 今天发的就是 `--resume <sid>`。
#[test]
fn the_three_ways_to_say_resume_land_on_the_same_intent() {
    for a in [
        v(&["resume", "abc-123"]),
        v(&["--resume", "abc-123"]),
        v(&["--resume=abc-123"]),
    ] {
        let o = match parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => *o,
            other => panic!("{other:?}"),
        };
        assert_eq!(o.action, Action::Resume, "写法 {a:?} 没落到 resume");
        assert_eq!(o.sid, "abc-123", "写法 {a:?} 的 sid 不对");
    }
}

/// 〔搬自 `ccm-cli`「resume 后跟 flag → 报错（别把 --tmux 当 sid）」与 attach 同形那条〕
#[test]
fn a_positional_action_never_swallows_the_next_flag_as_its_value() {
    assert_eq!(err(&["resume", "--tmux"]), "resume 需要 <sid>");
    assert_eq!(err(&["resume"]), "resume 需要 <sid>");
    assert_eq!(err(&["attach", "--tmux"]), "attach 需要 <会话名>");
    assert_eq!(err(&["attach"]), "attach 需要 <会话名>");
}

/// 〔搬自 `ccm-cli`「未知选项报错」「未知 agent 报错」「--account 与 --base 互斥」〕
#[test]
fn the_combination_rules_all_fail_loudly() {
    assert!(err(&["--nope"]).starts_with("未知选项: --nope"));
    assert!(err(&["--agent", "gemini"]).starts_with("未知 agent: gemini"));
    assert_eq!(
        err(&["--account", "z", "--base"]),
        "--account 与 --base 互斥"
    );
    assert!(err(&["--detach"]).starts_with("--detach 需要配合 --tmux"));
    assert!(err(&["--tmux-size", "1x1"]).starts_with("--tmux-size 需要配合 --tmux"));
    assert!(err(&["--tmux=a", "--tmux-base", "b"]).starts_with("--tmux=<名> 与 --tmux-base"));
    assert!(err(&["--tmux", "--bus-register"]).starts_with("--bus-register 需要配合 --detach"));
    assert!(err(&["--bus-note", "x"]).starts_with("--bus-note 需要配合 --bus-register"));
    // 位置动作只认**第一个** token —— 排在旗标后面的 `resume` 是一个多余的位置参数
    assert!(err(&["--agent", "codex", "resume"]).starts_with("多余的位置参数"));
    assert!(err(&["resume", "s", "--agent", "codex"]).starts_with("agent=codex 不支持 resume"));
    assert_eq!(
        err(&["foo"]),
        "多余的位置参数: foo（动作只能是 new/resume/attach 且必须在最前）"
    );
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
    assert!(err(&["--tmux", "--tmux-size", "x50"]).starts_with("非法 --tmux-size"));
}

/// 〔搬自 `ccm-cli`「`-- 之后透传给 agent`」与「resume 不带 `--` 时不许多出任何参数」〕
#[test]
fn everything_after_the_terminator_goes_to_the_agent_untouched() {
    let o = ok(&["--", "-p", "hi there", "--tmux"]);
    assert_eq!(o.passthru, v(&["-p", "hi there", "--tmux"]));
    assert!(
        o.use_tmux == Defaults::USE_TMUX,
        "`--` 之后的 --tmux 不许被本层认走"
    );
    assert!(ok(&["resume", "s"]).passthru.is_empty());
}

/// 🔴 `KR48D4` 的机检：**每个默认值只许有一处住址。**
///
/// 判法不是「数一数」，是**真去比**：`parse(&[])` 的结果必须逐个字段等于
/// [`Defaults`] 声明的那些值。任何人在别处再写一份默认（比如在 `plan.rs` 里
/// `if agent.is_empty() { agent = "claude" }`），只要那份与这里分叉，这条就红。
#[test]
fn every_default_lives_only_in_the_defaults_block() {
    let o = ok(&[]);
    assert_eq!(o.action, Defaults::ACTION);
    assert_eq!(o.agent, Defaults::AGENT);
    assert_eq!(o.cwd_spec, Defaults::CWD);
    assert_eq!(o.use_tmux, Defaults::USE_TMUX);
    assert_eq!(o.use_base, Defaults::USE_BASE);
    assert_eq!(o.detach, Defaults::DETACH);
    assert_eq!(o.print, Defaults::PRINT);
    assert_eq!(o.bus_register, Defaults::BUS_REGISTER);
    assert!(!o.launcher_explicit, "没给 --launcher 就不许记成显式");
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
