//! 路由：从请求路径里切出路由键，把其余部分原样交给上游。
//!
//! 形状 `/<前缀>/<seg1>/<seg2>/<真路径>` —— 两段都是不透明串，中转不认识任何 agent 叫什么。流标签取自请求头（`server.rs::stream_label`），
//! 会话 id 归 agent 自己。
//!
//! # 两个前缀 = 两种模式
//!
//! ```text
//! /s/…  代入模式：表里必须有这一行，没有 ⇒ 404
//! /t/…  直通模式：中转永不代入 auth；`seg2` 只当标签
//! ```
//!
//! 意图写在线上，两条路不可能互相静默降级：一个前缀 ＋「查不到就直通」的话，一次 `seg2` 打字错误就会从「该代入却没代入（loud 404）」
//! 变成「静默用了下游自己的凭据」。两个前缀让这件事构造上不可能：`/s/` 永远 fail-closed，`/t/` 从来不代入。
//! `/t/` 的流量挂在全量注入开关后面（默认开）。注入哪个地址由上游选择 `accounts/upstream_select/endpoint.rs` 拼（`relay_route_core::base_url`，与本文件切的是同一份语法）。
//!
//! # `<account>` 那一段
//!
//! 路由键带账号那一段（上游端点按 (agent, 账号) 查表）；不复用会话那一段：一个值装会话 id 与账号 id 两件事，代价是将来才发作、发作时找不回原因。
//! `<account>` 段本身不保证那个账号存在 —— 它只保证形状对得上；「表里有没有这一行」是上游选择的活，查不到就是 404
//! （不许回落到别的账号的 key，也不许回落到默认上游）。

/// 切出来的路由。`rest` 逐字保留原请求的 `路径 + 查询串`，中转不重写它。
///
/// # 🔴 这几个字段**用位置名，不用业务名**
///
/// 先前它是 `{ agent, account, key }` 三个业务名。今天是
/// `{ mode, key: RouteKey{seg1,seg2}, rest }` —— 中转只知道「第 1/2 段」，整包交给上游选择当键
/// （第 3 段退役）。
/// 谁是 agent、谁是账号，**只在 `accounts/` 那一层才有这两个词**。
#[derive(Debug, PartialEq, Eq)]
pub struct Route {
    /// 哪个前缀进来的 —— `/s/` 代入 · `/t/` 直通。中转只转交，不解释。
    pub mode: super::Mode,
    /// 前两段，整包交给上游选择当键。中转**不解释**它们。
    pub key: super::RouteKey,
    /// 紧跟第 2 段的来处段（会话血缘，`relay_route_core::Origin`）；没有 ⇒ `None`。不进路由键、不交上游，原样递给上游选择。
    pub origin: Option<super::RouteOrigin>,
    pub rest: String,
}

/// 一段路由键里允许的字符 —— 白名单，不是黑名单（ASCII 字母数字与 `-` `_`，1..=128 字节）。
///
/// 收窄到这几类是**故意的**：路由键要参与日志与 tee 行，放开任意字节等于给
/// 「把控制字符 / 换行塞进 tee 流」开一条路。`.` 与 `/` **不在**白名单里 ⇒ `..` 这种段根本构造不出来。
///
/// ★ 同一条性质只许有一个实现：装路由表时判「这个账号 id 当得了路由段吗」与本函数问的是同一个问题。
/// 它与两个前缀、拼 / 拆路由一起住共享 crate `relay_route_core`（目标）：
/// monitor 那一侧（起会话身份 token · 载荷里中转地址的校验）与这里 `use` 同一份，先前的「两侧各写一份、样例对拍」退役。
/// 那个 crate 不是业务 crate（没有账号 / 凭据的名字），本文件是通信层成员也可以依赖它（`C2`）。
pub use relay_route_core::segment_is_safe;

/// 解析 `/<前缀>/<seg1>/<seg2>/<rest>`。不是这个形状就返回 `None`（调用方回 404）。
/// 前缀闭集与切法住 `relay_route_core::parse_target`（唯一住址）；本函数只把共享 crate 的模式
/// 换成中转自己的契约类型 `super::Mode`（上游选择收的是它）。
pub fn parse(target: &str) -> Option<Route> {
    let p = relay_route_core::parse_target(target)?;
    Some(Route {
        mode: match p.mode {
            relay_route_core::RouteMode::Substitute => super::Mode::Substitute,
            relay_route_core::RouteMode::Passthrough => super::Mode::Passthrough,
        },
        key: super::RouteKey {
            seg1: p.seg1.to_string(),
            seg2: p.seg2.to_string(),
        },
        origin: p.origin.map(|o| super::RouteOrigin {
            token: o.token.to_string(),
            parent: o.parent.map(str::to_string),
        }),
        rest: format!("/{}", p.rest),
    })
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/route_tests.rs"]
mod tests;
