//! agent 适配层判据里经历史清单的那几条：适配层是独立的包（`backend-agents`）、够不到历史这一层 ⇒ 挂在 `history` 底下，
//! 这一层拆成包时跟着走。夹具照本机实测的形状合成，正文是编的。

const SID_A: &str = "0199aaaa-0000-7000-8000-000000000001";

/// 夹具注册表里 Codex 那一家的记录根（裸 fn，不读环境变量；同 `fake_tests` 的做法）。
fn label_root() -> Option<std::path::PathBuf> {
    Some(
        std::env::temp_dir()
            .join(format!("ccm-codexhist-label-{}", std::process::id()))
            .join("sessions"),
    )
}

/// 核心取名那一处（标签页 · 通知 · 账号页标题都走它）认 Codex 自己的记录格式：一份说过话的 Codex 会话，
/// 名字是它说的第一句人话（与历史清单那一行同一个摘录），不是「没有说过话的会话」。
/// 夹具：`session_meta` · developer 注入 · 带种类注解的环境块 · 带种类注解的一句人话。
#[test]
fn the_core_name_of_a_codex_session_is_its_first_human_line() {
    let d = std::env::temp_dir().join(format!("ccm-codexhist-label-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let day = d.join("sessions").join("2026").join("10").join("12");
    std::fs::create_dir_all(&day).unwrap();
    let p = day.join(format!("rollout-2026-10-12T00-00-00-{SID_A}.jsonl"));
    let kinds = |k: &str| {
        format!(r#""internal_chat_message_metadata_passthrough":{{"content_item_kinds":["{k}"]}}"#)
    };
    std::fs::write(
        &p,
        [
            r#"{"timestamp":"t","type":"session_meta","payload":{"id":"x","cwd":"/w/proj"}}"#
                .to_string(),
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
            if a.kind == crate::agents::codex::AGENT_KIND {
                crate::agents::Adapter {
                    history: Some(crate::agents::HistoryFace {
                        root: label_root,
                        ..crate::agents::codex::HISTORY
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
