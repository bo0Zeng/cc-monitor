use super::*;
use crate::agents::drift::MAX_KEYS;

// 计数 · 封顶 · 空键 · 排序的判据住通用那一份（`agents/drift_tests.rs`）；这里只判这一家交的面与它那一本账接没接上。

/// 全局那两个入口接的是这一家那一本账 —— 至少走一次。
#[test]
fn the_global_entry_points_delegate_to_the_ledger() {
    // 用一个**本测试专属**的键：全局账本会被别的测试写，只断言「我这条在」。
    let key = "u-cc1-delegation-probe";
    record(DriftFace::KnownTypeParseFailed, key);
    let found = snapshot()
        .into_iter()
        .find(|f| f.face == DriftFace::KnownTypeParseFailed.word())
        .and_then(|f| f.entries.into_iter().find(|e| e.key == key));
    let e = found.expect("全局 record/snapshot 没接上账本");
    assert!(e.count >= 1);
    // 帧命令那一形：各家的面并成一份，面名是那一家的词。
    let v = crate::agents::drift_report();
    assert!(v.faces.iter().any(|f| f.face == "known_type_parse_failed"));
}

/// 线上那个词就是变体名的 snake_case（界面与诊断文本照它认）；枚举加了新面而这里没跟 ⇒ 红。
#[test]
fn every_face_word_is_its_variant_in_snake_case() {
    let faces = [
        DriftFace::UnknownRecordType,
        DriftFace::KnownTypeParseFailed,
        DriftFace::UnknownSessionKind,
        DriftFace::UnknownSessionStatus,
        DriftFace::UnknownWaitingFor,
        DriftFace::UnknownMcpList,
    ];
    for f in faces {
        let snake: String = format!("{f:?}")
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                let lower = c.to_ascii_lowercase();
                if c.is_ascii_uppercase() && i > 0 {
                    vec!['_', lower]
                } else {
                    vec![lower]
                }
            })
            .collect();
        assert_eq!(f.word(), snake);
    }
    let src = guard_core::production_code(include_str!(
        "../../../../src/backend/agents/claudecode/drift.rs"
    ));
    let at = src
        .find("enum DriftFace {")
        .expect("找不到枚举 —— 抽取坏了");
    let end = src[at..].find("\n}").map(|k| at + k).expect("枚举没收尾");
    let variants = src[at..end]
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.ends_with(',') && t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
        .count();
    assert_eq!(
        variants,
        faces.len(),
        "`DriftFace` 有 {variants} 个变体，本条只覆盖了 {} 个",
        faces.len()
    );
}

/// 界面（`record-reads.ts`）与壳（`diagnostics_report::records_in`）读的漂移账回包：线上字节。
/// 两个面 · 一个溢出 · 同数按键名排。
#[test]
fn the_drift_reply_the_ui_reads_keeps_its_wire_bytes() {
    // 局部账本：全局那一本会被任何跑过解析的判据写进去。
    let mut led: DriftLedger<DriftFace> = DriftLedger::new();
    led.record(DriftFace::UnknownSessionKind, "bg");
    led.record(DriftFace::UnknownSessionKind, "bg");
    led.record(DriftFace::UnknownSessionKind, "aa");
    for i in 0..=MAX_KEYS {
        led.record(DriftFace::UnknownRecordType, &format!("t{i:02}"));
    }
    let reply = crate::agents::drift::DriftReport::of(led.snapshot());
    // 机器页那一格在这里写好（出口照抄）：各面加起来几个键。
    assert_eq!(
        reply.said,
        copy_core::copy_text(
            "backend.cc.driftSome",
            &[("n", &(MAX_KEYS + 1 + 2).to_string())]
        )
    );
    assert_eq!(
        crate::agents::drift::DriftReport::of(Vec::new()).said,
        copy_core::copy_text("hostKey.compare.none", &[])
    );
    let wire = serde_json::to_string(&serde_json::to_value(&reply).unwrap()).unwrap();
    assert!(
        wire.starts_with(r#"{"faces":[{"entries":[{"count":1,"key":"<overflow>"},"#),
        "{wire}"
    );
    let tail = r#"{"count":1,"key":"t63"}],"face":"unknown_record_type"},{"entries":[{"count":2,"key":"bg"},{"count":1,"key":"aa"}],"face":"unknown_session_kind"}],"said":"#;
    assert!(
        wire.ends_with(&format!("{tail}{}}}", serde_json::json!(reply.said))),
        "{wire}"
    );
    assert_eq!(wire.matches(r#""count":1,"key":"t"#).count(), MAX_KEYS);
}
