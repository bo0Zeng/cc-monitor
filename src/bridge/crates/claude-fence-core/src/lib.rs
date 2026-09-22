//! **哪些路径是 Claude 自己的数据，不许我们写** —— 那一个判定 ＋ 它的拒绝。
//!
//! # 🔴 它为什么是一个**共享 crate**〔2026-09-22〕
//!
//! 用户 2026-09-22 逐字裁定「**允许**」后端在用户显式操作下写用户自己选的路径
//! （`设计/60 §8.3` 那道政策题）。⇒ 那道围栏从此要由**拥有那份数据的那台机器**执行
//! —— 远端的 Claude 数据在远端，只有远端那个后端够得着它。
//!
//! 而 `claude_data_fence`（monitor 侧）自己的头注立着一条纪律逐字：
//! 「**判定与拒绝这一对不许再分**」。两棵树各写一份就是两个家
//! ⇒ 判定搬进共享 crate，**两侧引同一个**。
//!
//! ⚠ **函数体一个字节都没改** —— 从 `src/bridge/src/claude_data_fence.rs` 原样搬来。
//! 本轮变的是它住哪、谁够得着它，不是它判什么。想改判定的射程，那是另一件、要用户拍。
//!
//! # ⚠ 它**够不着线**，而这一条由依赖表担保
//!
//! 本 crate **零依赖**：语料是一个路径字符串，没有连接、没有 IO、没有 `async`。
//! ⇒ 「`guard_write` 的 `Err` 在任何一次往返**之前**就出来」这件事，
//! 在这一层是**构造上**的，不靠判据去扫。
//!
//! ⚠ 那两条各自的射程与「它管不着什么」照旧住调用方那一侧的头注与判据
//! （monitor：`claude_data_fence` 模块；判据：`tests/bridge/claude_data_fence_tests.rs`）。

/// F47 / F03b 防误伤守卫：该路径是否 Claude 数据源文件（jsonl / pidfile）。
///
/// 写命令拒碰这些——往正被 Claude 打开的会话文件写会损坏会话；要管这些用历史浏览器
/// （`§1` 例外 3 的 `remote_history::delete_remote_history_session`，带二次确认），
/// 不走文件面板。**结构判定**（与 `sftp::is_safe_remote_jsonl` 同风格，batch20 起
/// 不靠 `.claude` 字面，闭 `CLAUDE_CONFIG_DIR` 缺口）。
pub fn is_protected_claude_data_path(path: &str) -> bool {
    let p = path.replace('\\', "/");
    // batch20 审计修：**结构判定**，不靠 `/.claude/` 字面——Claude 数据文件结构为 `<任意>/projects/<proj>/<sid>.jsonl`
    // （projects 下恰 2 段）或 `<任意>/sessions/<x>.json`（sessions 下 1 段）。**闭 `CLAUDE_CONFIG_DIR` 重定位缺口**：
    // 重定位后路径成 `<CFGDIR>/projects/.../*.jsonl`，原字面 `/.claude/` 判定会漏、SFTP 面板可覆写 live jsonl。
    let jsonl_protected = p.rfind("/projects/").is_some_and(|i| {
        let parts: Vec<&str> = p[i + "/projects/".len()..].split('/').collect();
        parts.len() == 2
            && !parts[0].is_empty()
            && parts[1].len() > ".jsonl".len()
            && parts[1].ends_with(".jsonl")
    });
    let json_protected = p.rfind("/sessions/").is_some_and(|i| {
        let rest = &p[i + "/sessions/".len()..];
        !rest.contains('/') && rest.len() > ".json".len() && rest.ends_with(".json")
    });
    jsonl_protected || json_protected
}

/// 拒 Claude 数据源路径的写守卫（返回 `Err` 便于 `?`）。
///
/// 🔴 **这是围栏的另一半，刻意与判定同住** —— 判定说「是不是」，它说「那就不做，
/// 并且这么告诉人」。分居两处的代价是具体的：拒绝的原话会长出第二份
/// （`filewin::writeops::fence_notice` 那一份是**刻意**的第二份文案，
/// 因为窗口那一层要自己说话；但**判定与拒绝**这一对不许再分）。
pub fn guard_write(path: &str) -> Result<(), String> {
    if is_protected_claude_data_path(path) {
        return Err(format!(
            "拒绝写 Claude 数据源文件({path})——管理会话文件请用历史浏览器"
        ));
    }
    Ok(())
}
