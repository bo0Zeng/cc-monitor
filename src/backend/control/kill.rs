//! 杀一个 tmux 会话（改状态的 tmux 命令一律归 `control/`）。argv 直传、不过 shell，三道门（Gate 1/2/3）由
//! [`super::gate::admit_destructive`] 过；界面经通道直接说本命令（`src/frontend/ui/tmux-control.ts::killSession`）。
//!
//! # 杀的是句柄不是名字
//!
//! `admit_destructive` 回的令牌里是 `#{session_id}`（tmux 的 `$N`，server 生命周期内唯一、不复用），之后 `kill-session -t '$3'`：
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

/// 命令级错误：码 ＋ 那一句 ＋ 下层原话（tmux 的 stderr · 起不来时的系统报错，进复制详情）。与 [`super::launch`] 同型。
type CmdErr = crate::wire::spec::Fail;

/// `kill` 的入参（`deny_unknown_fields`）。会话名的**形状**在 [`admit_existing_name`] 判：
/// 与 `launch` 同一条纪律，这**不是安全边界**（对端本来就能在这台机上跑任意命令），
/// 而是「这组参数能不能构成一次有意义的 tmux 调用」。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct KillReq {
    /// 要杀的 tmux 会话名。
    pub(crate) name: String,
    /// 自报的前端（过「哪个前端的会话」那一维）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) client: Option<String>,
    /// 可带：只结束挂着这个会话（`@ccm_sid`）的窗格（1–128 个 `[A-Za-z0-9_-]`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional))]
    pub(crate) sid: Option<String>,
}

/// `kill` 的回包：线上形住 `cmd-wire::session`。
pub(crate) use cmd_wire::session::Killed;

crate::wire::typed::codes! {
    /// `kill` 的码：三道门 · 杀那一发 tmux。
    pub(crate) enum KillErr {
        /// tmux 起不来。
        NoTmux = "no_tmux",
        /// 没有这个会话（或带了 `sid`、此刻没有窗格挂着它）。
        NoSuchSession = "no_such_session",
        /// 身份门不过（不是我们起的 · 别的前端的）。
        WrongOwner = "wrong_owner",
        /// 窗口门不过（那个会话里还有别的窗口）。
        TooManyWindows = "too_many_windows",
        /// tmux 起来了、但没杀成（tmux 原话进复制详情）。
        KillFailed = "kill_failed",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `kill`：杀一个 tmux 会话（破坏性：三道门在 `gate::admit_destructive`）。阻塞档（起 tmux）。
pub(crate) struct Kill;

impl crate::wire::typed::Command for Kill {
    const NAME: &'static str = "kill";
    type Req = KillReq;
    type Reply = Killed;
    type Err = KillErr;
}

impl crate::wire::typed::Handle for Kill {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: KillReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Killed, KillErr>> + Send + 'static {
        async move { kill(&req).map_err(|f| refused(&req.name, f)) }
    }
}

/// 下层那一份失败 ⇒ `kill` 的码，那一句换成给人看的（[`said`]）、下层那句进原话。
pub(crate) fn refused(name: &str, f: CmdErr) -> KillErr {
    crate::wire::typed::of_fail_said(f, KillErr::KillFailed, |c| Some(said(name, c)))
}

/// 结束会话被拒成 `code` 时给人看的那一句（批量停那一项也照它说）。`target` 是 tmux 会话名。
pub(crate) fn said(target: &str, code: &str) -> String {
    match code {
        "bad_args" => copy_text("tmuxControl.kill.badName", &[("target", target)]),
        "no_tmux" => copy_text("tmuxControl.kill.noTmux", &[("target", target)]),
        "no_such_session" => copy_text("tmuxControl.kill.noSuchSession", &[("target", target)]),
        "wrong_owner" => copy_text("tmuxControl.kill.wrongOwner", &[("target", target)]),
        "too_many_windows" => copy_text("tmuxControl.kill.tooManyWindows", &[("target", target)]),
        "kill_failed" => copy_text("tmuxControl.kill.failed", &[("target", target)]),
        "child_timed_out" => copy_text("tmuxControl.kill.childTimedOut", &[("target", target)]),
        _ => copy_text("tmuxControl.kill.otherCode", &[("target", target)]),
    }
}

/// 已有会话名的 Gate 1（结束 · 抓屏 · 送键共用）：规则只有一份 `crate::control::gate_rules::existing_tmux_name_issue`
/// （空 · 控制符 · 视觉欺骗字符），外加 `:`（tmux 目标语法的分隔符，真会话名里不会有）。
/// `=` 不拒：`=a=b:` 精确命中名叫 `a=b` 的会话。
pub(crate) fn admit_existing_name(name: &str) -> Result<(), CmdErr> {
    use crate::control::gate_rules::TmuxNameIssue as I;
    match crate::control::gate_rules::existing_tmux_name_issue(name) {
        None if name.contains(':') => Err(CmdErr::from((
            "bad_args",
            copy_text("beKill.name.colon", &[("name", &format!("{name:?}"))]),
        ))),
        None => Ok(()),
        Some(I::Empty) => Err(CmdErr::from((
            "bad_args",
            copy_text("beKill.name.empty", &[]),
        ))),
        Some(I::Control(_)) => Err(CmdErr::from((
            "bad_args",
            copy_text("beKill.name.control", &[]),
        ))),
        Some(I::Deceptive(c)) => Err(CmdErr::from((
            "bad_args",
            copy_text(
                "beKill.name.deceptive",
                &[("cp", &format!("U+{:04X}", c as u32))],
            ),
        ))),
        Some(other) => Err(CmdErr::from((
            "bad_args",
            crate::common::contract::malformed(&format!("unexpected name issue: {other:?}")),
        ))),
    }
}

/// 真做事：过三道门 → 对**句柄**下 `kill-session`。`requester` ＝ 请求自报的前端（`gate::identity`）。
/// 按 `sid` 认：要求此刻有窗格挂着它（批量停按 sid 认出这个名字，认完名字换了人 ⇒ 不杀）；
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
    // Gate 1（`=name:` 精确匹配，`exact_target` 内部）· Gate 2（身份）· Gate 3（windows==1）⇒ 通过后拿到令牌。
    let doomed = super::gate::admit_destructive(name, &target, sid, requester)?;
    end(Ending::Doomed(name, doomed))
}

