/// 一份 `ccm` 模块源码里吃一个 agent 名、回那一家那一份的函数（行首 `pub(crate) fn <名>(agent: &str)`）。
///
/// 人群只到 `control/ccm/mod.rs` 为止，`agents/mod.rs::account_env_of` 不算 —— 那是 ccm 问适配层的口，不是 ccm 自己的表。
fn per_agent_fns(src: &str) -> Vec<String> {
    let prod = guard_core::production_code(src);
    let mut out: Vec<String> = Vec::new();
    for line in prod.lines() {
        let Some(rest) = line.trim().strip_prefix("pub(crate) fn ") else {
            continue;
        };
        let Some((name, args)) = rest.split_once('(') else {
            continue;
        };
        if args.starts_with("agent: &str)") {
            out.push(name.to_string());
        }
    }
    out
}

/// →**`ccm` 是一套骨架，按哪一家起有什么不同全在适配层的起会话事实上**：
/// `control/ccm/mod.rs` 生产段里吃 agent 名的函数：零（带正控）。
#[test]
fn ccm_has_no_per_agent_function() {
    assert_eq!(
        per_agent_fns(
            "pub(crate) fn env_of(agent: &str) -> &str {\n    \"\"\n}\npub(crate) fn other(x: u8) {}\n"
        ),
        vec!["env_of".to_string()],
        "认法坏了：认不出一个吃 agent 名的函数"
    );
    let p = guard_core::addr::repo_root().join("src/backend/control/ccm/mod.rs");
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()));
    assert!(
        guard_core::production_code(&src)
            .lines()
            .any(|l| l.trim().starts_with("pub(crate) fn ")),
        "`control/ccm/mod.rs` 生产段里一个 `pub(crate) fn` 都抠不到 —— 读错了文件，下面的零个是空转"
    );
    let fns = per_agent_fns(&src);
    assert!(
        fns.is_empty(),
        "`control/ccm/mod.rs` 又长出了吃 agent 名的函数：{fns:?}\n\
         ⇒ 按哪一家起会有什么不同，做成适配层起会话事实（`agents::LaunchFace`）上的一格（没声明 = 不支持），ccm 问那一格。"
    );
}
