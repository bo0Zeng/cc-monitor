use super::*;

/// ★★ **`KR96D2` 死值验第一刀：铸名避让只许问那一张快照，且算完就定死。**
///
/// 「另起一份名字集合」有两种长法，这里各钉一条：
///
/// | 长法 | 挡它的是什么 |
/// |---|---|
/// | 在这里自己去列一遍 tmux、拼一个 `Vec<String>` 喂给 `build` | **类型**：`plan::build` 只收 `TakenNames`，而它的字段是 `common::session_snapshot` 模块私有的 ⇒ **编译不过** |
/// | 让 `build` 拿到名字之后，在 `execute` 里再退让一次（本文件从前正是这样） | **本条**：算完的名字一个字都不许再改 |
///
/// 🔴 第二种是本件之前的真实形状 —— 后果不是「名字错了」，是
/// **`--print` 与真跑吐的名字可以不一样**，而 `--print` 的全部意义就是当平价预言机。
#[test]
fn the_name_avoidance_has_exactly_one_source_and_the_plan_settles_it() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/mod.rs"
    ));
    crate::guard_support::assert_no_test_code("control/ccm/mod.rs", &prod);

    let builds = prod.matches("plan::build(").count();
    assert_eq!(
        builds, 1,
        "本文件算了 {builds} 次计划（登记 1）—— `--print` 与真跑必须共用**同一次** \
             `plan::build` 的产物，算两次就是两条路各自铸一次名。"
    );
    assert!(
        prod.contains("session_snapshot::global().taken_names()"),
        "喂给 `plan::build` 的那份「已占用的名字」不是从会话快照来的。\n\
             ★ `R52` 裁定二：那张 hash 表只有一处住址（`common::session_snapshot`）。"
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
        crate::guard_support::production_code(include_str!(
            "../../../src/backend/control/ccm/plan.rs"
        ))
        .contains("fn next_free_name"),
        "`plan.rs` 里找不到 `next_free_name` —— 退让规则搬家/改名了，本条在空转"
    );
}

/// 两条进入路都只经 [`intercept`]，而**别的写法一律进不来**。
#[test]
fn there_are_exactly_two_ways_in() {
    let none: Vec<String> = vec![];
    assert_eq!(intercept("/usr/local/bin/ccm", &none), Some(vec![]));
    assert_eq!(intercept("ccm", &none), Some(vec![]));
    assert_eq!(intercept("C:\\x\\ccm.exe", &none), Some(vec![]));
    let sub = vec!["ccm".to_string(), "resume".to_string()];
    assert_eq!(
        intercept("/opt/cc-monitor-backend", &sub),
        Some(vec!["resume".to_string()])
    );
    // 不是 ccm ⇒ 一律放行给流模式 / wire 子命令
    assert_eq!(intercept("/opt/cc-monitor-backend", &none), None);
    assert_eq!(
        intercept("/opt/cc-monitor-backend", &vec!["--ping".to_string()]),
        None
    );
    assert_eq!(intercept("/opt/ccmonitor", &none), None, "子串不算");
}

/// 〔CC1〕「怎么叫我」＝ 进程 argv 里 `intercept` **吃掉的那一段**；两个入口各一格，再加回环。
///
/// 回环是这条的正题：把 `self_invocation` 交出来的那一段 ＋ 任意一串 ccm 参数**再喂给 `intercept`**，
/// 必须原样拿回那串参数 —— 也就是「pane 里把自己再叫一次，叫得回 ccm 模式、参数一个不多一个不少」。
/// 〔BS1b 现打的那一形：入口② 只取 argv0 ⇒ `intercept("cc-monitor-backend", ["--cwd", …])` 是 `None`，
///  那一跳直接掉进后端直连口。〕⚠ 这是单元层的**同源**自检；异源判据在 e2e `backend-cc-bus.sh` [17]。
#[test]
fn calling_myself_again_goes_back_through_the_same_way_in() {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // 入口①：只有 argv0。
    assert_eq!(
        self_invocation(&v(&["/x/ccm", "--tmux", "--cwd", "/p"])),
        v(&["/x/ccm"])
    );
    // 入口②：argv0 ＋ 子命令词（这一格就是 BS1b 那个缺陷的反面）。
    assert_eq!(
        self_invocation(&v(&[
            "/x/cc-monitor-backend",
            "ccm",
            "--tmux",
            "--cwd",
            "/p"
        ])),
        v(&["/x/cc-monitor-backend", SUBCOMMAND_WORD])
    );
    // 回环：两个入口、同一串内层参数 ⇒ 都叫得回 ccm 模式、拿回的参数逐字相同。
    let inner = v(&["--cwd", "/p", "--agent", "claude", "--", "--tail-only"]);
    for outer in [
        v(&["/x/ccm", "--tmux=n"]),
        v(&["/x/cc-monitor-backend", "ccm", "--tmux=n"]),
    ] {
        let mut again = self_invocation(&outer);
        again.extend(inner.iter().cloned());
        let (a0, rest) = again.split_first().expect("非空");
        assert_eq!(
            intercept(a0, rest),
            Some(inner.clone()),
            "从 {outer:?} 进来的，在 pane 里把自己再叫一次（{again:?}）叫不回 ccm 模式"
        );
    }
}