/// 一发 tmux（`launch::run_with` 那一口的形状）。
type Tmux<'a> = dyn Fn(&[&str]) -> Result<super::launch::Ran, CmdErr> + 'a;

/// 结束会话的两种令牌：门放行的（`kill`），或本趟刚建、启动命令没送进的那个空壳（`ccm` 收掉它）。
pub(crate) enum Ending<'a> {
    /// `name`：说给人听的会话名（杀成之后按它认 cc-bus 名册）。
    Doomed(&'a str, super::gate::Doomed),
    /// 只在仍是空壳时收；`tmux` 同 `launch::run_with` 那一口（判据交隔离 socket 上的）。
    Bare(super::launch::JustMade, &'a Tmux<'a>),
}

/// 结束会话的唯一一处：只收令牌（[`Ending`]），对令牌里的目标下手。
pub(crate) fn end(ending: Ending<'_>) -> Result<super::cc_bus::BusCleanup, CmdErr> {
    let (name, doomed) = match ending {
        Ending::Doomed(n, d) => (n, d),
        Ending::Bare(made, tmux) => return end_bare(made.target(), tmux),
    };
    let (handle, only) = match doomed.at() {
        super::gate::EndAt::Session(h) => (h.as_str(), None),
        super::gate::EndAt::Panes(h, ps) => (h.as_str(), Some(ps.as_slice())),
    };
    let mut argv: Vec<&str> = Vec::new();
    match only {
        None => argv.extend(["kill-session", "-t", handle]),
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
    let panes = pane_pids(handle, only);
    let out = Child::new("tmux")
        .args(&argv)
        .run(KILL_TMUX_WITHIN)
        .map_err(tmux_unavailable)?;
    if out.status.success() {
        let bus = super::cc_bus::unregister_panes(name, &panes);
        return Ok(bus);
    }
    Err(kill_refused(&out.stderr))
}

/// [`end`] 的空壳那一臂。判与收在同一发 tmux 里（`if-shell -F`）：只一个窗口、一个窗格、窗格起的是默认 shell
/// （`pane_start_command` 空）、没有客户端接着 ⇒ `kill-session`；别的形一律不动。不看窗格前台是什么：shell 刚起时
/// rc 里的子进程会短暂占着前台（实测 40 趟里 4 趟）。收成 ⇒ 空的注销结局（空壳上没有登记）；不在 · 不是空壳 ⇒ `kill_failed`。
fn end_bare(t: &str, tmux: &Tmux<'_>) -> Result<super::cc_bus::BusCleanup, CmdErr> {
    const BARE: &str = "#{&&:#{==:#{session_windows},1},#{&&:#{==:#{window_panes},1},#{&&:#{==:#{pane_start_command},},#{==:#{session_attached},0}}}}";
    let there = || matches!(tmux(&["has-session", "-t", t]), Ok(r) if r.ok);
    let kill = format!("kill-session -t {t}");
    if there()
        && matches!(tmux(&["if-shell", "-F", "-t", t, BARE, &kill]), Ok(r) if r.ok)
        && !there()
    {
        return Ok(Default::default());
    }
    Err(CmdErr::new(
        "kill_failed",
        copy_text("beKill.run.failed", &[]),
    ))
}

/// tmux 这个程序起不来：句子只说原因词（未装 / 无权限 …），系统原话进复制详情；超时那一档照旧。
pub(crate) fn tmux_unavailable(e: crate::platform::child::ChildFail) -> CmdErr {
    CmdErr::from(e.into_cmd_said("no_tmux", |why| {
        copy_text("reason.tmux.startFailed", &[("why", why)])
    }))
}

/// tmux 起来了、但没杀成：句子说「tmux 报错」，tmux 自己说的那句进复制详情。
pub(crate) fn kill_refused(stderr: &[u8]) -> CmdErr {
    CmdErr::new("kill_failed", copy_text("beKill.run.failed", &[]))
        .with_raw(Some(&String::from_utf8_lossy(stderr)))
}

/// 要走的那几个 pane 的根进程 pid（`only` ＝ 只结束这几个窗格；`None` ＝ 整个会话）。问不到 ⇒ 空（顺手注销那一步随之不做，不影响杀）。
fn pane_pids(handle: &str, only: Option<&[String]>) -> Vec<u32> {
    super::gate::panes_on(None, handle)
        .into_iter()
        .filter(|p| only.is_none_or(|ps| ps.contains(&p.pane)))
        .map(|p| p.pid)
        .collect()
}

/// `kill` 的本体：会话名过 Gate 1 的形状 · 前端名 · sid 形状 ⇒ 过门 ⇒ 杀 ⇒ 成品。
pub(crate) fn kill(req: &KillReq) -> Result<Killed, CmdErr> {
    admit_existing_name(&req.name)?;
    let client = super::gate::requester(req.client.as_deref())?;
    // 带了 `sid` ⇒ 结束挂着它的那个窗格（会话里还有别的 claude 窗格时不关整个会话）。
    let sid = super::gate::sid_shape(req.sid.as_deref())?;
    let bus = run_expecting(&req.name, sid.as_deref(), client.as_deref())?;
    Ok(reply(&req.name, &bus))
}

/// 帧面成品 `{session, killed, bus}` 的构造器（纯）：跨语言金样 `tests/__fixtures__/tmux-control.golden.json` 拿**同一个**构造器对拍，
/// 界面（`src/frontend/ui/tmux-control.ts::killSession`）直接收这份成品，monitor 那一跳只搬字节。
pub(crate) fn reply(name: &str, bus: &super::cc_bus::BusCleanup) -> Killed {
    Killed {
        session: name.to_string(),
        killed: true,
        bus: bus.note(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/kill_tests.rs"]
mod tests;
