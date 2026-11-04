//! 收下一条会话时（`RunTrack::adopt`），子记录只为还没收场、或最近还在写的那几个子运行从头读：
//! 父侧已经说了收场（预读时从父记录补进簿里）、记录又早不写了的，只留游标，不读正文；它之后再被写，照旧从 0 读起。
//! 夹具照 Claude Code 的形状、只造结构（正文占位）。

use super::*;
use crate::agents::RunFaces;
use std::path::{Path, PathBuf};

const SID: &str = "s-adopt";

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

fn faces() -> RunFaces {
    RunFaces::of(&crate::agents::claudecode::RECORDS)
}

fn child_of(parent: &Path, run: &str) -> PathBuf {
    let stem = parent.file_stem().unwrap().to_string_lossy().into_owned();
    parent
        .with_file_name(stem)
        .join("subagents")
        .join(format!("agent-{run}.jsonl"))
}

fn spawn(tool: &str, label: &str) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"u-{tool}","message":{{"id":"m-{tool}","role":"assistant","content":[{{"type":"tool_use","id":"{tool}","name":"Agent","input":{{"description":"{label}","subagent_type":"k"}}}}]}}}}"#
    )
}

/// 前台派出那次拿到结果 ⇒ 父侧说它收场了。
fn fg_done(tool: &str, run: &str) -> String {
    format!(
        r#"{{"type":"user","uuid":"r-{tool}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{tool}","content":"x"}}]}},"toolUseResult":{{"status":"completed","agentId":"{run}"}}}}"#
    )
}

/// 后台派出当场回的回执 ⇒ 还在跑。
fn launched(tool: &str, run: &str) -> String {
    format!(
        r#"{{"type":"user","uuid":"r-{tool}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{tool}","content":"x"}}]}},"toolUseResult":{{"status":"async_launched","agentId":"{run}"}}}}"#
    )
}

fn child_tool(run: &str, rid: &str) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"c-{rid}","isSidechain":true,"agentId":"{run}","message":{{"id":"{rid}","role":"assistant","content":[{{"type":"tool_use","id":"x-{rid}","name":"Bash","input":{{"command":"{pad}"}}}}],"stop_reason":"tool_use"}}}}"#,
        pad = "x".repeat(400)
    )
}

