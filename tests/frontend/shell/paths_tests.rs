//! # 要求住址：`INVARIANTS §2`（含「唯一的明文例外：`CCM_DATA_DIR`」那一段）· V160（一台机器一个家 `~/.cc-monitor/`）
//!
//! `with_nothing_set_it_is_the_documented_default` 点的是 `§2` 正文，逐字「monitor 自己的 data dir 永远是 `~/.cc-monitor/`」。
//! 其余几条判的是那句「永远」的**出口**（`config.rs::monitor_data_dir_from`），逐字点 `§2` 例外段的四条规矩：
//! 「只认**绝对路径**；空串 == 没设」·「给了但不合法（相对路径）⇒ **`None`，不退回用户真 profile**」·
//! 「全树只经 `config.rs::resolve_monitor_data_dir` 派生」。
//! 〔JA1 点址 2026-09-24：当时缺条、候选升格 · TL2 2026-09-25 补成 `§2` 的明文例外（IV1 报的第 3 件）〕

use super::*;

// ════════════════════════════════════════════════════════════════════════
// 🔴`CCM_DATA_DIR` —— 那个目录此前没有任何出口
// ════════════════════════════════════════════════════════════════════════
//
// # 这一摞为什么不碰环境变量
//
// `lib_env_scrub_tests` 那条判据的头注逐字：
// 「**绝不能在测试里 set/remove 真实的 `CLAUDE_*` 变量（会干扰并发测试与宿主环境）**」
// —— cargo test 多线程跑、进程级 env 是共享的，而 `resolve_monitor_data_dir`
// 有 **8 处**消费者（凭据库 · 历史元数据 · 自启 · 全景 · config.json · 设置面板那一块 …）。
// ⇒ 本摞全部走 `monitor_data_dir_from(env_val, home)` 这个**纯**入口，一次 `set_var` 都没有。
//
// ⚠ 代价如实记：**「`resolve_monitor_data_dir` 真的去读那个 env」这一格本摞买不到**。
// 它由末尾那条源码判据（那一处 `env::var(DATA_DIR_ENV)` 恰好一处）顶着，
// 而那是**代理**不是行为。

fn home() -> std::path::PathBuf {
    std::path::PathBuf::from("/home/u")
}

/// 夹具里「一条绝对路径」**按平台取那一形**：`/tmp/iso-42` 在 Windows 上没有盘符，不是绝对路径
/// （按进程当前那一盘解）⇒ 拿它当绝对路径的夹具在那边会被**正确地**拒掉。
const ABS: &str = if cfg!(windows) {
    r"C:\iso-42"
} else {
    "/tmp/iso-42"
};

/// 没设 ⇒ 默认落点，而它就是设计写死的那一条。
#[test]
fn with_nothing_set_it_is_the_documented_default() {
    let got = monitor_data_dir_from(None, Some(home())).expect("有 home 却算不出落点");
    assert_eq!(got, std::path::PathBuf::from("/home/u/.cc-monitor"));
}

/// 给了一条绝对路径 ⇒ **原样用**，而且**不掺 home**。
///
/// 后一半要紧：掺了的话「隔离到临时目录」会变成「在临时目录下又建一层 .claude」。
#[test]
fn an_absolute_override_is_used_verbatim() {
    let got = monitor_data_dir_from(Some(ABS), Some(home())).expect("绝对路径被拒了");
    assert_eq!(got, std::path::PathBuf::from(ABS));
    // 🔴 连 home 都拿不到时它照样成立 —— 证明这条出口**不偷偷依赖 home**。
    assert_eq!(
        monitor_data_dir_from(Some(ABS), None),
        Some(std::path::PathBuf::from(ABS)),
        "没有 home 时那条绝对路径也被拒了 —— 那说明它偷偷拿 home 兜了一下"
    );
    // 周围空白不算内容。
    assert_eq!(
        monitor_data_dir_from(Some(&format!("  {ABS}  ")), Some(home())),
        Some(std::path::PathBuf::from(ABS))
    );
}

/// 设成空串 == 没设（shell 里 `CCM_DATA_DIR=` 是最常见的「取消」写法）。
#[test]
fn an_empty_value_means_unset_not_broken() {
    let def = monitor_data_dir_from(None, Some(home()));
    for empty in ["", "   ", "\t", "\n"] {
        assert_eq!(
            monitor_data_dir_from(Some(empty), Some(home())),
            def,
            "`{empty:?}` 没被当成「没设」"
        );
    }
}

/// 🔴🔴 **相对路径 ⇒ `None`，而不是退回用户真 profile。**
///
/// # 这一条是这整件事存在的理由
///
/// 退回真 profile 看起来「更稳」，实际是**反面**：那一趟自动化会以为自己被隔离了，
/// 而它正在写用户的东西 —— `config.json` · tab 集合名 · 固定了哪些 tab · 凭据库。
/// 理由是「**集合名是用户手写的真相，不是能重算的缓存**」。
///
/// 🔴 这一形 2026-09-21 在那台 Win11 虚拟机上**真发生过**：跑 tier-2 时
/// `auto-launch.json` 从 87 字节被改成 133 字节，那一路只能靠跑前备份、跑后还原
/// 才做到零残留 —— 而「靠每次记得备份」不是一个机制，是一次运气。
#[test]
fn a_relative_override_refuses_instead_of_quietly_using_the_real_profile() {
    let real = monitor_data_dir_from(None, Some(home()));
    // Windows 上还有两形也按进程当前那一盘解（与相对路径同一条理由拒）：有根没盘符（`/x` · `\x`）、有盘符没根（`C:x`）。
    // 同一个 `/x` 在 Linux 上是绝对路径、原样用 —— 那一半由 `ABS`（Linux 上就是这一形）钉着。
    let windows_relative: &[&str] = if cfg!(windows) {
        &["/x", r"\x", "C:x"]
    } else {
        &[]
    };
    for rel in ["iso", "./iso", "../iso", "a/b"]
        .iter()
        .chain(windows_relative)
        .copied()
    {
        let got = monitor_data_dir_from(Some(rel), Some(home()));
        assert_eq!(
            got, None,
            "`{rel}` 没被拒 —— 实得 {got:?}。\n\
             ★ 如果它等于默认落点，那正是这条出口存在要挡的那一形：\n\
               一趟以为自己被隔离了的自动化，正在写用户真 profile，而且没有一句话。"
        );
        assert_ne!(got, real, "`{rel}` 被静默退回了真 profile");
    }
    // 阴性对照：绝对路径**不**走这一支（少了这一半，上面可以靠「什么都拒」全绿）。
    assert!(monitor_data_dir_from(Some(ABS), Some(home())).is_some());
}

