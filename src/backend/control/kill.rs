//! 结束一个终端（改状态的 tmux 命令一律归 `control/`）：`terminal-end` 按名单里的句柄认那一行，tmux 那一形按那一行的句柄过身份门与窗口门（Gate 2/3，
//! [`super::gate::admit_destructive`]）、argv 直传不过 shell；托管那一形不经 tmux、挂断子进程组。批量停（`sessions-stop`）· 换号重启停旧那一步同一条。
//!
//! # 杀的是句柄不是名字
//!
//! `admit_destructive` 回的令牌里是 `#{session_id}`（tmux 的 `$N`，server 生命周期内唯一、不复用），之后 `kill-session -t '$3'`：
//! 名字在窗口期内被重新绑定到别的会话也杀不到别人（同 `super::gate` 头注的 TOCTOU 分析）。
//!
//! # 错误码
//!
//! 门与 tmux 那一下（本模块 / `gate`）：`bad_args` · `no_tmux` · `no_such_session` ·
//! `wrong_owner`（Gate 2 不通过）· `too_many_windows`（Gate 3 不通过）· `kill_failed`。

use copy_core::copy_text;

/// 命令级错误：码 ＋ 那一句 ＋ 下层原话（tmux 的 stderr · 起不来时的系统报错，进复制详情）。与 [`super::launch`] 同型。
use crate::common::said::Said;

/// 结束会话被拒成 `code` 时给人看的那一句（批量停那一项也照它说）。`target` 是 tmux 会话名。
pub fn said(target: &str, code: &str) -> String {
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

/// 已有会话名的 Gate 1（送键那一路 `launch::parse_request` 用；结束按名单句柄，不按名字）：规则只有一份 `crate::control::gate_rules::existing_tmux_name_issue`
/// （空 · 控制符 · 视觉欺骗字符 · `:`）；本处只按它出声。
pub(crate) fn admit_existing_name(name: &str) -> Result<(), Said> {
    use crate::control::gate_rules::TmuxNameIssue as I;
    match crate::control::gate_rules::existing_tmux_name_issue(name) {
        None => Ok(()),
        Some(I::TargetSyntax(':')) => Err(Said::new(
            "bad_args",
            copy_text("beKill.name.colon", &[("name", &format!("{name:?}"))]),
        )),
        Some(I::Empty) => Err(Said::new("bad_args", copy_text("beKill.name.empty", &[]))),
        Some(I::Control(_)) => Err(Said::new("bad_args", copy_text("beKill.name.control", &[]))),
        Some(I::Deceptive(c)) => Err(Said::new(
            "bad_args",
            copy_text(
                "beKill.name.deceptive",
                &[("cp", &format!("U+{:04X}", c as u32))],
            ),
        )),
        Some(other) => Err(Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!("unexpected name issue: {other:?}")),
        )),
    }
}

/// 一发 tmux（`launch::run_with` 那一口的形状）。
type Tmux<'a> = dyn Fn(&[&str]) -> Result<super::launch::Ran, Said> + 'a;

/// 结束会话的两种令牌：门放行的（`terminal-end`），或本趟刚建、启动命令没送进的那个空壳（`ccm` 收掉它）。
pub(crate) enum Ending<'a> {
    /// 门放行的那一个，连同问的是哪台 tmux（与列名单、过门同一台）。
    Doomed(crate::host::tmux::On<'a>, super::gate::Doomed),
    /// 只在仍是空壳时收；`tmux` 同 `launch::run_with` 那一口（判据交隔离 socket 上的）。
    Bare(super::launch::JustMade, &'a Tmux<'a>),
}

/// 结束会话的唯一一处：只收令牌（[`Ending`]），对令牌里的目标下手。
pub(crate) fn end(ending: Ending<'_>) -> Result<(), Said> {
    let (on, doomed) = match ending {
        Ending::Doomed(on, d) => (on, d),
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
    #[expect(
        clippy::disallowed_methods,
        reason = "`kill-session` / `kill-pane`：破坏性，但改的是 tmux server 的运行期状态、不写文件，且先过三道门（单窗口）；本文件只这一处"
    )]
    let out = crate::host::tmux::child(on.socket)
        .args(&argv)
        .run(crate::host::tmux::TMUX_WITHIN)
        .map_err(tmux_unavailable)?;
    if out.status.success() {
        return Ok(());
    }
    Err(kill_refused(&out.stderr))
}

