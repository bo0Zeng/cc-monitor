//! `diagnostics-report` 的判据：拼法 · 记录账的数法 · 认不出的 config 键照共享键表算 · 每台一行的并法。

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
            line("this-box", "up", None, Some("p8n-mcp-sync"), Some(0)),
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
        &*copy_text(
            "diagnostics.unknown.machine",
            &[("machine", "gpu-01 ·"), ("n", "3")],
        ),
        &*copy_text(
            "diagnostics.unknown.machineUnread",
            &[("machine", "devbox ·")],
        ),
        &*copy_text(
            "rsDiagReport.text.config",
            &[
                ("n", "2"),
                (
                    "keys",
                    &format!(
                        "resumeCommandLocal{}x",
                        copy_core::copy_static!("plan.page.listSep")
                    ),
                ),
            ],
        ),
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
                machine: "this-box".into(),
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
        &[line("this-box", "up", None, None, Some(0))],
        &[],
        None,
    );
    assert!(
        r.text.contains(&copy_text("rsDiagReport.text.noLog", &[])),
        "{}",
        r.text
    );
    assert!(
        r.text.contains(&copy_text(
            "rsDiagReport.text.config",
            &[
                ("n", "0"),
                ("keys", copy_core::copy_static!("acct.val.none"))
            ],
        )),
        "{}",
        r.text
    );
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

#[test]
fn 认不出的顶层键照共享键表算_照盘上的先后() {
    let keys: Vec<String> = ["zeta", "theme", "diagnostics", "alpha"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        unknown_config_keys(&keys),
        vec!["zeta".to_string(), "alpha".to_string()],
        "登记过的键（界面那张表同一份）不该算认不出"
    );
    let table: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(CONFIG_KEYS_JSON).expect("键表是一份 JSON 对象");
    assert!(!table.is_empty(), "键表读成空 ⇒ 每个键都会被说成认不出");
    let all: Vec<String> = table.keys().cloned().collect();
    assert!(unknown_config_keys(&all).is_empty());
}

#[test]
fn 每台一行_本机在前带本机的构建_别台的构建照那条流的hello_读不到的那台照实空着() {
    let local = cmd_wire::machine::LOCAL.to_string();
    let mut states = BTreeMap::new();
    states.insert(
        "devbox".to_string(),
        StateWords {
            os: Some("Linux".into()),
            state: "down".into(),
            reason: Some("auth".into()),
            version: None,
        },
    );
    let builds: BTreeMap<String, String> = [("gpu-01".to_string(), "p8m".to_string())].into();
    let report =
        serde_json::json!({"faces": [{"face": "f", "entries": [{"key": "k", "count": 3}]}]});
    let lines = lines_of(
        &states,
        &builds,
        &[
            (local.clone(), Some(serde_json::json!({"faces": []}))),
            ("devbox".to_string(), None),
            ("gpu-01".to_string(), Some(report)),
        ],
    );
    let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        names,
        vec![&*copy_text("detail.label.local", &[]), "devbox", "gpu-01"]
    );
    assert_eq!(lines[0].build.as_deref(), Some(crate::common::id::BUILD_ID));
    assert_eq!(lines[0].records, Some(0));
    assert_eq!(lines[1].state, "down");
    assert_eq!(lines[1].reason.as_deref(), Some("auth"));
    assert_eq!(
        lines[1].build, None,
        "断着的那台没有 hello ⇒ 构建空着，不猜"
    );
    assert_eq!(lines[1].records, None, "问不到记录账 ⇒ 读不到，不说成 0");
    assert_eq!(lines[2].build.as_deref(), Some("p8m"));
    assert_eq!(lines[2].records, Some(3));
}
