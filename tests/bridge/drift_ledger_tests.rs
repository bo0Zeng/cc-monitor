use super::*;

/// **一律用局部账本**（见 `record_into` 头注）—— 全局账本会被任何跑过 `parse_line`
/// 的测试污染，在它上面断言整表形状必然 flaky（实测 6 次全量跑红 4 次）。
fn fresh() -> Ledger {
    Ledger::new()
}

#[test]
fn counts_and_keeps_only_the_first_sample() {
    let mut led = fresh();
    record_into(
        &mut led,
        DriftFace::UnknownRecordType,
        "mode",
        Some("{\"type\":\"mode\",\"a\":1}"),
    );
    record_into(
        &mut led,
        DriftFace::UnknownRecordType,
        "mode",
        Some("{\"type\":\"mode\",\"b\":2}"),
    );
    record_into(&mut led, DriftFace::UnknownRecordType, "pr-link", None);
    let snap = snapshot_of(&led);
    assert_eq!(snap.len(), 1);
    let f = &snap[0];
    assert_eq!(f.face, DriftFace::UnknownRecordType);
    assert!(!f.overflowed);
    assert_eq!(f.entries[0].key, "mode"); // count 降序
    assert_eq!(f.entries[0].count, 2);
    assert_eq!(
        f.entries[0].first_sample.as_deref(),
        Some("{\"type\":\"mode\",\"a\":1}"),
        "**首见**样例应当被留住，后来的不许覆盖"
    );
    assert_eq!(f.entries[1].key, "pr-link");
    assert!(f.entries[1].first_sample.is_none());
}

/// ★ **有界**：键数触顶后新键并进 `<overflow>`，内存不随 CC 的想象力增长。
#[test]
fn the_key_count_is_bounded() {
    let mut led = fresh();
    for i in 0..(MAX_KEYS + 30) {
        record_into(
            &mut led,
            DriftFace::UnknownRecordType,
            &format!("t{i}"),
            None,
        );
    }
    let f = snapshot_of(&led).remove(0);
    assert!(f.overflowed, "溢出了却没标记");
    assert_eq!(
        f.entries.len(),
        MAX_KEYS + 1,
        "键数应当是上限 + 一个 <overflow>，实得 {}",
        f.entries.len()
    );
    let ov = f
        .entries
        .iter()
        .find(|e| e.key == OVERFLOW_KEY)
        .expect("有溢出键");
    assert_eq!(ov.count, 30, "溢出的 30 个应当全部并进 <overflow>");
    // 已经在表里的老键仍然正常累加（溢出不影响它们）。
    record_into(&mut led, DriftFace::UnknownRecordType, "t0", None);
    let f2 = snapshot_of(&led).remove(0);
    assert_eq!(f2.entries.iter().find(|e| e.key == "t0").unwrap().count, 2);
}

/// 样例按**字符边界**截断 —— 多字节字符不许被切成半个（那会让诊断面显示成乱码）。
#[test]
fn samples_are_truncated_on_a_char_boundary() {
    let mut led = fresh();
    let long = "中".repeat(MAX_SAMPLE_BYTES);
    record_into(
        &mut led,
        DriftFace::KnownTypeParseFailed,
        "user",
        Some(&long),
    );
    let s = snapshot_of(&led)[0].entries[0]
        .first_sample
        .clone()
        .expect("有样例");
    assert!(s.len() <= MAX_SAMPLE_BYTES + 4, "没截断：{}", s.len());
    assert!(s.ends_with('…'), "截断标记没了");
    assert!(
        s.trim_end_matches('…').chars().all(|c| c == '中'),
        "切出了半个字符 —— 诊断面会显示成乱码"
    );
}

/// 空键不许变成空行。
#[test]
fn an_empty_key_is_labelled() {
    let mut led = fresh();
    record_into(&mut led, DriftFace::UnknownSessionKind, "", None);
    assert_eq!(snapshot_of(&led)[0].entries[0].key, "<empty>");
}

/// 全局那两个入口只是纯函数的薄壳 —— 至少走一次，别让它们成为未覆盖的分叉。
#[test]
fn the_global_entry_points_delegate_to_the_pure_ones() {
    // 用一个**本测试专属**的键与机器名：全局账本会被别的测试写，只断言「我这条在」。
    let key = "u-cc1-delegation-probe";
    let probe = Origin("st3-delegation-probe".into());
    record(&probe, DriftFace::UnknownBackendToken, key, Some("s"));
    let found = snapshot(&probe)
        .into_iter()
        .find(|f| f.face == DriftFace::UnknownBackendToken)
        .and_then(|f| f.entries.into_iter().find(|e| e.key == key));
    let e = found.expect("全局 record/snapshot 没接上纯函数");
    assert!(e.count >= 1);
    assert_eq!(e.first_sample.as_deref(), Some("s"));
}

// ── 〔ST3〕按机器分 ─────────────────────────────────────────────────────────

use crate::origin::Origin;

fn key_set(v: &[DriftFaceReport]) -> std::collections::BTreeSet<(DriftFace, String)> {
    v.iter()
        .flat_map(|f| f.entries.iter().map(move |e| (f.face, e.key.clone())))
        .collect()
}

