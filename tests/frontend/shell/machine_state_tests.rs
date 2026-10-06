//! 机器状态成品：每一态怎么到 · 给哪几颗修法 · 线上形状与跨语言金样逐格相等。

use super::*;

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
    dial_failed(&o, Some("auth"));
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
    dial_failed(&o, None);
    down(&o);
    assert_eq!(product(&o, true).reason.as_deref(), Some("other"));
}

#[test]
fn a_changed_host_key_is_its_own_state() {
    let o = fresh("指纹");
    dial_failed(&o, Some("host_key"));
    down(&o);
    let p = product(&o, true);
    assert_eq!(p.state, MachineStateKind::HostKeyChanged);
    assert_eq!(p.fixes, vec![MachineFix::CompareFingerprint]);
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
    dial_failed(&o, Some("timeout"));
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
                dial_failed(&o, Some("auth"));
                down(&o)
            }
            "无应答" => {
                dial_failed(&o, Some("timeout"));
                down(&o)
            }
            "指纹变了" => {
                dial_failed(&o, Some("host_key"));
                down(&o)
            }
            "要更新" => up(&o, "p7a", VersionRelation::Older),
            "较新" => up(&o, "p9a", VersionRelation::Newer),
            "版本不可比" => up(&o, "dev", VersionRelation::Incomparable),
            "不让转发" => unsupported(&o, NO_FORWARDING),
            "停用" => {}
            "本机没连上" => return local_product(false, Some("p7z")),
            other => panic!("金样里多了一形：{other}"),
        }
        product(&o, name != "停用")
    };
    let cases = golden["cases"].as_array().expect("金样没有 cases");
    assert_eq!(cases.len(), 11, "金样的形数变了 —— 两侧一起改");
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
