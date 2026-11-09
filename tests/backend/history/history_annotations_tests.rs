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
//! 2. **写一条不丢别的**：夹具拷进临时目录 → 改一条 → 再读：其余每条逐格 == 金样；被改那条只变了 patch 那几格 ＋ `updatedAt`；
//!    条目里 / 顶层认不出的键原样还在；被改那条身上的蛇形别名摘了（否则下次严格读读不懂）。
//! 3. **读不懂就不写**：坏 JSON · 字段类型不对 · 同一格驼峰与蛇形都在 · 顶层不是对象 ⇒ 拒，文件逐字节不变。
//! 4. patch 语义逐格（缺格 / `null` 不改；空白串清空）；忘掉一条；路径只认绝对路径。
//!
//! # 买不到
//!
//! - 🔴 用户那台机器上的**真** `history-metadata.json`：devbox 上没有这份文件（monitor 主要跑在另一台），真文件的前后对拍没跑。
//! - 两个后端进程同时写；真 Windows 上的原子挪。

use super::*;
use crate::common::said::Said;
use crate::wire::typed::{take, Codes as _};

/// 线上那一份入参照分派那一层解（`typed::take`）再交本体；回包 · 失败都投成线上那一份（判据比的是线上形状）。
fn answer_annotate_at(own: &Own, args: &Value, now: i64) -> Result<Value, Said> {
    let req = take::<AnnotateReq>(args)?;
    let got = annotate_at(own, req, now).map_err(|e| e.into_said())?;
    Ok(serde_json::to_value(got).expect("回包总能序列化"))
}

/// 删会话连带删注解那一下（[`forget_at`]）：失败照码比（码投成 [`Said`] 那一份）。
fn answer_forget_at(own: &Own, sid: &str) -> Result<bool, Said> {
    forget_at(own, sid).map_err(|e| e.into_said())
}

fn fixture_dir() -> PathBuf {
    guard_core::addr::repo_root().join("tests/__fixtures__")
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

/// 写句柄的根 ＝ 那份文件所在的临时目录（数据目录的替身）。
fn own_of(p: &Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_data_at(p.parent().expect("临时文件有上一级"))
}

fn read_table(p: &Path) -> Table {
    match load_at(p) {
        Loaded::Read(t) => t,
        other => panic!("读不动：{other:?}"),
    }
}

/// ★ 判据 1：读夹具 == 金样（逐条、逐格；蛇形别名与驼峰读成同一格）。
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
    let got = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"starred": true}}),
        4242,
    )
    .unwrap();
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
        let e = answer_annotate_at(
            &own_of(&p),
            &json!({"sid": "s2", "patch": {"hide": "on"}}),
            1,
        )
        .expect_err("读不懂也写了");
        assert_eq!(
            e.code.as_deref(),
            Some("annotations_unreadable"),
            "{bad}：{e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            bad,
            "读不懂那份被动了"
        );
        let e = answer_forget_at(&own_of(&p), "s").expect_err("读不懂也删了");
        assert_eq!(e.code.as_deref(), Some("annotations_unreadable"));
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

/// patch 语义逐格：缺格 / `null` 不改；空白串清空；多一格拒。
#[test]
fn patch_semantics_match_what_the_monitor_did() {
    let p = temp_copy("patch");
    let sid = "0000aaaa-0000-4000-8000-000000000001";
    // null 不改（plain default：`null` 到不了「清空」那一档）。
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"customTitle": null}}),
        7,
    )
    .unwrap();
    assert_eq!(e["entry"]["customTitle"], "占位标题一");
    // 回包带改之前改过的名（`prev_title`）：tab 上只知道定好的名，撤销改名照它交回去。
    assert_eq!(e["prev_title"], "占位标题一", "{e}");
    // 空白串清空。
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"customTitle": "  "}}),
        8,
    )
    .unwrap();
    assert_eq!(e["entry"]["customTitle"], Value::Null);
    assert_eq!(e["prev_title"], "占位标题一", "{e}");
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"customTitle": "修登录"}}),
        8,
    )
    .unwrap();
    assert_eq!(e["entry"]["customTitle"], "修登录");
    assert_eq!(
        e["prev_title"],
        Value::Null,
        "改之前没有改过的名 ⇒ null：{e}"
    );
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"customTitle": ""}}),
        8,
    )
    .unwrap();
    assert_eq!(
        e["entry"]["customTitle"],
        Value::Null,
        "改成空 ＝ 回到原标题"
    );
    assert_eq!(e["prev_title"], "修登录", "{e}");
    assert_eq!(e["entry"]["starred"], true, "没给的格被动了");
    // 新的一条：从缺省起。
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "fresh", "patch": {"hide": "on"}}),
        9,
    )
    .unwrap();
    assert_eq!(
        e["entry"],
        json!({"starred": false, "customTitle": null, "hide": "on", "updatedAt": 9})
    );
    // ★ 回包带改之前那一格（`prev`）：撤销照它交回去 ——
    //   先无单独设置 → 隐藏（`prev: null`）→ 撤销交 `rule` ⇒ 注解里那一格回到没有（不是钉成「单独设为显示」）。
    assert_eq!(e["prev"], Value::Null, "{e}");
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "fresh", "patch": {"hide": "rule"}}),
        10,
    )
    .unwrap();
    assert_eq!(e["prev"], "on", "{e}");
    assert!(
        e["entry"].get("hide").is_none(),
        "撤销后那一格该回到没有：{e}"
    );
    let disk: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert!(disk["entries"]["fresh"].get("hide").is_none(), "{disk}");
    // 原本单独设为显示的：隐藏时 `prev: "off"`；不改隐藏那一格的补丁也照实回那一格。
    answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "fresh", "patch": {"hide": "off"}}),
        11,
    )
    .unwrap();
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "fresh", "patch": {"hide": "on"}}),
        12,
    )
    .unwrap();
    assert_eq!(e["prev"], "off", "{e}");
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "fresh", "patch": {"starred": true}}),
        13,
    )
    .unwrap();
    assert_eq!(e["prev"], "on", "{e}");
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
            answer_annotate_at(&own_of(&p), &bad, 1)
                .unwrap_err()
                .code
                .as_deref(),
            Some("bad_args"),
            "{bad}"
        );
    }
    assert_eq!(std::fs::read(&p).unwrap(), before, "坏入参也写了");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 固定是会话上的一格（`pinned`）：翻过去 · 翻回来，别的格不动，摘光了盘上那一格也没了；组那一节写注解时原样留着。
