//! 中转去上游那一跳的读数：这个中转起来以后的合计 —— 上次连的上游（主机:口）· 那条 TCP 经没经代理（出网口开它那一刻说的）·
//! 线上进出字节（TLS 之下那条 TCP 上数，[`crate::upstream::Metered`]）· 上次连上了还是卡在哪一步（[`FailedAt`]）。
//!
//! 记点两处：连上 ＝ 读到最终回包头（`server::read_final_head`）；没连上 ＝ 四种传输失败都经过的那一处（`server::respond_upstream_failed`）。
//! 那一句（进本机那台复制详情末尾「连接：」那一段，同 SSH 那几跳一形）也住这里（[`HopFacts::line`]）；归到哪台是宿主的事。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use crate::server::FailedAt;

/// 上次那一发的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Last {
    /// 读到了上游的最终回包头。
    Up,
    /// 卡在这一步（连接 · 发请求 · 等响应 · 读响应）。
    Failed(FailedAt),
}

/// 此刻的读数（[`HopMeter::facts`] 出的那一份）。一发都没往上游走过 ⇒ 全是缺省。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HopFacts {
    /// 上次连的上游（`主机:口`，不带路径前缀）。
    pub endpoint: Option<String>,
    /// 上次开出的那条 TCP 经没经代理。
    pub via_proxy: bool,
    /// 线上收到的字节（合计）。
    pub bytes_in: u64,
    /// 线上发出的字节（合计）。
    pub bytes_out: u64,
    pub last: Option<Last>,
}

impl HopFacts {
    /// 那一句（「中转 上游〔· 经代理〕· 收 · 发〔· 上次卡在…〕」）；一发都没往上游走过 ⇒ 没有。
    pub fn line(&self) -> Option<String> {
        use copy_core::{copy_text, size_text};
        let endpoint = self.endpoint.as_ref()?;
        let place = if self.via_proxy {
            copy_text("beLinks.route.proxy", &[("endpoint", endpoint)])
        } else {
            endpoint.clone()
        };
        let line = copy_text(
            "beLinks.relay.line",
            &[
                ("endpoint", &place),
                ("in", &size_text(self.bytes_in)),
                ("out", &size_text(self.bytes_out)),
            ],
        );
        Some(match self.last {
            Some(Last::Failed(at)) => copy_text(
                "beLinks.relay.failed",
                &[("line", &line), ("hop", at.words().1)],
            ),
            Some(Last::Up) | None => line,
        })
    }
}

/// 一个中转的那一份记账（`Relay` 构造时起一份，各条上游连接共用）。
#[derive(Debug, Default)]
pub struct HopMeter {
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    seen: Mutex<Seen>,
}

#[derive(Debug, Default)]
struct Seen {
    endpoint: Option<String>,
    via_proxy: bool,
    last: Option<Last>,
}

impl HopMeter {
    fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 线上收到 `n` 字节。
    pub(crate) fn heard(&self, n: usize) {
        self.bytes_in.fetch_add(n as u64, Ordering::Relaxed);
    }

    /// 线上发出 `n` 字节。
    pub(crate) fn sent(&self, n: usize) {
        self.bytes_out.fetch_add(n as u64, Ordering::Relaxed);
    }

    /// 出网口刚开出一条（经没经代理是它说的）。
    pub(crate) fn opened(&self, via_proxy: bool) {
        self.seen().via_proxy = via_proxy;
    }

    /// 这一发在 `endpoint` 那条上读到了最终回包头。
    pub(crate) fn answered(&self, endpoint: String, via_proxy: bool) {
        let mut s = self.seen();
        s.endpoint = Some(endpoint);
        s.via_proxy = via_proxy;
        s.last = Some(Last::Up);
    }

    /// 这一发到 `endpoint` 卡在 `at`。
    pub(crate) fn failed(&self, endpoint: String, at: FailedAt) {
        let mut s = self.seen();
        s.endpoint = Some(endpoint);
        s.last = Some(Last::Failed(at));
    }

    /// 此刻的读数。
    pub fn facts(&self) -> HopFacts {
        let s = self.seen();
        HopFacts {
            endpoint: s.endpoint.clone(),
            via_proxy: s.via_proxy,
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            last: s.last,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/comms/outward/hop_tests.rs"]
mod tests;
