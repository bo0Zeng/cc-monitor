//! 版本门的读法：严格收那两格 · 手上没带身份就不问、照接。
use super::*;

#[test]
fn the_verdict_is_read_strictly() {
    assert_eq!(
        decode_verdict(&serde_json::json!({"action": "replace", "older": true})),
        Ok(Verdict {
            replace: true,
            older: true
        })
    );
    assert_eq!(
        decode_verdict(&serde_json::json!({"action": "attach", "older": false})),
        Ok(Verdict {
            replace: false,
            older: false
        })
    );
    for bad in [
        serde_json::json!({"action": "attach"}),
        serde_json::json!({"action": "swap", "older": true}),
        serde_json::json!({"action": "attach", "older": false, "x": 1}),
    ] {
        assert!(decode_verdict(&bad).is_err(), "{bad}");
    }
}

#[tokio::test(start_paused = true)]
async fn without_an_own_identity_nothing_is_asked_and_the_link_goes_ahead() {
    let got: Result<Verdict, ()> = verdict_for(None, |_| async { panic!("不该问") }).await;
    assert_eq!(
        got,
        Ok(Verdict {
            replace: false,
            older: false
        })
    );
}