/// 〔搬自 `ccm-contract-parity` 的 `--ccm-probe` 那 5 条〕
///
/// 这份输出是**外部契约**：`ccm_probe.rs::parse_probe_output` 按行解析
/// `version=` / `capabilities=`。
#[test]
fn the_probe_output_is_the_shape_its_parser_expects() {
    let out = probe_output("/usr/local/bin/ccm");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "name=ccm", "首行逐字 name=ccm 是判活依据");
    assert_eq!(lines[1], format!("version={CCM_VERSION}"));
    assert_eq!(lines[2], "self=/usr/local/bin/ccm");
    assert!(lines[3].starts_with("capabilities="));
    assert_eq!(lines[4], "agents=claude,codex");
    // 🔴 `K-R70`：**身份那一行，取自这个进程自己编进来的常量**。
    //   它与 `version=` 分开问是刻意的（前者「你是哪一份」、后者「你认得哪些参数」），
    //   理由全文住 `probe_output` 的头注。
    assert_eq!(lines[5], format!("build={}", crate::BUILD_ID));
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
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/argv.rs"
    ));
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
    // 〔本轮变异台自查逮到，08-xx 那一族的又一形〕`contains` 那一版**是空转的**：
    // 把 `--detach` 自己那一行整行删掉，判据**照样绿** ——
    // 因为 `--bus-register` 那一行的括注里逐字写着「（需要 `--detach`）」。
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

