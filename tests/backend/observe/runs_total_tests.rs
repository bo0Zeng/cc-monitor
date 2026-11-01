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
    assert_eq!(
        summary,
        done_text(70),
        "总数照实数：70 个收场的，不是表长 {RUNS_KEEP}"
    );
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

#[test]
fn older_group_head_is_written_here() {
    let book = RunBook::default();
    spawn(&book, RECENT_ENDED + 2, Some(RunEnd::Done));
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + 100);
    let Frame::SessionRuns(r) = book.frame_at(SID, now) else {
        panic!("frame_at 该出运行表帧");
    };
    assert_eq!(r.groups.older.len(), 2, "前提：更早那一组两条");
    assert_eq!(
        r.groups.older_text.map(|w| w.0),
        Some(copy_text("agentsPanel.group.older", &[("n", "2")])),
        "更早那一组的组头字由这里写好，界面照抄"
    );

    let few = RunBook::default();
    spawn(&few, 1, Some(RunEnd::Done));
    let Frame::SessionRuns(r) = few.frame_at(SID, now) else {
        panic!("frame_at 该出运行表帧");
    };
    assert_eq!(r.groups.older_text, None, "更早那一组空 ⇒ 不上线");
}

/// 收场的子运行：用了多久那一句由出帧这一处写好（起止都定了，是静态的），卡上照抄；在跑的不上线。
#[test]
fn ended_run_carries_written_took() {
    let book = RunBook::default();
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let t1 = t0 + Duration::from_secs(90);
    book.with(|m| {
        let s = m.entry(SID.to_string()).or_default();
        s.mark(mark("r-done", None), Some("q-done".into()), t0, ms(t0));
        s.mark(mark("r-done", Some(RunEnd::Done)), None, t1, ms(t1));
        s.mark(mark("r-live", None), Some("q-live".into()), t0, ms(t0));
    });
    let now = t0 + Duration::from_secs(100);
    let Frame::SessionRuns(r) = book.frame_at(SID, now) else {
        panic!("frame_at 该出运行表帧");
    };
    let took = |run: &str| {
        r.runs
            .iter()
            .find(|i| i.run == run)
            .map(|i| i.took_text.clone().map(|w| w.0))
    };
    let want = copy_text(
        "agentWindow.facts.took",
        &[("dur", &copy_core::short_duration(90_000))],
    );
    assert_eq!(
        took("r-done"),
        Some(Some(want)),
        "收场了 ⇒ 写好的用时那一句"
    );
    assert_eq!(took("r-live"), Some(None), "在跑 ⇒ 不上线");
}

/// 出一帧（同 [`frame`] 的时刻）：整份运行表那一帧。
fn frame_whole(book: &RunBook) -> crate::wire::SessionRuns {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + 100);
    let Frame::SessionRuns(r) = book.frame_at(SID, now) else {
        panic!("frame_at 该出运行表帧");
    };
    r
}

/// 格子上 agent 那一枚：有在跑的 ⇒ 照在跑的个数写好那一句；全收场 ⇒ 不上线。
#[test]
fn running_badge_is_written_here() {
    let book = RunBook::default();
    spawn(&book, 3, None);
    let r = frame_whole(&book);
    let text = r.groups.running_text.map(|w| w.0);
    assert_eq!(
        text,
        Some(copy_text("runs.badge.running", &[("running", "3")])),
        "三个在跑 ⇒ 写好的那一句"
    );
    assert!(
        !text.unwrap_or_default().contains('〔'),
        "那一句的文案条目缺了（照出来的是占位）"
    );

    let done = RunBook::default();
    spawn(&done, 2, Some(RunEnd::Done));
    assert_eq!(
        frame_whole(&done).groups.running_text,
        None,
        "没有在跑的 ⇒ 不上线"
    );
}

/// 监控板 peek：照三组序（在跑 ++ 最近 ++ 更早）只交前 [`PEEK_SHOWN`] 个，余下的个数写成「+N」。
#[test]
fn peek_takes_first_few_in_group_order_and_writes_the_rest() {
    let book = RunBook::default();
    spawn(&book, 7, Some(RunEnd::Done));
    spawn_more(&book, 7, 3);
    let r = frame_whole(&book);
    let order: Vec<String> = r
        .groups
        .running
        .iter()
        .chain(&r.groups.recent)
        .chain(&r.groups.older)
        .cloned()
        .collect();
    assert_eq!(order.len(), 10, "前提：表里 10 个");
    assert_eq!(
        r.groups.peek,
        Some(order[..PEEK_SHOWN].to_vec()),
        "照三组序取前几个"
    );
    let more = r.groups.peek_more_text.map(|w| w.0);
    assert_eq!(
        more,
        Some(copy_text("runs.peek.more", &[("more", "2")])),
        "10 个只交 8 个 ⇒ 另 2 个"
    );
    assert!(!more.unwrap_or_default().contains('〔'), "文案条目缺了");

    let few = RunBook::default();
    spawn(&few, 3, None);
    let r = frame_whole(&few);
    assert_eq!(r.groups.peek.map(|p| p.len()), Some(3));
    assert_eq!(r.groups.peek_more_text, None, "没超 ⇒ 不上线");
}

/// 接着 [`spawn`] 再派 `n` 个在跑的（编号从 `from` 起，更晚动）。
fn spawn_more(book: &RunBook, from: usize, n: usize) {
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    book.with(|m| {
        let s = m.entry(SID.to_string()).or_default();
        for i in from..from + n {
            let seen = t0 + Duration::from_secs(i as u64);
            s.mark(mark(&format!("r{i:05}"), None), Some(format!("q{i}")), seen, ms(seen));
        }
    });
}
