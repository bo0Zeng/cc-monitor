//! agent 页按派出关系成树：运行表那一帧里每一项写好它直接派出的几个（`kids`，按开始先后）·
//! 折起时那一句（`kids_text`：下面几个 · 几个在跑，数整棵子树）· 默认开不开（`kids_open`：第一层开，第二层起收）；
//! 三组只列第一层（主运行派的，或派它的那个已不在表里的），监控板的 peek 与在跑那一枚照旧数全部。

use super::testing::seed;
use super::*;

const SID: &str = "s-tree";

fn info(run: &str, state: RunState, parent: Option<&str>, started: u64) -> RunInfo {
    RunInfo {
        run: run.to_string(),
        state,
        parent: parent.map(str::to_string),
        started_ms: Some(started),
        ..RunInfo::default()
    }
}

/// 三层：a（第一层，在跑）→ b（第二层，完成）→ c（在跑）· d（完成）；e 第一层没派过；o 派它的那个不在表里。
fn book() -> RunBook {
    let book = RunBook::default();
    for i in [
        info("a", RunState::Running, None, 1),
        info("b", RunState::Done, Some("a"), 2),
        info("d", RunState::Done, Some("b"), 4),
        info("c", RunState::Running, Some("b"), 3),
        info("e", RunState::Done, None, 5),
        info("o", RunState::Done, Some("gone"), 6),
    ] {
        seed(&book, SID, i);
    }
    book
}

fn frame(book: &RunBook) -> crate::wire::SessionRuns {
    let Frame::SessionRuns(r) = book.frame_at(SID, SystemTime::now()) else {
        panic!("frame_at 该出运行表帧");
    };
    r
}

fn of<'a>(r: &'a crate::wire::SessionRuns, run: &str) -> &'a RunInfo {
    r.runs.iter().find(|i| i.run == run).expect("表里该有它")
}

fn below(n: usize, running: usize) -> Words {
    let n = n.to_string();
    Words(if running > 0 {
        copy_text(
            "agentsPanel.fold.belowRunning",
            &[("n", &n), ("running", &running.to_string())],
        )
    } else {
        copy_text("agentsPanel.fold.below", &[("n", &n)])
    })
}

#[test]
fn three_levels_second_folded_with_summary() {
    let r = frame(&book());
    let a = of(&r, "a");
    assert_eq!(a.kids, vec!["b".to_string()]);
    assert!(a.kids_open, "第一层默认展开");
    assert_eq!(
        a.kids_text,
        Some(below(3, 1)),
        "数整棵子树：b · c · d，c 在跑"
    );
    let b = of(&r, "b");
    assert_eq!(b.kids, vec!["c".to_string(), "d".to_string()], "按开始先后");
    assert!(!b.kids_open, "第二层默认收着");
    assert_eq!(b.kids_text, Some(below(2, 1)));
    for leaf in ["c", "d", "e", "o"] {
        let l = of(&r, leaf);
        assert!(
            l.kids.is_empty() && l.kids_text.is_none() && !l.kids_open,
            "{leaf} 没派过"
        );
    }
}

#[test]
fn nothing_running_below_says_count_only() {
    let book = RunBook::default();
    seed(&book, SID, info("a", RunState::Done, None, 1));
    seed(&book, SID, info("b", RunState::Done, Some("a"), 2));
    seed(&book, SID, info("c", RunState::Failed, Some("b"), 3));
    let r = frame(&book);
    assert_eq!(of(&r, "b").kids_text, Some(below(1, 0)));
    assert_eq!(of(&r, "a").kids_text, Some(below(2, 0)));
}

#[test]
fn groups_list_first_level_only_peek_counts_all() {
    let r = frame(&book());
    let g = &r.groups;
    let listed: Vec<&String> = g.running.iter().chain(&g.recent).chain(&g.older).collect();
    let mut listed: Vec<&str> = listed.iter().map(|s| s.as_str()).collect();
    listed.sort_unstable();
    assert_eq!(
        listed,
        vec!["a", "e", "o"],
        "三组只列第一层（派它的不在表里 ⇒ 也算第一层）"
    );
    assert_eq!(g.running, vec!["a".to_string()]);
    assert_eq!(g.peek.as_ref().map(Vec::len), Some(6), "peek 照旧列全部");
    assert_eq!(
        g.running_text,
        Some(Words(copy_text("runs.badge.running", &[("running", "2")]))),
        "在跑那一枚数全部在跑的（a · c）"
    );
}

#[test]
fn parent_loop_does_not_hang() {
    let book = RunBook::default();
    seed(&book, SID, info("x", RunState::Running, Some("y"), 1));
    seed(&book, SID, info("y", RunState::Running, Some("x"), 2));
    let r = frame(&book);
    let g = &r.groups;
    assert_eq!(
        g.running.len() + g.recent.len() + g.older.len(),
        1,
        "成环的那一圈断开、取一个当第一层，不丢也不重"
    );
    assert_eq!(g.running, vec!["x".to_string()], "从开始最早的那个断开");
    assert_eq!(of(&r, "x").kids, vec!["y".to_string()]);
    assert!(of(&r, "y").kids.is_empty(), "断开的那一条不再挂回去");
}

#[test]
fn agent_count_in_facts_follows_the_tree() {
    let book = RunBook::default();
    seed(&book, SID, info("a", RunState::Done, None, 1));
    seed(&book, SID, info("b", RunState::Done, Some("a"), 2));
    seed(&book, SID, info("s", RunState::Done, Some("s"), 3));
    let r = frame(&book);
    let one = Words(copy_text("rsTurns.proc.agents", &[("n", "1")]));
    let agents = |run: &str| of(&r, run).facts.iter().any(|l| l.text == one);
    assert!(agents("a"), "a 底下挂着 b ⇒ 事实那一行写 agent ×1");
    assert!(!agents("s"), "自己派自己不算派出：树上底下没有，事实也不写");
}
