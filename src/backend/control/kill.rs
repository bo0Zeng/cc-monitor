//! 杀一个 tmux 会话（改状态的 tmux 命令一律归 `control/`）。argv 直传、不过 shell，三道门（Gate 1/2/3）由
//! [`super::gate::admit_destructive`] 过；界面经通道直接说本命令（`src/frontend/ui/tmux-control.ts::killSession`）。
//!
//! # 杀的是句柄不是名字
//!
//! `admit_destructive` 回的是 `#{session_id}`（tmux 的 `$N`，server 生命周期内唯一、不复用），之后 `kill-session -t '$3'`：
//! 名字在窗口期内被重新绑定到别的会话也杀不到别人（同 `super::gate` 头注的 TOCTOU 分析）。
//!
//! # 杀成之后顺手从 cc-bus 收掉登记在这个会话上的 id
//!
//! 杀之前读下全部 pane 的根进程 pid，杀成之后按 `agents.tsv` 第 4 列那个 pid 认人、逐个 `cc-kill`（`cc_bus::unregister_panes`）；
//! 那一步失败不改杀会话的结局；注销的结局进成品的 `bus` 那一格（注销了谁 · 谁没注销成 · 名册读不到），界面说一句。
//!
//! # 错误码
//!
//! 命令级（本模块 / `gate`）：`bad_args` · `no_tmux` · `no_such_session` ·
//! `wrong_owner`（Gate 2 不通过）· `too_many_windows`（Gate 3 不通过）· `kill_failed`。

use crate::platform::child::{Child, Deadline};
use copy_core::copy_text;

/// `kill-session` / `kill-pane` 那一发的期限：tmux 一发 5 s（同 watcher 探测 tmux 的期限）。
const KILL_TMUX_WITHIN: Deadline = Deadline::secs(5);

/// `kill` 整条命令总期限的上限（探身份 · 列窗格 · 杀 · 顺手注销 cc-bus 共用）。
pub(crate) const KILL_CAP: Deadline = Deadline::secs(8);

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
            "bad_args",
            crate::common::contract::malformed("missing `name`"),
        ))?;
    admit_existing_name(name)?;
    Ok(name.to_string())
}

/// 已有会话名的 Gate 1（结束 · 抓屏 · 送键共用）：规则只有一份 `crate::control::gate_rules::existing_tmux_name_issue`
/// （空 · 控制符 · 视觉欺骗字符），外加 `:`（tmux 目标语法的分隔符，真会话名里不会有）。
/// `=` 不拒：`=a=b:` 精确命中名叫 `a=b` 的会话。
pub(crate) fn admit_existing_name(name: &str) -> Result<(), CmdErr> {
    use crate::control::gate_rules::TmuxNameIssue as I;
    match crate::control::gate_rules::existing_tmux_name_issue(name) {
        None if name.contains(':') => Err((
            "bad_args",
            copy_text("beKill.name.colon", &[("name", &format!("{name:?}"))]),
        )),
        None => Ok(()),
        Some(I::Empty) => Err(("bad_args", copy_text("beKill.name.empty", &[]))),
        Some(I::Control(_)) => Err(("bad_args", copy_text("beKill.name.control", &[]))),
        Some(I::Deceptive(c)) => Err((
            "bad_args",
            copy_text(
                "beKill.name.deceptive",
                &[("cp", &format!("U+{:04X}", c as u32))],
            ),
        )),
        Some(other) => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("unexpected name issue: {other:?}")),
        )),
    }
}

/// 真做事：过三道门 → 对**句柄**下 `kill-session`。`requester` ＝ 请求自报的前端（`gate::identity`）。
pub(crate) fn run(
    name: &str,
    requester: Option<&str>,
) -> Result<super::cc_bus::BusCleanup, CmdErr> {
    run_expecting(name, None, requester)
}

/// 同 [`run`]，但按 `sid` 认：要求此刻有窗格挂着它（批量停按 sid 认出这个名字，认完名字换了人 ⇒ 不杀）；
/// 那个会话里还有别的 claude 窗格 ⇒ 只结束挂着它的窗格（`gate::admit_destructive` 说了算）。
pub(crate) fn run_as(
    name: &str,
    sid: &str,
    requester: Option<&str>,
) -> Result<super::cc_bus::BusCleanup, CmdErr> {
    run_expecting(name, Some(sid), requester)
}

