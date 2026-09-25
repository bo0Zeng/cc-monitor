//! 〔SR1a · 2026-09-24〕**按拨号身份复用 SSH 连接** —— 「本机只常驻一个后端、所有 SSH 连接由它复用」的那个「复用」。
//!
//! # 身份是什么
//!
//! 拨号请求里**决定「连到哪、以谁的身份」**的那几项：`host · port · user · key_path · host_key_fingerprint ·
//! 竞速地址 · 跳板`（[`identity`]）。用法（`use`）· 命令 · 阶段行 · `agent_sock` 都**不在**身份里 ——
//! 同一台远端的长流与一次性查询正是要共用一条连接。配置一改（换了钥匙 / 指纹固化了 / 换了跳板）就是
//! 另一个身份、另一条连接；旧的那条随用它的最后一条链路一起收掉。
//!
//! # 活多久：**没有定时器**
//!
//! 表里存 `Weak`，链路手里拿 `Arc` ⇒ **最后一条链路走了，连接就断**（`Arc` 落零 ⇒ russh 句柄 drop）。
//! 「空闲多久再关」要一个会自己醒的构件，本 crate 不许有（`no_timer_guard`）。代价如实写：
//! 远端长流断了之后，下一次一次性查询要重新握手（长流在的时候它们全都复用那一条）。
//!
//! # 同一身份并发来拨
//!
//! 按身份串行：每个身份一把异步锁，第二个来的等第一个拨完、直接复用它拨出来的那条。
//! 黑洞地址的握手可以很久 —— 等着的那一个由界面侧的握手期限兜（到点它会 `link-close`，
//! 这条链路的任务被 abort，锁随之放开）。
//!
//! # 复用前问一句还活着没有
//!
//! `is_closed()` 为真 ⇒ 当它不在，重拨。「看着活着、开 channel 却失败」那一形由调用方处理
//! （`uses::Lease::session_channel`：摘掉 → 重拨一次）—— 那是**同一机制换一条新连接**，不是换一条路。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::connect::Linked;
use super::DialRequest;

/// 池里的一条连接要回答的唯一一问。**抽成 trait 是为了判据**：池的记账（复用 / 串行 / 收掉）
/// 不需要真 SSH 就验得动（`dial_pool_tests` 拿一个假连接喂它）。
pub(crate) trait Conn: Send + Sync + 'static {
    fn is_closed(&self) -> bool;
}

impl Conn for Linked {
    fn is_closed(&self) -> bool {
        self.session.is_closed()
    }
}

type Slot<C> = Arc<tokio::sync::Mutex<Weak<C>>>;

/// 连接池。键 = [`identity`]。
pub(crate) struct Pool<C> {
    slots: Mutex<HashMap<String, Slot<C>>>,
}

impl<C: Conn> Pool<C> {
    pub(crate) fn new() -> Self {
        Pool {
            slots: Mutex::new(HashMap::new()),
        }
    }

    /// 取一条：同身份且还活着 ⇒ 复用（第二个值 `true`）；否则 `dial` 一条新的、记下（`false`）。
    pub(crate) async fn get<F, Fut, E>(&self, key: &str, dial: F) -> Result<(Arc<C>, bool), E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<C, E>>,
    {
        let slot = {
            let mut g = self.slots.lock().unwrap_or_else(|e| e.into_inner());
            // 顺手清掉已经没人用的格子（**只在有人来取的时候**扫一次 —— 不要定时器）。
            // 正被人拿着锁的格子（正在拨）一律留着。
            g.retain(|_, s| s.try_lock().map(|w| w.strong_count() > 0).unwrap_or(true));
            Arc::clone(g.entry(key.to_string()).or_default())
        };
        let mut held = slot.lock().await;
        if let Some(c) = held.upgrade() {
            if !c.is_closed() {
                return Ok((c, true));
            }
        }
        let c = Arc::new(dial().await?);
        *held = Arc::downgrade(&c);
        Ok((c, false))
    }

    /// 这一条不能用了（开 channel 失败）⇒ 从池里摘掉。**只摘还指着它的那一格**：
    /// 别人可能已经换上了一条新的，那一条不许被这一次摘掉。
    pub(crate) fn evict(&self, key: &str, c: &Arc<C>) {
        let g = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(slot) = g.get(key) {
            if let Ok(mut w) = slot.try_lock() {
                if w.upgrade().is_some_and(|cur| Arc::ptr_eq(&cur, c)) {
                    *w = Weak::new();
                }
            }
        }
    }

    /// 此刻还活着（有人在用）的连接数。
    pub(crate) fn live(&self) -> usize {
        let g = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        g.values()
            .filter(|s| s.try_lock().map(|w| w.strong_count() > 0).unwrap_or(true))
            .count()
    }
}

/// 本进程那一个 SSH 连接池。
pub(crate) fn ssh() -> &'static Pool<Linked> {
    static P: OnceLock<Pool<Linked>> = OnceLock::new();
    P.get_or_init(Pool::new)
}

/// 一份拨号请求的**连接身份**（规范化 JSON 串，键序固定）。
pub(crate) fn identity(req: &DialRequest) -> String {
    let endpoints: Vec<serde_json::Value> = req
        .race_order()
        .iter()
        .map(|e| serde_json::json!([e.host, e.port]))
        .collect();
    let jump = req
        .jump
        .as_ref()
        .map(|j| serde_json::json!([j.host, j.port, j.user, j.key_path, j.host_key_fingerprint]));
    serde_json::json!([
        req.host,
        req.port,
        req.user,
        req.key_path,
        req.host_key_fingerprint,
        endpoints,
        jump
    ])
    .to_string()
}

#[cfg(test)]
#[path = "../../../tests/backend/dial_pool_tests.rs"]
mod tests;