/// 进出组不走这里（`groups-write move`，组员关系与那一片的组记录同一把锁里一起改）：patch 带 `join` · `leave` ⇒ 拒、文件不动。
#[test]
fn pin_is_a_cell_on_the_session_and_membership_is_not_patched_here() {
    let p = temp_copy("groups");
    let sid = "0000aaaa-0000-4000-8000-000000000001";
    let mut raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    raw["groups"] = json!({"g1": {"name": "开发", "made": 1, "named": 1}});
    std::fs::write(&p, serde_json::to_vec(&raw).unwrap()).unwrap();
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"pinned": true}}),
        7,
    )
    .unwrap();
    assert_eq!(e["entry"]["pinned"], true);
    assert_eq!(e["entry"]["customTitle"], "占位标题一", "别的格不动");
    answer_annotate_at(
        &own_of(&p),
        &json!({"sid": sid, "patch": {"pinned": false}}),
        9,
    )
    .unwrap();
    let disk: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert!(disk["entries"][sid].get("pinned").is_none(), "{disk}");
    assert_eq!(disk["groups"]["g1"]["name"], "开发", "组那一节原样");
    let before = std::fs::read(&p).unwrap();
    for patch in [json!({"join": ["g1"]}), json!({"leave": ["g1"]})] {
        assert_eq!(
            answer_annotate_at(&own_of(&p), &json!({"sid": sid, "patch": patch}), 10)
                .unwrap_err()
                .code
                .as_deref(),
            Some("bad_args"),
            "{patch}"
        );
    }
    assert_eq!(std::fs::read(&p).unwrap(), before);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 「无人答时自动答」那一格的值域：0 ＝ 关 · 超了分钟上限拒 · 那一句去首尾空白、空白 ⇒ 缺省 · 过长拒；分钟上限短于那一家给 hook 的期限（到点时 hook 还活着）。
#[test]
fn the_auto_answer_setting_is_checked_before_it_is_written() {
    let on = |min: u32, say: Option<&str>| cmd_wire::needs::AutoAnswer {
        after_min: min,
        say: say.map(str::to_string),
    };
    assert_eq!(auto_checked(on(0, Some("x"))), Ok(None), "0 ＝ 关");
    assert!(auto_checked(on(AUTO_MAX_MIN + 1, None)).is_err());
    assert_eq!(
        auto_checked(on(3, Some("  往下做  "))),
        Ok(Some(on(3, Some("往下做"))))
    );
    assert_eq!(auto_checked(on(3, Some("   "))), Ok(Some(on(3, None))));
    assert!(auto_checked(on(3, Some(&"字".repeat(501)))).is_err());
    assert!(u64::from(AUTO_MAX_MIN) * 60 < crate::agents::claudecode::asks::HOOK_TIMEOUT_SECS);
}

