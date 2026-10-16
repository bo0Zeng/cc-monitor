//! 主线外清单：分叉选最新 · 多根只折死胡同的人话根 · 重复投递幂等 · 排队裸叶豁免 · 逐条喂与整批算逐条相同。
use super::*;

/// 一批事实 ⇒ 主线外清单（判据的对照：与 [`Chain`] 逐条记、逐条算的结果相同；冷读那一路改成扫描图里的 [`Chain`] 接着记）。
pub(crate) fn off_of(facts: impl IntoIterator<Item = ChainFact>) -> Vec<String> {
    let mut c = Chain::new();
    for f in facts {
        c.load(f);
    }
    c.settle();
    c.off
}

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
    l.text = Some(Words::of(text));
    ChainFact::Node(l)
}

fn off(facts: &[ChainFact]) -> Vec<String> {
    let mut v = off_of(facts.iter().cloned());
    v.sort();
    v
}

/// 实时那一路一条一条来：记一条、算一次；回：清单变没变。
fn feed(c: &mut Chain, f: ChainFact) -> bool {
    c.load(f);
    c.settle()
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
    let mut q = vec![ChainFact::Queued(Words::of("q-one"))];
    q.extend(r.iter().cloned());
    assert_eq!(off(&q), s(&[]));
    // 排队那句在后面才来也一样。
    let mut late = r.to_vec();
    late.push(ChainFact::Queued(Words::of("q-one")));
    assert_eq!(off(&late), s(&[]));
}

