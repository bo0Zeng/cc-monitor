//! 机器状态成品：每一态怎么到 · 给哪几颗修法 · 线上形状与跨语言金样逐格相等。

use super::*;

/// 拨号那一层带回的那一句 ＋ 一份固定的详情（金样要逐字比，不用带时刻的那几形）。
fn said(s: &str) -> crate::detail::Said {
    crate::detail::Said {
        said: s.to_string(),
        detail: "命令：dial\n原话：raw words".to_string(),
    }
}

/// 每条判据一台自己的机器（表是进程内共享的）。
fn fresh(name: &str) -> String {
    let o = format!("ms-{name}");
    forget(&o);
    o
}

#[test]
fn a_round_walks_connecting_then_up_with_the_version() {
    let o = fresh("走一轮");
    assert_eq!(
        product(&o, true).state,
        MachineStateKind::Unknown,
        "还没轮过就有了结论"
    );
    connecting(&o, "deploy");
    connecting(&o, "attach");
    let p = product(&o, true);
    assert_eq!(
        (p.state, p.stage.as_deref()),
        (MachineStateKind::Connecting, Some("attach"))
    );
    up(&o, "p7z", VersionRelation::Same);
    let p = product(&o, true);
    assert_eq!(p.state, MachineStateKind::Up);
    assert_eq!(
        p.version.as_deref(),
        Some(PRODUCT_VERSION),
        "同一份字节报的是产品版本号，不是构建标识"
    );
    assert_eq!(p.stage, None, "连上了还挂着「正在连」那一步");
    assert!(p.fixes.is_empty());
}

#[test]
fn a_failed_round_takes_the_reason_the_dial_layer_noted() {
    let o = fresh("没连上");
    connecting(&o, "deploy");
    dial_failed(&o, Some("auth"), None, &said("拨号没成"));
    down(&o);
    let p = product(&o, true);
    assert_eq!(
        (p.state, p.reason.as_deref()),
        (MachineStateKind::Down, Some("auth"))
    );
    assert_eq!(p.fixes, vec![MachineFix::PushKey, MachineFix::ConnSettings]);
    // 原因码只用一次：下一轮没带码 ⇒ 说不清（`other`），不沿用上一轮的「密钥被拒」。
    down(&o);
    assert_eq!(product(&o, true).reason.as_deref(), Some("other"));
    // 老后端没给码 ⇒ 不记。
    dial_failed(&o, None, None, &said("拨号没成"));
    down(&o);
    assert_eq!(product(&o, true).reason.as_deref(), Some("other"));
}

#[test]
fn a_changed_host_key_is_its_own_state() {
    let o = fresh("指纹");
    dial_failed(&o, Some("host_key"), Some("SHA256:new"), &said("拨号没成"));
    down(&o);
    let p = product(&o, true);
    assert_eq!(p.state, MachineStateKind::HostKeyChanged);
    assert_eq!(p.fixes, vec![MachineFix::CompareFingerprint]);
    assert_eq!(
        p.seen_host_key.as_deref(),
        Some("SHA256:new"),
        "比对框要的那枚指纹没带出来"
    );
    // 下一轮换成别的原因 ⇒ 那枚指纹不再挂着。
    dial_failed(&o, Some("timeout"), None, &said("拨号没成"));
    down(&o);
    assert_eq!(product(&o, true).seen_host_key, None);
}

#[test]
fn deploying_says_install_first_then_update_once_seen() {
    let o = fresh("装");
    deploying_auto(&o);
    assert_eq!(product(&o, true).state, MachineStateKind::Installing);
    up(&o, "p7a", VersionRelation::Older);
    deploying_auto(&o);
    assert_eq!(
        product(&o, true).state,
        MachineStateKind::Updating,
        "握过手的那台换版本说成了「安装」"
    );
    deploying(&o, true);
    assert_eq!(product(&o, true).fixes, Vec::<MachineFix>::new());
}

