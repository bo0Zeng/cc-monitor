use super::*;

fn claude() -> RecordFace {
    crate::agents::claudecode::RECORDS
}

const USER: &str = r#"{"type":"user","uuid":"u1","timestamp":"t","cwd":"/w","message":{"role":"user","content":"q"}}"#;
const MODE: &str = r#"{"type":"mode","mode":"normal"}"#;

const FOLD_PAGE: &[&str] = &[
    r#"{"type":"user","uuid":"u1","timestamp":"2026-01-02T03:04:05.000Z","cwd":"/w","message":{"role":"user","content":[{"type":"text","text":"ZQKEEP-user"}]}}"#,
    r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","timestamp":"2026-01-02T03:04:06.000Z","message":{"role":"assistant","model":"m","content":[{"type":"thinking","thinking":"ZQBODY-think"},{"type":"text","text":"ZQBODY-say"},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/f.txt","zq_extra":"ZQBODY-input"}}],"usage":{"input_tokens":11,"output_tokens":22}}}"#,
    r#"{"type":"user","uuid":"u2","parentUuid":"a1","timestamp":"2026-01-02T03:04:07.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ZQBODY-result"}]},"toolUseResult":{"type":"text","file":{"numLines":7}}}"#,
    r#"{"type":"queue-operation","operation":"remove","timestamp":"2026-01-02T03:04:08.000Z","content":"ZQKEEP-queued"}"#,
    r#"{"type":"zq-no-such-kind","uuid":"x1","timestamp":"2026-01-02T03:04:09.000Z","payload":"ZQBODY-raw"}"#,
];

