//! 〔骨架〕骨架索引的扫描（帧面 `history-index` 用的那一个）与 `--read-session-from-offset` 选项解析的判据。
//!
//! 买到：索引与 seq 空间**逐行对齐**（与 `--read-session-tail` 那份计数同一口径、同一个 `line_counts`）·
//! 偏移是绝对的、续传从 `end` 接得上 · 宽度无关料按块型分开数 · 选项解析拒写错的尾随参数。
//! **买不到**：料「数得准不准」只对到原料层（前端最终建不建卡、建成哪种卡，这里不知道）；

use super::*;

/// 扫一遍，排成「头 · 每行 · 尾」三段（头带 `from`，尾带 `count` · `end`），下面的判据照这三段读。
fn index_of(data: &[u8], from: u64, until: Option<u64>) -> Vec<serde_json::Value> {
    let mut rows = vec![serde_json::json!({"kind": "session_index", "v": 1, "from": from})];
    let (count, end) = scan_session_index(
        &data[from.min(data.len() as u64) as usize..],
        from,
        until,
        |row| {
            rows.push(serde_json::to_value(row).expect("每行都是 JSON"));
            Ok(())
        },
    )
    .expect("index ok");
    rows.push(serde_json::json!({"kind": "session_index_end", "count": count, "end": end}));
    rows
}

/// 头一行、尾一行、中间是行 —— 形状本身。
fn split(
    v: &[serde_json::Value],
) -> (&serde_json::Value, &[serde_json::Value], &serde_json::Value) {
    assert!(v.len() >= 2, "至少有头尾两行：{v:?}");
    assert_eq!(
        v[0]["kind"], "session_index",
        "首行必须能认出「对面会出索引」"
    );
    assert_eq!(
        v[v.len() - 1]["kind"],
        "session_index_end",
        "没有尾行 ⇒ 输出被截断"
    );
    (&v[0], &v[1..v.len() - 1], &v[v.len() - 1])
}

/// 🔴 **seq 空间对齐**：第 k 行就是第 k 个可计行 —— 与 `split_tail`（`--read-session-tail` 的口径锚）
/// 数出来的 `total` 相等、每行起点相等。空行不占号、torn 残尾不计。
#[test]
fn index_rows_share_the_seq_space_with_the_tail_counter() {
    let data: &[u8] = b"{\"type\":\"user\",\"uuid\":\"a\"}\n\n   \n{\"type\":\"assistant\",\"uuid\":\"b\"}\n\xEF\xBB\xBF{\"type\":\"system\",\"subtype\":\"api_error\",\"timestamp\":\"t\"}\n{\"type\":\"torn";
    let v = index_of(data, 0, None);
    let (_, rows, tail) = split(&v);
    let (meta, _, _) = split_tail(data, 1_000);
    let total: serde_json::Value = serde_json::from_str(meta.trim()).unwrap();
    assert_eq!(
        rows.len() as u64,
        total["total"].as_u64().unwrap(),
        "索引行数 ≠ 可计行数"
    );
    assert_eq!(tail["count"].as_u64().unwrap(), rows.len() as u64);
    // 起点逐个对：手算 —— 行 0 在 0；两行空白占 1+4 字节；行 1 在 27+1+4=32；BOM 行紧随其后
    let first_len = b"{\"type\":\"user\",\"uuid\":\"a\"}\n".len() as u64;
    assert_eq!(rows[0]["o"], 0);
    assert_eq!(rows[0]["n"], first_len);
    assert_eq!(rows[1]["o"], first_len + 1 + 4);
    assert_eq!(rows[1]["u"], "b");
    let second_end = rows[1]["o"].as_u64().unwrap() + rows[1]["n"].as_u64().unwrap();
    assert_eq!(rows[2]["o"], second_end);
    assert_eq!(rows[2]["t"], "retry", "BOM 行照常解析");
    // end = 最后一个完整行的末字节（torn 那截不算）＝ 下次续传的 offset
    let torn_at = data.len() as u64 - b"{\"type\":\"torn".len() as u64;
    assert_eq!(tail["end"].as_u64().unwrap(), torn_at);
}