/// 一份子记录（几行工具调用）＋ 派出那一刻旁边落的那一份；写入时刻拨到 `age` 之前。回：它的字节数。
fn child(parent: &Path, run: &str, tool: &str, age: Duration) -> u64 {
    let p = child_of(parent, run);
    let lines: Vec<String> = (0..20)
        .map(|i| child_tool(run, &format!("{run}-{i}")))
        .collect();
    append(&p, &lines);
    std::fs::write(
        p.with_file_name(format!("agent-{run}.meta.json")),
        format!(r#"{{"agentType":"k","description":"x","toolUseId":"{tool}"}}"#),
    )
    .unwrap();
    let f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    f.set_modified(SystemTime::now() - age).unwrap();
    std::fs::metadata(&p).unwrap().len()
}

/// 父记录写好之后照真流程收下：预读那一截（父侧的派出 / 收场补进簿里）→ 宣告时 `adopt`。
fn adopt(parent: &Path, track: &mut RunTrack) {
    let bytes = std::fs::read(parent).unwrap();
    track.prime(SID, &bytes);
    track.adopt(SID, parent);
}

fn old() -> Duration {
    STALE_AFTER + Duration::from_secs(3600)
}

#[test]
fn adopting_reads_only_the_children_that_have_not_ended() {
    let dir = test_home::scratch("only-live");
    let parent = dir.join(format!("{SID}.jsonl"));
    let mut lines = Vec::new();
    for i in 0..5 {
        lines.push(spawn(&format!("t{i}"), &format!("L{i}")));
        lines.push(fg_done(&format!("t{i}"), &format!("w{i}")));
    }
    lines.push(spawn("t9", "L9"));
    lines.push(launched("t9", "w9"));
    append(&parent, &lines);
    for i in 0..5 {
        child(&parent, &format!("w{i}"), &format!("t{i}"), old());
    }
    let live = child(&parent, "w9", "t9", Duration::from_secs(5));
    let book = Arc::new(RunBook::default());
    let mut track = RunTrack::new(faces(), book.clone());
    adopt(&parent, &mut track);
    assert_eq!(
        track.child_bytes(),
        live,
        "收下会话时只该从头读那一份还在跑的子记录；父侧已说收场、早不写了的五份不读"
    );
    let mut states: Vec<(String, Option<String>, RunState)> = book
        .runs(SID)
        .into_iter()
        .map(|r| (r.run, r.label, r.state))
        .collect();
    states.sort_by(|a, b| a.0.cmp(&b.0));
    let mut want: Vec<(String, Option<String>, RunState)> = (0..5)
        .map(|i| (format!("w{i}"), Some(format!("L{i}")), RunState::Done))
        .collect();
    want.push(("w9".into(), Some("L9".into()), RunState::Running));
    assert_eq!(states, want, "没读的照样列出、标签与状态照父侧");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_ended_child_still_being_written_is_read() {
    let dir = test_home::scratch("fresh");
    let parent = dir.join(format!("{SID}.jsonl"));
    append(&parent, &[spawn("t1", "L1"), fg_done("t1", "w1")]);
    let n = child(&parent, "w1", "t1", Duration::from_secs(5));
    let mut track = RunTrack::new(faces(), Arc::new(RunBook::default()));
    adopt(&parent, &mut track);
    assert_eq!(
        track.child_bytes(),
        n,
        "父侧说了收场、但记录最近还在写 ⇒ 照读"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_skipped_child_written_again_is_read_from_the_start() {
    let dir = test_home::scratch("again");
    let parent = dir.join(format!("{SID}.jsonl"));
    append(&parent, &[spawn("t1", "L1"), fg_done("t1", "w1")]);
    child(&parent, "w1", "t1", old());
    let mut track = RunTrack::new(faces(), Arc::new(RunBook::default()));
    adopt(&parent, &mut track);
    assert_eq!(track.child_bytes(), 0, "收场又早不写的那一份不读");
    let p = child_of(&parent, "w1");
    append(&p, &[child_tool("w1", "w1-more")]);
    assert_eq!(
        track.on_path(&p).map(|(sid, _)| sid),
        Some(SID.to_string()),
        "它再被写 ⇒ 仍认得是这条会话的子记录"
    );
    assert_eq!(
        track.child_bytes(),
        std::fs::metadata(&p).unwrap().len(),
        "再被写 ⇒ 从 0 读起（之前那一截一起读进来）"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn skipped_children_keep_the_table_in_write_order() {
    let dir = test_home::scratch("order");
    let parent = dir.join(format!("{SID}.jsonl"));
    // 父记录里先派的那个，记录反而写得晚：运行表照最近动静（记录的写入时刻）排，不照父记录的先后。
    append(
        &parent,
        &[
            spawn("t1", "L1"),
            fg_done("t1", "w1"),
            spawn("t2", "L2"),
            fg_done("t2", "w2"),
        ],
    );
    child(&parent, "w1", "t1", old());
    child(&parent, "w2", "t2", old() + Duration::from_secs(600));
    let book = Arc::new(RunBook::default());
    let mut track = RunTrack::new(faces(), book.clone());
    adopt(&parent, &mut track);
    assert_eq!(track.child_bytes(), 0);
    let order: Vec<String> = book.runs(SID).into_iter().map(|r| r.run).collect();
    assert_eq!(order, vec!["w2".to_string(), "w1".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 子运行派出的孙运行：派它的那一行只写在子运行（父侧已收场、早不写了，不读）的记录里；
/// 标签 · 类别 · 谁派的 · 后台派出照孙运行旁边派出那一刻落的那一份来，不靠读它的上一层。
#[test]
fn a_grandchild_keeps_its_label_when_its_spawner_is_not_read() {
    let dir = test_home::scratch("nested");
    let parent = dir.join(format!("{SID}.jsonl"));
    append(&parent, &[spawn("t1", "L1"), fg_done("t1", "w1")]);
    child(&parent, "w1", "t1", old());
    let w1 = child_of(&parent, "w1");
    let as_w1 = |l: String| {
        l.replacen(
            r#"{"type":"#,
            r#"{"isSidechain":true,"agentId":"w1","type":"#,
            1,
        )
    };
    append(
        &w1,
        &[as_w1(spawn("t2", "L2")), as_w1(launched("t2", "g1"))],
    );
    let f = std::fs::OpenOptions::new().append(true).open(&w1).unwrap();
    f.set_modified(SystemTime::now() - old()).unwrap();
    let g1 = child_of(&parent, "g1");
    append(&g1, &[child_tool("g1", "g1-0")]);
    std::fs::write(
        g1.with_file_name("agent-g1.meta.json"),
        r#"{"agentType":"kg","description":"L2","toolUseId":"t2","parentAgentId":"w1","spawnDepth":2,"requestShape":"background"}"#,
    )
    .unwrap();
    let book = Arc::new(RunBook::default());
    let mut track = RunTrack::new(faces(), book.clone());
    adopt(&parent, &mut track);
    assert_eq!(
        track.child_bytes(),
        std::fs::metadata(&g1).unwrap().len(),
        "只读还在跑的孙运行；派它的那个子运行已收场、早不写了，不读"
    );
    let g = book
        .runs(SID)
        .into_iter()
        .find(|r| r.run == "g1")
        .expect("孙运行在表里");
    assert_eq!(
        (
            g.label.as_deref(),
            g.kind.as_deref(),
            g.parent.as_deref(),
            g.background,
            g.tool.as_deref()
        ),
        (Some("L2"), Some("kg"), Some("w1"), true, Some("t2")),
        "孙运行的标签 · 类别 · 谁派的 · 后台派出照派出那一刻落的那一份"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 分叉出去的孙运行，记录开头抄着派它的那段对话（连派它的那次调用，记成它自己的）：「谁派的」照派出那一刻落的那一份，
/// 不因为那一行写在它自己的记录里就成了它自己。
#[test]
fn a_forked_grandchild_is_not_its_own_spawner() {
    let dir = test_home::scratch("fork");
    let parent = dir.join(format!("{SID}.jsonl"));
    append(&parent, &[spawn("t1", "L1"), fg_done("t1", "w1")]);
    child(&parent, "w1", "t1", old());
    let as_g1 = |l: String| {
        l.replacen(
            r#"{"type":"#,
            r#"{"isSidechain":true,"agentId":"g1","type":"#,
            1,
        )
    };
    let g1 = child_of(&parent, "g1");
    append(&g1, &[as_g1(spawn("t2", "L2")), child_tool("g1", "g1-0")]);
    std::fs::write(
        g1.with_file_name("agent-g1.meta.json"),
        r#"{"agentType":"fork","isFork":true,"description":"L2","toolUseId":"t2","parentAgentId":"w1","spawnDepth":2,"requestShape":"background"}"#,
    )
    .unwrap();
    let book = Arc::new(RunBook::default());
    let mut track = RunTrack::new(faces(), book.clone());
    adopt(&parent, &mut track);
    let g = book
        .runs(SID)
        .into_iter()
        .find(|r| r.run == "g1")
        .expect("孙运行在表里");
    assert_eq!(g.parent.as_deref(), Some("w1"));
    let _ = std::fs::remove_dir_all(&dir);
}