/// [`end`] 的空壳那一臂。判与收在同一发 tmux 里（`if-shell -F`）：只一个窗口、一个窗格、窗格起的是默认 shell
/// （`pane_start_command` 空）、没有客户端接着 ⇒ `kill-session`；别的形一律不动。不看窗格前台是什么：shell 刚起时
/// rc 里的子进程会短暂占着前台（实测 40 趟里 4 趟）。收成 ⇒ `Ok`；不在 · 不是空壳 ⇒ `kill_failed`。
fn end_bare(t: &str, tmux: &Tmux<'_>) -> Result<(), Said> {
    const BARE: &str = "#{&&:#{==:#{session_windows},1},#{&&:#{==:#{window_panes},1},#{&&:#{==:#{pane_start_command},},#{==:#{session_attached},0}}}}";
    let there = || matches!(tmux(&["has-session", "-t", t]), Ok(r) if r.ok);
    let kill = format!("kill-session -t {t}");
    if there()
        && matches!(tmux(&["if-shell", "-F", "-t", t, BARE, &kill]), Ok(r) if r.ok)
        && !there()
    {
        return Ok(());
    }
    Err(crate::wire::spec::fail(
        "kill_failed",
        copy_text("beKill.run.failed", &[]),
    ))
}

/// tmux 这个程序起不来：那一句住宿主口（句子只说原因词，系统原话进复制详情；超时那一档照旧）。
use crate::host::tmux::tmux_unavailable;

/// tmux 起来了、但没杀成：句子说「tmux 报错」，tmux 自己说的那句进复制详情。
pub(crate) fn kill_refused(stderr: &[u8]) -> Said {
    crate::wire::spec::fail("kill_failed", copy_text("beKill.run.failed", &[]))
        .and_raw(Some(&String::from_utf8_lossy(stderr)))
}

// ── `terminal-end`：按名单里的句柄结束一个终端，两种宿主同一条 ─────────────────────

/// `terminal-end` 的入参：目标恰给一个（名单里的句柄 · 会话 id），同 `terminal-input`。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct TerminalEndReq {
    /// 目标：名单里的不透明句柄（与 `sid` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub terminal: Option<String>,
    /// 目标：会话 id（与 `terminal` 恰给一个）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sid: Option<String>,
    /// 自报的前端（过「哪个前端的会话」那一维）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub client: Option<String>,
}

pub use cmd_wire::session::TerminalEnded;

crate::wire::typed::codes! {
    /// `terminal-end` 的码：认目标 · 列名单 · 过门 · 结束那一下。
    pub enum TerminalEndErr {
        /// 目标没恰给一个（或不是非空短串）。
        BadTarget = "bad_target",
        /// 名单里此刻没有它。
        NotKnown = "not_known",
        /// 不止一个终端挂着这个 sid。
        Ambiguous = "ambiguous",
        /// 没有这个会话（tmux：过门那一刻它已经没了）。
        NoSuchSession = "no_such_session",
        /// 身份门不过（不是我们起的 · 别的前端的）。
        WrongOwner = "wrong_owner",
        /// 窗口门不过（tmux 那个会话里还有别的窗口）。
        TooManyWindows = "too_many_windows",
        /// 没结束成（原话进复制详情）。
        KillFailed = "kill_failed",
        /// 列名单失败。
        Unobservable = "unobservable",
        /// 那一发 `tmux` 过了期限。
        ChildTimedOut = "child_timed_out",
    }
}

/// `terminal-end`：结束名单里的一个终端（tmux：三道门、杀句柄；托管：子进程组挂断）。阻塞档。
pub struct TerminalEnd;

impl crate::wire::typed::Command for TerminalEnd {
    const NAME: &'static str = "terminal-end";
    type Req = TerminalEndReq;
    type Reply = TerminalEnded;
    type Err = TerminalEndErr;
}

impl crate::wire::typed::Handle for TerminalEnd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: TerminalEndReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<TerminalEnded, TerminalEndErr> {
        end_terminal(
            crate::host::tmux::On::default(),
            crate::host::managed::installed(),
            &req,
        )
        .map_err(|f| match f.code.as_deref() {
            // 同 `terminals-list`：tmux 装了却起不来 ⇒ 列不出（不另立 `no_tmux`：没 tmux 照样结束托管的）。
            Some("no_tmux") => {
                TerminalEndErr::Unobservable(crate::common::contract::joined(f.said, f.raw))
            }
            _ => crate::wire::typed::of_fail(f, TerminalEndErr::KillFailed),
        })
    }
}