/// 🔴 `K-R61` 09-11：**`base-url-across-tmux` 这个 token 不是一句自称。**
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
        "`base-url-across-tmux` 不在 CAPABILITIES 里了 —— 那么 monitor 侧\n\
             `history.rs::RELAY_KEEPS_THE_OLD_PATH` 的退役条件就**又没有落点了**，\n\
             而它正是 `K-R61` 立件的原因（前提指着一个已经被删掉的文件）。"
    );

    // ② 实现这一半 —— 真算一遍，只喂替身环境，一个字节不碰这台机器（`K31`）。
    let base_env = || Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    let payload_of = |env: &Env| -> String {
        let args: Vec<String> = ["--tmux=n1", "--cwd", "/p"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let Parsed::Opts(o) = argv::parse(&args).expect("该解析得动") else {
            panic!("`--tmux=n1` 不该被解析成 Early")
        };
        match plan::build(&o, env, &AccountTable::default(), None).expect("该算得出计划") {
            Plan::Container(c) => c.payload,
            other => panic!("`--tmux=` 该走容器路，实得 {other:?}"),
        }
    };

    let mut with_relay = base_env();
    with_relay.anthropic_base_url = Some("https://relay.example/v1".into());
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

/// 闭集只有一处住址：`AGENTS` 与那几个按 agent 分支的函数必须**逐个对得上**。
#[test]
fn the_agent_set_has_one_address_and_every_member_is_wired() {
    assert_eq!(AGENTS, &["claude", "codex"]);
    for a in AGENTS {
        assert!(!default_launcher(a).is_empty(), "{a} 没有默认启动器");
    }
    assert_eq!(resume_flag("claude"), Some("--resume"));
    assert_eq!(resume_flag("codex"), None, "codex 没有 resume flag");
    assert_eq!(nested_env("claude").len(), 4);
    assert!(
        nested_env("codex").is_empty(),
        "codex 不清 claude 的嵌套标记"
    );
    assert!(needs_bus_id("codex") && !needs_bus_id("claude"));
    assert!(has_identity("claude") && !has_identity("codex"));
}

/// 〔搬自 `ccm-rbind-title` 的 format 那一格〕
///
/// ⚠ **如实边界**：那套 e2e 是**真起一个私有 socket 的 tmux**、把 pane 标题冲成
/// 「⠐ 理解…」再读窗口标题的。这里只钉**那个格式串本身**，
/// 「tmux 真的这么解释它」那一半**没有判据了** —— 登记在件文件 `§8`，别读成等价。
#[test]
fn the_window_title_is_synthesised_from_the_identity_tag_not_the_pane_title() {
    assert_eq!(
        TERMINAL_BIND_TITLE_FORMAT,
        "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}"
    );
    assert!(
        TERMINAL_BIND_TITLE_FORMAT.starts_with("#{?@ccm_sid,"),
        "它必须是**条件式**：有 @ccm_sid 才出 marker，没有才回退 #T"
    );
    assert!(
        TERMINAL_BIND_TITLE_FORMAT.contains("ccm-rbind-#{@ccm_sid}"),
        "marker 必须逐字是 monitor 侧 bind.rs 要扫的那个前缀 + sid"
    );
    assert!(
        TERMINAL_BIND_TITLE_FORMAT.ends_with(",#T}"),
        "sid 还没回填时要回退 pane 标题，而不是产出一个空的 `ccm-rbind-`"
    );
}

/// 🔴 这个格式串**盘上有两份**（本常量 ＋ `control/launch.rs` 那一行的字面量），
/// 而两份不许漂开。
///
/// # 为什么不干脆收口成一份
///
/// 试过。收口之后 monitor 侧
/// `ccm_cli_contract::the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move`
/// 当场红：那条判据数的是 `control/launch.rs` **生产段里**「事实标记读点」的处数
/// （登记 2 处 = 这一行里的条件头 `@ccm_sid` 与取值 `#{@ccm_sid}`），
/// 收口成一个标识符之后它读到 **0**，而 0 的含义逐字是「标题回填没了」。
/// ⇒ 收口会把一条真判据变瞎。**留两份 + 本条钉住它们逐字相同**，买到的比收口多。
#[test]
fn the_window_title_format_has_the_same_text_on_both_sides() {
    let launch = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    assert!(
        launch.contains(TERMINAL_BIND_TITLE_FORMAT),
        "`control/launch.rs` 的生产段里找不到这个格式串的逐字副本：\n  {TERMINAL_BIND_TITLE_FORMAT}\n             两份已经漂开了（或者那一行被收口成了标识符 —— 别那么做，理由见本条头注）。"
    );
    assert!(
        launch.contains("set-titles-string"),
        "`launch.rs` 不再设 `set-titles-string` 了 —— 那是标题回填的落点"
    );
}

/// 〔搬自 `ccm-cli` WIRE/launch「发对了①–⑤」「缺省尺寸①②」「控制字符①–④」那几族〕
///
/// 从前那几条测的是「`ccm` 编出来的那段 JSON 上线之后逐字节对不对」。同一个进程之下
/// 没有「上线」这回事了 —— 剩下的真契约是**那几件事一件都不许丢**，
/// 而且**必须经过 `parse_request` 那道门**（字段校验只长在它身上）。
#[test]
fn the_container_launch_goes_through_the_one_door_with_every_field_intact() {
    let c = plan::Container {
        name: "n1".into(),
        cwd: "/p".into(),
        agent: "claude".into(),
        ccm_sid: "p1".into(),
        size: Some(("220".into(), "50".into())),
        detach: true,
        payload: "'/usr/local/bin/ccm' '--cwd' '/p'".into(),
        self_check: "'/usr/local/bin/ccm' '--cwd' '/p' '--print'".into(),
        trust_poll: true,
        bus: None,
    };
    let req = crate::control::launch::parse_request(&launch_args(&c)).expect("该过得了门");
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
    assert!(matches!(
        req.mode,
        crate::control::launch::Mode::CreateOrAttach
    ));
    // 不给尺寸 ⇒ 请求里**没有** width/height（不是空串、不是 0）
    let mut c2 = c.clone();
    c2.size = None;
    let r2 = crate::control::launch::parse_request(&launch_args(&c2)).expect("该过得了门");
    assert!(r2.width.is_none() && r2.height.is_none());
    // 🔴 那道门真的在判：载荷里塞一个 ESC ⇒ 被**挡在这一侧**，不是发出去被拒
    let mut c3 = c.clone();
    c3.name = "n\u{1b}1".into();
    assert!(
        crate::control::launch::parse_request(&launch_args(&c3)).is_err(),
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
// 题面：`--agent codex` 在 Windows 上不再 `program not found`。
// 真机读数（那一跳到底在哪）住 `真相源/106 §3.3`。
// ═══════════════════════════════════════════════════════════════════════

/// 造一份**直路计划**，`tmux` 那一格由调用方给 —— 走的是生产那条链
/// （`argv::parse` → `plan::build`），不是手搓一个 `Direct`。
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
    let o = match argv::parse(&a).expect("该解析得动") {
        Parsed::Opts(o) => o,
        other => panic!("{other:?}"),
    };
    match plan::build(&o, &e, &AccountTable::default(), None).expect("该算得出计划") {
        Plan::Direct(d) => d,
        other => panic!("直路测试却算出 {other:?}"),
    }
}

/// ★★ **`P19` 的承重前提：那段配方整段裹在 `$TMUX` 的守卫里。**
///
/// [`needs_shell`] 敢在「不在 tmux 里」时跳过 `sh -c`，靠的**只有**这一条事实 ——
/// 守卫为假 ⇒ 那段串在 `exec` 之前一个字都不做 ⇒ 省掉的是一个空转的中间进程。
/// 🔴 谁把配方改成「无条件先做点什么」（哪怕只是 `export CC_BUS_ID=`），
/// 那条等价当场不成立，而 [`needs_shell`] 会**静默地少做一件事** ——
/// 本条就是为了让那一刻响，而不是让它安静。
#[test]
fn the_bus_id_recipe_is_wholly_guarded_by_tmux_so_skipping_the_shell_is_exact() {
    let r = BUS_ID_RECIPE;
    assert!(
        r.starts_with("if [ -n \"${TMUX:-}\" ]; then "),
        "配方不再以 `$TMUX` 守卫开头 ⇒ `needs_shell` 跳过 shell 的那条等价不成立了：{r:?}"
    );
    assert!(
        r.ends_with("fi;"),
        "配方守卫之后还跟着别的东西 ⇒ 那部分在跳过 shell 时会被悄悄丢掉：{r:?}"
    );
    // 恰好一对 `if … fi` ⇒ 守卫**罩住全身**，中途没有再关一次再做别的事。
    assert_eq!(
        r.matches("fi;").count(),
        1,
        "配方里有不止一处 `fi;` —— 守卫在半路关过，后面那段是无条件的：{r:?}"
    );
    assert_eq!(
        r.matches("if ").count(),
        1,
        "配方里有不止一个 `if` —— 重新数一遍哪一段是无条件的：{r:?}"
    );
    // 反向自检：这几针不是靠「配方恰好是个空串」空转的。
    assert!(
        r.len() > 80 && r.contains("CC_BUS_ID"),
        "配方短得不像话或者不再设 `CC_BUS_ID` —— 本条在空转：{r:?}"
    );
}

/// ★★ **`P19` 正题：还非得要 POSIX shell 的只剩三件，codex 的配方不是其中之一。**
///
/// 🔴 **上一版的条件是 `!ccm_env.is_empty() || bus_id_recipe || resolved.is_some()`**
/// —— `bus_id_recipe` 单独成闸，而 `needs_bus_id("codex")` 恒真 ⇒ **每一趟**
/// `--agent codex` 都要一个 `sh`。真机现打（Win11，`真相源/106 §3.3`）：
/// `--agent codex --launcher hostname` → `EXIT=4 program not found`，
/// 同一个 launcher 在 `--agent claude` 那趟 `EXIT=0`。
///
/// ⚠ **本条买不到的那一维**：它判的是「这一趟要不要请 shell 进来」这个**判定**，
/// **不是**「在真 Windows 上真的起来了」。后者要那台 Win11 虚拟机，本路没去动它。
#[test]
fn only_three_things_still_need_a_posix_shell_and_the_codex_recipe_is_not_one_of_them() {
    // ── 🔴 P19 买的就是这一格：codex + 不在 tmux（Windows 上 `$TMUX` 恒空）────
    let codex_bare = direct_of(&["--agent", "codex", "--cwd", "/p"], None);
    // 反空真自检：`None` 必须是**因为守卫为假**得来的，不许是因为配方那一格自己没了。
    assert!(
        codex_bare.bus_id_recipe,
        "codex 这一趟连 `bus_id_recipe` 都是假的 —— 下面那条 `None` 会因为错的理由绿"
    );
    assert!(!codex_bare.inside_tmux, "夹具没把 `$TMUX` 置空，本格白测");
    assert_eq!(
        needs_shell(&codex_bare, None),
        None,
        "codex 不在 tmux 里还要请一个 `sh` 进来 —— 那正是 Windows 上那句 `program not found`"
    );

    // ── 在 tmux 里：配方真有事可做 ⇒ 照旧经 shell，且**说得出为什么** ─────────
    let codex_in_tmux = direct_of(
        &["--agent", "codex", "--cwd", "/p"],
        Some("/faux/socket,1,0"),
    );
    assert!(codex_in_tmux.inside_tmux, "夹具没把 `$TMUX` 置上，本格白测");
    assert_eq!(
        needs_shell(&codex_in_tmux, None),
        Some(WHY_SHELL_BUS_ID),
        "在 tmux 里那段配方要现问一次 tmux ⇒ 这一趟**必须**经 shell（`INVARIANTS §33a` 那条实测例）"
    );

    // ── claude 两态都不要 shell（它压根没有那段配方）──────────────────────
    for tmux in [None, Some("/faux/socket,1,0")] {
        let d = direct_of(&["--agent", "claude", "--cwd", "/p"], tmux);
        assert!(!d.bus_id_recipe, "claude 不该有 cc-bus 配方");
        assert_eq!(
            needs_shell(&d, None),
            None,
            "claude 这一趟不该要 shell（tmux={tmux:?}）"
        );
    }

    // ── 另两件**是真的要 shell**，本轮一件都没假装它们不要 ──────────────────
    let mut with_env = direct_of(&["--agent", "codex", "--cwd", "/p"], None);
    with_env.ccm_env = "export HTTPS_PROXY=http://x:1".into();
    assert_eq!(
        needs_shell(&with_env, None),
        Some(WHY_SHELL_CCM_ENV),
        "`CCM_ENV` 是一段任意 shell，只有 shell 解释得了 —— 这一条不许被收窄掉"
    );
    assert_eq!(
        needs_shell(&codex_bare, Some("claude --resume x")),
        Some(WHY_SHELL_RESOLVED),
        "后端答出的是一整条命令串，要 shell 拆词（`set -f; exec $cmd`）—— 这一条也不许被收窄掉"
    );
    // 优先级：`CCM_ENV` 先答（它是最外那一段），与 `render_direct` 的拼串顺序同向。
    assert_eq!(
        needs_shell(&with_env, Some("claude --resume x")),
        Some(WHY_SHELL_CCM_ENV),
        "两件都在时该先说最外那一段"
    );
}

/// ★ **每一条「为什么非得经 shell」都真有人用，而且只有一处用**（`D7` ＋ `D1`）。
///
/// 🔴 这几句话不是装饰：`sh` 不在这台机器上时，它们就是用户唯一看得到的**原因**。
/// 真机读数（`真相源/106 §3.3`）为此只能靠「同一个 launcher 在 claude 那趟 `EXIT=0`」
/// 反推出「找不到的是 `sh` 不是 `hostname`」—— **错的归因比失败本身更贵**。
///
/// 人群从源码派生（生产段里每一处 `WHY_SHELL_` 的出现），**相等**断言：
/// 声明一处 ＋ 取用一处 = 2。多了 ⇒ 同一句归因有第二处住址；少了 ⇒ 留了条用不上的话。
#[test]
fn every_reason_for_needing_a_shell_is_declared_once_and_used_once() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/mod.rs"
    ));
    crate::guard_support::assert_no_test_code("control/ccm/mod.rs", &prod);

    let reasons: &[(&str, &str)] = &[
        ("WHY_SHELL_CCM_ENV", WHY_SHELL_CCM_ENV),
        ("WHY_SHELL_RESOLVED", WHY_SHELL_RESOLVED),
        ("WHY_SHELL_BUS_ID", WHY_SHELL_BUS_ID),
        ("WHY_SHELL_ATTACH", WHY_SHELL_ATTACH),
        ("WHY_SHELL_CONTAINER_TAIL", WHY_SHELL_CONTAINER_TAIL),
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
    // 五句话两两不同：同一句话贴在两处出口上等于没有归因。
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
        prod.matches("Command::new(").count(),
        2,
        "本文件生产段的起进程口不是 2 处了 —— `readonly_guard::spawn_registry` 那个数要一起看"
    );
}

