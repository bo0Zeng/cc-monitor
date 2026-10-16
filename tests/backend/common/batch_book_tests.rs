//! 一组的结局那一句（`said_of`）：两端同一句，后端一处写。

use super::*;

fn outs(states: &[ItemState]) -> Vec<BatchOutcome> {
    states
        .iter()
        .enumerate()
        .map(|(i, s)| BatchOutcome::bare(key_of(i), *s))
        .collect()
}

/// ★ 是 0 的那一段不写：「删除 5 项 · 完成 3 · 失败 ×2」，不出「跳过 0」「失败 ×0」「已取消 0」「完成 0」；全成 ⇒「已删除 5 项」。
#[test]
fn the_ending_line_leaves_out_the_parts_that_are_zero() {
    use ItemState::*;
    let seg = |k: &str, n: &str| match k {
        "ok" => copy_text("beFiles.batch.segOk", &[("ok", n)]),
        "skip" => copy_text("beFiles.batch.segSkip", &[("skip", n)]),
        "bad" => copy_text("beFiles.batch.segBad", &[("bad", n)]),
        _ => copy_text("beFiles.batch.segLeft", &[("left", n)]),
    };
    let sep = copy_text("kit.text.sep", &[]);
    let head = |n: &str| copy_text("beFiles.batch.head", &[("action", "删除"), ("n", n)]);
    let line = |parts: Vec<String>| parts.join(&sep);
    assert_eq!(
        said_of("删除", &outs(&[Done, Done, Done, Failed, Exists]), None),
        line(vec![head("5"), seg("ok", "3"), seg("bad", "2")])
    );
    assert_eq!(
        said_of("删除", &outs(&[Done, Skipped, Cancelled]), None),
        line(vec![
            head("3"),
            seg("ok", "1"),
            seg("skip", "1"),
            seg("left", "1")
        ])
    );
    assert_eq!(
        said_of("删除", &outs(&[Failed, Failed]), None),
        line(vec![head("2"), seg("bad", "2")])
    );
    assert_eq!(
        said_of("删除", &outs(&[Done, Done]), None),
        copy_text("beFiles.batch.done", &[("action", "删除"), ("n", "2")])
    );
    for s in [
        said_of("删除", &outs(&[Done, Failed]), None),
        said_of("删除", &outs(&[Skipped, Cancelled]), None),
    ] {
        assert!(
            !s.contains(" 0") && !s.contains("×0"),
            "写了是 0 的那一段：{s}"
        );
    }
}
