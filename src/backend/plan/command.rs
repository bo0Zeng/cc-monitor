//! 以人的身份代敲 pb 的用户命令（`continue` · `pause` · `view`）：当前目录设在工作区根、环境只给白名单
//! （不带 `PB_ID` ⇒ pb 认作人；带了 pb 会拒「只有人敲」）、限时；认退出码，拆 `view` 那句话里的页面路径。
//!
//! 这几条写盘的是 pb 自己（`continue` / `pause` 改工作区 `.env` 的 `auto`，`view` 往系统临时目录写一页）；
//! 本族照旧一个字节都不写。拆 pb 原话只在 [`classify`] 这一处。

use crate::platform::child::Deadline;
use copy_core::copy_text;
use serde_json::{json, Value};
use std::path::Path;

/// 代敲的那几条（pb 的命令名原样）。
pub(crate) const VERBS: [&str; 3] = ["continue", "pause", "view"];

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

/// 跑一条：`verb` 必是 [`VERBS`] 之一（帧面先判）。
pub(crate) fn run(entry: &Path, ws: &Path, verb: &str) -> Result<Value, Refused> {
    let py = crate::plugin::discover::find(
        &super::dump::interpreter(),
        &[],
        true,
        &copy_text("bePlan.dump.noPythonHint", &[]),
    )
    .map_err(|said| ("failed", said, None))?;
    let entry_s = entry.to_string_lossy().to_string();
    let child =
        crate::plugin::invoke::child_for(&py, &[entry_s.as_str(), verb], &[]).current_dir(ws);
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
            Ok(json!({ "rc": 0, "said": said, "path": path }))
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
