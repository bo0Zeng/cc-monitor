use super::*;

/// 判据自带的面：只要说得出线上词，账本就收（跟哪一家无关）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Face {
    Alpha,
    Beta,
}

impl DriftFaceWord for Face {
    fn word(self) -> &'static str {
        match self {
            Face::Alpha => "alpha",
            Face::Beta => "beta",
        }
    }
}

/// 断言整表形状的在局部账本上跑：各家的全局账本会被别的判据写进去。
fn fresh() -> DriftLedger<Face> {
    DriftLedger::new()
}

#[test]
fn the_same_key_recorded_three_times_counts_three() {
    let mut led = fresh();
    for _ in 0..3 {
        led.record(Face::Alpha, "mode");
    }
    let snap = led.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].face, "alpha", "面交的是那一家的线上词");
    assert_eq!(
        snap[0].entries,
        vec![DriftEntry {
            key: "mode".to_string(),
            count: 3
        }]
    );
}

/// 有界：键数触顶后新键并进 `<overflow>`、多一个记一笔；表里的老键照常累加。
#[test]
fn keys_past_the_cap_go_into_overflow() {
    let mut led = fresh();
    for i in 0..(MAX_KEYS + 3) {
        led.record(Face::Alpha, &format!("t{i}"));
    }
    let f = led.snapshot().remove(0);
    assert_eq!(
        f.entries.len(),
        MAX_KEYS + 1,
        "上限那么多个键 ＋ 一个溢出键"
    );
    assert!(
        !f.entries.iter().any(|e| e.key == format!("t{MAX_KEYS}")),
        "触顶之后的新键不进表"
    );
    let ov = f
        .entries
        .iter()
        .find(|e| e.key == "<overflow>")
        .expect("有溢出键");
    assert_eq!(ov.count, 3, "多出来的三个各记进溢出键一笔");
    led.record(Face::Alpha, "t0");
    let f = led.snapshot().remove(0);
    assert_eq!(
        f.entries.iter().find(|e| e.key == "t0").map(|e| e.count),
        Some(2),
        "老键照常累加"
    );
    assert_eq!(
        f.entries
            .iter()
            .find(|e| e.key == OVERFLOW_KEY)
            .map(|e| e.count),
        Some(3),
        "老键不并进溢出键"
    );
    // 封顶按面算：另一面不受这一面触顶的影响。
    led.record(Face::Beta, "fresh");
    let beta = led
        .snapshot()
        .into_iter()
        .find(|f| f.face == "beta")
        .expect("beta 面");
    assert_eq!(beta.entries[0].key, "fresh");
}

#[test]
fn an_empty_key_is_recorded_as_empty_label() {
    let mut led = fresh();
    led.record(Face::Beta, "");
    assert_eq!(led.snapshot()[0].entries[0].key, "<empty>");
}

/// 快照：按次数降序、同数按键名升序（诊断面不许每次刷新都跳）。
#[test]
fn snapshot_sorts_by_count_then_key() {
    let mut led = fresh();
    for (key, n) in [("b", 1), ("c", 2), ("a", 1), ("d", 3)] {
        for _ in 0..n {
            led.record(Face::Alpha, key);
        }
    }
    let keys: Vec<_> = led.snapshot()[0]
        .entries
        .iter()
        .map(|e| (e.key.clone(), e.count))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("d".to_string(), 3),
            ("c".to_string(), 2),
            ("a".to_string(), 1),
            ("b".to_string(), 1)
        ]
    );
}

/// 两家吃同一份漂移输入 ⇒ 出同一份账（面的词各是各的，账一样）：经两家真的写入口 · 读口走一遍。
/// 账本是进程内全局的、别的判据也往里写 ⇒ 键取本条独一份的前缀、只比带前缀的那几条；面取别处少写的那一面，离封顶远。
#[test]
fn both_families_keep_the_same_ledger_for_the_same_input() {
    use crate::agents::claudecode::drift as cc;
    use crate::agents::codex::record as cx;
    const P: &str = "p4-144-same-input-";
    let input = ["b", "c", "a", "c", "b", "c", "d"];
    for key in input {
        let key = format!("{P}{key}");
        cc::record(cc::DriftFace::UnknownWaitingFor, &key);
        cx::drift(cx::DriftFace::CompressedRollout, &key);
    }
    let mine = |snap: Vec<DriftFaceReport>, face: &str| -> Vec<DriftEntry> {
        snap.into_iter()
            .filter(|f| f.face == face)
            .flat_map(|f| f.entries)
            .filter(|e| e.key.starts_with(P))
            .collect()
    };
    let a = mine(cc::snapshot(), "unknown_waiting_for");
    let b = mine(cx::drift_snapshot(), "compressed_rollout");
    let want: Vec<(String, u64)> = [("c", 3), ("b", 2), ("a", 1), ("d", 1)]
        .iter()
        .map(|(k, n)| (format!("{P}{k}"), *n))
        .collect();
    let pairs = |v: &[DriftEntry]| -> Vec<(String, u64)> {
        v.iter().map(|e| (e.key.clone(), e.count)).collect()
    };
    assert_eq!(pairs(&a), want, "Claude Code 那一本记得不对");
    assert_eq!(a, b, "同一份输入，两家的账不一样");
}

/// 计数 · 封顶 · 空键 · 排序只住这一份：两家漂移那一段（从面的枚举起到文件末，判据模块与注释已剥）只交面、只开一本 `DriftLedger`，
/// 不另写常量、不另记键、不另排序。
#[test]
fn the_families_only_name_their_faces() {
    let drift_part = |src: &str| -> String {
        let src = guard_core::production_code(src);
        let at = src.find("enum DriftFace {").expect("找不到面的枚举");
        src[at..].to_string()
    };
    let families = [
        (
            "claudecode/drift.rs",
            drift_part(include_str!(
                "../../../src/backend/agents/claudecode/drift.rs"
            )),
        ),
        (
            "codex/record.rs",
            drift_part(include_str!("../../../src/backend/agents/codex/record.rs")),
        ),
    ];
    for (name, src) in &families {
        assert!(
            src.contains("DriftLedger<DriftFace>"),
            "{name} 的账不是通用那一本"
        );
        assert!(
            src.contains("impl DriftFaceWord for DriftFace"),
            "{name} 的面没经 DriftFaceWord 交出"
        );
        for own in [
            "<overflow>",
            "<empty>",
            "MAX_KEYS",
            "OVERFLOW",
            "DriftEntry",
            "BTreeMap",
            "HashMap",
            "sort",
            "unwrap_or_else",
        ] {
            assert!(!src.contains(own), "{name} 另写了一份漂移账：`{own}`");
        }
    }
}
