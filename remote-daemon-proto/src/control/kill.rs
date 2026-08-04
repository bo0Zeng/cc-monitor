//! F04a：**杀一个 tmux 会话**（定框 C5：任何改状态的 tmux 命令一律归 `control/`）。
//!
//! # 它与 monitor 侧那条路的关系
//!
//! monitor 的 `tmux.rs::kill_remote_tmux` 今天拼一条穿过 ssh + shell 的原子命令，
//! 带 §34 的 **Gate 1/2/3**。本模块是它在 daemon 侧的对应物：
//! **argv 直传、不过 shell**，三道门由 [`super::gate::admit_destructive`] 复现。
//!
//! ⚠ **本模块落地不等于 monitor 那条路已经切过来了。** 定框 C6 逐字写着
//! 「**先搬 Gate 2，再切 kill / send-keys —— 顺序不可反**」；F03 搬了 Gate 2，
//! F04a（本件）搬 Gate 3 + 这条 kill，**切路由是 F04b**。
//! 那件的验证面里有「真远端那一跳」，本机结构性验不了（ROADMAP §5）⇒ 单独一件。
//!
//! # ★ 为什么杀的是句柄不是名字
//!
//! `admit_destructive` 回的是 `#{session_id}`（tmux 的 `$N`，server 生命周期内唯一、不复用）。
//! 之后 `kill-session -t '$3'`：名字在窗口期内被重新绑定到别的会话也**杀不到别人**。
//! 这与 `super::gate` 头注那段 TOCTOU 分析是同一条纪律 —— **破坏性动作尤其不能对名字下手。**
//!
//! # 错误码
//!
//! 命令级（本模块 / `gate`）：`invalid_args` · `no_tmux` · `no_such_session` ·
//! `wrong_owner`（Gate 2 不通过）· `too_many_windows`（Gate 3 不通过）· `kill_failed`。

use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::gate`] 同型。
type CmdErr = (&'static str, String);

/// 从入方向的 `args` 取出会话名并做**形状**校验。
///
/// 与 `launch::parse_request` 同一条纪律：这**不是安全边界**（对端本来就能在这台机上跑任意命令），
/// 而是「这组参数能不能构成一次有意义的 tmux 调用」。
/// `:` / `=` 是 tmux 目标语法的一部分，名字里带它们会让 `=name:` 变成别的意思。
pub(crate) fn parse_name(args: &serde_json::Value) -> Result<String, CmdErr> {
    let name = args
        .as_object()
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .ok_or(("invalid_args", "缺 `name`".to_string()))?;
    if name.trim().is_empty() {
        return Err(("invalid_args", "`name` 为空".to_string()));
    }
    if name.chars().any(char::is_control) {
        return Err(("invalid_args", "`name` 含控制字符".to_string()));
    }
    if name.contains(':') || name.contains('=') {
        return Err((
            "invalid_args",
            format!("`name` 不许含 `:` 或 `=`（它们是 tmux 目标语法）：{name:?}"),
        ));
    }
    Ok(name.to_string())
}

/// 真做事：过三道门 → 对**句柄**下 `kill-session`。
pub(crate) fn run(name: &str) -> Result<(), CmdErr> {
    let target = super::launch::exact_target(name);
    // ★ Gate 1（`=name:` 精确匹配，`exact_target` 内部）· Gate 2（身份）· Gate 3（windows==1）
    //   ⇒ 通过后拿到句柄。**顺序不可反**：门在 kill 之前，由
    //   `the_kill_path_admits_before_it_kills` 钉住。
    let handle = super::gate::admit_destructive(name, &target)?;
    let out = Command::new("tmux")
        .args(["kill-session", "-t", &handle])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                format!("起不来 tmux（远端装了吗？PATH 里有吗？）：{e}"),
            )
        })?;
    if out.status.success() {
        return Ok(());
    }
    Err((
        "kill_failed",
        format!(
            "kill-session 失败（会话已过门但没杀成）：{}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    ))
}

/// 入方向命令的入口：`args` → 结局 JSON。
pub(crate) fn kill_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let name = parse_name(args).map_err(|(c, m)| (c.to_string(), m))?;
    run(&name).map_err(|(c, m)| (c.to_string(), m))?;
    Ok(serde_json::json!({ "session": name, "killed": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shape_validation_rejects_what_would_break_the_tmux_target() {
        for (bad, why) in [
            (json!({}), "缺 name"),
            (json!({ "name": "" }), "空"),
            (json!({ "name": "  " }), "只有空白"),
            (json!({ "name": "a:b" }), "含 `:`（tmux 目标语法）"),
            (json!({ "name": "=a" }), "含 `=`（tmux 目标语法）"),
            (json!({ "name": "a\nb" }), "含控制字符"),
        ] {
            assert!(parse_name(&bad).is_err(), "{why} 应当被拒：{bad:?}");
        }
        assert_eq!(
            parse_name(&json!({ "name": "proj-cc" })).unwrap(),
            "proj-cc"
        );
    }

    /// ★ **生产接线（顺序钉）**：门必须在 `kill-session` **之前**。
    ///
    /// 反过来（先杀再判）＝ 门形同虚设，而「函数被调用了」这种判据照样绿。
    /// 同 `launch.rs::the_send_into_arm_admits_before_it_types` 一族 ——
    /// **破坏性动作的顺序错法后果最重**，所以单独钉。
    #[test]
    fn the_kill_path_admits_before_it_kills() {
        let src = crate::guard_support::production_code(include_str!("kill.rs"));
        let admit = src
            .find("gate::admit_destructive")
            .expect("生产段里没有 `gate::admit_destructive` —— 这条 kill 没过门");
        let verb = format!("kill-{}", "session");
        let act = src
            .find(verb.as_str())
            .expect("生产段里没有 kill-session —— 抽取坏了");
        assert!(
            admit < act,
            "`kill-session` 排在过门之前 —— 先杀再判，门就没意义了"
        );
        // 杀的必须是 `admit_destructive` 回的**句柄**，不是名字。
        assert!(
            src.contains("\"-t\", &handle"),
            "kill 的目标不是过门时拿到的句柄 —— 对名字下手就把 TOCTOU 窗口放回来了，\
             而这是**破坏性**动作"
        );
    }

    /// ★ Gate 3 只给破坏性动作：本模块用 `admit_destructive`，**不是** `admit`。
    #[test]
    fn kill_uses_the_destructive_gate_not_the_plain_one() {
        let src = crate::guard_support::production_code(include_str!("kill.rs"));
        assert!(
            src.contains("admit_destructive"),
            "kill 必须走带 Gate 3 的那个门"
        );
        // 运行时拼，免得命中上一行自己。
        let plain = format!("gate::admit({}", "");
        assert!(
            !src.contains(plain.as_str()),
            "kill 走了非破坏性的 `admit`（没有 Gate 3）—— 多窗口会话会被误杀"
        );
    }
}
