use super::*;
use serde_json::Value;

fn base() -> NeedsBase {
    NeedsBase {
        kind: NeedsKind::Approve,
        tool: Some("Bash".into()),
        call: Some("c".into()),
        what: Some("ls".into()),
        text: Words("t".into()),
        head: Words("h".into()),
        head_code: Some("ls".into()),
        rank: 1,
        waited_ms: Some(1),
        since_ms: Some(1),
    }
}

fn ask(choices: Vec<Choice>) -> Ask {
    Ask {
        id: "a".into(),
        kind: AskKind::Approve,
        tool: "Bash".into(),
        run: None,
        what: Some("ls".into()),
        detail: "ls".into(),
        choices,
        questions: Vec::new(),
    }
}

fn choice() -> Choice {
    Choice {
        id: "allow".into(),
        text: Words("y".into()),
        look: ChoiceLook::Main,
        note: NoteNeed::None,
        row: ChoiceRow::Main,
        picks: false,
    }
}

fn answerable() -> Needs {
    let mut n = Needs::new(base(), Words("m".into()));
    n.answer_with(ask(vec![choice()]), Words("h".into()), None);
    n
}

fn manual() -> Needs {
    Needs::new(base(), Words("m".into()))
}

fn wire(n: &Needs) -> Value {
    serde_json::to_value(n).expect("写得出")
}

fn read(v: Value) -> Result<Needs, serde_json::Error> {
    serde_json::from_value(v)
}

/// 答得了 · 答不了两种各写出去再读回来，原样。
#[test]
fn both_shapes_round_trip_through_the_wire() {
    for n in [answerable(), manual()] {
        assert_eq!(read(wire(&n)).expect("读得回"), n);
    }
}

/// 读回来也经同一个构造口：线上那一份「`ask` 为空 ⇔ `manualText` 有字」对不上 · `ask` 在却一项都不给选 ·
/// 抬头与 `ask` 不同在 ⇒ 拒。
#[test]
fn a_wire_needs_whose_answer_cells_disagree_is_refused() {
    let mut both = wire(&answerable());
    both["manualText"] = Value::String("m".into());
    let mut neither = wire(&manual());
    neither["manualText"] = Value::Null;
    let mut no_choices = wire(&answerable());
    no_choices["ask"]["choices"] = Value::Array(Vec::new());
    let mut head_without_ask = wire(&manual());
    head_without_ask["askHead"] = Value::String("h".into());
    let mut ask_without_head = wire(&answerable());
    ask_without_head
        .as_object_mut()
        .expect("是一份对象")
        .remove("askHead");
    for (what, v) in [
        ("两格都有", both),
        ("两格都空", neither),
        ("ask 没有选项", no_choices),
        ("没有 ask 却带抬头", head_without_ask),
        ("有 ask 却没抬头", ask_without_head),
    ] {
        assert!(read(v).is_err(), "{what} 的那一份该被拒");
    }
}
