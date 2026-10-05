//! 判据用：往一本运行簿里直接记一个在跑的子运行（不走记录那一条管线）。

use super::*;

pub(crate) fn seed_running(book: &RunBook, sid: &str, run: &str, label: Option<&str>) {
    book.with(|m| {
        m.entry(sid.to_string()).or_default().runs.push(Run {
            info: RunInfo {
                run: run.to_string(),
                label: label.map(str::to_string),
                kind: None,
                tool: None,
                state: RunState::Running,
                last: None,
            },
            seen: SystemTime::now(),
            rid: None,
            closed: None,
        });
    });
}
