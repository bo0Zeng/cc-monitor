//! 以人的身份代敲 pb 的命令（`continue` · `pause` · `adopt` · `render` · `repo`；`adopt` 接的不是当前仓库时先 `repo <仓库名>`；
//! `repo` 敲哪几个参数照 [`super::new_slice::repo_args`]，与新计划框写出的那一行同一判）：
//! 起法见 [`super::pb`]（当前目录设在工作区根）；认退出码，拆 `render` 那句话里的页面路径。
//! 这一侧自己带着 agent 的身份（在 agent 的 shell 里起的）⇒ 用户命令一律不代敲：pb 的用户命令只许人发起。
//! 人定了事不经这里：由会话里的 agent 写原话节点（pb 0.5 没有记原话的命令）。
//!
//! 这几条写盘的是 pb 自己（`continue` / `pause` 改 `auto_continue`，`repo` / `adopt` 改工作区配置，`render` 往系统临时目录写一页）；
//! 本族照旧一个字节都不写。拆 pb 原话只在 [`classify`] 这一处。

use crate::plugin::invoke::Done;
use cmd_wire::plan::{PlanAdopted, PlanCmdReply};
use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 界面能点的那几条（pb 的命令名原样）。
pub(crate) const VERBS: [&str; 5] = ["continue", "pause", "adopt", "render", "repo"];

/// 只许人发起的那几条（pb 的用户命令）：代敲只在这一侧不带 agent 身份时起。
const USER_VERBS: [&str; 4] = ["continue", "pause", "repo", "adopt"];

/// pb 认 agent 的那个环境变量（钩子递进 agent 的 shell）。
const AGENT_ENV: &str = "PB_ID";

/// pb 的退出码：拒 · 读不成 · 用法错。
const RC_REFUSED: i32 = 2;
const RC_UNREADABLE: i32 = 3;
const RC_USAGE: i32 = 4;

/// `render` 成了时 pb 说的那一句的开头（后面跟页面路径）。
const RENDER_WROTE: &str = "写了 ";

/// 跑一条（`args[0]` 是动词，帧面先判）；这一侧带不带 agent 身份照本进程的环境。
pub(crate) fn run(entry: &Path, ws: &Path, args: &[&str]) -> Result<PlanCmdReply, Said> {
    run_as(entry, ws, args, std::env::var_os(AGENT_ENV).is_some())
}

/// [`run`] 的本体：`agent` ＝ 这一侧带着 agent 的身份 ⇒ 用户命令不起 pb、回拒。
pub(crate) fn run_as(
    entry: &Path,
    ws: &Path,
    args: &[&str],
    agent: bool,
) -> Result<PlanCmdReply, Said> {
    let verb = args.first().copied().unwrap_or_default();
    if agent && USER_VERBS.contains(&verb) {
        return Err(Said::new(
            "refused",
            copy_text("bePlan.command.humanOnly", &[("verb", verb)]),
        ));
    }
    answer(verb, super::pb(entry, ws, args))
}

/// 起了一次 pb ⇒ 回包或拒（纯函数）：起不来 ⇒ `failed`；过了期限 ⇒ 「pb <动词> 超时」（等了多久照原语说的）；跑完了 ⇒ [`classify`]。
pub(crate) fn answer(verb: &str, r: Result<Done, Said>) -> Result<PlanCmdReply, Said> {
    match r {
        Err(s) => Err(s.coded("failed")),
        Ok(Done {
            waited_secs: Some(secs),
            ..
        }) => Err(Said::new(
            "failed",
            copy_text(
                "bePlan.command.timedOut",
                &[
                    ("verb", verb),
                    ("dur", &copy_core::format_duration(secs * 1000)),
                ],
            ),
        )),
        Ok(d) => classify(verb, d.code, &d.stdout, &d.stderr),
    }
}

/// 要接过来的那个仓库（成品里那一片的几格）。
pub(crate) struct Adopting<'a> {
    pub(crate) slice: &'a str,
    /// 仓库相对工作区根在哪。
    pub(crate) path: &'a str,
    pub(crate) docs_only: bool,
    /// 是不是当前仓库（pb 的 `adopt` 只接它）。
    pub(crate) current: bool,
}

/// 接过一个仓库：已经接过 ⇒ 不起 pb；不是当前仓库 ⇒ 先 `repo <仓库名>`（只给名字 ＝ 切过去，路径与领域照旧）再 `adopt`。
/// 回包多 `adopted`：接过来的是哪一个 · 仓库在哪 · 当前仓库换没换。
pub(crate) fn adopt_with(
    t: &Adopting,
    run: &dyn Fn(&[&str]) -> Result<PlanCmdReply, Said>,
) -> Result<PlanCmdReply, Said> {
    let adopted = |switched: bool| {
        Some(PlanAdopted {
            slice: t.slice.to_string(),
            path: t.path.to_string(),
            switched,
        })
    };
    if !t.docs_only {
        let said = copy_text("bePlan.command.adopted", &[("slice", t.slice)]);
        return Ok(PlanCmdReply {
            rc: 0,
            said: Some(said),
            path: None,
            adopted: adopted(false),
        });
    }
    if !t.current {
        run(&["repo", t.slice])?;
    }
    let out = run(&["adopt"])?;
    Ok(PlanCmdReply {
        adopted: adopted(!t.current),
        ..out
    })
}

/// 退出码 ＋ 两条输出 ⇒ 回包（`adopted` 空着）或拒（纯函数）。`said` 是 pb 的第一句（它是中文、给人读的）。
pub(crate) fn classify(
    verb: &str,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<PlanCmdReply, Said> {
    let line = |b: &[u8]| {
        let l = crate::plugin::invoke::first_line(b);
        (!l.trim().is_empty()).then_some(l)
    };
    match code {
        Some(0) => {
            let said = line(stdout);
            let path = (verb == "render")
                .then(|| said.as_deref().and_then(|s| s.strip_prefix(RENDER_WROTE)))
                .flatten()
                .map(|p| p.trim().to_string());
            if verb == "render" && path.is_none() {
                return Err(Said::new("failed", copy_text("bePlan.command.noPage", &[]))
                    .and_raw(said.as_deref()));
            }
            Ok(PlanCmdReply {
                rc: 0,
                said,
                path,
                adopted: None,
            })
        }
        Some(RC_REFUSED) => {
            let said = line(stderr).unwrap_or_default();
            Err(Said::new("refused", said.clone()).and_raw(Some(&said)))
        }
        Some(RC_UNREADABLE) => Err(Said::new(
            "not_workspace",
            copy_text("bePlan.face.notWorkspace", &[]),
        )
        .and_raw(line(stderr).as_deref())),
        Some(RC_USAGE) => Err(Said::new(
            "pb_unsupported",
            copy_text("bePlan.command.noVerb", &[("verb", verb)]),
        )
        .and_raw(line(stderr).as_deref())),
        other => Err(Said::new(
            "failed",
            copy_text(
                "bePlan.command.exited",
                &[
                    ("verb", verb),
                    (
                        "rc",
                        &other.map_or_else(|| "?".to_string(), |c| c.to_string()),
                    ),
                ],
            ),
        )
        .and_raw(line(stderr).as_deref())),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/command_tests.rs"]
mod tests;
