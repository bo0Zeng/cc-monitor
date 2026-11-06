use super::*;

// 那一格（终端里敲 `ccm` 走到哪）要起登录 shell，判它的在 `on_path_tests`；这里只比名片那几格。
fn answer_probe() -> serde_json::Value {
    let here = on_path::on_path(&on_path::Reach::Ours, "");
    serde_json::to_value(super::answer_probe_for(
        crate::platform::tmux::TMUX_PLATFORM,
        here,
    ))
    .unwrap()
}

/// ★★ **死值验：铸名避让只许问那一张快照，且算完就定死。**
///
/// 「另起一份名字集合」有两种长法，这里各钉一条：
///
/// | 长法 | 挡它的是什么 |
/// |---|---|
/// | 在这里自己去列一遍 tmux、拼一个 `Vec<String>` 喂给 `build` | **类型**：`plan::build` 只收 `TakenNames`，而它的字段是 `common::session_snapshot` 模块私有的 ⇒ **编译不过** |
/// | 让 `build` 拿到名字之后，在 `execute` 里再退让一次 | **本条**：算完的名字一个字都不许再改 |
///
/// 🔴 第二种的后果不是「名字错了」，是
/// **`--print` 与真跑吐的名字可以不一样**，而 `--print` 的全部意义就是当平价预言机。
#[test]
fn the_name_avoidance_has_exactly_one_source_and_the_plan_settles_it() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/ccm/mod.rs"));
    guard_core::assert_no_test_code("control/ccm/mod.rs", &prod);

    let builds = prod.matches("plan::build(").count();
    assert_eq!(
        builds, 1,
        "本文件算了 {builds} 次计划（登记 1）—— `--print` 与真跑必须共用**同一次** \
             `plan::build` 的产物，算两次就是两条路各自铸一次名。"
    );
    assert!(
        prod.contains("host::snapshot().taken_names()"),
        "喂给 `plan::build` 的那份「已占用的名字」不是从会话快照来的。\n\
             ★ 那张 hash 表只有一处住址（`common::session_snapshot`，进程内那一份是 `host::snapshot`）。"
    );
    assert!(
        !prod.contains("next_free_name"),
        "本文件生产段里又出现了 `next_free_name` —— 退让回到了计划之外。\n\
             ★ 算完的名字就是最终名；在这里再退让一次 = `--print` 与真跑又分叉了。"
    );
    assert!(
        !prod.contains("gate::list_sessions"),
        "本文件又直接去问 `gate::list_sessions` 了 —— 那是判活那条投影，\n\
             铸名要的是 `TakenNames`（同一张快照，但过的是那个造不出第二份的类型）。"
    );
    // 反向自检：这几针不是靠「本文件恰好不含那些词」空转的。
    assert!(
        guard_core::production_code(include_str!("../../../src/backend/control/ccm/plan.rs"))
            .contains("fn next_free_name"),
        "`plan.rs` 里找不到 `next_free_name` —— 退让规则搬家/改名了，本条在空转"
    );
}

/// 没有 `--` 又没有终端 ⇒ 不起 claude；有 `--` 的、或 stdin / stdout 有一边是终端的照旧。
/// 容器路在 pane 里把自己再叫一次那一行恒带 `--`（`plan::build` 的 `inner`），所以它不看终端。
#[test]
fn a_bare_call_without_any_terminal_does_not_start_an_agent() {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for bare in [
        v(&[]),
        v(&["--list-projects"]),
        v(&["--stream"]),
        v(&["--resume", "x"]),
    ] {
        assert!(refuses_without_terminal(&bare, false, false), "{bare:?}");
        assert!(
            !refuses_without_terminal(&bare, true, false),
            "{bare:?}：stdin 是终端"
        );
        assert!(
            !refuses_without_terminal(&bare, false, true),
            "{bare:?}：stdout 是终端"
        );
        assert!(!refuses_without_terminal(&bare, true, true), "{bare:?}");
    }
    for with_end in [
        v(&["--", "--ccm-print"]),
        v(&["--", "--ccm-version"]),
        v(&["--", "--ccm-tmux", "--detach"]),
        v(&["--resume", "x", "--", "--cwd", "/p"]),
        v(&["-p", "--", "-x", "--"]),
    ] {
        assert!(
            !refuses_without_terminal(&with_end, false, false),
            "{with_end:?}"
        );
    }
    let o = match argv::test_support::parse(&v(&[
        "--resume",
        "x",
        "--",
        "--ccm-tmux",
        "--detach",
        "--cwd",
        "/p",
    ])) {
        Ok(argv::Parsed::Opts(o)) => o,
        other => panic!("{other:?}"),
    };
    let Ok(Plan::Container(c)) = plan::build(
        &o,
        &Env::for_preview(),
        &plan::test_support::no_library,
        None,
    ) else {
        panic!("不是容器路")
    };
    assert!(
        c.payload.contains(" '--' "),
        "pane 里那一行没带 --：{}",
        c.payload
    );
}

/// 〔搬自 `ccm-contract-parity` 的 `--ccm-probe` 那 5 条〕
///
/// 这份输出是**外部契约**：调用方按行读 `capabilities=`。
#[test]
fn the_probe_output_is_the_shape_its_parser_expects() {
    let out = probe_output("/usr/local/bin/ccm");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "name=ccm", "首行逐字 name=ccm 是判活依据");
    assert_eq!(lines[1], format!("version={CCM_VERSION}"));
    assert_eq!(lines[2], "self=/usr/local/bin/ccm");
    assert!(lines[3].starts_with("capabilities="));
    assert_eq!(lines[4], "agents=claude,codex");
    // 🔴 **身份那一行，取自这个进程自己编进来的常量**。
    //   它与 `version=` 分开问是刻意的（前者「你是哪一份」、后者「你认得哪些参数」），
    //   理由全文住 `probe_output` 的头注。
    assert_eq!(lines[5], format!("build={}", crate::common::id::BUILD_ID));
    assert_eq!(lines.len(), 6, "多一行少一行都是契约变更，实得 {lines:?}");
    assert!(out.ends_with('\n'), "最后一行也要有换行");
    // 消费者今天要的那 8 个（`ccm_invocation.rs::CLI_REQUIRED_CAPS` ＋ account 维度）
    for c in [
        "new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid", "account",
    ] {
        assert!(
            CAPABILITIES.contains(&c),
            "渲染器要的能力 `{c}` 不在 CAPABILITIES 里 ⇒ 那条路当场 NotInstalled"
        );
    }
}

