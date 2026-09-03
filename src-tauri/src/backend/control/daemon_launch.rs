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
mod tests {
    use super::*;

    /// ★ **`mode` 恒 `send-into`**，而且 `create-or-attach` 绝不许从这条路溜出去。
    ///
    /// 变异「把 mode 改成 create-or-attach」= 让这条路开始**新建会话** ——
    /// 那正是 issue #76 的失管会话形态（本条与 CLI 渲染器的 #76 防线是同一条纪律的两侧）。
    #[test]
    fn the_only_mode_this_channel_can_speak_is_send_into() {
        let args = crate::inbound_client::launch_args(
            "send-into",
            "cc-x",
            "true",
            None,
            None,
            Default::default(),
        );
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

    /// ★★ **`may_fall_back: true` 只许出现在一处**〔audit-0805 08-08，Phase G 第 77 件〕。
    ///
    /// 上面那条（`only_the_provably_unsent_cases_may_fall_back`）钉的是**语义**：
    /// 每一档翻译得对不对。旁边那条钉的是**接线**：TS 侧真的按三态分流。
    /// ★ 两条都不管**第三件事**：**谁能构造出一个可回落的结局**。
    ///
    /// 08-08 实测：把「协议漂移」那档从 `SendIntoResponse::refused(…)` 改成直接写字面量
    /// `SendIntoResponse { typed: false, reason: …, may_fall_back: true }`，
    /// **monitor 1003 + vitest 1294 一条都不红**。而那正是模块头注逐字警告的那件事：
    /// 漂移时我们**不知道它键没键入**，回落等于用一条**没有 §34 门**的整串重做一遍，
    /// 后果不可撤销（载荷第二次键进正在跑 claude 的 pane、被当成 prompt 提交、写进历史）。
    ///
    /// # 修法两层，失效模式不同
    ///
    /// 1. **字段私有**（本轮一并做）—— 别的模块由**编译器**挡住；
    /// 2. **本条** —— 模块自己这一半编译器管不着（同文件的分支与 `mod tests` 都够得着），
    ///    于是钉：生产段里 `may_fall_back: true` **恰好一处**，且那一处在 `fn unsent` 体内。
    ///
    /// ⚠ 为什么不钉「所有构造都必须走两个构造器」：`Routed::Done` 那档本来就是直接写字面量
    /// （`typed: true`，它既不是 unsent 也不是 refused），硬要求走构造器会逼人再造一个
    /// `fn done()` —— 那是**为判据改代码形状**，而真正要守的性质只有一句：
    /// **可回落只能从「能证明没发出去」那一档产出**。
    #[test]
    fn may_fall_back_true_lives_in_exactly_one_place() {
        let src = std::fs::read_to_string(file!())
            .or_else(|_| {
                std::fs::read_to_string(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("src/backend/control/daemon_launch.rs"),
                )
            })
            .expect("读不到本模块源码");
        let prod = guard_core::production_code(&src);
        // 运行时拼，免得命中本条自己的说明文字（F58 在这上面栽过两次）。
        let needle = format!("may_fall_back: {}", "true");
        let hits: Vec<&str> = prod
            .lines()
            .map(str::trim)
            .filter(|l| l.contains(&needle))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "生产段里 `{needle}` 出现 {} 处（应恰好 1 处，在 `fn unsent` 里）：\n{}\n\n\
             ★ 多出来的那处意味着**有一档不经「能证明没发出去」这个判断就允许了回落**。\n\
             那条回落走的整串没有 §34 的门 ⇒ 一次门拒绝、或一次「daemon 可能已经键入过」\n\
             会被原样重做一遍；后者**不可撤销**（载荷第二次进正在跑 claude 的 pane，\n\
             被当成 prompt 提交并写进对话历史）。\n\
             ⇒ 新的一档要允许回落，就让它走 `SendIntoResponse::unsent(...)`，\n\
             并先回答一句「凭什么能证明这条命令根本没发出去」。\n\
             ⚠ 0 处 = `unsent` 那一处被改写了 ⇒ 没有 daemon 的远端全都用不了（C7 过渡期），\n\
             那是另一个方向的坏，同样要红。",
            hits.len(),
            hits.join("\n")
        );

        // 那一处必须**在 `fn unsent` 体内** —— 否则「只有一处」可以被搬到任何地方去满足。
        let at = prod
            .find("fn unsent(")
            .expect("`fn unsent` 不见了 —— 本条的锚点没了");
        let end = prod[at..]
            .find("\n    }")
            .map(|k| at + k)
            .unwrap_or(prod.len());
        assert!(
            prod[at..end].contains(&needle),
            "唯一那处 `{needle}` 不在 `fn unsent` 体内了。\n\
             「恰好一处」这件事本身不够 —— 它可以被搬进任何一档去满足计数，\n\
             而语义要求它只能从「能证明没发出去」那一档产出。"
        );
    }

