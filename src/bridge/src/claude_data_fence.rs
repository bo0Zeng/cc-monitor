//! **哪些路径是 Claude 自己的数据，不许我们写。**
//! 〔`设计/99 §2 Q2`，用户 2026-09-21 逐字裁「拆」〕
//!
//! # 本模块只有这一件事
//!
//! 一个判定（[`is_protected_claude_data_path`]）＋ 它的拒绝（[`guard_write`]）。
//! 没有别的。它的语料是**一个路径字符串**，没有连接、没有 IO、没有 `async`
//! ⇒ 「这条路径被挡住」这件事在一台**没有任何连接**的机器上判得动，
//! 而 `guard_write` 的 `Err` 是在任何一次往返**之前**就出来的。
//!
//! 那个「之前」不是散文：`remote_write_registry_tests` 的
//! `a_fenced_write_refuses_before_it_touches_the_wire` 逐条要求池子里那七条写命令
//! 的函数体里 `guard_write` 出现在拿连接（`pool_for` / 借通道）**之前**；
//! 本模块「够不着线」这件事由 `claude_data_fence_tests` 的
//! `the_fence_cannot_reach_the_wire` 从另一侧钉着（本文件生产段里零传输符号）。
//!
//! # 它是 `src/doc/INVARIANTS.md` `§1` 那两段澄清的**共同**执行体
//!
//! `§1` 是「monitor 零侵入 Claude Code 数据源」。它底下**两段澄清**各自逐字点名本判定：
//!
//! | 澄清段 | 谁在用 | 用法 |
//! |---|---|---|
//! | **F47**（SFTP 文件面板） | `sftp_pool` 那七条写命令 · `filewin::writeops::fenced_path` | 面板/窗口写**任意用户选的路径** ⇒ 拒碰 Claude 的 jsonl/pidfile |
//! | **F03b**（收件箱编辑） | `skill_host::resolve_editable` 的第②道 | **纵深**：即使声明表写歪了，也不许碰 Claude 的数据 |
//!
//! ⇒ 两段澄清共用**一个**判定，这是它有独立住址的第一个理由：在它搬出来之前，
//! 一个写**本机** `INBOX.txt` 的模块要伸手到一个名叫「SFTP 连接池」的模块里，
//! 去问一个关于**本机路径**的问题。
//!
//! ⚠ `§1` 的**内容**本轮一个字都没动（那要用户拍）——
//! 动的只有它写的**住址**：原先写 `sftp_pool::is_protected_claude_data_path`，
//! 现在写本模块。
//!
//! # 🔴 判定本身一个字节都没改
//!
//! [`is_protected_claude_data_path`] 的函数体是从 `sftp_pool` **原样**搬过来的
//! （结构判定：`<任意>/projects/<proj>/<sid>.jsonl` 恰 2 段 · `<任意>/sessions/<x>.json`
//! 恰 1 段；batch20 那次审计修闭的 `CLAUDE_CONFIG_DIR` 重定位缺口也原样在内）。
//! [`guard_write`] 的拒绝原话同样原样。⇒ **本轮变的是它住哪、谁能看见它、谁在数它**，
//! 不是它判什么。想改判定的射程，那是另一件、要用户拍。
//!
//! # ⚠ 它**不**管什么（只登记，不扩射程）
//!
//! - **方向相反的那一道不在这儿**：从前是 `sftp` 里的 `is_safe_remote_jsonl`〔散文墓碑〕，
//!   〔RW1 · 第四波 09-24〕今天住后端 `agents::claudecode::paths::session_file_for_delete_in`，正题恰恰是
//!   「**只许**删恰是 `projects/<proj>/<sid>.jsonl` 的那一份」（`§1` 例外 3，历史浏览器删会话）。
//!   两道都读 Claude 的目录结构、方向相反，**别互相替代、也别合并**。
//!   「全仓还有谁在读这个结构」由 `claude_data_fence_tests` 的
//!   `the_protected_path_judgement_has_exactly_one_home` 钉成相等断言。
//! - **本判定只看路径的形状**，不看那场会话是不是真的活着（要那个得问
//!   `session_map`，而面板上那一问会变成一次网络往返 ⇒ 刻意不要）。
//! - 它管不着的那几类路径**如实登记在**
//!   `claude_data_fence_tests::THE_SHAPES_THIS_FENCE_DOES_NOT_COVER` 里
//!   —— 那张表是**读数**，不是待办：往里加一类就是扩射程，要用户拍。

/// F47 / F03b 防误伤守卫：该路径是否 Claude 数据源文件（jsonl / pidfile）。
///
/// 写命令拒碰这些——往正被 Claude 打开的会话文件写会损坏会话；要管这些用历史浏览器
/// （`§1` 例外 3 的 `remote_history::delete_remote_history_session`，带二次确认），
/// 不走文件面板。**结构判定**（与从前那道 `is_safe_remote_jsonl`〔散文墓碑〕同风格，batch20 起
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

#[cfg(test)]
#[path = "../../../tests/bridge/claude_data_fence_tests.rs"]
mod tests;
