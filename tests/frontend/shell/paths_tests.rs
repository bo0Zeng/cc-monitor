//! # 要求住址：`INVARIANTS §2`（含「唯一的明文例外：`CCM_DATA_DIR`」那一段）
//!
//! `with_nothing_set_it_is_the_documented_default` 点的是 `§2` 正文，逐字「monitor 自己的 data dir 永远是 `~/.cc-monitor/`」（V160）。
//! 其余几条判的是那句「永远」的**出口**（`config.rs::monitor_data_dir_from`），逐字点 `§2` 例外段的四条规矩：
//! 「只认**绝对路径**；空串 == 没设」·「给了但不合法（相对路径）⇒ **`None`，不退回用户真 profile**」·
//! 「全树只经 `config.rs::resolve_monitor_data_dir` 派生」。
//! 〔JA1 点址 2026-09-24：当时缺条、候选升格 · TL2 2026-09-25 补成 `§2` 的明文例外（IV1 报的第 3 件）〕

use super::*;

// ════════════════════════════════════════════════════════════════════════
// 🔴〔`P17` 2026-09-22〕`CCM_DATA_DIR` —— 那个目录此前没有任何出口
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
    let got = monitor_data_dir_from(Some("/tmp/iso-42"), Some(home())).expect("绝对路径被拒了");
    assert_eq!(got, std::path::PathBuf::from("/tmp/iso-42"));
    // 🔴 连 home 都拿不到时它照样成立 —— 证明这条出口**不偷偷依赖 home**。
    assert_eq!(
        monitor_data_dir_from(Some("/tmp/iso-42"), None),
        Some(std::path::PathBuf::from("/tmp/iso-42")),
        "没有 home 时那条绝对路径也被拒了 —— 那说明它偷偷拿 home 兜了一下"
    );
    // 周围空白不算内容。
    assert_eq!(
        monitor_data_dir_from(Some("  /tmp/iso-42  "), Some(home())),
        Some(std::path::PathBuf::from("/tmp/iso-42"))
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
/// 而 `设计/30 §B.4` 逐字的理由是「**集合名是用户手写的真相，不是能重算的缓存**」。
///
/// 🔴 这一形 2026-09-21 在那台 Win11 虚拟机上**真发生过**：跑 tier-2 时
/// `auto-launch.json` 从 87 字节被改成 133 字节，那一路只能靠跑前备份、跑后还原
/// 才做到零残留 —— 而「靠每次记得备份」不是一个机制，是一次运气。
#[test]
fn a_relative_override_refuses_instead_of_quietly_using_the_real_profile() {
    let real = monitor_data_dir_from(None, Some(home()));
    for rel in ["iso", "./iso", "../iso", "a/b"] {
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
    assert!(monitor_data_dir_from(Some("/x"), Some(home())).is_some());
}

/// 连 home 都拿不到、又没给 env ⇒ `None`（老行为，一个字没改）。
#[test]
fn no_home_and_no_override_is_still_none() {
    assert_eq!(monitor_data_dir_from(None, None), None);
}

/// 🔴 **那个出口真的盖住了 monitor 这一侧的全部** —— 没人自己拼那条路径。
///
/// ⚠ 判源码是**代理**（同族先例 `transfer_tests::the_real_adapters_speak_only_through_the_channel`）。
/// 买的是：`CCM_DATA_DIR` 一设，monitor 这棵树上**所有**落点一起挪
/// —— 而不是挪了八分之七。
///
/// ⚠ **射程边界，如实登记（两条都不在本条管辖内）**：
/// ① `src/common/creds-core/src/store.rs` 自己拼 `claudecode-frontend`
///    ——它吃一个传进来的 `home`，是**共享 crate**，monitor 与后端都用；
///    monitor 这侧经 `creds_store.rs` → `resolve_monitor_data_dir` 进来 ⇒ 被盖住；
///    后端那侧走它自己的 env（`src/backend/accounts/creds.rs` 头注逐字
///    「env 覆盖优先」）⇒ **两个出口，各管一半**，本条不合并它们。
/// ② `src/backend` 整棵树不在本条射程里（另一个 workspace）。
#[test]
fn nothing_else_in_the_monitor_tree_builds_that_path_itself() {
    let root = crate::guard_support::repo_src_root().join("frontend/shell/src");
    let mut builders: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    let needle = format!("join({:?})", "claudecode-frontend");
    for f in guard_core::files_by_extension(&root, "rs") {
        let path = root.join(&f);
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        scanned += 1;
        let prod = guard_core::production_code(&src);
        if prod.contains(needle.as_str()) {
            builders.push(f);
        }
    }
    // 反空真：扫描面没塌。
    assert!(
        scanned > 50,
        "只扫到 {scanned} 份 `.rs` —— 扫描面塌了，下面那一比会空真地绿"
    );
    // 〔TAIL〕规则搬进共享 crate（`creds_core::store::monitor_data_dir`，远端常驻后端按同一份推默认路径）⇒
    //   monitor 这棵树里一处都不该自己拼；`config.rs`（原 `paths.rs`）转交它（正控，防「一处都没扫到」的空真）。
    assert!(
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/config.rs"))
            .contains("creds_core::store::monitor_data_dir("),
        "`config.rs`（原 `paths.rs`）不再转交共享那一份规则"
    );
    assert_eq!(
        builders,
        Vec::<String>::new(),
        "monitor 这棵树里有地方自己拼那条路径（规则只该住 `creds_core::store::monitor_data_dir`）。\n\
         ★ 多一处就意味着 `CCM_DATA_DIR` 盖不住它 ⇒ 一趟自以为被隔离的跑\n\
           仍然会往用户真 profile 里写那一样东西。\n\
         ⇒ 处置：让它经 `config::resolve_monitor_data_dir()` 派生。"
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
