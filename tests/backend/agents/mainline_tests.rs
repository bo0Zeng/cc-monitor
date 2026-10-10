//! 主线外清单：分叉选最新 · 多根只折死胡同的人话根 · 重复投递幂等 · 排队裸叶豁免 · 逐条喂与整批算逐条相同。
use super::*;

#[derive(Clone, Copy)]
enum K {
    Said,
    Reply,
    Sys,
    Att,
}

fn node(id: &str, parent: Option<&str>, at: &str, k: K) -> ChainFact {
    ChainFact::Node(Link {
        id: id.into(),
        parent: parent.map(Into::into),
        at: at.into(),
        said: matches!(k, K::Said),
        reply: matches!(k, K::Reply),
        interrupt: false,
        text: None,
        shown: !matches!(k, K::Att),
    })
}

fn interrupt(id: &str, parent: Option<&str>, at: &str) -> ChainFact {
    let ChainFact::Node(mut l) = node(id, parent, at, K::Said) else {
        unreachable!()
    };
    l.interrupt = true;
    ChainFact::Node(l)
}

fn said_text(id: &str, parent: Option<&str>, at: &str, text: &str) -> ChainFact {
    let ChainFact::Node(mut l) = node(id, parent, at, K::Said) else {
        unreachable!()
    };
    l.text = Some(text.into());
    ChainFact::Node(l)
}

fn off(facts: &[ChainFact]) -> Vec<String> {
    let mut v = off_of(facts.iter().cloned());
    v.sort();
    v
}

fn s(v: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = v.iter().map(|x| x.to_string()).collect();
    v.sort();
    v
}

#[test]
fn a_single_chain_has_nothing_off() {
    let r = [
        node("A", None, "t01", K::Said),
        node("B", Some("A"), "t02", K::Reply),
        node("C", Some("B"), "t03", K::Said),
        node("D", Some("C"), "t04", K::Reply),
    ];
    assert_eq!(off(&r), s(&[]));
}

#[test]
fn at_a_fork_the_branch_with_the_latest_descendant_wins() {
    let r = [
        node("A", None, "t01", K::Said),
        node("B", Some("A"), "t02", K::Reply),
        node("C1", Some("B"), "t03", K::Said),
        node("C2", Some("B"), "t05", K::Said),
    ];
    assert_eq!(off(&r), s(&["C1"]));
}

#[test]
fn a_first_message_retracted_before_any_reply_is_off() {
    let r = [
        node("R1", None, "t01", K::Said),
        node("R2", None, "t03", K::Said),
        node("A2", Some("R2"), "t04", K::Reply),
    ];
    assert_eq!(off(&r), s(&["R1"]));
}

#[test]
fn interrupted_then_retracted_roots_are_off_and_the_last_stays() {
    let r = [
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        interrupt("I1", Some("A1"), "t03"),
        node("R2", None, "t04", K::Said),
        node("A2", Some("R2"), "t05", K::Reply),
        interrupt("I2", Some("A2"), "t06"),
        node("R3", None, "t07", K::Said),
        node("A3", Some("R3"), "t08", K::Reply),
    ];
    assert_eq!(off(&r), s(&["R1", "A1", "I1", "R2", "A2", "I2"]));
}

#[test]
fn a_compaction_boundary_root_keeps_the_history_before_it() {
    let r = [
        node("P1", None, "t01", K::Said),
        node("PA", Some("P1"), "t02", K::Reply),
        node("S", None, "t03", K::Sys),
        node("CS", Some("S"), "t04", K::Said),
        node("CA", Some("CS"), "t05", K::Reply),
    ];
    assert_eq!(off(&r), s(&[]));
}

#[test]
fn a_complete_conversation_under_a_missing_ancestor_is_kept() {
    let r = [
        node("X", Some("MISSING"), "t01", K::Said),
        node("XA", Some("X"), "t02", K::Reply),
        node("Y", None, "t03", K::Said),
        node("YA", Some("Y"), "t04", K::Reply),
    ];
    assert_eq!(off(&r), s(&[]));
}