    /// ★★ **F14 的核心性质**：只有「能证明这条命令根本没发出去」的档才许回落。
    ///
    /// 反过来错法（把 `Refused` 也映射成 `mayFallBack:true`）会让调用方用那条**无门**的整串
    /// 把一次门拒绝、或一次「daemon 可能已经键入过」重做一遍 —— 后者的后果是
    /// **载荷第二次被键入进一个正在跑 claude 的 pane**、被当成 prompt 提交、不可撤销。
    #[test]
    fn only_the_provably_unsent_cases_may_fall_back() {
        use super::super::daemon_route::Routed;
        let done = SendIntoResponse::from_routed(Routed::Done);
        assert!(done.typed && !done.may_fall_back && done.reason.is_none());

        let unsent = SendIntoResponse::from_routed(Routed::NoChannel("没通道".into()));
        assert!(
            !unsent.typed && unsent.may_fall_back,
            "「证明没发出去」那档必须允许回落 —— 否则没有 daemon 的远端全都用不了（C7 过渡期）"
        );
        assert!(unsent.reason.is_some(), "回落时 reason 是调用方唯一的线索");

        for why in ["拒绝：wrong_owner", "等应答超时", "协议漂移"] {
            let r = SendIntoResponse::from_routed(Routed::Refused(why.into()));
            assert!(
                !r.typed && !r.may_fall_back,
                "`Refused({why})` 被判成了可回落 —— 那条整串**没有 §34 的门**，\n\
                 回落等于用一条无门的路把「被门拒绝」或「可能已经键入过」重做一遍。"
            );
        }
        // 坏数据那一档也不许回落：拿空载荷去渲染整串只会产出无意义的命令。
        let bad = SendIntoResponse::refused("会话名或载荷为空");
        assert!(!bad.may_fall_back);
    }

    /// ★★ **生产接线（跨语言）**：TS 侧真的按三态分流，`refused` 那支**不许**落到整串。
    ///
    /// # 为什么这条判据长在 Rust 侧
    ///
    /// `SendIntoResponse` 的 TS 类型是**手写**的（`src/launch-cli-wire.ts`，不是 ts-rs 生成）
    /// ⇒ 字段名两侧靠人同步。而「回落契约」这件事**只有两侧一起看才成立**：
    /// Rust 说了 `may_fall_back:false`，TS 不读它就等于没说。
    #[test]
    fn refused_never_falls_back_to_the_whole_string() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");
        let read = |rel: &str| {
            let p = root.join(rel);
            assert!(p.is_file(), "读不到 {rel} —— 读不到的文件只会静默返回空串");
            std::fs::read_to_string(p).unwrap_or_default()
        };

        // ① 跨语言字段名：手写类型里必须有 serde camelCase 后的那个名字。
        let wire = read("src/launch-cli-wire.ts");
        assert!(
            wire.contains("mayFallBack"),
            "`src/launch-cli-wire.ts` 的手写 `SendIntoResponse` 里没有 `mayFallBack` ——\n\
             Rust 侧发了这个字段而 TS 侧读不到 ⇒ 回落契约单方面失效（`undefined` 是 falsy，\n\
             恰好会被读成「不许回落」，所以它是 fail-closed 的 —— 但那是巧合，不是设计）。"
        );

        // ② 三态分流真的在生产路径上。
        let run = read("src/remote-launch-run.ts");
        for needle in ["\"refused\"", "\"typed\"", "mayFallBack"] {
            assert!(
                run.contains(needle),
                "`remote-launch-run.ts` 里找不到 {needle} —— 三态分流没接上"
            );
        }
        // ③ ★ `refused` 那支必须**在**回落之前就地返回。
        let at = run
            .find("sent.verdict === \"refused\"")
            .expect("上面已断言过存在");
        let arm = &run[at..(at + 700).min(run.len())];
        assert!(
            arm.contains("return false"),
            "`refused` 那支没有就地 `return` —— 它会穿到下面的整串回落。\n\
             那条整串（`session-backend.ts` 的 `send-keys …; attach …`）**没有 §34 的门**。\n\
             实得这一段：{arm:?}"
        );
        // ④ 而且必须让用户看见（回落可以静默，**拒绝不行** —— 否则他以为成功了）。
        assert!(
            arm.contains("showActionFailureToast"),
            "`refused` 那支没有 toast —— 用户会以为就地 resume 成功了。\n\
             ⚠ 这与「回落绝不 toast」那条纪律**不矛盾**：回落用户看不出区别，拒绝是真没做成。"
        );
    }
}