/// 「无人答时自动答」那一格：写上 · 只换分钟留着那一句 · 空白那一句回到缺省 · 0 摘掉（盘上那一格也没了）· 超了上限拒、文件不动。
#[test]
fn the_auto_answer_cell_is_written_kept_and_removed() {
    let p = temp_copy("auto");
    let at = |patch: Value, now: i64| {
        answer_annotate_at(&own_of(&p), &json!({"sid": "fresh", "patch": patch}), now)
    };
    let e = at(
        json!({"autoAnswer": {"afterMin": 10, "say": " 往下做 "}}),
        1,
    )
    .unwrap();
    assert_eq!(
        e["entry"]["autoAnswer"],
        json!({"afterMin": 10, "say": "往下做"})
    );
    let e = at(json!({"autoAnswer": {"afterMin": 5}}), 2).unwrap();
    assert_eq!(
        e["entry"]["autoAnswer"],
        json!({"afterMin": 5, "say": "往下做"}),
        "只换分钟"
    );
    let e = at(json!({"starred": true}), 3).unwrap();
    assert_eq!(
        e["entry"]["autoAnswer"],
        json!({"afterMin": 5, "say": "往下做"}),
        "没给的格不动"
    );
    let e = at(json!({"autoAnswer": {"afterMin": 5, "say": "  "}}), 4).unwrap();
    assert_eq!(
        e["entry"]["autoAnswer"],
        json!({"afterMin": 5}),
        "空白 ⇒ 缺省那一句"
    );
    assert_eq!(
        auto_answer_at(&p, "fresh"),
        Some(cmd_wire::needs::AutoAnswer {
            after_min: 5,
            say: None
        })
    );
    let before = std::fs::read(&p).unwrap();
    assert_eq!(
        at(json!({"autoAnswer": {"afterMin": AUTO_MAX_MIN + 1}}), 5)
            .unwrap_err()
            .code
            .as_deref(),
        Some("bad_args")
    );
    assert_eq!(std::fs::read(&p).unwrap(), before, "拒了也写了");
    let e = at(json!({"autoAnswer": {"afterMin": 0}}), 6).unwrap();
    assert!(e["entry"].get("autoAnswer").is_none(), "{e}");
    let disk: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert!(
        disk["entries"]["fresh"].get("autoAnswer").is_none(),
        "{disk}"
    );
    assert_eq!(auto_answer_at(&p, "fresh"), None);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 改了「无人答时自动答」那一格 ⇒ 推 `changed {needs, key: sid}`（出口照它重读会话事实）；只改别的格不推。
#[test]
fn changing_the_auto_answer_cell_pushes_needs_for_that_session() {
    let p = temp_copy("auto-push");
    let mut rx = crate::history::needs_changed::changes().subscribe();
    let pushed = |rx: &mut tokio::sync::broadcast::Receiver<Option<String>>, sid: &str| {
        let mut hit = false;
        while let Ok(s) = rx.try_recv() {
            hit |= s.as_deref() == Some(sid);
        }
        hit
    };
    let at = |sid: &str, patch: Value| {
        answer_annotate_at(&own_of(&p), &json!({"sid": sid, "patch": patch}), 1).unwrap()
    };
    at("auto-push-a", json!({"starred": true}));
    assert!(!pushed(&mut rx, "auto-push-a"), "只改星标不推");
    at("auto-push-a", json!({"autoAnswer": {"afterMin": 5}}));
    assert!(pushed(&mut rx, "auto-push-a"));
    at("auto-push-a", json!({"autoAnswer": {"afterMin": 0}}));
    assert!(pushed(&mut rx, "auto-push-a"), "关掉也推");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 忘掉一条（删会话时连带）：只那一条没了，其余逐格不动；不在就不写。
#[test]
fn forgetting_removes_exactly_that_entry() {
    let p = temp_copy("forget");
    let sid = "0000aaaa-0000-4000-8000-000000000004";
    assert_eq!(answer_forget_at(&own_of(&p), sid).unwrap(), true);
    let mut want = golden();
    want.as_object_mut().unwrap().remove(sid);
    assert_eq!(norm(&read_table(&p)), want);
    let before = std::fs::read(&p).unwrap();
    assert_eq!(answer_forget_at(&own_of(&p), "nope").unwrap(), false);
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
    answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "s", "patch": {"starred": true}}),
        5,
    )
    .unwrap();
    let raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(raw["version"], 1);
    assert_eq!(raw["entries"]["s"]["starred"], true);
    let _ = std::fs::remove_dir_all(&d);
}

