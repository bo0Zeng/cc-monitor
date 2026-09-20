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
///
/// # 🔴 条 48（2026-09-18 拍板 (a)）：这几个字段**用位置名，不用业务名**
///
/// 先前它是 `{ agent, account, key }` 三个业务名。今天是
/// `{ mode, key: RouteKey{seg1,seg2}, stream }` —— 层 1 只知道「第 1/2/3 段」，
/// 把前两段整包交给层 2 当键、把第 3 段当自己那条流的名字。
/// 谁是 agent、谁是账号，**只在 `accounts/` 那一层才有这两个词**。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Route {
    /// 哪个前缀进来的 —— `/s/` 代入 · `/t/` 直通。层 1 只转交，不解释。
    pub(crate) mode: super::Mode,
    /// 前两段，整包交给层 2 当键。层 1 **不解释**它们。
    pub(crate) key: super::RouteKey,
    /// 第 3 段 —— 层 1 自己那条流的名字（它是 sid，但层 1 不需要知道）。
    pub(crate) stream: String,
    pub(crate) rest: String,
}

impl Route {
    /// 这一条请求在 tee 上的身份。**三个标签收成一个**（`20 §4`）。
    pub(crate) fn stream_id(&self) -> super::StreamId<'_> {
        super::StreamId {
            key: &self.key,
            stream: &self.stream,
        }
    }
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

/// 解析 `/s/<seg1>/<seg2>/<seg3>/<rest>`。不是这个形状就返回 `None`（调用方回 404）。
///
/// ⚠ 四个槽位**一格没动**（`20 §0`：线格式本来就是对的），动的只是层 1 怎么称呼它们。
/// ⇒ `segment_is_safe` 一字不动，`parse` 只是把切出来的三段装进新名字。
pub(crate) fn parse(target: &str) -> Option<Route> {
    let after = target.strip_prefix("/s/")?;
    let mode = super::Mode::Substitute;
    let (seg1, after) = after.split_once('/')?;
    let (seg2, after) = after.split_once('/')?;
    let (seg3, rest) = after.split_once('/')?;
    if !segment_is_safe(seg1) || !segment_is_safe(seg2) || !segment_is_safe(seg3) {
        return None;
    }
    Some(Route {
        mode,
        key: super::RouteKey {
            seg1: seg1.to_string(),
            seg2: seg2.to_string(),
        },
        stream: seg3.to_string(),
        rest: format!("/{rest}"),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/route_tests.rs"]
mod tests;
