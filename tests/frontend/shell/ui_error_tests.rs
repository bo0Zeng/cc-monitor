//! 推给界面的出错只这一种：带文案键、那句话照键从文案表取、日志行的形状只许待在复制详情里。

use super::*;
use std::cell::RefCell;

thread_local! {
    /// 本线程正在收的那一份（[`during`] 里才有）；别的线程照常走出口。
    static CAPTURE: RefCell<Option<Vec<UiErrorPayload>>> = const { RefCell::new(None) };
}

/// `tell` 在测试构建里先问这里：本线程在收 ⇒ 收下、不再往出口走。
pub(crate) fn captured(p: &UiErrorPayload) -> bool {
    CAPTURE.with(|c| match c.borrow_mut().as_mut() {
        Some(v) => {
            v.push(p.clone());
            true
        }
        None => false,
    })
}

/// `f` 跑的那一段里推给界面的出错事件（照线上 JSON 形）。
pub(crate) fn during(f: impl FnOnce()) -> Vec<serde_json::Value> {
    CAPTURE.with(|c| *c.borrow_mut() = Some(Vec::new()));
    f();
    let got = CAPTURE.with(|c| c.borrow_mut().take()).unwrap_or_default();
    got.iter()
        .map(|p| serde_json::to_value(p).expect("payload 序列化"))
        .collect()
}

/// 一条推给界面的出错事件：带文案键 · 那句话 = 照键与参数取出来的那一句 · 除复制详情外没有一格是日志行的形状。
pub(crate) fn assert_is_copy_not_log(ev: &serde_json::Value) {
    let key = ev.get("key").and_then(|v| v.as_str()).unwrap_or_default();
    assert!(!key.is_empty(), "推给界面的出错事件没有文案键：{ev}");
    let args: Vec<(String, String)> = ev
        .get("args")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default();
    let args_ref: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let table: serde_json::Value =
        serde_json::from_str(copy_core::TABLE_JSON).expect("文案表是 JSON");
    let entry = &table["entries"][key];
    assert!(entry.is_object(), "文案键 {key} 不在表里：{ev}");
    let want: std::collections::BTreeSet<&str> = entry["args"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    let have: std::collections::BTreeSet<&str> = args.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(have, want, "文案键 {key} 的参数与表里那一条不等：{ev}");
    let from_table = copy_core::copy_text(key, &args_ref);
    assert_eq!(
        ev.get("said").and_then(|v| v.as_str()),
        Some(from_table.as_str()),
        "界面那句不是照文案键取出来的：{ev}"
    );
    let Some(obj) = ev.as_object() else {
        panic!("出错事件不是对象：{ev}")
    };
    for (field, v) in obj {
        if field == "detail" {
            continue;
        }
        let mut texts = Vec::new();
        collect_strings(v, &mut texts);
        for t in texts {
            if let Some(shape) = log_line_shape(&t) {
                panic!("推给界面的「{field}」一格里是日志行的形状（{shape}）：{t}");
            }
        }
    }
}

fn collect_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        serde_json::Value::Object(m) => m.values().for_each(|x| collect_strings(x, out)),
        _ => {}
    }
}

/// 日志行的三种形状：模块路径（`a::b`）· `key=value` · 打头的方括号账名（`[死亡账]`）。没有 ⇒ `None`。
pub(crate) fn log_line_shape(t: &str) -> Option<&'static str> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    for (i, _) in t.match_indices("::") {
        let before = t[..i].chars().next_back().is_some_and(word);
        let after = t[i + 2..].chars().next().is_some_and(word);
        if before && after {
            return Some("模块路径");
        }
    }
    for (i, _) in t.match_indices('=') {
        let before = t[..i].chars().next_back().is_some_and(word);
        let after = t[i + 1..]
            .chars()
            .next()
            .is_some_and(|c| c != ' ' && c != '=');
        if before && after {
            return Some("key=value");
        }
    }
    let lead = t.trim_start();
    if lead.starts_with('[') && lead.find(']').is_some_and(|j| j > 1) {
        return Some("方括号账名");
    }
    None
}

