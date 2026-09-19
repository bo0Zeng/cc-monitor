use super::*;
use std::io::Write;

/// 独立临时目录（惯例同 utils.rs 测试：temp_dir + pid + 用途标记）。
fn temp_jsonl(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ccm-watcher-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("test-session.jsonl")
}

/// 跑一次 process_file，收集这次 emit 的所有行。
fn run_once(
    path: &Path,
    offsets: &Arc<Mutex<HashMap<PathBuf, FileCursor>>>,
    seqs: &Arc<Mutex<HashMap<PathBuf, u64>>>,
) -> Vec<JsonlLine> {
    let collected: Arc<Mutex<Vec<JsonlLine>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = collected.clone();
    let on_batch: BatchHandler = Arc::new(move |batch| sink.lock().extend(batch));
    let active: ActiveFilter = Arc::new(|_| true);
    process_file(path, offsets, seqs, &on_batch, &active);
    let out = collected.lock().clone();
    out
}

#[test]
fn torn_line_deferred_until_newline_arrives() {
    let path = temp_jsonl("torn");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    // 完整一行 + 无尾 \n 的半行
    std::fs::write(&path, b"{\"a\":1}\n{\"a\":2,\"tex").unwrap();
    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 1, "partial must not be emitted");
    assert_eq!(out[0].raw, r#"{"a":1}"#);
    assert_eq!(out[0].seq, 0);
    let key = path_key(&path);
    let cur = offsets.lock().get(&key).copied().unwrap();
    assert_eq!(
        cur.consumed, 8,
        "offset stops after the complete line, not at EOF"
    );
    assert_eq!(cur.seen_len, 19, "seen_len covers the torn tail");

    // 半行补全 + 再来一行完整的
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(b"t\":\"x\"}\n{\"a\":3}\n").unwrap();
    drop(f);
    let out2 = run_once(&path, &offsets, &seqs);
    assert_eq!(out2.len(), 2, "completed line emitted exactly once");
    assert_eq!(out2[0].raw, r#"{"a":2,"text":"x"}"#);
    assert_eq!(out2[0].seq, 1, "seq continuous across the deferral");
    assert_eq!(out2[1].raw, r#"{"a":3}"#);
    assert_eq!(out2[1].seq, 2);

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn torn_multibyte_utf8_does_not_produce_replacement_char() {
    let path = temp_jsonl("mb");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    // "文" = E6 96 87；撕在第二个字节后
    let full = "{\"t\":\"文\"}\n".as_bytes();
    std::fs::write(&path, &full[..7]).unwrap(); // {"t":" + E6（多字节撕裂点）
    let out = run_once(&path, &offsets, &seqs);
    assert!(
        out.is_empty(),
        "torn multibyte tail must be deferred, not lossy-decoded"
    );

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(&full[7..]).unwrap();
    drop(f);
    let out2 = run_once(&path, &offsets, &seqs);
    assert_eq!(out2.len(), 1);
    assert_eq!(
        out2[0].raw, "{\"t\":\"文\"}",
        "no U+FFFD in the healed line"
    );

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn normal_multi_line_and_incremental_append_regression() {
    let path = temp_jsonl("normal");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    std::fs::write(&path, b"{\"n\":1}\n\n{\"n\":2}\n").unwrap();
    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 2, "blank line skipped");
    assert_eq!(out.iter().map(|l| l.seq).collect::<Vec<_>>(), vec![0, 1]);

    // 无新字节 → 无重投
    let again = run_once(&path, &offsets, &seqs);
    assert!(again.is_empty(), "no re-emit on unchanged file");

    // 增量追加只发新行
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    f.write_all(b"{\"n\":3}\n").unwrap();
    drop(f);
    let out2 = run_once(&path, &offsets, &seqs);
    assert_eq!(out2.len(), 1);
    assert_eq!(out2[0].seq, 2);

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// F14 审计修：torn tail 挂起时文件被非追加式重写、且新长度落在
/// [consumed, 旧 EOF) 窗口内——seen_len 判定必须检出截断并全量重读，
/// 不得从旧 consumed 错位读出垃圾行。
#[test]
fn rewrite_within_torn_window_detected_as_truncation() {
    let path = temp_jsonl("rewrite");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    // 19 字节：完整行(8) + torn tail(11)。consumed=8, seen_len=19。
    std::fs::write(&path, b"{\"a\":1}\n{\"a\":2,\"tor").unwrap();
    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 1);

    // 整体重写成 18 字节新内容：len(18) >= consumed(8) 但 < seen_len(19)
    std::fs::write(&path, b"{\"b\":111}\n{\"b\":2}\n").unwrap();
    let out2 = run_once(&path, &offsets, &seqs);
    assert_eq!(out2.len(), 2, "rewrite must be detected and re-read fully");
    assert_eq!(
        out2[0].raw, r#"{"b":111}"#,
        "no garbage line from a stale offset"
    );
    assert_eq!(out2[1].raw, r#"{"b":2}"#);
    assert_eq!(out2[0].seq, 1, "seq keeps climbing across truncation");

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// F14 审计修：截断到空文件（早退路径）也必须重置游标，
/// 文件重新长回超过旧 offset 时不得漏检、不得丢前缀。
#[test]
fn truncate_to_empty_then_regrow_reads_from_zero() {
    let path = temp_jsonl("regrow");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    std::fs::write(&path, b"{\"n\":1}\n{\"n\":2}\n").unwrap(); // 16 字节
    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 2);

    // 截断到 0（早退分支），再长出比旧 offset 更长的新内容
    std::fs::write(&path, b"").unwrap();
    let empty = run_once(&path, &offsets, &seqs);
    assert!(empty.is_empty());

    std::fs::write(&path, b"{\"m\":1}\n{\"m\":2}\n{\"m\":3}\n").unwrap(); // 24 > 16
    let out2 = run_once(&path, &offsets, &seqs);
    assert_eq!(out2.len(), 3, "must re-read from byte 0, no lost prefix");
    assert_eq!(out2[0].raw, r#"{"m":1}"#);
    assert_eq!(out2[0].seq, 2, "seq never resets");

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// \r\n 行尾：raw 须与旧 BufRead::lines() 行为一致（剥 \n 及紧邻单个 \r），
/// offset 按含 \r\n 的字节数推进。手写字节循环后这是新代码路径，必须钉住。
#[test]
fn crlf_line_endings_match_old_lines_behavior() {
    let path = temp_jsonl("crlf");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    std::fs::write(&path, b"{\"a\":1}\r\n{\"a\":2}\n").unwrap();
    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].raw, r#"{"a":1}"#, "\\r must be stripped");
    assert_eq!(out[1].raw, r#"{"a":2}"#);
    let key = path_key(&path);
    assert_eq!(offsets.lock().get(&key).unwrap().consumed, 17);

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// 旧病 #3 回归测试：完整行内含非法 UTF-8 只 lossy 该行，不截断批次。
#[test]
fn invalid_utf8_in_complete_line_does_not_abort_batch() {
    let path = temp_jsonl("badutf8");
    let offsets = Arc::new(Mutex::new(HashMap::new()));
    let seqs = Arc::new(Mutex::new(HashMap::new()));

    let mut bytes = b"{\"a\":1}\n".to_vec();
    bytes.extend_from_slice(b"\xFF\xFEgarbage\n"); // 非法 UTF-8 完整行
    bytes.extend_from_slice(b"{\"a\":3}\n");
    std::fs::write(&path, &bytes).unwrap();

    let out = run_once(&path, &offsets, &seqs);
    assert_eq!(out.len(), 3, "batch must not be silently aborted");
    assert_eq!(out[2].raw, r#"{"a":3}"#, "lines after the bad one survive");
    assert!(
        out[1].raw.contains('\u{FFFD}'),
        "bad line lossy-decoded, still delivered"
    );

    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
