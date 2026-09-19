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
        intercept("/opt/cc-monitor-remote", &sub),
        Some(vec!["resume".to_string()])
    );
    // 不是 ccm ⇒ 一律放行给流模式 / wire 子命令
    assert_eq!(intercept("/opt/cc-monitor-remote", &none), None);
    assert_eq!(
        intercept("/opt/cc-monitor-remote", &vec!["--ping".to_string()]),
        None
    );
    assert_eq!(intercept("/opt/ccmonitor", &none), None, "子串不算");
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
        self_path: "/usr/local/bin/ccm".into(),
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
    assert_eq!(RBIND_TITLE_FORMAT, "#{?@ccm_sid,ccm-rbind-#{@ccm_sid},#T}");
    assert!(
        RBIND_TITLE_FORMAT.starts_with("#{?@ccm_sid,"),
        "它必须是**条件式**：有 @ccm_sid 才出 marker，没有才回退 #T"
    );
    assert!(
        RBIND_TITLE_FORMAT.contains("ccm-rbind-#{@ccm_sid}"),
        "marker 必须逐字是 monitor 侧 bind.rs 要扫的那个前缀 + sid"
    );
    assert!(
        RBIND_TITLE_FORMAT.ends_with(",#T}"),
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
        launch.contains(RBIND_TITLE_FORMAT),
        "`control/launch.rs` 的生产段里找不到这个格式串的逐字副本：\n  {RBIND_TITLE_FORMAT}\n             两份已经漂开了（或者那一行被收口成了标识符 —— 别那么做，理由见本条头注）。"
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
