//! 「开着即活」那一路（没有 pidfile 的那一家：有进程开着它的记录写 ⇒ 活）：宣告 · 行 · 一轮结束 · 摘除（写完关闭 / 进程退出）·
//! resume 重开 · 起步扫到已在写的。写者是 `sh` 造的假进程（`exec 3>>记录`，追加写开着；读 stdin 一行就关掉那个 fd）；
//! 记录是造的中性样本（Codex rollout 的形状：`session_meta` · 一条用户话 · `task_complete`），不是真会话。

use super::*;
use std::io::Write;
use std::process::{Child, Command, Stdio};

const SID: &str = "0000cafe-0000-4000-8000-0000000000c1";

fn record_lines(turn: &str) -> String {
    format!(
        concat!(
            "{{\"timestamp\":\"2026-10-10T00:00:00.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{sid}\",\"cwd\":\"/work/demo\"}}}}\n",
            "{{\"timestamp\":\"2026-10-10T00:00:01.000Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"hello\"}}]}}}}\n",
            "{{\"timestamp\":\"2026-10-10T00:00:02.000Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\",\"turn_id\":\"{turn}\"}}}}\n",
        ),
        sid = SID,
        turn = turn
    )
}

struct Rig {
    /// 打标那一跳走假 tmux（不往跑测试那个终端所在的真 tmux 上打标）。
    _iso: crate::control::identity_tag::door::Isolated,
    dir: PathBuf,
    record: PathBuf,
    state: ReaderState,
    sink: FrameSink,
    rx: tokio::sync::mpsc::Receiver<Frame>,
}

/// 记录树那一家的根（空）＋ Codex 那一家的根（判活认写者），一份造好的 rollout。
fn rig(tag: &str) -> Rig {
    let iso = crate::control::identity_tag::door::isolate();
    let dir = std::env::temp_dir().join(format!("ccm-wr-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sessions = dir.join("codex").join("sessions");
    let day = sessions.join("2026").join("10").join("10");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::create_dir_all(dir.join("projects")).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let record = dir
        .join("codex/sessions/2026/10/10")
        .join(format!("rollout-2026-10-10T00-00-00-{SID}.jsonl"));
    std::fs::write(&record, record_lines("t1")).unwrap();
    let mut state = ReaderState::new(dir.join("projects"), false, false);
    let kind = crate::agents::codex::AGENT_KIND;
    state.follow(&crate::agents::Followed {
        kind,
        root: dir.join("codex").join("sessions"),
        face: crate::agents::record_face(kind).unwrap(),
        live: crate::agents::LiveBy::Writer,
    });
    let (tx, rx) = tokio::sync::mpsc::channel::<Frame>(256);
    Rig {
        _iso: iso,
        dir,
        record,
        state,
        sink: FrameSink::new(tx),
        rx,
    }
}

/// 开着 `file` 追加写的假进程；读到第一行 ⇒ 关掉那个 fd；读到第二行 ⇒ 退出。
fn writer(file: &Path) -> Child {
    let mut c = Command::new("sh")
        .arg("-c")
        .arg("exec 3>>\"$1\"; echo up; read _; exec 3>&-; echo closed; read _")
        .arg("sh")
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    said(&mut c, "up");
    c
}

fn said(c: &mut Child, want: &str) {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), want);
}

fn close_fd(c: &mut Child) {
    c.stdin.as_mut().unwrap().write_all(b"\n").unwrap();
    said(c, "closed");
}

fn gone(mut c: Child) {
    let _ = c.kill();
    let _ = c.wait();
}

fn drain(rx: &mut tokio::sync::mpsc::Receiver<Frame>) -> Vec<Frame> {
    let mut v = Vec::new();
    while let Ok(f) = rx.try_recv() {
        v.push(f);
    }
    v
}

/// 帧的简写：`added:<kind>:<置信度>` · `line:<seq>` · `turn_end:<uuid>` · `removed:<cause>`；别的帧不记。
fn brief(frames: &[Frame]) -> Vec<String> {
    frames
        .iter()
        .filter_map(|f| match f {
            Frame::SessionAdded {
                sid,
                agent_kind,
                liveness_confidence,
                ..
            } => {
                assert_eq!(sid, SID);
                Some(format!(
                    "added:{agent_kind}:{}",
                    liveness_confidence.as_deref().unwrap_or("-")
                ))
            }
            Frame::Line { seq, .. } => Some(format!("line:{seq}")),
            Frame::TurnEnd { uuid, .. } => Some(format!("turn_end:{uuid}")),
            Frame::SessionRemoved { cause, .. } => Some(format!("removed:{cause:?}")),
            _ => None,
        })
        .collect()
}