// ════════════════════════════════════════════════════════════════════════════════════════
// 〔S5 · 第四波〕直路上 `--ccm-sid` 没有载体时**说一句**（不报错、照常起）
// 要求住址：`调研/设计/99 §4.4` 那一行（「不报错 … 直路语义走已落地的启动期令牌那条路」）·
// `lib.rs::TARGET_GAPS` 旧话逐字「被接受、零效果、而且不出声」。
// ════════════════════════════════════════════════════════════════════════════════════════

/// 出声的只有「无载体」那一格；没给 `--ccm-sid` 的一趟一个字不多。
#[test]
fn a_direct_ccm_sid_without_a_carrier_says_so() {
    assert_eq!(
        direct_identity_note(&direct_of(&["--ccm-sid", "s-1"], None)),
        Some(DIRECT_SID_NO_CARRIER)
    );
    assert_eq!(direct_identity_note(&direct_of(&[], None)), None);
    assert!(
        DIRECT_SID_NO_CARRIER.contains("CCM_RBIND_TOKEN")
            && DIRECT_SID_NO_CARRIER.contains("照常起"),
        "那句话得说清靠什么认、以及这一趟照常起：{DIRECT_SID_NO_CARRIER}"
    );
}

/// 真跑那一侧真的读这一格 —— 防「字段有了、没人读」（那正是本件的原病：进了 `Opts`、零效果）。
/// 整行相等：撑大成别的表达式时那一行就不见了。
#[test]
fn exec_direct_really_reads_the_identity_cell() {
    let prod = crate::guard_support::production_code(own_source());
    let at = guard_core::pin_line(&prod, "if let Some(note) = direct_identity_note(d) {")
        .unwrap_or_else(|e| panic!("`exec_direct` 不再读直路身份那一格：{e}"));
    let fn_at = guard_core::pin_line(
        &prod,
        "fn exec_direct(d: &plan::Direct, resolved: Option<&str>) -> i32 {",
    )
    .expect("`exec_direct` 的签名变了");
    assert_eq!(
        at,
        fn_at + 1,
        "那一句不在 `exec_direct` 的第一行 —— 要在走 `sh -c` 那条岔路之前说（两条路都得出声）"
    );
}