#[test]
fn a_foreign_local_backend_is_judged_by_build_order() {
    let _g = crate::inbound_client::local_origin_test_lock();
    note_local_foreign(Some("p7a-old"));
    let p = local_product(false, Some("p8c-mine"));
    assert_eq!(
        (p.state, p.version_relation),
        (MachineStateKind::NeedsUpdate, Some(VersionRelation::Older))
    );
    assert_eq!(p.fixes, vec![MachineFix::Retry], "本机那一台的修法只有重试");
    note_local_foreign(Some("p9a-new"));
    assert_eq!(
        local_product(false, Some("p8c-mine")).state,
        MachineStateKind::Newer
    );
    note_local_foreign(Some("dev-tree"));
    assert_eq!(
        local_product(false, Some("p8c-mine")).state,
        MachineStateKind::Down
    );
    // 接上了就是连着，版本是手上这一版。
    note_local_foreign(None);
    let p = local_product(true, Some("p8c-mine"));
    assert_eq!(
        (p.state, p.version_relation),
        (MachineStateKind::Up, Some(VersionRelation::Same))
    );
    assert_eq!(
        local_product(false, Some("p8c-mine")).state,
        MachineStateKind::Down
    );
}

#[test]
fn the_version_relation_decides_up_needs_update_newer() {
    for (rel, want, fixes) in [
        (VersionRelation::Same, MachineStateKind::Up, vec![]),
        (VersionRelation::Incomparable, MachineStateKind::Up, vec![]),
        (
            VersionRelation::Older,
            MachineStateKind::NeedsUpdate,
            vec![MachineFix::Update],
        ),
        (VersionRelation::Newer, MachineStateKind::Newer, vec![]),
    ] {
        let o = fresh(&format!("{rel:?}"));
        up(&o, "b1", rel);
        let p = product(&o, true);
        assert_eq!((p.state, p.version_relation), (want, Some(rel)), "{rel:?}");
        // 只有同一份字节报得出产品版本号；构建标识不上界面。
        assert_eq!(p.version.is_some(), rel == VersionRelation::Same, "{rel:?}");
        assert_ne!(p.version.as_deref(), Some("b1"), "构建标识漏进了版本那一格");
        assert_eq!(p.fixes, fixes, "{rel:?}");
    }
}

#[test]
fn disabled_wins_over_whatever_the_table_says() {
    let o = fresh("停用");
    dial_failed(&o, Some("timeout"), None, &said("拨号没成"));
    down(&o);
    let p = product(&o, false);
    assert_eq!(p.state, MachineStateKind::Disabled);
    assert_eq!(p.reason, None, "停用了还说上一轮为什么没连上");
    assert_eq!(p.fixes, vec![MachineFix::Connect]);
}

#[test]
fn every_down_reason_gets_its_fixes() {
    use MachineFix as F;
    for (why, want) in [
        ("resolve", vec![F::ConnSettings]),
        ("unreachable", vec![F::Retry, F::ConnSettings]),
        ("timeout", vec![F::Retry, F::ConnSettings]),
        ("auth", vec![F::PushKey, F::ConnSettings]),
        ("key_unreadable", vec![F::PushKey, F::ConnSettings]),
        ("password", vec![F::PushKey]),
        ("jump", vec![F::ConnSettings]),
        ("other", vec![F::Retry, F::ConnSettings]),
        (LOCAL_DOWN, vec![F::Retry]),
    ] {
        assert_eq!(fixes_of(MachineStateKind::Down, Some(why)), want, "{why}");
    }
}

