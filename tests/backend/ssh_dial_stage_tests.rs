//! 拨号阶段那一行的形状：带地址的那几支带结构化的那一台与拨号核心写好的那一句（读的那一侧照摆，不按种类取字、不拆地址）；
//! 「过了一步」与它们同一族、字节照旧。

use copy_core::copy_text;

use crate::{stage, Endpoint, StageRow};

fn h22() -> Endpoint {
    Endpoint {
        host: "h".into(),
        port: 22,
    }
}

fn line(row: StageRow) -> serde_json::Value {
    serde_json::to_value(row).expect("一行 JSON")
}

#[test]
fn a_dialing_stage_carries_the_endpoint_and_the_sentence() {
    let v = line(StageRow {
        stage: stage::dialing(h22(), "h:22"),
    });
    assert_eq!(
        v,
        serde_json::json!({ "stage": {
            "kind": "dialing",
            "endpoint": { "host": "h", "port": 22 },
            "said": copy_text("beProbe.stage.dial", &[("endpoint", "h:22")]),
        }})
    );
}

#[test]
fn a_jump_hop_is_named_in_the_sentence_and_structured_in_the_endpoint() {
    let named = copy_text("beConnect.jump.label", &[("hop", "bastion")]);
    let v = line(StageRow {
        stage: stage::failed(h22(), &named, "[tcp] refused"),
    });
    assert_eq!(
        v["stage"]["endpoint"],
        serde_json::json!({ "host": "h", "port": 22 })
    );
    assert_eq!(
        v["stage"]["said"],
        copy_text(
            "beProbe.stage.failed",
            &[("endpoint", &named), ("reason", "[tcp] refused")]
        )
    );
}

#[test]
fn every_stage_but_a_step_says_its_sentence() {
    let all = [
        stage::dialing(h22(), "h:22"),
        stage::host_key(h22(), "h:22", "SHA256:x"),
        stage::failed(h22(), "h:22", "[tcp] refused"),
        stage::won(h22(), "h:22"),
        stage::auth_ok(),
        stage::auth_failed("钥匙不对"),
        stage::established(),
    ];
    for st in all {
        let v = line(StageRow { stage: st });
        assert!(
            v["stage"]["said"].as_str().is_some_and(|s| !s.is_empty()),
            "{v}"
        );
    }
    assert_eq!(
        line(StageRow {
            stage: stage::auth_failed("钥匙不对")
        })["stage"]["ok"],
        false
    );
}

#[test]
fn a_step_line_keeps_its_bytes() {
    let row = StageRow {
        stage: stage::step(crate::step::TCP),
    };
    assert_eq!(
        serde_json::to_string(&row).expect("一行 JSON"),
        r#"{"stage":{"kind":"step","step":"tcp"}}"#
    );
    let back: StageRow =
        serde_json::from_str(r#"{"stage":{"kind":"step","step":"tcp"}}"#).expect("读得回");
    assert_eq!(back, row);
}
