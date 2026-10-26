//! 跑一次 `pb dump`（起法见 [`super::pb`]，当前目录设在要问的那个目录）；认退出码与形状版本。
//!
//! 「这个目录算不算 pb 工作区」不在这里另写一份：pb 自己从当前目录往上找，找不到回 3。

use crate::plugin::invoke::Done;
use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 认得的形状版本（`pb dump` 顶上的 `shape`）。pb 改了形状会把它加一 ⇒ 这里不认 ⇒ 当作「pb 太旧 / 太新」说出来，不硬读。
pub(crate) const SHAPE: u64 = 6;

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

/// 在工作区根 `ws` 里跑 `<entry> dump --node <仓库> <编号> <工作区>`：一个节点的 agent 视角（pb 原样那一段）。
/// 不成 ⇒ [`NodeMiss`]。
pub(crate) fn run_node(entry: &Path, ws: &Path, repo: &str, id: &str) -> Result<String, NodeMiss> {
    let ws_s = ws.to_string_lossy().to_string();
    node_of(ran_of(super::pb(
        entry,
        ws,
        &["dump", "--node", repo, id, ws_s.as_str()],
    )))
}

/// 在工作区根 `ws` 里跑 `<entry> dump --refs <仓库> <工作区>`，`text` 从标准输入进去：pb 认出的那几处与各自的标题（pb 那一份原样）。
/// 起不来 · 超时 · 退出码不是 0 · 形状不认得 ⇒ `None`（那一批回空，正文原样）；pb 说那个仓库读不成（`error`）照样交回，调用方判。
pub(crate) fn run_refs(
    entry: &Path,
    ws: &Path,
    repo: &str,
    text: &str,
) -> Option<serde_json::Value> {
    let ws_s = ws.to_string_lossy().to_string();
    match ran_of(super::pb_fed(
        entry,
        ws,
        &["dump", "--refs", repo, ws_s.as_str()],
        Some(text.as_bytes()),
    )) {
        Ran::Dump { doc, .. } => Some(doc),
        other => {
            tracing::debug!("pb dump --refs 没答：{other:?}");
            None
        }
    }
}

/// `dump --node` 没给出那一段：pb 说那个仓库 · 那个节点不在（`error`，pb 那一句）· 别的结局（同 [`run`]）。
#[derive(Debug)]
pub(crate) enum NodeMiss {
    Gone(String),
    Ran(Ran),
}

/// `dump --node` 的结局 ⇒ 那一段原文（纯函数）：带 `error` ⇒ `Gone`；读到了却没有 `agent_view` ⇒ 读不懂。
pub(crate) fn node_of(ran: Ran) -> Result<String, NodeMiss> {
    let Ran::Dump { doc, .. } = ran else {
        return Err(NodeMiss::Ran(ran));
    };
    if let Some(e) = doc.get("error").and_then(serde_json::Value::as_str) {
        return Err(NodeMiss::Gone(e.to_string()));
    }
    doc.get("agent_view")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            NodeMiss::Ran(Ran::Failed {
                said: copy_text("bePlan.dump.unparsable", &[]),
                raw: None,
            })
        })
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/dump_tests.rs"]
mod tests;
