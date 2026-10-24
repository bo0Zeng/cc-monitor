//! 跑一次 `pb dump`（起法见 [`super::pb`]，当前目录设在要问的那个目录）；认退出码与形状版本。
//!
//! 「这个目录算不算 pb 工作区」不在这里另写一份：pb 自己从当前目录往上找，找不到回 3。

use crate::plugin::invoke::Done;
use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 认得的形状版本（`pb dump` 顶上的 `shape`）。pb 改了形状会把它加一 ⇒ 这里不认 ⇒ 当作「pb 太旧 / 太新」说出来，不硬读。
pub(crate) const SHAPE: u64 = 4;

/// pb 的退出码：读不成 · 用法错。
const RC_UNREADABLE: i32 = 3;
const RC_USAGE: i32 = 4;

/// 跑一次的结局。
#[derive(Debug)]
pub(crate) enum Ran {
    /// 读到了：整份 JSON ＋ 原始输出（取摘要用）。
    Dump {
        doc: serde_json::Value,
        raw: Vec<u8>,
    },
    /// 那个目录不在任何工作区里（pb 回 3；话是 pb 的 stderr 第一句）。
    NotWorkspace(String),
    /// 这个 pb 没有 `dump`，或形状版本不认得 ⇒ 要换 pb。
    Unsupported(String),
    /// 别的失败（起不来 · 超时 · 输出读不懂 · 别的退出码）：一句话 ＋ 原话。
    Failed { said: String, raw: Option<String> },
}

/// 在 `dir` 里跑 `<entry> dump`。
pub(crate) fn run(entry: &Path, dir: &Path) -> Ran {
    ran_of(super::pb(entry, dir, &["dump"]))
}

/// 起了一次 pb ⇒ 结局（纯函数）：起不来 ⇒ 失败；过了期限 ⇒ 读计划超时（等了多久照原语说的）；跑完了 ⇒ [`classify`]。
pub(crate) fn ran_of(r: Result<Done, Said>) -> Ran {
    match r {
        Err(Said { said, raw }) => Ran::Failed { said, raw },
        Ok(Done {
            waited_secs: Some(secs),
            ..
        }) => Ran::Failed {
            said: copy_text(
                "bePlan.dump.timedOut",
                &[("dur", &copy_core::format_duration(secs * 1000))],
            ),
            raw: None,
        },
        Ok(d) => classify(d.code, &d.stdout, &d.stderr),
    }
}

/// 退出码 ＋ 两条输出 ⇒ 结局（纯函数）。
pub(crate) fn classify(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Ran {
    let diag = || {
        let d = crate::plugin::invoke::first_line(stderr);
        (!d.trim().is_empty()).then_some(d)
    };
    match code {
        Some(0) => {}
        Some(RC_UNREADABLE) => return Ran::NotWorkspace(diag().unwrap_or_default()),
        Some(RC_USAGE) => {
            return Ran::Unsupported(copy_text("bePlan.dump.noDump", &[]));
        }
        other => {
            return Ran::Failed {
                said: copy_text(
                    "bePlan.dump.exited",
                    &[(
                        "rc",
                        &other.map_or_else(|| "?".to_string(), |c| c.to_string()),
                    )],
                ),
                raw: diag(),
            }
        }
    }
    let doc: serde_json::Value = match serde_json::from_slice(stdout) {
        Ok(v) => v,
        Err(e) => {
            return Ran::Failed {
                said: copy_text("bePlan.dump.unparsable", &[]),
                raw: Some(e.to_string()),
            }
        }
    };
    let shape = doc.get("shape").and_then(serde_json::Value::as_u64);
    if shape != Some(SHAPE) {
        return Ran::Unsupported(copy_text(
            "bePlan.dump.shape",
            &[(
                "shape",
                &shape.map_or_else(|| "?".to_string(), |s| s.to_string()),
            )],
        ));
    }
    Ran::Dump {
        doc,
        raw: stdout.to_vec(),
    }
}

/// 在工作区根 `ws` 里跑 `<entry> dump --cell <片> <格> <工作区>`：一格的 agent 视角（pb 原样那一段）。
/// 不成 ⇒ [`CellMiss`]。
pub(crate) fn run_cell(entry: &Path, ws: &Path, slice: &str, id: &str) -> Result<String, CellMiss> {
    let ws_s = ws.to_string_lossy().to_string();
    cell_of(ran_of(super::pb(
        entry,
        ws,
        &["dump", "--cell", slice, id, ws_s.as_str()],
    )))
}

/// `dump --cell` 没给出那一段：pb 说那一片 · 那一格不在（`error`，pb 那一句）· 别的结局（同 [`run`]）。
#[derive(Debug)]
pub(crate) enum CellMiss {
    Gone(String),
    Ran(Ran),
}

/// `dump --cell` 的结局 ⇒ 那一段原文（纯函数）：带 `error` ⇒ `Gone`；读到了却没有 `agent_view` ⇒ 读不懂。
pub(crate) fn cell_of(ran: Ran) -> Result<String, CellMiss> {
    let Ran::Dump { doc, .. } = ran else {
        return Err(CellMiss::Ran(ran));
    };
    if let Some(e) = doc.get("error").and_then(serde_json::Value::as_str) {
        return Err(CellMiss::Gone(e.to_string()));
    }
    doc.get("agent_view")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            CellMiss::Ran(Ran::Failed {
                said: copy_text("bePlan.dump.unparsable", &[]),
                raw: None,
            })
        })
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/dump_tests.rs"]
mod tests;
