//! Codex 历史清单那一面的判据。
//!
//! 守的要求：「codex 合成的项目与会话一起进后端
//! （join 只一个家）」—— 口径逐格照搬 monitor 那两份（枚举 · 首条真用户话），搬家那一刻用户看到的 Codex 会话不许变样。
//!
//! 判据：① 枚举只认 `rollout-<ts>-<uuid>.jsonl`、sid 取末尾 UUID、cwd 取首行 `session_meta`（缺 ⇒ 空串）；
//! ② 首条真用户话跳过注入上下文、拍平数组、取 200 字符；③ 注入上下文那张表与 monitor 渲染那一侧**逐项相等**
//! （运行期读 monitor 源码现抠，异源）。

use super::*;

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-codexhist-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const SID_A: &str = "0199aaaa-0000-7000-8000-000000000001";
const SID_B: &str = "0199aaaa-0000-7000-8000-000000000002";

fn meta(cwd: &str) -> String {
    format!(r#"{{"timestamp":"t","type":"session_meta","payload":{{"id":"x","cwd":"{cwd}"}}}}"#)
}

fn user(text_json: &str) -> String {
    format!(
        r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"message","role":"user","content":{text_json}}}}}"#
    )
}

#[test]
fn only_rollout_files_are_sessions_and_cwd_comes_from_the_first_line() {
    let home = tmp("enum");
    let day = home.join("sessions/2026/09/25");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-03-{SID_A}.jsonl")),
        format!(
            "{}\n{}\n",
            meta("/w/a"),
            user(r#"[{"type":"input_text","text":"hi"}]"#)
        ),
    )
    .unwrap();
    // 首行不是 session_meta ⇒ cwd 空串（归「(codex)」组）。
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-04-{SID_B}.jsonl")),
        format!("{}\n", user(r#""x""#)),
    )
    .unwrap();
    // 不是 rollout 命名 / 末尾不是 UUID / 不是 .jsonl ⇒ 不算会话。
    std::fs::write(day.join("notes.jsonl"), "{}\n").unwrap();
    std::fs::write(day.join("rollout-2026-09-25T01-02-05-nope.jsonl"), "{}\n").unwrap();
    std::fs::write(
        day.join(format!("rollout-2026-09-25T01-02-06-{SID_A}.txt")),
        "{}\n",
    )
    .unwrap();

    let mut got: Vec<(String, String)> = sessions_under(&home)
        .into_iter()
        .map(|s| (s.sid, s.cwd))
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            (SID_A.to_string(), "/w/a".to_string()),
            (SID_B.to_string(), String::new()),
        ]
    );
    // 没装 Codex（没有会话目录）⇒ 空，不报错。
    assert!(sessions_under(&home.join("nope")).is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_first_real_user_line_skips_injected_context_and_is_capped() {
    let d = tmp("excerpt");
    let p = d.join("r.jsonl");
    let long = "问".repeat(250);
    std::fs::write(
        &p,
        [
            meta("/w"),
            user(r#"[{"type":"input_text","text":"<environment_context>\n<cwd>/w</cwd>"}]"#),
            user(r#"[{"type":"input_text","text":"  # AGENTS.md instructions for /w"}]"#),
            format!(r#"{{"type":"response_item","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"不是用户"}}]}}}}"#),
            user(&format!(r#"[{{"type":"input_text","text":"{long}"}}]"#)),
        ]
        .join("\n"),
    )
    .unwrap();
    assert_eq!(first_user_excerpt(&p), "问".repeat(200));
    // 数组拍平：多段 text 用换行连起来。
    let q = d.join("q.jsonl");
    std::fs::write(
        &q,
        user(r#"[{"type":"input_text","text":"甲"},{"type":"input_image"},{"type":"input_text","text":"乙"}]"#),
    )
    .unwrap();
    assert_eq!(first_user_excerpt(&q), "甲\n乙");
    assert_eq!(first_user_excerpt(&d.join("missing.jsonl")), "");
    let _ = std::fs::remove_dir_all(&d);
}

/// 夹具注册表里 Codex 那一家的记录根（裸 fn，不读环境变量；同 `fake_tests` 的做法）。
fn label_root() -> Option<std::path::PathBuf> {
    Some(
        std::env::temp_dir()
            .join(format!("ccm-codexhist-label-{}", std::process::id()))
            .join("sessions"),
    )
}

/// 核心取名那一处（标签页 · 通知 · 账号页标题都走它）认这一家自己的记录格式：一份说过话的 Codex 会话，
/// 名字是它说的第一句人话（与历史清单那一行同一个摘录），不是「没有说过话的会话」。
/// 夹具照本机实测的形状合成：`session_meta` · developer 注入 · 带种类注解的环境块 · 带种类注解的一句人话。
#[test]
fn the_core_name_of_a_codex_session_is_its_first_human_line() {
    let d = tmp("label");
    let day = d.join("sessions").join("2026").join("10").join("12");
    std::fs::create_dir_all(&day).unwrap();
    let p = day.join(format!("rollout-2026-10-12T00-00-00-{SID_A}.jsonl"));
    let kinds = |k: &str| {
        format!(r#""internal_chat_message_metadata_passthrough":{{"content_item_kinds":["{k}"]}}"#)
    };
    std::fs::write(
        &p,
        [
            meta("/w/proj"),
            r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_started"}}"#.to_string(),
            format!(
                r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"message","role":"developer","content":[{{"type":"input_text","text":"<collaboration_mode>x"}}],{}}}}}"#,
                kinds("collaboration_mode.instructions")
            ),
            format!(
                r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"<environment_context>\n<cwd>/w/proj</cwd>"}}],{}}}}}"#,
                kinds("environments.context")
            ),
            format!(
                r#"{{"timestamp":"t","type":"response_item","payload":{{"type":"message","role":"user","content":[{{"type":"input_text","text":"修一下夹具里的那条测试"}}],{}}}}}"#,
                kinds("user.text")
            ),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();
    assert_eq!(label_root().unwrap(), d.join("sessions"));
    let reg: Vec<crate::agents::Adapter> = crate::agents::REGISTRY
        .iter()
        .map(|a| {
            if a.kind == super::super::AGENT_KIND {
                crate::agents::Adapter {
                    history: Some(crate::agents::HistoryFace {
                        root: label_root,
                        ..super::super::HISTORY
                    }),
                    upstream: None,
                    ..*a
                }
            } else {
                crate::agents::Adapter {
                    upstream: None,
                    ..*a
                }
            }
        })
        .collect();
    let facts = crate::history::listing::label_facts_among(&reg, &p);
    assert_eq!(facts.first, "修一下夹具里的那条测试");
    assert_eq!(facts.project_dir.as_deref(), Some("/w/proj"));
    let (name, named) = crate::history::search_rules::session_label(&facts);
    assert!(named, "说过话的会话起了名字：{name}");
    assert_eq!(name, "修一下夹具里的那条测试");
    let _ = std::fs::remove_dir_all(&d);
}

