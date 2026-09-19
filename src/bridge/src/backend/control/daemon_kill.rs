//! F04b：**daemon `kill` 的 monitor 侧发送端** —— 「控制搬进 daemon」的第二条通道。
//!
//! # 它在定框里的位置
//!
//! C5 逐字写着「任何**改状态**的 tmux 命令一律归 `control/`」，C6 写着
//! 「**先搬 Gate 2，再切 kill / send-keys —— 顺序不可反**」。
//! F03 搬了 Gate 2、F04a 搬了 Gate 3 + daemon 侧的 `control/kill.rs`，
//! **本模块是那条顺序的最后一步**：让 monitor 真的走过去。
//!
//! # 切过去换来的是什么（不是「架构更整齐」这种空话）
//!
//! 今天 `tmux.rs::kill_remote_tmux` 拼一条穿过 ssh + shell 的原子命令，最后
//! `tmux kill-session -t '=name:'` —— **对名字下手**。daemon 侧那条
//! （`control/kill.rs`）先 `admit_destructive` 拿到 `#{session_id}` 句柄，
//! 再 `kill-session -t '$3'` —— **对句柄下手**。
//! tmux 的 `$N` 在 server 生命周期内唯一且不复用 ⇒ 名字在探测与执行之间被重新绑定
//! 也杀不到别人身上。**破坏性动作尤其不能对名字下手**（`control/gate` 头注那段 TOCTOU 分析）。
//! ⇒ 切路由本身就是**安全性的净改善**，不只是搬家。
//!
//! # ★★ 三态而不是两态：为什么「过门被拒」不许回落
//!
//! C7 允许过渡期的回落，但回落有一个**危险的错法**：把 daemon 的一次**拒绝**
//! （`wrong_owner` / `too_many_windows`）当成「daemon 不可用」，转头用 SSH 那条路再杀一次。
//! 那等于**把一次被门拒绝洗成另一条路的成功** —— 今天两条路的门恰好等价，所以看不出问题；
//! 哪天有一侧漂了，这就是一个静默的权限旁路。
//!
//! ⚠ **分流规则本身不在这里** —— F04c 把它搬进了 [`super::daemon_route`]，
//! 与 `send-keys` 那条命令共用**一份**（两份必漂，而漂开的后果就是上面那条）。
//! 本模块只负责「拒绝该怎么对用户说」。

use std::time::Duration;

use super::daemon_route::{no_channel, route_call_error, Routed};

/// 一次 `kill` 的往返上限。同 `daemon_launch::CALL_TIMEOUT_SECS` 的理由：
/// §41「零定时器」管的是 daemon 侧不许等，客户端侧的等待本来就归客户端。
const CALL_TIMEOUT_SECS: u64 = 10;

/// 从 daemon 的 `kill` 应答里读出 `killed`。
///
/// 三态都要有说法（同 `daemon_launch::typed_from_reply`）：字段在且是 bool ⇒ 照抄；
/// 字段缺 / 类型不对 / 整个 body 缺 ⇒ **不当成 false**，而是诚实报「应答形状不认识」——
/// 那是协议漂移，不是「没杀成」。⚠ 而且对 kill 尤其重要：形状不认识时我们**不知道它杀没杀**，
/// 所以调用方必须把它当成 `Refused`（不回落），否则就是在未知状态上再做一次破坏性动作。
pub(crate) fn killed_from_reply(reply: Option<&serde_json::Value>) -> Result<bool, String> {
    let Some(v) = reply else {
        return Err("daemon 的 kill 应答没有 body（协议漂移？）".into());
    };
    match v.get("killed") {
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!("kill 应答里的 killed 不是 bool：{other}")),
        None => Err(format!("kill 应答里没有 killed 字段：{v}")),
    }
}

/// daemon 的错误码 → 用户看的话。**与今天那条 SSH 路的文案逐条对齐**，
/// 否则同一个拒绝在两条路上说两种话，用户会以为是两个不同的问题。
fn refusal_text(code: &str, message: &str) -> String {
    match code {
        "no_tmux" => "远端未安装 tmux".to_string(),
        "no_such_session" => "远端会话已不存在（可能已被终止）".to_string(),
        "wrong_owner" => format!(
            "拒绝 kill：目标未通过身份守卫（{message}）——可能不是本工具管理的会话\
             （避免误杀你自己的 tmux 会话）"
        ),
        "too_many_windows" => format!(
            "拒绝 kill：目标未通过窗口守卫（{message}）——它已被扩展出额外窗口\
             （请到该 tmux 里自行处理）"
        ),
        _ => format!("远端 kill 失败（{code}）：{message}"),
    }
}

/// **F04b：杀一个远端 tmux 会话（走 daemon `control/kill.rs`）。**
///
/// 不是 `#[tauri::command]` —— 前端**够不着才对**（C9：frontend 只剩开窗）。
/// 唯一调用方是 `tmux.rs::kill_remote_tmux`，它按三态分流。
pub(crate) async fn daemon_kill(origin: &str, name: &str) -> Routed {
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return no_channel(origin);
    };
    let args = serde_json::json!({ "name": name });
    match client
        .call("kill", args, Duration::from_secs(CALL_TIMEOUT_SECS))
        .await
    {
        Ok(reply) => match killed_from_reply(reply.as_ref()) {
            // daemon 只在真杀掉时回 `killed:true`（`kill.rs::kill_for_inbound`）。
            Ok(true) => Routed::Done,
            Ok(false) => Routed::Refused(
                "daemon 回报未杀掉，但也没给错误码 —— 协议漂移，不再用另一条路重试".into(),
            ),
            Err(e) => Routed::Refused(format!(
                "{e} —— ⚠ 应答形状不认识时无法判断它杀没杀，因此不再用另一条路重杀"
            )),
        },
        Err(e) => route_call_error(&e, refusal_text),
    }
}

/// ★ **「怎么算一处 tmux 建会话」只有一个家**〔audit-0805 08-08，E3〕。
///
/// 本文件的创建路径登记表与 `account_usage.rs` 的 D3 例外表问的是同一件事，
/// 而 08-08 实测它们的**发现口径不一样**：这里同时认命令串与 argv 两种形态，
/// 那边只认命令串。于是用 argv 形态（`Command::new("tmux").args(["new-session","-d",…])`）
/// 新建一处会话时，**这里红、那边不红** —— 那边的表可以静默变得不完整。
///
/// ⇒ 与其在两处各写一份近似的口径（那是下一个漂移源），不如让它们问同一个函数。
#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/daemon_kill_creation_detect.rs"]
pub(crate) mod creation_detect;

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/daemon_kill_tests.rs"]
mod tests;
