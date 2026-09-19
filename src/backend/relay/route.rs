//! 路由：从请求路径里切出**路由键**，把其余部分原样交给上游。
//!
//! 形状 `/s/<agent>/<account>/<key>/<真路径>` —— 三段都是**不透明串**。
//! 中转不认识任何 agent 叫什么，也不解释 `<key>` 是会话 id 还是别的什么。
//!
//! # ⚠ `<account>` 那一段是 `K-H2` 加的，理由与代价逐条记这里
//!
//! `K11 裁定一`（现行版）那张表逐字写着上游端点归「中转的路由表
//! （**路由键带账号那一段**）」——**而在 `K-H2` 之前，路由键里没有那一段**：
//! `<agent>` 是 `K9` 裁定二要的「agent-aware 的路由键」，`<key>` 是会话 id 那一类。
//! ⇒ 加这一段是**照裁定做**，不是发明一个新维度。
//!
//! **为什么不复用 `<key>` 段**（`K-H2` `Bx` 判的，PM 08-28 采纳）：
//! 那会让 `<key>` 同时装会话 id 与账号 id ——**一个值装了两件事**，
//! 本工作区最贵的那族病。它今天不疼（tee 那条流零消费者），
//! 而它的代价是**将来才发作、发作时找不回原因**的一次回退。
//! 加一段的代价是**今天可数的**：本文件的 `parse` · 本文件的判据 ·
//! `src/doc/IPC-PROTOCOL.md` 那一行 · tee 的行契约。
//!
//! ⚠ **`<account>` 段本身不保证那个账号存在** —— 它只保证「形状对得上」。
//! 「表里有没有这一行」是 `relay::table` 的活，查不到就是 **404**
//! （`K-H2` `KH2`：不许回落到别的账号的 key，也不许回落到默认上游）。

/// 切出来的路由。`rest` 逐字保留原请求的 `路径 + 查询串`，中转不重写它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) agent: String,
    /// 路由表的**索引键**（`K-H2`）。中转不解释它，只拿它去查表。
    pub(crate) account: String,
    pub(crate) key: String,
    pub(crate) rest: String,
}

/// 一段路由键里允许的字符 —— 白名单，不是黑名单。
///
/// 收窄到这几类是**故意的**：路由键要参与日志与 tee 行，放开任意字节等于给
/// 「把控制字符 / 换行塞进 tee 流」开一条路。`.` 与 `/` **不在**白名单里 ⇒
/// `..` 这种段根本构造不出来。
///
/// ★ 它是 `pub(crate)` 的，理由是**同一条性质只许有一个实现**〔`K-H2`〕：
/// 装路由表的时候要判「这个账号 id 当得了路由段吗」，那与本函数问的是
/// **同一个问题**。两边各写一份，漂开的那天没有任何东西会说，
/// 而症状是「文件里配了一条账号，中转永远 404」这种查不出来的形状。
pub(crate) fn segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 解析 `/s/<agent>/<account>/<key>/<rest>`。不是这个形状就返回 `None`（调用方回 404）。
pub(crate) fn parse(target: &str) -> Option<Route> {
    let after = target.strip_prefix("/s/")?;
    let (agent, after) = after.split_once('/')?;
    let (account, after) = after.split_once('/')?;
    let (key, rest) = after.split_once('/')?;
    if !segment_is_safe(agent) || !segment_is_safe(account) || !segment_is_safe(key) {
        return None;
    }
    Some(Route {
        agent: agent.to_string(),
        account: account.to_string(),
        key: key.to_string(),
        rest: format!("/{rest}"),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/route_tests.rs"]
mod tests;
