//! Codex 历史清单那一面的判据。
//!
//! 守的要求：主会话 09-25 裁（「主会话裁」第 2 条，逐字）「codex 合成的项目与会话一起进后端
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
