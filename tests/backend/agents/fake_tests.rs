use super::*;

/// 夹具根：按 pid + tag 隔开。
///
/// ⚠ 建在临时目录里：用户 08-14 明令**不要动生产** ——
/// 这份夹具与 `~/.cc-monitor/`、`~/.claude/` 一个字节都不相干。
fn fixture_root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ccm-s6-{tag}-{}", std::process::id()))
}

// ⚠ 各格自己的 home 指针：**裸 fn，不读环境变量**。
//
// 为什么不用 `CCM_FAKE_AGENT_HOME`：`std::env::set_var` 在多线程测试进程里与
// 别的线程的 `getenv` 竞争（同进程里 `claudecode::home()` / `codex::home()` 正在读
// `HOME`/`CLAUDE_CONFIG_DIR`/`CODEX_HOME`）。`Adapter.home` 本来就是**裸函数指针**
// ——`S5` 选函数指针的一个副产物就是「假 agent 不需要任何运行时装配」。⇒ 直接给指针。
fn home_of_announce() -> Option<PathBuf> {
    Some(fixture_root("announce"))
}
fn home_of_walk() -> Option<PathBuf> {
    Some(fixture_root("walk"))
}
fn home_of_hole() -> Option<PathBuf> {
    Some(fixture_root("hole"))
}

/// 建一份**假 agent 自己布局**的 home（`convos/` + `live/` + `fake-config.json`）。
fn build_fixture(tag: &str) -> PathBuf {
    let root = fixture_root(tag);
    let _ = std::fs::remove_dir_all(&root);
    let proj = records_root(&root).join("-home-u-proj");
    std::fs::create_dir_all(&proj).expect("建会话记录根");
    std::fs::create_dir_all(pidfile_root(&root)).expect("建 pidfile 目录");
    std::fs::write(
        proj.join(session_file_name(FIXTURE_SESSION_ID)),
        "{\"role\":\"user\",\"text\":\"hi\"}\n\
             {\"role\":\"agent\",\"usage\":{\"in\":11,\"out\":22}}\n",
    )
    .expect("写会话记录");
    std::fs::write(
        pidfile_root(&root).join("424242.json"),
        "{\"pid\":424242,\"cwd\":\"/home/u/proj\"}\n",
    )
    .expect("写 pidfile");
    std::fs::write(
        root.join("fake-config.json"),
        format!("{{\"trusted\":{{\"{FIXTURE_CWD}\":true}}}}"),
    )
    .expect("写账号配置");
    root
}

fn caps_with(home_ptr: fn() -> Option<PathBuf>) -> FakeCaps {
    FakeCaps {
        home: Some(home_ptr),
        ..FakeCaps::full()
    }
}

