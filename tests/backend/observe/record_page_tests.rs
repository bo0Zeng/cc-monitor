use super::*;

fn claude() -> RecordFace {
    crate::agents::claudecode::RECORDS
}

const USER: &str = r#"{"type":"user","uuid":"u1","timestamp":"t","cwd":"/w","message":{"role":"user","content":"q"}}"#;
const MODE: &str = r#"{"type":"mode","mode":"normal"}"#;

/// 行摘要：每个可计行一条；末端是含 `\n` 之后那个字节（CRLF 的 `\r` 计在内）· 残尾 `null` · 空白行不占 ·
/// 不进界面的只有 `{end, hash}`。
#[test]
fn rows_carry_exact_ends_and_only_displayable_messages() {
    let page = format!("{USER}\r\n\n  \n{MODE}\n{{torn");
    let rows = rows_of(&claude(), 100, page.as_bytes());
    assert_eq!(rows.len(), 3, "{rows:?}");
    let u = USER.len() as u64 + 2;
    assert_eq!(rows[0]["end"], 100 + u);
    assert_eq!(rows[0]["cwd"], "/w");
    assert_eq!(rows[0]["message"]["uuid"], "u1");
    assert_eq!(rows[1]["end"], 100 + u + 1 + 3 + MODE.len() as u64 + 1);
    assert!(
        rows[1].get("message").is_none(),
        "没有读者的元数据记录不带成品"
    );
    assert!(rows[2]["end"].is_null(), "残尾没有末端");
    // 摘要只看正文（去掉 `\r`）：同一行 LF / CRLF 两种收尾摘要相同。
    assert_eq!(rows[0]["hash"], line_hash(USER.as_bytes()));
}

/// 记录行：第 k 个可计行是 `seq + k`，只出进界面的那些；`nextSeq` 数的是可计行。
#[test]
fn record_lines_number_countable_lines_and_keep_only_displayable() {
    let page = format!("{MODE}\n\n{USER}\n");
    let (lines, next) = record_lines_of_page(
        &claude(),
        std::path::Path::new("/p/abc.jsonl"),
        7,
        page.as_bytes(),
    );
    assert_eq!(next, 9);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["seq"], 8);
    assert_eq!(lines[0]["session_id"], "abc");
    assert_eq!(lines[0]["path"], "/p/abc.jsonl");
    assert_eq!(lines[0]["message"]["type"], "user");
}

/// 〔原 monitor `parser_tests::parse_for_kind_dispatches_claude_and_codex`〕按文件落在谁的根下认是哪一家：〔散文墓碑〕
/// 落在 Codex 记录根下 ⇒ Codex 那一家解释；否则 ⇒ 记录树那一家（Claude）。
#[test]
fn the_record_face_follows_the_root_the_file_lives_under() {
    let codex_msg = r#"{"timestamp":"t","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}"#;
    fn codex_root() -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from("/codex-root"))
    }
    fn no_sessions() -> Vec<crate::agents::SynthSession> {
        Vec::new()
    }
    fn no_excerpt(_: &std::path::Path) -> String {
        String::new()
    }
    fn home() -> Option<std::path::PathBuf> {
        None
    }
    fn bare(kind: &'static str) -> crate::agents::Adapter {
        crate::agents::Adapter {
            kind,
            home,
            account_env: None,
            assets: None,
            history: None,
            upstream: None,
            mcp: None,
            footprint: None,
            records: None,
            processes: None,
            launch: None,
        }
    }
    let reg = [
        crate::agents::Adapter {
            history: Some(crate::agents::HistoryFace {
                sessions: no_sessions,
                excerpt: no_excerpt,
                root: codex_root,
            }),
            records: Some(crate::agents::codex::RECORDS),
            ..bare("codex-like")
        },
        crate::agents::Adapter {
            records: Some(claude()),
            ..bare("claude-like")
        },
    ];
    let face =
        crate::agents::record_face_among(&reg, std::path::Path::new("/codex-root/2026/x.jsonl"))
            .expect("Codex 根下的文件没人认");
    let p = (face.parse)(codex_msg).unwrap().unwrap();
    assert_eq!(
        p.message["type"], "assistant",
        "Codex 那一家没映射进渲染模型"
    );
    let face = crate::agents::record_face_among(
        &reg,
        std::path::Path::new("/home/u/.claude/projects/a/s.jsonl"),
    )
    .expect("记录树下的文件没人认");
    let p = (face.parse)(USER).unwrap().unwrap();
    assert_eq!(p.cwd.as_deref(), Some("/w"));
    // Codex 的事件行 ⇒ 抢救形、保原文；空行两家都 `Ok(None)`。
    let evt =
        r#"{"timestamp":"t","type":"event_msg","payload":{"type":"task_complete","turn_id":"x"}}"#;
    let p = (crate::agents::codex::RECORDS.parse)(evt).unwrap().unwrap();
    assert_eq!(p.message["type"], "cc-monitor-unrecognized");
    assert!((crate::agents::codex::RECORDS.parse)("  ")
        .unwrap()
        .is_none());
    assert!((claude().parse)("").unwrap().is_none());
}