/// 〔搬自 `ccm-cli` KCY4「行为/capabilities 变了 ⇒ 版本号跟着走」〕
///
/// 这一版是**后端的原生命令**，不是那份 bash ⇒ 版本号必须比最后一版 bash（`4`）大。
#[test]
fn the_version_moved_because_the_implementation_did() {
    let n: u32 = CCM_VERSION.parse().expect("版本号是十进制");
    assert!(n > 4, "最后一版 bash 是 4，原生实现必须大于它，实得 {n}");
}

/// 〔搬自 `ccm-cli` KCY4「用法块里有那一行」〕
///
/// **每个认得的旗标都要在 [`USAGE`] 里说得出** —— 这条同时是
/// `protocol_doc_guard::TERMINAL_SURFACE_FILES` 那格受管例外的配套判据
/// （那边保证「旗标字面量只住 argv.rs」，这边保证「住在那里的都说得出来」）。
#[test]
fn every_flag_we_accept_has_a_usage_line() {
    let src =
        guard_core::production_code(include_str!("../../../src/backend/host/ccm_line/argv.rs"));
    let mut flags: Vec<String> = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = src[from..].find("\"--") {
        let at = from + rel + 1;
        let tail = &src[at..];
        if let Some(end) = tail[1..].find('"') {
            let tok = tail[..end + 1].to_string();
            // ⚠ 只收**像旗标的那一形**：`argv.rs` 里还有一堆以 `--` 开头的
            //   **报错文案**（「--account 与 --base 互斥」…），把它们当旗标
            //   会让这条判据去 USAGE 里找一整句话 —— 那是量具的射程画错了。
            let looks_like_a_flag = tok.len() > 2
                && tok[2..]
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if looks_like_a_flag && !flags.contains(&tok) {
                flags.push(tok);
            }
        }
        from = at + 2;
    }
    assert!(
        flags.len() >= 15,
        "只抽到 {} 个旗标 —— 抽取坏了，本断言在空转：{flags:?}",
        flags.len()
    );
    // 🔴 **匹配单位是「有没有属于它自己的那一行」，不是 `USAGE.contains(旗标)`。**
    //
    // 〔变异台自查逮到，同一族的又一形〕`contains` 那一版**是空转的**：
    // 把 `--detach` 自己那一行整行删掉，判据**照样绿** ——
    // 因为别的旗标那一行的括注里逐字写着「（要带 `--detach`）」一类的话。
    // 匹配单位（全文）比事实（它有没有自己的条目）**大**了一格，
    // 于是「顺带被别人提到一句」被读成了「说明了它」。
    let has_own_line = |flag: &str| {
        USAGE.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with(flag)
                && t[flag.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c == ' ' || c == '[' || c == '=' || c == ',')
        })
    };
    let missing: Vec<&String> = flags.iter().filter(|f| !has_own_line(f)).collect();
    assert!(
        missing.is_empty(),
        "这些旗标认得、但 `--help` 里**没有属于它自己的那一行**：{missing:?}\n\
             （用户看得见的唯一一份说明就是 USAGE；认一个不说一个 = 隐藏开关。\n\
              ⚠ 在别的行的括注里被提一句**不算** —— 那一版实测是空转的。）"
    );
}

