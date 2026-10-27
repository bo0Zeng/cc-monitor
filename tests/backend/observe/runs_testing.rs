//! 判据用：往一本运行簿里直接记一个在跑的子运行（不走记录那一条管线）；运行表那一帧的字由这里写好（出口照抄）。

use super::*;

pub(crate) fn seed_running(book: &RunBook, sid: &str, run: &str, label: Option<&str>) {
    seed(
        book,
        sid,
        RunInfo {
            run: run.to_string(),
            label: label.map(str::to_string),
            ..RunInfo::default()
        },
    );
}

fn seed(book: &RunBook, sid: &str, info: RunInfo) {
    book.with(|m| {
        m.entry(sid.to_string()).or_default().runs.push(Run {
            info,
            seen: SystemTime::now(),
            rid: None,
            closed: None,
        });
    });
}

fn table(book: &RunBook, sid: &str) -> Vec<RunInfo> {
    let Frame::SessionRuns(s) = book.frame_at(sid, SystemTime::now()) else {
        panic!("运行表那一帧")
    };
    s.runs
}

/// ★ 运行表的每一项带写好的状态字 · 名字 · 最近一件事：出口照抄，不按 `state` · `last.t` 挑字、不替没名字的补「agent」。
#[test]
fn the_run_table_carries_its_own_words() {
    let book = RunBook::default();
    let sid = "s-words";
    seed(
        &book,
        sid,
        RunInfo {
            run: "a".into(),
            label: Some("审交互".into()),
            last: Some(RunDid::Tool {
                name: "Bash".into(),
            }),
            ..RunInfo::default()
        },
    );
    seed(
        &book,
        sid,
        RunInfo {
            run: "b".into(),
            state: RunState::Failed,
            last: Some(RunDid::Think),
            ..RunInfo::default()
        },
    );
    let t = table(&book, sid);
    let words = |r: &RunInfo| {
        (
            r.status_text.clone(),
            r.label_text.clone(),
            r.last_text.clone(),
        )
    };
    assert_eq!(
        words(&t[0]),
        (
            Words(copy_text("runs.state.running", &[])),
            Words("审交互".into()),
            Some(Words(copy_text("runs.last.tool", &[("tool", "Bash")]))),
        )
    );
    assert_eq!(
        words(&t[1]),
        (
            Words(copy_text("runs.state.failed", &[])),
            Words(copy_text("runs.label.unnamed", &[])),
            Some(Words(copy_text("runs.last.think", &[]))),
        )
    );
    for (s, key) in [
        (RunState::Done, "runs.state.done"),
        (RunState::Stopped, "runs.state.stopped"),
        (RunState::Unknown, "runs.state.unknown"),
    ] {
        assert_eq!(status_text(s), Words(copy_text(key, &[])), "{s:?}");
    }
}
