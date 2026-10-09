//! 跑一次 `pb dump`：当前目录设在要问的那个目录、环境只给白名单（不带 `PB_ID` ⇒ pb 认作人）、限时；认退出码与形状版本。
//!
//! 「这个目录算不算 pb 工作区」不在这里另写一份：pb 自己从当前目录往上找，找不到回 3。

use crate::platform::child::Deadline;
use copy_core::copy_text;
use std::path::Path;

/// 认得的形状版本（`pb dump` 顶上的 `shape`）。pb 改了形状会把它加一 ⇒ 这里不认 ⇒ 当作「pb 太旧 / 太新」说出来，不硬读。
pub(crate) const SHAPE: u64 = 1;

/// 一次 dump 最多等多久。量过：252 格的研究盘 1.1 秒；留一个数量级给慢盘与远端。
pub(crate) const DEADLINE_SECS: u64 = 20;

/// pb 的退出码（TOOLS.md「退出码」那一节）：读不成 · 用法错。
const RC_UNREADABLE: i32 = 3;
const RC_USAGE: i32 = 4;

/// 起 pb 用的解释器（pb 自己的钩子也这样起它）。
fn interpreter() -> String {
    format!("python3{}", std::env::consts::EXE_SUFFIX)
}

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
    let py = match crate::plugin::discover::find(
        &interpreter(),
        &[],
        true,
        &copy_text("bePlan.dump.noPythonHint", &[]),
    ) {
        Ok(p) => p,
        Err(said) => return Ran::Failed { said, raw: None },
    };
    let entry_s = entry.to_string_lossy().to_string();
    let child = crate::plugin::invoke::child_for(&py, &[entry_s.as_str(), "dump"], &[]).current_dir(dir);
    match child.run(Deadline::secs(DEADLINE_SECS)) {
        Ok(out) => classify(out.status.code(), &out.stdout, &out.stderr),
        Err(e) if e.is_timed_out() => Ran::Failed {
            said: copy_text("bePlan.dump.timedOut", &[("secs", &DEADLINE_SECS.to_string())]),
            raw: None,
        },
        Err(e) => Ran::Failed {
            said: copy_text("bePlan.dump.notRun", &[]),
            raw: Some(e.to_string()),
        },
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
                    &[("code", &other.map_or_else(|| "?".to_string(), |c| c.to_string()))],
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
            &[("shape", &shape.map_or_else(|| "?".to_string(), |s| s.to_string()))],
        ));
    }
    Ran::Dump {
        doc,
        raw: stdout.to_vec(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/dump_tests.rs"]
mod tests;