/// 续传：从上次的 `end` 起算，只出新行、偏移仍是**绝对**的。
#[test]
fn index_resumes_from_the_previous_end() {
    let first: &[u8] = b"{\"type\":\"user\"}\n{\"type\":\"assistant\"}\n";
    let mut grown = first.to_vec();
    grown.extend_from_slice(b"{\"type\":\"user\",\"uuid\":\"new\"}\n");
    let v1 = index_of(first, 0, None);
    let end = split(&v1).2["end"].as_u64().unwrap();
    assert_eq!(end, first.len() as u64);
    let v2 = index_of(&grown, end, None);
    let (head, rows, _) = split(&v2);
    assert_eq!(head["from"], end);
    assert_eq!(rows.len(), 1, "续传只出新行");
    assert_eq!(rows[0]["o"], end, "偏移是绝对的，不是相对 from");
    assert_eq!(rows[0]["u"], "new");
}

/// `--until`：只收起点 < until 的行，起点在界内的那一行**整行**收。
#[test]
fn until_cuts_by_line_start_never_mid_line() {
    let data: &[u8] = b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n";
    // 8 = 第二行起点：只收第一行
    assert_eq!(split(&index_of(data, 0, Some(8))).1.len(), 1);
    // 9 = 第二行中间：第二行起点 8 < 9 ⇒ 整行收
    assert_eq!(split(&index_of(data, 0, Some(9))).1.len(), 2);
    // until ≤ from ⇒ 空，但头尾照出
    assert_eq!(split(&index_of(data, 8, Some(8))).1.len(), 0);
}

/// 宽度无关料：正文 / 代码 / 折叠单元分开数；CJK 口径同前端 `> 0x2E80`。
#[test]
fn facts_count_prose_code_and_folded_units_separately() {
    let line = serde_json::json!({
        "type": "assistant",
        "uuid": "u1",
        "timestamp": "2026-10-09T00:00:00Z",
        "message": {"role": "assistant", "content": [
            {"type": "thinking", "thinking": "很长很长的思考不该算进正文"},
            {"type": "text", "text": "第一段ab\n\n```rust\nfn a() {}\nfn b() {}\n```\n结尾"},
            {"type": "tool_use", "id": "t", "name": "Bash", "input": {"command": "ls"}}
        ]}
    })
    .to_string();
    let r = index_row(line.as_bytes(), 10, line.len() as u64 + 1);
    assert_eq!(r.o, 10);
    assert_eq!(
        r.t,
        Some(crate::agents::record::RecordClass::Reply),
        "骨架行的类是通用记录的类"
    );
    assert_eq!(r.u.as_deref(), Some("u1"));
    assert_eq!(r.fd, 2, "thinking + tool_use 两个折叠单元");
    assert_eq!(r.cb, 1);
    assert_eq!(r.cl, 2, "代码块内两行");
    assert_eq!(r.pl, 2, "正文两个非空硬行（空行不算）");
    assert_eq!(
        r.ch,
        "第一段ab".chars().count() as u32 + "结尾".chars().count() as u32
    );
    assert_eq!(r.cj, 5, "第一段 3 + 结尾 2");
}

#[test]
fn facts_for_user_string_meta_sidechain_system_and_garbage() {
    let u = r#"{"type":"user","isMeta":true,"isSidechain":true,"agentId":"a1","message":{"role":"user","content":"两行\n第二行"}}"#;
    let r = index_row(u.as_bytes(), 0, 0);
    assert!(r.sc);
    assert_eq!(r.sp, Some("system"), "isMeta ⇒ 系统注入");
    assert_eq!((r.pl, r.ch, r.cj), (2, 5, 5));
    // tool_result 数组：只算折叠单元，正文 0
    let tr = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"大段输出"}]}}"#;
    let r = index_row(tr.as_bytes(), 0, 0);
    assert_eq!((r.fd, r.ch, r.pl), (1, 0, 0));
    // system：正文在顶层 content
    let sys = r#"{"type":"system","content":"API Error"}"#;
    assert_eq!(index_row(sys.as_bytes(), 0, 0).ch, 9);
    // 🔴 非 JSON：**行仍在**（占一个 seq），只是没有料 —— 丢了它后面全错一位
    let bad = index_row(b"{\"type\":\"user\"", 5, 15);
    assert_eq!((bad.o, bad.n, bad.t), (5, 15, None));
}

