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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn killed_from_reply_reads_the_flag_and_refuses_to_guess() {
        assert_eq!(
            killed_from_reply(Some(&serde_json::json!({ "killed": true }))),
            Ok(true)
        );
        assert_eq!(
            killed_from_reply(Some(&serde_json::json!({ "killed": false }))),
            Ok(false)
        );
        // ★ 缺字段 / 类型不对 / 无 body 一律是**协议漂移**，不是「没杀成」。
        for bad in [
            serde_json::json!({}),
            serde_json::json!({ "killed": "yes" }),
            serde_json::json!({ "session": "x-cc" }),
        ] {
            let r = killed_from_reply(Some(&bad));
            assert!(r.is_err(), "{bad} 应当报协议漂移，而不是被读成 false");
        }
        assert!(killed_from_reply(None).is_err());
    }

    /// ★★ **跨轨钉（本件 D 审计自己抓出来的）：创建路径不许铸出主路杀不掉的名字。**
    ///
    /// # 它抓的是什么
    ///
    /// daemon 的 `kill.rs::parse_name` 拒 `:` 与 `=`（「它们是 tmux 目标语法」）。
    /// TS 侧 `isValidTmuxName` 只禁了 `:`，**`=` 是允许的** ⇒ F04b 切完之后，
    /// 一个像 `proj=x-cc` 的名字**建得出来、却在主路上杀不掉**（daemon 回 `invalid_args`）。
    /// 那是一条真的（虽然窄的）回归：切之前 SSH 那条路杀得掉它。
    ///
    /// 处置选的是「**改结构让问题不存在**」（同仓 U3 的先例）：不给 kill 开
    /// 「形状拒绝就回落 shell 路」的特例（那正是本模块要挡的洗白），
    /// 而是把 `=` 加进**创建**路径的禁字集。
    ///
    /// ⚠ 本条钉的是「**两侧的禁字集不许再漂开**」：daemon 拒的每个字符，
    /// 创建路径都必须拒。反过来不要求（创建路径可以更严，比如 glob）。
    #[test]
    fn the_creation_path_cannot_mint_a_name_the_main_path_cannot_kill() {
        let kill_rs = include_str!("../../../../remote-daemon-proto/src/control/kill.rs");
        let kill_prod = guard_core::production_code(kill_rs);
        // 反向锚点：daemon 那条形状门还在（它没了本条就在空转）。
        assert!(
            kill_prod.contains("name.contains(':')") && kill_prod.contains("name.contains('=')"),
            "daemon 的 `parse_name` 不再拒 `:`/`=` 了 —— 本条判据的前提没了，回来重裁"
        );
        let ts = include_str!("../../../../src/shell-quote.ts");
        let at = ts
            .find("export function isValidNewTmuxName")
            .expect("找不到创建路径的校验函数 —— 改名了就把本条一起改");
        let body = &ts[at..(at + 200).min(ts.len())];
        assert!(
            body.contains("isValidTmuxName(name)"),
            "创建路径不再走 `isValidTmuxName` —— `:` 那一半的禁令没了：{body:?}"
        );
        assert!(
            body.contains('='),
            "创建路径的禁字集里没有 `=` —— 于是它能铸出 `proj=x-cc` 这种\n\
             **建得出来、主路杀不掉**的名字（daemon 的 kill 形状门拒 `=`）。\n\
             ⚠ 正确的处置不是让 kill 在形状拒绝时回落到 shell 路 ——\n\
             那是把一次拒绝洗成另一条路的成功。实得这一段：{body:?}"
        );
    }

    /// ★ **前提触发器：耐久文档里那句「过渡期回落」不许比代码活得久。**
    ///
    /// # 为什么专门给一句文档配一条判据
    ///
    /// F07 顺出的一般化：**「状态列」与「实测答案」是耐久文档里最易腐的两种字段** ——
    /// 它们描述**当下**，而文档寿命比「当下」长。F04b 自己就撞到四处：
    /// `IPC-PROTOCOL` 说 kill 的 shell 路是主路（已降为回落）·
    /// `INVARIANTS §A5` 说 kill「无此白名单」（**自 F04 起就假了**）·
    /// `INVARIANTS §34` 说三道门住 `tmux.rs`（主路那份已在 daemon）·
    /// 用量方案文档说 kill「daemon 不参与」。
    ///
    /// 处置不是「以后记得更新」，是**配一条触发器**：本条把那句话与
    /// 「回落这段代码到底还在不在」绑在一起。F11 删回落时它会主动红，
    /// 逼人回来把那句话一起改掉。
    #[test]
    fn the_doc_sentence_about_the_transitional_fallback_cannot_outlive_the_code() {
        let tmux_rs = guard_core::production_code(include_str!("../../tmux.rs"));
        let at = tmux_rs
            .find("pub async fn kill_remote_tmux(")
            .expect("找不到 kill 命令 —— 签名变了就把本条一起改");
        let body = &tmux_rs[at..];
        let end = body.find("\n}\n").map(|k| k + 3).unwrap_or(body.len());
        let fallback_alive = body[..end].contains("connect_and_exec_cmd");
        let doc = include_str!("../../../../doc/IPC-PROTOCOL.md");
        let doc_says_transitional = doc.contains("过渡期回落");
        assert_eq!(
            fallback_alive, doc_says_transitional,
            "代码与文档对不上了：\n\
             · `kill_remote_tmux` 里还有一次性 SSH 回落吗 = {fallback_alive}\n\
             · `doc/IPC-PROTOCOL.md` 还写着「过渡期回落」吗 = {doc_says_transitional}\n\
             ⚠ 如果是**删掉了回落**（F11 的活）：那句话要一起改，否则下一个读者会以为\n\
             「没有 daemon 的远端」还有一条路可走 —— 而那正是 C7 说的过渡期已经结束。\n\
             ⚠ 如果是**改了文档措辞**：本条判据跟着改（它钉的是两者一致，不是某个字面量）。"
        );
    }

    /// 两条路的拒绝文案必须说同一件事 —— 同一个拒绝在两条路上说两种话，
    /// 用户会以为是两个不同的问题。
    #[test]
    fn the_refusal_wording_matches_the_ssh_path() {
        let ssh = include_str!("../../tmux.rs");
        for (code, needle) in [
            ("no_tmux", "远端未安装 tmux"),
            ("no_such_session", "远端会话已不存在（可能已被终止）"),
            ("wrong_owner", "可能不是本工具管理的会话"),
            ("too_many_windows", "请到该 tmux 里自行处理"),
        ] {
            let mine = refusal_text(code, "m");
            assert!(
                mine.contains(needle),
                "`{code}` 的文案里没有 {needle:?}：{mine}"
            );
            assert!(
                ssh.contains(needle),
                "SSH 那条路里已经没有 {needle:?} 了 —— 两条路的文案漂了，\
                 要么一起改，要么本条判据该跟着改"
            );
        }
    }
}
