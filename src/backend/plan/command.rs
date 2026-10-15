//! 以人的身份代敲 pb 的命令（`continue` · `pause` · `take` · `view`；`take` 不在此刻那一片时先 `repo <片名>`）：
//! 当前目录设在工作区根、环境只给白名单（不带 `PB_ID` ⇒ pb 认作人）、限时；认退出码，拆 `view` 那句话里的页面路径。
//! 这一侧自己带着 agent 的身份（在 agent 的 shell 里起的）⇒ 用户命令一律不代敲：pb 的用户命令只许人发起。
//!
//! 这几条写盘的是 pb 自己（`continue` / `pause` 改 `auto`，`repo` / `take` 改工作区配置，`view` 往系统临时目录写一页）；
//! 本族照旧一个字节都不写。拆 pb 原话只在 [`classify`] 这一处。

use crate::platform::child::Deadline;
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

/// 界面能点的那几条（pb 的命令名原样）。
pub(crate) const VERBS: [&str; 4] = ["continue", "pause", "take", "view"];

/// pb 的用户命令（只许人敲）：代敲的这几条只在这一侧不带 agent 身份时起。
const USER_VERBS: [&str; 4] = ["continue", "pause", "repo", "take"];

/// pb 认 agent 的那个环境变量（钩子递进 agent 的 shell）。
const AGENT_ENV: &str = "PB_ID";

/// 一条最多等多久（`view` 要把整张图画成一页，量过与 `dump` 同量级）。
pub(crate) const DEADLINE_SECS: u64 = 20;

/// pb 的退出码（TOOLS.md「退出码」那一节）：拒 · 读不成 · 用法错。
const RC_REFUSED: i32 = 2;
const RC_UNREADABLE: i32 = 3;
const RC_USAGE: i32 = 4;

/// `view` 成了时 pb 说的那一句的开头（后面跟页面路径）。
const VIEW_WROTE: &str = "写了 ";

/// 没成 ⇒ `(码, 给人看的那一句, 原话)`。
pub(crate) type Refused = (&'static str, String, Option<String>);

/// 跑一条（`args[0]` 是动词，帧面先判）；这一侧带不带 agent 身份照本进程的环境。
pub(crate) fn run(entry: &Path, ws: &Path, args: &[&str]) -> Result<Value, Refused> {
    run_as(entry, ws, args, std::env::var_os(AGENT_ENV).is_some())
}

/// [`run`] 的本体：`agent` ＝ 这一侧带着 agent 的身份 ⇒ 用户命令不起 pb、回拒。
pub(crate) fn run_as(
    entry: &Path,
    ws: &Path,
    args: &[&str],
    agent: bool,
) -> Result<Value, Refused> {
    let verb = args.first().copied().unwrap_or_default();
    if agent && USER_VERBS.contains(&verb) {
        return Err((
            "refused",
            copy_text("bePlan.command.humanOnly", &[("verb", verb)]),
            None,
        ));
    }
    let py = crate::plugin::discover::find(
        &super::dump::interpreter(),
        &[],
        true,
        &copy_text("bePlan.dump.noPythonHint", &[]),
    )
    .map_err(|said| ("failed", said, None))?;
    let entry_s = entry.to_string_lossy().to_string();
    let argv: Vec<&str> = std::iter::once(entry_s.as_str())
        .chain(args.iter().copied())
        .collect();
    let child = crate::plugin::invoke::child_for(&py, &argv, &[]).current_dir(ws);
    match child.run(Deadline::secs(DEADLINE_SECS)) {
        Ok(out) => classify(verb, out.status.code(), &out.stdout, &out.stderr),
        Err(e) if e.is_timed_out() => Err((
            "failed",
            copy_text(
                "bePlan.command.timedOut",
                &[
                    ("verb", verb),
                    ("dur", &copy_core::format_duration(DEADLINE_SECS * 1000)),
                ],
            ),
            None,
        )),
        Err(e) => Err((
            "failed",
            copy_text("bePlan.dump.notRun", &[]),
            Some(e.to_string()),
        )),
    }
}

/// 要接过仓库的那一片（成品里那一片的几格）。
pub(crate) struct Taking<'a> {
    pub(crate) slice: &'a str,
    /// 仓库相对工作区根在哪。
    pub(crate) path: &'a str,
    pub(crate) docs_only: bool,
    /// 是不是工作区配置里此刻那一片（pb 的 `take` 只接它）。
    pub(crate) current: bool,
}

/// 接过一片的仓库：已经接过 ⇒ 不起 pb；不是此刻那一片 ⇒ 先 `repo <片名>`（只给名字 ＝ 切过去，路径与领域照旧）再 `take`。
/// 回包多 `took {slice, path, switched}`：接到了哪一片 · 仓库在哪 · 此刻那一片换没换。
pub(crate) fn take_with(
    t: &Taking,
    run: &dyn Fn(&[&str]) -> Result<Value, Refused>,
) -> Result<Value, Refused> {
    let took = |switched: bool| json!({"slice": t.slice, "path": t.path, "switched": switched});
    if !t.docs_only {
        let said = copy_text("bePlan.command.taken", &[("slice", t.slice)]);
        return Ok(json!({"rc": 0, "said": said, "path": null, "took": took(false)}));
    }
    if !t.current {
        run(&["repo", t.slice])?;
    }
    let mut out = run(&["take"])?;
    out["took"] = took(!t.current);
    Ok(out)
}

/// 退出码 ＋ 两条输出 ⇒ 回包 `{rc, said, path}` 或拒（纯函数）。`said` 是 pb 的第一句（它是中文、给人读的）。
pub(crate) fn classify(
    verb: &str,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Value, Refused> {
    let line = |b: &[u8]| {
        let l = crate::plugin::invoke::first_line(b);
        (!l.trim().is_empty()).then_some(l)
    };
    match code {
        Some(0) => {
            let said = line(stdout);
            let path = (verb == "view")
                .then(|| said.as_deref().and_then(|s| s.strip_prefix(VIEW_WROTE)))
                .flatten()
                .map(|p| p.trim().to_string());
            if verb == "view" && path.is_none() {
                return Err(("failed", copy_text("bePlan.command.noPage", &[]), said));
            }
            Ok(json!({ "rc": 0, "said": said, "path": path, "took": null }))
        }
        Some(RC_REFUSED) => {
            let said = line(stderr).unwrap_or_default();
            Err(("refused", said.clone(), Some(said)))
        }
        Some(RC_UNREADABLE) => Err((
            "not_workspace",
            copy_text("bePlan.face.notWorkspace", &[]),
            line(stderr),
        )),
        Some(RC_USAGE) => Err((
            "pb_unsupported",
            copy_text("bePlan.command.noVerb", &[("verb", verb)]),
            line(stderr),
        )),
        other => Err((
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
            line(stderr),
        )),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/command_tests.rs"]
mod tests;