// ── 〔WIN1 · 第四波 4D · RT1 F7〕`--ccm-probe` 自报的能力 = 这台机器上做得到的那一份 ──────────
//
// 要求住址：`设计/96 §2` 第 2 层「能力清单从实现派生」（`lib.rs` 头注那张表逐字「**能力清单从实现派生，
// `CAPABILITIES` 由它们汇总而来，不许手写**」）；读数出处 `第四波记录/RT1.md §8` F7 逐字「`ccm.exe --ccm-probe`
// 在 Windows 上自报 `tmux, attach, detach, tmux-size, tmux-base, bus-register` 等能力 —— 这些在 Windows 上都做不到」。
// 异源：Windows 那一格的期望是**手写的两张名单**（做得到 / 做不到），不从 `CCM_TMUX_CARRIED` 或
// `ccm_launcher_with` 里抠 —— 否则两侧同源、恒真。
// ⚠ 买不到：`TMUX_PLATFORM` 在本机是 `AskThePath`，Windows 那一档由入参模拟；真 `ccm.exe` 吐什么要真机
//    （`第四波记录/WIN1.md` 的虚拟机读数）。

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
        crate::TmuxPlatform::AbsentUnlessExeOnPath,
    ));
    let want_absent = [
        "tmux",
        "attach",
        "detach",
        "tmux-size",
        "tmux-base",
        "bus-register",
        "ccm-sid",
        "base-url-across-tmux",
    ];
    let want_present = [
        "new",
        "resume",
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
        crate::TmuxPlatform::AskThePath,
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
    let me = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/mod.rs"
    ));
    let body = me
        .split("pub(crate) fn probe_output(self_path: &str) -> String {")
        .nth(1)
        .and_then(|b| b.split("\n}\n").next())
        .expect("找不到 `probe_output` —— 抽取器坏了");
    assert_eq!(
        body.trim(),
        "probe_output_for(self_path, crate::TMUX_PLATFORM)",
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
        caps_line(&probe_output_for("/x/ccm", crate::TmuxPlatform::AskThePath))
    );
}