#[test]
fn the_log_line_shape_detector_sees_all_three_and_passes_a_sentence() {
    assert_eq!(
        log_line_shape("monitor_lib::backend_policy"),
        Some("模块路径")
    );
    assert_eq!(log_line_shape("判定=崩溃"), Some("key=value"));
    assert_eq!(log_line_shape("[死亡账] 本机"), Some("方括号账名"));
    assert_eq!(
        log_line_shape("本机上的 cc-monitor 意外退出 · 已重新启动"),
        None
    );
    assert_eq!(log_line_shape("1 + 1 = 2"), None);
}

/// 每一种要告诉用户的出错（变体 × 之后怎么样了，全列）：推出去的都过 [`assert_is_copy_not_log`]，
/// 只记日志的那几格一条都不推。账行（日志行格式）只在复制详情里。
#[test]
fn every_kind_of_ui_error_is_a_copy_keyed_sentence_and_the_log_line_stays_in_the_detail() {
    use crate::backend_policy::{ledger_line, Outcome};
    let deaths = [
        Death::Crashed {
            how: Outcome::Exited(1),
        },
        Death::Refused { code: 1 },
        Death::Misread { detail: "x".into() },
        Death::NeverStarted {
            reason: "x".into(),
            looked_at: Vec::new(),
        },
    ];
    let mut pushed = Vec::new();
    for d in &deaths {
        for then in [Then::Restarted, Then::Down] {
            let line = ledger_line("<local>", d);
            let got = during(|| {
                tell(UiError::LocalBackendDied {
                    death: d.clone(),
                    then,
                    ledger_line: line.clone(),
                })
            });
            for ev in &got {
                assert_is_copy_not_log(ev);
                let detail = ev["detail"].as_str().unwrap_or_default();
                assert!(detail.contains(&line), "账行没进复制详情：{ev}");
                assert_eq!(
                    ev.get("reconnect").is_some(),
                    then == Then::Down,
                    "连不上才给［重新连接］：{ev}"
                );
            }
            pushed.push((format!("{d:?}/{then:?}"), got.len()));
        }
    }
    let quiet: Vec<&str> = pushed
        .iter()
        .filter(|(_, n)| *n == 0)
        .map(|(s, _)| s.as_str())
        .collect();
    assert_eq!(
        quiet.len(),
        3,
        "只记日志的应当恰好是 读坏/重起 · 没起来 两种 × 两格里的三格：{quiet:?}"
    );
    let got = during(|| {
        tell(UiError::MachinesUnreadable {
            path: "/x/config.json".into(),
            why: "y".into(),
        })
    });
    assert_eq!(got.len(), 1);
    assert_is_copy_not_log(&got[0]);
}

/// 推给界面的出错只有一个出口：事件名只在定义它的两处与 `logging.rs` 装出口那一处出现，
/// 且壳里没有任何 tracing Layer（那是从前把整行错误日志推上屏的那条路）。
#[test]
fn the_ui_error_event_has_one_exit_and_no_log_layer_feeds_it() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut uses: Vec<String> = Vec::new();
    let mut layers: Vec<String> = Vec::new();
    for (path, raw) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let code = guard_core::production_code(&raw);
        for line in code.lines() {
            if ["\"monitor-error\"", "UI_ERROR", "ui_error::EVENT"]
                .iter()
                .any(|n| guard_core::contains_word(line, n))
            {
                uses.push(format!("{rel}: {}", line.trim()));
            }
            if ["Layer<S> for", "tracing_subscriber::Layer"]
                .iter()
                .any(|n| guard_core::contains_word(line, n))
            {
                layers.push(format!("{rel}: {}", line.trim()));
            }
        }
    }
    let files: std::collections::BTreeSet<&str> =
        uses.iter().map(|u| u.split(':').next().unwrap()).collect();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        vec!["logging.rs", "ui_contract.rs", "ui_error.rs"],
        "出错事件名出现在别处：{uses:?}"
    );
    assert_eq!(uses.len(), 3, "出错事件名多了一处用法：{uses:?}");
    assert!(layers.is_empty(), "壳里又有了 tracing Layer：{layers:?}");
}
