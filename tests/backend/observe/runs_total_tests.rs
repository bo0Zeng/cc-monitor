//! 子运行的个数照实说（A2-11）：总数 ＝ 运行表里的 ＋ 被挤出的已收场的，随运行表那一帧的 `summary` 交出去；
//! 运行表照旧只列 [`RUNS_KEEP`] 个。数漏过（在跑的被挤出 · 被挤出的超了 [`ENDED_KEEP`]）⇒ 说「至少」。

use super::*;
use crate::agents::RunMark;

const SID: &str = "s-total";

fn mark(run: &str, end: Option<RunEnd>) -> RunMark {
    RunMark {
        run: run.to_string(),
        end,
        did: None,
        answered: false,
        error: None,
    }
}

/// 先后派 `n` 个子运行（`end` ⇒ 各自收场的样子；`None` ⇒ 都在跑），一个比一个晚动。
fn spawn(book: &RunBook, n: usize, end: Option<RunEnd>) {
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    book.with(|m| {
        let s = m.entry(SID.to_string()).or_default();
        for i in 0..n {
            let seen = t0 + Duration::from_secs(i as u64);
            s.mark(
                mark(&format!("r{i:05}"), end),
                Some(format!("q{i}")),
                seen,
                ms(seen),
            );
        }
    });
}

/// 出一帧（时刻取第一个运行之后 100 秒：远不到「久未动静」，在跑的照旧在跑）：回（运行表几项 · `summary`）。
fn frame(book: &RunBook) -> (usize, String) {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + 100);
    let Frame::SessionRuns(r) = book.frame_at(SID, now) else {
        panic!("frame_at 该出运行表帧");
    };
    let summary = r.summary.map(|w| w.0).unwrap_or_default();
    assert!(
        !summary.contains('〔'),
        "那一句的文案条目缺了（照出来的是占位）：{summary}"
    );
    (r.runs.len(), summary)
}

fn done_text(n: usize) -> String {
    copy_text("agentsPanel.render.summaryDone", &[("n", &n.to_string())])
}

#[test]
fn seventy_ended_runs_count_as_seventy() {
    let book = RunBook::default();
    spawn(&book, 70, Some(RunEnd::Done));
    let (listed, summary) = frame(&book);
    assert_eq!(listed, RUNS_KEEP, "运行表照旧只列上限那么多");
    assert_eq!(summary, done_text(70), "总数照实数：70 个收场的，不是表长 {RUNS_KEEP}");
}

#[test]
fn ended_beyond_keep_says_at_least() {
    let book = RunBook::default();
    let n = RUNS_KEEP + ENDED_KEEP + 5;
    spawn(&book, n, Some(RunEnd::Done));
    let (_, summary) = frame(&book);
    let kept = RUNS_KEEP + ENDED_KEEP;
    assert_eq!(
        summary,
        copy_text(
            "agentsPanel.render.summaryDoneAtLeast",
            &[("n", &kept.to_string())]
        ),
        "被挤出的已收场的超了上限 ⇒ 数不全，只说至少仍记着的那么多"
    );
}

#[test]
fn running_pushed_out_says_at_least() {
    let book = RunBook::default();
    spawn(&book, RUNS_KEEP + 3, None);
    let (listed, summary) = frame(&book);
    assert_eq!(listed, RUNS_KEEP);
    assert_eq!(
        summary,
        copy_text(
            "agentsPanel.render.summaryAtLeast",
            &[
                ("n", &RUNS_KEEP.to_string()),
                ("running", &RUNS_KEEP.to_string())
            ]
        ),
        "在跑的被挤出（没收场、不进被挤出的那一份）⇒ 数漏过，只说至少"
    );
}

#[test]
fn within_keep_is_exact() {
    let book = RunBook::default();
    spawn(&book, 3, Some(RunEnd::Done));
    let (listed, summary) = frame(&book);
    assert_eq!(listed, 3);
    assert_eq!(summary, done_text(3));
}
