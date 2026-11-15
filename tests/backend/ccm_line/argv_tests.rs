//! # （ccm argv 的组合规则一条都不许静默忽略）＋ / `D2`
//!
//! 后端的纪律是「一条都不许静默忽略」（`ccm_line/argv.rs`）；三种互斥 / 依赖组合
//! 就是 `the_combination_rules_all_fail_loudly` 逐条断言的那几种。`ccm_flag_literals_live_only_in_argv_rs` 对 `D1`「一个判定只有一个家」；
//! `every_default_lives_only_in_the_defaults_block` 对 `D2`「一个数只有一个住址」。

use super::test_support::{mixed_to_split, parse};
use super::*;

/// 本文件的夹具把 ccm 选项写在前 ⇒ 喂解析器之前换成 `<交给 agent 的…> -- <ccm 的…>` 排列（意图逐词不变）。
fn parse_mixed(a: &[String]) -> Result<Parsed, Die> {
    parse(&mixed_to_split(a))
}

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn ok(a: &[&str]) -> Opts {
    match parse_mixed(&v(a)).expect("该解析得动") {
        Parsed::Opts(o) => *o,
        other => panic!("期望拿到 Opts，实得 {other:?}"),
    }
}

fn err(a: &[&str]) -> String {
    match parse_mixed(&v(a)) {
        Err(Die(m)) => m,
        other => panic!("期望 die，实得 {other:?}"),
    }
}

/// ccm 只看不吃 `--resume` / `--continue`：几种写法都原样进透传、顺序不变（一种都不落成 ccm 自己的 resume 动作）。
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

