//! # 要求住址：`INVARIANTS §33a`（`ccm --print` 是平价预言机：说的就是真跑做的，而且是纯的）
//!
//! 核原文：`INVARIANTS §33a` 铁律 1 逐字「凡是真 exec 路会设置的、**ccm 自己决定的环境变量**，`--print` **必须**说出来，且**顺序对齐**」，
//! 铁律 2 逐字「**`--print` 仍然必须是纯的**」—— 段序 · 机器级 env 在前 · print 与真跑读同一个计划 · 在不在 tmux 不进 print 那一侧，判的正是这两条。
//! 次住址（逐字核过）：会话名那几条对「给会话起一个**你认得出来**的名字」；attach 目标对 `INVARIANTS §31a` 的精确形态；
//! 默认 cwd 是恒等对；登记不成要出声对 `D4`；账号库解析不动不说成没有对 `D7`；
//! 容器收尾先自检对「确认放在 ccm 收尾最前面」。账号四条路 / quote / `set -f` / BOM 那几条是普通单测，没有逐字原文。〔JA1 点址 2026-09-24〕

use super::*;
use crate::control::ccm::argv::Parsed;

/// 本文件的夹具沿用 V138 写法（ccm 选项在前）⇒ 喂解析器之前换成 V151 排列（意图逐词不变）。
fn parse(a: &[String]) -> Result<Parsed, crate::control::ccm::argv::Die> {
    crate::control::ccm::argv::parse(&crate::control::ccm::argv::tests::v138_to_v151(a))
}

fn env() -> Env {
    Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    }
}

fn plan_of(args: &[&str], env: &Env, t: &AccountTable) -> Plan {
    plan_of_with(args, env, t, None)
}

/// 造一份**固定快照**的 `TakenNames`。
///
/// 🔴 只能这么造 —— `TakenNames` 的字段是 `common::session_snapshot` 私有的，
/// 本模块（含测试段）**写不出第二种构造法**。那正是 `KR96D2` 第一刀要的形状：
/// 「铸名另起一份名字集合」在这里是**编译不过**，不是靠注释劝阻。
fn snapshot_of(names: &[&str]) -> TakenNames {
    let rows: Vec<crate::common::session_snapshot::SessionRow> = names
        .iter()
        .map(|n| crate::common::session_snapshot::SessionRow {
            name: (*n).to_string(),
            ccm_sid: String::new(),
        })
        .collect();
    crate::common::session_snapshot::SessionSnapshot::with_prober(move || Ok(rows.clone()))
        .taken_names()
        .expect("固定夹具问得到")
}

fn plan_of_with(args: &[&str], env: &Env, t: &AccountTable, taken: Option<&TakenNames>) -> Plan {
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match parse(&a).expect("该解析得动") {
        Parsed::Opts(o) => build(&o, env, t, taken).expect("该算得出计划"),
        other => panic!("{other:?}"),
    }
}

fn printed(args: &[&str]) -> String {
    render(&plan_of(args, &env(), &AccountTable::default()))
}

/// 〔搬自 `ccm-cli`「零修饰」「resume <sid>」「--launcher 覆盖」「--base」「--model」
/// 「--agent codex」「-- 透传」与 `ccm-contract-parity` B 组的顺序那几条〕
///
/// 这一条钉的是**一条起会话命令长什么样**：段的顺序就是契约
/// （CCM_ENV → CC_BUS_ID → 账号目录 → 清嵌套 → cd → exec）。
/// `--resume` / `--model` 不再是 ccm 的：原样接在启动器后面（从前 resume 由 ccm 拼、`--model` 变成 export `ANTHROPIC_MODEL`）。
#[test]
fn the_shape_of_one_launch_command_line() {
    let nested =
        "unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION";
    assert_eq!(
        printed(&["--cwd", "/p"]),
        format!("{nested}; cd '/p' && exec claude")
    );
    assert_eq!(
        printed(&["--resume", "abc-123", "--cwd", "/p", "--launcher", "claude"]),
        format!("{nested}; cd '/p' && exec claude --resume abc-123")
    );
    assert_eq!(
        printed(&["--cwd", "/p", "--base"]),
        format!("unset CLAUDE_CONFIG_DIR; {nested}; cd '/p' && exec claude")
    );
    assert_eq!(
        printed(&["--cwd", "/p", "--model", "opus"]),
        format!("{nested}; cd '/p' && exec claude --model opus")
    );
    // codex：换启动器 + **不清** claude 的嵌套标记 + cc-bus 身份配方
    assert_eq!(
        printed(&["--cwd", "/p", "--ccm-agent", "codex"]),
        format!("{} cd '/p' && exec codex", *super::super::BUS_ID_RECIPE)
    );
    // 透传参数含特殊字符 ⇒ 正确 quote
    assert_eq!(
        printed(&["--cwd", "/p", "--", "-p", "hi there"]),
        format!("{nested}; cd '/p' && exec claude -p 'hi there'")
    );
    // V138：不写 `--` 也一样交出去。
    assert_eq!(
        printed(&["-p", "hi there", "--cwd", "/p"]),
        format!("{nested}; cd '/p' && exec claude -p 'hi there'")
    );
}

/// 〔搬自 `ccm-contract-parity`「claude 不得被注入 CC_BUS_ID」〕
#[test]
fn only_codex_gets_the_bus_id_recipe() {
    assert!(!printed(&["--cwd", "/p"]).contains("CC_BUS_ID"));
    assert!(printed(&["--cwd", "/p", "--ccm-agent", "codex"]).contains("CC_BUS_ID"));
}

/// 〔搬自 `ccm-cli` 账号那一族：显式 / 继承 / 默认号 / --base 四条路〕
///
/// ✅ 最后那条（裸终端落默认号）**是用户 09-12 裁定要的行为**（`DECISIONS.md#R28`），
/// 不是病灶；③ 那条（不许覆盖继承）是同一裁的另一半。见 [`resolve_account`] 头注。
#[test]
fn the_four_ways_an_account_gets_picked() {
    let dz = tempdir();
    let db = tempdir();
    let t = table(&[
        ("z", Some(dz.as_str()), true),
        ("b", Some(db.as_str()), false),
    ]);
    let mut e = env();
    // ① 显式 --account 赢
    assert!(render(&plan_of(&["--cwd", "/p", "--account", "b"], &e, &t))
        .contains(&format!("export CLAUDE_CONFIG_DIR='{db}'")));
    // ② 裸终端（无继承）⇒ 落 manifest 默认号 z
    assert!(render(&plan_of(&["--cwd", "/p"], &e, &t))
        .contains(&format!("export CLAUDE_CONFIG_DIR='{dz}'")));
    // ③ 外层已继承 ⇒ **保留继承的**，不被默认号静默覆盖（`R08` 的原病）
    e.inherited_config_dir = Some(db.clone());
    let line = render(&plan_of(&["--cwd", "/p"], &e, &t));
    assert!(
        !line.contains("export CLAUDE_CONFIG_DIR"),
        "不许覆盖继承：{line}"
    );
    // ④ --base 显式清空，不受继承影响
    assert!(
        render(&plan_of(&["--cwd", "/p", "--base"], &e, &t)).contains("unset CLAUDE_CONFIG_DIR")
    );
    // ⑤ 显式 --account 压过继承
    assert!(render(&plan_of(&["--cwd", "/p", "--account", "z"], &e, &t))
        .contains(&format!("export CLAUDE_CONFIG_DIR='{dz}'")));
}

