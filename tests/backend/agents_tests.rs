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
    Adapter { kind: "alpha",   home: synth_home_present, account_env: None, assets: None },
    Adapter { kind: "ghost",   home: synth_home_absent, account_env: None, assets: None },
    Adapter { kind: "nameless", home: synth_home_unknown, account_env: None, assets: None },
    Adapter { kind: "filey",   home: synth_home_is_a_file, account_env: None, assets: None },
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
