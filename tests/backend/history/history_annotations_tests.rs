//! 历史注解换读写者（monitor → 本机常驻后端）的判据。
//!
//! # 守的要求（住址）
//!
//! 要求：「本机注解（`history-metadata.json`：星标 / 改名 / 隐藏）的
//! **读写者**换成本机常驻后端 —— **文件留在原处、同一路径，不迁移、一条不丢**」；「🔴 用户的历史注解一条不许丢：
//! 判据要钉『迁移前后读出来的注解逐条相等』」。
//!
//! # 判据
//!
//! 1. **迁移前后读出来的注解逐条相等**：新读者（本模块）读结构占位夹具 `tests/__fixtures__/history-metadata.fixture.json`
//!    ＝ 金样 `history-metadata.readout.golden.json`；金样由**旧读者**（monitor `history·rs::HistoryMetadata`）读同一份夹具产出、
//!    monitor 那一侧 `c4d_the_old_reader_reads_the_annotation_fixture_as_the_golden`〔散文墓碑〕在旧读者还在的那一拍（子步 4）对过、之后随旧读者一起退役。异源：两个 crate、两份实现。
//! 2. **写一条不丢别的**：夹具拷进临时目录 → 改一条 → 再读：其余每条逐格 == 金样；被改那条只变了 patch 那几格 ＋ `updatedAt`；
//!    条目里 / 顶层认不出的键原样还在；被改那条身上的蛇形别名摘了（否则下次严格读读不懂）。
//! 3. **读不懂就不写**：坏 JSON · 字段类型不对 · 同一格驼峰与蛇形都在 · 顶层不是对象 ⇒ 拒，文件逐字节不变。
//! 4. patch 语义逐格照搬 monitor 从前那份（缺格 / `null` 不改；空白串清空）；忘掉一条；路径只认绝对路径。
//!
//! # 买不到
//!
//! - 🔴 用户那台机器上的**真** `history-metadata.json`：devbox 上没有这份文件（monitor 主要跑在另一台），真文件的前后对拍没跑。
//! - 两个后端进程同时写；真 Windows 上的原子挪。

use super::*;

fn fixture_dir() -> PathBuf {
    crate::guard_support::repo_root().join("tests/__fixtures__")
}

fn golden() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(fixture_dir().join("history-metadata.readout.golden.json"))
            .expect("金样"),
    )
    .expect("金样不是 JSON")
}

fn norm(t: &Table) -> Value {
    serde_json::to_value(t).expect("Table 总能序列化")
}

fn temp_copy(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-annot-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("history-metadata.json");
    std::fs::copy(fixture_dir().join("history-metadata.fixture.json"), &p).unwrap();
    p
}

fn read_table(p: &Path) -> Table {
    match load_at(p) {
        Loaded::Read(t) => t,
        other => panic!("读不动：{other:?}"),
    }
}

/// ★ 判据 1：新读者读夹具 == 旧读者读出来的金样（逐条、逐格）。
#[test]
fn the_new_reader_reads_the_fixture_exactly_as_the_old_reader_did() {
    let t = read_table(&fixture_dir().join("history-metadata.fixture.json"));
    assert_eq!(t.len(), 6, "夹具六条，新读者少读了");
    assert_eq!(
        norm(&t),
        golden(),
        "迁移前后读出来的注解不一致 —— 用户的星标 / 改名 / 隐藏会在换读写者那一刻变样"
    );
}