/// 〔搬自 `ccm-cli`「账号不存在 → 中止」「可用列表」「无账号库 → 退化为基座」〕
#[test]
fn picking_an_account_never_falls_back_to_a_different_one() {
    let dz = tempdir();
    let t = table(&[("z", Some(dz.as_str()), true), ("f", None, false)]);
    let a: Vec<String> = ["--cwd", "/p", "--account", "nope"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let o = match parse(&a).expect("解析得动") {
        Parsed::Opts(o) => o,
        other => panic!("{other:?}"),
    };
    let Die(msg) = build(&o, &env(), &t, None).expect_err("不存在的账号必须中止");
    assert!(msg.starts_with("账号 'nope' 不可用"), "{msg}");
    assert!(
        msg.contains("可用: z f"),
        "报不可用必须说出有哪些可用：{msg}"
    );
    // 无账号库 ⇒ 一个字都不说，退化为基座（没有 CLAUDE_CONFIG_DIR 注入）
    let empty = AccountTable::default();
    assert!(!render(&plan_of(&["--cwd", "/p"], &env(), &empty)).contains("CLAUDE_CONFIG_DIR="));
}

/// 〔搬自 `ccm-cli`「deriveTmuxName 对拍」那 5 条（跨语言双写点的**本侧**）〕
///
/// ⚠ **如实边界**：bash 那版是拿 `npx tsx` 真跑前端那个函数来对拍的（跨语言）。
/// 这里只钉**本侧**的规则，**跨语言那一半没了** —— 登记在件文件 `§8`，不许读成等价。
#[test]
fn the_session_name_derivation_rule() {
    assert_eq!(derive_tmux_name("/home/pi/proj"), "proj-cc");
    assert_eq!(derive_tmux_name("/home/pi/a  b"), "a-b-cc");
    assert_eq!(derive_tmux_name("/home/pi/proj///"), "proj-cc");
    assert_eq!(derive_tmux_name("/"), "session-cc");
    assert_eq!(derive_tmux_name("/home/pi/.hidden.dir"), "hidden-dir-cc");
    // 截 32 之后再剥首尾 `-`（顺序承重：先剥后截会留下一个尾 `-`）
    assert_eq!(
        derive_tmux_name(&format!("/x/{}", "a".repeat(40))),
        format!("{}-cc", "a".repeat(32))
    );
}

/// 要求：「tmux 名派生 ＋ 撞名避让只留后端」—— 分叉那条的基名随派生一起搬来（原 `fork-launch.vitest.ts::forkTmuxName` 那一族）。
/// 基名必须与源名不同（同名 ⇒ `ccm` 把新会话接进原窗口）；拿 cwd 当源也必须是一个**建得出来**的新会话名（真正的消费者 [`validate_tmux_name`] 收得下）。
#[test]
fn the_fork_base_differs_from_its_source_and_is_always_a_legal_new_name() {
    assert_eq!(fork_tmux_base("myproj-cc"), "myproj-fork-cc");
    assert_eq!(fork_tmux_base("bare"), "bare-fork-cc");
    assert_eq!(fork_tmux_base("/home/pi/proj"), "proj-fork-cc");
    assert_eq!(fork_tmux_base("/"), "session-fork-cc");
    assert_eq!(fork_tmux_base(""), "session-fork-cc");
    let taken = snapshot_of(&["myproj-fork-cc", "myproj-fork-cc-2"]);
    assert_eq!(
        mint_tmux_name(&fork_tmux_base("myproj-cc"), &taken),
        "myproj-fork-cc-3"
    );
    for src in [
        "/home/pi/proj",
        "/tmp/e2e-remote",
        "/p/my proj",
        "/",
        "",
        "中文目录",
        "myproj-cc",
    ] {
        let n = fork_tmux_base(src);
        assert_ne!(n, src);
        assert!(
            validate_tmux_name(&n).is_ok(),
            "源 {src:?} 产出了建不出来的名字 {n:?}"
        );
        assert!(n.ends_with("-cc"), "{n} 丢了 -cc 形状");
    }
}

/// 〔搬自 `ccm-cli` / `cc-spawn-uplift` 的取名那一族〕**三条取名路的退让态度不一样。**
///
/// 🔴 这一条是**自查逮到的**（铁律 15 那一拍）：头一版原生实现**整个没有退让**，
/// 于是 `--tmux-base=<基名>`（`C15` 给 cc-spawn 的那条路，它的全部意义就是「撞了就退让」）
/// **静默退化成了 `--tmux=<名>`** —— 写了个修饰、看起来生效了、实际被吃掉。
#[test]
fn only_two_of_the_three_naming_paths_step_aside_on_a_collision() {
    let e = env();
    let t = AccountTable::default();
    // 🔴 `K-R96`：量的是**最终名**，不是那个「要不要退让」的布尔。
    //    原版量布尔 ⇒ 「布尔为 true 而退让根本没被执行」照样绿 ——
    //    那正好是这条判据当初逮到的那个病（退让被静默吃掉）换个位置复发。
    let taken = snapshot_of(&["n1", "proj-cc"]);
    let path = |args: &[&str]| match plan_of_with(args, &e, &t, Some(&taken)) {
        Plan::Container(c) => c.name,
        other => panic!("该是容器路：{other:?}"),
    };
    // 显式名：**不退让** —— 调用方说的就是要这个名，撞了走 C14 响亮失败
    assert_eq!(path(&["--ccm-tmux=n1", "--cwd", "/p"]), "n1");
    // 基名：退让（cc-spawn 随后要读回真名字）
    assert_eq!(path(&["--tmux-base", "n1", "--cwd", "/p"]), "n1-2");
    // 不给名、从 cwd 派生：退让
    assert_eq!(path(&["--ccm-tmux", "--cwd", "/x/proj"]), "proj-cc-2");
    // 问不到快照（`None`）⇒ 诚实降级成「不退让」，不是「假装没占」之外的第三种行为
    assert_eq!(
        match plan_of_with(&["--ccm-tmux", "--cwd", "/x/proj"], &e, &t, None) {
            Plan::Container(c) => c.name,
            other => panic!("该是容器路：{other:?}"),
        },
        "proj-cc"
    );
    // 退让规则本身
    assert_eq!(next_free_name("n1", &[]), "n1");
    assert_eq!(next_free_name("n1", &["other".into()]), "n1");
    assert_eq!(next_free_name("n1", &["n1".into()]), "n1-2");
    assert_eq!(next_free_name("n1", &["n1".into(), "n1-2".into()]), "n1-3");
    // 只撞中间那个不影响：2 空着就取 2
    assert_eq!(next_free_name("n1", &["n1".into(), "n1-3".into()]), "n1-2");
}

/// ★★ **`KR96D2` 死值验第二刀：`--print` 相对于快照是**纯**的。**
///
/// 口径由 `§0c` 定死：**同一份快照 ＋ 同一份输入 ⇒ 同一份输出**（不是「不查实时状态」）。
/// 判据喂的就是一个**固定夹具快照**。
#[test]
fn printing_twice_against_the_same_snapshot_gives_the_same_line() {
    let e = env();
    let t = AccountTable::default();
    let taken = snapshot_of(&["proj-cc", "proj-cc-2"]);
    let once = render(&plan_of_with(
        &["--ccm-tmux", "--cwd", "/x/proj"],
        &e,
        &t,
        Some(&taken),
    ));
    let twice = render(&plan_of_with(
        &["--ccm-tmux", "--cwd", "/x/proj"],
        &e,
        &t,
        Some(&taken),
    ));
    assert_eq!(once, twice, "同一份快照喂两次，`--print` 吐了两样东西");
    assert!(
        once.contains("proj-cc-3"),
        "喂进去的快照里 `proj-cc` 与 `proj-cc-2` 都占着，名字该让到 `proj-cc-3`。\n             实得：{once}"
    );
}

/// ★★ **`KR96D2` 死值验第三刀：快照变了，名字必须跟着变。**
///
/// 这一条是上一条的**反面**，缺了它「纯」就退化成「恒定」——
/// 一个把名字写死的实现能同时通过「喂两次一样」和「不查实时状态」。
#[test]
fn a_different_snapshot_moves_the_name() {
    let e = env();
    let t = AccountTable::default();
    let name = |taken: &TakenNames| match plan_of_with(
        &["--ccm-tmux", "--cwd", "/x/proj"],
        &e,
        &t,
        Some(taken),
    ) {
        Plan::Container(c) => c.name,
        other => panic!("该是容器路：{other:?}"),
    };
    assert_eq!(name(&snapshot_of(&[])), "proj-cc");
    assert_eq!(name(&snapshot_of(&["proj-cc"])), "proj-cc-2");
    assert_eq!(name(&snapshot_of(&["proj-cc", "proj-cc-2"])), "proj-cc-3");
}

/// ★★ **`KR96D3`：名字是 `<项目名>-cc`，sid 一个片段都不许进去；而 `@ccm_sid` 必须还在。**
///
/// 第四刀（把 `@ccm_sid` 也一起去掉 ⇒ 必须红）就在下半段：
/// sid **必须还在**，只是**不在名字里** —— 它的载体是 tmux 的 `@ccm_sid` 选项。
#[test]
fn the_session_name_reads_like_a_project_and_the_sid_rides_the_tmux_option() {
    let e = env();
    let t = AccountTable::default();
    const SID: &str = "cb3230f3-dead-beef-0000-111122223333";
    let plan = plan_of_with(
        &[
            "--resume",
            SID,
            "--ccm-sid",
            SID,
            "--ccm-tmux",
            "--cwd",
            "/home/pi/my-proj",
        ],
        &e,
        &t,
        Some(&snapshot_of(&[])),
    );
    let Plan::Container(c) = plan else {
        panic!("该是容器路")
    };
    assert_eq!(c.name, "my-proj-cc", "名字该读得出是哪个项目");
    // ① 名字里出现 sid 片段 ⇒ 红。逐字扫**每一个** ≥4 字符的前缀，不是只看 8 位那一种。
    let mut checked = 0usize;
    for k in 4..=SID.len() {
        let frag = &SID[..k];
        checked += 1;
        assert!(
            !c.name.contains(frag),
            "会话名 {:?} 里带着 sid 片段 {frag:?} —— 用户 `R55` 裁定一逐字：\n                 「**要是可读的名字 / 不要id**」",
            c.name
        );
    }
    assert!(
        checked > 30,
        "只扫了 {checked} 个片段 —— 扫描器坏了，本条在空转"
    );
    // ④ 而 sid 本身**必须还在**：它骑在 `@ccm_sid` 上（`P3` 逐字：名字不是 sid 的载体）。
    assert_eq!(
        c.ccm_sid, SID,
        "sid 从计划里消失了 —— 「不进名字」不等于「不要了」。\n             `@ccm_sid` 是它真正的载体，杀会话的菜单与身份判定都认那个。"
    );
}

/// 〔搬自 `ccm-cli` 名字校验那一族〕—— 会话名会被拼进 tmux 目标语法，是一条注入面。
///
/// 要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；①形。
///
/// 规则今天住 `control/gate_rules.rs`（全仓唯一一份）；本函数只剩「说哪一句」。多出来两格（欺骗字符 · 超过 128）各一条，
/// 正控加两个真实会走的名字（带空格 · 中文），免得三句话焊成恒拒。
#[test]
fn a_session_name_that_would_confuse_tmux_is_refused() {
    for ok in ["ok-name", "my session", "项目", &"a".repeat(128)] {
        assert!(validate_tmux_name(ok).is_ok(), "{ok:?} 该建得出来");
    }
    for bad in ["", "-lead", "a*b", "a?b", "a.b", "a:b", "a=b", "a\u{1}b"] {
        assert!(validate_tmux_name(bad).is_err(), "'{bad}' 不该被放行");
    }
    let deceptive = validate_tmux_name("a\u{202e}b").expect_err("欺骗字符该拒");
    assert!(deceptive.0.contains("看不见的字符"), "{}", deceptive.0);
    let long = validate_tmux_name(&"a".repeat(129)).expect_err("超过 128 该拒");
    assert!(long.0.contains("128"), "{}", long.0);
}

/// 〔搬自 `ccm-print-parity` 全 5 个场景 ＋ `ccm-cli` R08 那 5 条〕
///
/// 容器路：外层 tmux 编排必须带上会话名 / cwd / 意图标 / 内层载荷，
/// 而**内层载荷必须显式带上账号**（不能靠继承穿 tmux 边界 —— `R08` 的原病）。
#[test]
fn the_container_path_carries_every_intent_inward() {
    let db = tempdir();
    let t = table(&[
        ("z", Some(db.as_str()), true),
        ("b", Some(db.as_str()), false),
    ]);
    let mut e = env();
    e.inherited_config_dir = Some(db.clone());
    let p = plan_of(
        &[
            "--resume",
            "p1",
            "--ccm-tmux=cc-p1",
            "--cwd",
            "/tmp",
            "--ccm-sid",
            "p1",
            "--base",
        ],
        &e,
        &t,
    );
    let out = render(&p);
    let Plan::Container(c) = &p else {
        panic!("该是容器路：{p:?}")
    };
    assert!(out.contains("-s 'cc-p1'"), "会话名没进 new-session：{out}");
    assert!(out.contains("-c '/tmp'"), "cwd 没进 -c：{out}");
    assert!(out.contains("@ccm_sid_expect 'p1'"), "意图标没打：{out}");
    assert!(
        !out.contains("@ccm_sid ") && !out.contains("@ccm_sid'"),
        "不许写**事实**标记 @ccm_sid（那是通道 B 的，破坏性动作只认它）：{out}"
    );
    // 内层载荷：按 argv 元素逐个 quote 过一层，所以判的是 payload 本身
    // V138：`--resume` 是透传，内层放在 `--` 后面原样交出去。
    assert!(
        // 内层 `self <交给 claude 的…> -- <ccm 的…>`：`--resume` 在 `--` 左边。
        c.payload.contains("/ccm' '--resume' 'p1' '--' '--cwd'"),
        "resume 没进内层：{}",
        c.payload
    );
    assert!(
        c.payload.contains("'--base'"),
        "--base 没进内层（账号维度恒显式表态）：{}",
        c.payload
    );
    assert!(c.payload.contains("'--ccm-sid' 'p1'"), "{}", c.payload);
    // 〔搬自 `ccm-print-parity` 场景 newTmuxCustomLauncher / resumeTmuxWithModel〕
    let p4 = plan_of(
        &[
            "--ccm-tmux=n4",
            "--cwd",
            "/p",
            "--launcher",
            "CCMPROBE",
            "--model",
            "opus",
        ],
        &env(),
        &AccountTable::default(),
    );
    let Plan::Container(c4) = &p4 else {
        panic!("该是容器路")
    };
    assert!(
        c4.payload.contains("'--launcher' 'CCMPROBE'"),
        "{}",
        c4.payload
    );
    assert!(
        c4.payload.contains("/ccm' '--model' 'opus' '--' '--cwd'"), // `--model` 在 `--` 左边
        "{}",
        c4.payload
    );
    // 继承账号那条路：内层必须显式 export 继承来的那个目录
    let p2 = plan_of(&["--ccm-tmux=n1", "--cwd", "/p"], &e, &t);
    let Plan::Container(c2) = &p2 else {
        panic!("该是容器路")
    };
    assert!(
        c2.payload
            .starts_with(&format!("export CLAUDE_CONFIG_DIR={}; ", sq(&db))),
        "内层没把继承来的账号显式化（tmux 边界会吃掉它）：{}",
        c2.payload
    );
    // 而且绝不能落到默认号 z 上
    assert!(
        !c2.payload.contains("'--account'"),
        "不许悄悄换成默认号：{}",
        c2.payload
    );
    // 裸终端（无继承）⇒ 内层仍落默认号 z（粘滞体验）
    let mut e2 = env();
    e2.inherited_config_dir = None;
    let p3 = plan_of(&["--ccm-tmux=n1", "--cwd", "/p"], &e2, &t);
    let Plan::Container(c3) = &p3 else {
        panic!("该是容器路")
    };
    assert!(
        c3.payload.contains("'--account' 'z'"),
        "裸终端该落默认号：{}",
        c3.payload
    );
}

/// 🔴 **三个变量必须被显式化到载荷内侧** —— tmux 的进程边界会吃掉外层那句 `export`。
///
/// # 这一条是 `K-R48` 第二拍补的，补的是**别人家的判据搬过来时空出来的那一格**
///
/// 从前盯这件事的是 monitor 侧 `backend/control/payload.rs` 的两条：
/// `the_ccm_container_path_forwards_the_relay_base_url_across_the_tmux_boundary` 与
/// `…_forwards_the_launch_identity_…`。它们的做法是**把 `shared/ccm` 里那段 bash
/// 窗口原样交给 `bash` 跑一遍**再读载荷 —— 脚本删了，那两条连被测对象都没有了。
///
/// ⚠ **[`the_container_path_carries_every_intent_inward`] 顶不了这一格**：
/// 它只钉了 `CLAUDE_CONFIG_DIR`（账号那条），另两个**一个字都没提**。
/// 差点就这么丢了 —— 而丢掉的后果逐字住 `K-R48` `§0c`：
/// 「`CCM_LAUNCH_ID` 被吃掉 ⇒ 身份 token 丢」（**今天真有生产人群**）。
///
/// # 为什么三条一起钉、且要**非空对照**
///
/// 「没设那个变量 ⇒ 不加前缀」与「设了 ⇒ 加前缀」必须成对：只钉后者的话，
/// 「无条件加一个空 export」也能全绿，而那会把内层的值**清空**（比不转发更坏）。
#[test]
fn the_container_path_forwards_every_inherited_variable_inward() {
    let db = tempdir();
    let base = |e: &Env| -> String {
        let p = plan_of(
            &["--ccm-tmux=n1", "--cwd", "/p"],
            e,
            &AccountTable::default(),
        );
        let Plan::Container(c) = p else {
            panic!("该是容器路")
        };
        c.payload
    };
    // 非空对照：三个都没有 ⇒ 载荷就是裸 argv（证明这把尺子不是恒带前缀）。
    let clean = base(&env());
    assert!(
        !clean.contains("export "),
        "三个变量都没设，载荷却带了 export —— 那会把内层的值清空：{clean}"
    );
    // ① 继承来的账号目录（`R08` 07-28，真机复现过的静默换号）。
    let mut e1 = env();
    e1.inherited_config_dir = Some(db.clone());
    assert!(
        base(&e1).starts_with(&format!("export CLAUDE_CONFIG_DIR={}; ", sq(&db))),
        "继承来的账号没被显式化：{}",
        base(&e1)
    );
    // ② 中转地址（`K-H2b` 08-28）。
    let mut e2 = env();
    e2.anthropic_base_url = Some("https://relay.example/v1".into());
    assert!(
        base(&e2).starts_with("export ANTHROPIC_BASE_URL='https://relay.example/v1'; "),
        "中转地址没被显式化 ⇒ 走 tmux 的会话静默不走中转：{}",
        base(&e2)
    );
    // ③ 身份 token（`K-P5c` 09-02）—— 三个里**今天真有生产人群**的那一个。
    let mut e3 = env();
    e3.ccm_launch_id = Some("L-42".into());
    assert!(
        base(&e3).starts_with("export CCM_LAUNCH_ID='L-42'; "),
        "身份 token 没被显式化 ⇒ cc-monitor 认不出这个会话：{}",
        base(&e3)
    );
    // ④ 值必须经 `sq`（带引号 / 空格的值不许把载荷拆断）。
    let mut e4 = env();
    e4.ccm_launch_id = Some("it's here".into());
    assert!(
        base(&e4).starts_with(&format!("export CCM_LAUNCH_ID={}; ", sq("it's here"))),
        "转发的值没经 quote：{}",
        base(&e4)
    );
}

/// 🔴 **`--bus-register` 要了登记而登记不成，必须出声；而「脚本在」不等于「跑得起来」。**
///
/// # 这一条是 `K-R48` 第二拍补的，补的是**三处一起丢掉的东西**
///
/// 把 `tests/e2e/cc-spawn-uplift.sh` 的 `$CCM` 指向二进制之后现打：**48 过 / 24 败**。
/// 24 条里 22 条是同一族，逐条追下去是首版原生实现丢了三样旧 bash 实现有的东西：
///   ① `discover_bus_scripts` 的**第三档 `PATH`** —— docstring 写着、实现里没有。
///      旧 `ccm` 住 `shared/`（部署形态 `~/.claude/skills/ccm`），**兄弟目录**正好是
///      `cc-bus/scripts` ⇒ 第二档几乎总命中；今天后端住 `~/.cc-monitor/bin/`，
///      **旁边永远没有 `cc-bus/`** ⇒ 第二档在真实部署里**恒不命中**。
///   ② 三处判「这个脚本能不能用」写成 `is_file()`，而旧 bash 判的是 `-x`
///      ⇒「在、但没有执行位」被读成「它能用」，拼进 seq 执行时静默失败（整段是 `|| true`）。
///   ③ 两句 stderr 整个没了：找不到 cc-bus ⇒「**没有登记**」；`cc-spawned-record`
///      不可执行 ⇒「**不进 spawn 台账**」。`--bus-register` 于是成了一个
///      「**要了、没做、也不说**」的旗标 —— 那正是本工作区反复消灭的那类静默降级。
///
/// # 它买不到什么
///
/// **那两句 stderr 只钉「生产段里有这一句」，没钉「真跑时它真的印出来了」** ——
/// 后者要捕获进程的 stderr，而 `build()` 是纯函数、`eprintln!` 直接写 fd 2。
/// 行为那一半住 `tests/e2e/cc-spawn-uplift.sh`（**而那套不在出货门禁里**，如实登记）。
#[test]
fn asking_for_bus_registration_and_not_getting_it_is_never_silent() {
    // ① `is_exec`：**行为**判据 —— 造两个真文件，一个有执行位一个没有。
    let d = tempdir();
    let ok = std::path::Path::new(&d).join("cc-register");
    let bad = std::path::Path::new(&d).join("cc-spawned-record");
    std::fs::write(&ok, "#!/bin/sh\n").expect("写夹具");
    std::fs::write(&bad, "#!/bin/sh\n").expect("写夹具");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ok, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert!(is_exec(&ok), "有执行位的判成不可执行");
        assert!(
            !is_exec(&bad),
            "**在、但没有执行位**被判成「它能用」—— 那正是首版 `is_file()` 的病：\n\
                 拼进 seq 之后执行时静默失败（整段是 `|| true`），而调用方以为登记好了"
        );
    }
    assert!(
        !is_exec(&std::path::Path::new(&d).join("根本不存在")),
        "不存在的文件被判成可执行"
    );
    assert!(!is_exec(std::path::Path::new(&d)), "目录被判成可执行文件");
    let _ = std::fs::remove_dir_all(&d);

    // ② 查找次序**三档都在**：`PATH` 那一档是本拍补回来的，别再删。
    let me = crate::guard_support::production_code(include_str!(
        "../../../../src/backend/control/ccm/plan.rs"
    ));
    for anchor in [
        "std::env::var(\"CC_BUS_SCRIPTS\")",
        "join(\"cc-bus\").join(\"scripts\")",
        "std::env::split_paths(&std::env::var_os(\"PATH\")?)",
    ] {
        assert!(
            me.contains(anchor),
            "`discover_bus_scripts` 少了一档：{anchor}\n\
                 三档是 `CC_BUS_SCRIPTS` → 本程序目录旁的 `cc-bus/scripts` → `PATH`。\n\
                 ⚠ 第二档在真实部署里**恒不命中**（后端住 `~/.cc-monitor/bin/`，旁边没有 `cc-bus/`）\n\
                 ⇒ 删掉 `PATH` 那一档 = `--bus-register` 在生产上永远登记不成。"
        );
    }

    // ③ 两句诊断在生产段里（**这一格只钉形状，行为归 e2e**，见头注）。
    // 两句进了文案表：生产段认 key，表里那句认话。
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../../../../src/shared/copy/table.json")).expect("表");
    // 「派生台账」换成人话之后，`noSpawnRecord` 那句不再按原文认；它说没说出来由 e2e
    //   `cc-spawn-uplift.sh` [18] 按文案键整行比。这里只钉它在表里、带 `{path}`。
    assert_eq!(
        table["entries"]["bePlan.bus.noSpawnRecord"]["args"],
        serde_json::json!(["path"]),
        "表里没有 bePlan.bus.noSpawnRecord，或它不再点名是哪个文件"
    );
    for (key, say) in [("bePlan.bus.noScripts", "没有登记")] {
        assert!(
            table["entries"][key]["zh"]
                .as_str()
                .unwrap_or("")
                .contains(say),
            "表里 {key} 那句没说「{say}」"
        );
    }
    for say in ["\"bePlan.bus.noScripts\"", "\"bePlan.bus.noSpawnRecord\""] {
        assert!(
            me.contains(say),
            "`--bus-register` 登记不成时那句「{say}」不见了 —— \n\
                 旗标于是变成「要了、没做、也不说」。整段是 `|| true`，**不会有别的东西替它出声**。"
        );
    }
    // 反向锚点：那两句必须在**同一个函数**里（`build()` 的容器分支），
    // 搬到别处等于「说是说了，但那条路上说不到」。
    let bus_block = me
        .split("let bus = if o.bus_register {")
        .nth(1)
        .expect("找不到 `--bus-register` 那一段 —— 抽取器坏了，本条会零命中地绿");
    let head = &bus_block[..bus_block.len().min(1200)];
    assert!(
        head.contains("\"bePlan.bus.noScripts\"") && head.contains("\"bePlan.bus.noSpawnRecord\""),
        "那两句不在 `--bus-register` 那一段里了 —— 它们要在**决定登记不成的那一刻**说出来"
    );
}

