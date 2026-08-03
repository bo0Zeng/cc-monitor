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

/// 结局。`typed == false` 时 `reason` 必有值 —— 那是调用方回落的唯一线索。
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendIntoResponse {
    /// 载荷是否**真的键入了**。daemon 的 `typed` 逐字转发，不做乐观解读。
    pub typed: bool,
    pub reason: Option<String>,
}

impl SendIntoResponse {
    fn degraded(reason: impl Into<String>) -> Self {
        Self {
            typed: false,
            reason: Some(reason.into()),
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

/// **U8a-2c-1：往已存在的远端 tmux 会话里键入载荷（`send-keys` 那半边走 daemon）。**
///
/// `attach` 那半边**不在这里** —— 见模块头注（§1.3）。
#[tauri::command]
pub async fn daemon_send_into(req: SendIntoRequest) -> SendIntoResponse {
    if req.name.trim().is_empty() || req.payload.is_empty() {
        return SendIntoResponse::degraded("会话名或载荷为空 —— 拒绝发出（坏数据不是缺省）");
    }
    let Some(client) = crate::inbound_client::client_for(&req.origin) else {
        return SendIntoResponse::degraded(format!(
            "[{}] 没有可用的控制通道（daemon 未在场或长连接未握手）",
            req.origin
        ));
    };
    let args = crate::inbound_client::launch_args("send-into", &req.name, &req.payload, None, None);
    match client
        .call("launch", args, Duration::from_secs(CALL_TIMEOUT_SECS))
        .await
    {
        Ok(reply) => match typed_from_reply(reply.as_ref()) {
            Ok(typed) => SendIntoResponse {
                typed,
                reason: if typed {
                    None
                } else {
                    Some("daemon 回报未键入（会话可能已不存在）".into())
                },
            },
            Err(e) => SendIntoResponse::degraded(e),
        },
        Err(e) => SendIntoResponse::degraded(format!("daemon launch 调用失败：{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ **`mode` 恒 `send-into`**，而且 `create-or-attach` 绝不许从这条路溜出去。
    ///
    /// 变异「把 mode 改成 create-or-attach」= 让这条路开始**新建会话** ——
    /// 那正是 issue #76 的失管会话形态（本条与 CLI 渲染器的 #76 防线是同一条纪律的两侧）。
    #[test]
    fn the_only_mode_this_channel_can_speak_is_send_into() {
        let args = crate::inbound_client::launch_args("send-into", "cc-x", "true", None, None);
        assert_eq!(args["mode"], "send-into");
        // 本模块的生产段里不许出现另一个 mode 字面量。
        let prod = guard_core::production_code(include_str!("daemon_launch.rs"));
        assert!(
            !prod.contains(&format!("\"create{}attach\"", "-or-")),
            "生产段出现了 create-or-attach —— 这条路一旦能新建会话，就是 #76 的失管会话形态；\
             那一格的幂等语义与 daemon 的 created/typed 不逐字等价，要切得先对拍（见模块头注）"
        );
        assert_eq!(
            prod.matches("\"send-into\"").count(),
            1,
            "`send-into` 这个字面量在生产段应当只出现一次（就是发出去那处）"
        );
    }

    /// ★ 没有控制通道 ⇒ **诚实降级**，而不是 panic、也不是假装键入了。
    #[test]
    fn no_control_channel_degrades_honestly_with_a_reason() {
        let r = tokio::runtime::Runtime::new().unwrap().block_on(async {
            daemon_send_into(SendIntoRequest {
                // 不可能被注册的 origin（注册表是进程内的）。
                origin: "u8a-2c-1-没有这个远端".into(),
                name: "cc-x".into(),
                payload: "true".into(),
            })
            .await
        });
        assert!(!r.typed, "没有通道却报 typed=true —— 调用方会以为已经键入");
        let reason = r.reason.expect("降级必须带理由（前端唯一的回落线索）");
        assert!(
            reason.contains("控制通道"),
            "理由没说清是通道不在：{reason}"
        );
    }

    /// 坏数据不是缺省：空会话名 / 空载荷 ⇒ 一个字节都不发。
    #[test]
    fn empty_name_or_payload_is_refused_before_any_io() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        for (name, payload) in [("", "true"), ("  ", "true"), ("cc-x", "")] {
            let r = rt.block_on(daemon_send_into(SendIntoRequest {
                origin: "任意".into(),
                name: name.into(),
                payload: payload.into(),
            }));
            assert!(!r.typed);
            assert!(
                r.reason.as_deref().is_some_and(|s| s.contains("为空")),
                "({name:?},{payload:?}) 的理由不对：{:?}",
                r.reason
            );
        }
    }

    /// ★ `typed` 三态：true / false / **形状不认识**。
    ///
    /// 第三态是要紧的那个：字段缺了**不能当 false** —— 那会把「协议漂移」报成
    /// 「会话不存在」，而这两件事的处置完全不同（一个要改代码，一个让用户重试）。
    #[test]
    fn typed_is_read_out_of_three_states_not_two() {
        use serde_json::json;
        assert_eq!(typed_from_reply(Some(&json!({"typed": true}))), Ok(true));
        assert_eq!(
            typed_from_reply(Some(&json!({"typed": false, "created": false}))),
            Ok(false)
        );
        for bad in [json!({}), json!({"typed": "yes"}), json!({"typed": 1})] {
            assert!(
                typed_from_reply(Some(&bad)).is_err(),
                "{bad} 应当报「形状不认识」而不是当 false"
            );
        }
        assert!(typed_from_reply(None).is_err());
    }

    /// ★ 跨轨：本模块发的字段名必须是 **daemon 那条命令声明过**的。
    ///
    /// `inbound_client::launch_args_field_names_match_the_daemon_parser` 钉的是编码器 ↔ 解析器；
    /// 这条钉的是**本模块用到的那三个**在 daemon 的 `REGISTRY` 里真有登记 ——
    /// daemon 改字段名或把 launch 摘掉，这条红。
    #[test]
    fn the_fields_this_channel_sends_are_declared_by_the_daemon_registry() {
        const INBOUND: &str = include_str!("../../../../remote-daemon-proto/src/inbound.rs");
        let at = INBOUND
            .find("name: \"launch\",")
            .expect("daemon 的 REGISTRY 里找不到 launch —— 抽取坏了，本断言在空转");
        let rest = &INBOUND[at..];
        let fields_at = rest.find("fields: &[").expect("launch 那条没有 fields");
        let body = &rest[fields_at..];
        let body = &body[..body.find(']').expect("fields 没收尾")];
        assert!(body.len() > 30, "抽到的 fields 太短（{body:?}）—— 抽取坏了");
        for f in ["mode", "name", "payload"] {
            assert!(
                body.contains(&format!("\"{f}\"")),
                "daemon 的 launch 没有声明字段 `{f}`，而本模块在发它：{body}"
            );
        }
    }
}
