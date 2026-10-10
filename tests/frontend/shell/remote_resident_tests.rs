//! monitor 这一侧的远端常驻后端（`remote_resident.rs`）。
//! 守的要求：「远端常驻、本机远端同形」；只升不降「部署只在『我的比盘上的新』时才换」。

use super::*;

fn hello(build: &str) -> String {
    format!(
        r#"{{"kind":"hello","v":1,"build_id":"{build}","host_arch":"x86_64","claude_dir":"/h/.claude"}}"#
    )
}

/// hello 只读线上形状：是 hello ⇒ 那台报的 build（缺 ⇒ 空串，交本机后端判）；不是 hello ⇒ 不是我们的。
/// 「换一次 · 只升不降」那张真值表随判定进了本机常驻后端（`tests/backend/control/deploy_plan_tests.rs::the_verdict_replaces_only_upward_and_only_once`）。
#[test]
fn the_hello_is_read_for_its_build_and_nothing_else() {
    assert_eq!(hello_build(&hello("p4a-x")), Ok("p4a-x".to_string()));
    assert_eq!(hello_build(r#"{"kind":"hello","v":1}"#), Ok(String::new()));
    assert!(hello_build(r#"{"attach":"refused"}"#).is_err());
}

/// 本机常驻后端的答严格收：恰 `{action, older}`，`action` 只认 `replace` / `attach`（金样同 `IPC-PROTOCOL.md` 那一节）。
#[test]
fn the_verdict_answer_is_read_strictly() {
    use serde_json::json;
    assert_eq!(
        decode_verdict(&json!({"action": "replace", "older": true})),
        Ok(Verdict {
            replace: true,
            older: true
        })
    );
    assert_eq!(
        decode_verdict(&json!({"action": "attach", "older": false})),
        Ok(Verdict {
            replace: false,
            older: false
        })
    );
    for bad in [
        json!({"action": "keep", "older": false}),
        json!({"action": "attach"}),
        json!({"action": "attach", "older": "no"}),
        json!({"action": "attach", "older": false, "extra": 1}),
        json!(null),
    ] {
        assert!(decode_verdict(&bad).is_err(), "{bad}");
    }
}

fn exec(stdout: &str, stderr: &str, code: Option<u32>) -> crate::stream_source::RemoteExec {
    crate::stream_source::RemoteExec {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_status: code,
    }
}

/// D2 `--resident-ensure` 的答：成了 ⇒ 端口；那台脱离不了（`unsupported`，非 unix）⇒ **明说**「远端只支持 Unix」、
/// 带上那台原话，并归成 `Unsupported`（`run` 据此不再自动重连）；别的失败 ⇒ 那台原话原样；
/// 老后端掉进流模式发 hello ⇒ 「太旧」（`Failed`：部署会把它换掉，照常重连）。四样都是失败，没有「回落」那一格。
/// 要求：「非 unix 远端（Windows 远端，不承诺）连不上常驻时**明说**不支持，不静默」。
#[test]
fn the_ensure_answer_names_a_non_unix_remote_as_unsupported_and_never_falls_back() {
    let ok = parse_answer(&exec(r#"{"pid":7}"#, "", Some(0)), "devbox").unwrap();
    assert_eq!(parse_ensured(&ok), Ok(()));
    // 那台的失败信封只有一种（`{code, message, detail, data?}`）：`detail` 是那台写好的一整份复制详情，原样转交（补「本机」一行）。
    use copy_core::detail::{Detail, Label};
    let wrote = |code: &str, raw: &str| {
        Detail::new()
            .item(Label::Command, "resident-ensure")
            .item(Label::Code, code)
            .item(Label::Raw, raw)
            .render()
    };
    let envelope = |code: &str, message: &str, detail: &str| {
        serde_json::json!({"code": code, "message": message, "detail": detail}).to_string()
    };
    // 脱离不了（非 unix）⇒ 句子由这边说「远端只支持 Unix」；那台的那一句（它就是系统原话）与那台的详情都进复制详情。
    match parse_answer(
        &exec("", &envelope("unsupported", "不是 unix", &wrote("unsupported", "")), Some(2)),
        "devbox",
    ) {
        Err(AttachErr::Unsupported(s, code)) => {
            assert_eq!(code, crate::machine_state::NOT_UNIX);
            assert_eq!(s.said, copy_text("rsRemoteResident.ensure.unsupported", &[]));
            assert!(s.detail.contains("不是 unix"), "{}", s.detail);
            assert!(s.detail.contains("resident-ensure"), "那台写的那份没带上：{}", s.detail);
        }
        other => panic!("{other:?}"),
    }
    // 别的失败 ⇒ 句子照那台那一句；那台的详情（含下层原话）整份进复制详情，句子里没有原话。
    match parse_answer(
        &exec(
            "",
            &envelope("spawn_failed", "x", &wrote("spawn_failed", "Permission denied (os error 13)")),
            Some(2),
        ),
        "devbox",
    ) {
        Err(AttachErr::Failed(s)) => {
            assert_eq!(s.said, "x");
            assert!(s.detail.contains("Permission denied (os error 13)"), "{}", s.detail);
            assert!(
                s.detail.contains(&copy_text("detail.label.local", &[])),
                "远端那份没补「本机」一行：{}",
                s.detail
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        parse_answer(
            &exec("", r#"{"code":"spawn_failed","message":"x"}"#, Some(2)),
            "devbox"
        ),
        Err(AttachErr::Failed("x".into()))
    );
    assert_eq!(
        parse_answer(&exec(&hello("p1a-old"), "", None), "devbox"),
        Err(AttachErr::Failed(copy_core::backend_old("devbox").into()))
    );
    // 已有人在听 ⇒ pid 可能是 null；不是对象才算认不得。
    assert_eq!(parse_ensured(&serde_json::json!({"pid":null})), Ok(()));
    assert!(parse_ensured(&serde_json::json!("7")).is_err());
}

/// attach 行是远端 `listen::attach_verdict` / `attach_flags` 读得懂的形状：「我要流」＋ 这条连接的旗标，没有钥匙。
#[test]
fn the_attach_line_asks_for_the_stream_with_exactly_the_negotiated_flags() {
    let l = attach_line((false, true));
    assert!(l.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(l.trim()).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"attach": true, "flags": ["--tail-only"]})
    );
}

/// T4 **停的结局只认三个词**（本机远端同一个读法）：`graceful` · `killed` · `not_running` 各落一格、pid 原样；
/// 认不出的词 / 老后端那一形（`{"stopped":<pid>}`）⇒ 错，不猜成「停了」；退出 2 ⇒ stderr 那句原样。
/// 要求：「回结局 `{stopped: "graceful" | "killed" | "not_running"}` …… monitor 发一次远端 exec、按结局出声」。
#[test]
fn the_stop_answer_is_one_of_three_words_or_an_error() {
    for (line, want) in [
        (
            r#"{"stopped":"graceful","pid":42}"#,
            StopAnswer {
                stopped: StopWord::Graceful,
                pid: Some(42),
            },
        ),
        (
            r#"{"stopped":"killed","pid":7}"#,
            StopAnswer {
                stopped: StopWord::Killed,
                pid: Some(7),
            },
        ),
        (
            r#"{"stopped":"not_running","pid":null}"#,
            StopAnswer {
                stopped: StopWord::NotRunning,
                pid: None,
            },
        ),
    ] {
        assert_eq!(
            read_stop(&exec(line, "", Some(0)), "devbox"),
            Ok(want),
            "{line}"
        );
    }
    for line in [r#"{"stopped":42}"#, r#"{"stopped":"stopped"}"#, r#"{}"#] {
        assert!(
            read_stop(&exec(line, "", Some(0)), "devbox").is_err(),
            "{line}"
        );
    }
    let err = read_stop(
        &exec(
            "",
            r#"{"code":"stop_failed","message":"强杀之后还在"}"#,
            Some(2),
        ),
        "devbox",
    )
    .unwrap_err();
    assert_eq!(err.said, "强杀之后还在");
    // 交给机器页的线上形：三个词原样（snake_case），不是 Rust 的变体名。
    let wire: Vec<String> = [StopWord::Graceful, StopWord::Killed, StopWord::NotRunning]
        .iter()
        .map(|w| {
            serde_json::to_value(w)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(wire, ["graceful", "killed", "not_running"]);
}

/// **monitor 等那一趟的期限 > 那台停它的最长时间**（宽限期 ＋ 强杀后再等的那一段）：否则 monitor 先放弃、
/// 那台其实停成了，界面却说「超时」。跨半边：现抠后端 `control/resident.rs` 两个毫秒字面量（各恰好一处），与 `dial_host::ONE_SHOT_DEADLINE` 比。
/// 要求：「远端控制方只发一次『停』并拿回结局，不远程轮询」。
#[test]
fn the_one_shot_deadline_outlasts_the_remote_stop() {
    let be = guard_core::production_code(include_str!("../../../src/backend/control/resident.rs"));
    let lit = |name: &str| -> u128 {
        let anchor = format!("pub(crate) const {name}: u32 = ");
        assert_eq!(
            be.matches(&anchor).count(),
            1,
            "`{name}` 的声明不是恰好一处"
        );
        let tail = &be[be.find(&anchor).expect("声明") + anchor.len()..];
        tail.chars()
            .take_while(|c| c.is_ascii_digit() || *c == '_')
            .filter(|c| *c != '_')
            .collect::<String>()
            .parse()
            .expect("毫秒字面量")
    };
    let worst = lit("STOP_GRACE_MS") + lit("KILL_WAIT_MS");
    let ours = crate::dial_host::ONE_SHOT_DEADLINE.as_millis();
    assert!(
        worst < ours,
        "那台停一次最长 {worst}ms，monitor 只等 {ours}ms —— 会在结局出来之前放弃"
    );
}

/// 小中继的第一行：拒绝那一形读出理由，别的（hello）照 hello 读。理由词与后端那一侧逐字相同（跨 crate 字面量）。
#[test]
fn the_relay_refusal_line_is_read_and_its_words_match_the_backend() {
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"absent"}"#).as_deref(),
        Some(RELAY_ABSENT)
    );
    assert_eq!(
        relay_refusal(r#"{"attach":"refused","reason":"unreachable"}"#).as_deref(),
        Some("unreachable")
    );
    assert_eq!(relay_refusal(&hello("p1")), None);
    let be = include_str!("../../../src/backend/stream/listen.rs");
    assert!(
        be.contains(&format!(
            "pub const REFUSE_ABSENT: &str = \"{RELAY_ABSENT}\";"
        )),
        "后端那一侧「没人在听」的理由词与这里不同了"
    );
}

/// 中继的编排（替身接中继）：① `absent`（刚起的还没绑上）两次、第三次接上 ⇒ 等着再接、恰好 3 次、接成；
/// ② 别的理由 ⇒ 接 1 次就停、那一句说「连不上 · 需重启」；③ 一直 `absent` ⇒ 封顶次数后说「后端没在运行」。
#[tokio::test]
async fn the_relay_waits_only_while_nobody_listens() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let tries = AtomicUsize::new(0);
    let got = retry_relay(
        || {
            let n = tries.fetch_add(1, Ordering::SeqCst);
            async move {
                Ok::<_, AttachErr>(if n < 2 {
                    Err(RELAY_ABSENT.to_string())
                } else {
                    Ok(n)
                })
            }
        },
        "devbox",
        4,
        std::time::Duration::from_millis(1),
    )
    .await;
    assert_eq!((got, tries.load(Ordering::SeqCst)), (Ok(2), 3));

    let tries = AtomicUsize::new(0);
    let got = retry_relay(
        || {
            tries.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Result<(), String>, AttachErr>(Err("unreachable".to_string())) }
        },
        "devbox",
        4,
        std::time::Duration::from_millis(1),
    )
    .await;
    assert_eq!(tries.load(Ordering::SeqCst), 1, "再接也一样的理由还在重接");
    assert_eq!(
        got.map_err(AttachErr::said).map_err(|s| s.said),
        Err(copy_text(
            "rsRemoteResident.relay.unreachable",
            &[("machine", "devbox")]
        ))
    );

    let tries = AtomicUsize::new(0);
    let got = retry_relay(
        || {
            tries.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Result<(), String>, AttachErr>(Err(RELAY_ABSENT.to_string())) }
        },
        "devbox",
        4,
        std::time::Duration::from_millis(1),
    )
    .await;
    assert_eq!(tries.load(Ordering::SeqCst), 4);
    assert_eq!(
        got.map_err(AttachErr::said).map_err(|s| s.said),
        Err(copy_text(
            "rsRemoteResident.relay.absent",
            &[("machine", "devbox")]
        ))
    );
}

/// ④ 手上没带后端字节（「我这一版」是 `None`）⇒ `resident-verdict` **不问**、照接、不判旧（换装无从发起）；
/// 有值 ⇒ 照问、照它答的做。问的那一方是个假后端：记下被问了几次、问的是哪一版。
#[tokio::test]
async fn without_own_bytes_the_resident_verdict_is_not_asked() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    let asked = AtomicUsize::new(0);
    let with = Mutex::new(String::new());
    let fake = |mine: &'static str| {
        asked.fetch_add(1, Ordering::SeqCst);
        *with.lock().unwrap() = mine.to_string();
        async {
            Ok(Verdict {
                replace: true,
                older: true,
            })
        }
    };
    let none = verdict_for(None, fake).await;
    assert_eq!(
        none,
        Ok(Verdict {
            replace: false,
            older: false
        }),
        "没带字节却判了换 / 判了旧"
    );
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "没带字节却去问了 resident-verdict"
    );

    let fake = |mine: &'static str| {
        asked.fetch_add(1, Ordering::SeqCst);
        *with.lock().unwrap() = mine.to_string();
        async {
            Ok(Verdict {
                replace: true,
                older: true,
            })
        }
    };
    let some = verdict_for(Some("p9z-mine"), fake).await;
    assert_eq!(
        (
            some,
            asked.load(Ordering::SeqCst),
            with.lock().unwrap().clone()
        ),
        (
            Ok(Verdict {
                replace: true,
                older: true
            }),
            1,
            "p9z-mine".to_string()
        ),
        "有「我这一版」却没照问 / 问的不是这一版"
    );
}
