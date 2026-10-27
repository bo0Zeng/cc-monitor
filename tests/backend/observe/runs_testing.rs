//! 判据用：往一本运行簿里直接记一个子运行（不走记录那一条管线）。

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

/// 记一项照给的事实（状态 · 名字 · 最近一件事 …）。
pub(crate) fn seed(book: &RunBook, sid: &str, info: RunInfo) {
    book.with(|m| {
        m.entry(sid.to_string()).or_default().runs.push(Run {
            info,
            seen: SystemTime::now(),
            rid: None,
            closed: None,
        });
    });
}