/// ★ 判据 2：改一条，其余逐格不动；认不出的键原样留着；被改那条只变 patch 那几格。
#[test]
fn writing_one_entry_loses_nothing_else() {
    let p = temp_copy("one");
    let before_raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    let sid = "0000aaaa-0000-4000-8000-000000000002"; // 蛇形别名那一条
    let got =
        answer_annotate_at(&p, &json!({"sid": sid, "patch": {"starred": true}}), 4242).unwrap();
    assert_eq!(got["entry"]["starred"], true);
    assert_eq!(got["entry"]["updatedAt"], 4242);

    let after = read_table(&p);
    let mut want = golden();
    want[sid]["starred"] = json!(true);
    want[sid]["updatedAt"] = json!(4242);
    assert_eq!(
        norm(&after),
        want,
        "改一条之后读回来：其余每条必须逐格等于金样，被改那条只变 patch 那一格 ＋ updatedAt"
    );

    let after_raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(
        after_raw["extraTop"], before_raw["extraTop"],
        "顶层认不出的键没了"
    );
    assert_eq!(
        after_raw["entries"]["0000aaaa-0000-4000-8000-000000000005"],
        before_raw["entries"]["0000aaaa-0000-4000-8000-000000000005"],
        "没改的那一条（带认不出的键）原文变了"
    );
    let patched = after_raw["entries"][sid].as_object().unwrap();
    for a in ["custom_title", "updated_at"] {
        assert!(
            !patched.contains_key(a),
            "被改那一条身上还留着蛇形别名 `{a}` —— 与驼峰那一格同时在，下一次严格读就读不懂了"
        );
    }
    // 没被改的每一条，原文逐键不变（不只是读出来一样）。
    for (k, v) in before_raw["entries"].as_object().unwrap() {
        if k != sid {
            assert_eq!(&after_raw["entries"][k], v, "{k} 的原文被动了");
        }
    }
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// ★ 判据 3：读不懂的那一份，一个字节都不写。
#[test]
fn an_unreadable_file_is_never_overwritten() {
    let d = std::env::temp_dir().join(format!("ccm-annot-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("history-metadata.json");
    for bad in [
        "{not json",
        r#"{"entries": {"s": {"starred": "yes"}}}"#,
        r#"{"entries": {"s": {"customTitle": "a", "custom_title": "b"}}}"#,
        r#"{"version": -1, "entries": {}}"#,
        r#"[1, 2]"#,
    ] {
        std::fs::write(&p, bad).unwrap();
        let e = answer_annotate_at(&p, &json!({"sid": "s2", "patch": {"hidden": true}}), 1)
            .expect_err("读不懂也写了");
        assert_eq!(e.0, "annotations_unreadable", "{bad}：{e:?}");
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            bad,
            "读不懂那份被动了"
        );
        let e = answer_forget_at(&p, &json!({"sid": "s"})).expect_err("读不懂也删了");
        assert_eq!(e.0, "annotations_unreadable");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), bad);
        assert!(
            matches!(load_at(&p), Loaded::Unreadable(_)),
            "读不懂要说「读不懂」，不许说成空表：{bad}"
        );
    }
    // 临时文件没留下（写口的旁名是 `<那份文件名>.….tmp`）。
    let stray: Vec<_> = guard_core::scan_tree!(&d, &["tmp"])
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(stray.is_empty(), "拒写之后留下了临时文件：{stray:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// patch 语义逐格照搬 monitor 从前那份：缺格 / `null` 不改；空白串清空；多一格拒。
#[test]
fn patch_semantics_match_what_the_monitor_did() {
    let p = temp_copy("patch");
    let sid = "0000aaaa-0000-4000-8000-000000000001";
    // null 不改（plain default：`null` 到不了「清空」那一档）。
    let e =
        answer_annotate_at(&p, &json!({"sid": sid, "patch": {"customTitle": null}}), 7).unwrap();
    assert_eq!(e["entry"]["customTitle"], "占位标题一");
    // 空白串清空。
    let e =
        answer_annotate_at(&p, &json!({"sid": sid, "patch": {"customTitle": "  "}}), 8).unwrap();
    assert_eq!(e["entry"]["customTitle"], Value::Null);
    assert_eq!(e["entry"]["starred"], true, "没给的格被动了");
    // 新的一条：从缺省起。
    let e = answer_annotate_at(&p, &json!({"sid": "fresh", "patch": {"hidden": true}}), 9).unwrap();
    assert_eq!(
        e["entry"],
        json!({"starred": false, "customTitle": null, "hidden": true, "updatedAt": 9})
    );
    // 坏入参拒，文件不动。
    let before = std::fs::read(&p).unwrap();
    for bad in [
        json!({"patch": {}}),
        json!({"sid": "", "patch": {}}),
        json!({"sid": "x"}),
        json!({"sid": "x", "patch": {"bogus": 1}}),
        // 「上次用哪个号起的」不归注解（会话所在那台的起会话账号记录）。
        json!({"sid": "x", "patch": {"lastAccount": "a"}}),
        json!({"sid": "x", "patch": {"starred": "yes"}}),
    ] {
        assert_eq!(
            answer_annotate_at(&p, &bad, 1).unwrap_err().0,
            "bad_args",
            "{bad}"
        );
    }
    assert_eq!(std::fs::read(&p).unwrap(), before, "坏入参也写了");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 忘掉一条（删会话时连带）：只那一条没了，其余逐格不动；不在就不写。
#[test]
fn forgetting_removes_exactly_that_entry() {
    let p = temp_copy("forget");
    let sid = "0000aaaa-0000-4000-8000-000000000004";
    assert_eq!(
        answer_forget_at(&p, &json!({"sid": sid})).unwrap(),
        json!({"removed": true})
    );
    let mut want = golden();
    want.as_object_mut().unwrap().remove(sid);
    assert_eq!(norm(&read_table(&p)), want);
    let before = std::fs::read(&p).unwrap();
    assert_eq!(
        answer_forget_at(&p, &json!({"sid": "nope"})).unwrap(),
        json!({"removed": false})
    );
    assert_eq!(std::fs::read(&p).unwrap(), before, "不在的那一条也写了一遍");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 文件不在：读 = 空表（同 monitor），写 = 从缺省形状起一份。
#[test]
fn a_missing_file_reads_empty_and_the_first_write_creates_it() {
    let d = std::env::temp_dir().join(format!("ccm-annot-new-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let p = d.join("history-metadata.json");
    assert_eq!(load_at(&p), Loaded::Read(Table::new()));
    answer_annotate_at(&p, &json!({"sid": "s", "patch": {"starred": true}}), 5).unwrap();
    let raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(raw["version"], 0);
    assert_eq!(raw["entries"]["s"]["starred"], true);
    let _ = std::fs::remove_dir_all(&d);
}

/// 位置只跟着家走（期望手写）：默认 `<HOME>/.cc-monitor/history-metadata.json`；`CCM_DATA_DIR`（绝对）⇒ 它根上那一份；
/// 相对 / 没有家目录 ⇒ 没有（不猜）。从前另指它的 `CCM_HISTORY_METADATA` 删了，给了也不认。
#[test]
fn the_location_only_follows_the_home() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    };
    assert_eq!(
        path_from(&env(&[
            ("HOME", "/h"),
            ("CCM_HISTORY_METADATA", "/x/m.json")
        ])),
        Some(PathBuf::from("/h/.cc-monitor/history-metadata.json"))
    );
    assert_eq!(
        path_from(&env(&[("HOME", "/h"), ("CCM_DATA_DIR", "/iso")])),
        Some(PathBuf::from("/iso/history-metadata.json"))
    );
    assert_eq!(
        path_from(&env(&[("HOME", "/h"), ("CCM_DATA_DIR", "rel")])),
        None
    );
    assert_eq!(path_from(&env(&[])), None);
}