/// 按 sid 找那份 rollout（注册表 `RecordFace.find_session` 那一格）：日期树里那一份 · 形状不合法先于 IO 就拒 ·
/// 文件名末尾不是这个 sid 的不算 · 符号链接不算命中 · 找不到照实拒。
#[test]
fn a_session_is_found_by_sid_under_the_dated_tree() {
    let home = tmp("find");
    let root = parse::sessions_root(&home);
    let day = root.join("2026/10/02");
    std::fs::create_dir_all(&day).unwrap();
    let want = day.join(format!("rollout-2026-10-02T10-00-00-{SID_A}.jsonl"));
    std::fs::write(&want, format!("{}\n", meta("/w"))).unwrap();
    // 名字里带着这个 sid、却不是末尾那一段 ⇒ 不是它。
    std::fs::write(
        day.join(format!("rollout-{SID_A}-{SID_B}.jsonl")),
        format!("{}\n", meta("/w")),
    )
    .unwrap();
    assert_eq!(find_session(&root, SID_A), Ok(want.clone()));
    assert!(find_session(&root, "0199aaaa-0000-7000-8000-00000000000f").is_err());
    assert!(find_session(&root, "../x").is_err());
    assert!(find_session(&home.join("nowhere"), SID_A).is_err());
    // 指过去的链接不算：同一个 sid 只剩链接那一份 ⇒ 找不到。
    #[cfg(unix)]
    {
        std::fs::remove_file(&want).unwrap();
        let elsewhere = home.join(format!("rollout-2026-10-02T10-00-00-{SID_A}.jsonl"));
        std::fs::write(&elsewhere, format!("{}\n", meta("/w"))).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &want).unwrap();
        assert!(find_session(&root, SID_A).is_err());
    }
    let _ = std::fs::remove_dir_all(&home);
}