/// 连 home 都拿不到、又没给 env ⇒ `None`（老行为，一个字没改）。
#[test]
fn no_home_and_no_override_is_still_none() {
    assert_eq!(monitor_data_dir_from(None, None), None);
}

/// 🔴 **那个出口真的盖住了 monitor 这一侧的全部** —— monitor 经 `config.rs` 转交共享那一份规则、只读一处 env。
///
/// ⚠ 判源码是**代理**（同族先例 `transfer_tests::the_real_adapters_speak_only_through_the_channel`）。
/// 「没人自己拼那条路径」那一半挪进 [`the_data_dir_is_spelled_in_one_place`]（全 `src/` 生产段，不只 monitor 这棵树）。
#[test]
fn nothing_else_in_the_monitor_tree_builds_that_path_itself() {
    // 规则搬进共享 crate（`creds_core::store::monitor_data_dir`，远端常驻后端按同一份推默认路径）⇒
    //   `config.rs`（原 `paths.rs`）转交它。
    assert!(
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"))
            .contains("creds_core::store::monitor_data_dir("),
        "`config.rs`（原 `paths.rs`）不再转交共享那一份规则"
    );

    // 那个 env 名字**只在一处被读** —— 否则「读了哪个变量」会长出第二种答案。
    let me = guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"));
    let read = format!("env::var({})", "DATA_DIR_ENV");
    assert_eq!(
        me.matches(read.as_str()).count(),
        1,
        "`config.rs`（原 `paths.rs`）里读 `{}` 的地方不是恰好一处",
        crate::config::DATA_DIR_ENV
    );
    // 反空真：这把尺子认得出「不在」。
    assert!(!me.contains("env::var(DATA_DIR_ENV_THAT_DOES_NOT_EXIST)"));
}

/// 要求住址：「一台机器一个家 `~/.cc-monitor/`……**不写搬家代码、不认老路径**」· `INVARIANTS §2` 规矩 3「默认住址只在 `creds_core::store::monitor_data_dir` 拼」。
///
/// ① `src/` 生产段（`.rs` 剥掉测试段；`src/doc` 与 `.md` 散文、不进 git 的 `.cargo/` · `gen/` · `embedded-backends/` 不算）**零处**旧住址的名字；
/// ② 默认住址那个目录名在 `creds-core/src/store.rs` 生产段恰好一处，且就在 `monitor_data_dir` 里。
/// 正控：同一个判定认得出合成语料里的旧名。
#[test]
fn the_data_dir_is_spelled_in_one_place() {
    let old = ["claudecode", "frontend"].join("-");
    let spells_old = |rel: &str, text: &str| -> bool {
        let prod = if rel.ends_with(".rs") {
            guard_core::production_code(text)
        } else {
            text.to_string()
        };
        prod.contains(old.as_str())
    };
    let root = crate::guard_support::repo_src_root();
    let skip = [
        "doc/",
        "frontend/shell/.cargo/",
        "frontend/shell/gen/",
        "frontend/shell/embedded-backends/",
    ];
    let mut scanned = 0usize;
    let mut hits: Vec<String> = Vec::new();
    for rel in guard_core::files_under(&root) {
        if rel.ends_with(".md") || skip.iter().any(|p| rel.starts_with(p)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(root.join(&rel)) else {
            continue; // 二进制（图标之类）
        };
        scanned += 1;
        if spells_old(&rel, &text) {
            hits.push(rel);
        }
    }
    assert!(scanned > 500, "只扫到 {scanned} 份 —— 扫描面塌了");
    assert_eq!(
        hits,
        Vec::<String>::new(),
        "生产段里还有数据目录的旧住址（不认老路径）"
    );
    assert!(
        spells_old("x.ts", &format!("const d = '~/.claude/{old}';")),
        "正控：认不出旧名"
    );
    // 阴性对照：测试段里的旧名不算（夹具许写旧住址）。
    assert!(!spells_old(
        "x.rs",
        &format!("fn f() {{}}\n\n#[cfg(test)]\nmod tests {{\n    const D: &str = \"{old}\";\n}}\n")
    ));

    let store =
        guard_core::production_code(include_str!("../../../src/common/creds-core/src/store.rs"));
    let dir_lit = format!("{:?}", ".cc-monitor");
    assert_eq!(
        store.matches(dir_lit.as_str()).count(),
        1,
        "store.rs 里数据目录名不是恰好一处"
    );
    let at = guard_core::find_pinned(&store, "pub fn monitor_data_dir(")
        .expect("monitor_data_dir 不在了");
    let body = &store[at..at + store[at..].find("\n}\n").expect("没收尾")];
    assert!(
        body.contains(dir_lit.as_str()),
        "默认住址不在 monitor_data_dir 里拼：{body}"
    );
}