/// 零值不上线：索引每条一行，省下来的是乘以条数的字节。
#[test]
fn zero_facts_are_not_serialized() {
    let r = index_row(br#"{"type":"attachment"}"#, 0, 22);
    let s = serde_json::to_string(&r).unwrap();
    assert_eq!(s, r#"{"o":0,"n":22}"#, "不进界面的行连类都不带");
}

/// `--until` 让透传收成半开区间 `[offset, end)`；`None` 字节一个不变（老调用方零回归，
/// 由 `stream_from_offset_production_path_byte_parity` 另钉）。
#[test]
fn stream_from_offset_until_is_half_open() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-until-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let data = b"{\"a\":0}\n{\"a\":1}\n{\"a\":2}\n";
    let p = tmp.join("s.jsonl");
    std::fs::write(&p, data).unwrap();
    for (off, until, want) in [
        (0u64, 8u64, &data[0..8]),
        (8, 16, &data[8..16]),
        (8, 8, &data[0..0]),
        (16, 4, &data[0..0]),
        (16, 999, &data[16..]),
    ] {
        let mut f = std::fs::File::open(&p).unwrap();
        let mut got = Vec::new();
        stream_from_offset(&mut f, off, Some(until), &mut got).unwrap();
        assert_eq!(got, want, "[{off},{until})");
    }
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn from_offset_args_parse_strictly_and_in_any_order() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let parse = |v: &[&str]| {
        let a = s(v);
        parse_from_offset_args(&a).map(|(o, p)| (o, p.into_iter().cloned().collect::<Vec<_>>()))
    };
    // 老形状（无选项）零回归
    assert_eq!(
        parse(&["/p.jsonl", "0"]).unwrap(),
        (FromOffsetOpts::default(), s(&["/p.jsonl", "0"]))
    );
    // 选项在后 · 在前都认
    assert_eq!(
        parse(&["/p.jsonl", "5", "--until", "42"]).unwrap(),
        (FromOffsetOpts { until: Some(42) }, s(&["/p.jsonl", "5"]))
    );
    assert_eq!(
        parse(&["--until", "42", "/p.jsonl", "5"]).unwrap(),
        (FromOffsetOpts { until: Some(42) }, s(&["/p.jsonl", "5"]))
    );
    // 骨架索引不再是这条的选项（帧命令 `history-index`）
    assert!(
        parse(&["--index", "/p.jsonl", "5"]).is_err(),
        "--index 已删"
    );
    assert!(
        parse(&["/p.jsonl", "0", "--until"]).is_err(),
        "--until 缺值"
    );
    assert!(
        parse(&["--until", "x", "/p.jsonl", "0"]).is_err(),
        "--until 非数字"
    );
    assert!(
        parse(&["/p.jsonl", "0", "--indx"]).is_err(),
        "写错的选项必须报错"
    );
    assert!(
        parse(&["/p.jsonl", "0", "extra"]).is_err(),
        "多余的位置参数必须报错"
    );
}

/// **跨 crate 的 seq 空间对拍**：后端索引读同一份夹具，逐行对同一份金标准。
/// monitor 那侧（`session_skeleton·rs::LineNumberer`）在 `tests/frontend/shell/session_skeleton_tests.rs`
/// 对**同一份**金标准 —— 两侧实现不同源，金标准是手算的（见 `.golden` 头注）。
#[test]
fn index_rows_match_the_shared_seq_space_golden() {
    let data: &[u8] = include_bytes!("../../__fixtures__/skeleton-seq-space.jsonl");
    let golden = include_str!("../../__fixtures__/skeleton-seq-space.golden");
    let mut want: Vec<(u64, Option<String>)> = Vec::new();
    let (mut count, mut end) = (None, None);
    for l in golden.lines() {
        if let Some(v) = l.strip_prefix("#count\t") {
            count = v.parse::<u64>().ok();
        } else if let Some(v) = l.strip_prefix("#end\t") {
            end = v.parse::<u64>().ok();
        } else if !l.starts_with('#') {
            let (s, u) = l.split_once('\t').unwrap();
            want.push((s.parse().unwrap(), (u != "-").then(|| u.to_string())));
        }
    }
    assert!(
        !want.is_empty(),
        "金标准一行都没抽到 —— 下面的相等在空集上绿"
    );
    let v = index_of(data, 0, None);
    let (_, rows, tail) = split(&v);
    let got: Vec<(u64, Option<String>)> = rows
        .iter()
        .enumerate()
        .map(|(k, r)| (k as u64, r["u"].as_str().map(str::to_string)))
        .collect();
    assert_eq!(got, want);
    assert_eq!(tail["count"].as_u64(), count);
    assert_eq!(tail["end"].as_u64(), end);
}