/// `terminal-end` 的本体：名单里对上那一行（两种宿主合一份；`managed` ＝ 托管名单的家，生产是装好的那一个）⇒ 按它的宿主结束。
/// 托管那一行不靠 tmux（这台没装 tmux 也照结束）；tmux 那一行过三道门，过门那一刻 tmux 起不来 ⇒ `kill_failed`（不回 `no_tmux`：名单刚列出它）。
pub fn end_terminal(
    on: crate::host::tmux::On<'_>,
    managed: Option<&crate::platform::fs::Own>,
    req: &TerminalEndReq,
) -> Result<TerminalEnded, Said> {
    let list = || {
        crate::host::list_with(on, &|| {
            managed
                .map(crate::host::managed::listed_at)
                .unwrap_or_default()
        })
    };
    end_listed(on, managed, &list, req)
}

/// [`end_terminal`] 的本体，列名单那一下由调用方给（生产 = 两种宿主合一份；判据交一个头一次漏掉它的）。
pub(crate) fn end_listed(
    on: crate::host::tmux::On<'_>,
    managed: Option<&crate::platform::fs::Own>,
    list: &dyn Fn() -> Result<crate::host::tmux::Listed, Said>,
    req: &TerminalEndReq,
) -> Result<TerminalEnded, Said> {
    use super::terminals::{find_one, target};
    let t = target(req.terminal.as_deref(), req.sid.as_deref())?;
    let requester = super::gate::requester(req.client.as_deref())?;
    // 托管名单里认得出它（句柄 · 挂着那个 sid 的唯一一个托管终端）⇒ 只读托管名单、挂断它，一发 tmux 都不起（这台的 tmux 卡住也照结束）。
    // 一个 sid 只在一处跑：托管里认出来了就不再去 tmux 那边找第二个。
    if let Some(own) = managed {
        let (hosted, _) = crate::host::managed::listed_at(own);
        if let Ok(row) = find_one(&hosted, &t) {
            end_row(on, managed, row, requester.as_deref())?;
            return Ok(TerminalEnded {
                terminal: row.handle(),
            });
        }
    }
    let not_known = |f: &Said| f.code.as_deref() == Some("not_known");
    let (mut rows, _, _) = list()?;
    // 名单里一时没有它（刚起 · 列的那一刻 tmux 慢）⇒ 当场重读一次再判。
    if find_one(&rows, &t).is_err_and(|f| not_known(&f)) {
        rows = list()?.0;
    }
    let row = find_one(&rows, &t).map_err(|f| {
        if not_known(&f) {
            // 给人看的那一句说「已不在」；`find_one` 那一句（诊断）留在复制详情。
            let asked = req
                .terminal
                .as_deref()
                .or(req.sid.as_deref())
                .unwrap_or_default();
            Said {
                said: copy_text("tmuxControl.kill.noSuchSession", &[("target", asked)]),
                ..f
            }
        } else {
            f
        }
    })?;
    end_row(on, managed, row, requester.as_deref())?;
    Ok(TerminalEnded {
        terminal: row.handle(),
    })
}

/// 结束名单里的那一行（[`end_terminal`] 认完目标之后）：tmux 那一形按这一行的句柄（`row.id`）在列名单的同一台 tmux 上过三道门、
/// 杀句柄（`gate::admit_destructive`；不按名字再认一次）；托管那一形按行事实过身份门、挂断子进程组。
pub(crate) fn end_row(
    on: crate::host::tmux::On<'_>,
    managed: Option<&crate::platform::fs::Own>,
    row: &crate::host::TermRow,
    requester: Option<&str>,
) -> Result<(), Said> {
    match row.host {
        crate::wire::TerminalHost::Tmux => {
            super::gate::admit_destructive(on, row, requester)
                .and_then(|doomed| end(Ending::Doomed(on, doomed)))
                .map_err(|f| {
                    if f.code.as_deref() == Some("no_tmux") {
                        crate::wire::spec::fail("kill_failed", copy_text("beKill.run.failed", &[]))
                            .and_raw(Some(&f.said))
                    } else {
                        f
                    }
                })?;
        }
        crate::wire::TerminalHost::Managed => {
            if super::gate::identity_of(row, requester) != super::gate::Who::Pass {
                return Err(crate::wire::spec::fail(
                    "wrong_owner",
                    copy_text("tmuxControl.kill.wrongOwner", &[("target", &row.name)]),
                ));
            }
            // 名单里有托管那一行 ⇒ 托管名单的家装着（`managed` 有值）。
            let done = match managed {
                Some(own) => crate::host::managed::end_at(own, &row.id),
                None => Err(std::io::ErrorKind::NotFound.into()),
            };
            done.map_err(|e| {
                crate::wire::spec::fail("kill_failed", copy_text("beKill.run.failed", &[]))
                    .and_raw(Some(&e.to_string()))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/backend/control/kill_tests.rs"]
mod tests;