/// resume 的那条会话按这一家的写法认：Claude 的旗标形（`--resume <sid>` · `--resume=` · `-r`），Codex 的子命令形
/// （透传第一个词恰是 `resume`）。认错了家（Codex 那一趟里的 `--resume`、Claude 那一趟里的首词 `resume`）不算。
#[test]
fn the_resumed_session_is_read_the_way_that_agent_resumes() {
    assert_eq!(ok(&["--resume", "s1"]).resumes.as_deref(), Some("s1"));
    assert_eq!(ok(&["--resume=s1"]).resumes.as_deref(), Some("s1"));
    assert_eq!(ok(&["-r", "s1"]).resumes.as_deref(), Some("s1"));
    assert_eq!(ok(&["resume", "s1"]).resumes, None);
    let codex = |a: &[&str]| {
        let mut w = v(a);
        w.extend(v(&["--", "--ccm-agent", "codex"]));
        match parse(&w).expect("该解析得动") {
            Parsed::Opts(o) => *o,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(codex(&["resume", "s1"]).resumes.as_deref(), Some("s1"));
    assert_eq!(codex(&["resume", "s1"]).passthru, v(&["resume", "s1"]));
    assert_eq!(codex(&["resume", "--last"]).resumes, None);
    assert_eq!(codex(&["--resume", "s1"]).resumes, None);
    assert_eq!(codex(&["exec", "resume", "s1"]).resumes, None);
}

/// 选号照「这一家有没有账号这一维」认，不写死哪一家：声明了账号面的每一家 `--account` / `--account-dir` 都收下；
/// 没声明的 ⇒ 明说不行（不静默导出一个它不读的变量）。`--base` 每一家都照收。
#[test]
fn picking_an_account_follows_whether_that_agent_has_accounts() {
    let dir = "/h/.cc-monitor/accounts/b";
    let parsed = |a: &[&str]| parse(&v(a));
    let mut with_accounts = 0;
    for kind in crate::agents::REGISTRY.iter().map(|a| a.kind) {
        let by_name = parsed(&["--", "--ccm-agent", kind, "--account", "b"]);
        let by_dir = parsed(&["--", "--ccm-agent", kind, "--account-dir", dir]);
        if crate::agents::account_env_of(kind).is_some() {
            with_accounts += 1;
            match by_name {
                Ok(Parsed::Opts(o)) => assert_eq!(o.account, "b", "{kind}"),
                other => panic!("{kind} 有账号面，--account 该收下：{other:?}"),
            }
            match by_dir {
                Ok(Parsed::Opts(o)) => assert_eq!(o.account_dir, dir, "{kind}"),
                other => panic!("{kind} 有账号面，--account-dir 该收下：{other:?}"),
            }
        } else {
            let said = crate::agents::family_said_among(crate::agents::REGISTRY, kind);
            let want = copy_core::copy_text("beArgv.validate.agentNoAccounts", &[("agent", &said)]);
            for got in [by_name, by_dir] {
                match got {
                    Err(Die(m)) => assert_eq!(m, want, "{kind}"),
                    other => panic!("{kind} 没有账号面，选号该被拒：{other:?}"),
                }
            }
        }
        assert!(ok(&["--ccm-agent", kind, "--base"]).use_base, "{kind}");
    }
    // 不说是哪一家 ⇒ 默认那一家（有账号面）照收。
    assert_eq!(ok(&["--account", "b"]).account, "b");
    assert!(
        with_accounts > 0,
        "注册表里一家声明了账号面的都没有 —— 上面收下那一半空转着绿"
    );
}

/// `--attach <名>` 取值时不许把下一个旗标吞成名字。
#[test]
fn the_attach_option_never_swallows_the_next_flag_as_its_value() {
    assert!(err(&["--attach", "--ccm-tmux"]).contains("--attach"));
    assert!(err(&["--attach"]).contains("--attach"));
    assert_eq!(ok(&["--attach", "cc-foo"]).attach_name, "cc-foo");
}

/// 〔搬自 `ccm-cli`「不认识的 agent 报错」「--account 与 --base 互斥」〕
#[test]
fn the_combination_rules_all_fail_loudly() {
    assert_eq!(
        err(&["--ccm-agent", "gemini"]),
        copy_core::copy_text(
            "beAgents.pick.unknown",
            &[("agent", "gemini"), ("known", "claude / codex")]
        )
    );
    // 写空 ⇒ 默认那一家（两种写法），不当成漏了参数、也不报不认识。
    assert_eq!(ok(&["--ccm-agent", ""]).agent, Defaults::agent());
    assert_eq!(ok(&["--ccm-agent="]).agent, Defaults::agent());
    assert_eq!(
        err(&["--account", "z", "--base"]),
        copy_core::copy_static!("beArgv.validate.accountAndBase")
    );
    assert!(
        err(&["--detach"]).starts_with(copy_core::copy_static!("beArgv.validate.detachNeedsTmux"))
    );
    assert!(err(&["--tmux-size", "1x1"])
        .starts_with(copy_core::copy_static!("beArgv.validate.sizeNeedsTmux")));
    assert!(err(&["--ccm-tmux=a", "--tmux-base", "b"])
        .starts_with(copy_core::copy_static!("beArgv.validate.nameAndBase")));
    // `--bus-note` · `--bus-register` 删了（起会话不记派生台账；会话 id 由 cc-monitor 认，cc-bus 不用登记）：
    // ccm 不认它们，原样交给 agent。
    assert_eq!(ok(&["--bus-note", "x"]).passthru, v(&["--bus-note", "x"]));
    assert_eq!(
        ok(&["--ccm-tmux", "--detach", "--bus-register"]).passthru,
        v(&["--bus-register"])
    );
    // 这几形不报「未知选项 / 多余的位置参数」，原样交给 agent。
    assert_eq!(ok(&["--nope", "foo"]).passthru, v(&["--nope", "foo"]));
    assert_eq!(
        ok(&["--ccm-agent", "codex", "resume", "s"]).passthru,
        v(&["resume", "s"])
    );
    // claude 自己的 `--tmux` / `--agent` 原样交出去；`new` / 位置词 `attach` 交给 claude。
    assert_eq!(
        ok(&["--tmux", "--agent", "x"]).passthru,
        v(&["--tmux", "--agent", "x"])
    );
    // 首词 `new` 不再是 ccm 的位置动作：没有 `--` 的词一律原样交 claude。
    assert_eq!(ok(&["new", "--ccm-tmux"]).passthru, v(&["new"]));
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
    assert!(copy_core::copy_matches(
        "beArgv.validate.badSize",
        &err(&["--ccm-tmux", "--tmux-size", "x50"])
    ));
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

/// 🔴 机检：**每个默认值只许有一处住址。**
///
/// 判法不是「数一数」，是**真去比**：`parse(&[])` 的结果必须逐个字段等于
/// [`Defaults`] 声明的那些值。任何人在别处再写一份默认（比如在 `plan.rs` 里
/// `if agent.is_empty() { agent = "claude" }`），只要那份与这里分叉，这条就红。
#[test]
fn every_default_lives_only_in_the_defaults_block() {
    let o = ok(&[]);
    assert_eq!(o.agent, Defaults::agent());
    assert_eq!(o.cwd_spec, Defaults::CWD);
    assert_eq!(o.use_tmux, Defaults::USE_TMUX);
    assert_eq!(o.use_base, Defaults::USE_BASE);
    assert_eq!(o.detach, Defaults::DETACH);
    assert_eq!(o.print, Defaults::PRINT);
    assert!(o.passthru.is_empty() && o.attach_name.is_empty());
    // 反向：把默认值本身换掉，上面那一族必须跟着动 —— 否则它们是自说自话。
    assert_ne!(
        Defaults::agent(),
        "",
        "默认 agent 是空串的话这条判据就是空真"
    );
}

/// 🔴 机检：**ccm 的旗标字面量只住 `argv.rs`。**
///
/// 失效方向：「有人再起第二个 argv 解析口、或把某条子命令的行为
/// 在第二处重写一遍」。判法 = 扫 `wire/ccm_line/` 与 `control/ccm/` 两棵目录下**除 `argv.rs` 外**
/// 全部 `.rs` 的生产段，不许出现 `"--` 打头的字面量（正控：`argv.rs` 自己有）。
/// 买不到：不经字面量拼出来的旗标名；这两棵目录以外的文件。
#[test]
fn ccm_flag_literals_live_only_in_argv_rs() {
    let src = guard_core::addr::backend_src_root();
    let mut others: Vec<(String, String)> = Vec::new();
    for (dir, excluded) in [
        (
            "wire/ccm_line",
            &["src/backend/wire/ccm_line/argv.rs"] as &[&str],
        ),
        ("control/ccm", &[]),
    ] {
        for (p, raw) in guard_core::scan_tree_excluding(&src.join(dir), &["rs"], excluded) {
            let rel = p
                .strip_prefix(&src)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            others.push((rel, raw));
        }
    }
    assert!(
        others.len() >= 5,
        "两棵目录只扫到 {} 份（argv.rs 之外）—— 住址坏了，本条在空人群上恒绿：{:?}",
        others.len(),
        others.iter().map(|(r, _)| r).collect::<Vec<_>>()
    );
    let argv =
        guard_core::production_code(include_str!("../../../src/backend/wire/ccm_line/argv.rs"));
    assert!(
        argv.contains("\"--"),
        "正控：`argv.rs` 里一个旗标字面量都没有 —— 认法坏了"
    );
    let mut hits: Vec<String> = Vec::new();
    for (name, raw) in &others {
        if guard_core::production_code(raw).contains("\"--") {
            hits.push(name.clone());
        }
    }
    assert!(
        hits.is_empty(),
        "这些文件里出现了 ccm 旗标的字面量：{hits:?}\n\
             要的是「一条命令一处实现」——旗标名的住址是 `argv.rs::flag`，\n\
             别处只许 `use` 它。在第二处敲一遍字面量，两份迟早分叉。"
    );
}

/// 透传里找一个旗标的位置（全仓一份）：`<旗标> <值>` ⇒ 值那一格；`<旗标>=<值>` ⇒ 那一个词；写了几次认最后一个；那一家自己的 `--` 之后不看；
/// 旗标在末尾没给值也算（值读不出）。
#[test]
fn a_passthru_flag_is_found_last_and_never_past_the_agents_end() {
    let w = |a: &[&str]| v(a);
    let at = |a: &[&str]| flag_at(&w(a), &["--settings"]);
    assert_eq!(
        at(&["--settings", "a", "--settings", "b"]),
        Some(FlagAt::Next(3))
    );
    assert_eq!(
        at(&["--settings", "a", "--settings=b"]),
        Some(FlagAt::Inline(2))
    );
    assert_eq!(
        at(&["--settings", "a", "--", "--settings", "b"]),
        Some(FlagAt::Next(1))
    );
    assert_eq!(at(&["--", "--settings", "b"]), None);
    assert_eq!(at(&["x", "--settings"]), Some(FlagAt::Next(2)));
    assert_eq!(FlagAt::Next(2).value(&w(&["x", "--settings"])), None);
    assert_eq!(FlagAt::Inline(0).value(&w(&["--settings=q"])), Some("q"));
    // 几个写法同一个意思（`--resume` 与 `-r`）：认最后写的那一个。
    assert_eq!(
        flag_at(&w(&["-r", "a", "--resume", "b"]), &["--resume", "-r"]),
        Some(FlagAt::Next(3))
    );
}

/// ccm 不再收 `--ccm-sid`：建会话不写身份（`@ccm_sid` 只由后端认过会话文件才打），这个词没有用处 ⇒ 当不认识的选项报。
/// `--resume <sid>` / `--model` 不是 ccm 的，它们的值交给 claude 自己判（直路不过 shell，容器路走唯一的 quote）。
#[test]
fn the_session_id_flag_is_gone() {
    let uuid = "0473c3a0-1111-2222-3333-444455556666";
    for word in ["--ccm-sid", "--ccm-sid=x"] {
        let got = match parse(&v(&["--", word, uuid])) {
            Err(Die(m)) => m,
            other => panic!("{word} 该当不认识的选项报，实得 {other:?}"),
        };
        assert_eq!(got, copy_text("beArgv.parse.unknownRight", &[("w", word)]));
    }
}

/// 〔`INVARIANTS §47` ①〕`--account`：与建账号的那个工具逐字同的那一份判（`shell_quote_core::account_name_ok`），**正反各一格**。
#[test]
fn an_account_name_is_judged_before_it_goes_anywhere() {
    assert_eq!(ok(&["--account", "work"]).account, "work");
    for bad in ["a.b", "_a", "a b"] {
        let arg = format!("--account={bad}");
        assert!(
            copy_core::copy_matches("beArgv.validate.badAccount", &err(&[&arg])),
            "坏账号名 {bad:?} 放行了"
        );
    }
}

/// 要求：「`new` 是 ccm 自己的词，写在 `--` 右边 —— `ccm [claude 的] -- new [ccm 选项]`；
/// `ccm new` 整行交 claude，不开例外」。直接喂原排列（不经本文件的换排列）。
#[test]
fn new_is_ccms_word_only_as_the_first_word_right_of_the_end() {
    let raw = |a: &[&str]| parse(&v(a));
    assert!(
        matches!(raw(&["-p", "x", "--", "new", "--ccm-tmux"]), Ok(Parsed::Opts(o)) if o.passthru == v(&["-p", "x"]) && o.use_tmux),
        "`-- new …` 没被当成 ccm 的起新会话"
    );
    assert!(
        matches!(raw(&["new"]), Ok(Parsed::Opts(o)) if o.passthru == v(&["new"])),
        "`ccm new` 该整行交 claude"
    );
    assert!(
        matches!(raw(&["--", "--ccm-tmux", "new"]), Err(super::Die(m)) if m.contains(copy_core::copy_static!("beArgv.parse.newNotFirst"))),
        "`new` 不在右边第一个却被认了 / 没说清为什么"
    );
}

/// 自己的名字认的是这份二进制的名字（`ccm` · `cc-monitor-backend` · 落盘的 `cc-monitor-backend-<build>`），
/// 不是编进这一段的那个包叫什么：这一段换了包，认的名字也不跟着变。
#[test]
fn own_names_are_the_binary_names_whatever_package_this_code_sits_in() {
    use crate::ccm_line::is_own_name;
    for own in ["ccm", "cc-monitor-backend", "cc-monitor-backend-p11b"] {
        assert!(is_own_name(own), "`{own}` 没认成自己");
    }
    for other in ["claude", "backend-wire", "cc-monitor"] {
        assert!(!is_own_name(other), "`{other}` 被认成了自己");
    }
}

/// `ccm -- --hosts` ⇒ 立即答「这台的托管终端」，不起会话。
#[test]
fn hosts_is_an_early_answer() {
    match parse(&v(&["--", "--hosts"])).expect("该解析得动") {
        Parsed::Early(Early::Hosts) => {}
        other => panic!("该是 Early::Hosts：{other:?}"),
    }
}

/// 托管入口的两个词：`ls` ⇒ `-- --hosts`；`attach <名>` ⇒ `-- --attach <名>`；别的第一个词不归它（`None`）。
/// 缺名 · 多出词 ⇒ 说清、不猜。
#[test]
fn the_hosted_entry_owns_only_ls_and_attach() {
    assert_eq!(hosted_entry(&v(&["ls"])), Some(Ok(v(&["--", "--hosts"]))));
    assert_eq!(
        hosted_entry(&v(&["attach", "proj-cc"])),
        Some(Ok(v(&["--", "--attach", "proj-cc"])))
    );
    for other in [v(&[]), v(&["--resume", "x"]), v(&["lsx"]), v(&["讲讲 ls"])] {
        assert_eq!(hosted_entry(&other), None, "{other:?} 不归托管入口");
    }
    assert!(matches!(hosted_entry(&v(&["attach"])), Some(Err(_))));
    assert!(matches!(hosted_entry(&v(&["attach", "--x"])), Some(Err(_))));
    assert!(matches!(hosted_entry(&v(&["ls", "x"])), Some(Err(_))));
    assert!(matches!(
        hosted_entry(&v(&["attach", "a", "b"])),
        Some(Err(_))
    ));
}

/// `a` 是 `attach` 的短写（同 tmux）：`a <名>` ⇒ `-- --attach <名>`；缺名 · 多出词说的同 `attach` 那一句。
#[test]
fn the_hosted_entry_takes_a_as_attach() {
    assert_eq!(
        hosted_entry(&v(&["a", "code-picture-cc"])),
        Some(Ok(v(&["--", "--attach", "code-picture-cc"])))
    );
    for (short, long) in [
        (v(&["a"]), v(&["attach"])),
        (v(&["a", "--x"]), v(&["attach", "--x"])),
        (v(&["a", "x", "y"]), v(&["attach", "x", "y"])),
    ] {
        let said = hosted_entry(&short);
        assert!(matches!(said, Some(Err(_))), "{short:?} 该说清：{said:?}");
        assert_eq!(said, hosted_entry(&long), "{short:?} 说的该同 {long:?}");
    }
}

/// 「这一家有没有账号这一维」只问注册表那一格：喂一行没声明号载体的替身 ⇒ `--account` · `--account-dir` 都明说不行；
/// 同一行声明了号载体 ⇒ 照收。今天两家都有账号面，这一支只有替身跑得到。
#[test]
fn a_stand_in_without_an_account_carrier_refuses_picking_an_account() {
    let kind = crate::agents::fake::AGENT_KIND;
    let with = crate::agents::fake::full_row(|| None);
    let bare = crate::agents::Adapter {
        account_env: None,
        ..crate::agents::fake::full_row(|| None)
    };
    assert!(with.account_env.is_some(), "替身那一行声明了号载体");
    assert!(
        bare.accounts.is_some(),
        "只去掉号载体，账号面照留：判的只是号载体那一格"
    );
    // 那一句说的是那一家的叫法（替身行头上的），不是 kind。
    let said = crate::agents::family_said_among(std::slice::from_ref(&bare), kind);
    assert_ne!(said, kind);
    let want = copy_core::copy_text("beArgv.validate.agentNoAccounts", &[("agent", &said)]);
    for (name, dir) in [("b", ""), ("", "/h/.cc-monitor/accounts/b")] {
        let mut o = blank_opts(&[]);
        o.agent = kind.to_string();
        o.account = name.to_string();
        o.account_dir = dir.to_string();
        match finish_among(std::slice::from_ref(&bare), o.clone()) {
            Err(Die(m)) => assert_eq!(m, want, "{name:?} {dir:?}"),
            other => panic!("没有号载体的替身选号该被拒：{other:?}"),
        }
        let ok = finish_among(std::slice::from_ref(&with), o).expect("声明了号载体 ⇒ 照收");
        assert_eq!((ok.account.as_str(), ok.account_dir.as_str()), (name, dir));
    }
}

/// 不带 `--agent` ⇒ 起的是默认那一家；它不认先定会话ID、又要不轮换 ⇒ 拒，那一句说默认那一家的叫法（用户没敲那个码，不回显 kind）。
#[test]
fn without_an_agent_the_refusal_says_the_default_familys_name() {
    let row = crate::agents::fake::family_row("fake", "CCM_FAKE_A", "fakea", true, || None);
    let row = crate::agents::Adapter {
        launch: row.launch.map(|l| crate::agents::LaunchFace {
            preset_sid: None,
            ..l
        }),
        ..row
    };
    let mut o = blank_opts(&[]);
    // 不带 `--agent`：那一格空（`blank_opts` 预填的是生产默认那一家）。
    o.agent = String::new();
    o.no_rotation = true;
    match finish_among(std::slice::from_ref(&row), o) {
        Err(Die(m)) => assert_eq!(
            m,
            copy_core::copy_text(
                "beArgv.validate.noRotationNoSid",
                &[("agent", crate::agents::fake::said_of("fake"))],
            )
        ),
        other => panic!("不认先定会话ID的默认那一家不轮换该被拒：{other:?}"),
    }
}