/// `S6-Z1`：12 种能力**全部实现**，且**每一种都与两家真 agent 不同形**。
///
/// # 为什么"不同形"是本件的地基，而不是趣味
///
/// 假 agent 若照抄 Claude 的布局（`projects/` + `.jsonl`），通用层拿 Claude 的知识
/// 去解释它的 home **恰好也能读出东西** —— 那时"走通全流程"证明的是
/// 「Claude 的布局被施加到了另一个目录上」，不是「通用层能容纳第二种布局」。
/// ⇒ 少了这一格，`S6` 的正题随时可能退化成一个**粉饰的通过**。
#[test]
fn every_fake_capability_differs_in_shape_from_both_real_agents() {
    // ⚠ 不叫 `home`：本模块的能力 8 就叫 `home()`，遮住它这一格就验不了它。
    let h = Path::new("/h");
    use crate::agents::{claudecode as cc, codex as cx};

    // 1 会话记录根 / 4 pidfile 目录：与 Claude 的两个根都不同，彼此也不同。
    assert_ne!(
        records_root(h),
        cc::paths::projects_root(h),
        "会话记录根与 Claude 同形"
    );
    assert_ne!(
        records_root(h),
        cc::paths::sessions_root(h),
        "会话记录根撞上了 Claude 的 pidfile 目录"
    );
    assert_ne!(
        pidfile_root(h),
        cc::paths::sessions_root(h),
        "pidfile 目录与 Claude 同形"
    );
    assert_ne!(pidfile_root(h), records_root(h), "本层自己两个根撞了");

    // 2 会话文件判定：两边**互相认不出对方的记录**（双向，否则只证明了一半）。
    let mine = Path::new("/h/convos/p").join(session_file_name(FIXTURE_SESSION_ID));
    let theirs =
        Path::new("/h/projects/p").join(cc::records::session_file_name(FIXTURE_SESSION_ID));
    assert!(is_session_file(&mine), "本层认不出自己的记录：{mine:?}");
    assert!(
        !cc::records::is_session_file(&mine),
        "Claude 的判定认出了本层的记录 —— 两种布局同形，`S6` 的正题会变成粉饰的通过：{mine:?}"
    );
    assert!(
        !is_session_file(&theirs),
        "本层认出了 Claude 的记录：{theirs:?}"
    );
    assert!(
        cc::records::is_session_file(&theirs),
        "对照坏了：Claude 认不出自己的记录"
    );

    // 3 会话文件命名
    assert_ne!(
        session_file_name(FIXTURE_SESSION_ID),
        cc::records::session_file_name(FIXTURE_SESSION_ID),
        "会话文件命名与 Claude 同形"
    );

    // 5 账号环境变量名
    assert_ne!(
        ACCOUNT_DIR_ENV,
        cc::paths::CONFIG_DIR_ENV,
        "账号环境变量名与 Claude 同名"
    );

    // 6 账号信任判定：文件名与字段路径都不同 —— 拿 Claude 的形状喂本层要判成"不信任"。
    let root = build_fixture("trust");
    assert_eq!(trust_of_config(&root, FIXTURE_CWD), Ok(true));
    assert_eq!(trust_of_config(&root, "/somewhere/else"), Ok(false));
    std::fs::write(
        root.join("fake-config.json"),
        format!("{{\"projects\":{{\"{FIXTURE_CWD}\":{{\"hasTrustDialogAccepted\":true}}}}}}"),
    )
    .expect("写 Claude 形状的配置");
    assert_eq!(
        trust_of_config(&root, FIXTURE_CWD),
        Ok(false),
        "本层认得懂 Claude 形状的信任字段 —— 那两种「账号知识」其实是同一种"
    );
    let _ = std::fs::remove_dir_all(&root);

    // 7 判活 cmdline：两边的**放行集不同**，否则"判活"这一格没有独立内容。
    assert!(cmdline_may_be_agent("fakeagent revive x"));
    assert!(
        !cmdline_may_be_agent("/usr/bin/node /x/claude"),
        "本层放行了 Claude 的 cmdline"
    );
    assert!(
        cc::liveness::cmdline_may_be_agent("/usr/bin/node /x/claude"),
        "对照坏了：Claude 那家不再认自己的 cmdline"
    );

    // 8 解析本机 home：Claude 那家**恒有值**（有默认），本层**没有默认**。
    // 这条差别让 `visible_among` 里 `None` 那条分支第一次由一家真实的适配层走过。
    assert!(
        cc::home().is_some(),
        "Claude 那家不再恒 Some 了 —— 本条的对照失效"
    );
    if std::env::var_os(HOME_ENV).is_none() {
        // 正常状态：这个变量只有本夹具会用，机器上不会有人设它。
        assert!(home().is_none(), "本层在没有 {HOME_ENV} 时不该答得出 home");
    }

    // 10/11/12 resume 三件事：与 Claude **和** Codex 都不同 ——
    // 与 Codex 也不同才排除了"它只是第三个 codex"。
    assert_ne!(DEFAULT_COMMAND, cc::resume::DEFAULT_COMMAND);
    assert_ne!(DEFAULT_COMMAND, cx::resume::DEFAULT_COMMAND);
    assert_ne!(SESSION_NAME_PREFIX, cc::resume::SESSION_NAME_PREFIX);
    assert_ne!(SESSION_NAME_PREFIX, cx::resume::SESSION_NAME_PREFIX);
    assert_ne!(
        resume_command("b", "s"),
        cc::resume::resume_command("b", "s")
    );
    assert_ne!(
        resume_command("b", "s"),
        cx::resume::resume_command("b", "s"),
        "resume 命令形与 Codex 同形 —— 那样它就只是第三个 codex"
    );

    // 能力清单与 `FakeCaps` 的字段**不许漂开**：挖每一个洞都要真的少一种能力。
    let full = FakeCaps::full();
    assert_eq!(
        full.present(),
        CAPABILITIES.len(),
        "`FakeCaps::full()` 交出来的能力数与 `CAPABILITIES` 对不上 —— 两处漂开了"
    );
    for cap in CAPABILITIES {
        assert_eq!(
            full.without(cap).present(),
            CAPABILITIES.len() - 1,
            "挖「{cap}」这个洞没有挖到任何东西"
        );
    }
}

