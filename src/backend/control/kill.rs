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
//! # 〔SH1 · D-g〕杀成之后顺手从 cc-bus 收掉登记在这个会话上的 id
//!
//! 杀之前读下全部 pane 的根进程 pid，杀成之后按 `agents.tsv` 第 4 列那个 pid 认人、逐个 `cc-kill`（`cc_bus::unregister_panes`）；
//! 那一步失败只 warn，不改杀会话的结局，应答形状不变。
//!
//! # 错误码
//!
//! 命令级（本模块 / `gate`）：`invalid_args` · `no_tmux` · `no_such_session` ·
//! `wrong_owner`（Gate 2 不通过）· `too_many_windows`（Gate 3 不通过）· `kill_failed`。

use copy_core::copy_text;
use std::process::{Command, Stdio};

use crate::common::tmux_utf8::UTF8_CLIENT_FLAG;

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::gate`] 同型。
type CmdErr = (&'static str, String);

/// 从入方向的 `args` 取出会话名并做**形状**校验。
///
/// 与 `launch::parse_request` 同一条纪律：这**不是安全边界**（对端本来就能在这台机上跑任意命令），
/// 而是「这组参数能不能构成一次有意义的 tmux 调用」。
pub(crate) fn parse_name(args: &serde_json::Value) -> Result<String, CmdErr> {
    let name = args
        .as_object()
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .ok_or((
            "invalid_args",
            crate::common::contract::malformed("missing `name`"),
        ))?;
    admit_existing_name(name)?;
    Ok(name.to_string())
}

/// 〔TAIL · DUP3 §5 ③ ⑦〕**已有会话名**的 Gate 1（结束 · 抓屏 · 送键共用）：规则只有一份
/// `gate_core::existing_tmux_name_issue`（空 · 控制符 · 视觉欺骗字符），外加 `:`（tmux 目标语法的分隔符，真会话名里不会有）。
/// `=` 不拒：`=a=b:` 精确命中名叫 `a=b` 的会话，attach 那一条早就放行它。
pub(crate) fn admit_existing_name(name: &str) -> Result<(), CmdErr> {
    use gate_core::TmuxNameIssue as I;
    match gate_core::existing_tmux_name_issue(name) {
        None if name.contains(':') => Err((
            "invalid_args",
            copy_text("beKill.name.colon", &[("name", &format!("{name:?}"))]),
        )),
        None => Ok(()),
        Some(I::Empty) => Err(("invalid_args", copy_text("beKill.name.empty", &[]))),
        Some(I::Control(_)) => Err(("invalid_args", copy_text("beKill.name.control", &[]))),
        Some(I::Deceptive(c)) => Err((
            "invalid_args",
            copy_text(
                "beKill.name.deceptive",
                &[("cp", &format!("U+{:04X}", c as u32))],
            ),
        )),
        Some(other) => Err((
            "invalid_args",
            crate::common::contract::malformed(&format!("unexpected name issue: {other:?}")),
        )),
    }
}

/// 真做事：过三道门 → 对**句柄**下 `kill-session`。
pub(crate) fn run(name: &str) -> Result<(), CmdErr> {
    let target = super::launch::exact_target(name);
    // ★ Gate 1（`=name:` 精确匹配，`exact_target` 内部）· Gate 2（身份）· Gate 3（windows==1）
    //   ⇒ 通过后拿到句柄。**顺序不可反**：门在 kill 之前，由
    //   `the_kill_path_admits_before_it_kills` 钉住。
    let handle = super::gate::admit_destructive(name, &target)?;
    // 〔SH1 · D-g〕杀之前记下这个会话全部 pane 的根进程 pid：杀完按它认 cc-bus 名册里登记在这里的 id（不按会话名猜）。
    let panes = pane_pids(&handle);
    let out = Command::new("tmux")
        .args(["kill-session", "-t", &handle])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            (
                "no_tmux",
                copy_text("beKill.run.noTmux", &[("e", &e.to_string())]),
            )
        })?;
    if out.status.success() {
        super::cc_bus::unregister_panes(name, &panes);
        return Ok(());
    }
    Err((
        "kill_failed",
        copy_text(
            "beKill.run.failed",
            &[("e", String::from_utf8_lossy(&out.stderr).trim())],
        ),
    ))
}

/// 〔SH1 · D-g〕这个会话（句柄）全部 pane 的根进程 pid。只读 tmux；问不到 ⇒ 空（顺手注销那一步随之不做，不影响杀）。
/// `INVARIANTS §49`：argv 直传 ⇒ UTF-8 旗排在子命令前（读的虽是数字，照表带）。
fn pane_pids(handle: &str) -> Vec<u32> {
    Command::new("tmux")
        .args([
            UTF8_CLIENT_FLAG,
            "list-panes",
            "-F",
            "#{pane_pid}",
            "-s",
            "-t",
            handle,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
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
