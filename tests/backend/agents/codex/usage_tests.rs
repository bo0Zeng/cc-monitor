//! Codex 报用量与开窗：喂给一次性 app-server 的那几行 · 回包读成两窗 · 开窗那一句的参数。
//! 回包形照 codex-rs `rust-v0.167.0` 的 `GetAccountRateLimitsResponse`（camelCase）手写，值是编的。

use super::*;
use serde_json::{json, Value};

#[test]
fn the_probe_feeds_the_handshake_then_one_rate_limits_read() {
    assert_eq!((FACE.program, FACE.args), ("codex", &["app-server"][..]));
    assert_eq!(FACE.dir_env, "CODEX_HOME");
    assert_eq!(
        FACE.stdin,
        &[
            super::super::app_server::INITIALIZE,
            super::super::app_server::INITIALIZED,
            r#"{"id":2,"method":"account/rateLimits/read"}"#,
        ]
    );
    let enough = FACE.enough.expect("读到回包那一行就够");
    assert!(enough(&json!({"id": 2, "result": {}}).to_string()));
    assert!(!enough(&json!({"id": 1, "result": {}}).to_string()));
    for k in ["CODEX_API_KEY", "OPENAI_API_KEY", "CODEX_ACCESS_TOKEN"] {
        assert!(FACE.env_remove.contains(&k), "{k}");
    }
}

fn reply(result: Value) -> String {
    format!(
        "{}\n{}\n",
        json!({"id": 1, "result": {}}),
        json!({"id": 2, "result": result})
    )
}

fn window(used: i64, mins: i64, resets: i64) -> Value {
    json!({"usedPercent": used, "windowDurationMins": mins, "resetsAt": resets})
}

/// 两窗照它报的时长起名（`300m` · `10080m`），用量折成比例，重置时刻原样（unix 秒）；多桶那一份里有 `codex` ⇒ 读它。
#[test]
fn the_reply_reads_into_the_two_windows() {
    let snap = |used| json!({"limitId": "codex", "primary": window(used, 300, 1_800_000_000), "secondary": window(40, 10080, 1_800_500_000)});
    let got = (FACE.read)(&reply(json!({"rateLimits": snap(25)})), 0).expect("读得懂");
    assert_eq!(got.len(), 2);
    assert_eq!(
        (got[0].name.as_str(), got[0].used, got[0].resets_at),
        ("300m", Some(0.25), Some(1_800_000_000))
    );
    assert_eq!(
        (got[1].name.as_str(), got[1].used, got[1].resets_at),
        ("10080m", Some(0.4), Some(1_800_500_000))
    );
    let multi = json!({"rateLimits": snap(25), "rateLimitsByLimitId": {"codex": snap(60), "other": snap(99)}});
    let got = (FACE.read)(&reply(multi), 0).expect("读得懂");
    assert_eq!(got[0].used, Some(0.6));
}

/// 回包是 `error` · 没有 `"id":2` 那一行 · 一窗都没带 · 别的一族 ⇒ 读不懂（不猜）。
#[test]
fn odd_replies_are_unreadable() {
    let err = format!(
        "{}\n",
        json!({"id": 2, "error": {"code": 1, "message": "not logged in"}})
    );
    assert!((FACE.read)(&err, 0).is_err());
    assert!((FACE.read)("", 0).is_err());
    assert!((FACE.read)(&reply(json!({"rateLimits": {"limitId": "codex"}})), 0).is_err());
    let other = json!({"rateLimits": {"limitId": "other", "primary": window(1, 300, 1)}});
    assert!((FACE.read)(&reply(other), 0).is_err());
}

/// 开窗：`exec --ephemeral --skip-git-repo-check <一句>`（不落会话记录 · 目录不是 git 仓也照跑）。
#[test]
fn the_warm_up_is_one_ephemeral_exec() {
    assert_eq!(
        &WARM.args[..3],
        &["exec", "--ephemeral", "--skip-git-repo-check"]
    );
    assert_eq!(WARM.args.len(), 4);
    assert!(WARM.env_set.is_empty());
}