#[test]
fn a_resend_not_in_the_queue_stays_off() {
    let r = [
        ChainFact::Queued(Words::of("other")),
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
        ChainFact::Queued(Words::of("queued then cut")),
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
    let changed: Vec<bool> = r.iter().cloned().map(|f| feed(&mut c, f)).collect();
    assert_eq!(changed, [false, false, false, false, true, false]);
    assert_eq!(c.off(), ["C1", "D1"]);
}

#[test]
fn incremental_and_batch_agree_on_every_prefix_of_mixed_shapes() {
    let r = [
        ChainFact::Queued(Words::of("qq")),
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
        feed(&mut c, r[k].clone());
        let mut inc = c.off().to_vec();
        inc.sort();
        assert_eq!(inc, off(&r[..=k]), "prefix {k}");
    }
}

/// 攒着记（`load`）、隔几条算一次（`settle`）与记一条算一次在每个算过的点上清单相同；`settle` 回的「变没变」是相对上一次算的。
#[test]
fn loading_in_batches_and_settling_agrees_with_pushing_one_by_one() {
    let r = [
        ChainFact::Queued(Words::of("qq")),
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        interrupt("I1", Some("A1"), "t03"),
        node("R2", None, "t04", K::Said),
        node("A2", Some("R2"), "t05", K::Reply),
        node("U3", Some("A2"), "t07", K::Said),
        said_text("b", Some("U3"), "t08", "qq"),
        node("A3", Some("U3"), "t09", K::Reply),
        node("late-child", Some("late-parent"), "t12", K::Said),
        node("late-parent", Some("A3"), "t11", K::Reply),
        node("U5", Some("A2"), "t14", K::Said),
        node("A5", Some("U5"), "t15", K::Reply),
        ChainFact::Queued(Words::of("zz")),
    ];
    for every in 1..=r.len() {
        let mut one = Chain::new();
        let mut many = Chain::new();
        let mut before = Vec::<String>::new();
        for (k, f) in r.iter().enumerate() {
            feed(&mut one, f.clone());
            many.load(f.clone());
            if (k + 1) % every == 0 || k + 1 == r.len() {
                let changed = many.settle();
                assert_eq!(many.off(), one.off(), "每 {every} 条算一次，第 {k} 条");
                assert_eq!(
                    changed,
                    many.off() != before.as_slice(),
                    "每 {every} 条算一次，第 {k} 条：变没变"
                );
                before = many.off().to_vec();
                assert!(!many.settle(), "算完再算一次 ⇒ 没变");
            }
        }
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
        let a = off_of(
            c_lines
                .iter()
                .filter_map(|l| claude(&crate::agents::Line::new(l, 0))),
        );
        let b = off_of(
            f_lines
                .iter()
                .filter_map(|l| fake(&crate::agents::Line::new(l, 0))),
        );
        assert!(!a.is_empty());
        assert_eq!(a, b);
    }
}

/// 合成一份大记录形状的链（每轮人一句 · 回复 · 十来条工具占位；每五轮一处回退重说；每轮一句排队），
/// 回：(节点数, 记完算完、事实都用掉之后链索引在本线程上还占着的字节)。
fn resident(turns: usize) -> (usize, usize) {
    let uuid = |k: usize| format!("{k:08x}-0000-4000-8000-{k:012x}");
    let at = |k: usize| {
        format!(
            "2026-10-16T{:02}:{:02}:{:02}.{:03}Z",
            k / 3_600_000 % 24,
            k / 60_000 % 60,
            k / 1000 % 60,
            k % 1000
        )
    };
    let base = crate::alloc_probe::reset_peak();
    let mut facts = Vec::new();
    let mut k = 0usize;
    let mut tip: Option<String> = None;
    let mut push = |facts: &mut Vec<ChainFact>,
                    k: &mut usize,
                    parent: Option<String>,
                    kind: K,
                    text: Option<Words>| {
        *k += 1;
        let ChainFact::Node(mut l) = node(&uuid(*k), parent.as_deref(), &at(*k), kind) else {
            unreachable!()
        };
        l.text = text;
        facts.push(ChainFact::Node(l));
        uuid(*k)
    };
    for t in 0..turns {
        let said = format!("{t:06} words");
        facts.push(ChainFact::Queued(Words::of(&said)));
        if t % 5 == 4 {
            push(
                &mut facts,
                &mut k,
                tip.clone(),
                K::Said,
                Some(Words::of(&format!("r{said}"))),
            );
        }
        let mut p = push(
            &mut facts,
            &mut k,
            tip.clone(),
            K::Said,
            Some(Words::of(&said)),
        );
        p = push(&mut facts, &mut k, Some(p), K::Reply, None);
        for j in 0..12 {
            let kind = if j % 2 == 0 { K::Reply } else { K::Said };
            let text = matches!(kind, K::Said).then(|| Words::of(""));
            p = push(&mut facts, &mut k, Some(p), kind, text);
        }
        tip = Some(p);
    }
    let n = facts
        .iter()
        .filter(|f| matches!(f, ChainFact::Node(_)))
        .count();
    let mut c = Chain::new();
    for f in facts {
        c.load(f);
    }
    c.settle();
    let held = crate::alloc_probe::reset_peak() - base;
    assert!(!c.off().is_empty());
    (n, held.max(0) as usize)
}

/// 链只答「谁在主线上」：节点只记下标 · 时刻 · 几个标志 · 人话原文的摘要（原文进不来，类型 `Words` 挡着）⇒ 每节点有上界。
#[test]
fn the_chain_index_stays_small_per_node() {
    let (n, held) = resident(600);
    assert!(held <= n * 200, "{n} 个节点占 {held} 字节，每节点超过 200");
}

/// 骨架那一形存了再读回来接着记，与一直在内存里接着记逐条相同：还没见到的上一条（之后才来）· 排队摘要 · 时刻都带过去了。
#[test]
fn a_chain_read_back_from_its_skeleton_goes_on_like_the_one_in_memory() {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Held(#[serde(with = "skel")] Chain);
    let before = [
        ChainFact::Queued(Words::of("qq")),
        node("R1", None, "t01", K::Said),
        node("A1", Some("R1"), "t02", K::Reply),
        interrupt("I1", Some("A1"), "t03"),
        node("late-child", Some("late-parent"), "t05", K::Said),
        said_text("b", Some("A1"), "t04", "qq"),
        node("U2", Some("A1"), "t06", K::Said),
    ];
    let after = [
        node("late-parent", Some("U2"), "t07", K::Reply),
        node("A3", Some("late-child"), "t08", K::Reply),
        node("C1", Some("A1"), "t09", K::Said),
    ];
    let mut live = Chain::new();
    for f in before.iter().cloned() {
        live.load(f);
    }
    live.settle();
    let text = serde_json::to_string(&Held(live.clone())).unwrap();
    let Held(mut back) = serde_json::from_str(&text).unwrap();
    assert_eq!(back.off(), live.off());
    assert_eq!(
        serde_json::to_string(&Held(back.clone())).unwrap(),
        text,
        "读回来再存一遍不一样"
    );
    for f in after {
        feed(&mut live, f.clone());
        feed(&mut back, f);
        assert_eq!(back.off(), live.off());
    }
    assert!(!live.off().is_empty());
}
