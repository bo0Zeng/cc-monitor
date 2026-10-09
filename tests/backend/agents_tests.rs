use super::*;

/// 合成夹具的根。**非捕获 fn**（不是闭包）—— [`Adapter::home`] 是裸函数指针，
/// 而这正是 `S5` 选函数指针的一个副产物：假 agent 不需要任何运行时装配。
fn fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!("ccm-s5-registry-{}", std::process::id()))
}

/// 一个**存在**的 home（测试会先把它 `mkdir` 出来）。
fn synth_home_present() -> Option<PathBuf> {
    Some(fixture_root().join("present-home"))
}
/// 一个**不存在**的 home —— 判准该把它挡掉。
fn synth_home_absent() -> Option<PathBuf> {
    Some(fixture_root().join("absent-home"))
}
/// 一个同名的**文件** —— 存在，但不是目录，同样该被挡掉。
fn synth_home_is_a_file() -> Option<PathBuf> {
    Some(fixture_root().join("a-file"))
}
/// **说不出候选路径**的那家（codex 在没有 `HOME`/`CODEX_HOME` 的环境里就是这样）。
fn synth_home_unknown() -> Option<PathBuf> {
    None
}

#[rustfmt::skip]
const SYNTH_REGISTRY: &[Adapter] = &[
    Adapter { kind: "alpha",   home: synth_home_present, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
    Adapter { kind: "ghost",   home: synth_home_absent, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
    Adapter { kind: "nameless", home: synth_home_unknown, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
    Adapter { kind: "filey",   home: synth_home_is_a_file, account_env: None, assets: None, history: None, upstream: None, mcp: None, footprint: None, accounts: None, records: None, processes: None, launch: None, compact_request: None, local: None },
];

/// `S5-Y1`：**看得见 = home 目录存在**。整条链喂合成注册表，一次验四种形态。
///
/// ⚠ 三个阴性项（`ghost` 缺席 · `nameless` 说不出路径 · `filey` 是文件）是本条的
/// 全部意义所在：只喂存在的那家、只断言"返回非空"的话，判准坏成「永远全放行」照样绿 ——
/// 而那种坏法正是本件最怕的：backend 会声明它其实**看不见**的 agent，
/// 消费方照着那个 path 去列会话，只会得到空/报错。
///
/// ⚠ 本条**完全不依赖跑测试这台机器上装了什么** —— 注册表是合成的，
/// home 路径由 pid 隔开。真机那半由
/// `wire::tests::the_backend_can_already_discover_homes_it_just_does_not_send_them` 兜。
#[test]
fn only_agents_whose_home_directory_exists_are_visible() {
    let root = fixture_root();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("present-home")).expect("建 home");
    std::fs::write(root.join("a-file"), b"not a home").expect("建文件");

    let got = visible_among(SYNTH_REGISTRY);
    let seen: Vec<(&str, &str)> = got
        .iter()
        .map(|h| (h.agent_kind.as_str(), h.path.as_str()))
        .collect();
    let want_path = synth_home_present()
        .expect("夹具")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        seen,
        vec![("alpha", want_path.as_str())],
        "判准是「home 目录**存在**」，四种形态各验一次：\n\
             · alpha（目录在）——进；\n\
             · ghost（目录不在）——一项都不许产，否则后端会声明它其实看不见的 agent；\n\
             · nameless（连候选路径都说不出）——出局，别拿空路径去 stat；\n\
             · filey（同名的**文件**）——不算家，报出去的 path 会被消费方当目录拼子路径。"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// `S5-Y1` 的另一半：**生产注册表**这条路与合成那条是同一条 —— 别只有夹具是活的。
///
/// ⚠ 它是**恒等式**（`visible_homes()` ≡ `visible_among(REGISTRY)`），
/// 所以在一台**一个 agent home 都没有**的机器上它是空转的（两边都空）——
/// 如实登记在功能件 `§4`。它逮的是「有人把 `visible_homes` 的body 掏空/改成读别的表」。
#[test]
fn the_public_entry_point_is_the_registry_run_through_the_same_criterion() {
    assert_eq!(
        visible_homes(),
        visible_among(REGISTRY),
        "`visible_homes()` 不再是「注册表 × 判准」了 —— \
             生产入口与被判据喂过夹具的那条路已经分叉"
    );
}

/// `S5-Y3`：注册表每家一条、kind 非空且互不相同。
///
/// ⚠ 重复的 kind 是**静默**坏法：`homes` 里出现两条同 `agent_kind`，
/// 消费方（`claude_home_from_hello` 那类 `.find(kind == …)`）只会拿到第一条，
/// 另一条永远无人消费 —— 编译过、判据全绿、行为错。
#[test]
fn the_registry_names_every_agent_exactly_once() {
    assert!(
        REGISTRY.len() >= 2,
        "注册表只剩 {} 家 —— 有人把某个适配层从表里删了（或抽取坏了）",
        REGISTRY.len()
    );
    let mut kinds: Vec<&str> = REGISTRY.iter().map(|a| a.kind).collect();
    assert!(
        kinds.iter().all(|k| !k.trim().is_empty()),
        "有 agent 的 kind 是空的 ⇒ 它在 wire 上没有身份：{kinds:?}"
    );
    let before = kinds.len();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        before,
        "注册表里有**重复的 agent_kind**：{kinds:?}\n\
             ⇒ `homes` 会出现两条同 kind 的项，而消费侧按 kind 取第一条 —— \
             第二条永远无人消费，且没有任何东西会报错。"
    );
}

/// `S5-Y3` 的另一半：注册表答得出的 home **就是**各适配层自己那条解析路。
///
/// ⚠ 本条**不断言具体路径**（那取决于跑测试这台机器的 `HOME`/`CLAUDE_CONFIG_DIR`）——
/// 它断言的是"注册表接的是那根管子"：函数指针指错了家（两条都接 claude），
/// 上面两条都还会绿，而后端会把同一个目录报成两个 agent 的家。
#[test]
fn each_registry_entry_asks_its_own_adapter_for_the_home() {
    let claude = REGISTRY
        .iter()
        .find(|a| a.kind == claudecode::AGENT_KIND)
        .expect("注册表里没有 claude 那家");
    assert_eq!(
        (claude.home)(),
        claudecode::home(),
        "claude 那条的 home 指针没接到 `claudecode::home`"
    );
    let cx = REGISTRY
        .iter()
        .find(|a| a.kind == codex::AGENT_KIND)
        .expect("注册表里没有 codex 那家");
    assert_eq!(
        (cx.home)(),
        codex::home(),
        "codex 那条的 home 指针没接到 `codex::home`"
    );
    assert_ne!(
        claudecode::AGENT_KIND,
        codex::AGENT_KIND,
        "两家的 kind 撞了"
    );
}

// 要求：「同一个数写在两处，其中一处一定先腐」· 「加第三种 agent ＝ 在 `agents/` 下加一个目录 ＋ 在注册表里加一行，**不改主程序**」。
// ═══ 起会话事实只有一个家（`agents/<名>/resume.rs` 经注册表 `Adapter.launch`）════════════════
// 要求：「起会话那几格事实在 monitor `adapter.rs` 与后端 `control/ccm/` 各一份（靠金样对着）→
// 收成后端一个家，monitor 经帧或生成物取」。`ccm --agent` 的闭集也从这张注册表派生（`agents::launchable_kinds`）。
// 从前住 monitor `tests/frontend/shell/agent_profile_parity_tests.rs` 与 `adapter_tests.rs` 那几条随家搬来（金样的期望一字未改）。

const PROFILE_GOLDEN: &str = include_str!("../__fixtures__/agent-profile-golden.tsv");

/// 金样 → `(agent, key, value)`（`<empty>` 是空串的写法）。
fn golden_rows() -> Vec<(String, String, String)> {
    PROFILE_GOLDEN
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert_eq!(f.len(), 3, "夹具行不是 3 列：{l:?}");
            let v = if f[2] == "<empty>" { "" } else { f[2] };
            (f[0].to_string(), f[1].to_string(), v.to_string())
        })
        .collect()
}