/// 〔搬自 `ccm-print-parity`「含空格 cwd 正确带引号」与 `ccm-cli` 的 quote 那族〕
#[test]
fn every_value_that_reaches_a_shell_is_quoted() {
    let out = render(&plan_of(
        &["--ccm-tmux", "--cwd", "/home/pi/my proj"],
        &env(),
        &AccountTable::default(),
    ));
    assert!(out.contains("-c '/home/pi/my proj'"), "{out}");
    assert!(out.contains("-s 'my-proj-cc'"), "{out}");
    assert_eq!(qarg("a b"), "'a b'");
    assert_eq!(qarg("ok-1.2/x"), "ok-1.2/x");
    assert_eq!(qarg(""), "''");
    assert_eq!(qarg("it's"), "'it'\\''s'");
}

/// 〔搬自 `ccm-print-parity`「attach 到 cc-p1」〕—— `=名:` 是 tmux 的**精确匹配**形。
#[test]
fn attach_uses_the_exact_match_target() {
    assert_eq!(printed(&["--attach", "cc-p1"]), "tmux attach -t '=cc-p1:'"); // V138：`attach <名>` → `--attach <名>`
}

/// 🔴 `KR58D3` —— 不给 `--cwd` 的默认是**恒等**：就是调用方自己的 cwd，一层都不跳。
///
/// 〔用@09-11 逐字〕「`cc` 默认就起会话就行，**跳目录是我自己的设置，不要搞进 app**。」
/// `K37` 第三条：**诚实的默认 = 恒等 / 不作为 / 沿用调用者已有状态**；
/// 从一张表里替他挑一个具体值（工作区 / 仓的父目录）**不诚实**。
///
/// 〔本条是上一版那条「auto 有几条分支」的**翻面**，不是它的替补：那一版搬自
/// `ccm-cli`「布局 1–5」，断的正是今天被裁掉的那两档。同样那几种布局留在这里，
/// 从「证明会跳」变成「证明不跳」—— **射程一格没少**。〕
#[test]
fn the_default_cwd_is_the_identity_in_every_layout() {
    let mut e = env();
    let d = tempdir();
    // 布局1：站在 $HOME —— 从前跳 `$CCM_WORKSPACE`，今天就是 $HOME。
    e.pwd = e.home.clone();
    assert_eq!(cwd_of(&[], &e), e.home, "在 $HOME 裸敲不许再跳工作区");
    // 布局2/3：git 仓根 / 仓的子目录 —— 从前跳**仓的父目录**，今天就是站着的那个目录。
    std::fs::create_dir_all(format!("{d}/repo/sub")).expect("造夹具");
    std::fs::write(format!("{d}/repo/.git"), "gitdir: /elsewhere").expect("造夹具");
    for p in [format!("{d}/repo"), format!("{d}/repo/sub")] {
        e.pwd = p.clone();
        assert_eq!(cwd_of(&[], &e), p, "在 git 仓里敲不许再跳到仓外");
    }
    // 布局4：非 git 目录 —— 一直是它自己（这一档本来就诚实，留着当对照）。
    e.pwd = d.clone();
    assert_eq!(cwd_of(&[], &e), d);
    // resume / attach 那一支**一个字都没动**：它本来就不走 auto，
    // 再解析一次会让 claude 按 `projects/<enc(cwd)>/<sid>.jsonl` 找不到会话（实测踩过）。
    e.pwd = format!("{d}/repo");
    assert_eq!(cwd_of(&["--resume", "s"], &e), format!("{d}/repo"));
    assert_eq!(cwd_of(&["--attach", "n"], &e), format!("{d}/repo"));
    // 显式 `--cwd` 仍然赢 —— 拿掉的是「替用户挑」，不是「用户自己挑」。
    assert_eq!(cwd_of(&["--cwd", "/x/y"], &e), "/x/y");
}

