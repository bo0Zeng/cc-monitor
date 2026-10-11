//! 「你说过的话」清单纯核的判据（`observe::user_inputs`）。
//!
//! 买到：四条口径逐条两向（该进的一条不少、不该进的一条不多，**按文件顺序**）· 头尾形状 ·
//! `end` 只算完整行 · 增量从 `end` 接得上（切成两段再拼 == 一次全量，逐条相等）· 摘要的截断/折叠。
//! **买不到**：渲染那边再剥一层 `stripInternalNoise` 的那几条（头注登记的已知不等价）不在这里判；
//! 夹具是**合成的结构**，不含任何真会话正文。

use super::*;

/// 合成夹具：每行一个形状，`uuid` 名字自带「该不该进」—— `in-*` 该进，`out-*` 不该进。
fn fixture() -> Vec<String> {
    vec![
        // ── 该进的 ──
        r#"{"type":"user","uuid":"in-str","timestamp":"t1","message":{"role":"user","content":"hello there"}}"#.into(),
        r#"{"type":"user","uuid":"in-blocks","timestamp":"t2","message":{"content":[{"type":"text","text":"a"},{"type":"image"},{"type":"text","text":"b"}]}}"#.into(),
        // 文本块与工具结果混着：只取文本块
        r#"{"type":"user","uuid":"in-mixed","message":{"content":[{"type":"tool_result","content":"x"},{"type":"text","text":"why"}]}}"#.into(),
        // 显式 false 不算「是」
        r#"{"type":"user","uuid":"in-flags-false","isMeta":false,"isSidechain":false,"message":{"content":"ok"}}"#.into(),
        // BOM ＋ 前后空白
        "\u{feff}  {\"type\":\"user\",\"uuid\":\"in-bom\",\"message\":{\"content\":\"bom\"}}  ".into(),
        // ── 不该进的 ──
        r#"{"type":"user","uuid":"out-meta","isMeta":true,"message":{"content":"injected"}}"#.into(),
        r#"{"type":"user","uuid":"out-side","isSidechain":true,"agentId":"a1","message":{"content":"sub agent prompt"}}"#.into(),
        r#"{"type":"user","uuid":"out-toolresult","message":{"content":[{"type":"tool_result","content":"x"}]}}"#.into(),
        r#"{"type":"user","uuid":"out-blank","message":{"content":"   \n\t "}}"#.into(),
        r#"{"type":"user","uuid":"out-emptyblocks","message":{"content":[{"type":"text","text":""}]}}"#.into(),
        r#"{"type":"user","message":{"content":"no uuid"}}"#.into(),
        r#"{"type":"user","uuid":"","message":{"content":"empty uuid"}}"#.into(),
        r#"{"type":"assistant","uuid":"out-assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#.into(),
        r#"{"type":"system","uuid":"out-system","content":"sys"}"#.into(),
        r#"{"type":"user","uuid":"out-broken","message":{"content":"#.into(),
        "".into(),
        // 放在最后一条「该进」的：顺序判据要看到它排在所有「不该进」的后面仍然是第 6 条
        r#"{"type":"user","uuid":"in-last","message":{"content":"last"}}"#.into(),
    ]
}

fn bytes_of(lines: &[String]) -> Vec<u8> {
    let mut v = Vec::new();
    for l in lines {
        v.extend_from_slice(l.as_bytes());
        v.push(b'\n');
    }
    v
}

fn run(data: &[u8], from: u64) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    write_user_inputs(&data[from as usize..], from, &mut out).expect("write ok");
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("每行都是 JSON"))
        .collect()
}

fn uuids(v: &[serde_json::Value]) -> Vec<String> {
    v[1..v.len() - 1]
        .iter()
        .map(|r| r["uuid"].as_str().unwrap().to_string())
        .collect()
}