/// 金样：每一形由生产函数现产，序列化后与 `tests/__fixtures__/machine-state.golden.json` 逐格相等；
/// 界面那一侧（`tests/frontend/ui/settings/machine-state.vitest.ts`）读同一份，逐形解码、逐形画。
#[test]
fn the_product_matches_the_cross_language_golden() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/machine-state.golden.json"))
            .expect("金样不是 JSON");
    let make = |name: &str| -> MachineState {
        let o = fresh(&format!("金样-{name}"));
        match name {
            "连着" => up(&o, "p7z", VersionRelation::Same),
            "正在连" => connecting(&o, "attach"),
            "密钥被拒" => {
                dial_failed(&o, Some("auth"), None, &said("拨号没成"));
                down(&o)
            }
            "无应答" => {
                dial_failed(&o, Some("timeout"), None, &said("拨号没成"));
                down(&o)
            }
            "指纹变了" => {
                dial_failed(&o, Some("host_key"), Some("SHA256:new"), &said("拨号没成"));
                down(&o)
            }
            "要更新" => up(&o, "p7a", VersionRelation::Older),
            "较新" => up(&o, "p9a", VersionRelation::Newer),
            "版本不可比" => up(&o, "dev", VersionRelation::Incomparable),
            "不让转发" => unsupported(&o, NO_FORWARDING),
            "要密码" => {
                dial_failed(&o, Some("password"), None, &said("拨号没成"));
                down(&o)
            }
            "正在装" => deploying(&o, true),
            "正在更新" => deploying(&o, false),
            "停用" => {}
            "本机没连上" => {
                let _g = crate::inbound_client::local_origin_test_lock();
                note_local_foreign(None);
                return local_product(false, Some("p7z"));
            }
            other => panic!("金样里多了一形：{other}"),
        }
        product(&o, name != "停用")
    };
    let cases = golden["cases"].as_array().expect("金样没有 cases");
    assert_eq!(cases.len(), 14, "金样的形数变了 —— 两侧一起改");
    for c in cases {
        let name = c["name"].as_str().unwrap();
        // 金样里产品版本号写成占位 `<ver>`（发版会改它，金样不跟着改）。
        let got = serde_json::to_value(make(name))
            .unwrap()
            .to_string()
            .replace(&format!("\"{PRODUCT_VERSION}\""), "\"<ver>\"");
        let got: serde_json::Value = serde_json::from_str(&got).unwrap();
        assert_eq!(
            got, c["machine"],
            "「{name}」：现产的成品与金样对不上\n现产：{got}"
        );
    }
}

/// 没连上那一轮带复制详情：拨号那一层的那一句进「断在」、连的是哪台进「对象」、码是这一轮的原因码，原话照后端那份；
/// 接常驻后端那一步记下的比拨号那一层的优先；新一轮起步 · 连上 ⇒ 清掉；停用那一行不带。
#[test]
fn a_failed_round_carries_the_copy_detail() {
    let o = fresh("带详情");
    connecting(&o, "deploy");
    dial_failed(&o, Some("auth"), None, &said("step-said"));
    down(&o);
    let d = product(&o, true).detail.expect("没连上那一轮没有详情");
    assert!(d.contains("断在：step-said"), "{d}");
    assert!(d.contains(&format!("对象：{o}")), "{d}");
    assert!(d.contains("码：auth"), "{d}");
    assert!(d.contains("原话：raw words"), "{d}");
    assert_eq!(product(&o, false).detail, None, "停用那一行带了详情");
    // 接常驻后端那一步记下的优先（它比拨号那一层更完整）。
    connecting(&o, "deploy");
    dial_failed(&o, None, None, &said("拨号那一层"));
    round_failed(&crate::origin::Origin(o.clone()), &said("round-said"));
    down(&o);
    let d = product(&o, true).detail.unwrap();
    assert!(d.contains("断在：round-said"), "{d}");
    // 新一轮起步就清掉；这一轮什么都没记 ⇒ 没有详情（不沿用上一轮的）。
    connecting(&o, "deploy");
    assert_eq!(product(&o, true).detail, None, "新一轮还挂着上一轮的详情");
    down(&o);
    assert_eq!(product(&o, true).detail, None, "沿用了上一轮的详情");
    // 连上 ⇒ 清掉。
    dial_failed(&o, Some("timeout"), None, &said("x"));
    up(&o, "p7z", VersionRelation::Same);
    assert_eq!(product(&o, true).detail, None);
}