fn run_expecting(
    name: &str,
    sid: Option<&str>,
    requester: Option<&str>,
) -> Result<super::cc_bus::BusCleanup, CmdErr> {
    let target = super::launch::exact_target(name);
    // Gate 1（`=name:` 精确匹配，`exact_target` 内部）· Gate 2（身份）· Gate 3（windows==1）⇒ 通过后拿到句柄。
    // 门在 kill 之前（`the_kill_path_admits_before_it_kills` 钉住）。
    let end = super::gate::admit_destructive(name, &target, sid, requester)?;
    // 结束整个会话，还是只结束挂着那个 sid 的几个窗格（会话里还跑着别的 claude）。都对放行时拿到的句柄下手。
    let (handle, only) = match end {
        super::gate::EndAt::Session(h) => (h, None),
        super::gate::EndAt::Panes(h, ps) => (h, Some(ps)),
    };
    let mut argv: Vec<&str> = Vec::new();
    match &only {
        None => argv.extend(["kill-session", "-t", &handle]),
        Some(ps) => {
            for (i, p) in ps.iter().enumerate() {
                if i > 0 {
                    argv.push(";");
                }
                argv.extend(["kill-pane", "-t", p.as_str()]);
            }
        }
    }
    // 杀之前记下要走的那几个 pane 的根进程 pid：杀完按它认 cc-bus 名册里登记在这里的 id（不按会话名猜）。
    let panes = pane_pids(&handle, only.as_deref());
    let out = Child::new("tmux")
        .args(&argv)
        .run(KILL_TMUX_WITHIN)
        .map_err(|e| {
            e.into_cmd_err("no_tmux", |e| {
                copy_text("beKill.run.noTmux", &[("e", &e.to_string())])
            })
        })?;
    if out.status.success() {
        let bus = super::cc_bus::unregister_panes(name, &panes);
        return Ok(bus);
    }
    Err((
        "kill_failed",
        copy_text(
            "beKill.run.failed",
            &[("e", String::from_utf8_lossy(&out.stderr).trim())],
        ),
    ))
}

/// 要走的那几个 pane 的根进程 pid（`only` ＝ 只结束这几个窗格；`None` ＝ 整个会话）。问不到 ⇒ 空（顺手注销那一步随之不做，不影响杀）。
fn pane_pids(handle: &str, only: Option<&[String]>) -> Vec<u32> {
    super::gate::panes_on(None, handle)
        .into_iter()
        .filter(|p| only.is_none_or(|ps| ps.contains(&p.pane)))
        .map(|p| p.pid)
        .collect()
}

/// 入方向命令的入口：`args` → 结局 JSON。
pub(crate) fn kill_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let name = parse_name(args).map_err(|(c, m)| (c.to_string(), m))?;
    let client = super::gate::requester_of(args).map_err(|(c, m)| (c.to_string(), m))?;
    // 带了 `sid` ⇒ 结束挂着它的那个窗格（会话里还有别的 claude 窗格时不关整个会话）。
    let sid = super::gate::sid_of(args).map_err(|(c, m)| (c.to_string(), m))?;
    let bus = run_expecting(&name, sid.as_deref(), client.as_deref())
        .map_err(|(c, m)| (c.to_string(), m))?;
    Ok(reply(&name, &bus))
}

/// 帧面成品 `{session, killed}` 的构造器 —— 从 [`kill_for_inbound`] 里原样抽出来（逻辑不动），
/// 只为让跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 拿**同一个**构造器对拍：
/// 界面（`src/frontend/ui/tmux-control.ts::killSession`）从此直接收这份成品，monitor 那一跳只搬字节。
pub(crate) fn reply(name: &str, bus: &super::cc_bus::BusCleanup) -> serde_json::Value {
    serde_json::json!({ "session": name, "killed": true, "bus": bus.to_json() })
}

#[cfg(test)]
#[path = "../../../tests/backend/control/kill_tests.rs"]
mod tests;