/// 🔴 **`base-url-across-tmux` 这个 token 不是一句自称。**
///
/// 它声明的那件事（把 `ANTHROPIC_BASE_URL` 带过 tmux 的进程边界）由本条
/// **真去算一遍容器路的计划**来兑现 —— 量的是 [`plan::build`] 交出来的那一串载荷，
/// 不是源码文本，也不是本条自己再写一遍的什么规则。
///
/// ⇒ 两个方向都有牙：
/// - 把 token 从 [`CAPABILITIES`] 里删掉 ⇒ 第一格红（**说了才算数**）；
/// - 把 `plan.rs` 那句 `export ANTHROPIC_BASE_URL=…` 掏掉 ⇒ 第二格红（**做到了才许说**）。
///
/// ⚠ **本条没买到的**：「变量真的穿过了一次**真** tmux 边界」要真机 tmux，本条量的是
/// 载荷字符串。那一半归 e2e，别把本条读成「实测过了」。
#[test]
fn the_base_url_token_is_declared_because_the_tmux_path_really_forwards_it() {
    // ① 申报这一半。
    assert!(
        CAPABILITIES.contains(&"base-url-across-tmux"),
        "`base-url-across-tmux` 不在 CAPABILITIES 里了 —— 容器路把继承来的中转地址带过 tmux 边界那件事\n\
             就**没有申报了** —— 能力表上的每个 token 都要说得出它兑现在哪。"
    );

    // ② 实现这一半 —— 真算一遍，只喂替身环境，一个字节不碰这台机器。
    let base_env = || Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        base_url_env: crate::agents::claudecode::paths::BASE_URL_ENV.into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    let payload_of = |env: &Env| -> String {
        let args: Vec<String> = ["--ccm-tmux=n1", "--cwd", "/p"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let Parsed::Opts(o) = argv::test_support::parse(&argv::test_support::mixed_to_split(&args))
            .expect("该解析得动")
        else {
            panic!("`--tmux=n1` 不该被解析成 Early")
        };
        match plan::build(&o, env, &plan::test_support::no_library, None).expect("该算得出计划")
        {
            Plan::Container(c) => c.payload,
            other => panic!("`--tmux=` 该走容器路，实得 {other:?}"),
        }
    };

    let mut with_relay = base_env();
    with_relay.inherited_base_url = Some("https://relay.example/v1".into());
    let sent = payload_of(&with_relay);
    assert!(
        sent.contains("export ANTHROPIC_BASE_URL='https://relay.example/v1'"),
        "容器路的载荷里没有把中转地址显式化 ⇒ 它在 tmux 边界上会被吃掉，\n\
             而 CAPABILITIES 里那个 `base-url-across-tmux` 就成了一句**假申报**。\n\
             实得载荷：{sent}"
    );

    // ③ 反空真对照：不给这个变量，那一串里不许出现它。
    //    （没有这一格，上面那句 `contains` 可能是靠载荷恒带某段文本过的。）
    let clean = payload_of(&base_env());
    assert!(
        !clean.contains("ANTHROPIC_BASE_URL"),
        "没设中转地址，载荷却带上了它 —— 上面那一格此刻是恒真的：{clean}"
    );
}

/// 闭集只有一处住址（注册表里带起会话事实的那几家），每一家按哪几格起都由它那一格声明；
/// 这里逐格钉住两家今天的组合（ccm 按这几格起，行为与按名字分叉时逐条相同）。
#[test]
fn the_agent_set_has_one_address_and_every_member_is_wired() {
    assert_eq!(agents(), ["claude", "codex"]);
    let face = |a: &str| {
        crate::agents::launch_face_among(crate::agents::REGISTRY, a)
            .expect("注册表里有这一家的起会话事实")
    };
    for a in agents() {
        assert!(!face(a).default_launcher.is_empty(), "{a} 没有默认启动器");
    }
    assert_eq!(
        argv::Defaults::agent(),
        "claude",
        "不给 `--agent` 起的那一家变了"
    );
    assert_eq!(face("claude").nested_env.len(), 4);
    assert!(
        face("codex").nested_env.is_empty(),
        "codex 不清 claude 的嵌套标记"
    );
}

/// 「发对了」「缺省尺寸」「控制字符」那几族。
///
/// 同一个进程之下没有「上线」这回事 —— 真契约是**那几件事一件都不许丢**，
/// 而且**必须经过 `parse_request` 那道门**（字段校验只长在它身上）。
#[test]
fn the_container_launch_goes_through_the_one_door_with_every_field_intact() {
    let c = plan::Container {
        host: plan::TerminalHost::Tmux,
        host_run: Vec::new(),
        name: "n1".into(),
        cwd: "/p".into(),
        agent: "claude".into(),
        ccm_sid: "p1".into(),
        size: Some(("220".into(), "50".into())),
        detach: true,
        payload: "'/usr/local/bin/ccm' '--cwd' '/p'".into(),
        self_check: "'/usr/local/bin/ccm' '--cwd' '/p' '--print'".into(),
        inner_head: vec!["/usr/local/bin/ccm".into()],
        self_check_argv: Vec::new(),
        unset: Vec::new(),
    };
    let req = crate::control::launch::parse_request(launch_args(&c)).expect("该过得了门");
    assert_eq!(req.name, "n1");
    assert_eq!(req.payload, c.payload, "载荷不许被改一个字节");
    assert_eq!(req.cwd.as_deref(), Some("/p"));
    assert_eq!(req.ccm_sid.as_deref(), Some("p1"), "意图标不许丢");
    assert_eq!(
        req.agent.as_deref(),
        Some("claude"),
        "@ccm_agent 不许丢（它丢过一次）"
    );
    assert_eq!(req.width.as_deref(), Some("220"));
    assert_eq!(req.height.as_deref(), Some("50"));
    assert_eq!(
        req.client.as_deref(),
        Some(crate::control::gate_rules::CLIENT_TERMINAL),
        "ccm 起的会话要声明成用户终端起的（不归任何一个前端）"
    );
    assert!(matches!(
        req.mode,
        crate::control::launch::Mode::CreateOrAttach
    ));
    // 不给尺寸 ⇒ 请求里**没有** width/height（不是空串、不是 0）
    let mut c2 = c.clone();
    c2.size = None;
    let r2 = crate::control::launch::parse_request(launch_args(&c2)).expect("该过得了门");
    assert!(r2.width.is_none() && r2.height.is_none());
    // 🔴 那道门真的在判：载荷里塞一个 ESC ⇒ 被**挡在这一侧**，不是发出去被拒
    let mut c3 = c.clone();
    c3.name = "n\u{1b}1".into();
    assert!(
        crate::control::launch::parse_request(launch_args(&c3)).is_err(),
        "控制字符没被挡住 —— 那道门被绕过去了（直接造 LaunchRequest 就是这个后果）"
    );
}

/// 〔搬自 `ccm-cli` WIRE/launch 撞名那一族〕—— 撞名的那句话必须带上是哪个名字。
#[test]
fn the_name_taken_message_says_which_name() {
    let msg = NAME_TAKEN_FMT.replacen("%s", "cc-proj", 1);
    assert!(msg.contains("cc-proj"), "不带名字的报错等于没报：{msg}");
    assert_eq!(
        NAME_TAKEN_FMT.matches("%s").count(),
        1,
        "格式串只许有一个占位"
    );
    // 🔴 结尾必须是**字面的两个字符** `\` + `n`，不是一个真换行 —— 见常量头注。
    //   写成真换行 ⇒ `--print` 吐的那条命令会在这里断成两行。
    assert!(
        NAME_TAKEN_FMT.ends_with("\\n") && !NAME_TAKEN_FMT.ends_with('\n'),
        "它是 printf 的格式串，结尾要是字面 \\n：{NAME_TAKEN_FMT:?}"
    );
    assert_eq!(
        msg.replace("\\n", "\n").lines().count(),
        1,
        "译回真换行之后它是**一行**（末尾一个换行），不是两行"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// `P19`（09-22）· codex 那一支不经 shell 的那条路
// 要求：`--agent codex` 在 Windows 上不再 `program not found`。
// 真机读数（那一跳到底在哪）住。
// ═══════════════════════════════════════════════════════════════════════

/// 造一份**直路计划**，`tmux` 那一格由调用方给 —— 走的是生产那条链
/// （`argv::parse_layered` → `plan::build`），不是手搓一个 `Direct`。
/// 便条那一格「起在哪种宿主里」：托管进程设的 `CCM_HOST` ⇒ 托管（压过 `TMUX`）；`TMUX` ⇒ tmux；都没有 · 空值 ⇒ 直路。
#[test]
fn the_last_hop_names_the_host_it_runs_in() {
    use crate::wire::TerminalHost::{Managed, Tmux};
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    assert_eq!(host_here(env(&[("CCM_HOST", "h1")])), Some(Managed));
    assert_eq!(
        host_here(env(&[("CCM_HOST", "h1"), ("TMUX", "/s,1,0")])),
        Some(Managed)
    );
    assert_eq!(host_here(env(&[("TMUX", "/s,1,0")])), Some(Tmux));
    assert_eq!(host_here(env(&[("TMUX", ""), ("CCM_HOST", "")])), None);
    assert_eq!(host_here(env(&[])), None);
}

fn direct_of(args: &[&str], tmux: Option<&str>) -> plan::Direct {
    let e = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        tmux: tmux.map(str::to_string),
        ..Default::default()
    };
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let o = match argv::test_support::parse(&argv::test_support::mixed_to_split(&a))
        .expect("该解析得动")
    {
        Parsed::Opts(o) => o,
        other => panic!("{other:?}"),
    };
    match plan::build(&o, &e, &plan::test_support::no_library, None).expect("该算得出计划") {
        Plan::Direct(d) => d,
        other => panic!("直路测试却算出 {other:?}"),
    }
}

/// ★★ **`P19`：还非得要 POSIX shell 的只剩 `CCM_ENV` 那一件。**
///
/// 直路上 codex 与 claude 在 tmux 内外都不要 shell（起会话不替 cc-bus 交身份，没有那段配方）；
/// `CCM_ENV` 是一段任意 shell，只有 shell 解释得了。
///
/// ⚠ **本条买不到的那一维**：它判的是「这一趟要不要请 shell 进来」这个**判定**，
/// **不是**「在真 Windows 上真的起来了」。后者要真 Windows。
#[test]
fn only_ccm_env_still_needs_a_posix_shell_on_the_direct_path() {
    for agent in ["codex", "claude"] {
        for tmux in [None, Some("/faux/socket,1,0")] {
            let d = direct_of(&["--ccm-agent", agent, "--cwd", "/p"], tmux);
            assert_eq!(
                needs_shell(&d),
                None,
                "{agent} 这一趟不该要 shell（tmux={tmux:?}）"
            );
        }
    }
    let mut with_env = direct_of(&["--ccm-agent", "codex", "--cwd", "/p"], None);
    with_env.ccm_env = "export HTTPS_PROXY=http://x:1".into();
    assert_eq!(
        needs_shell(&with_env),
        Some(WHY_SHELL_CCM_ENV.as_str()),
        "`CCM_ENV` 是一段任意 shell，只有 shell 解释得了 —— 这一条不许被收窄掉"
    );
}

/// ★ **每一条「为什么非得经 shell」都真有人用，而且只有一处用**（`D7` ＋ `D1`）。
///
/// 🔴 这几句话不是装饰：`sh` 不在这台机器上时，它们就是用户唯一看得到的**原因**。
/// 真机读数为此只能靠「同一个 launcher 在 claude 那趟 `EXIT=0`」
/// 反推出「找不到的是 `sh` 不是 `hostname`」—— **错的归因比失败本身更贵**。
///
/// 人群从源码派生（生产段里每一处 `WHY_SHELL_` 的出现），**相等**断言：
/// 声明一处 ＋ 取用一处 = 2。多了 ⇒ 同一句归因有第二处住址；少了 ⇒ 留了条用不上的话。
#[test]
fn every_reason_for_needing_a_shell_is_declared_once_and_used_once() {
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/ccm/mod.rs"));
    guard_core::assert_no_test_code("control/ccm/mod.rs", &prod);

    let reasons: &[(&str, &str)] = &[
        ("WHY_SHELL_CCM_ENV", WHY_SHELL_CCM_ENV.as_str()),
        ("WHY_SHELL_ATTACH", WHY_SHELL_ATTACH.as_str()),
        (
            "WHY_SHELL_CONTAINER_TAIL",
            WHY_SHELL_CONTAINER_TAIL.as_str(),
        ),
    ];
    for (name, text) in reasons {
        assert_eq!(
            prod.matches(name).count(),
            2,
            "`{name}` 在生产段出现 {} 次（该是 2：声明一处 ＋ 取用一处）。\n\
             多了 ⇒ 同一句归因有了第二处住址；少了 ⇒ 这是一条没人用的话。",
            prod.matches(name).count()
        );
        assert!(
            text.chars().count() >= 12,
            "`{name}` 那句话只有 {} 个字 —— 太短，答不出「为什么非得经它」",
            text.chars().count()
        );
    }
    // 四句话两两不同：同一句话贴在两处出口上等于没有归因。
    let mut seen: std::collections::BTreeSet<&str> = Default::default();
    for (_, text) in reasons {
        assert!(seen.insert(text), "有两条出口贴着同一句归因：{text:?}");
    }
    // 🔴 `sh` 起不来时那句话要带得出机器可认的 code（同 `no_tmux:` 那一形）。
    assert_eq!(NO_SHELL, "no_shell");
    assert_eq!(
        prod.matches("NO_SHELL").count(),
        2,
        "`NO_SHELL` 在生产段出现 {} 次（该是 2：声明一处 ＋ 那句话里取用一处）",
        prod.matches("NO_SHELL").count()
    );
    // 反向自检：抽取没坏（生产段里真有那两处起进程口）。
    assert_eq!(
        prod.matches("Child::new(").count(),
        4,
        "本文件生产段的起进程口不是 4 处了（sh 收尾 · exec 那一家 · 脱离起托管进程 · 托管那一形的自检） —— 后端 clippy 起进程那一组的放行点要一起看"
    );
}

// ── `--ccm-probe` 自报的能力 = 这台机器上做得到的那一份 ──────────
//
// 第 2 层「能力清单从实现派生」（`lib.rs` 头注那张表逐字「**能力清单从实现派生，
// `CAPABILITIES` 由它们汇总而来，不许手写**」）；读数要求：「`ccm.exe --ccm-probe`
// 在 Windows 上自报 `tmux, detach, tmux-size, tmux-base` 等能力 —— 这些在 Windows 上都做不到」。
// 异源：Windows 那一格的期望是**手写的两张名单**（做得到 / 做不到），不从 `CCM_TMUX_CARRIED` 或
// `ccm_launcher_with` 里抠 —— 否则两侧同源、恒真。
// ⚠ 买不到：`TMUX_PLATFORM` 在本机是 `AskThePath`，Windows 那一档由入参模拟；真 `ccm.exe` 吐什么要真机
//    （虚拟机读数）。

fn caps_line(out: &str) -> std::collections::BTreeSet<String> {
    out.lines()
        .find_map(|l| l.strip_prefix("capabilities="))
        .expect("`--ccm-probe` 没有 `capabilities=` 那一行")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn on_windows_the_probe_does_not_claim_what_only_tmux_can_carry() {
    let win = caps_line(&probe_output_for(
        "C:\\x\\ccm.exe",
        crate::platform::tmux::TmuxPlatform::AbsentUnlessExeOnPath,
    ));
    let want_absent = [
        "tmux",
        "detach",
        "tmux-size",
        "tmux-base",
        "ccm-sid",
        "base-url-across-tmux",
    ];
    // `attach` 做得到：`--attach <名>` 先认托管名单（瘦客户端接上），认不到才落 `tmux attach`。
    let want_present = [
        "new",
        "resume",
        "attach",
        "account",
        "model",
        "cwd",
        "agent",
        "launcher",
        "print",
        "backend-discover",
        "account-via-backend",
    ];
    let want: std::collections::BTreeSet<String> =
        want_present.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        win, want,
        "Windows 那一份 `--ccm-probe` 自报的能力与手写期望不等（两向）；做不到的那几条：{want_absent:?}"
    );
    // 正控：Linux（问 PATH 的那一档）上逐字是整张表 —— 两张手写名单的并集。
    let linux = caps_line(&probe_output_for(
        "/usr/local/bin/ccm",
        crate::platform::tmux::TmuxPlatform::AskThePath,
    ));
    let all: std::collections::BTreeSet<String> = want_present
        .iter()
        .chain(want_absent.iter())
        .map(|s| s.to_string())
        .collect();
    assert_eq!(linux, all, "Linux 那一份应当逐字是整张 `CAPABILITIES`");
}

/// 生产那一口真的把**本二进制的**平台档交进去（不是回到整张表，也不是写死某一档）。
#[test]
fn the_real_probe_asks_this_binarys_own_platform() {
    let me = guard_core::production_code(include_str!("../../../src/backend/control/ccm/mod.rs"));
    let body = me
        .split("pub fn probe_output(self_path: &str) -> String {")
        .nth(1)
        .and_then(|b| b.split("\n}\n").next())
        .expect("找不到 `probe_output` —— 抽取器坏了");
    assert_eq!(
        body.trim(),
        "probe_output_for(self_path, crate::platform::tmux::TMUX_PLATFORM)",
        "`probe_output` 要把本二进制的 `TMUX_PLATFORM` 交给内核"
    );
    assert_eq!(
        me.matches("CAPABILITIES.join(").count(),
        0,
        "又把整张 `CAPABILITIES` 原样吐进 `--ccm-probe` 了（Windows 上会自报 tmux 那一族）"
    );
    // 本机读数：Linux 上生产那一份 == 问 PATH 那一档。
    assert_eq!(
        caps_line(&probe_output("/x/ccm")),
        caps_line(&probe_output_for(
            "/x/ccm",
            crate::platform::tmux::TmuxPlatform::AskThePath
        ))
    );
}

/// 要求：「件 E 让 ccm 恒是那台后端本体之后，这一问改问后端自己」。
/// 帧 `ccm-probe` 出成品：每一格 == CLI `--ccm-probe` 那张名片的同名行（异源：一边是 JSON，一边切文本行）；
/// 键集合 == 金样 `tests/__fixtures__/ccm-probe.golden.json` 的键（界面解码器读同一份金样）。
#[test]
fn the_probe_frame_answers_the_same_card_as_the_cli_flag() {
    let v = answer_probe();
    let card = probe_output("x");
    let line = |k: &str| {
        card.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .unwrap_or_else(|| panic!("名片里没有 {k}= 那一行"))
            .to_string()
    };
    let list = |k: &str| {
        v[k].as_array()
            .expect(k)
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    assert_eq!(v["version"].as_str(), Some(line("version").as_str()));
    assert_eq!(v["build"].as_str(), Some(crate::common::id::BUILD_ID));
    assert_eq!(line("build"), crate::common::id::BUILD_ID);
    assert_eq!(
        list("capabilities"),
        line("capabilities"),
        "帧面与 CLI 那一口的能力不是同一份"
    );
    assert_eq!(list("agents"), line("agents"));
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/ccm-probe.golden.json")).unwrap();
    let keys = |x: &serde_json::Value| {
        x.as_object()
            .unwrap()
            .keys()
            .filter(|k| !k.starts_with('_'))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&v),
        keys(&golden["product"]),
        "成品的键 ≠ 金样的键 —— 界面解码器读的是金样"
    );
}

/// 要求：「tmux 名派生 ＋ 撞名避让只留后端」。起会话铸名两形各一格 == 手写期望；避让问的是**交进来那张快照**（被占 ⇒ 往后排）；没装 tmux ⇒ 基名。
#[test]
fn minting_derives_here_and_steps_aside_on_this_machines_snapshot() {
    use crate::common::session_snapshot::{SessionRow, SessionSnapshot};
    let snap = SessionSnapshot::with_prober(|| {
        Ok(["proj-cc", "proj-fork-cc", "proj-fork-cc-2"]
            .iter()
            .map(|n| SessionRow {
                name: (*n).to_string(),
                ccm_sid: String::new(),
            })
            .collect())
    });
    use crate::control::session_batch::NameBase;
    let mint = |b: NameBase| terminal_name_mint_among(b, &snap, Vec::new());
    assert_eq!(mint(NameBase::Cwd("/home/u/proj")).unwrap(), "proj-cc-2");
    assert_eq!(mint(NameBase::Cwd("/home/u/other")).unwrap(), "other-cc");
    assert_eq!(mint(NameBase::ForkOf("proj-cc")).unwrap(), "proj-fork-cc-3");
    assert_eq!(
        mint(NameBase::ForkOf("/home/u/gone")).unwrap(),
        "gone-fork-cc"
    );
    let no_tmux = SessionSnapshot::with_prober(|| {
        Err(crate::common::said::Said::new(
            "no_tmux",
            "tmux: not found".to_string(),
        ))
    });
    assert_eq!(
        terminal_name_mint_among(NameBase::Cwd("/home/u/proj"), &no_tmux, Vec::new()).unwrap(),
        "proj-cc"
    );
}

/// 同目录已有托管终端 `proj-cc` ⇒ 起会话铸的名字不取 `proj-cc`（起会话那一步 ccm 照显式名起，撞上托管名就退出码 3、报「名字已占用」）。
/// tmux 装着（快照里没有它）· 没装 tmux 各一格：铸名与 ccm 算计划认同一份「已占用」（`taken_here`）。
#[test]
fn minting_steps_aside_from_live_hosted_terminals_too() {
    use crate::common::session_snapshot::{SessionRow, SessionSnapshot};
    use crate::control::session_batch::NameBase;
    let hosted = || {
        vec![SessionRow {
            name: "proj-cc".to_string(),
            ccm_sid: String::new(),
        }]
    };
    let tmux = SessionSnapshot::with_prober(|| Ok(Vec::new()));
    assert_eq!(
        terminal_name_mint_among(NameBase::Cwd("/home/u/proj"), &tmux, hosted()).unwrap(),
        "proj-cc-2"
    );
    let no_tmux = SessionSnapshot::with_prober(|| {
        Err(crate::common::said::Said::new(
            "no_tmux",
            "tmux: not found".to_string(),
        ))
    });
    assert_eq!(
        terminal_name_mint_among(NameBase::Cwd("/home/u/proj"), &no_tmux, hosted()).unwrap(),
        "proj-cc-2"
    );
}

// ── 起会话只有 ccm 一处（要求：「所有的起会话都是一处ccm」）──────────────────────────────

/// 一段生产代码里「直接起 agent 的命令」的痕迹（名字从注册表取，不写死）：
/// ① 串字面量以某一家的默认启动器打头（`"claude"` · `"claude …"`）；② 串字面量里带「启动器 ＋ 这一家的 resume 词」
/// （`claude --resume` · `codex resume`）。
fn direct_agent_launch_hits(code: &str, launchers: &[&str], resume_tokens: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for lit in string_literals(code) {
        let t = lit.trim_start();
        for l in launchers {
            let at_head = t
                .strip_prefix(l)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '));
            if at_head {
                hits.push(format!("行首是 agent 命令名：{lit:?}"));
            }
            for r in resume_tokens {
                if lit.contains(&format!("{l} {r}")) && !at_head {
                    hits.push(format!("带 `{l} {r}`：{lit:?}"));
                }
            }
        }
    }
    hits
}

/// Rust 源码里的串字面量内容（普通串认 `\` 转义；`r"…"` / `r#"…"#` 认原样；`'x'` 字符字面量跳过、生命周期不当成引号）。
fn string_literals(code: &str) -> Vec<String> {
    let b: Vec<char> = code.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < b.len() {
        match b[i] {
            'r' if b.get(i + 1).is_some_and(|c| *c == '"' || *c == '#')
                && !b
                    .get(i.wrapping_sub(1))
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_') =>
            {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&'"') {
                    i += 1;
                    continue;
                }
                let close: String = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let rest: String = b[j + 1..].iter().collect();
                let end = rest.find(&close).unwrap_or(rest.len());
                out.push(rest[..end].to_string());
                i = j + 1 + rest[..end].chars().count() + close.chars().count();
            }
            '\'' if b.get(i + 2) == Some(&'\'')
                || (b.get(i + 1) == Some(&'\\') && b.get(i + 3) == Some(&'\'')) =>
            {
                i += if b.get(i + 1) == Some(&'\\') { 4 } else { 3 };
            }
            '"' => {
                let mut s = String::new();
                let mut j = i + 1;
                while j < b.len() && b[j] != '"' {
                    if b[j] == '\\' {
                        j += 1;
                    }
                    if let Some(c) = b.get(j) {
                        s.push(*c);
                    }
                    j += 1;
                }
                out.push(s);
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// ★★ **后端生产段里，除 `ccm` 自己（`control/ccm/`，最终 exec 那一处）与 agent 注册表（`agents/`，起会话事实声明的地方）之外，
/// 零处渲染出直接起 agent 的命令**。名字从注册表取（每一家的默认启动器 ＋ resume 词）。
///
/// 命中为空（没有登记的例外）。
/// 正控：同一个针在注册表那一侧量得到默认启动器的字面量；一段现造的语料里该中的中、`ccm …` 不中。
#[test]
fn nothing_but_ccm_renders_a_command_that_starts_an_agent() {
    let faces: Vec<crate::agents::LaunchFace> = crate::agents::REGISTRY
        .iter()
        .filter_map(|a| a.launch)
        .collect();
    let launchers: Vec<&str> = faces.iter().map(|f| f.default_launcher).collect();
    let tokens: Vec<&str> = faces.iter().map(|f| f.resume_token).collect();
    assert!(
        launchers.len() >= 2,
        "注册表里由我们起的那几家只抠到 {launchers:?}"
    );

    // 正控 ①：现造的语料。
    let corpus = format!(
        "let a = \"{l} --resume x\"; let b = \"{l}\"; let c = \"ccm --resume x -- --base\"; \
         let d = '\"'; let e = r#\"{l} {r} s\"#;",
        l = launchers[0],
        r = tokens[0]
    );
    assert_eq!(
        direct_agent_launch_hits(&corpus, &launchers, &tokens).len(),
        3,
        "针在现造语料上失准"
    );

    let root = guard_core::addr::backend_src_root();
    let mut outside: Vec<String> = Vec::new();
    let mut registry_hits = 0;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let under = path.strip_prefix(&root).unwrap_or(&path);
        let rel = under.to_string_lossy().replace('\\', "/");
        let code = guard_core::strip_comment_lines(&guard_core::production_code(&src));
        let hits = direct_agent_launch_hits(&code, &launchers, &tokens);
        // 按路径分量认住址（逐段相等，不是拿串的前缀比）：注册表那一侧 = `agents/` 整棵；`ccm` 自己 = `control/ccm/` 整棵。
        let seg: Vec<String> = under
            .iter()
            .map(|c| c.to_string_lossy().into_owned())
            .collect();
        let top = (
            seg.first().map(String::as_str),
            seg.get(1).map(String::as_str),
        );
        if top.0 == Some("agents") {
            registry_hits += hits.len();
        } else if top != (Some("control"), Some("ccm")) {
            outside.extend(hits.into_iter().map(|h| format!("{rel}：{h}")));
        }
    }
    // 正控 ②：注册表那一侧声明默认启动器的那几个字面量量得到（扫描面是真树，针也真认得注册表里的名字）。
    assert!(
        registry_hits >= launchers.len(),
        "注册表那一侧只量到 {registry_hits} 处 —— 扫描面或针坏了"
    );
    assert_eq!(
        outside,
        Vec::<String>::new(),
        "ccm 以外又有地方渲出了直接起 agent 的命令 —— 起会话只交一行 `ccm …`，环境 / 中转地址 / 身份由那台的 ccm 做"
    );
}

/// ★ 找上游那个变量名问注册表那一家的上游格：Claude 有（继承值照读、不注入时照清）；Codex 没登记上游 ⇒ 名字空、继承值不读，
/// 它的载荷一个字不提那个变量（不注入、也不清 —— 它本就不读）。不继承（别名预览）⇒ 两样继承值都不读。
#[test]
fn the_upstream_variable_comes_from_the_agents_own_upstream_cell() {
    let ours = format!(
        "http://127.0.0.1:8788/{}/t/claude-code/q",
        "0123456789abcdef".repeat(4)
    );
    let ours = ours.as_str();
    let get = |k: &str| match k {
        "ANTHROPIC_BASE_URL" => Some(ours.to_string()),
        "CLAUDE_CONFIG_DIR" => Some("/acct/q".to_string()),
        _ => None,
    };
    let filled = |agent: &str, inherit: bool| {
        let mut e = Env::default();
        agent_env(
            agent,
            &mut e,
            inherit.then_some(&get as &dyn Fn(&str) -> Option<String>),
        );
        e
    };
    let claude = filled("claude", true);
    assert_eq!(claude.base_url_env, "ANTHROPIC_BASE_URL");
    assert_eq!(claude.inherited_base_url.as_deref(), Some(ours));
    assert_eq!(claude.inherited_config_dir.as_deref(), Some("/acct/q"));
    let codex = filled("codex", true);
    assert_eq!(
        (
            codex.base_url_env.as_str(),
            codex.inherited_base_url.as_deref()
        ),
        ("", None)
    );
    let preview = filled("claude", false);
    assert_eq!(
        (preview.inherited_base_url, preview.inherited_config_dir),
        (None, None)
    );
    // 载荷：Claude 不注入时清掉继承来的那条中转；Codex 一个字不提。
    let line = |agent: &str| {
        let mut e = filled(agent, true);
        e.home = "/home/pi".into();
        e.pwd = "/p".into();
        e.accts_manifest = "/nonexistent/accounts.json".into();
        e.self_argv = vec!["/usr/local/bin/ccm".into()];
        let a: Vec<String> = ["--ccm-agent", agent, "--cwd", "/p"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let Parsed::Opts(o) =
            argv::test_support::parse(&argv::test_support::mixed_to_split(&a)).expect("该解析得动")
        else {
            panic!("不该是 Early")
        };
        plan::render(&plan::build(&o, &e, &plan::test_support::no_library, None).expect("计划"))
    };
    assert!(
        line("claude").contains("unset ANTHROPIC_BASE_URL; "),
        "{}",
        line("claude")
    );
    assert!(
        !line("codex").contains("ANTHROPIC_BASE_URL"),
        "{}",
        line("codex")
    );
}

/// 直路的启动器在**计划里**就换成要起的那个文件，与足迹 · 后端自己起 agent 同一个找法（`platform::shell::agent_launch_in`）：
/// 后端自己的 `PATH` 上没有、起会话那个 shell 的有（经 ssh 非登录 shell 起的后端）⇒ 计划里是那个文件，`--ccm-print` 与真跑都起得来；
/// 起会话那个 shell 的上也没有 ⇒ 计划里就拒（托管那一形的自检当场说，不等里面那一跳起了再退）。
/// Windows：`.cmd` 是那个文件、`.ps1` 经 PowerShell；用户给了路径 ⇒ 不替他找。后面的词原样跟着。
#[test]
fn the_launcher_is_resolved_in_the_plan_on_the_session_shells_path() {
    use crate::platform::shell::agent_launch_in;
    use std::path::{Path, PathBuf};
    let user_bin = PathBuf::from("/u/.local/bin");
    let has = |set: Vec<PathBuf>| move |p: &Path| set.iter().any(|f| f == p);
    let session = "/u/.local/bin:/usr/bin:/bin";
    let own = "/usr/bin:/bin";
    let unix_has = has(vec![user_bin.join("claude")]);
    let resolved = |d: &mut plan::Direct, session: Option<&str>| {
        launcher_resolved(d, &|p| {
            agent_launch_in(p, session, Some(own), false, &unix_has)
        })
    };
    let mut d = direct_of(&["--model", "m", "--", "--cwd", "/p"], None);
    assert_eq!(d.argv.first().map(String::as_str), Some("claude"), "夹具");
    resolved(&mut d, Some(session)).expect("起会话那个 shell 的 PATH 上有 ⇒ 计划算得出");
    assert_eq!(
        d.argv.first().map(String::as_str),
        Some("/u/.local/bin/claude")
    );
    assert!(
        d.argv.windows(2).any(|w| w == ["--model", "m"]),
        "后面的词原样跟着：{:?}",
        d.argv
    );
    // 起会话那个 shell 的 PATH 上没有（只本进程的那一份上有也不算）⇒ 计划里就拒。
    let mut d = direct_of(&["--", "--cwd", "/p"], None);
    let why = agent_launch_in("claude", Some(own), Some(session), false, &unix_has)
        .expect_err("夹具：这一份上没有");
    assert!(
        matches!(
            launcher_resolved(&mut d, &|p| agent_launch_in(p, Some(own), Some(session), false, &unix_has)),
            Err(Die(m)) if m == copy_text("beCcm.plan.launcherAbsent", &[("prog", "claude"), ("why", &why)])
        ),
        "找不到要在计划里拒"
    );
    // Windows：npm 的 .cmd 就是那个文件；只有 .ps1 ⇒ 经 PowerShell 起它。
    let npm = PathBuf::from("/w/npm");
    let win = |files: Vec<PathBuf>| {
        let mut d = direct_of(&["--", "--cwd", "/p"], None);
        let is = has(files);
        launcher_resolved(&mut d, &|p| {
            agent_launch_in(p, Some("/w/npm"), None, true, &is)
        })
        .expect("找得到");
        d.argv
    };
    let cmd = npm.join("claude.cmd").display().to_string();
    let ps = npm.join("claude.ps1").display().to_string();
    assert_eq!(
        win(vec![npm.join("claude.cmd"), npm.join("claude.ps1")])[0],
        cmd
    );
    assert_eq!(
        win(vec![npm.join("claude.ps1")])[..4],
        ["powershell.exe", "-NoProfile", "-File", ps.as_str()]
    );
    // 用户给了路径 ⇒ 不替他找。
    let mut d = direct_of(&["--cwd", "/p", "--launcher", "/opt/x/claude"], None);
    assert_eq!(d.argv[0], "/opt/x/claude", "夹具：{:?}", d.argv);
    launcher_resolved(&mut d, &|p| {
        agent_launch_in(p, Some(session), None, false, &has(vec![]))
    })
    .expect("给了路径 ⇒ 照它");
    assert_eq!(d.argv[0], "/opt/x/claude");
}

/// busy2：容器路送启动命令没确认 ⇒ 送达未知且不带 `--detach` 照样接进去；明确没送进且会话是本趟建的 ⇒ 收掉再报；别的照报。
#[test]
fn busy2_after_an_unconfirmed_start_command_join_or_reap() {
    use crate::control::launch::Delivery;
    let cases = [
        (
            Some((Delivery::Unknown, true)),
            false,
            AfterFail::JoinAnyway,
        ),
        (Some((Delivery::Unknown, true)), true, AfterFail::Report),
        (
            Some((Delivery::NotDelivered, true)),
            false,
            AfterFail::ReapThenReport,
        ),
        (
            Some((Delivery::NotDelivered, true)),
            true,
            AfterFail::ReapThenReport,
        ),
        (
            Some((Delivery::NotDelivered, false)),
            false,
            AfterFail::Report,
        ),
        (Some((Delivery::Unknown, false)), false, AfterFail::Report),
        (None, false, AfterFail::Report),
    ];
    for (untyped, detach, want) in cases {
        assert_eq!(
            after_launch_fail(untyped, detach),
            want,
            "{untyped:?} detach={detach}"
        );
    }
}

/// 起托管终端：托管进程（这里是假的：`sh -c` 一句）还没起好就退了 ⇒ 起的那一方**当场**报「托管进程已退出」、码与它的原话，
/// 不白等起好的期限、不报一句猜测。
#[cfg(unix)]
#[test]
fn a_hosted_terminal_that_exits_before_it_is_up_is_reported_at_once_with_its_words() {
    let (own, data) = crate::host::managed::run::test_support::sandbox_own("ccmdbg-exit");
    let said = "ccm: 托管终端 ccmdbg-x 已在运行";
    let host = [
        "sh".to_string(),
        "-c".to_string(),
        format!("printf '%s\\n' '{said}'; exit 3"),
    ];
    let got = spawn_hosted(&host, &[], "ccmdbg-x", &own);
    let _ = std::fs::remove_dir_all(&data);
    let m = match got {
        Err(HostSpawn::Failed(m)) => m,
        Err(HostSpawn::Refused(m)) => panic!("不该是拒：{m}"),
        Ok(()) => panic!("托管进程退了却报起好了"),
    };
    // 「当场」由结构保证（读到回信管道的头就交，没有轮询圈）；这里钉的是结局不是「等到期限」那一句。
    assert!(
        !m.contains(&copy_text(
            "beCcm.execute.hostNotUp",
            &[("name", "ccmdbg-x"), ("secs", "10")]
        )),
        "退了却报成等到期限：{m}"
    );
    assert!(
        m.contains(&copy_text(
            "beCcm.execute.hostExited",
            &[("name", "ccmdbg-x"), ("exit", "3")]
        )),
        "{m}"
    );
    assert!(m.contains(said), "原话没带上：{m}");
}

/// 托管那一形的自检算的要是里面那一跳那一份计划：里面那一跳的环境里带着托管终端的标记（⇒ 直路，启动器在计划里解析），
/// 自检也带上同一格 ⇒ 启动器找不到时自检当场没过，不报起好了（里面那一跳一起就退的那一种不再假成功）。
#[test]
fn the_hosted_self_check_plans_what_the_hop_inside_will_plan() {
    let e = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        can_host: true,
        ..Default::default()
    };
    let a: Vec<String> = ["--hosted", "--cwd", "/p"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let o = match argv::test_support::parse(&argv::test_support::mixed_to_split(&a))
        .expect("该解析得动")
    {
        Parsed::Opts(o) => o,
        other => panic!("{other:?}"),
    };
    let c = match plan::build(&o, &e, &plan::test_support::no_library, None).expect("该算得出计划")
    {
        Plan::Container(c) if c.host == plan::TerminalHost::Managed => c,
        other => panic!("托管测试却算出 {other:?}"),
    };
    let check = hosted_self_check_child(&c)
        .expect("托管那一形有自检")
        .built();
    let host = check
        .get_envs()
        .find(|(k, _)| *k == crate::host::managed::run::HOST_ENV)
        .and_then(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()));
    assert_eq!(
        host.as_deref(),
        Some(c.name.as_str()),
        "自检没带托管终端的标记"
    );
}

/// 托管进程起好了说一声（回信管道里那一行）⇒ 起的那一方当场算起好，不看盘、不轮询。
#[cfg(unix)]
#[test]
fn a_hosted_terminal_is_up_when_it_says_so() {
    let (own, data) = crate::host::managed::run::test_support::sandbox_own("ccmdbg-up");
    let host = [
        "sh".to_string(),
        "-c".to_string(),
        format!("printf '%s\\n' '{}'", crate::platform::child::READY_LINE),
    ];
    let got = spawn_hosted(&host, &[], "ccmdbg-y", &own);
    let _ = std::fs::remove_dir_all(&data);
    assert!(
        matches!(got, Ok(())),
        "{:?}",
        got.err().map(|e| match e {
            HostSpawn::Failed(m) | HostSpawn::Refused(m) => m,
        })
    );
}