fn cwd_of(args: &[&str], e: &Env) -> String {
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match parse(&a).expect("解析得动") {
        Parsed::Opts(o) => resolve_cwd(&o, e),
        other => panic!("{other:?}"),
    }
}

/// 〔搬自 `ccm-contract-parity` B 组「CCM_ENV 被 eval / --print 里也在 / 早于会话级 env」〕
///
/// `CCM_ENV` 是**机器级** env（代理等，旧 `CC_ENV` 的搬家）：它必须排在会话级 env
/// **之前**，否则 `--account` 想覆盖它时反而被它盖回去。差分对顺序失明 ⇒ 单钉。
#[test]
fn the_machine_level_env_comes_first_and_the_session_level_one_wins() {
    let dz = tempdir();
    let t = table(&[("z", Some(dz.as_str()), true)]);
    let mut e = env();
    e.ccm_env = "export CCM_ENV_PROBE=from-ccm-env".into();
    let line = render(&plan_of(&["--cwd", "/p", "--account", "z"], &e, &t));
    assert!(
        line.starts_with("export CCM_ENV_PROBE=from-ccm-env; "),
        "{line}"
    );
    let i_env = line.find("CCM_ENV_PROBE").expect("该有机器级 env");
    let i_acct = line.find("CLAUDE_CONFIG_DIR").expect("该有账号目录");
    assert!(i_env < i_acct, "机器级 env 必须排在会话级之前：{line}");
}

/// 〔搬自 `ccm-contract-parity` A / A′ 两组「print↔exec 一致」〕
///
/// 从前那两组要**真跑一趟**再与 `--print` 差分，因为两条路是两份代码。
/// 今天它们读的是**同一个 [`Plan`]** ⇒ 这条判据钉的是那个结构事实：
/// 渲染函数的全部输入只有 `Plan`，没有第二个来源（删了进程内 `resolve` 那一问）。
#[test]
fn print_and_exec_cannot_drift_because_they_read_the_same_plan() {
    let p = plan_of(
        &["--cwd", "/p", "--model", "opus"],
        &env(),
        &AccountTable::default(),
    );
    assert_eq!(render(&p), render(&p), "渲染必须是纯函数");
    let Plan::Direct(d) = &p else {
        panic!("该是 Direct")
    };
    // 真跑那一侧读的就是这几个字段（`run::exec_direct`），逐个在这里点名。
    assert_eq!(d.cwd, "/p");
    assert_eq!(d.argv, vec!["claude", "--model", "opus"]);
}

/// ★★ **`inside_tmux` 是真跑那一侧独用的一格，[`render`] 一个字不看。**
///
/// `INVARIANTS §33a` 铁律 2 逐字：`--print` 必须是**纯的** —— 不查实时 tmux 状态、
/// 输出对宿主环境**逐字节稳定**；值不知道就**打印配方**。
/// 🔴 那一条自己就有过反面教材（`§33a` 记着）：第一版让 `--print` 直接查 tmux 把值烘进去，
/// 于是同一条命令在开发者的 tmux 里与 CI 上吐得不一样，而当时的「修法」是给判据加
/// `env -u TMUX` —— **那是为实现让路去改判据**。
/// ⇒ 本条就是那条反面教材的防线：谁把 `inside_tmux` 接进 [`render`]，同一条计划立刻
/// 在 tmux 内外吐出两种输出，本条当场红。
#[test]
fn whether_we_are_inside_tmux_never_reaches_the_print_side() {
    let outside = Env {
        tmux: None,
        ..env()
    };
    let inside = Env {
        tmux: Some("/faux/socket,1,0".into()),
        ..env()
    };
    let args = &["--ccm-agent", "codex", "--cwd", "/p"];
    let po = plan_of(args, &outside, &AccountTable::default());
    let pi = plan_of(args, &inside, &AccountTable::default());

    // 反空真①：两份计划**真的不同** —— 否则下面那条相等是恒真。
    assert_ne!(
        po, pi,
        "`$TMUX` 变了而计划一个字节没变 —— `inside_tmux` 没接上，本条在空转"
    );
    let (Plan::Direct(dout), Plan::Direct(din)) = (&po, &pi) else {
        panic!("该是两条直路")
    };
    assert!(!dout.inside_tmux && din.inside_tmux, "夹具没把那一格拨动");
    // 反空真②：这一趟真带着那段配方 —— 否则「渲染不变」是因为压根没有可变的东西。
    assert!(
        dout.bus_id_recipe && din.bus_id_recipe,
        "codex 该带 cc-bus 配方"
    );

    // 正题：渲染**逐字节相等**。
    assert_eq!(
        render(&po),
        render(&pi),
        "`--print` 随 `$TMUX` 变了 —— 平价预言机不再是纯的（`§33a` 铁律 2）"
    );
    // 而且两边都**说出了**那段配方（`§33a` 铁律 1：exec 路会设的 env，print 必须说）。
    assert!(
        render(&po).contains(super::super::BUS_ID_RECIPE.as_str()),
        "`--print` 没说出 cc-bus 那段配方 —— 那正是 `§33a` 开张时抓到的第一例"
    );
}