/// ★ J1：两台各记一笔 ⇒ 各自的快照**恰好**是自己那一笔（两向集合相等），没记过的那台 ⇒ 空。
#[test]
fn each_machine_sees_exactly_its_own_book() {
    let mut book = Book::new();
    let local = Origin::local();
    let devbox = Origin("devbox".into());
    record_in_book(
        &mut book,
        &local,
        DriftFace::UnknownSessionKind,
        "workflow",
        None,
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownRecordType,
        "mode",
        Some("{}"),
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownBackendToken,
        "capabilities:x",
        None,
    );
    let want = |xs: &[(DriftFace, &str)]| -> std::collections::BTreeSet<(DriftFace, String)> {
        xs.iter().map(|(f, k)| (*f, k.to_string())).collect()
    };
    assert_eq!(
        key_set(&snapshot_in_book(&book, &local)),
        want(&[(DriftFace::UnknownSessionKind, "workflow")]),
        "本机那一本不是恰好本机那一笔"
    );
    assert_eq!(
        key_set(&snapshot_in_book(&book, &devbox)),
        want(&[
            (DriftFace::UnknownRecordType, "mode"),
            (DriftFace::UnknownBackendToken, "capabilities:x"),
        ]),
        "devbox 那一本不是恰好 devbox 那两笔"
    );
    assert!(
        snapshot_in_book(&book, &Origin("laptop".into())).is_empty(),
        "没记过的那台答出了东西 —— 拿别台的账冒充它"
    );
}

/// 同一个键在两台上**各数各的**（计数与首见样例都不串台）。
#[test]
fn the_same_key_counts_separately_per_machine() {
    let mut book = Book::new();
    let local = Origin::local();
    let devbox = Origin("devbox".into());
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownRecordType,
        "mode",
        Some("devbox-1"),
    );
    record_in_book(
        &mut book,
        &devbox,
        DriftFace::UnknownRecordType,
        "mode",
        Some("devbox-2"),
    );
    record_in_book(
        &mut book,
        &local,
        DriftFace::UnknownRecordType,
        "mode",
        Some("local-1"),
    );
    let first = |o: &Origin| snapshot_in_book(&book, o)[0].entries[0].clone();
    assert_eq!(
        (first(&devbox).count, first(&devbox).first_sample.as_deref()),
        (2, Some("devbox-1"))
    );
    assert_eq!(
        (first(&local).count, first(&local).first_sample.as_deref()),
        (1, Some("local-1"))
    );
}

/// 有界是**每台每面**：一台触顶不挤占另一台。
#[test]
fn one_machine_overflowing_does_not_touch_another() {
    let mut book = Book::new();
    let devbox = Origin("devbox".into());
    for i in 0..(MAX_KEYS + 3) {
        record_in_book(
            &mut book,
            &devbox,
            DriftFace::UnknownRecordType,
            &format!("t{i}"),
            None,
        );
    }
    record_in_book(
        &mut book,
        &Origin::local(),
        DriftFace::UnknownRecordType,
        "fresh",
        None,
    );
    assert!(snapshot_in_book(&book, &devbox)[0].overflowed);
    let local = snapshot_in_book(&book, &Origin::local());
    assert!(!local[0].overflowed, "devbox 触顶连带本机那一本也标了溢出");
    assert_eq!(local[0].entries[0].key, "fresh");
}

/// ★ 每个面都必须说清楚「看不懂时会发生什么」—— 诊断面直接显示这句话。
#[test]
fn every_face_states_its_consequence() {
    let faces = [
        DriftFace::UnknownRecordType,
        DriftFace::KnownTypeParseFailed,
        DriftFace::UnknownSessionKind,
        DriftFace::UnknownBackendToken,
    ];
    for f in faces {
        assert!(f.consequence().len() > 10, "{f:?} 的后果说明太短");
    }
    // 计数自检：枚举加了新面而这里没跟 ⇒ 红。
    let src = guard_core::production_code(include_str!("../../src/bridge/src/drift_ledger.rs"));
    let at = src
        .find("pub enum DriftFace")
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
        "`DriftFace` 有 {variants} 个变体，本条只覆盖了 {} 个 —— 新增面必须来这里写后果",
        faces.len()
    );
}

/// 〔ST3〕读口：答的是所问那台、回包带回那台；空白名（「没说」）拒收，不许被当成某一台。
#[test]
fn the_read_side_answers_the_asked_machine_and_echoes_it() {
    let probe = Origin("st3-read-probe".into());
    record(&probe, DriftFace::UnknownRecordType, "st3-read-key", None);
    let r = tauri::async_runtime::block_on(drift_ledger_report(probe.clone()))
        .expect("读口拒了一台正常的机器");
    assert_eq!(r.origin, probe, "回包没带回所问那台");
    assert!(key_set(&r.faces).contains(&(DriftFace::UnknownRecordType, "st3-read-key".to_string())));
    let other =
        tauri::async_runtime::block_on(drift_ledger_report(Origin("st3-read-other".into())))
            .unwrap();
    assert!(other.faces.is_empty(), "问另一台却答出了探针那台的账");
    let err = tauri::async_runtime::block_on(drift_ledger_report(Origin("  ".into())))
        .expect_err("空白名被当成了某一台");
    assert!(
        err.contains("drift_ledger_report"),
        "拒收的话没点名是哪条命令：{err}"
    );
}