/// 位置只跟着家走（期望手写）：默认 `<HOME>/.cc-monitor/history-metadata.json`；`CCM_DATA_DIR`（绝对）⇒ 它根上那一份；
/// 相对 / 没有家目录 ⇒ 没有（不猜）。`CCM_HISTORY_METADATA` 给了也不认。
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
    // 绝对路径照这台平台的写法（Windows 上 `/iso` 没有盘符，不算绝对）。
    const ISO: &str = if cfg!(windows) { r"C:\iso" } else { "/iso" };
    assert_eq!(
        path_from(&env(&[("HOME", "/h"), ("CCM_DATA_DIR", ISO)])),
        Some(PathBuf::from(ISO).join("history-metadata.json"))
    );
    assert_eq!(
        path_from(&env(&[("HOME", "/h"), ("CCM_DATA_DIR", "rel")])),
        None
    );
    assert_eq!(path_from(&env(&[])), None);
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_hands_its_raw_to_the_detail_not_the_sentence() {
    // 那份文件谁都读不了（权限 000）⇒ 读不出来：码照旧，下层原话跟着失败走（进复制详情），不上句子。
    use std::os::unix::fs::PermissionsExt;
    let p = temp_copy("noread");
    let d = p.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "s2", "patch": {"hide": "on"}}),
        1,
    )
    .expect_err("读不出来也写了");
    assert_eq!(e.code.as_deref(), Some("annotations_unreadable"), "{e:?}");
    let raw = e.raw.as_deref().expect("原话丢了");
    assert!(!e.said.contains(raw), "原话上了句子：{e:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 「下次恢复用」那一格：记下的模型 · 权限档读得回；同一个 `end` 上先后记的两样并成一份；记录长过了再记 ⇒ 换掉旧那一份；
/// 改星标那一类写不碰它，别的条目原样。
#[test]
fn the_resume_pick_is_kept_merged_on_one_end_and_replaced_past_it() {
    let p = temp_copy("resume-with");
    let own = own_of(&p);
    let sid = "0000aaaa-0000-4000-8000-0000000000aa";
    let pick = |model: Option<&str>, mode: Option<&str>, end: u64| ResumeWith {
        model: model.map(str::to_string),
        permission_mode: mode.map(str::to_string),
        end,
    };
    let others = |p: &Path| {
        let mut t = read_table(p);
        t.remove(sid);
        t
    };
    let before = others(&p);
    keep_resume_at(&own, sid, pick(Some("sonnet"), None, 40), 1).unwrap();
    assert_eq!(
        resume_with_at(&p, sid),
        Some(pick(Some("sonnet"), None, 40))
    );
    keep_resume_at(&own, sid, pick(None, Some("plan"), 40), 2).unwrap();
    assert_eq!(
        resume_with_at(&p, sid),
        Some(pick(Some("sonnet"), Some("plan"), 40))
    );
    keep_resume_at(&own, sid, pick(None, Some("acceptEdits"), 90), 3).unwrap();
    assert_eq!(
        resume_with_at(&p, sid),
        Some(pick(None, Some("acceptEdits"), 90))
    );
    answer_annotate_at(&own, &json!({"sid": sid, "patch": {"starred": true}}), 4).unwrap();
    assert_eq!(
        resume_with_at(&p, sid),
        Some(pick(None, Some("acceptEdits"), 90))
    );
    assert_eq!(others(&p), before, "别的条目跟着变了");
    assert_eq!(
        resume_with_at(&p, "0000aaaa-0000-4000-8000-000000000002"),
        None
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// 比这一版新的：读不懂 —— 读回那一句、写一律拒、原文一个字节不动。
/// 比这一版老的（`version: 0`）：当没有；写一回 ⇒ 盖成这一版，原文照字节留成 `history-metadata.json.v0`。
#[test]
fn a_newer_file_is_left_as_is_and_an_older_one_is_kept_beside() {
    let d = std::env::temp_dir().join(format!("ccm-annot-ver-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("history-metadata.json");
    let newer = r#"{"version": 2, "entries": {}}"#;
    std::fs::write(&p, newer).unwrap();
    assert!(matches!(load_at(&p), Loaded::Unreadable(_)), "新版读成了表");
    let e = answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "s", "patch": {"starred": true}}),
        1,
    )
    .expect_err("新版也写了");
    assert_eq!(e.code.as_deref(), Some("annotations_unreadable"), "{e:?}");
    let e = answer_forget_at(&own_of(&p), "s").expect_err("新版也删了");
    assert_eq!(e.code.as_deref(), Some("annotations_unreadable"));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), newer, "原文被动了");

    let older = r#"{"version": 0, "entries": {"s": {"hidden": false}}}"#;
    std::fs::write(&p, older).unwrap();
    assert_eq!(load_at(&p), Loaded::Read(Table::new()), "老版该当没有");
    answer_annotate_at(
        &own_of(&p),
        &json!({"sid": "s2", "patch": {"starred": true}}),
        1,
    )
    .expect("老版当没有、照写");
    assert_eq!(
        std::fs::read_to_string(p.with_file_name("history-metadata.json.v0")).unwrap(),
        older,
        "老版原文没留"
    );
    let raw: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    assert_eq!(raw["version"], 1);
    assert!(raw["entries"].get("s").is_none() && raw["entries"]["s2"]["starred"] == true);
    let _ = std::fs::remove_dir_all(&d);
}
