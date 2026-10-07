//! 「复制诊断信息」那一段：拼法 ＋ 记录账的数法。

use super::*;

fn line(
    name: &str,
    state: &str,
    reason: Option<&str>,
    build: Option<&str>,
    records: Option<u64>,
) -> MachineLine {
    MachineLine {
        name: name.to_string(),
        os: Some("Linux".to_string()),
        state: state.to_string(),
        reason: reason.map(str::to_string),
        version: None,
        build: build.map(str::to_string),
        records,
    }
}

#[test]
fn 一段里有版本构建各台状态与原因码未识别数据日志位置() {
    let r = render(
        "4.1.1",
        Some("p8n-mcp-sync"),
        &[
            line("本机", "up", None, Some("p8n-mcp-sync"), Some(0)),
            line("devbox", "down", Some("auth"), None, None),
            line("gpu-01", "up", None, Some("p8m"), Some(3)),
        ],
        &["resumeCommandLocal".to_string(), "x".to_string()],
        Some("/h/.cc-monitor/logs/monitor.2026-10-06.log"),
    );
    for want in [
        "4.1.1",
        "p8n-mcp-sync",
        "devbox · Linux · down · auth",
        &*copy_core::copy_text(
            "diagnostics.unknown.machine",
            &[("machine", "gpu-01 ·"), ("n", "3")],
        ),
        &*copy_core::copy_text(
            "diagnostics.unknown.machineUnread",
            &[("machine", "devbox ·")],
        ),
        "config.json · 2 项：resumeCommandLocal、x",
        "/h/.cc-monitor/logs/monitor.2026-10-06.log",
    ] {
        assert!(
            r.text.contains(want),
            "诊断信息里没有「{want}」：\n{}",
            r.text
        );
    }
    assert_eq!(
        r.unknown,
        vec![
            UnknownOnMachine {
                machine: "本机".into(),
                records: Some(0)
            },
            UnknownOnMachine {
                machine: "devbox".into(),
                records: None
            },
            UnknownOnMachine {
                machine: "gpu-01".into(),
                records: Some(3)
            },
        ],
        "日志页那一行读的数与段里的不是同一份"
    );
    assert_eq!(r.config_unknown, 2);
}

#[test]
fn 没写日志文件与没有认不出的键各有一句() {
    let r = render(
        "4.1.1",
        None,
        &[line("本机", "up", None, None, Some(0))],
        &[],
        None,
    );
    assert!(
        r.text.contains(&copy_text("rsDiagReport.text.noLog", &[])),
        "{}",
        r.text
    );
    assert!(r.text.contains("config.json · 0 项：—"), "{}", r.text);
}

#[test]
fn 记录账各面各条相加_形状不对不猜成零() {
    let v = serde_json::json!({"faces": [
        {"face": "unknown_record_type", "entries": [{"key": "a", "count": 2}, {"key": "b", "count": 5}]},
        {"face": "known_type_parse_failed", "entries": [{"key": "c", "count": 1}]}
    ]});
    assert_eq!(records_in(&v), Some(8));
    assert_eq!(records_in(&serde_json::json!({"faces": []})), Some(0));
    assert_eq!(
        records_in(&serde_json::json!({})),
        None,
        "缺 faces 说成 0 条 ＝ 把读不到说成都认得"
    );
    assert_eq!(
        records_in(&serde_json::json!({"faces": [{"entries": [{"count": "x"}]}]})),
        None
    );
}
