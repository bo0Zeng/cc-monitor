//! 假适配层的**子运行形状**：故意与 Claude Code 每一格都不同形（目录名 · 文件名 · 字段名 · 流协议事件名 · 请求头），
//! 让通用层的那批判据拿它跑 —— 通用层要是偷认了 Claude Code 的形状，这里会认不出东西、判据红。
//!
//! | 格 | 本假适配层 |
//! |---|---|
//! | 对账键 | 记录的 `resp` |
//! | 归属 | `lane`（有它 ＝ 子运行；`over: "ok"` / `"bad"` ＝ 终局；`use` ＝ 调了哪个工具，`kind: "say"` ＝ 在说话） |
//! | 派出 | `{"kind":"spawn","call":…,"title":…,"role":…}` 给标签；`{"kind":"spawned","call":…,"lane":…}` 说是哪个（带 `fin: "ok"/"bad"` ＝ 前台跑完） |
//! | 收场 | `{"kind":"settled","lane":…,"how":"ok"/"bad"/"halted"}`（派出那一方说的）；子运行自己 `over: "cut"` ＝ 被叫停 |
//! | 子运行记录住址 | `<父记录去后缀>.lanes/<lane>.ndjson` |
//! | 流协议 | `{"ev":"open","rid":…}` · `{"ev":"part","at":i,"is":"call"|"words","call":…}` · `{"ev":"chunk","at":i,"txt":…}` · `{"ev":"shut"}` · `{"ev":"fail"}` |

use crate::agents::{
    BlockKind, ChildLink, RunDid, RunEnd, RunFaces, RunMark, StreamEv, StreamFace,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

const LANES_SUFFIX: &str = "lanes";
const LANE_EXT: &str = "ndjson";

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn response_id(v: &Value) -> Option<String> {
    s(v, "resp").map(str::to_string)
}

fn run_of(v: &Value) -> Option<RunMark> {
    let run = s(v, "lane")?.to_string();
    if matches!(s(v, "kind"), Some("spawned" | "settled")) {
        return None;
    }
    let end = match s(v, "over") {
        Some("ok") => Some(RunEnd::Done),
        Some("bad") => Some(RunEnd::Failed),
        Some("cut") => Some(RunEnd::Stopped),
        _ => None,
    };
    let did = match (s(v, "use"), s(v, "kind")) {
        (Some(t), _) => Some(RunDid::Tool {
            name: t.to_string(),
        }),
        (None, Some("say")) => Some(RunDid::Say),
        _ => None,
    };
    Some(RunMark { run, end, did })
}

fn child_link(v: &Value) -> Vec<ChildLink> {
    let how = |k: &str| match s(v, k) {
        Some("ok") => Some(RunEnd::Done),
        Some("bad") => Some(RunEnd::Failed),
        Some("halted") => Some(RunEnd::Stopped),
        _ => None,
    };
    if s(v, "kind") == Some("settled") {
        return vec![ChildLink {
            run: s(v, "lane").map(str::to_string),
            end: how("how"),
            ..ChildLink::default()
        }];
    }
    let Some(call) = s(v, "call") else {
        return Vec::new();
    };
    match s(v, "kind") {
        Some("spawn") => vec![ChildLink {
            tool: Some(call.to_string()),
            label: s(v, "title").map(str::to_string),
            kind: s(v, "role").map(str::to_string),
            ..ChildLink::default()
        }],
        Some("spawned") => vec![ChildLink {
            tool: Some(call.to_string()),
            run: s(v, "lane").map(str::to_string),
            end: how("fin"),
            ..ChildLink::default()
        }],
        _ => Vec::new(),
    }
}

fn hint(line: &str) -> bool {
    line.contains("\"call\"") || line.contains("\"settled\"")
}

fn sources(parent: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (parent.parent(), parent.file_stem().and_then(|x| x.to_str()))
    else {
        return Vec::new();
    };
    let lanes = dir.join(format!("{stem}.{LANES_SUFFIX}"));
    let Ok(rd) = std::fs::read_dir(&lanes) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == LANE_EXT))
        .collect();
    v.sort();
    v
}

/// [`sources`] 的反方向：`<父记录去后缀>.lanes/<lane>.ndjson` ⇒ 父记录（与子运行记录同一个后缀）。
fn owner(p: &Path) -> Option<PathBuf> {
    if !p.extension().is_some_and(|e| e == LANE_EXT) {
        return None;
    }
    let lanes = p.parent()?;
    let stem = lanes
        .file_name()?
        .to_str()?
        .strip_suffix(&format!(".{LANES_SUFFIX}"))?;
    Some(lanes.with_file_name(format!("{stem}.{LANE_EXT}")))
}

/// 本假适配层的运行面。
pub(crate) const FACES: RunFaces = RunFaces {
    response_id: Some(response_id),
    run_of: Some(run_of),
    child_link: Some(child_link),
    children: Some(crate::agents::ChildFace {
        sources,
        owner,
        hint,
    }),
};

fn fold(data: &str) -> Vec<StreamEv> {
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return Vec::new();
    };
    let at = || v.get("at").and_then(Value::as_u64);
    let ev = match s(&v, "ev") {
        Some("open") => s(&v, "rid").map(|r| StreamEv::Start { rid: r.to_string() }),
        Some("part") => at().map(|i| {
            let call = s(&v, "is") == Some("call");
            StreamEv::Block {
                i,
                kind: if call {
                    BlockKind::Tool
                } else {
                    BlockKind::Text
                },
                tool: call.then(|| s(&v, "call").map(str::to_string)).flatten(),
            }
        }),
        Some("chunk") => at().and_then(|i| {
            Some(StreamEv::Text {
                i,
                s: s(&v, "txt")?.to_string(),
            })
        }),
        Some("shut") => Some(StreamEv::Stop { ok: true }),
        Some("fail") => Some(StreamEv::Stop { ok: false }),
        _ => None,
    };
    ev.into_iter().collect()
}

/// 本假适配层上游的流协议面。
pub(crate) const STREAM: StreamFace = StreamFace { fold };