/// 🔴 主判据：出来的 uuid 序列 **==** 夹具里 `in-*` 那几条按文件顺序（两向：多一条少一条都红）。
/// 反空真：夹具里 `in-*` / `out-*` 两类都非空（否则「相等」可以是两个空集相等）。
#[test]
fn the_four_rules_pick_exactly_the_main_line_user_inputs_in_file_order() {
    let lines = fixture();
    let expected: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            let i = l.find("\"uuid\":\"")? + 8;
            let u = &l[i..i + l[i..].find('"')?];
            u.starts_with("in-").then(|| u.to_string())
        })
        .collect();
    let outs = lines.iter().filter(|l| l.contains("\"out-")).count();
    assert_eq!(expected.len(), 6, "夹具里「该进」的条数变了：{expected:?}");
    assert!(
        outs >= 8,
        "夹具里「不该进」的一类塌了（{outs}）—— 两向判据此刻只剩一向"
    );
    let got = uuids(&run(&bytes_of(&lines), 0));
    assert_eq!(got, expected);
}

#[test]
fn rows_carry_timestamp_and_the_text_blocks_joined() {
    let v = run(&bytes_of(&fixture()), 0);
    let rows = &v[1..v.len() - 1];
    assert_eq!(rows[0]["timestamp"], "t1");
    assert_eq!(rows[0]["excerpt"], "hello there");
    // 两个文本块用 `\n` 拼，再折叠成一行 ⇒ "a b"；图片块不贡献文字
    assert_eq!(rows[1]["excerpt"], "a b");
    assert_eq!(rows[2]["excerpt"], "why");
    // 没有 timestamp ⇒ 空串（不是缺字段）
    assert_eq!(rows[2]["timestamp"], "");
}

