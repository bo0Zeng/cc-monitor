//! 〔TAP · V124〕**tee 的消费侧（monitor 这一半）**：本机常驻后端发来的 `tap` 帧，原样交给会话流的句柄
//! （`event_replay::EventReplay::on_tap`），前端经通道 `subscribe(origin, "session-tap")` 按 credit 收（`设计/05 §15`）。
//!
//! # 它只做一件事：转交，零解释（设计住仓外 `调研/第四波记录/TAP.md §1.2`；出处 `设计/20 §8`）
//!
//! - 读循环（`backend::control::local_backend::absorb_local_frame`）手里没有 `AppHandle` ⇒ 与 [`crate::session_facts`] 同形：
//!   出口由 `lib.rs` 的 setup 装一次（[`install_sink`]），交来的每一帧交给出口；装之前来的直接丢（前端那时也还没在听）。
//! - **不进任何缓冲**：不进 `local_lines`（内容通道）、不进重放缓冲（`EventReplay.history`）、不攒 —— 收一帧，
//!   交给此刻订了 `session-tap` 的订阅（有 credit 当场交，没 credit 丢、位置照占、原位 `Gap`：`05 §3.3.4` 级 2）。
//!   ⇒ SSE 那一路断 / 丢 / 挤都碰不到 jsonl 那条对的路（V24：SSE 只保快，落盘保对）。
//! - **不认识 SSE**：`data` 原样交前端（前端的活卡状态机认 `message_start` / `content_block_delta` …）；
//!   `stream` 原样交（前端拿它对 tab 的 sid）；缺口由前端看 `n` 算（`设计/05 §3.3.4`：检漏纯算术 · 判可恢复归上层）。
//! - 〔HOST · V139〕本机那条流（中转住本机常驻后端，V107）与远端那条流（远端中转住远端常驻后端）都有它，origin 各是各的。

use std::sync::OnceLock;

/// 一个 `tap` 帧（`ssh_source::InboundFrame::Tap` 的载荷）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tap {
    pub stream: String,
    pub resp: u64,
    pub n: u64,
    pub body: TapBody,
}

/// `tap` 帧的两形：一个 SSE 事件原文 · 这个响应收尾了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TapBody {
    Data(String),
    End(TapEnd),
}

/// 后端 `wire::TapEnd` 的两个线上字面量（**双写点**：后端那侧由 `wire_tests::tap_frames_have_exactly_these_bytes`
/// 钉精确字节，这一侧由 `ssh_source_parse_frame_tests` 用同一串帧喂 `parse_frame` 钉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapEnd {
    Done,
    Broken,
}

impl TapEnd {
    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "done" => Some(TapEnd::Done),
            "broken" => Some(TapEnd::Broken),
            _ => None,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            TapEnd::Done => "done",
            TapEnd::Broken => "broken",
        }
    }
}

type Sink = Box<dyn Fn(crate::bridge::SessionTapPayload) + Send + Sync>;

fn sink() -> &'static OnceLock<Sink> {
    static SINK: OnceLock<Sink> = OnceLock::new();
    &SINK
}

/// 装出口。**只装一次**（第二次装被忽略并记一句 —— setup 只跑一次，多装一定是接线错了）。
pub fn install_sink(f: impl Fn(crate::bridge::SessionTapPayload) + Send + Sync + 'static) {
    if sink().set(Box::new(f)).is_err() {
        tracing::warn!("session_tap 的出口装了第二次 —— 忽略（setup 只该装一次）");
    }
}

/// 一帧 → 发给前端的那一件（纯函数：字段一一照搬，`origin` 由调用方给）。
pub fn to_payload(origin: &str, t: Tap) -> crate::bridge::SessionTapPayload {
    let (data, end) = match t.body {
        TapBody::Data(d) => (Some(d), None),
        TapBody::End(e) => (None, Some(e.as_wire().to_string())),
    };
    crate::bridge::SessionTapPayload {
        origin: crate::origin::Origin(origin.to_string()),
        stream: t.stream,
        resp: t.resp,
        n: t.n,
        data,
        end,
    }
}

/// 读循环交来一帧：交给出口（没装 ⇒ 丢）。**从不阻塞**（出口是句柄的 `on_tap`：只做 credit 记账 ＋ 投递）。
pub fn deliver(origin: &str, t: Tap) {
    if let Some(f) = sink().get() {
        f(to_payload(origin, t));
    }
}
