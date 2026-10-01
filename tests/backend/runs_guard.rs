//! 要求：子 agent 的流归各自的运行、主 tab 上常驻一行，通用层只认「运行」、不认任何一家的形状 —— 用户原话「如果是其他agent呢, 比如codex / 思考怎么解耦, 不要硬适配claude code」。
//!
//! 两族判据：
//! 1. **扫描**：通用层（`src/backend/observe/` · `src/backend/stream/` · `src/frontend/ui/`）零出现 Claude Code 子运行形状的那几个字面量；
//!    正控：同一把扫描在适配层（`src/backend/agents/`）里把它们一个不少地找到（两向集合相等）。
//! 2. **同一批通用判据，两套形状各跑一遍**：假适配层（`agents/fake/runs.rs`，每一格都故意不同形）与 Claude Code 的真形状
//!    （合成夹具，只采结构）。一个会话 ＋ 两个子运行 ＋ 三段流：子运行的流各归各的运行；主运行那段在有子运行在跑、又没自报时
//!    先挂起，记录对上之后才放出、只带主运行自己的东西；没自报的子运行段按它的记录归位；子运行写出终局 ⇒ 运行表里它变完成；
//!    子运行都收场之后，主运行的流不再挂起。

use crate::agents::{RunFaces, StreamEv, StreamFace};
use crate::observe::runs::{RunBook, RunTrack};
use crate::relay::{TapBody, TapEvent};
use crate::stream::run_route::RunRouter;
use crate::stream::wire::{Frame, RunState};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Claude Code 子运行形状的那几个字面量（它们只许住适配层）。
const NEEDLES: &[&str] = &[
    "subagents",
    "isSidechain",
    "agentId",
    "subagent_type",
    "message_start",
    "content_block",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// 一棵树里每个针出现在哪几份文件（原文照扫，注释也算 —— 「零出现」就是零出现）。
fn hits(root: &Path, exts: &[&str]) -> Vec<(String, String)> {
    let files = guard_core::scan_tree_excluding(root, exts, &[]);
    assert!(!files.is_empty(), "{} 下一份文件都没扫到 —— 扫描空转", root.display());
    let mut out = Vec::new();
    for (p, text) in files {
        for n in NEEDLES {
            if text.contains(n) {
                out.push((n.to_string(), p.display().to_string()));
            }
        }
    }
    out
}

#[test]
fn the_general_layers_never_name_a_single_agents_run_shape() {
    let r = repo();
    let mut found = Vec::new();
    for (dir, exts) in [
        ("src/backend/observe", &["rs"][..]),
        ("src/backend/stream", &["rs"][..]),
        ("src/frontend/ui", &["ts", "css", "html", "json"][..]),
    ] {
        found.extend(hits(&r.join(dir), exts));
    }
    assert!(
        found.is_empty(),
        "通用层里出现了某一家的子运行形状（它们只许住 src/backend/agents/<家>/）：\n{}",
        found
            .iter()
            .map(|(n, p)| format!("  {n}  ←  {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // 正控：同一把扫描在适配层里把六个针全找到（扫描本身没瞎）。
    let in_adapters: std::collections::BTreeSet<String> = hits(&r.join("src/backend/agents"), &["rs"])
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let want: std::collections::BTreeSet<String> = NEEDLES.iter().map(|s| s.to_string()).collect();
    assert_eq!(in_adapters, want, "正控：适配层里这几个针该一个不少（扫描瞎了，或适配层的形状搬走了）");
}

// ── 两套形状 ────────────────────────────────────────────────────────────────────

/// 一家的子运行形状在判据里的样子：它的运行面 ＋ 上游协议面 ＋ 造夹具的几只手（只造结构，不含任何真会话正文）。
struct Shape {
    name: &'static str,
    faces: RunFaces,
    stream: StreamFace,
    parent_of: fn(&Path, &str) -> PathBuf,
    child_of: fn(&Path, &str) -> PathBuf,
    /// 主运行一次应答写出的那条记录。
    main_say: fn(&str) -> String,
    /// 父记录：派出一个子运行（工具调用 id, 标签）。
    spawn: fn(&str, &str) -> String,
    /// 父记录：那次调用的结果说出派出的是哪个（工具调用 id, 子运行）。
    spawned: fn(&str, &str) -> String,
    /// 子运行的一条记录：它调了一个工具（子运行, 对账键, 工具名）。
    child_tool: fn(&str, &str, &str) -> String,
    /// 子运行的终局记录（子运行, 对账键）。
    child_end: fn(&str, &str) -> String,
    /// 一次应答的原始流事件（对账键, 工具名）：开始 ＋ 一块工具 ＋ 收尾。
    sse: fn(&str, Option<&str>) -> Vec<String>,
}

fn claude_code() -> Shape {
    Shape {
        name: "claude-code",
        faces: RunFaces::of(&crate::agents::claudecode::RECORDS),
        stream: crate::agents::sse_anthropic::FACE,
        parent_of: |d, sid| d.join(format!("{sid}.jsonl")),
        child_of: |parent, run| {
            let stem = parent.file_stem().unwrap().to_string_lossy().into_owned();
            parent
                .with_file_name(stem)
                .join("subagents")
                .join(format!("agent-{run}.jsonl"))
        },
        main_say: |rid| {
            format!(r#"{{"type":"assistant","uuid":"u-{rid}","message":{{"id":"{rid}","role":"assistant","content":[{{"type":"text","text":"x"}}],"stop_reason":"end_turn"}}}}"#)
        },
        spawn: |tool, label| {
            format!(r#"{{"type":"assistant","uuid":"u-{tool}","message":{{"id":"m-{tool}","role":"assistant","content":[{{"type":"tool_use","id":"{tool}","name":"Agent","input":{{"description":"{label}","subagent_type":"k"}}}}]}}}}"#)
        },
        spawned: |tool, run| {
            format!(r#"{{"type":"user","uuid":"r-{tool}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{tool}","content":"x"}}]}},"toolUseResult":{{"status":"async_launched","agentId":"{run}"}}}}"#)
        },
        child_tool: |run, rid, name| {
            format!(r#"{{"type":"assistant","uuid":"c-{rid}","isSidechain":true,"agentId":"{run}","message":{{"id":"{rid}","role":"assistant","content":[{{"type":"tool_use","id":"x-{rid}","name":"{name}","input":{{}}}}],"stop_reason":"tool_use"}}}}"#)
        },
        child_end: |run, rid| {
            format!(r#"{{"type":"assistant","uuid":"c-{rid}","isSidechain":true,"agentId":"{run}","message":{{"id":"{rid}","role":"assistant","content":[{{"type":"text","text":"x"}}],"stop_reason":"end_turn"}}}}"#)
        },
        sse: |rid, tool| {
            let mut v = vec![format!(r#"{{"type":"message_start","message":{{"id":"{rid}"}}}}"#)];
            if let Some(t) = tool {
                v.push(format!(r#"{{"type":"content_block_start","index":0,"content_block":{{"type":"tool_use","name":"{t}"}}}}"#));
            } else {
                v.push(r#"{"type":"content_block_start","index":0,"content_block":{"type":"text"}}"#.into());
                v.push(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#.into());
            }
            v.push(r#"{"type":"message_stop"}"#.into());
            v
        },
    }
}

fn fake() -> Shape {
    Shape {
        name: "fake",
        faces: crate::agents::fake::runs::FACES,
        stream: crate::agents::fake::runs::STREAM,
        parent_of: |d, sid| d.join(format!("sess-{sid}.ndjson")),
        child_of: |parent, run| {
            let stem = parent.file_stem().unwrap().to_string_lossy().into_owned();
            parent
                .with_file_name(format!("{stem}.lanes"))
                .join(format!("{run}.ndjson"))
        },
        main_say: |rid| format!(r#"{{"kind":"say","resp":"{rid}"}}"#),
        spawn: |tool, label| format!(r#"{{"kind":"spawn","call":"{tool}","title":"{label}","role":"k"}}"#),
        spawned: |tool, run| format!(r#"{{"kind":"spawned","call":"{tool}","lane":"{run}"}}"#),
        child_tool: |run, rid, name| format!(r#"{{"kind":"act","lane":"{run}","resp":"{rid}","use":"{name}"}}"#),
        child_end: |run, rid| format!(r#"{{"kind":"say","lane":"{run}","resp":"{rid}","over":"ok"}}"#),
        sse: |rid, tool| {
            let mut v = vec![format!(r#"{{"ev":"open","rid":"{rid}"}}"#)];
            match tool {
                Some(t) => v.push(format!(r#"{{"ev":"part","at":0,"is":"call","call":"{t}"}}"#)),
                None => {
                    v.push(r#"{"ev":"part","at":0,"is":"words"}"#.into());
                    v.push(r#"{"ev":"chunk","at":0,"txt":"hi"}"#.into());
                }
            }
            v.push(r#"{"ev":"shut"}"#.into());
            v
        },
    }
}

// ── 同一批通用判据 ──────────────────────────────────────────────────────────────

const SID: &str = "s-1";

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("runs-guard-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn append(p: &Path, lines: &[String]) {
    use std::io::Write;
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

/// 一段流交给流归位（`owner` 空 ＝ 请求没自报运行）：回放出来的帧。
fn feed(router: &mut RunRouter, resp: u64, owner: &str, data: Vec<String>) -> Vec<Frame> {
    let n = data.len() as u64;
    let mut out = Vec::new();
    for (i, d) in data.into_iter().enumerate() {
        out.extend(router.on_tap(TapEvent {
            stream: SID.into(),
            owner: owner.into(),
            resp,
            n: i as u64,
            body: TapBody::Data(d),
        }));
    }
    out.extend(router.on_tap(TapEvent {
        stream: SID.into(),
        owner: owner.into(),
        resp,
        n,
        body: TapBody::End { broken: false },
    }));
    out
}

/// 帧的摘要：（第几段, 归哪个运行, 这一件是什么）。
fn said(frames: &[Frame]) -> Vec<(u64, Option<String>, String)> {
    frames
        .iter()
        .map(|f| match f {
            Frame::Tap {
                resp, run, ev, end, ..
            } => (
                *resp,
                run.clone(),
                match (ev, end) {
                    (Some(StreamEv::Start { rid }), _) => format!("start {rid}"),
                    (Some(StreamEv::Block { tool: Some(t), .. }), _) => format!("tool {t}"),
                    (Some(StreamEv::Block { .. }), _) => "block".into(),
                    (Some(StreamEv::Text { s, .. }), _) => format!("text {s}"),
                    (Some(StreamEv::Stop { ok }), _) => format!("stop {ok}"),
                    (None, Some(e)) => format!("end {e:?}"),
                    (None, None) => "?".into(),
                },
            ),
            other => panic!("流归位只该出 tap 帧：{other:?}"),
        })
        .collect()
}

fn states(book: &RunBook) -> Vec<(String, Option<String>, Option<String>, RunState)> {
    let mut v: Vec<_> = book
        .runs(SID)
        .into_iter()
        .map(|r| (r.run, r.label, r.tool, r.state))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn run_the_scenario(shape: &Shape) {
    let w = |s: &str| Some(s.to_string());
    let dir = scratch(shape.name);
    let parent = (shape.parent_of)(&dir, SID);
    let book = Arc::new(RunBook::default());
    let mut track = RunTrack::new(shape.faces, book.clone());
    let mut router = RunRouter::new(book.clone(), vec![shape.stream]);
    let main_line = |track: &RunTrack, line: String| {
        append(&parent, std::slice::from_ref(&line));
        track.main_record(SID, &line);
    };

    // 主运行派出两个子运行（后台派出：结果当场说出是哪个）。
    for l in [
        (shape.spawn)("t1", "L1"),
        (shape.spawn)("t2", "L2"),
        (shape.spawned)("t1", "w1"),
        (shape.spawned)("t2", "w2"),
    ] {
        main_line(&track, l);
    }
    append(&(shape.child_of)(&parent, "w1"), &[(shape.child_tool)("w1", "r-w1a", "Bash")]);
    append(&(shape.child_of)(&parent, "w2"), &[(shape.child_tool)("w2", "r-w2a", "Grep")]);
    assert!(track.adopt(SID, &parent), "[{}] 会话宣告时它已有的子运行没被发现", shape.name);
    assert_eq!(
        states(&book),
        vec![
            ("w1".into(), w("L1"), w("t1"), RunState::Running),
            ("w2".into(), w("L2"), w("t2"), RunState::Running),
        ],
        "[{}] 运行表：两个子运行、标签与父侧工具调用对上、都在跑",
        shape.name
    );

    // 三段流：主运行（没自报）· 两个子运行（自报了是谁）。
    let main = feed(&mut router, 0, "", (shape.sse)("r-main", None));
    let s1 = feed(&mut router, 1, "w1", (shape.sse)("r-w1b", Some("Bash")));
    let s2 = feed(&mut router, 2, "w2", (shape.sse)("r-w2b", Some("Grep")));
    let on_main: Vec<String> = said(&[main.clone(), s1.clone(), s2.clone()].concat())
        .into_iter()
        .filter(|(_, run, _)| run.is_none())
        .map(|(_, _, what)| what)
        .collect();
    assert!(
        !on_main.iter().any(|w| w == "tool Bash" || w == "tool Grep"),
        "[{}] 主活卡上出现了子运行的「准备调用 Bash」：{on_main:?}",
        shape.name
    );
    assert!(
        main.is_empty(),
        "[{}] 有子运行在跑、主运行那段又没自报 ⇒ 对账前不许上主活卡：{:?}",
        shape.name,
        said(&main)
    );
    assert_eq!(
        said(&s1),
        vec![
            (1, w("w1"), "start r-w1b".into()),
            (1, w("w1"), "tool Bash".into()),
            (1, w("w1"), "stop true".into()),
            (1, w("w1"), "end Done".into()),
        ],
        "[{}] 子运行 w1 的流归 w1 那一行",
        shape.name
    );
    assert!(
        said(&s2).iter().all(|(r, run, _)| *r == 2 && *run == w("w2")),
        "[{}] 子运行 w2 的流归 w2 那一行：{:?}",
        shape.name,
        said(&s2)
    );

    // 主运行那一次的记录落盘 ⇒ 挂起的那段按它的归属放出：只有主运行自己的东西。
    main_line(&track, (shape.main_say)("r-main"));
    let released = router.on_learned();
    assert_eq!(
        said(&released),
        vec![
            (0, None, "start r-main".into()),
            (0, None, "block".into()),
            (0, None, "text hi".into()),
            (0, None, "stop true".into()),
            (0, None, "end Done".into()),
        ],
        "[{}] 主活卡只有主运行那段（记录对上之后放出，归主运行）",
        shape.name
    );

    // 没自报的子运行段：挂起，等它自己的记录对上对账键 ⇒ 归它。
    let quiet = feed(&mut router, 3, "", (shape.sse)("r-w1c", Some("Read")));
    assert!(quiet.is_empty(), "[{}] 没自报的那段在对上之前不许放出", shape.name);
    let w1 = (shape.child_of)(&parent, "w1");
    append(&w1, &[(shape.child_tool)("w1", "r-w1c", "Read")]);
    assert!(track.on_path(&w1).is_some(), "[{}] 子运行记录的文件事件没被认出", shape.name);
    assert!(
        said(&router.on_learned())
            .iter()
            .all(|(r, run, _)| *r == 3 && *run == w("w1")),
        "[{}] 没自报的子运行段按它记录的对账键归到 w1",
        shape.name
    );

    // 子运行写出终局 ⇒ 运行表里它变完成；两个都收场之后，主运行的流不再挂起。
    append(&w1, &[(shape.child_end)("w1", "r-w1d")]);
    assert_eq!(track.on_path(&w1), Some((SID.to_string(), true)), "[{}] 终局没让运行表变", shape.name);
    let w2 = (shape.child_of)(&parent, "w2");
    append(&w2, &[(shape.child_end)("w2", "r-w2d")]);
    track.on_path(&w2);
    assert_eq!(
        states(&book).into_iter().map(|(r, _, _, s)| (r, s)).collect::<Vec<_>>(),
        vec![("w1".into(), RunState::Done), ("w2".into(), RunState::Done)],
        "[{}] 子运行写出终局 ⇒ 那一行变完成",
        shape.name
    );
    let after = feed(&mut router, 4, "", (shape.sse)("r-main2", None));
    assert!(
        !after.is_empty() && said(&after).iter().all(|(_, run, _)| run.is_none()),
        "[{}] 没有在跑的子运行 ⇒ 主运行的流直接上（延迟不变）",
        shape.name
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_fake_adapter_with_a_different_shape_runs_the_same_general_criteria() {
    run_the_scenario(&fake());
}

#[test]
fn claude_codes_real_shape_runs_the_same_general_criteria() {
    run_the_scenario(&claude_code());
}

/// 不填那几格 ＝ 没有子运行 ＝ 全归主运行（Codex 今天就是这一形）：流不挂起，记录不进运行表。
#[test]
fn an_adapter_that_declares_no_runs_keeps_everything_on_the_main_run() {
    let book = Arc::new(RunBook::default());
    let track = RunTrack::new(RunFaces::NONE, book.clone());
    let rec = track.main_record(SID, r#"{"lane":"w1","resp":"r1"}"#);
    assert!(!rec.in_run && rec.rid.is_none() && !rec.changed);
    assert!(book.runs(SID).is_empty());
    let mut router = RunRouter::new(book, vec![crate::agents::fake::runs::STREAM]);
    let f = feed(&mut router, 0, "", (fake().sse)("r1", None));
    assert!(!f.is_empty() && said(&f).iter().all(|(_, run, _)| run.is_none()));
}
