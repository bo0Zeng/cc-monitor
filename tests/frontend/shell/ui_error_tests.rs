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

/// 同 [`during`]，`fut` 跑完那一段（单线程运行时：`spawn` 出去的也在本线程上跑）。
pub(crate) async fn during_async(
    fut: impl std::future::Future<Output = ()>,
) -> Vec<serde_json::Value> {
    CAPTURE.with(|c| *c.borrow_mut() = Some(Vec::new()));
    fut.await;
    let got = CAPTURE.with(|c| c.borrow_mut().take()).unwrap_or_default();
    got.iter()
        .map(|p| serde_json::to_value(p).expect("payload 序列化"))
        .collect()
}

/// 一条推给界面的出错事件：那一句是从文案表取出来的（键在表里 · 占位符都填上了）· 除复制详情外没有一格是日志行的形状。
/// 那一句逐字等于哪一条，由各变体的测试照键与参数比（[`the_remote_kinds_are_copy_keyed_sentences_too`] 等）。
pub(crate) fn assert_is_copy_not_log(ev: &serde_json::Value) {
    let said = ev.get("said").and_then(|v| v.as_str()).unwrap_or_default();
    assert!(!said.is_empty(), "推给界面的出错事件没有那一句：{ev}");
    assert!(!said.starts_with('〔'), "那一句的文案键不在表里：{ev}");
    let unfilled = said.split('{').skip(1).any(|s| {
        s.find('}')
            .is_some_and(|j| j > 0 && s[..j].chars().all(|c| c.is_alphanumeric() || c == '_'))
    });
    assert!(!unfilled, "那一句里还有没填的占位符：{ev}");
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
            let line = ledger_line(d);
            let got = during(|| {
                tell(UiError::LocalBackendDied {
                    death: d.clone(),
                    then,
                    ledger_line: line.clone(),
                })
            });
            for ev in &got {
                assert_is_copy_not_log(ev);
                let key = match (d, then) {
                    (Death::Crashed { .. }, Then::Restarted) => "rsUiError.localExited.restarted",
                    (Death::Crashed { .. }, Then::Down) => "rsUiError.localExited.down",
                    (Death::Refused { .. }, Then::Restarted) => "rsUiError.localQuit.restarted",
                    (Death::Refused { .. }, Then::Down) => "rsUiError.localQuit.down",
                    (Death::Misread { .. }, Then::Down) => "rsUiError.localUnreadable.down",
                    _ => panic!("只记日志的那一格推出去了：{ev}"),
                };
                assert_eq!(
                    ev["said"],
                    copy_core::copy_text(key, &[]),
                    "{d:?}/{then:?} 那一句不是照它的文案键取出来的：{ev}"
                );
                let detail = ev["detail"].as_str().unwrap_or_default();
                assert!(detail.contains(&line), "账行没进复制详情：{ev}");
                assert_eq!(
                    ev.get("reconnect").is_some(),
                    then == Then::Down,
                    "断开才给［重新连接］：{ev}"
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
}

/// 远端那四种（解不开的帧 · 拥塞丢帧 · 历史快照没拉下 · 文件窗口开了又退）也走这一个出口：
/// 每种恰好一条、过 [`assert_is_copy_not_log`]，原话只在复制详情里，没有［重新连接］。
#[test]
fn the_remote_kinds_are_copy_keyed_sentences_too() {
    let cases: Vec<(String, UiError, &str)> = vec![
        (
            copy_core::copy_text(
                "rsSshSource.health.badFrame",
                &[("host", "devbox"), ("kind", "line")],
            ),
            UiError::FrameUnreadable {
                host: "devbox".into(),
                kind: "line".into(),
                why: "frame `line` malformed: missing field".into(),
            },
            "missing field",
        ),
        (
            copy_core::copy_text(
                "rsUiError.overflow.said",
                &[("host", "devbox"), ("said", "那台写的一句")],
            ),
            UiError::Overflow {
                host: "devbox".into(),
                said: "那台写的一句".into(),
            },
            "",
        ),
        (
            copy_core::copy_text("rsSshSource.snapshot.failed", &[("sid", "abcd1234")]),
            UiError::HistoryUnfetched {
                sid: "abcd1234".into(),
                raw: "连接断了 · 夹具".into(),
            },
            "连接断了 · 夹具",
        ),
        (
            copy_core::copy_text("rsFilewinProc.late.exited", &[]),
            UiError::FileWindowExited {
                code: Some("exit status: 7".into()),
                raw: None,
            },
            "exit status: 7",
        ),
        (
            copy_core::copy_text("rsFilewinProc.late.exited", &[]),
            UiError::FileWindowExited {
                code: Some("exit status: 1".into()),
                raw: Some("夹具 stderr 一行".into()),
            },
            "夹具 stderr 一行",
        ),
    ];
    for (want, e, raw) in cases {
        let got = during(|| tell(e.clone()));
        assert_eq!(got.len(), 1, "{e:?} 没推、或推了不止一条：{got:?}");
        let ev = &got[0];
        assert_is_copy_not_log(ev);
        assert_eq!(
            ev["said"], want,
            "{e:?} 那一句不是照它的文案键取出来的：{ev}"
        );
        assert!(
            ev.get("reconnect").is_none(),
            "远端提示不给［重新连接］：{ev}"
        );
        assert!(
            ev["detail"].as_str().unwrap_or_default().contains(raw),
            "原话没进复制详情：{ev}"
        );
    }
    let got = during(|| {
        tell(UiError::Overflow {
            host: "devbox".into(),
            said: "那台写的一句".into(),
        })
    });
    let said = got[0]["said"].as_str().unwrap_or_default();
    assert!(
        said.contains("devbox") && said.contains("那台写的一句"),
        "拥塞那一句没点名那台、或没原样带那台写的：{said}"
    );
}

/// 每一种要告诉用户的出错各一例（变体全列；`LocalBackendDied` 列推得出去的那几格）。
fn every_told_kind() -> Vec<UiError> {
    use crate::backend_policy::Outcome;
    let died = |death: Death, then: Then| UiError::LocalBackendDied {
        death,
        then,
        ledger_line: "账行".into(),
    };
    vec![
        died(
            Death::Crashed {
                how: Outcome::Exited(1),
            },
            Then::Restarted,
        ),
        died(
            Death::Crashed {
                how: Outcome::Exited(1),
            },
            Then::Down,
        ),
        died(Death::Refused { code: 1 }, Then::Restarted),
        died(Death::Refused { code: 1 }, Then::Down),
        died(Death::Misread { detail: "x".into() }, Then::Down),
        UiError::FrameUnreadable {
            host: "h".into(),
            kind: "line".into(),
            why: "w".into(),
        },
        UiError::Overflow {
            host: "h".into(),
            said: "那台写的一句".into(),
        },
        UiError::HistoryUnfetched {
            sid: "s".into(),
            raw: "r".into(),
        },
        UiError::FileWindowExited {
            code: None,
            raw: None,
        },
    ]
}

/// 线上形只带界面读的那几格：每一种推出去的事件，键集合 ＝ {said, detail} ∪ {reconnect}（断开那几格才有）。
#[test]
fn the_wire_form_carries_only_what_the_ui_reads() {
    for e in every_told_kind() {
        let got = during(|| tell(e.clone()));
        assert_eq!(got.len(), 1, "{e:?} 没推、或推了不止一条：{got:?}");
        let keys: Vec<&str> = got[0]
            .as_object()
            .map(|m| m.keys().map(String::as_str).collect())
            .unwrap_or_default();
        let want: Vec<&str> = if got[0].get("reconnect").is_some() {
            vec!["detail", "reconnect", "said"]
        } else {
            vec!["detail", "said"]
        };
        assert_eq!(
            keys, want,
            "{e:?} 推出去的格子不是界面读的那几格：{}",
            got[0]
        );
    }
}

/// 推给界面的出错只有一个出口：事件名只在定义它的两处与 `logging.rs` 装出口那一处出现，
/// 且壳里没有任何 tracing Layer（有了它，整行错误日志就会被推上屏）。
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
