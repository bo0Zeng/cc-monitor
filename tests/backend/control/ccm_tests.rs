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

// `under_the_name_ccm_only_backend_first_words_reach_the_backend`〔散文墓碑〕并进 `claude_flags_tests` 那一条（切法 ＋ 撞名收成一刀）。

/// 〔「路由不看 argv0」〕要求：「没有 `--` ⇒ 整行原样交 claude」＋「route 里没有 base ≠ ccm 那一支」。
/// 分流先看被叫成什么、再看参数：叫 `ccm` / 后端本名 ⇒ 零参数是「起一个 claude」，打头的 `--` 紧跟后端词才进后端；
/// 被叫成别的名字（`~/.cc-monitor/bin/<名>` 那条链接）⇒ 等同 `ccm @<名> …`，参数一个都不进后端。
/// 回环：pane 里把自己再叫一次（[`self_invocation`] ＋ 内层参数）叫得回 ccm 本身、参数一个不多一个不少、配置不叠第二遍。
#[test]
fn routing_reads_the_name_it_was_called_by_then_the_argv() {
    let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for me in [
        "ccm",
        "/x/ccm",
        "C:\\x\\ccm.exe",
        "/x/cc-monitor-backend",
        "cc-monitor-backend-4.1.2",
    ] {
        assert_eq!(
            route(me, &[]),
            Entry::Ccm(vec![]),
            "零参数不再是流模式（{me}）"
        );
        assert_eq!(
            route(me, &v(&["--stream"])),
            Entry::Ccm(v(&["--stream"])),
            "没有打头的 `--` 就交 claude"
        );
        assert_eq!(
            route(me, &v(&["--", "--stream", "--tail-only"])),
            Entry::Backend(v(&["--stream", "--tail-only"]))
        );
        assert_eq!(
            route(me, &v(&["--", "--ping"])),
            Entry::Backend(v(&["--ping"]))
        );
        assert_eq!(
            route(me, &v(&["--", "--ccm-tmux"])),
            Entry::Ccm(v(&["--", "--ccm-tmux"]))
        );
    }
    assert_eq!(
        route(
            "/home/u/.cc-monitor/bin/teamcct",
            &v(&["你好", "--", "--ccm-print"])
        ),
        Entry::Ccm(v(&["@teamcct", "你好", "--", "--ccm-print"]))
    );
    assert_eq!(
        route("teamcct", &v(&["--", "--ping"])),
        Entry::Ccm(v(&["@teamcct", "--", "--ping"])),
        "被叫成配置名的那一趟不进后端"
    );
    let inner = v(&["--tail-only", "--", "--cwd", "/p", "--ccm-agent", "claude"]);
    for (a0, me) in [
        ("/x/ccm", "/x/ccm"),
        ("/x/cc-monitor-backend", "/x/cc-monitor-backend"),
        ("/x/teamcct", "/x/ccm"),
        ("teamcct", "ccm"),
    ] {
        let mut again = self_invocation(&v(&[a0, "--", "--ccm-tmux=n"]));
        assert_eq!(again, v(&[me]), "「怎么叫我」（{a0}）");
        again.extend(inner.iter().cloned());
        assert_eq!(
            match route(&again[0], &again[1..]) {
                Entry::Ccm(v) => Some(v),
                Entry::Backend(_) => None,
            },
            Some(inner.clone()),
            "从 {a0} 进来的在 pane 里叫不回 ccm"
        );
    }
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
    let o = match argv::parse(&v(&[
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
    let Ok(Plan::Container(c)) =
        plan::build(&o, &Env::for_preview(), &AccountTable::default(), None)
    else {
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
        "`base-url-across-tmux` 不在 CAPABILITIES 里了 —— 容器路把继承来的中转地址带过 tmux 边界那件事\n\
             就**又没有申报了**，而它正是 `K-R61` 立件的原因（前提指着一个已经被删掉的文件）。"
    );

    // ② 实现这一半 —— 真算一遍，只喂替身环境，一个字节不碰这台机器（`K31`）。
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
        let Parsed::Opts(o) = argv::parse(&argv::tests::mixed_to_split(&args)).expect("该解析得动")
        else {
            panic!("`--tmux=n1` 不该被解析成 Early")
        };
        match plan::build(&o, env, &AccountTable::default(), None).expect("该算得出计划") {
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
    assert!(face("codex").needs_bus_id && !face("claude").needs_bus_id);
    assert!(face("claude").has_identity && !face("codex").has_identity);
    assert!(face("claude").has_pidfiles && !face("codex").has_pidfiles);
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
        inner: vec!["/usr/local/bin/ccm".into(), "--cwd".into(), "/p".into()],
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
// 要求：`--agent codex` 在 Windows 上不再 `program not found`。
// 真机读数（那一跳到底在哪）住。
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
    let o = match argv::parse(&argv::tests::mixed_to_split(&a)).expect("该解析得动") {
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
    let r: &str = &BUS_ID_RECIPE;
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
/// `--agent codex` 都要一个 `sh`。真机现打（Win11）：
/// `--agent codex --launcher hostname` → `EXIT=4 program not found`，
/// 同一个 launcher 在 `--agent claude` 那趟 `EXIT=0`。
///
/// ⚠ **本条买不到的那一维**：它判的是「这一趟要不要请 shell 进来」这个**判定**，
/// **不是**「在真 Windows 上真的起来了」。后者要那台 Win11 虚拟机，本路没去动它。
#[test]
fn only_two_things_still_need_a_posix_shell_and_the_codex_recipe_is_not_one_of_them() {
    // ── 🔴 P19 买的就是这一格：codex + 不在 tmux（Windows 上 `$TMUX` 恒空）────
    let codex_bare = direct_of(&["--ccm-agent", "codex", "--cwd", "/p"], None);
    // 反空真自检：`None` 必须是**因为守卫为假**得来的，不许是因为配方那一格自己没了。
    assert!(
        codex_bare.bus_id_recipe,
        "codex 这一趟连 `bus_id_recipe` 都是假的 —— 下面那条 `None` 会因为错的理由绿"
    );
    assert!(!codex_bare.inside_tmux, "夹具没把 `$TMUX` 置空，本格白测");
    assert_eq!(
        needs_shell(&codex_bare),
        None,
        "codex 不在 tmux 里还要请一个 `sh` 进来 —— 那正是 Windows 上那句 `program not found`"
    );

    // ── 在 tmux 里：配方真有事可做 ⇒ 照旧经 shell，且**说得出为什么** ─────────
    let codex_in_tmux = direct_of(
        &["--ccm-agent", "codex", "--cwd", "/p"],
        Some("/faux/socket,1,0"),
    );
    assert!(codex_in_tmux.inside_tmux, "夹具没把 `$TMUX` 置上，本格白测");
    assert_eq!(
        needs_shell(&codex_in_tmux),
        Some(WHY_SHELL_BUS_ID.as_str()),
        "在 tmux 里那段配方要现问一次 tmux ⇒ 这一趟**必须**经 shell（`INVARIANTS §33a` 那条实测例）"
    );

    // ── claude 两态都不要 shell（它压根没有那段配方）──────────────────────
    for tmux in [None, Some("/faux/socket,1,0")] {
        let d = direct_of(&["--ccm-agent", "claude", "--cwd", "/p"], tmux);
        assert!(!d.bus_id_recipe, "claude 不该有 cc-bus 配方");
        assert_eq!(
            needs_shell(&d),
            None,
            "claude 这一趟不该要 shell（tmux={tmux:?}）"
        );
    }

    // ── 另一件**是真的要 shell**，本轮没假装它不要（删了 `resolve` 那一件）──
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
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/mod.rs"
    ));
    crate::guard_support::assert_no_test_code("control/ccm/mod.rs", &prod);

    let reasons: &[(&str, &str)] = &[
        ("WHY_SHELL_CCM_ENV", WHY_SHELL_CCM_ENV.as_str()),
        ("WHY_SHELL_BUS_ID", WHY_SHELL_BUS_ID.as_str()),
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
        2,
        "本文件生产段的起进程口不是 2 处了 —— `readonly_guard::spawn_registry` 那个数要一起看"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 帧命令 `ccm-print`：别名预览
// 要求：「`ccm --print` 不跑、吐出等价的一行 shell ⇒ 生成器旁边显示**这条别名实际会执行什么**，
// 是真验证，不是前端拼串」。
// ═══════════════════════════════════════════════════════════════════════

/// **C1：预览与 `ccm --print` 是同一个计划函数。**
///
/// 源码那一半：`run` 与 [`answer_print`] 都经 [`plan_of`]（`plan::build(` 全文件恰好一处由
/// `the_name_avoidance_has_exactly_one_source_and_the_plan_settles_it` 钉着），预览那一处交的是预览环境、不继承账号。
/// 行为那一半（异源）：同一组参数，`answer_print` 的 `line` == 手搭一份「家目录里的新终端」环境、
/// 直接走 `argv::parse` → `plan::build` → `plan::render` 的产物（直路 · 容器路 · 显式不带账号三形）。
#[test]
fn the_alias_preview_is_the_same_plan_as_ccm_print() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/ccm/mod.rs"
    ));
    for pin in [
        "plan_of(&o, env, true)",
        "plan_of(&o, Env::for_preview(), false)",
    ] {
        guard_core::find_pinned(&prod, pin).unwrap_or_else(|e| {
            panic!("`{pin}` 不是恰好一处（{e}）—— 预览与 `--print` 不再走同一个计划函数")
        });
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for args in [
        // 别名的预置参数就是一条 argv（`<交给 claude 的…> -- <ccm 的…>`）。
        vec!["--", "--cwd", "/p", "--ccm-agent", "claude"],
        vec!["--", "--ccm-tmux=w5alias-preview-probe", "--cwd", "/p"],
        vec!["--model", "m", "--", "--base", "--cwd", "/q"],
    ] {
        let got = answer_print(&serde_json::json!({ "args": args }))
            .unwrap_or_else(|e| panic!("{args:?} 预览被拒：{e:?}"));
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let o = match argv::parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => o,
            other => panic!("{other:?}"),
        };
        let pick =
            |k: &str, d: String| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or(d);
        let e = Env {
            home: home.clone(),
            pwd: home.clone(),
            accts_manifest: plan::accts_manifest_under(&home),
            ccm_env: pick("CCM_ENV", argv::Defaults::ENV.to_string()),
            account_env: crate::agents::account_env_of(&o.agent)
                .unwrap_or_default()
                .to_string(),
            base_url_env: crate::agents::base_url_env_of(&o.agent)
                .unwrap_or_default()
                .to_string(),
            self_argv: vec!["ccm".into()],
            bus_scripts: plan::discover_bus_scripts(),
            ..Default::default()
        };
        // 账号表照这台机器的真值（预览要的就是这一点：「这台机器上敲这条别名」）—— 读法与真跑同一个 `AccountTable::load`。
        let table = if needs_account_table(&o, &e) {
            AccountTable::load(&e.accts_manifest)
        } else {
            AccountTable::default()
        };
        let plan = plan::build(&o, &e, &table, None).expect("该算得出计划");
        assert_eq!(
            got["line"].as_str().expect("line"),
            plan::render(&plan),
            "{args:?}：预览与同一语境下的 `--print` 不是同一行"
        );
    }
}

/// **C2：预览的语境逐格写死**（「从这台机器家目录里的一个新终端敲这条别名」）。各格一刀：
/// 叫的是 `ccm` · cwd = home · 不在 tmux 里 · 没有继承来的中转地址 / 启动号 / 账号目录。
#[test]
fn the_alias_preview_speaks_for_a_fresh_terminal_at_home() {
    let e = Env::for_preview();
    assert_eq!(
        e.self_argv,
        vec![SUBCOMMAND_WORD.to_string()],
        "别名叫的是 `ccm`"
    );
    assert_eq!(e.pwd, e.home, "不给 --cwd 的别名在家目录里敲");
    assert!(e.tmux.is_none(), "新终端不在 tmux 里");
    assert!(e.inherited_config_dir.is_none(), "账号目录变量不继承");
    assert!(
        e.inherited_base_url.is_none(),
        "常驻后端进程身上的中转地址不是那个终端的"
    );
    // 行为：不给 --cwd ⇒ 落在家目录；容器路内层叫回的是 `ccm`。
    let line = |args: &[&str]| -> String {
        answer_print(&serde_json::json!({ "args": args })).expect("该答得出")["line"]
            .as_str()
            .expect("line")
            .to_string()
    };
    let home = std::env::var("HOME").unwrap_or_default();
    assert!(
        line(&["--", "--ccm-agent", "claude"]).contains(&plan::qarg(&home)),
        "不给 --cwd 却没落在家目录"
    );
    let boxed = line(&["--", "--ccm-tmux=w5alias-preview-probe", "--cwd", "/p"]);
    assert!(
        boxed.contains("'ccm' '--' '--cwd'"),
        "容器路内层没有叫回 `ccm`：{boxed}"
    );
}

/// `ccm-print` 的两种拒：形状不对 ⇒ `bad_args`；ccm 自己拒 / 不起会话的那一形 ⇒ `refused`，原话带回。
#[test]
fn the_alias_preview_refuses_in_the_words_of_ccm() {
    let code = |v: serde_json::Value| answer_print(&v).map(|_| "ok").unwrap_or_else(|(c, _)| c);
    assert_eq!(code(serde_json::json!({})), "bad_args");
    assert_eq!(code(serde_json::json!({ "args": [1] })), "bad_args");
    assert_eq!(
        code(serde_json::json!({ "args": vec!["x"; PRINT_MAX_WORDS + 1] })),
        "bad_args"
    );
    assert_eq!(
        code(serde_json::json!({ "args": ["--", "--ccm-help"] })),
        "refused"
    );
    // 未知旗标交给 claude、不拒 ⇒ 拿一条 ccm 自己的组合规则当「拒」的样本。
    let (c, said) = answer_print(&serde_json::json!({ "args": ["--", "--detach"] }))
        .expect_err("--detach 不带 --tmux 该被拒");
    assert_eq!(c, "refused");
    let want = match argv::parse(&["--".to_string(), "--detach".to_string()]) {
        Err(argv::Die(m)) => m,
        other => panic!("{other:?}"),
    };
    assert_eq!(said, want, "拒的那句不是 ccm 自己的原话");
    assert_eq!(
        code(serde_json::json!({ "args": ["--", "--cwd", "/p"] })),
        "ok"
    );
    // 右边认不得 ⇒ ccm 的原话拒（不猜）。
    assert_eq!(
        code(serde_json::json!({ "args": ["--", "--model", "m"] })),
        "refused"
    );
}

// ── `--ccm-probe` 自报的能力 = 这台机器上做得到的那一份 ──────────
//
// 第 2 层「能力清单从实现派生」（`lib.rs` 头注那张表逐字「**能力清单从实现派生，
// `CAPABILITIES` 由它们汇总而来，不许手写**」）；读数要求：「`ccm.exe --ccm-probe`
// 在 Windows 上自报 `tmux, attach, detach, tmux-size, tmux-base, bus-register` 等能力 —— 这些在 Windows 上都做不到」。
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

/// 〔`INVARIANTS §49`〕**codex 的 cc-bus 身份配方读会话名时，那个 tmux 客户端是 UTF-8 客户端。**
///
/// 要求住址：`INVARIANTS §49`「本仓每一处按格式串读 tmux 打印通道的调用点，起的 tmux 客户端都必须是 UTF-8 客户端」·
/// `TL2.md §9.1` 3（`BUS_ID_RECIPE` 读 `#S` 是五处真违反之一）。
///
/// 真跑：隔离 socket（显式 `-S` ＋ `-f /dev/null`，不碰默认 socket）起一个**中文名**会话，pane 里整条命令跑在
/// `LC_ALL=C` 下（tmux 只看 `LC_ALL`→`LC_CTYPE`→`LANG` 第一个非空值有没有 `UTF-8`）。生产那一段配方原样执行，
/// `CC_BUS_ID` 必须逐字节就是会话名。
///
/// ⚠ **台架为什么要一个 shim 把 tmux 客户端那一侧的 `TMUX` 摘掉**〔SH1 09-26 现打，tmux 3.6〕：
/// `TMUX` 已设的客户端，tmux **一律按 UTF-8 打**，不看 locale（`env -i LC_ALL=C TMUX=/x,1,0 tmux … display-message -p` 读出原样中文；
/// 摘掉 `TMUX` 就是 `_`）。而这段配方整段裹在 `[ -n "${TMUX:-}" ]` 里 ⇒ 生产上它恒在 `TMUX` 已设时跑 ⇒
/// **在 3.6 上它今天其实没被改写**，TL2 记的「真违反」是**潜伏**的：靠的是一条手册里没写的启发式（`K-R12` 的实验室当年
/// 在 tmux 外面量，没碰到这一格）。旗保证的是**不靠它**。⇒ 台架必须把那条启发式关掉，否则下面的反向正控立不住、
/// 那条相等是空真（本条第一版就是这么红在反向正控上的）。
/// ★ 反向正控：同一个 pane、同一个 locale、同一个 shim，**不带旗**的 `display-message -p "#S"` 必须被改写。
#[test]
fn the_bus_id_recipe_reads_the_session_name_through_a_utf8_client() {
    let dir = std::env::temp_dir().join(format!("ccm-sh1-u8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("shim")).expect("建台架目录");
    let sock = dir.join("sock");
    let real = std::process::Command::new("sh")
        .args(["-c", "command -v tmux"])
        .output()
        .expect("sh 不可执行");
    let real = String::from_utf8_lossy(&real.stdout).trim().to_string();
    assert!(
        !real.is_empty(),
        "找不到 tmux —— 本测试要求环境有 tmux（刻意不静默跳过）"
    );
    let shim = dir.join("shim").join("tmux");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nunset TMUX\nexec '{real}' -S '{}' \"$@\"\n",
            sock.display()
        ),
    )
    .expect("写 shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
            .expect("chmod shim");
    }
    let name = "u8甲乙";
    let script = format!(
        "{recipe} printf '%s' \"$CC_BUS_ID\" > '{d}/id'; \
         printf '%s' \"$(tmux display-message -p '#S')\" > '{d}/raw'; touch '{d}/done'; exec sleep 30",
        recipe = BUS_ID_RECIPE.as_str(),
        d = dir.display()
    );
    let path = format!(
        "{}:{}",
        dir.join("shim").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let tmux = |args: &[&str]| {
        std::process::Command::new(&real)
            .arg("-S")
            .arg(&sock)
            .args(args)
            .output()
            .expect("tmux 不可执行")
    };
    let path_kv = format!("PATH={path}");
    let out = tmux(&[
        "-f",
        "/dev/null",
        "new-session",
        "-d",
        "-s",
        name,
        "env",
        "LC_ALL=C",
        "LANG=C",
        "LC_CTYPE=C",
        &path_kv,
        "sh",
        "-c",
        &script,
    ]);
    assert!(
        out.status.success(),
        "隔离 socket 上建会话失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 夹具侧的有界等待（pane 里那条命令由另一个进程跑完）；等不到就失败，不静默跳过。
    for _ in 0..250 {
        if dir.join("done").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let id = std::fs::read(dir.join("id"));
    let raw = std::fs::read(dir.join("raw"));
    let _ = tmux(&["kill-server"]);
    let _ = std::fs::remove_dir_all(&dir);
    let id = id.expect("配方那一趟没跑完（`id` 没写出来）");
    let raw = raw.expect("反向正控那一趟没跑完（`raw` 没写出来）");
    assert!(
        !raw.is_empty() && raw != name.as_bytes(),
        "反向正控没立住：不带旗的 `display-message` 在这个台架上也读得出中文（或什么都没读到：{raw:?}）—— \
         台架不是非 UTF-8 客户端，下面那条相等是空真"
    );
    assert_eq!(
        String::from_utf8_lossy(&id),
        name,
        "配方在非 UTF-8 客户端下读出的会话名被改写了 —— `BUS_ID_RECIPE` 的 `display-message` 没带 UTF-8 旗（`INVARIANTS §49`）"
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
    assert_eq!(v["build"].as_str(), Some(crate::BUILD_ID));
    assert_eq!(line("build"), crate::BUILD_ID);
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

/// 要求：「tmux 名派生 ＋ 撞名避让只留后端，前端要名字就问后端」。
/// 帧命令 `terminal-name-mint` 两形各一格 == 手写期望；避让问的是**交进来那张快照**（被占 ⇒ 往后排）；没装 tmux ⇒ 基名；入参不恰一格 ⇒ `bad_args`。
#[test]
fn the_mint_frame_derives_here_and_steps_aside_on_this_machines_snapshot() {
    use crate::common::session_snapshot::{SessionRow, SessionSnapshot};
    use serde_json::json;
    let snap = SessionSnapshot::with_prober(|| {
        Ok(["proj-cc", "proj-fork-cc", "proj-fork-cc-2"]
            .iter()
            .map(|n| SessionRow {
                name: (*n).to_string(),
                ccm_sid: String::new(),
            })
            .collect())
    });
    let mint = |a: serde_json::Value| {
        terminal_name_mint_with(&a, &snap).map(|m| serde_json::to_value(m).unwrap())
    };
    assert_eq!(
        mint(json!({"cwd": "/home/u/proj"})).unwrap(),
        json!({"name": "proj-cc-2"})
    );
    assert_eq!(
        mint(json!({"cwd": "/home/u/other"})).unwrap(),
        json!({"name": "other-cc"})
    );
    assert_eq!(
        mint(json!({"forkOf": "proj-cc"})).unwrap(),
        json!({"name": "proj-fork-cc-3"})
    );
    assert_eq!(
        mint(json!({"forkOf": "/home/u/gone"})).unwrap(),
        json!({"name": "gone-fork-cc"})
    );
    let no_tmux = SessionSnapshot::with_prober(|| Err(("no_tmux", "tmux: not found".to_string())));
    assert_eq!(
        terminal_name_mint_with(&json!({"cwd": "/home/u/proj"}), &no_tmux)
            .unwrap()
            .name,
        "proj-cc"
    );
    for bad in [
        json!({}),
        json!({"cwd": "/a", "forkOf": "b"}),
        json!({"cwd": 3}),
    ] {
        assert_eq!(mint(bad).unwrap_err().0, "bad_args");
    }
}

// ── 起会话只有 ccm 一处（用户原话「所有的起会话都是一处ccm」）──────────────────────────────

/// 一段生产代码里「直接起 agent 的命令」的痕迹（名字从注册表取，不写死）：
/// ① 串字面量以某一家的默认启动器打头（`"claude"` · `"claude …"`）；② 串字面量里带「启动器 ＋ 这一家的 resume 词」
/// （`claude --resume` · `codex resume`）；③ 调这一家的 resume 命令形（`.resume_command)(`，拼出来的就是 `<启动器> <resume 词> <sid>`）。
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
    hits.extend(
        code.matches(".resume_command)(")
            .map(|_| "调了这一家的 resume 命令形".to_string()),
    );
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
                    .chain(std::iter::repeat('#').take(hashes))
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
/// 两向相等：命中 == 登记的那一处（`control/resolve_query.rs`：给仓外终端客户端的那份冻结契约 `--resolve`，
/// 它的 `command` 今天仍是 `<启动器> --resume <sid>`，改成 `ccm …` 是一次跨仓契约变更 —— 待定，不在这里悄悄改）。
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
         let d = '\"'; let e = r#\"{l} {r} s\"#; let f = (face.resume_command)(&base, sid);",
        l = launchers[0],
        r = tokens[0]
    );
    assert_eq!(
        direct_agent_launch_hits(&corpus, &launchers, &tokens).len(),
        4,
        "针在现造语料上失准"
    );

    let root = crate::guard_support::src_root();
    let mut outside: Vec<String> = Vec::new();
    let mut registry_hits = 0;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let under = path.strip_prefix(&root).unwrap_or(&path);
        let rel = under.to_string_lossy().replace('\\', "/");
        let code = guard_core::strip_comment_lines(&crate::guard_support::production_code(&src));
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
        ["control/resolve_query.rs：调了这一家的 resume 命令形"],
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
        let Parsed::Opts(o) = argv::parse(&argv::tests::mixed_to_split(&a)).expect("该解析得动")
        else {
            panic!("不该是 Early")
        };
        plan::render(&plan::build(&o, &e, &AccountTable::default(), None).expect("计划"))
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

impl crate::guard_support::Shaped for Minted {
    fn samples() -> Vec<Self> {
        vec![Minted {
            name: "proj-cc-2".into(),
        }]
    }
}