/// `S6-Z2`（正题上半）：一个**全新的 agent 被发现、进得了 `hello.homes`** ——
/// 而通用层**一行没改**。
///
/// ⚠ 这一段是**真的过通用层**：`agents::visible_among` 的判准与 `wire::Frame::Hello`
/// 的序列化都是通用层的机器，本件没动它们一个字节。
/// 它是 `G1` 成功标准②今天**唯一真正成立**的那一格。
#[test]
fn a_brand_new_agent_is_discovered_and_announced_with_zero_general_layer_change() {
    let root = build_fixture("announce");
    let adapter = crate::agents::Adapter {
        kind: AGENT_KIND,
        account_env: None,
        // 最小假 agent 没有资产面（它要证的是「通用层零改动」，不是资产）。
        assets: None,
        history: None,
        upstream: None,
        mcp: None,
        home: home_of_announce,
        footprint: None,
        accounts: None,
        records: None,
        processes: None,
        launch: None,
    };
    let discovered = crate::agents::visible_among(std::slice::from_ref(&adapter));
    assert_eq!(
        discovered.len(),
        1,
        "通用层的发现判准没有认出这家新 agent —— 而它的 home 目录就在那儿：{root:?}"
    );
    assert_eq!(discovered[0].agent_kind, AGENT_KIND);
    assert_eq!(discovered[0].path, root.to_string_lossy());

    let line = crate::stream::wire::to_line(&crate::stream::wire::Frame::Hello {
        v: 1,
        build_id: "s6".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: discovered,
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        // `K-P4`（09-04）：同上 —— 空表省略，期望字节不变。
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .expect("填了第三家的 hello 必须序列化得出来");
    assert_eq!(
        line,
        format!(
            "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"s6\",\"host_arch\":\"x86_64\",\
                 \"claude_dir\":\"/c\",\"homes\":[{{\"agent_kind\":\"fake\",\"path\":{}}}]}}\n",
            serde_json::to_string(&root.to_string_lossy().into_owned()).unwrap()
        ),
        "第三个 agent 塞进 `Hello.homes` 之后字节形状不对 —— \
             `S4` 立的通用形状（agent 维度落在**值**里）没有真的容下第三家"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `S6-Z2`（正题下半）：12 种能力**凑得出一条完整的路**。
///
/// `L2` 的钉法逐字：「`S6` 的最小假 agent —— 它只实现这组接口，
/// **若接口漏了什么，`S6` 走不通就会红**」。本格就是那条钉法。
///
/// ⚠ **边界写在明处**：后五段**没有过通用层**（见 [`walk`] 头注）。
/// 本条证明的是接口面**够用**（`D4` 反推出来的 12 种能力不缺），
/// **不是**通用层**能用**它们 —— 后者今天不成立，差距 27 处逐条登记在
/// `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`。
#[test]
fn the_twelve_reverse_derived_capabilities_are_enough_to_finish_the_pipeline() {
    let root = build_fixture("walk");
    let done = walk(&caps_with(home_of_walk), &root);
    assert_eq!(
        done.as_deref(),
        Ok(STAGES),
        "12 种能力走不完全流程 —— 那说明 `L2` 反推出来的接口面**漏了东西**"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `S6-Z4`（反向夹具）：**删掉任何一种能力，流程都在一个说得出话的地方停**。
///
/// # 它守的是件里逐字禁掉的那种坏法
///
/// 「不许静默当成"这个 agent 没有会话"」。本条对 12 种能力**逐一**挖洞，
/// 每次都要求停下来的地方**点得出段名与能力名**，且点的是**挖掉的那一种**。
///
/// ⚠ **两向都验**（铁律：只验"逮得住"会引进假阳）：
/// ① 正向对照是上面那条 `…are_enough_to_finish_the_pipeline`（不挖洞就走得完）；
/// ② 本格内部再验一次「停的位置**随挖的洞变**」—— 一个在第一段恒停的 driver
///    对每个洞都会"红"，那种红是假的。
///
/// ⚠ 对照着看的是**通用层今天的反应**：同样是"这家的布局它不认识"，
/// 用量聚合那条查询 / `search_query::run` / `--session-accounts` 一律 **rc=0 + 零输出**
/// （实测读数见 `PR-S6.md`）—— 那正是**静默**。两者的差就是 `L2` 要补的东西。
#[test]
fn removing_any_one_capability_stops_the_flow_somewhere_that_can_name_it() {
    let root = build_fixture("hole");
    let full = caps_with(home_of_hole);
    let mut stopped_at: Vec<(&str, &'static str, &'static str)> = Vec::new();
    for cap in CAPABILITIES {
        match walk(&full.without(cap), &root) {
            Ok(done) => panic!(
                "挖掉能力「{cap}」之后流程仍然走完了 {done:?} —— \
                     少一种能力却一路绿灯，那正是件里禁掉的「静默当成这个 agent 没有会话」"
            ),
            Err(Stop::MissingCapability { stage, capability }) => {
                assert!(
                    STAGES.contains(&stage),
                    "停在一个不认识的段名 `{stage}`（挖的是「{cap}」）"
                );
                assert!(
                    CAPABILITIES.contains(&capability),
                    "停下来时报的能力名 `{capability}` 不在登记表里（挖的是「{cap}」）"
                );
                stopped_at.push((cap, stage, capability));
            }
        }
    }
    // 停的位置必须**随挖的洞变**：全停在同一段 = driver 在第一段就恒停，
    // 那种"会红"对挖了哪个洞根本不敏感 —— 是假的。
    let distinct_stages: std::collections::BTreeSet<&str> =
        stopped_at.iter().map(|(_, s, _)| *s).collect();
    assert!(
        distinct_stages.len() >= 5,
        "12 个洞只停出了 {} 个不同的段：{stopped_at:?}\n\
             ⇒ driver 对「挖了哪个洞」不敏感，本格的「会红」是假的",
        distinct_stages.len()
    );
    // 点名点错了人，运维照着去修会修错地方 —— 说得出话但说错了话，比说不出话更坏。
    for (holed, stage, named) in &stopped_at {
        assert_eq!(
            holed, named,
            "挖的是「{holed}」，停下来却点名「{named}」（段 `{stage}`）"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// `S6-Z3`（本件最要紧的一格）：**通用层今天怎么回答一个不是 Claude 的 agent** —— 实测。
///
/// # 它逮的是件里逐字禁掉的那种坏法，而且逮到了
///
/// 件里写着反向夹具「不许**静默**当成"这个 agent 没有会话"」。
/// 本格把一个装好的、有会话有 pidfile 有账号配置的新 agent 的 home 直接喂给通用层的
/// 四条一次性入口，读它们的实际反应 —— 实测三种，**只有一种算说得出话**：
///
/// | 入口 | 今天的反应 |
/// |---|---|
/// | `history_query --list-projects` | rc=2 + 报错，但措辞是 **Claude 的布局**（`<home>/projects`） |
/// | `search_query --search` | **rc=0、零输出** ⇒ 静默 |
/// | `accounts_query --session-accounts` | **rc=0、零行** ⇒ 静默 |
/// | `resolve_query`（`agentKind:"fake"`，生产注册表里没有这一家） | **rc=0，返回默认那一家的 `claude --resume <sid>`** |
///
/// 最后一条：resume 已经按注册表里那一家的那一格拼了（注册了的家走它自己的命令形，见
/// `the_ccm_plan_and_resume_read_only_the_launch_face_for_every_family`）；**没注册**的 kind 照线上契约落默认那一家
///（缺 / 空 / 认不出 = 默认那家）。对一个没进注册表的第三家，那仍是误路由 —— 要改得改契约，不在这里。
///
/// ⚠ 本格**不是**在说这些入口有 bug —— 它们今天的契约就是「只服务一种 agent」。
/// 它记的是：`G1` 成功标准②今天差的那 27 处，**每一处的失败长什么样**。
/// 收接口那轮要补的错误出口，规格就在这里和 `NEW_AGENT_BLOCKERS` 那一列里。
#[test]
fn the_general_layer_answers_a_non_claude_agent_silently_or_with_claudes_words() {
    let root = build_fixture("probe");
    // 前提：这家的数据是**真的在那儿**，而 Claude 的布局在这个 home 下**不存在**。
    // 少了这两句，下面的"零输出"可能只是因为夹具是空的（台架空转）。
    assert!(
        records_root(&root).join("-home-u-proj").is_dir(),
        "夹具没建起来，本格在空转"
    );
    assert!(
        !crate::agents::claudecode::paths::projects_root(&root).exists(),
        "夹具里出现了 Claude 的 `projects/` —— 那样下面测的就不是「第二种布局」了"
    );

    // ① 会话读：说得出话，但说的是**别人的话**（措辞是 Claude 的目录布局）。
    let rc = crate::observe::history_query::run(&root, &["--list-projects".to_string()]);
    assert_eq!(
        rc, 2,
        "`--list-projects` 对一个布局不同的 agent 不再报错了 —— \
             那它就退化成了「静默零输出」，比现在更坏"
    );

    // ② 搜索：**静默**（rc=0）。
    let rc = crate::observe::search_query::run(&root, &["--search".to_string(), "hi".to_string()]);
    assert_eq!(
        rc, 0,
        "`--search` 的反应变了 —— 本格记的是「今天它 rc=0 零输出」这个事实，\
             变了就该同轮改 `NEW_AGENT_BLOCKERS` 的失败形态那一列"
    );

    // ③ 账号：**静默**（rc=0、零行）。
    // ⚠ `--accts-dir` 显式指到夹具里一个不存在的目录：不指的话它会去读**用户真实的**
    //   `~/.claude-alt`（用户 08-14 明令不碰生产）。
    let no_accts = root.join("no-such-accts");
    let rc = crate::observe::accounts_query::run(
        &root,
        &[
            "--session-accounts".to_string(),
            "--accts-dir".to_string(),
            no_accts.to_string_lossy().into_owned(),
        ],
    );
    assert_eq!(rc, 0, "`--session-accounts` 的反应变了");

    // ④ resume：生产注册表里没有这一家 ⇒ 照线上契约落默认那一家。
    let spec = format!("{{\"agentKind\":\"{AGENT_KIND}\",\"sessionId\":\"{FIXTURE_SESSION_ID}\"}}");
    let plan = crate::control::resolve_query::resolve_json_for_inbound(&spec)
        .expect("`--resolve` 对注册表里没有的 agentKind 不报错（落默认那一家）");
    assert_eq!(
        plan["command"],
        serde_json::json!(format!("claude --resume {FIXTURE_SESSION_ID}")),
        "\n`--resolve` 对注册表里没有的 `agentKind:\"{AGENT_KIND}\"` 的返回变了。\n\
             本格记的事实是：**它不报错，落默认那一家**（注册表里声明 `is_default` 的那一家）。实得：{plan:?}"
    );
    assert_eq!(
        plan["sessionName"],
        serde_json::json!(format!("cc-{}", &FIXTURE_SESSION_ID[..8])),
        "会话名前缀也被**静默**给成了 Claude 的 `cc-`（这家自己的是 `{SESSION_NAME_PREFIX}-`）"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// 一张含夹具家的注册表：生产那几家的起会话事实 ＋ 本假 agent 的（其余各面这几条用不到，留空）。
fn registry_with_fake() -> Vec<crate::agents::Adapter> {
    let row = |kind, home, launch| crate::agents::Adapter {
        kind,
        home,
        account_env: None,
        assets: None,
        history: None,
        upstream: None,
        mcp: None,
        footprint: None,
        accounts: None,
        records: None,
        processes: None,
        launch,
    };
    let mut reg: Vec<crate::agents::Adapter> = crate::agents::REGISTRY
        .iter()
        .map(|a| row(a.kind, a.home, a.launch))
        .collect();
    reg.push(row(AGENT_KIND, home_of_announce, Some(LAUNCH)));
    reg
}

/// 起会话那几格：**通用层只看能力、不看名字**。
///
/// 用户逐字：「思考怎么解耦, 不要硬适配claude code」「如果是其他agent呢, 比如codex」。
/// 喂一张含本假 agent 的注册表（它的组合与两家都不同：要 cc-bus 身份 · 有身份面 · 弹信任框、话也不同 · 不留 pidfile），
/// 每一家都过同一条 ccm 规划与 resume 解析，每一格的产出都必须等于**那一家**声明的那一格。
/// 通用层若在哪一格上按名字认人，本假 agent 那一行就会答错。
#[test]
fn the_ccm_plan_and_resume_read_only_the_launch_face_for_every_family() {
    use crate::control::ccm::argv::{self, Parsed};
    use crate::control::ccm::plan::{self, AccountTable, Env, Plan};
    let reg = registry_with_fake();
    let env = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    fn scan(_: Option<&Path>) -> Vec<(String, u32)> {
        vec![("sid-1".into(), 4242)]
    }
    let opts = |agent: &str, args: &[&str]| -> argv::Opts {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let Parsed::Opts(mut o) = argv::parse(&argv::tests::v138_to_v151(&a)).expect("解析得动")
        else {
            panic!("该是一趟起会话")
        };
        // 解析那一关认的是生产注册表；这里要的是「同一份意图换一家起」。
        o.agent = agent.to_string();
        *o
    };
    let t = AccountTable::default();
    let mut seen: Vec<&str> = Vec::new();
    for a in &reg {
        let face = a.launch.expect("这张注册表里每一家都带起会话事实");
        seen.push(a.kind);
        // 直路：启动器 · 嵌套标记 · cc-bus 身份 · 身份面。
        let Ok(Plan::Direct(d)) = plan::build_among(&reg, &opts(a.kind, &[]), &env, &t, None)
        else {
            panic!("{} 该是直路", a.kind)
        };
        assert_eq!(
            d.argv.first().map(String::as_str),
            Some(face.default_launcher),
            "{}",
            a.kind
        );
        assert_eq!(d.nested, face.nested_env.to_vec(), "{} 的嵌套标记", a.kind);
        assert_eq!(
            d.bus_id_recipe, face.needs_bus_id,
            "{} 的 cc-bus 身份",
            a.kind
        );
        assert_eq!(d.has_identity, face.has_identity, "{} 的身份面", a.kind);
        // 容器路：信任框轮询认的就是这一家那句话。
        let Ok(Plan::Container(c)) = plan::build_among(
            &reg,
            &opts(a.kind, &["--ccm-tmux=w-x", "--detach"]),
            &env,
            &t,
            None,
        ) else {
            panic!("{} 该是容器路", a.kind)
        };
        assert_eq!(c.trust_prompt, face.trust_prompt, "{} 的信任框", a.kind);
        let tail = plan::render_container_tail(&c);
        match face.trust_prompt {
            Some(p) => assert!(
                tail.contains(&format!("grep -q '{p}'")),
                "{} 声明了信任框，收尾却没有轮询它：{tail}",
                a.kind
            ),
            None => assert!(
                !tail.contains("grep -q"),
                "{} 没有信任框却挂了轮询：{tail}",
                a.kind
            ),
        }
        // resume 前问「是不是已在别处跑着」：只对留 pidfile 的那一家问。
        let mut e = env.clone();
        e.running_sessions = Some(scan);
        let refused =
            plan::build_among(&reg, &opts(a.kind, &["--resume", "sid-1"]), &e, &t, None).is_err();
        assert_eq!(refused, face.has_pidfiles, "{} 的 pidfile 那一格", a.kind);
        // resume 规格：命令形与会话名前缀是这一家的。
        let spec = format!(
            "{{\"agentKind\":\"{}\",\"sessionId\":\"{FIXTURE_SESSION_ID}\"}}",
            a.kind
        );
        let rp = crate::control::resolve_query::resolve_json_among(&reg, &spec).expect("解析得动");
        assert_eq!(
            rp["command"],
            serde_json::json!((face.resume_command)(
                face.default_launcher,
                FIXTURE_SESSION_ID
            )),
            "{} 的 resume 命令",
            a.kind
        );
        assert_eq!(
            rp["sessionName"],
            serde_json::json!(format!(
                "{}-{}",
                face.session_name_prefix,
                &FIXTURE_SESSION_ID[..8]
            )),
            "{} 的会话名前缀",
            a.kind
        );
    }
    assert_eq!(
        seen,
        ["claude", "codex", AGENT_KIND],
        "这张注册表的人群变了"
    );
    // 本假 agent 声明了要预信任 ⇒ 它真被预信任；组合与两家都不同（不是第三个 claude / codex）。
    assert_eq!(LAUNCH.trust_prompt, Some("Trust this workspace?"));
    let differs = |k: &str| {
        let f = reg
            .iter()
            .find(|a| a.kind == k)
            .and_then(|a| a.launch)
            .expect("在注册表里");
        (
            f.needs_bus_id,
            f.has_identity,
            f.has_pidfiles,
            f.trust_prompt.is_some(),
        ) != (
            LAUNCH.needs_bus_id,
            LAUNCH.has_identity,
            LAUNCH.has_pidfiles,
            LAUNCH.trust_prompt.is_some(),
        )
    };
    assert!(
        differs("claude") && differs("codex"),
        "假 agent 的组合与某一家真 agent 同形 —— 那样它证不了通用层只看能力"
    );
    // 默认那一家由注册表声明：本假 agent 不是默认，生产那张恰一家是。
    assert_eq!(
        crate::agents::default_launch_among(&reg).map(|(k, _)| k),
        Some(crate::agents::default_kind())
    );
}

/// `S6-Z5`：**夹具家永远不上生产** —— 判据两向。
///
/// # 没有这一条，`agents/fake/` 就是一条捷径
///
/// 本层进了 `agent_locality_guard::HOMES`（那是四条判据共用的**排除表**）——
/// 也就是说它整层的格式知识都不被判据①扫。这是对的（它确实是一个 agent 家），
/// 但代价必须付：它**不许**混进 `agents::REGISTRY`，也**不许**丢掉那行 `#[cfg(test)]`。
/// 前者混进去 ⇒ 真 `hello.homes` 会声明一个根本不存在的 agent；
/// 后者丢掉 ⇒ 生产二进制里凭空多出一个假 agent 的知识，而没有任何东西会说。
#[test]
fn the_fixture_agent_never_ships() {
    // ① 不在生产注册表里。
    let kinds: Vec<&str> = crate::agents::REGISTRY.iter().map(|a| a.kind).collect();
    assert!(
        !kinds.contains(&AGENT_KIND),
        "夹具 agent `{AGENT_KIND}` 混进了生产注册表 `agents::REGISTRY`：{kinds:?}\n\
             ⇒ 真填 `hello.homes` 那天，backend 会向仓外消费方声明一个**不存在**的 agent。"
    );
    // ② 模块声明必须带 `#[cfg(test)]` —— 生产二进制里零字节。
    let mod_rs = std::fs::read_to_string(crate::guard_support::src_root().join("agents/mod.rs"))
        .expect("读 agents/mod.rs");
    let decl = format!("mod {AGENT_KIND};");
    let idx = mod_rs.find(&decl).unwrap_or_else(|| {
        panic!("`agents/mod.rs` 里找不到 `{decl}` —— 夹具家的模块声明改了形状，本条在空转")
    });
    let head = &mod_rs[..idx];
    let prev_line = head.lines().next_back().unwrap_or_default();
    let prev_prev = head.lines().nth_back(1).unwrap_or_default();
    assert!(
        prev_line.contains("cfg(test)") || prev_prev.contains("cfg(test)"),
        "`{decl}` 上面没有 `#[cfg(test)]` —— 夹具 agent 会被编进生产二进制。\n\
             上一行：{prev_line:?}\n上上行：{prev_prev:?}"
    );
}

/// **fake 适配器同拍长一格 MCP 读**：通用层经注册表那一格读到假 agent 的 MCP 条目 ——
/// 通用层（`agents::mcp_read_among`）不认识 `.fake-mcp.json` 这个名字，认它的只有假 agent 自己。
/// 要求：「`agents::Adapter` 长一格『MCP 读』（fake 适配器同拍）」。
#[test]
fn the_fake_agents_mcp_face_is_read_through_the_generic_layer() {
    let dir = std::env::temp_dir().join(format!("sh1-fake-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建夹具目录");
    std::fs::write(
        dir.join(".fake-mcp.json"),
        r#"{"servers":{"alpha":{"command":"/x"}}}"#,
    )
    .expect("写夹具");
    let adapter = crate::agents::Adapter {
        kind: AGENT_KIND,
        account_env: None,
        assets: None,
        history: None,
        upstream: None,
        mcp: Some(super::MCP),
        home: home_of_announce,
        footprint: None,
        accounts: None,
        records: None,
        processes: None,
        launch: None,
    };
    let got = crate::agents::mcp_read_among(std::slice::from_ref(&adapter), Some(&dir));
    let _ = std::fs::remove_dir_all(&dir);
    let got = got.expect("注册表里有 MCP 读面，通用层却没读它");
    assert_eq!(got.entries.len(), 1, "{got:?}");
    assert_eq!(
        (got.entries[0].scope, got.entries[0].name.as_str()),
        ("project", "alpha")
    );
    // 反向：没有这一格的注册表 ⇒ 通用层说「没有」，不回一份空的当成读过了。
    let bare = crate::agents::Adapter {
        mcp: None,
        ..adapter
    };
    assert!(crate::agents::mcp_read_among(
        std::slice::from_ref(&bare),
        Some(std::path::Path::new("/"))
    )
    .is_none());
}
