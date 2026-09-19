//! U8a-2c-1：**daemon `launch` 的 monitor 侧发送端** —— 「控制搬进 daemon」的第一条通道。
//!
//! # 它补的是哪一个洞
//!
//! daemon 的 `control/launch.rs` 从 U8a-2b 起就完整了：argv 直传不过 shell、`send-into`
//! 一等模式、真进程 + 真 tmux 的 e2e（`inbound-daemon-frames.sh` 四个 launch 场景）。
//! monitor 侧的编码器（`inbound_client::launch_args`）也早就写好了 ——
//! 但它带着 `#[allow(dead_code)]`：**零生产调用方**。
//!
//! 三视角复盘（2026-08-03）点破的「方向偏移」就是这个形状：九个 commit 完成的是
//! 「渲染从 TS 移到 monitor 的 Rust」，而主计划要的是「**控制从 monitor 移到 daemon**」。
//! 本模块是后者的第一条真通道。
//!
//! # 为什么只做 `send-into`（本轮的刀口）
//!
//! `session-backend.ts` 的 `send-into` 那一格今天产的串逐字是：
//!
//! ```text
//! tmux send-keys -t '=name:' '<载荷>' Enter; tmux attach -t '=name:'
//! ```
//!
//! **两半干干净净**：
//! - 前半 `send-keys` = daemon `launch{mode:"send-into"}` 的逐字对应（daemon 侧已验证）；
//! - 后半 `attach` **必须留在用户自己的终端里**（§1.3：pid 要等于 pidfile 名、
//!   tty/Ctrl-C 要落在 agent 上、`tmux attach` 要占住调用方终端）。
//!
//! 而且这一格**完全不经 ccm** —— CLI 渲染器对它恒返回
//! `Refusal::SendIntoHasNoCliForm`（#76 防线）⇒ **没有 ccm 契约冲突**。
//! 这是整条远端主路上唯一一处「daemon 能接的那半」与「必须留在终端的那半」天然分开的地方。
//!
//! ⚠ **`create-or-attach` 那一格刻意不做**：它今天靠一条 shell 串的
//! 「`new-session -d 2>/dev/null &&` 建失败被吞 ⇒ 短路跳过 send-keys」实现幂等，
//! 而 daemon 的 `created`/`typed` 两个布尔与那个技巧**不逐字等价**（谁在什么情况下不键入，
//! 两边的判据不同）。要切它得先把两种幂等语义对拍出来 —— 另立。
//!
//! # 返回值为什么是 tagged 而不是 `Result`
//!
//! 「这台远端没有可用的控制通道」**不是错误，是诚实降级**（同 `launch_wire` 的
//! `CliRenderResponse`）：调用方要拿着 `reason` 回落到今天那条整串。
//! 用 `Err` 表达它会和「真的发失败了」混成一件事，而那两件在前端要走**同一个**回落分支
//! 但**不同的**诊断文案。

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// daemon 一条 `launch` 的往返上限。**不是超时策略的一部分** ——
/// §41「零定时器」管的是 daemon 侧不许等；客户端侧的等待本来就归客户端
/// （见 §1.5「超时一律推给客户端」）。
const CALL_TIMEOUT_SECS: u64 = 10;

/// 往一个**已存在**的远端 tmux 会话里键入载荷。`origin` 是远端配置的 label。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SendIntoRequest {
    pub origin: String,
    /// tmux 会话名（**裸名**，`=name:` 的精确匹配形态由 daemon 侧的 `exact_target` 加）。
    pub name: String,
    /// 内层载荷（`env 前缀 → argv`）。由 `super::payload::render_payload` 产出。
    pub payload: String,
}