/// resume 的调用形态：`--` 开头 ＝ flag，否则 ＝ 子命令。
fn resume_kind_of(token: &str) -> &'static str {
    if token.starts_with("--") {
        "flag"
    } else {
        "subcommand"
    }
}

/// ★ 注册表里每一家的起会话事实 == 金样（4 个 key × 2 个 agent，两向：金样里每一家都在注册表里、注册表里每一家都在金样里）。
#[test]
fn the_launch_faces_agree_with_the_golden_table() {
    let rows = golden_rows();
    assert!(
        rows.len() >= 6,
        "只解析出 {} 行 —— 夹具路径或解析坏了",
        rows.len()
    );
    assert!(
        rows.iter().any(|r| r.2.is_empty()),
        "夹具里一行空值都没有 —— `<empty>` 那条路没被走过"
    );
    let golden_agents: std::collections::BTreeSet<&str> =
        rows.iter().map(|r| r.0.as_str()).collect();
    let registered: std::collections::BTreeSet<&str> = REGISTRY
        .iter()
        .filter(|a| a.launch.is_some())
        .map(|a| a.kind)
        .collect();
    assert_eq!(
        golden_agents, registered,
        "金样里的 agent 集 ≠ 注册表里带起会话事实的那几家"
    );
    let mut bad = Vec::new();
    for (agent, key, want) in &rows {
        let f = launch_face_among(REGISTRY, agent)
            .unwrap_or_else(|| panic!("注册表里没有 `{agent}` 的起会话事实"));
        let got = match key.as_str() {
            "default_launcher" => f.default_launcher.to_string(),
            "resume_token" => f.resume_token.to_string(),
            "resume_kind" => resume_kind_of(f.resume_token).to_string(),
            "nested_env" => f.nested_env.join(" "),
            "launch_args" => f.launch_args.join(" "),
            "preset_sid" => f.preset_sid.unwrap_or_default().to_string(),
            other => panic!("夹具里出现了未知 key `{other}` —— 加一项要来这里表态"),
        };
        if &got != want {
            bad.push(format!("  {agent}.{key}: 期望 {want:?} 实得 {got:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "起会话事实与金样不一致：\n{}",
        bad.join("\n")
    );
}

/// ★ 「不说是哪一家时起谁」与「凭据文件的行挂在谁名下」各由注册表里**恰一家**声明（两家都声明 ⇒ 谁先谁赢，那是静默）。
#[test]
fn exactly_one_family_is_the_default_and_exactly_one_owns_the_credentials_file() {
    let defaults: Vec<&str> = REGISTRY
        .iter()
        .filter(|a| a.launch.is_some_and(|f| f.is_default))
        .map(|a| a.kind)
        .collect();
    assert_eq!(
        defaults,
        [crate::agents::default_kind()],
        "声明默认的不是恰一家：{defaults:?}"
    );
    let owners: Vec<&str> = REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .filter(|u| u.owns_credentials_file)
        .map(|u| u.route_id)
        .collect();
    assert_eq!(
        owners,
        [crate::agents::credentials_file_agent()],
        "声明拥有凭据文件的不是恰一家：{owners:?}"
    );
}

/// ★★ 收 agent 名字的入口只有一种认法（`pick_among`）：没写 / 写空 ⇒ 默认那一家；拼错 ⇒ 报错、列出认得的几家，**不落默认**。
/// 喂一张含夹具家的注册表，期望的名单逐字手写（不从被测的注册表现推）。
#[test]
fn an_agent_name_left_out_is_the_default_and_a_misspelled_one_is_refused() {
    let row = |kind, home, launch| Adapter {
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
        compact_request: None,
        local: None,
    };
    let mut reg: Vec<Adapter> = REGISTRY
        .iter()
        .map(|a| row(a.kind, a.home, a.launch))
        .collect();
    reg.push(row(fake::AGENT_KIND, synth_home_absent, Some(fake::LAUNCH)));
    let kind = |n: Option<&str>| pick_kind_among(&reg, n).map(|(k, _)| k);
    // 没写 / 写空 / 只有空白 ⇒ 默认那一家。
    for n in [None, Some(""), Some("  ")] {
        assert_eq!(kind(n), Ok("claude"), "{n:?} 没落到默认那一家");
    }
    // 写了注册表里的一家 ⇒ 就是它（两侧空白不算字）。
    assert_eq!(kind(Some("codex")), Ok("codex"));
    assert_eq!(kind(Some(" fake ")), Ok("fake"));
    // 拼错 / 大小写不对 / 别家的名字 ⇒ 报错，说出那个名字与认得的几家。
    for bad in ["claud", "Codex", "gemini", "claude-code"] {
        let said = kind(Some(bad)).expect_err(&format!("{bad:?} 被认成了某一家"));
        assert_eq!(
            said,
            copy_core::copy_text(
                "beAgents.pick.unknown",
                &[("agent", bad), ("known", "claude / codex / fake")]
            )
        );
    }
    // 适配器 id 那一个值域同一条规则（名单换成适配器 id）。
    let adapter = |n: Option<&str>| pick_adapter_among(&reg, n).map(|(_, f)| f.adapter_id);
    assert_eq!(adapter(Some("")), Ok("claude-code"));
    assert_eq!(adapter(Some("codex")), Ok("codex"));
    assert_eq!(
        adapter(Some("claude")),
        Err(copy_core::copy_text(
            "beAgents.pick.unknown",
            &[("agent", "claude"), ("known", "claude-code / codex / fake")]
        )
        .to_string()),
        "适配器 id 那一问把 wire kind 也认了"
    );
    // 生产那张：入口共用的两个口与默认那一家对得上。
    assert_eq!(pick_kind(None).map(|(k, _)| k), Ok(default_kind()));
    assert_eq!(
        pick_adapter(Some("")).ok(),
        default_launch_among(REGISTRY).map(|(_, f)| f.adapter_id)
    );
}

/// ★ `resume_kind` 那一列不是凭空写的：夹具说 flag 的必须以 `--` 开头，说 subcommand 的必须不以 `--` 开头（从 monitor 搬来）。
#[test]
fn the_resume_kind_column_matches_reality() {
    let r = golden_rows();
    let get = |agent: &str, key: &str| -> String {
        r.iter()
            .find(|x| x.0 == agent && x.1 == key)
            .map(|x| x.2.clone())
            .unwrap_or_else(|| panic!("夹具里缺 {agent}.{key}"))
    };
    for agent in ["claude", "codex"] {
        let kind = get(agent, "resume_kind");
        let token = get(agent, "resume_token");
        match kind.as_str() {
            "flag" => assert!(
                token.starts_with("--"),
                "{agent} 记成 flag 但 token `{token}` 不以 `--` 开头"
            ),
            "subcommand" => assert!(
                !token.starts_with("--") && !token.is_empty(),
                "{agent} 记成 subcommand 但 token `{token}` 不像子命令名"
            ),
            other => panic!("{agent} 的 resume_kind `{other}` 不在 flag/subcommand 里"),
        }
    }
}

/// ★ 一个家：嵌套会话标记这几个名字在后端生产段里只作为串字面量出现在 `agents/claudecode/resume.rs`（恰各一处），
/// `control/ccm/` 零处（它按注册表读）；monitor 生产树零处（它读生成物）。异源：名字取自金样，不取自被测常量。
#[test]
fn the_launch_facts_are_spelled_in_one_place() {
    let rows = golden_rows();
    let names: Vec<String> = rows
        .iter()
        .filter(|r| r.1 == "nested_env")
        .flat_map(|r| {
            r.2.split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        names.len() >= 4,
        "金样里的嵌套标记少于 4 个 —— 抽取坏了：{names:?}"
    );
    let root = crate::guard_support::repo_root();
    let backend: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&root.join("src/backend"), &["rs"], &[])
            .into_iter()
            .map(|(p, raw)| {
                (
                    p.strip_prefix(&root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    guard_core::production_code(&raw),
                )
            })
            .collect();
    let monitor: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[])
            .into_iter()
            .map(|(p, raw)| {
                (
                    p.strip_prefix(&root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    guard_core::production_code(&raw),
                )
            })
            .collect();
    assert!(
        backend.len() > 100 && monitor.len() > 50,
        "扫描面塌了：后端 {} 份 · monitor {} 份",
        backend.len(),
        monitor.len()
    );
    let mut bad = Vec::new();
    for n in &names {
        let lit = format!("\"{n}\"");
        let hits: Vec<&str> = backend
            .iter()
            .filter(|(_, b)| b.contains(&lit))
            .map(|(f, _)| f.as_str())
            .collect();
        if hits != ["src/backend/agents/claudecode/resume.rs"] {
            bad.push(format!(
                "后端里 {lit} 住在 {hits:?}（该只在 agents/claudecode/resume.rs）"
            ));
        }
        for (f, b) in &monitor {
            if b.contains(&lit) {
                bad.push(format!("monitor 生产段 {f} 又写了一份 {lit}（该读生成物）"));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ── `K-R93`：把前端那份画像**生成出去**（生成器从 monitor `adapter_tests.rs` 搬来：值的家换成了后端注册表）──
//
// `npm run gen:types` 跑 `cd src/backend && cargo test --lib export_bindings` ⇒ 本条会被它跑到；门禁 `generated` 那一格随后判
// 「已提交的那份与 Rust 源一不一致」。monitor 的起前清洗也读这份生成物（`lib.rs::nested_env_markers`）。

/// 生成物的头。「Do not edit this file manually」那句话是给 `generated-boundary-guard.vitest.ts` 看的，别改措辞。
/// 🔴 与 `TABLE_HEADER_TAIL` 刻意分成两个字面量：`};` 落在列 0 会把测试模块切断（`guard_core::test_module_ranges`，`K-R93-deathvalue.md`）。
const TABLE_HEADER: &str = r#"// 本文件由 `src/backend/agents/mod.rs` 的注册表经 `export_bindings_agent_profile_table` 生成
// （`npm run gen:types`）。Do not edit this file manually.
//
// `K-R93`：**前端那份 agent 画像的值来自后端**。值的唯一住址是后端适配层（`agents/<名>/resume.rs`，注册表 `Adapter.launch`），
// `ccm` 按注册表读、界面与 monitor 读这份生成物 —— 不再是 monitor `adapter.rs` 与后端 `control/ccm/` 各一份。
//
// ⚠ **`null` ≠ 空**：`null` = 这一格今天没有（后端 `None`），**不许拿 claude 那份顶上**
// （`KR93D3`）；`[]` 才是「考据过、确实是空的」。
// 工具 / 判活进程那五格不在这里了：判卡型 · 认 tmux 会话是那台后端的事（`toolCards` · 批量停 / 起认窗格的 `agent`）。

export type AgentProfileRow = {
  /** 这张表的键（= `agent-profile-golden.tsv` 第一列，也是 `ccm --agent` 收的那个名字）。 */
  agent: string;
  /** 后端适配器 id（起会话那一发交回，上游选择按它挑那一行）。 */
  adapterId: string;
  defaultLauncher: string;
  launcherAlias: string | null;
  resumeKind: "flag" | "subcommand";
  resumeToken: string;
  nestedEnvVars: string[];
  /** 这一家有没有账号这一维（选号 · 跟随上次的号只对有的那一家）。 */
  hasAccounts: boolean;
  /** 这一家对用户的叫法（产品全称：「{名} 默认」·「{名} 会话还不能选账号」这类话里用）。 */
  displayName: string;
  /** 这一家认得的模型名（账号页「默认模型」下拉的选项）；`null` ＝ 没考据过。 */
  models: string[] | null;
  /** 消息流里说话的那一方叫什么（卡头 · 刻度悬停）：短名。 */
  speakerName: string;
"#;

/// 接着上面那一段 —— **第一行就是那个收尾的 `};`**（见上面为什么不能合并）。
const TABLE_HEADER_TAIL: &str = r#"};

export const AGENT_PROFILE_TABLE: readonly AgentProfileRow[] = [
"#;

/// `DEFAULT_AGENT` 那一格的头注。
const DEFAULT_HEADER: &str = r#"
/** 不说是哪一家时起的那一家（注册表里声明默认的那一家）：不给 agent 名字 ⇒ 就是它；给了表里没有的名字 ⇒ 拒。 */
"#;

/// 一个 TS 串字面量。**这个生成器不做转义** —— 真出现要转义的字符就当场炸，不产坏 TS。
fn ts_str(s: &str) -> String {
    assert!(
        !s.contains('"') && !s.contains('\\'),
        "画像里出现了要转义的字符：{s:?}"
    );
    format!("\"{s}\"")
}

fn render_row(kind: &str, f: &LaunchFace, has_accounts: bool) -> String {
    let list: Vec<String> = f.nested_env.iter().map(|s| ts_str(s)).collect();
    let fields = [
        ("agent", ts_str(kind)),
        ("adapterId", ts_str(f.adapter_id)),
        ("defaultLauncher", ts_str(f.default_launcher)),
        (
            "launcherAlias",
            f.launcher_alias
                .map(ts_str)
                .unwrap_or_else(|| "null".to_string()),
        ),
        ("resumeKind", ts_str(resume_kind_of(f.resume_token))),
        ("resumeToken", ts_str(f.resume_token)),
        ("nestedEnvVars", format!("[{}]", list.join(", "))),
        ("hasAccounts", has_accounts.to_string()),
        ("displayName", ts_str(f.display_name)),
        (
            "models",
            f.models.map_or_else(
                || "null".to_string(),
                |m| {
                    format!(
                        "[{}]",
                        m.iter().map(|s| ts_str(s)).collect::<Vec<_>>().join(", ")
                    )
                },
            ),
        ),
        ("speakerName", ts_str(f.speaker_name)),
    ];
    let mut s = String::from("  {\n");
    for (key, value) in fields {
        s.push_str(&format!("    {key}: {value},\n"));
    }
    s.push_str("  },\n");
    s
}

fn render_agent_profile_table() -> String {
    let mut s = String::from(TABLE_HEADER);
    s.push_str(TABLE_HEADER_TAIL);
    for a in REGISTRY {
        if let Some(f) = a.launch {
            s.push_str(&render_row(a.kind, &f, a.account_env.is_some()));
        }
    }
    s.push_str("];\n");
    s.push_str(DEFAULT_HEADER);
    s.push_str(&format!(
        "export const DEFAULT_AGENT: string = {};\n",
        ts_str(crate::agents::default_kind())
    ));
    s
}

/// `K-R93`：生成 `src/frontend/ui/generated/agent-profile-table.ts`。
#[test]
fn export_bindings_agent_profile_table() {
    let out =
        crate::guard_support::repo_root().join("src/frontend/ui/generated/agent-profile-table.ts");
    std::fs::write(&out, render_agent_profile_table())
        .unwrap_or_else(|e| panic!("写不进 {}：{e}", out.display()));
}

/// ★ ccm 不再做 resume 的决定（之后它是 claude 的壳；从 monitor `agent_profile_parity_tests.rs` 搬来）。
#[test]
fn ccm_makes_no_resume_decision_since_it_became_a_shell() {
    let dir = crate::guard_support::repo_root().join("src/backend/control/ccm");
    let ccm: String = ["mod.rs", "argv.rs", "plan.rs"]
        .iter()
        .map(|f| {
            std::fs::read_to_string(dir.join(f))
                .unwrap_or_else(|e| panic!("读不到 control/ccm/{f}：{e}"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        ccm.len() > 5000,
        "control/ccm 三份只有 {} 字节，抽错了？",
        ccm.len()
    );
    assert!(
        !ccm.contains("fn resume_flag(") && !ccm.contains("beArgv.validate.noResume"),
        "`control/ccm/` 又长出了 resume 的决定 —— ccm 只看不吃 `--resume`。"
    );
}

/// 会话的项目目录 ＝ 记录开头第一条带 `cwd` 的那一条：后面的记录进了子目录也不跟着漂；只读开头，大文件不整读。
#[test]
fn project_dir_is_the_first_cwd_in_the_head_and_reading_is_bounded() {
    let dir = std::env::temp_dir().join(format!("ccm-projdir-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rec = |cwd: &str| {
        format!(r#"{{"type":"user","cwd":"{cwd}","message":{{"role":"user","content":"q"}}}}"#)
    };
    // 开头一条 /a/proj，后面若干条 /a/proj/sub（shell 进了子目录）。
    let drift = dir.join("drift.jsonl");
    let mut body = String::from("{\"type\":\"mode\",\"mode\":\"normal\"}\n");
    body += &(rec("/a/proj") + "\n");
    for _ in 0..5 {
        body += &(rec("/a/proj/sub") + "\n");
    }
    std::fs::write(&drift, &body).unwrap();
    assert_eq!(
        crate::agents::claudecode::parse::project_dir(&drift).as_deref(),
        Some("/a/proj")
    );

    // 上界：带 cwd 的那一条排在上界之后 ⇒ 读不到（没有越过上界去读）。上界之内 ⇒ 读得到（正控）。
    let pad = |n: u64| {
        format!(
            "{{\"type\":\"mode\",\"pad\":\"{}\"}}\n",
            "x".repeat(n as usize)
        )
    };
    let cap = crate::agents::HEAD_CAP;
    let far = dir.join("far.jsonl");
    std::fs::write(&far, pad(cap) + &rec("/a/proj") + "\n").unwrap();
    assert_eq!(
        crate::agents::claudecode::parse::project_dir(&far),
        None,
        "上界之后的那一条也被读到了 —— 头部没有上界"
    );
    let near = dir.join("near.jsonl");
    std::fs::write(&near, pad(cap / 2) + &rec("/a/proj") + "\n").unwrap();
    assert_eq!(
        crate::agents::claudecode::parse::project_dir(&near).as_deref(),
        Some("/a/proj")
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ─── 通用层按 kind 显式取（不按注册序取「第一家」）───────────────────────────
//
// 合成注册表上两家同时声明资产面 · MCP 读面 · 账号库 · 找会话文件；问后一家 ⇒ 落到后一家（按注册序取第一家的话会落到前一家）。

fn alpha_skills() -> Option<PathBuf> {
    Some(PathBuf::from("/alpha/skills"))
}
fn beta_skills() -> Option<PathBuf> {
    Some(PathBuf::from("/beta/skills"))
}
fn alpha_project_skills(p: &Path) -> PathBuf {
    p.join(".alpha-skills")
}
fn beta_project_skills(p: &Path) -> PathBuf {
    p.join(".beta-skills")
}
fn alpha_mcp_file() -> Option<PathBuf> {
    Some(PathBuf::from("/alpha/mcp.json"))
}
fn beta_mcp_file() -> Option<PathBuf> {
    Some(PathBuf::from("/beta/mcp.json"))
}
fn no_sightings(_: &[String], _: Option<&Path>) -> Sightings {
    Sightings::default()
}
fn alpha_mcp(_: Option<&Path>) -> McpRead {
    McpRead {
        dirs: vec!["alpha".into()],
        ..McpRead::default()
    }
}
fn beta_mcp(_: Option<&Path>) -> McpRead {
    McpRead {
        dirs: vec!["beta".into()],
        ..McpRead::default()
    }
}
fn alpha_find(_: &Path, sid: &str) -> Result<PathBuf, String> {
    (sid == "alpha-sid")
        .then(|| PathBuf::from("/alpha/rec"))
        .ok_or_else(|| "alpha: not mine".to_string())
}
fn beta_find(_: &Path, sid: &str) -> Result<PathBuf, String> {
    (sid == "beta-sid")
        .then(|| PathBuf::from("/beta/rec"))
        .ok_or_else(|| "beta: not mine".to_string())
}
fn shared_root(home: &Path) -> PathBuf {
    home.to_path_buf()
}
fn no_email(_: &Path) -> Option<String> {
    None
}
fn no_line(_: &str) -> Result<Option<ParsedLine>, String> {
    Ok(None)
}
fn no_sid(_: &Path) -> Option<String> {
    None
}

const fn records(find: fn(&Path, &str) -> Result<PathBuf, String>) -> RecordFace {
    RecordFace {
        parse: no_line,
        sid: no_sid,
        is_session_file: |_| false,
        tree: None,
        turn_end: None,
        find_session: Some(find),
        branch: None,
        drift: None,
        text: None,
        delete: None,
        response_id: None,
        run_of: None,
        child_link: None,
        children: None,
        project_dir: None,
    }
}

const fn accounts(config_file: &'static str) -> AccountsFace {
    AccountsFace {
        identity: &[],
        config_file,
        user_mcp_key: "servers",
        shared_root,
        email_in: no_email,
        watched: &[],
        session_env: crate::agents::SessionEnvKeys {
            config_dir: "",
            base_url: "",
            settings_may_set_base_url: |_, _, _| false,
        },
        trust_in: |_, _| Err((String::new(), String::new())),
        trust: None,
    }
}

fn two_families() -> Vec<Adapter> {
    let row = |kind: &'static str,
               assets: AssetFace,
               mcp: fn(Option<&Path>) -> McpRead,
               acc: AccountsFace,
               find: fn(&Path, &str) -> Result<PathBuf, String>| Adapter {
        kind,
        home: synth_home_unknown,
        account_env: None,
        assets: Some(assets),
        history: None,
        upstream: None,
        mcp: Some(McpFace { read: mcp }),
        footprint: None,
        accounts: Some(acc),
        records: Some(records(find)),
        processes: None,
        launch: None,
        compact_request: None,
        local: None,
    };
    vec![
        row(
            "alpha",
            AssetFace {
                scan: no_sightings,
                skills_root: alpha_skills,
                project_skills_root: alpha_project_skills,
                user_mcp_file: alpha_mcp_file,
                project_mcp_file: ".alpha-mcp.json",
                servers_key: "servers",
            },
            alpha_mcp,
            accounts("alpha.json"),
            alpha_find,
        ),
        row(
            "beta",
            AssetFace {
                scan: no_sightings,
                skills_root: beta_skills,
                project_skills_root: beta_project_skills,
                user_mcp_file: beta_mcp_file,
                project_mcp_file: ".beta-mcp.json",
                servers_key: "servers",
            },
            beta_mcp,
            accounts("beta.json"),
            beta_find,
        ),
    ]
}

#[test]
fn each_question_lands_on_the_family_it_names_not_the_first_one() {
    let reg = two_families();
    let proj = Path::new("/p");
    assert_eq!(
        skill_root_among(&reg, "beta", None),
        Some(PathBuf::from("/beta/skills"))
    );
    assert_eq!(
        skill_root_among(&reg, "beta", Some(proj)),
        Some(PathBuf::from("/p/.beta-skills"))
    );
    assert_eq!(
        skill_root_among(&reg, "alpha", None),
        Some(PathBuf::from("/alpha/skills"))
    );
    assert_eq!(
        mcp_read_among(&reg, "beta", None).map(|r| r.dirs),
        Some(vec!["beta".to_string()])
    );
    assert_eq!(
        accounts_face_among(&reg, "beta").map(|a| a.config_file),
        Some("beta.json")
    );
    // 认不出的那一家 ⇒ 没有，不落到第一家。
    assert_eq!(skill_root_among(&reg, "gamma", None), None);
    assert!(mcp_read_among(&reg, "gamma", None).is_none());
    assert!(accounts_face_among(&reg, "gamma").is_none());
}

#[test]
fn finding_a_session_asks_every_family_and_whoever_knows_it_answers() {
    let reg = two_families();
    let root = Path::new("/records");
    assert_eq!(
        find_session_file_among(&reg, root, "beta-sid"),
        Ok(PathBuf::from("/beta/rec"))
    );
    assert_eq!(
        find_session_file_among(&reg, root, "alpha-sid"),
        Ok(PathBuf::from("/alpha/rec"))
    );
    assert_eq!(
        find_session_file_among(&reg, root, "nobody"),
        Err("alpha: not mine".to_string())
    );
}

/// 请求里没说是哪一家时那几处取「唯一声明了那一格的那一家」：两家都声明 ⇒ 说不出（`None`，照实拒），不按注册序挑。
#[test]
fn the_sole_family_is_answered_only_when_there_is_exactly_one() {
    let reg = two_families();
    assert_eq!(sole_kind_among(&reg, |a| a.assets.is_some()), None);
    assert_eq!(sole_kind_among(&reg, |a| a.accounts.is_some()), None);
    assert_eq!(record_tree_kind_among(&reg), None);
    assert_eq!(
        sole_kind_among(&reg[1..], |a| a.assets.is_some()),
        Some("beta")
    );
    assert_eq!(sole_kind_among(&reg, |a| a.launch.is_some()), None);
    // 生产注册表上今天各恰一家（账号库 · 资产面 · MCP 读面 · 记录树）。
    for (what, got) in [
        ("账号库", sole_kind(|a| a.accounts.is_some())),
        ("资产面", sole_kind(|a| a.assets.is_some())),
        ("MCP 读面", sole_kind(|a| a.mcp.is_some())),
        ("记录树", record_tree_kind()),
    ] {
        assert!(got.is_some(), "生产注册表上「{what}」不是恰一家");
    }
}