#[test]
fn trailing_non_conversation_records_after_an_interrupt_do_not_save_the_root() {
    let r = [
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        interrupt("I1", Some("A1"), "t03"),
        node("S1", Some("I1"), "t04", K::Sys),
        node("S2", Some("S1"), "t05", K::Sys),
        node("R2", None, "t06", K::Said),
        node("A2", Some("R2"), "t07", K::Reply),
    ];
    assert_eq!(off(&r), s(&["R1", "A1", "I1", "S1", "S2"]));
}

#[test]
fn a_lone_dead_end_root_is_the_winner_and_stays() {
    assert_eq!(off(&[node("R1", None, "t01", K::Said)]), s(&[]));
    assert_eq!(off(&[]), s(&[]));
}

#[test]
fn placeholders_off_the_main_line_are_not_listed() {
    let r = [
        node("A", None, "t01", K::Said),
        node("B", Some("A"), "t02", K::Reply),
        node("C1", Some("B"), "t03", K::Said),
        node("X1", Some("C1"), "t03", K::Att),
        node("C2", Some("B"), "t05", K::Said),
    ];
    assert_eq!(off(&r), s(&["C1"]));
}

#[test]
fn a_repeated_record_does_not_change_the_result() {
    let r = vec![
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        node("ATT", Some("A1"), "t03", K::Att),
        node("U2", Some("ATT"), "t04", K::Said),
        node("A2", Some("U2"), "t05", K::Reply),
        node("S", None, "t06", K::Sys),
        node("CS", Some("S"), "t07", K::Said),
        node("CA", Some("CS"), "t08", K::Reply),
    ];
    assert_eq!(off(&r), s(&[]));
    let mut dup = r.clone();
    dup.push(node("ATT", Some("A1"), "t03", K::Att));
    assert_eq!(off(&dup), off(&r));
    let mut twice = r.clone();
    twice.extend(r.iter().cloned());
    assert_eq!(off(&twice), off(&r));

    let w = vec![
        node("A", None, "t01", K::Said),
        node("B", Some("A"), "t02", K::Reply),
        node("C2", Some("B"), "t04", K::Said),
        node("ATT", Some("C2"), "t06", K::Att),
        node("D2", Some("ATT"), "t08", K::Reply),
        interrupt("C1", Some("B"), "t05"),
    ];
    let mut wd = w.clone();
    wd.push(node("ATT", Some("C2"), "t06", K::Att));
    assert_eq!(off(&w), s(&["C1"]));
    assert_eq!(off(&wd), s(&["C1"]));
}

#[test]
fn a_bare_queued_leaf_is_back_on_the_main_line() {
    let r = [
        node("X", None, "T01", K::Reply),
        said_text("b", Some("X"), "T02", "q-one"),
        said_text("tr", Some("X"), "T02", ""),
        interrupt("int", Some("tr"), "T03"),
        node("reply", Some("int"), "T04", K::Reply),
    ];
    assert_eq!(off(&r), s(&["b"]));
    let mut q = vec![ChainFact::Queued("q-one".into())];
    q.extend(r.iter().cloned());
    assert_eq!(off(&q), s(&[]));
    // 排队那句在后面才来也一样。
    let mut late = r.to_vec();
    late.push(ChainFact::Queued("q-one".into()));
    assert_eq!(off(&late), s(&[]));
}

#[test]
fn a_resend_not_in_the_queue_stays_off() {
    let r = [
        ChainFact::Queued("other".into()),
        node("P", None, "T01", K::Sys),
        said_text("a1", Some("P"), "T02", "same words"),
        said_text("a2", Some("P"), "T03", "same words"),
        said_text("win", Some("P"), "T04", "same words"),
        node("resp", Some("win"), "T05", K::Reply),
    ];
    assert_eq!(off(&r), s(&["a1", "a2"]));
}

#[test]
fn a_queued_text_with_children_is_not_exempt() {
    let r = [
        ChainFact::Queued("queued then cut".into()),
        node("X", None, "T01", K::Reply),
        said_text("q", Some("X"), "T02", "queued then cut"),
        interrupt("qi", Some("q"), "T03"),
        said_text("w", Some("X"), "T04", "other"),
        node("wr", Some("w"), "T05", K::Reply),
    ];
    assert_eq!(off(&r), s(&["q", "qi"]));
}

