//! 判据用：往一本运行簿里直接记一个子运行（不走记录那一条管线）· 读它不上线的那两格。

use super::*;

pub fn seed_running(book: &RunBook, sid: &str, run: &str, label: Option<&str>) {
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
pub fn seed(book: &RunBook, sid: &str, info: RunInfo) {
    seed_with(book, sid, info, None, 0);
}

/// 记一项照给的事实，外加收场时刻与调了几次工具（这两样不上线）。
pub fn seed_with(book: &RunBook, sid: &str, info: RunInfo, ended_ms: Option<u64>, calls: u32) {
    book.with(|m| {
        m.entry(sid.to_string()).or_default().runs.push(Run {
            info,
            ended_ms,
            calls,
            seen: SystemTime::now(),
            rid: None,
            closed: None,
        });
    });
}

/// 簿里这个运行不上线的两格：`(收场时刻, 调了几次工具)`；表里没有 ⇒ `None`。
pub fn kept(book: &RunBook, sid: &str, run: &str) -> Option<(Option<u64>, u32)> {
    book.with(|m| {
        m.get(sid)?
            .runs
            .iter()
            .find(|r| r.info.run == run)
            .map(|r| (r.ended_ms, r.calls))
    })
}