fn opened(p: &Path) -> crate::platform::writers::OpenBatch {
    crate::platform::writers::OpenBatch {
        opened: vec![p.to_path_buf()],
        ..Default::default()
    }
}

fn closed(p: &Path) -> crate::platform::writers::OpenBatch {
    crate::platform::writers::OpenBatch {
        closed: vec![p.to_path_buf()],
        ..Default::default()
    }
}

/// 有进程开着它写 ⇒ 宣告（帧上说是哪一家；判活与 pidfile 那一路同一档，不标 heuristic）· 已有的行 · 一轮结束；之后追加的行由文件事件那一路照常上流。
#[test]
fn a_record_being_written_is_announced_with_its_lines_and_turn_end() {
    let mut r = rig("announce");
    let w = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let frames = drain(&mut r.rx);
    assert_eq!(
        brief(&frames),
        vec!["added:codex:-", "line:0", "line:1", "line:2", "turn_end:t1"]
    );
    let Some(Frame::SessionAdded {
        path,
        project_dir,
        cwd,
        ..
    }) = frames.first()
    else {
        unreachable!()
    };
    assert_eq!(path.as_deref(), Some(r.record.to_str().unwrap()));
    assert_eq!(
        project_dir.as_deref(),
        Some("/work/demo"),
        "项目目录问那一家（session_meta.cwd）"
    );
    assert_eq!(
        cwd.as_deref(),
        Some("/work/demo"),
        "起会话的目录（客户端认「我刚起的那条」）"
    );
    // 第二轮：记录长了一截 ⇒ 读新行（与记录树那一家同一条读法）。
    let more = "{\"timestamp\":\"2026-10-10T00:00:03.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t2\"}}\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&r.record)
        .unwrap()
        .write_all(more.as_bytes())
        .unwrap();
    process_jsonl(&r.record, &mut r.state, &mut r.sink);
    assert_eq!(brief(&drain(&mut r.rx)), vec!["line:3", "turn_end:t2"]);
    // 已在跟的那一份又被打开（我们自己读它、别人读它）⇒ 什么都不出。
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    assert!(drain(&mut r.rx).is_empty());
    gone(w);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 只读开着它的（读的人）不算写者 ⇒ 不宣告。
#[test]
fn a_reader_holding_the_record_announces_nothing() {
    let mut r = rig("reader");
    let mut c = Command::new("sh")
        .arg("-c")
        .arg("exec 3<\"$1\"; echo up; read _")
        .arg("sh")
        .arg(&r.record)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    said(&mut c, "up");
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    assert!(brief(&drain(&mut r.rx)).is_empty());
    gone(c);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 这条流说了只看别的会话：这一条的行不上流，一轮结束照样上（桌面 · 手机的「一轮完成」通知认它）。
#[test]
fn an_unwatched_session_still_says_its_turn_end() {
    let mut r = rig("unwatched");
    let _ = watch_sessions(vec!["someone-else".to_string()], &mut r.state, &mut r.sink);
    let w = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    assert_eq!(
        brief(&drain(&mut r.rx)),
        vec!["added:codex:-", "turn_end:t1"]
    );
    gone(w);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 写者把记录关了（进程还在：常驻的那种）⇒ 复核没人开着写 ⇒ 摘。之后又有进程打开它接着写（resume）⇒ 再宣告。
#[test]
fn a_closed_record_retires_the_session_and_a_reopen_brings_it_back() {
    let mut r = rig("close");
    let mut w = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let _ = drain(&mut r.rx);
    close_fd(&mut w);
    on_writers(closed(&r.record), &mut r.state, &mut r.sink);
    assert_eq!(brief(&drain(&mut r.rx)), vec!["removed:Gone"]);
    assert!(r.state.active_sids.is_empty());
    // resume：另一个进程打开同一份接着写。
    let w2 = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let again = brief(&drain(&mut r.rx));
    assert_eq!(again.first().map(String::as_str), Some("added:codex:-"));
    gone(w);
    gone(w2);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 写完关闭了、可还有别的进程开着它写 ⇒ 不摘。
#[test]
fn a_close_while_another_writer_holds_it_keeps_the_session() {
    let mut r = rig("twowriters");
    let mut w1 = writer(&r.record);
    let w2 = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let _ = drain(&mut r.rx);
    close_fd(&mut w1);
    on_writers(closed(&r.record), &mut r.state, &mut r.sink);
    assert!(brief(&drain(&mut r.rx)).is_empty(), "还有人在写");
    assert!(r.state.active_sids.contains(SID));
    gone(w1);
    gone(w2);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 写的那个进程退了 ⇒ pidfd 醒、报的是这份记录（与 pidfile 那一路同一个 `PidDied`，主循环同一臂摘）。
#[test]
fn a_writer_exit_wakes_the_pidfd_for_its_record() {
    let mut r = rig("exit");
    let (tx, rx) = std::sync::mpsc::channel();
    r.state.events_tx = Some(tx);
    let w = writer(&r.record);
    let pid = w.id();
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let _ = drain(&mut r.rx);
    gone(w);
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(WatchEvent::PidDied { key, pid: p }) => {
            assert_eq!(key, path_key(&r.record));
            assert_eq!(p, pid);
        }
        _ => panic!("写者退了却没醒"),
    }
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 起步那一趟（与「重新对齐」同一个对表）：后端起来之前就在写的那条也宣告；写者走了之后再对一次表 ⇒ 摘。
#[test]
fn the_startup_scan_finds_a_session_already_being_written() {
    let mut r = rig("startup");
    let w = writer(&r.record);
    let pidfiles = r.dir.join("sessions");
    initial_session_scan(&pidfiles, &mut r.state, &mut r.sink);
    let b = brief(&drain(&mut r.rx));
    assert_eq!(b.first().map(String::as_str), Some("added:codex:-"));
    gone(w);
    let got = resync_sessions(&pidfiles, &mut r.state, &mut r.sink, None);
    assert_eq!(got.removed, 1);
    assert_eq!(brief(&drain(&mut r.rx)), vec!["removed:Gone"]);
    std::fs::remove_dir_all(&r.dir).ok();
}

/// 造好的 Codex 样本（宣告 · 三行 · 一轮结束 · 第二轮）真走一遍 watcher，线上那几行就是这份金样（路径里的临时目录换成 `/r`）。
/// 壳（`tests/frontend/shell/codex_stream_tests.rs`）与界面（`tests/frontend/ui/codex-turn-notify.vitest.ts`）读同一份往下走到系统通知。
/// 重打是显式动作：`REGOLD_CODEX_STREAM=1` 跑本条写盘；不设就只比。时刻的本地写法（`timeText`）随跑的那台机器的时区，换成 `HH:MM`。
pub(crate) const CODEX_STREAM_GOLDEN: &str = "tests/__fixtures__/codex-session-stream.golden.jsonl";

#[test]
fn the_codex_sample_walks_into_the_golden_frames() {
    let mut r = rig("golden");
    let w = writer(&r.record);
    on_writers(opened(&r.record), &mut r.state, &mut r.sink);
    let more = "{\"timestamp\":\"2026-10-10T00:00:03.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t2\"}}\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&r.record)
        .unwrap()
        .write_all(more.as_bytes())
        .unwrap();
    process_jsonl(&r.record, &mut r.state, &mut r.sink);
    let root = r.dir.to_string_lossy().into_owned();
    let produced: String = drain(&mut r.rx)
        .iter()
        .map(|f| {
            local_clock_out(
                &crate::stream::wire::to_line(f)
                    .unwrap()
                    .replace(&root, "/r"),
            )
        })
        .collect();
    gone(w);
    std::fs::remove_dir_all(&r.dir).ok();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(CODEX_STREAM_GOLDEN);
    if std::env::var_os("REGOLD_CODEX_STREAM").is_some() {
        std::fs::write(&path, &produced).expect("写金样");
    }
    let on_disk =
        std::fs::read_to_string(&path).expect("读金样（没有就 REGOLD_CODEX_STREAM=1 打一份）");
    assert_eq!(
        on_disk, produced,
        "Codex 样本的帧变了：形状是有意改的 ⇒ REGOLD_CODEX_STREAM=1 重打，连同壳与界面那两条一起对"
    );
}

/// `"timeText":"17:00"` ⇒ `"timeText":"HH:MM"`（本地时区写出来的那一格，金样里不留）。
fn local_clock_out(line: &str) -> String {
    let key = "\"timeText\":\"";
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find(key) {
        let after = &rest[i + key.len()..];
        let end = after.find('"').unwrap_or(after.len());
        out.push_str(&rest[..i + key.len()]);
        out.push_str("HH:MM");
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}
