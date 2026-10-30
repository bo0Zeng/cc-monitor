use super::*;
use crate::session::Launch;
use crate::{Machine, Sid};

fn sid(s: &str) -> Sid {
    s.parse().unwrap()
}

fn failed_one() -> SessionOutcome {
    SessionOutcome {
        machine: Machine::named("box2"),
        sid: sid("s-3"),
        outcome: SessionState::Failed,
        why: Some("wrong_owner".into()),
        said: Some("不是这一端起的".into()),
        copy_detail: "sessions-stop · wrong_owner".into(),
        title: "proj 改测试".into(),
        session: None,
    }
}

#[test]
fn sessions_batch_reply_wire_shape_keeps_null_keys_and_reads_back() {
    let reply = SessionsBatchReply {
        results: vec![
            SessionOutcome {
                machine: Machine::local(),
                sid: sid("s-1"),
                outcome: SessionState::Done,
                why: None,
                said: None,
                copy_detail: String::new(),
                title: "proj 起头".into(),
                session: Some("proj-cc".into()),
            },
            failed_one(),
        ],
        said: "结束 2 项 · 完成 1 · 失败 ×1".into(),
        failed: true,
        lines: vec!["失败 · proj 改测试（box2） · 不是这一端起的".into()],
        copy_detail: "sessions-stop · wrong_owner".into(),
    };
    let v = serde_json::to_value(&reply).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys.len(), 5, "{v}");
    for k in ["results", "said", "failed", "lines", "copyDetail"] {
        assert!(v.get(k).is_some(), "缺 {k}：{v}");
    }
    let done = &v["results"][0];
    assert_eq!(done["machine"], "<local>");
    assert_eq!(done["outcome"], "done");
    assert!(
        done["why"].is_null() && done.get("why").is_some(),
        "可缺的出 null：{done}"
    );
    assert!(
        done["said"].is_null() && done.get("said").is_some(),
        "{done}"
    );
    assert_eq!(done["copyDetail"], "");
    assert_eq!(done["title"], "proj 起头");
    assert_eq!(v["results"][1]["outcome"], "failed");
    assert!(v["results"][1]["session"].is_null() && v["results"][1].get("session").is_some());
    let back: SessionsBatchReply = serde_json::from_value(v).unwrap();
    assert_eq!(back, reply);
}

#[test]
fn session_outcome_takes_only_three_words() {
    let mut v = serde_json::to_value(failed_one()).unwrap();
    for w in ["done", "skipped", "failed"] {
        v["outcome"] = w.into();
        serde_json::from_value::<SessionOutcome>(v.clone()).unwrap();
    }
    v["outcome"] = "cancelled".into();
    assert!(serde_json::from_value::<SessionOutcome>(v).is_err());
}

#[test]
fn sessions_batch_req_ops_on_the_wire() {
    let stop: SessionsBatchReq = serde_json::from_value(serde_json::json!({
        "op": "stop",
        "items": [{"machine": "<local>", "sid": "s-1"}, {"machine": "box2", "sid": "s-3"}],
    }))
    .unwrap();
    assert_eq!(stop.op, SessionsOp::Stop);
    assert_eq!(stop.items.len(), 2);
    assert!(stop.items[0].machine.is_local());
    assert_eq!(stop.items[1].machine, Machine::named("box2"));
    assert_eq!(stop.client, None);

    let start = SessionsBatchReq {
        op: SessionsOp::Start { run: Launch::Tmux },
        items: vec![SessionItem {
            machine: Machine::local(),
            sid: sid("s-1"),
        }],
        client: Some("desktop".into()),
    };
    let v = serde_json::to_value(&start).unwrap();
    assert_eq!(v["op"], serde_json::json!({"start": {"run": "tmux"}}));
    assert_eq!(
        serde_json::from_value::<SessionsBatchReq>(v).unwrap(),
        start
    );

    // 坏 sid 在解入参时就拒；多出的键拒。
    assert!(
        serde_json::from_value::<SessionsBatchReq>(serde_json::json!({
            "op": "stop", "items": [{"machine": "<local>", "sid": "坏 id"}],
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionsBatchReq>(serde_json::json!({
            "op": "stop", "items": [], "extra": 1,
        }))
        .is_err()
    );
}