/// 结局。`typed == false` 时 `reason` 必有值 —— 那是调用方的唯一线索。
///
/// # ★★ F14 加了第三个字段 `may_fall_back`：两态表达不出「不许回落」
///
/// 原来只有 `{typed, reason}` 两态，调用方把 `typed:false` **一律**读成「回落到整串」。
/// 而那条整串（`session_backend` 的 `send-keys …; attach …`）**没有 §34 的门** ⇒
/// 一次 `wrong_owner` 或一次「daemon 已键入但应答超时」都会被那条无门的路**重做一遍**：
/// 后者的后果是**载荷第二次被键入进一个已经在跑 claude 的 pane** ——
/// 那条 `env … claude --resume …` 会被当成 **prompt 提交**、写进对话历史、**不可撤销**。
/// （F12 的 `/full-audit` 三个视角独立指向这里。）
///
/// ⇒ 分流判定收进 [`super::daemon_route`]（与 `kill`/`send-keys` 共用一份），
/// 这里只把它翻成线上的一个布尔。**`may_fall_back` 的语义严格是**：
/// 「**能证明这条命令根本没发出去**」，不是「失败了」。
/// ⚠⚠ **三个字段刻意不 `pub`**〔audit-0805 08-08〕：整条回落契约压在
/// 「哪一档配哪个 `may_fall_back`」上，而在此之前**任何模块都能直接写一个字面量**
/// 绕过 [`SendIntoResponse::unsent`] / [`SendIntoResponse::refused`]。
/// 实测：把「协议漂移」那档改成直接构造 `may_fall_back: true`，
/// **monitor 1003 + vitest 1294 一条都不红** —— 而那条路的后果是不可撤销的
/// （载荷第二次键进正在跑 claude 的 pane、被当成 prompt 提交）。
///
/// 字段私有 ⇒ **别的模块由编译器挡住**（全仓实测：外部零读点，它只经 serde 上线）；
/// 模块**自己**这一半编译器管不着（同文件的 `mod tests` 与后来新增的分支都够得着），
/// 由 `may_fall_back_true_lives_in_exactly_one_place` 钉。两层失效模式不同 ⇒ 是真纵深。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendIntoResponse {
    /// daemon 报的 `typed`，**逐字转发，不做乐观解读**。
    ///
    /// ⚠⚠ **它只有 `tmux send-keys` 的退出码那么强**〔audit-0805 F10 下半，报告 I-3〕。
    /// 本行原先断言载荷已确凿落地 —— 那是**替证据说大话**：
    /// pane 处于 copy-mode 时 `send-keys` **照样退 0**，键被键表吃掉、载荷没进应用。
    /// 而下游把 `typed:true` 读成「不必回落」（`launch-cli-wire.ts:63` 逐字：
    /// `typed:false` 时的 `reason` 是回落的**唯一线索**）⇒ 载荷静默消失。
    /// 由 daemon 侧 `typed_is_only_as_strong_as_the_send_keys_exit_code` 钉住。
    typed: bool,
    reason: Option<String>,
    /// **调用方可不可以回落到那条整串。** 只有「能证明没发出去」才 `true`。
    /// ⚠ TS 侧那个类型是**手写**的（`src/launch-cli-wire.ts`）⇒ 字段名两侧必须手动同步，
    /// 由 `refused_never_falls_back_to_the_whole_string` 钉住。
    may_fall_back: bool,
}

impl SendIntoResponse {
    /// 能证明没发出去 ⇒ 允许回落（C7 过渡期那条路）。
    fn unsent(reason: impl Into<String>) -> Self {
        Self {
            typed: false,
            reason: Some(reason.into()),
            may_fall_back: true,
        }
    }
    /// daemon 说过话了，**或者**我们无法证明它没执行 ⇒ **不许回落**。
    fn refused(reason: impl Into<String>) -> Self {
        Self {
            typed: false,
            reason: Some(reason.into()),
            may_fall_back: false,
        }
    }
    /// `Routed` → 线上结局。**分流不在这里做**，只在这里翻译。
    fn from_routed(r: super::daemon_route::Routed) -> Self {
        match r {
            super::daemon_route::Routed::Done => Self {
                typed: true,
                reason: None,
                may_fall_back: false,
            },
            super::daemon_route::Routed::NoChannel(why) => Self::unsent(why),
            super::daemon_route::Routed::Refused(why) => Self::refused(why),
        }
    }
}