// ── 夹具 ────────────────────────────────────────────────────────────
fn tempdir() -> String {
    let p = std::env::temp_dir().join(format!(
        "ccm-plan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("时钟")
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).expect("造夹具目录");
    p.to_string_lossy().to_string()
}

fn table(rows: &[(&str, Option<&str>, bool)]) -> AccountTable {
    AccountTable::from_accounts(
        rows.iter()
            .map(|(n, c, d)| Account {
                name: (*n).to_string(),
                config_dir: c.map(|x| x.to_string()),
                is_default: *d,
            })
            .collect(),
    )
}

/// 账号表的**读法只有一处**：那份 manifest。〔承接 `ccm-cli` KCY1 那一族的语义半〕
#[test]
fn the_account_table_has_exactly_one_source() {
    let d = tempdir();
    let m = format!("{d}/accounts.json");
    std::fs::write(
        &m,
        format!(r#"{{"accounts":[{{"name":"z","configDir":"{d}","isDefault":true}}]}}"#),
    )
    .expect("造夹具");
    let t = AccountTable::load(&m);
    assert_eq!(t.accounts.len(), 1);
    assert_eq!(t.default_name(), Some("z"));
    assert_eq!(t.config_dir_of("z"), Some(d.clone()));
    // manifest 不在 ⇒ **空表**，不是失败（那台机器就是没有账号库）
    assert!(AccountTable::load("/nonexistent/accounts.json")
        .accounts
        .is_empty());
    // manifest 里写着、盘上没有 ⇒ 当作不可用（目录存在性自己判）
    let m2 = format!("{d}/a2.json");
    std::fs::write(
        &m2,
        r#"{"accounts":[{"name":"g","configDir":"/nonexistent/gone"}]}"#,
    )
    .expect("造夹具");
    assert_eq!(AccountTable::load(&m2).config_dir_of("g"), None);
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔2026-09-21 真机现打〕那份 manifest 带 UTF-8 BOM ⇒ **整张账号库被静默吃掉**
// ════════════════════════════════════════════════════════════════════════
//
// # 怎么量到的
//
// 在本机那台 Win11 虚拟机上跑 `ccm.exe --account …`：报「账号不可用（不在
// …accounts.json）」，而那份文件里明明写着那个号。根因是它带 **UTF-8 BOM**
//（`EF BB BF` —— **PowerShell 5.1 `-Encoding UTF8` 的默认值**），`serde_json`
// 在第 1 列就失手（独立复算逐字：`expected value at line 1 column 1`），
// 而 `load` 那句 `unwrap_or_default()` 把失手**吞成空表**。
//
// ⇒ **用户拿 PowerShell 碰过这份文件，他的号全消失，还被告知本来就没有。**
//
// ⚠ 这一形本机（Linux）在构造上量不到：没人会用 PowerShell 写这份文件。
// 判据这一侧**不需要真 Windows** —— BOM 是三个字节，造得出来。
// ⇒ 平台专有的**触发条件**与平台专有的**行为**是两件事，别把前者当成「判不了」。

/// 🔴 带 BOM 的 manifest 照样读得出来（BOM 不是错误，是字节序标记）。
#[test]
fn a_manifest_with_a_utf8_bom_is_read_not_silently_eaten() {
    let d = tempdir();
    let m = format!("{d}/bom.json");
    let body = format!(r#"{{"accounts":[{{"name":"work","configDir":"{d}","isDefault":true}}]}}"#);
    // PowerShell 5.1 `-Encoding UTF8` 就是这么写的：正文前面三个字节 EF BB BF。
    std::fs::write(&m, format!("\u{FEFF}{body}")).expect("造夹具");
    // 夹具本身先自证：那三个字节真的在盘上（否则这条判据在量一个没有 BOM 的文件）。
    let raw = std::fs::read(&m).expect("读夹具");
    assert_eq!(
        &raw[..3],
        &[0xEF, 0xBB, 0xBF],
        "夹具没写出 BOM —— 这条判据此刻在量别的东西"
    );

    let t = AccountTable::load(&m);
    assert_eq!(
        t.accounts.len(),
        1,
        "带 BOM 的账号库被吃掉了 —— 实得 {} 个号",
        t.accounts.len()
    );
    assert_eq!(t.default_name(), Some("work"));
    assert_eq!(t.config_dir_of("work"), Some(d.clone()));
    // 而且**不许**被报成「解析不动」：它解析得动，只是带了 BOM。
    assert!(
        !t.names().contains("读不懂"),
        "一份合法的（只是带 BOM 的）账号库被报成了坏文件：{}",
        t.names()
    );
}

/// 🔴 真解析不动的那一份 ⇒ 照旧空表，**但那句话不许谎称「没有账号库」**。
///
/// # 少了它会怎样
///
/// 上一条只买「BOM 这一种」。而 `unwrap_or_default()` 那个形状的毛病不是 BOM ——
/// 是**把所有解析失败都说成「这台机器没有账号库」**。手写坏一个逗号是同一形，
/// 而那时用户会去找一个不存在的原因。
#[test]
fn a_manifest_we_cannot_parse_says_so_instead_of_claiming_there_is_no_library() {
    let d = tempdir();
    let m = format!("{d}/broken.json");
    std::fs::write(&m, r#"{"accounts":[{"name":"work",]}"#).expect("造夹具");
    let t = AccountTable::load(&m);
    assert!(t.accounts.is_empty(), "坏 JSON 竟然读出了号");
    let said = t.names();
    assert!(
        said.contains("读不懂"),
        "坏掉的账号库被报成了别的东西：{said}"
    );
    assert!(
        !said.contains("无账号库"),
        "坏掉的账号库被谎称成「没有账号库」—— 用户会去找一个不存在的原因：{said}"
    );

    // 🔴 阴性对照：**真的**没有账号库时，那句话仍然是「无账号库」
    //（少了这一半，上面那两比可以靠「恒说解析不动」全绿，
    //  那时没装过账号库的用户会收到一句「你的文件坏了」）。
    let none = AccountTable::load("/nonexistent/accounts.json");
    assert!(none.names().contains("无账号库"), "实得 {}", none.names());
    assert!(!none.names().contains("读不懂"), "实得 {}", none.names());

    // 空文件 == 没有账号库（**不是**坏文件）。
    let e = format!("{d}/empty.json");
    std::fs::write(&e, "   \n").expect("造夹具");
    assert!(
        AccountTable::load(&e).names().contains("无账号库"),
        "空文件被报成了坏文件"
    );
}

/// 用户看得见的那句话里**真的**带上了这个说法 —— 不是只有 `names()` 自己知道。
///
/// ⚠ 本条走 `resolve_account` 那条真路（`--account` 指名一个不存在的号），
/// 因为 `names()` 今天唯一的消费点就在那句报错里；只判 `names()` 的话，
/// 哪天那句报错不再喊它，这件事会静默失效。
#[test]
fn the_message_the_user_actually_sees_carries_the_reason() {
    let d = tempdir();
    let m = format!("{d}/broken2.json");
    std::fs::write(&m, "{oops").expect("造夹具");
    let mut e = env();
    e.accts_manifest = m;
    // 走**真 argv 解析**拿 `Opts`（本仓判据的惯例，见上方 `--account nope` 那条）——
    // 手搓一个 `Opts` 会绕开解析那一段，而那一段也在用户那条路上。
    let a: Vec<String> = ["--cwd", "/p", "--account", "work"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let o = match parse(&a).expect("解析得动") {
        Parsed::Opts(o) => o,
        other => panic!("{other:?}"),
    };
    let Die(msg) = resolve_account(&o, &e, &AccountTable::load(&e.accts_manifest))
        .expect_err("指名一个读不出来的号竟然成功了");
    assert!(msg.contains("读不懂"), "用户看到的那句话里没有原因：{msg}");
}

/// ★ 自检那一趟与 pane 里那一趟**是同一条命令**：同一段 `export` 前缀、同一个入口、
/// 同一串参数，只在 `--` 之前多一个 `--print`（放到 `--` 后面就成了透传给 agent 的参数，
/// 那一趟会**真起一个会话**而不是自检 —— 本件现打时就这么踩过一次）。
///
/// 另钉**顺序**：收尾里自检排在兜底轮询 / 接进去 / 登记**之前**（登记在它后面，才谈得上「不登记」）。
#[test]
fn the_self_check_is_the_payload_itself_plus_print_and_it_runs_before_registering() {
    let mut e = env();
    e.self_argv = vec!["/opt/cc-monitor-backend".into()];
    e.anthropic_base_url = Some("https://relay.example/v1".into());
    e.bus_scripts = Some("/opt/bus".into());
    for (args, has_passthru) in [
        (
            &["--ccm-tmux=n1", "--cwd", "/p", "--", "task", "--x"][..],
            true,
        ),
        (&["--ccm-tmux=n1", "--cwd", "/p"][..], false),
    ] {
        // 旗标排在最前（排到 `--` 后面就成了透传参数）。
        let mut a = vec!["--bus-register", "--detach"];
        a.extend_from_slice(args);
        let p = plan_of(&a, &e, &AccountTable::default());
        let Plan::Container(c) = &p else {
            panic!("该是容器路：{p:?}")
        };
        // ccm 那一半在 `--` 右边、恒在末尾 ⇒ 自检那一趟就是载荷末尾多一个 `--ccm-print`（有没有透传都一样）。
        let _ = has_passthru;
        let want = format!("{} '--ccm-print'", c.payload);
        assert_eq!(c.self_check, want, "自检与载荷不是同一条命令");
        assert!(
            c.self_check.starts_with(
                "export ANTHROPIC_BASE_URL='https://relay.example/v1'; '/opt/cc-monitor-backend' "
            ),
            "自检没带同一段 export 前缀 / 同一个入口：{}",
            c.self_check
        );
        let tail = render_container_tail(c);
        let at = |needle: &str| {
            tail.find(needle)
                .unwrap_or_else(|| panic!("收尾里没有 {needle}：{tail}"))
        };
        assert!(
            at(&c.self_check) < at("cc-register"),
            "自检排在登记后面了 ⇒ 起不来的也照登记：{tail}"
        );
        assert!(
            tail.contains("exit 4"),
            "自检不过没有 exit 4（起不来）：{tail}"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════
// 直路上的 `--ccm-sid` —— 交给启动期令牌那条路
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 那一行，逐字「主会话判：**不报错**（报错 ＝ 让它依赖 tmux，撞
// 「`--ccm-sid` 不要依赖 tmux」），直路语义走已落地的启动期令牌那条路」· `WN1.md §3`。
// 原病（`lib.rs::TARGET_GAPS` 那一行逐字）：「被接受、零效果、而且不出声」——
// `Plan::Direct` 里根本没有这一格。

/// 32 个小写十六进制 —— 令牌的合格形状（`identity_tag::token_is_safe`）。字面量，不从实现推。
const GOOD_LAUNCH_TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn direct_with(args: &[&str], launch_token: Option<&str>) -> Direct {
    let e = Env {
        launch_token: launch_token.map(str::to_string),
        ..env()
    };
    match plan_of(args, &e, &AccountTable::default()) {
        Plan::Direct(d) => d,
        other => panic!("直路测试却算出 {other:?}"),
    }
}

/// 四格逐格相等：没给 / 给了＋合格令牌 / 给了＋形状不对的令牌 / 给了＋没令牌。
#[test]
fn on_the_direct_path_ccm_sid_is_carried_by_the_launch_token() {
    let cells = [
        (&[][..], Some(GOOD_LAUNCH_TOKEN), DirectIdentity::NotAsked),
        (
            &["--ccm-sid", "s-1"][..],
            Some(GOOD_LAUNCH_TOKEN),
            DirectIdentity::ByLaunchToken,
        ),
        // 大写 —— 差一点对的串一律当没有（`token_is_safe` 头注：fail closed）。
        (
            &["--ccm-sid", "s-1"][..],
            Some("0123456789ABCDEF0123456789ABCDEF"),
            DirectIdentity::NoCarrier,
        ),
        (&["--ccm-sid", "s-1"][..], None, DirectIdentity::NoCarrier),
    ];
    for (args, tok, want) in cells {
        assert_eq!(
            direct_with(args, tok).identity,
            want,
            "直路 {args:?}（令牌 {tok:?}）交错了人"
        );
    }
}

/// `--print` 不看宿主环境（`INVARIANTS §33a` 铁律 2）：带不带 `--ccm-sid`、有没有令牌，
/// 直路吐的那一行**逐字节相等** —— 令牌是继承的环境，不是命令文本。
#[test]
fn the_direct_print_does_not_change_with_ccm_sid_or_the_token() {
    let bare = render(&Plan::Direct(direct_with(&[], None)));
    for (args, tok) in [
        (&["--ccm-sid", "s-1"][..], None),
        (&["--ccm-sid", "s-1"][..], Some(GOOD_LAUNCH_TOKEN)),
        (&[][..], Some(GOOD_LAUNCH_TOKEN)),
    ] {
        assert_eq!(
            render(&Plan::Direct(direct_with(args, tok))),
            bare,
            "直路 `--print` 随 {args:?} / 令牌 {tok:?} 变了"
        );
    }
}

// ── Windows 本机那一格：`is_exec` · 家目录 · 路径分隔符 ──────────────
//
// 要求：「**本机 Windows**（x86_64） | ✅ **承诺**」；
// 「「怎么读到这个事实」   → platform      （各平台读法不同）」—— 「这个文件跑得起来吗」在 Windows 上的读法是 `PATHEXT`；
// 读数要求：「`plan.rs::is_exec` 的 `#[cfg(not(unix))]` 分支是 `p.is_file()` ⇒ 在 Windows 上
// **「可执行」退化成「存在」**」与「那句错误话术里的路径是 `C:\…\fakehome/.claude-accts/accounts.json` ——
// **反斜杠与正斜杠混着**」（WN1 件 G）。
// 异源：期望是手写的判定表（Windows `cmd.exe` 的 `PATHEXT` 语义），不从被测函数里抠。
// ⚠ 买不到：`#[cfg(not(unix))]` 那一臂在本机不编译 —— 它「真的调了判定函数」由下面源码锚点钉，
//    「在 Windows 上真这么判」要真机（虚拟机读数）。

#[test]
fn on_windows_runnable_means_a_file_whose_extension_is_in_pathext() {
    use std::path::Path;
    let cells: &[(bool, &str, Option<&str>, bool, &str)] = &[
        (
            true,
            "C:/x/cc-register",
            None,
            false,
            "没有扩展名的文件在 Windows 上跑不起来",
        ),
        (
            true,
            "C:/x/cc-register",
            Some(".COM;.EXE;.BAT;.CMD;.PS1"),
            false,
            "PATHEXT 再长，无扩展名照样不行",
        ),
        (
            true,
            "C:/x/cc-register.cmd",
            None,
            true,
            "PATHEXT 缺席 ⇒ 用 cmd.exe 的默认那一份",
        ),
        (
            true,
            "C:/x/cc-register.CMD",
            Some(".com;.exe;.bat;.cmd"),
            true,
            "大小写不敏感",
        ),
        (
            true,
            "C:/x/cc-register.ps1",
            None,
            false,
            "默认 PATHEXT 里没有 .PS1",
        ),
        (
            true,
            "C:/x/cc-register.ps1",
            Some(".COM;.EXE;.PS1"),
            true,
            "用户加了 .PS1 就认",
        ),
        (true, "C:/x/tool.exe", Some(""), true, "空 PATHEXT 当缺席"),
        (
            true,
            "C:/x/notes.txt",
            Some(".COM;.EXE;.BAT;.CMD"),
            false,
            "扩展名不在表里",
        ),
        (
            false,
            "C:/x/tool.exe",
            None,
            false,
            "不是文件（目录 / 不存在）就不是可执行",
        ),
    ];
    for (is_file, p, pathext, want, why) in cells {
        assert_eq!(
            runnable_on_windows(*is_file, Path::new(p), *pathext),
            *want,
            "{p}（is_file={is_file}, PATHEXT={pathext:?}）：{why}"
        );
    }
}

/// 非 unix 那一臂**真的调了**上面那个判定、而且没退回 `is_file()` 一字了事。
/// 两向：那一臂里判定函数恰好 1 处；`is_exec` 函数体里裸 `p.is_file()` 零处（正控：unix 臂的 `m.is_file()` 在）。
#[test]
fn the_non_unix_arm_of_is_exec_delegates_to_the_pathext_rule() {
    let me = crate::guard_support::production_code(include_str!(
        "../../../../src/backend/control/ccm/plan.rs"
    ));
    let body = me
        .split("fn is_exec(p: &std::path::Path) -> bool {")
        .nth(1)
        .and_then(|b| b.split("\n}\n").next())
        .expect("找不到 `is_exec` 的函数体 —— 抽取器坏了，下面几条会零命中地绿");
    assert!(
        body.contains("m.is_file() && m.permissions().mode() & 0o111 != 0"),
        "正控：unix 臂那一句不在了 —— 抽取器切错了地方"
    );
    let arm = body
        .split("#[cfg(not(unix))]")
        .nth(1)
        .expect("`is_exec` 没有非 unix 那一臂了");
    assert_eq!(
        arm.matches("runnable_on_windows(").count(),
        1,
        "非 unix 那一臂要恰好调一次 `runnable_on_windows`：\n{arm}"
    );
    assert_eq!(
        arm.matches("p.is_file()").count(),
        1,
        "非 unix 那一臂里 `p.is_file()` 只许作为判定函数的第一个参数出现一次（退回裸 `is_file()` ⇒ 「可执行」又退化成「存在」）"
    );
    assert!(
        arm.contains("runnable_on_windows(p.is_file(),"),
        "`p.is_file()` 要喂进判定函数，不许单独当结论：\n{arm}"
    );
}

#[test]
fn the_home_is_home_then_userprofile_and_paths_under_it_are_joined_per_segment() {
    let env_of = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    assert_eq!(
        home_of(env_of(&[
            ("HOME", "/home/pi"),
            ("USERPROFILE", "C:\\Users\\zbl")
        ])),
        "/home/pi"
    );
    assert_eq!(
        home_of(env_of(&[("USERPROFILE", "C:\\Users\\zbl")])),
        "C:\\Users\\zbl",
        "Windows 默认没有 HOME"
    );
    assert_eq!(
        home_of(env_of(&[("HOME", ""), ("USERPROFILE", "C:\\Users\\zbl")])),
        "C:\\Users\\zbl",
        "空 HOME 当缺席"
    );
    assert_eq!(home_of(env_of(&[])), "", "两个都没有 ⇒ 空串（照旧）");

    // 本机（Linux）上逐段 join 与从前的 `format!("{home}/{rel}")` 逐字相等 —— 这一格不许变。
    for rel in [".claude-accts/accounts.json", ".config/ccm/config"] {
        assert_eq!(under_home("/home/pi", rel), format!("/home/pi/{rel}"));
    }
    assert_eq!(
        under_home("", ".claude-accts/accounts.json"),
        "/.claude-accts/accounts.json",
        "空家目录照旧"
    );

    // 生产那一处真的走这两个函数，不再手拼 `{home}/…`。
    let me = crate::guard_support::production_code(include_str!(
        "../../../../src/backend/control/ccm/plan.rs"
    ));
    let from_process = me
        .split("pub(crate) fn from_process(process_argv: &[String]) -> Self {")
        .nth(1)
        .and_then(|b| b.split("\n    }\n}\n").next())
        .expect("找不到 `Env::from_process` —— 抽取器坏了");
    assert_eq!(
        from_process.matches("home_of(").count(),
        1,
        "家目录要经 `home_of` 取"
    );
    assert_eq!(
        from_process.matches("under_home(&home,").count(),
        2,
        "配置文件与账号库两处都要经 `under_home` 拼"
    );
    assert_eq!(
        from_process.matches("format!(\"{home}/").count(),
        0,
        "又手拼 `{{home}}/…` 了 —— Windows 上就是 `\\` 与 `/` 混拼"
    );
    assert_eq!(
        from_process.matches("std::env::var(\"HOME\")").count(),
        0,
        "又只认 `HOME` 了 —— Windows 上默认没有它"
    );
}

/// 〔RK1 报 2〕E10：继承来的中转地址（钥匙已展开）不许原样进载荷 —— 渲回 `$(cat ~/…)` 形，
/// 真 `sh` 在夹具家目录下展开后 == 原地址；认不出的（用户自己的端点 · 形状不对的）原样。
/// 守的要求：`INVARIANTS §48.1a`（中转钥匙不进 argv）。
#[test]
fn us1_an_inherited_keyed_relay_url_goes_inward_as_a_file_read_not_as_the_key() {
    let key = "0123456789abcdef".repeat(4);
    let home = tempdir();
    let key_file = std::path::Path::new(&home).join(relay_route_core::KEY_FILE_REL);
    std::fs::create_dir_all(key_file.parent().unwrap()).unwrap();
    std::fs::write(&key_file, &key).unwrap();
    let keyed = format!("http://127.0.0.1:8788/{key}/t/claude-code/_");
    let payload_of = |v: &str| -> String {
        let mut e = env();
        e.anthropic_base_url = Some(v.to_string());
        let Plan::Container(c) = plan_of(
            &["--ccm-tmux=n1", "--cwd", "/p"],
            &e,
            &AccountTable::default(),
        ) else {
            panic!("该是容器路")
        };
        c.payload
    };
    let p = payload_of(&keyed);
    assert!(
        !p.contains(&key),
        "钥匙原样进了载荷（会进 tmux send-keys 的 argv）：{p}"
    );
    assert!(
        p.contains("'$(cat ~/.cc-monitor/relay-key)'"),
        "没渲成现读钥匙文件那一形：{p}"
    );
    // 真 shell：取载荷里那一句 export，在夹具家目录下跑、印出来 == 原地址。
    let export = &p[..p.find("; ").unwrap() + 2];
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{export}printf %s \"$ANTHROPIC_BASE_URL\""))
        .env("HOME", &home)
        .output()
        .expect("起 sh");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        keyed,
        "展开之后不是原地址"
    );
    // 认不出 ⇒ 原样（期望 == 输入经 sq 的那一形）。
    for v in [
        "https://relay.example/v1".to_string(),
        format!("http://127.0.0.1:8788/{}/t/claude-code/_", &key[..63]),
        format!(
            "http://127.0.0.1:8788/{}/t/claude-code/_",
            key.to_uppercase()
        ),
        format!("http://10.0.0.1:8788/{key}/t/claude-code/_"),
    ] {
        assert!(
            payload_of(&v).starts_with(&format!("export ANTHROPIC_BASE_URL={}; ", sq(&v))),
            "认不出的地址没有原样转：{v}"
        );
    }
}

/// 〔`INVARIANTS §47` ②〕自由文本那几格拼进 shell 之前的放行判定 —— **正反各一格**（§47「拒过头也算违反」）。
///
/// 要求住址：`INVARIANTS §47` ②「走唯一的 quote ＋ 这一种值的形式判定 ＋ 拒绝集」；主会话 09-26 按 V131 裁
/// 「自由文本路径的拒绝集只收控制字符（NUL / CR / LF）、形式判定按各自语境（cwd / 目录要绝对路径等）、然后唯一一处 quote ——
/// 不拒 shell 元字符」。模型名与 `--ccm-sid` 不在本条（交 DUP1）。
/// 交给 agent 的参数与登记备注可以跨行（`shell_quote_core::arg_text_ok`，「位置参数原样交给 claude」）：
/// 多行初始任务那一格正着放、原样进载荷；CR / NUL 照拒。
#[test]
fn free_text_values_pass_real_names_and_refuse_what_the_quote_cannot_hold() {
    let build_of = |args: &[&str], e: &Env| {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => build(&o, e, &AccountTable::default(), None),
            other => panic!("{other:?}"),
        }
    };
    // 正：带 `'` `(` 空格 `&` 的真实目录名 · 透传参数里的元字符 · 备注里的中文与标点。
    for args in [
        vec!["--cwd", "/home/u/Bob's notes (2019)"],
        vec!["--cwd", "/srv/a&b", "--", "--print", "a;b|c"],
        vec![
            "--ccm-tmux=n",
            "--detach",
            "--bus-register",
            "--bus-note",
            "给 aya 的备注（测试）",
        ],
    ] {
        build_of(&args, &env()).unwrap_or_else(|e| panic!("真实好值被拒了：{args:?} ⇒ {}", e.0));
    }
    // 正：多行初始任务（`cc-spawn <目录> "第一行<换行>第二行"` 交过来的那一形）—— 透传参数与备注都放，且原样进 pane 里键入的载荷。
    let multi = "第一行\n第二行";
    let mut with_bus = env();
    with_bus.bus_scripts = Some("/opt/cc-bus/scripts".into());
    let Plan::Container(c) = build_of(
        &[
            "--ccm-tmux=n",
            "--detach",
            "--bus-register",
            "--bus-note",
            multi,
            "--",
            multi,
        ],
        &with_bus,
    )
    .unwrap_or_else(|e| panic!("多行初始任务被拒了：{}", e.0)) else {
        panic!("该是容器路")
    };
    assert!(
        c.payload.contains(&sq(multi)),
        "多行任务没原样进载荷：{}",
        c.payload
    );
    assert_eq!(c.bus.as_ref().map(|b| b.note.as_str()), Some(multi));
    // 用户 09-26：相对 `--cwd` 按当前目录补成绝对、折掉 `.` / `..`（从前这里是「反」那一格）。
    let e0 = env();
    for (rel, want) in [
        ("rel/dir", format!("{}/rel/dir", e0.pwd)),
        (".", e0.pwd.clone()),
        ("../x/./y", "/x/y".to_string()),
    ] {
        let a: Vec<String> = ["--cwd", rel].iter().map(|s| s.to_string()).collect();
        let Parsed::Opts(o) = parse(&a).expect("解析得动") else {
            panic!()
        };
        assert_eq!(resolve_cwd(&o, &e0), want, "{rel:?}");
        build_of(&["--cwd", rel], &e0)
            .unwrap_or_else(|e| panic!("相对 {rel:?} 补全后被拒：{}", e.0));
    }
    // 用户 09-26：启动器按空格拆词、打头 `~/` 是家目录（与载荷那条路交给 shell 拆词同一个结果）。
    for (l, want) in [
        ("ccr code", vec!["ccr", "code", "-p"]),
        ("~/bin/claude --x", vec!["/home/pi/bin/claude", "--x", "-p"]),
    ] {
        let Plan::Direct(d) = build_of(&["--launcher", l, "-p"], &e0).expect("该起得来") else {
            panic!("该是直路")
        };
        assert_eq!(d.argv, want, "{l:?}");
    }
    // 反：`..` 段 · 换行 / CR / NUL，分别落在工作目录 · 启动器 · 透传参数 · 备注（后两格可以跨行，只拒 CR / NUL）。
    for (args, what) in [
        (vec!["--cwd", "/home/u/../etc"], "--cwd"),
        (vec!["--cwd", "/home/u/x\ny"], "--cwd"),
        (vec!["--launcher", "claude\rrm"], "--launcher"),
        (vec!["--", "ok", "bad\0"], "--"),
        (vec!["--", "ok", "a\r\nb"], "--"),
        (
            vec![
                "--ccm-tmux=n",
                "--detach",
                "--bus-register",
                "--bus-note",
                "a\rb",
            ],
            "--bus-note",
        ),
    ] {
        let e = build_of(&args, &env())
            .err()
            .unwrap_or_else(|| panic!("坏值拼进去了：{args:?}"));
        assert!(
            e.0.contains(what),
            "拒了，但没说清是哪一格（{what}）：{}",
            e.0
        );
    }
    // 继承来的那三个：只有容器路会把它们显式化进载荷 ⇒ 只在那条路上判；直路上一个用不上的怪值不挡（拒过头）。
    let mut dirty = env();
    dirty.anthropic_base_url = Some("http://x\nevil".into());
    build_of(&[], &dirty).unwrap_or_else(|e| panic!("直路上用不上的继承值挡住了起会话：{}", e.0));
    let e = build_of(&["--ccm-tmux=n"], &dirty)
        .err()
        .expect("容器路把带换行的继承值拼进载荷了");
    assert!(e.0.contains("ANTHROPIC_BASE_URL"), "{}", e.0);
}

/// 〔`INVARIANTS §47` ②〕manifest 里来的账号配置目录是本仓自管的路径 ⇒ 拼进容器路 / 直路那条 shell 串之前走**全表**
/// （`acct_core::config_dir_ok`：形式 ＋ 控制符 · 元字符 · 欺骗字符），不是自由文本那一层 —— **正反各一格**。
/// 要求住址：`INVARIANTS §47` ②「本仓自管的值（配置目录 · 后端落点）不走这一条，走全表」；TL3 交接：
/// 「账号配置目录全表住 `observe/accounts_query`，而 `control → observe` 是禁止方向」—— 全表搬进共享 crate，这里直接用。
#[test]
fn a_config_dir_from_the_manifest_goes_through_the_full_table() {
    let d = tempdir();
    let good = format!("{d}/.claude-accts/z");
    let bad = format!("{d}/a$b");
    for p in [&good, &bad] {
        std::fs::create_dir_all(p).expect("造夹具");
    }
    let t = table(&[
        ("z", Some(good.as_str()), true),
        ("x", Some(bad.as_str()), false),
    ]);
    let build_of = |args: &[&str]| {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => build(&o, &env(), &t, None),
            other => panic!("{other:?}"),
        }
    };
    let ok = build_of(&["--cwd", "/p", "--account", "z"])
        .unwrap_or_else(|e| panic!("好目录被拒了：{}", e.0));
    assert!(render(&ok).contains("CLAUDE_CONFIG_DIR"));
    let e = build_of(&["--cwd", "/p", "--account", "x"])
        .err()
        .expect("带 `$` 的配置目录拼进去了（自由文本那一层不拒元字符，全表拒）");
    assert!(e.0.contains("a$b"), "拒了，但没说清是哪个目录：{}", e.0);
}

/// 〔`INVARIANTS §47` ③〕`--launcher` 是**命令片段**，过全仓那一张白名单
/// （`shell_quote_core::launcher_refused_char`，与 monitor 本机 · 远端载荷同一条）—— 先前这一格只拒 NUL / CR / LF。
/// 正反各一格：真实启动器（带参数 · 路径 · 家目录下）建得出计划；白名单外的字符拒，那一句点出是哪一格、哪一个字符。
#[test]
fn a_launcher_is_one_command_fragment_from_the_shared_whitelist() {
    let build_of = |launcher: &str| {
        let a: Vec<String> = ["--cwd", "/p", "--launcher", launcher]
            .iter()
            .map(|s| s.to_string())
            .collect();
        match parse(&a).expect("该解析得动") {
            Parsed::Opts(o) => build(&o, &env(), &AccountTable::default(), None),
            other => panic!("{other:?}"),
        }
    };
    for good in [
        "claude",
        "ccr code",
        "/usr/local/bin/claude",
        "~/bin/claude --x",
    ] {
        build_of(good).unwrap_or_else(|e| panic!("真实启动器被拒了：{good:?} ⇒ {}", e.0));
    }
    for (bad, c) in [
        ("claude;rm", ';'),
        ("cla'ude", '\''),
        ("$(x)", '$'),
        ("a~b", '~'),
        ("clé", 'é'),
    ] {
        let e = build_of(bad)
            .err()
            .unwrap_or_else(|| panic!("坏启动器拼进去了：{bad:?}"));
        assert!(e.0.contains("--launcher"), "没说清是哪一格：{}", e.0);
        assert!(
            e.0.contains(&format!("{c:?}")),
            "{bad:?}：没说出是哪个字符：{}",
            e.0
        );
    }
}

// ───────── 〔「ccm 只看不吃 `--resume` 以复用 tmux 名」〕resume 先查是否已在跑 ─────────
// 守的要求：「ccm 只「看」不「吃」`--resume` / `--continue` 以复用 tmux 名」；主会话裁：在跑 ⇒ 接上它，不另起。

fn snapshot_rows(rows: &[(&str, &str)]) -> TakenNames {
    let rows: Vec<crate::common::session_snapshot::SessionRow> = rows
        .iter()
        .map(|(n, s)| crate::common::session_snapshot::SessionRow {
            name: (*n).to_string(),
            ccm_sid: (*s).to_string(),
        })
        .collect();
    crate::common::session_snapshot::SessionSnapshot::with_prober(move || Ok(rows.clone()))
        .taken_names()
        .expect("固定夹具问得到")
}

/// ★ 快照里有 `@ccm_sid` 对上的 tmux 会话 ⇒ 三种写法的 resume（直路 / 容器路 / `--detach`）都接上它；对不上 / 没给值 ⇒ 照旧起。
#[test]
fn fix_a_resume_of_a_session_already_running_in_tmux_rejoins_it() {
    let snap = snapshot_rows(&[("other", ""), ("work", "sid-1"), ("third", "sid-9")]);
    let t = AccountTable::default();
    let rejoin = |detach| Plan::Rejoin {
        name: "work".into(),
        sid: "sid-1".into(),
        detach,
    };
    for (args, want) in [
        (&["--resume", "sid-1"][..], Some(rejoin(false))),
        (&["-r", "sid-1", "--model", "x"][..], Some(rejoin(false))),
        (&["--resume=sid-1"][..], Some(rejoin(false))),
        // 这一族的参数按 V138 形写、由 `parse` 那层换成 V151 形（`v138_to_v151`）。
        (
            &["--ccm-tmux", "--resume", "sid-1"][..],
            Some(rejoin(false)),
        ),
        (
            &["--ccm-tmux", "--detach", "--resume", "sid-1"][..],
            Some(rejoin(true)),
        ),
        (&["--resume", "sid-2"][..], None),
        (&["--resume"][..], None),
        (&["--continue"][..], None),
        (&["-p", "sid-1"][..], None),
    ] {
        let got = plan_of_with(args, &env(), &t, Some(&snap));
        match want {
            Some(w) => assert_eq!(got, w, "{args:?}"),
            None => assert!(
                !matches!(got, Plan::Rejoin { .. }),
                "{args:?} 不该接：{got:?}"
            ),
        }
    }
    // 问不到快照 ⇒ 判不了「在跑」⇒ 照旧起（与避让同一个诚实降级）。
    assert!(!matches!(
        plan_of_with(&["--resume", "sid-1"], &env(), &t, None),
        Plan::Rejoin { .. }
    ));
    // `--ccm-print` 吐的就是真跑那一行：在不在 tmux 里是配方，不是宿主状态。
    assert_eq!(
        render(&rejoin(false)),
        "if [ -n \"${TMUX:-}\" ]; then tmux switch-client -t '=work:'; else tmux attach -t '=work:'; fi"
    );
    assert_eq!(render(&rejoin(true)), "echo 'ccm-session=work'");
}

// 在跑但不在 ccm 认得的 tmux 会话里 ⇒ 不另起，明说。
thread_local! {
    static SCANNED: std::cell::RefCell<Vec<Option<String>>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn fake_scan(dir: Option<&std::path::Path>) -> Vec<(String, u32)> {
    SCANNED.with(|s| s.borrow_mut().push(dir.map(|d| d.display().to_string())));
    vec![("sid-1".into(), 4242)]
}

/// ★ 注入的扫描说 `sid-1` 在跑（pid 4242）、快照里没它 ⇒ 拒并说出 pid；tmux 里认得出 ⇒ 仍是接上；扫描问的是这一趟那个账号的目录。
#[test]
fn fix_a_resume_of_a_session_running_outside_tmux_is_refused_and_says_where() {
    let t = AccountTable::default();
    let mut e = env();
    e.running_sessions = Some(fake_scan);
    let a: Vec<String> = ["--resume", "sid-1"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let Parsed::Opts(o) = parse(&a).expect("解析") else {
        panic!()
    };
    let Die(said) =
        build(&o, &e, &t, Some(&snapshot_rows(&[("other", "")]))).expect_err("在跑还另起了");
    assert!(said.contains("4242") && said.contains("sid-1"), "{said}");
    // 快照里认得出 ⇒ 接上（tmux 那一格在前）。
    assert!(matches!(
        build(&o, &e, &t, Some(&snapshot_rows(&[("work", "sid-1")]))),
        Ok(Plan::Rejoin { .. })
    ));
    // 别的 sid ⇒ 照旧起。
    let b: Vec<String> = ["--resume", "sid-2"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let Parsed::Opts(o2) = parse(&b).expect("解析") else {
        panic!()
    };
    assert!(build(&o2, &e, &t, None).is_ok());
    // 问的目录：继承来的账号目录；`--base` ⇒ 默认家目录（None）。
    SCANNED.with(|s| s.borrow_mut().clear());
    e.inherited_config_dir = Some("/h/.claude-accts/b".into());
    let _ = build(&o, &e, &t, None);
    let c: Vec<String> = ["--resume", "sid-1", "--base"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let Parsed::Opts(o3) = parse(&c).expect("解析") else {
        panic!()
    };
    let _ = build(&o3, &e, &t, None);
    assert_eq!(
        SCANNED.with(|s| s.borrow().clone()),
        vec![Some("/h/.claude-accts/b".to_string()), None]
    );
}

// ── 起会话只有 ccm 一处：monitor 交来的三个选项 ＋ 中转地址在最终 exec 那一处定 ──

/// V151 排列直接喂解析器（新选项不走上面那个换排列的夹具）。
fn plan_v151(args: &[&str], env: &Env) -> Result<Plan, crate::control::ccm::argv::Die> {
    let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    match crate::control::ccm::argv::parse(&a)? {
        Parsed::Opts(o) => build(&o, env, &AccountTable::default(), None),
        other => panic!("{other:?}"),
    }
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// `--account-dir` 就用那个目录（不查账号库）；两个身份 token 在直路上进 agent 进程环境（`--ccm-print` 说出来），
/// 在容器路上原样交给 pane 里那一趟（tmux 边界吃掉的环境不靠它）。
#[test]
fn the_three_options_monitor_hands_over_reach_the_agent_on_both_paths() {
    let e = env();
    let Plan::Direct(d) = plan_v151(
        &[
            "--resume",
            "s1",
            "--",
            "--account-dir",
            "/h/.claude-accts/w",
            "--ccm-rbind-token",
            TOKEN,
            "--ccm-launch-id",
            "s1",
            "--cwd",
            "/p",
        ],
        &e,
    )
    .unwrap() else {
        panic!("该走直路")
    };
    assert_eq!(d.config_dir, "/h/.claude-accts/w");
    let line = render(&Plan::Direct(d));
    assert!(
        line.contains("export CLAUDE_CONFIG_DIR='/h/.claude-accts/w'; "),
        "{line}"
    );
    assert!(
        line.contains(&format!("export CCM_RBIND_TOKEN='{TOKEN}'; ")),
        "{line}"
    );
    assert!(line.contains("export CCM_LAUNCH_ID='s1'; "), "{line}");
    assert!(
        line.ends_with("cd '/p' && exec claude --resume s1"),
        "{line}"
    );

    let Plan::Container(c) = plan_v151(
        &[
            "--resume",
            "s1",
            "--",
            "--ccm-tmux=p-cc",
            "--account-dir",
            "/h/.claude-accts/w",
            "--ccm-rbind-token",
            TOKEN,
            "--ccm-launch-id",
            "s1",
            "--cwd",
            "/p",
        ],
        &e,
    )
    .unwrap() else {
        panic!("该走容器路")
    };
    for want in [
        "'--account-dir' '/h/.claude-accts/w'".to_string(),
        format!("'--ccm-rbind-token' '{TOKEN}'"),
        "'--ccm-launch-id' 's1'".to_string(),
    ] {
        assert!(
            c.payload.contains(&want),
            "容器路没把 {want} 交进去：{}",
            c.payload
        );
    }
}

/// 三个选项的形状闸与互斥：坏值 / 与 `--account` · `--base` 同给 ⇒ 用法错。
#[test]
fn the_three_options_are_judged_and_account_dir_excludes_the_other_account_forms() {
    for bad in [
        vec!["--", "--ccm-rbind-token", "XYZ"],
        vec!["--", "--ccm-launch-id", "a/b"],
        vec!["--", "--account-dir", "rel/x"],
        vec!["--", "--account-dir", "/h/x", "--account", "w"],
        vec!["--", "--account-dir", "/h/x", "--base"],
    ] {
        let a: Vec<String> = bad.iter().map(|s| s.to_string()).collect();
        assert!(
            crate::control::ccm::argv::parse(&a).is_err(),
            "{bad:?} 该被拒"
        );
    }
}

/// 中转地址只在最终 exec 那一处定：问上游选择那一只手（带上这一发的账号），注入的那一句钥匙段是读钥匙文件的命令替换；
/// 环境里已经有一个 ⇒ 不问、不动（不是我们注入的那一形 ⇒ 记下要说一句）；拒 ⇒ 用法错带那一句。
#[test]
fn the_relay_address_is_decided_at_the_final_exec_and_only_there() {
    fn inject(
        agent: &str,
        a: &crate::accounts::upstream_select::endpoint::LaunchAccount,
    ) -> Result<Option<String>, String> {
        assert_eq!(agent, "claude-code", "问的不是这一家的适配器 id");
        assert_eq!(
            a,
            &crate::accounts::upstream_select::endpoint::LaunchAccount::Named {
                config_dir: "/h/.claude-accts/w".into()
            },
            "带过去的不是这一发的账号"
        );
        Ok(Some("http://127.0.0.1:8788/t/claude-code/w".into()))
    }
    fn never(
        _: &str,
        _: &crate::accounts::upstream_select::endpoint::LaunchAccount,
    ) -> Result<Option<String>, String> {
        panic!("环境里已经有地址还去问了")
    }
    fn refuse(
        _: &str,
        _: &crate::accounts::upstream_select::endpoint::LaunchAccount,
    ) -> Result<Option<String>, String> {
        Err("中转没在听".into())
    }
    let args = [
        "--resume",
        "s1",
        "--",
        "--account-dir",
        "/h/.claude-accts/w",
    ];
    let mut e = env();
    e.relay = Some(inject);
    let Plan::Direct(d) = plan_v151(&args, &e).unwrap() else {
        panic!()
    };
    assert_eq!(
        d.relay.as_deref(),
        Some("http://127.0.0.1:8788/t/claude-code/w")
    );
    assert!(
        render(&Plan::Direct(d)).contains(
            "export ANTHROPIC_BASE_URL='http://127.0.0.1:8788/'$(cat ~/.cc-monitor/relay-key)'/t/claude-code/w'; "
        ),
        "注入那一句的形状变了"
    );
    e.relay = Some(never);
    e.anthropic_base_url = Some("https://my.gateway/v1".into());
    let Plan::Direct(d) = plan_v151(&args, &e).unwrap() else {
        panic!()
    };
    assert_eq!(d.relay, None);
    assert!(d.keeps_user_base_url, "用户自己的端点没记下要说一句");
    e.anthropic_base_url = Some(format!(
        "http://127.0.0.1:8788/{}/t/claude-code/w",
        "a".repeat(64)
    ));
    let Plan::Direct(d) = plan_v151(&args, &e).unwrap() else {
        panic!()
    };
    assert!(
        !d.keeps_user_base_url,
        "外层 ccm 带进来的我们那一形被当成了用户的端点"
    );
    e.anthropic_base_url = None;
    e.relay = Some(refuse);
    assert_eq!(plan_v151(&args, &e).unwrap_err().0, "中转没在听");
    // 容器路不问（pane 里那一趟走直路时自己问）。
    e.relay = Some(never);
    assert!(matches!(
        plan_v151(&["--resume", "s1", "--", "--ccm-tmux=p-cc"], &e).unwrap(),
        Plan::Container(_)
    ));
}