#[test]
fn feeding_one_by_one_reports_changes_and_agrees_with_the_batch() {
    // 一条主线长着、用户在 B 之后回退重发：旧那一支变成主线外。
    let r = [
        node("A", None, "t01", K::Said),
        node("B", Some("A"), "t02", K::Reply),
        node("C1", Some("B"), "t03", K::Said),
        node("D1", Some("C1"), "t04", K::Reply),
        node("C2", Some("B"), "t05", K::Said),
        node("D2", Some("C2"), "t06", K::Reply),
    ];
    let mut c = Chain::new();
    let changed: Vec<bool> = r.iter().cloned().map(|f| c.push(f)).collect();
    assert_eq!(changed, [false, false, false, false, true, false]);
    assert_eq!(c.off(), ["C1", "D1"]);
}

#[test]
fn incremental_and_batch_agree_on_every_prefix_of_mixed_shapes() {
    let r = [
        ChainFact::Queued("qq".into()),
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        interrupt("I1", Some("A1"), "t03"),
        node("R2", None, "t04", K::Said),
        node("A2", Some("R2"), "t05", K::Reply),
        node("ATT", Some("A2"), "t06", K::Att),
        node("U3", Some("ATT"), "t07", K::Said),
        said_text("b", Some("U3"), "t08", "qq"),
        node("A3", Some("U3"), "t09", K::Reply),
        node("S", None, "t10", K::Sys),
        node("CS", Some("S"), "t11", K::Said),
        node("late-child", Some("late-parent"), "t12", K::Said),
        node("late-parent", Some("CS"), "t11", K::Reply),
        node("A4", Some("late-child"), "t13", K::Reply),
        node("U5", Some("A2"), "t14", K::Said),
        node("A5", Some("U5"), "t15", K::Reply),
    ];
    let mut c = Chain::new();
    for k in 0..r.len() {
        c.push(r[k].clone());
        let mut inc = c.off().to_vec();
        inc.sort();
        assert_eq!(inc, off(&r[..=k]), "prefix {k}");
    }
}

/// 同一批形状经两家各自的链事实面进来（字段名完全不同），清单逐条相同 —— 通用层没绑死哪一家。
#[test]
fn two_adapters_with_different_chain_fields_get_the_same_list() {
    // (id, parent, at, kind: u=人 a=回复 n=占位 i=打断, words)
    type Row<'a> = (&'a str, Option<&'a str>, &'a str, char, &'a str);
    let shapes: &[&[Row]] = &[
        &[
            ("A", None, "t01", 'u', "hi"),
            ("B", Some("A"), "t02", 'a', ""),
            ("C1", Some("B"), "t03", 'u', "old"),
            ("N", Some("C1"), "t03", 'n', ""),
            ("C2", Some("B"), "t05", 'u', "new"),
        ],
        &[
            ("R1", None, "t01", 'u', "x"),
            ("A1", Some("R1"), "t02", 'a', ""),
            ("I1", Some("A1"), "t03", 'i', ""),
            ("R2", None, "t04", 'u', "x"),
            ("A2", Some("R2"), "t05", 'a', ""),
        ],
    ];
    let claude = crate::agents::claudecode::RECORDS.chain.unwrap();
    let fake = crate::agents::fake::chain_fact;
    for rows in shapes {
        let c_lines: Vec<String> = rows
            .iter()
            .map(|(id, p, at, k, w)| {
                let (ty, content) = match k {
                    'a' => ("assistant", serde_json::json!([{"type":"text","text":"r"}])),
                    'n' => ("attachment", serde_json::json!(null)),
                    'i' => ("user", serde_json::json!("[Request interrupted by user]")),
                    _ => ("user", serde_json::json!(w)),
                };
                serde_json::json!({"type": ty, "uuid": id, "parentUuid": p, "timestamp": at,
                    "message": {"role": ty, "content": content}})
                .to_string()
            })
            .collect();
        let f_lines: Vec<String> = rows
            .iter()
            .map(|(id, p, at, k, w)| {
                let role = match k {
                    'a' => "bot",
                    'n' => "note",
                    _ => "human",
                };
                serde_json::json!({"node": id, "up": p, "when": at, "role": role, "cut": *k == 'i', "words": w})
                    .to_string()
            })
            .collect();
        let a = off_of(c_lines.iter().filter_map(|l| claude(l)));
        let b = off_of(f_lines.iter().filter_map(|l| fake(l)));
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }
}