/// 从 daemon 的应答里读出 `typed`。**抠出来是为了可断言**（命令本体要有活的控制通道才跑得起来）。
///
/// 三态都要有说法：字段在且是 bool ⇒ 照抄；字段缺 ⇒ **不当成 false**，
/// 而是诚实报「应答形状不认识」（那是协议漂移，不是「没键入」）；应答体缺失 ⇒ 同理。
pub(crate) fn typed_from_reply(reply: Option<&serde_json::Value>) -> Result<bool, String> {
    let Some(v) = reply else {
        return Err("daemon 的 launch 应答没有 body（协议漂移？）".into());
    };
    match v.get("typed") {
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("launch 应答里的 typed 不是 bool：{other}")),
        None => Err(format!("launch 应答里没有 typed 字段：{v}")),
    }
}

/// daemon 的错误码 → 用户看的话。与 `kill`/`send-keys` 两条同形。
fn refusal_text(code: &str, message: &str) -> String {
    match code {
        "no_tmux" => "远端未安装 tmux".to_string(),
        "no_such_session" => "远端会话已不存在（可能已被终止）".to_string(),
        "wrong_owner" => {
            format!("拒绝就地 resume：目标未通过身份守卫（{message}）——可能不是本工具管理的会话")
        }
        "typed_unconfirmed" => format!("载荷未必送达（{message}）—— 会话在，但 send-keys 失败"),
        _ => format!("远端就地 resume 失败（{code}）：{message}"),
    }
}

/// **U8a-2c-1：往已存在的远端 tmux 会话里键入载荷（`send-keys` 那半边走 daemon）。**
///
/// `attach` 那半边**不在这里** —— 见模块头注（§1.3）。
#[tauri::command]
pub async fn daemon_send_into(req: SendIntoRequest) -> SendIntoResponse {
    if req.name.trim().is_empty() || req.payload.is_empty() {
        // ⚠ **坏数据不许回落**：拿一个空载荷去渲染整串只会产出一条无意义的 shell 命令。
        return SendIntoResponse::refused("会话名或载荷为空 —— 拒绝发出（坏数据不是缺省）");
    }
    let Some(client) = crate::inbound_client::client_for(&req.origin) else {
        return SendIntoResponse::from_routed(super::daemon_route::no_channel(&req.origin));
    };
    // ⚠ `LaunchExtras::default()`：`agent` / `width` / `height` 是 `create-or-attach` 专有的
    //   （`K-P2` `D3` 加的），这条路**只发 `send-into`** ⇒ 一个都不该带。
    let args = crate::inbound_client::launch_args(
        "send-into",
        &req.name,
        &req.payload,
        None,
        None,
        Default::default(),
    );
    match client
        .call("launch", args, Duration::from_secs(CALL_TIMEOUT_SECS))
        .await
    {
        Ok(reply) => match typed_from_reply(reply.as_ref()) {
            Ok(true) => SendIntoResponse {
                typed: true,
                reason: None,
                may_fall_back: false,
            },
            // daemon 对 `send-into` 只在真键入时回 `typed:true`，否则回错误码 ⇒
            // `Ok(false)` 是协议漂移，而漂移时**我们不知道它键没键入** ⇒ 不许回落。
            Ok(false) => SendIntoResponse::refused(
                "daemon 回报未键入却没给错误码 —— 协议漂移，不再用另一条路重做",
            ),
            Err(e) => SendIntoResponse::refused(format!(
                "{e} —— ⚠ 应答形状不认识时无法判断载荷有没有落进去，因此不再用另一条路重键入"
            )),
        },
        Err(e) => {
            SendIntoResponse::from_routed(super::daemon_route::route_call_error(&e, refusal_text))
        }
    }
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/daemon_launch_tests.rs"]
mod tests;
