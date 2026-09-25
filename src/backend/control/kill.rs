//! F04a：**杀一个 tmux 会话**（定框 C5：任何改状态的 tmux 命令一律归 `control/`）。
//!
//! # 它与 monitor 侧那条路的关系
//!
//! monitor 的 `kill_remote_tmux`（〔C4e〕已迁到界面 `src/tmux-control.ts::killSession`，经通道直接说本命令）当年拼一条穿过 ssh + shell 的原子命令，
//! 带 §34 的 **Gate 1/2/3**。本模块是它在后端侧的对应物：
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
    Ok(reply(&name))
}

/// 〔C4e · 第四波 4C〕帧面成品 `{session, killed}` 的构造器 —— 从 [`kill_for_inbound`] 里原样抽出来（逻辑不动），
/// 只为让跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 拿**同一个**构造器对拍：
/// 界面（`src/tmux-control.ts::killSession`）从此直接收这份成品，monitor 那一跳只搬字节。
pub(crate) fn reply(name: &str) -> serde_json::Value {
    serde_json::json!({ "session": name, "killed": true })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/kill_tests.rs"]
mod tests;