/// 行摘要：每个可计行一条；末端是含 `\n` 之后那个字节（CRLF 的 `\r` 计在内）· 残尾 `null` · 空白行不占 ·
/// 不进界面的只有 `{end, hash}`。
#[test]
fn rows_carry_exact_ends_and_only_displayable_messages() {
    let page = format!("{USER}\r\n\n  \n{MODE}\n{{torn");
    let rows = rows_of(&claude(), 100, page.as_bytes(), false);
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
        false,
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
            accounts: None,
            records: None,
            processes: None,
            launch: None,
            compact_request: None,
            local: None,
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

/// 夹具里只住**正文**那几格的标记（开关开 ⇒ 一个都不许剩）。
/// 每个标记只有一个出处：思考 · 说的话 · 工具入参里主参数之外的那一格 · 工具结果正文 · 抢救下来的整行原文。
const GONE: &[&str] = &[
    "ZQBODY-think",
    "ZQBODY-say",
    "ZQBODY-input",
    "ZQBODY-result",
    "ZQBODY-raw",
];

/// **折起那一行自己要用的**那几格里的标记（开关开 ⇒ 一个都不许少）。
/// 它们在原文里也住 `message.content` / `content`，但后端判好的成品（`userText.text`）里**另有一份**
/// ⇒ 剥正文是**去重**，不是把人说的话弄丢。这一半不立，「不许有正文」那一半把投影整个弄坏也能恒绿。
const KEPT: &[&str] = &["ZQKEEP-user", "ZQKEEP-queued"];

/// 折起那一行要用的键名（开关开 ⇒ 逐个还在；它们正是界面画那一行读的那几格）。
const FOLDED_CELLS: &[(&str, &str)] = &[
    ("u1", "userText"),
    ("a1", "toolCards"),
    ("a1", "toolSteps"),
    ("u2", "toolResults"),
];

/// ★ **两头都断**：`summary_only` 开 ⇒ 正文那几格一个不剩、折起那一行要用的一格不少；关 ⇒ 正文**必须**在。
///
/// 两头各自都不够：只断「开 ⇒ 不许有正文」的话，把投影整个弄坏（`message` 恒空、一条都不出成品）也能让它绿；
/// 只断「关 ⇒ 有正文」的话，开关根本没接上也能绿。⇒ 本条逐标记两向都判，再加上**条数 · 行号 · 身份一格不变**
/// （剥的是内容，不是「这一行在不在、是第几行」）。
#[test]
fn the_summary_only_product_drops_every_body_cell_and_keeps_every_folded_cell() {
    let page = FOLD_PAGE.join("\n") + "\n";
    let at = std::path::Path::new("/p/zq.jsonl");
    let shape = |summary_only: bool| {
        let (lines, next) = record_lines_of_page(&claude(), at, 5, page.as_bytes(), summary_only);
        let (rows, text) = (
            rows_of(&claude(), 0, page.as_bytes(), summary_only),
            serde_json::to_string(&lines).unwrap(),
        );
        (lines, next, rows, text)
    };
    let (full_lines, full_next, full_rows, full_text) = shape(false);
    let (fold_lines, fold_next, fold_rows, fold_text) = shape(true);

    // ── 关（默认那一形）：正文**必须**在，一个标记都不许缺 ──
    for m in GONE.iter().chain(KEPT) {
        assert!(
            full_text.contains(m),
            "不给开关时 `{m}` 不在成品里 —— 夹具或投影坏了，下面那一半会恒绿"
        );
    }
    // ── 开：正文那几格一个不剩 ──
    for m in GONE {
        assert!(
            !fold_text.contains(m),
            "`summary_only` 置真，正文标记 `{m}` 还在成品里：{fold_text}"
        );
    }
    // ── 开：折起那一行自己要用的一格不少 ──
    for m in KEPT {
        assert!(
            fold_text.contains(m),
            "`summary_only` 置真把折起那一行要显示的 `{m}` 也剥掉了"
        );
    }
    let cell = |lines: &[serde_json::Value], uuid: &str, key: &str| {
        lines
            .iter()
            .find(|l| l["message"]["uuid"] == uuid)
            .unwrap_or_else(|| panic!("夹具里没有 uuid={uuid} 那一条"))["message"][key]
            .clone()
    };
    for (uuid, key) in FOLDED_CELLS {
        assert_eq!(
            cell(&fold_lines, uuid, key),
            cell(&full_lines, uuid, key),
            "`{key}`（uuid={uuid}）在折起那一形里变了 —— 它是界面画那一行读的那一格"
        );
    }
    // ── 开：`message` 那个对象留着（折起那一行按 `usage` 报字数），只是没了 `content` ──
    let msg = cell(&fold_lines, "a1", "message");
    assert_eq!(msg["role"], "assistant");
    assert_eq!(msg["usage"]["output_tokens"], 22);
    assert!(
        msg.get("content").is_none(),
        "`content` 该是**删掉**而不是给空值（给空值等于说这一条没有正文，那是假话）：{msg}"
    );

    // ── 两形之间：条数 · 行号 · 身份 · 行摘要那几格一格不变 ──
    assert_eq!(fold_next, full_next, "剥正文不许动行号");
    assert_eq!(fold_lines.len(), full_lines.len(), "剥正文不许少出一条");
    assert_eq!(fold_lines.len(), FOLD_PAGE.len(), "夹具每一行都该出成品");
    let ident = |lines: &[serde_json::Value]| -> Vec<serde_json::Value> {
        lines
            .iter()
            .map(|l| {
                serde_json::json!([
                    l["seq"].clone(),
                    l["session_id"].clone(),
                    l["path"].clone(),
                    l["cwd"].clone(),
                    l["message"]["uuid"].clone(),
                    l["message"]["type"].clone(),
                    l["message"]["timeText"].clone(),
                ])
            })
            .collect()
    };
    assert_eq!(
        ident(&fold_lines),
        ident(&full_lines),
        "行标识 / 时刻字格变了"
    );
    let ends = |rows: &[serde_json::Value]| -> Vec<serde_json::Value> {
        rows.iter()
            .map(|r| serde_json::json!([r["end"].clone(), r["hash"].clone(), r["cwd"].clone()]))
            .collect()
    };
    assert_eq!(
        ends(&fold_rows),
        ends(&full_rows),
        "行摘要的 `end` / `hash` / `cwd` 变了（续传要靠它们核「还是不是那一行」）"
    );
    // 剩不下**一个** `content` 键：三处剥（`message.content` · `queue-operation` 的 `content`）漏一处就红。
    // 比「小了多少」强：省多少是夹具的函数（真数据上的读数是交回时量的一次，约省七成），
    // 而「这个键名一个都不剩」是形状上的话，跟夹具大小无关。
    assert!(
        !fold_text.contains("\"content\""),
        "折起那一形里还剩 `content` 这个键：{fold_text}"
    );
    assert!(
        full_text.contains("\"content\""),
        "不给开关时连 `content` 键都没有 —— 上面那一条会恒绿"
    );
    assert!(
        fold_text.len() < full_text.len(),
        "折起那一形没比全文小（{} vs {}）",
        fold_text.len(),
        full_text.len()
    );
}