#[test]
fn head_and_tail_frame_the_rows_and_end_counts_only_complete_lines() {
    let lines = fixture();
    let mut data = bytes_of(&lines);
    let complete = data.len() as u64;
    // torn 残尾：一条还没写完的用户输入 —— 不出、不计进 end
    data.extend_from_slice(br#"{"type":"user","uuid":"in-torn","message":{"content":"half"#);
    let v = run(&data, 0);
    assert_eq!(v[0]["kind"], "user_inputs");
    assert_eq!(v[0]["v"], 1);
    assert_eq!(v[0]["from"], 0);
    let tail = &v[v.len() - 1];
    assert_eq!(tail["kind"], "user_inputs_end");
    assert_eq!(tail["count"], (v.len() - 2) as u64);
    assert_eq!(tail["end"], complete, "end 必须停在最后一个完整行的末字节");
    assert!(!uuids(&v).contains(&"in-torn".to_string()));
}

/// 增量：在任意一个**行边界** k 处切两段，`[0,k)` 的 `end` 喂回去当 `from`，
/// 两段拼起来 **==** 一次全量（逐条、含顺序）。每个行边界都切一次。
#[test]
fn incremental_from_end_concatenates_to_the_full_list_at_every_line_boundary() {
    let lines = fixture();
    let data = bytes_of(&lines);
    let full = uuids(&run(&data, 0));
    let mut boundaries = vec![0u64];
    for (i, b) in data.iter().enumerate() {
        if *b == b'\n' {
            boundaries.push(i as u64 + 1);
        }
    }
    assert_eq!(boundaries.len(), lines.len() + 1);
    for k in boundaries {
        let head = run(&data[..k as usize], 0);
        let end = head[head.len() - 1]["end"].as_u64().unwrap();
        assert_eq!(end, k);
        let tail = run(&data, end);
        assert_eq!(tail[0]["from"], end);
        let mut joined = uuids(&head);
        joined.extend(uuids(&tail));
        assert_eq!(joined, full, "在字节 {k} 处切开后拼不回全量");
    }
}

#[test]
fn excerpt_folds_whitespace_and_cuts_at_the_limit() {
    assert_eq!(excerpt("a\n\n  b\tc"), "a b c");
    let exact: String = "字".repeat(EXCERPT_MAX);
    assert_eq!(excerpt(&exact), exact, "恰好 {EXCERPT_MAX} 个字不加省略号");
    let over: String = "字".repeat(EXCERPT_MAX + 1);
    let got = excerpt(&over);
    assert_eq!(
        got.chars().count(),
        EXCERPT_MAX + 1,
        "截到上限 ＋ 一个省略号"
    );
    assert!(got.ends_with('…'));
    assert!(!exact.contains('…'));
}

/// 病态输入：前缀 640 个空白之后才是正文 ⇒ 先截断会一个字都不剩，必须退回整条折叠。
#[test]
fn excerpt_falls_back_to_the_whole_text_when_the_head_is_all_whitespace() {
    let text = format!("{}tail words here", " ".repeat(EXCERPT_MAX * 8));
    assert_eq!(excerpt(&text), "tail words here");
    // 对照：前缀够长时不退回（不整条折叠也拿得到够长的摘要）
    let long = format!(
        "{} {}",
        "w".repeat(EXCERPT_MAX),
        "z".repeat(EXCERPT_MAX * 20)
    );
    assert!(excerpt(&long).starts_with(&"w".repeat(EXCERPT_MAX)));
}

/// 从骨架索引的输出里把大纲搬出来（前端 `outline-source.ts::outlineSeedFromIndex` 做的同一件事）：
/// 带 `x` 的行 ⇒ `{uuid: u, excerpt: x, timestamp: ts ?? ""}`。
fn outline_from_index(data: &[u8]) -> Vec<(String, String, String)> {
    let mut v = Vec::new();
    crate::observe::history_query::scan_session_index(data, 0, None, |row| {
        v.push(serde_json::to_value(row).unwrap());
        Ok(())
    })
    .expect("index ok");
    v.iter()
        .filter(|r| r.get("x").is_some())
        .map(|r| {
            (
                r["u"].as_str().expect("带 x 的行必有 u").to_string(),
                r["x"].as_str().unwrap().to_string(),
                r.get("ts")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        })
        .collect()
}

/// 🔴骨架索引里**带 `x` 的行 ⇔ 夹具里的 `in-*`**（两向，文件序）。
/// 期望取自夹具的 uuid 命名（异源），不取自判定函数 —— 索引若对某一类用户输入漏标、
/// 或把工具结果回灌 / meta / 子 agent 标上，这一格当场红。
#[test]
fn the_skeleton_index_marks_exactly_the_user_inputs() {
    let lines = fixture();
    let expected: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            let i = l.find("\"uuid\":\"")? + 8;
            let u = &l[i..i + l[i..].find('"')?];
            u.starts_with("in-").then(|| u.to_string())
        })
        .collect();
    assert_eq!(expected.len(), 6, "夹具里「该进」的一类变了");
    let got: Vec<String> = outline_from_index(&bytes_of(&lines))
        .into_iter()
        .map(|(u, _, _)| u)
        .collect();
    assert_eq!(got, expected);
}

/// 🔴**两个出口给出同一份清单**：从索引搬出来的 == `--list-user-inputs` 的，
/// uuid · 摘要 · 时间戳逐条相等、含顺序。前端据此在索引带回清单时**不再单独要一趟**。
/// 反空真：清单非空，且摘要与时间戳两样里都有非平凡值（空串对空串不算数）。
#[test]
fn the_skeleton_index_carries_the_same_outline_as_the_list() {
    let data = bytes_of(&fixture());
    let v = run(&data, 0);
    let listed: Vec<(String, String, String)> = v[1..v.len() - 1]
        .iter()
        .map(|r| {
            (
                r["uuid"].as_str().unwrap().to_string(),
                r["excerpt"].as_str().unwrap().to_string(),
                r["timestamp"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        listed.iter().any(|(_, x, _)| x == "a b"),
        "摘要要走过折叠那一支"
    );
    assert!(listed.iter().any(|(_, _, t)| t == "t1"), "时间戳要有非空的");
    assert!(
        listed.iter().any(|(_, _, t)| t.is_empty()),
        "时间戳要有缺席的"
    );
    assert_eq!(outline_from_index(&data), listed);
}
