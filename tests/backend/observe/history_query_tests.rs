use super::*;
use crate::agents::claudecode::paths::projects_root;

fn fixture_project(root: &Path, dir_name: &str) -> PathBuf {
    let dir = root.join("projects").join(dir_name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_jsonl(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, lines.join("\n")).unwrap();
    p
}

#[test]
fn analyze_extracts_excerpt_title_cwd_count() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-test-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-a");
    let p = write_jsonl(
        &dir,
        "s1.jsonl",
        &[
            r#"{"type":"user","cwd":"/home/pi/proj","isMeta":true,"message":{"role":"user","content":"skill 注入不算"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"真正的首条用户输入，应该成为摘要"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"回复"}]}}"#,
            r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
            r#"{"type":"ai-title","aiTitle":"最新标题"}"#,
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["sessionId"], "s1");
    assert_eq!(v["messageCountApprox"], 5);
    assert_eq!(v["cwd"], "/home/pi/proj");
    assert_eq!(v["aiTitle"], "最新标题"); // 取最新
    assert!(v["firstUserExcerpt"]
        .as_str()
        .unwrap()
        .starts_with("真正的首条用户输入")); // isMeta 跳过
    std::fs::remove_dir_all(&tmp).ok();
}

// `truncate_is_char_safe` 随被测的 `truncate_chars`〔散文墓碑〕一起退役：摘录改用 `search_rules::truncate_excerpt`，
//   「不劈码点」由口径那一家自己的判据守（`tests/backend/observe/search_rules_tests.rs`）；本文件下面 C4d 那一节钉摘录的整形。

/// Batch11-F32：sessionKind:"bg" 探测 → isBg。
#[test]
fn analyze_session_detects_bg_kind() {
    let tmp = std::env::temp_dir().join(format!("ccm-isbg-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let f = tmp.join("bg-sid.jsonl");
    std::fs::write(
        &f,
        concat!(
            "{\"type\":\"ai-title\",\"aiTitle\":\"迁移任务\",\"sessionKind\":\"bg\"}\n",
            "{\"type\":\"user\",\"uuid\":\"u1\",\"sessionKind\":\"bg\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n"
        ),
    )
    .unwrap();
    let v = analyze_session(&f);
    assert_eq!(v["isBg"], true);
    let g = tmp.join("normal.jsonl");
    std::fs::write(
        &g,
        "{\"type\":\"user\",\"uuid\":\"u1\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
    )
    .unwrap();
    assert_eq!(analyze_session(&g)["isBg"], false);
    std::fs::remove_dir_all(&tmp).ok();
}

// 〔审计 F 🔴-6〕「围栏只许有一处」（audit-0805 08-06，定框 E3）那一条搬去 `fence_tests.rs`、名字照旧：
//   它先前只数**本文件**里的 `canonicalize()`（== 2），而 `search_query.rs` 里还有一份内联的 ——
//   「守卫范围 ≠ 性质范围」。围栏收进 `observe/fence.rs` 之后，人群换成 observe 全树。
// LOC1b 把那一条的锚从 `fn fence_under_projects` 换成了 `fn fence_under_root`（本体提成根是参数）；
//   那一形今天就是 `Fence::at` ＋ `admit`，锚跟着住 `fence_tests.rs`。

#[test]
fn list_sessions_rejects_path_traversal() {
    let tmp = std::env::temp_dir();
    assert!(list_sessions(&tmp, "../escape").is_err());
    assert!(list_sessions(&tmp, "a/b").is_err());
    assert!(list_sessions(&tmp, r"a\b").is_err());
}

#[test]
fn read_session_rejects_outside_projects() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-ro-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-b");
    write_jsonl(&dir, "ok.jsonl", &[r#"{"type":"user"}"#]);
    // projects 外的真实文件 → 拒绝
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "nope").unwrap();
    assert!(read_session(&tmp, &outside.to_string_lossy()).is_err());
    // 非 jsonl → 拒绝
    let txt = dir.join("note.txt");
    std::fs::write(&txt, "x").unwrap();
    assert!(read_session(&tmp, &txt.to_string_lossy()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

/// backend-02：offset 续拉的字节语义**逐字节对拍** watcher 发出的 `byte_offset`
/// 与 aterm `tail -c +(offset+1)`。用与 `byte_offset_matches_aterm_lineframer` 同一
/// 语料 `"你\r\nx\n"`（LineFramer offset=[5,7]）：从续点 N 起 = `bytes[N..]`。
#[test]
fn slice_from_offset_matches_lineframer_resume() {
    // 你=3B + \r\n=2 → line1 endOffset 5；x=1 + \n=1 → line2 endOffset 7；共 7B。
    let data = "你\r\nx\n".as_bytes();
    assert_eq!(data.len(), 7);
    // 从 0 续 = 整个文件（首次全量）。
    assert_eq!(slice_from_offset(data, 0), data);
    // 从 line1 的 byte_offset=5 续 = 只剩 line2 "x\n"（不重发 line1，不跳字节）。
    assert_eq!(slice_from_offset(data, 5), b"x\n");
    // 从 line2 的 byte_offset=7 续 = EOF、空（无新行）。
    assert_eq!(slice_from_offset(data, 7), b"");
    // offset > len（远端截断/重写）→ 空、不 panic（客户端另经 size 查 reset）。
    assert_eq!(slice_from_offset(data, 100), b"");
    // 中途续点（非行边界，理论上不该发生，但语义须良定义）：透传该字节起余部。
    assert_eq!(slice_from_offset(data, 4), b"\nx\n");
}

/// backend-02：offset 续拉沿用 `read_session` 的路径守卫（projects 外 / 非 jsonl 拒）。
#[test]
fn read_session_from_offset_path_guard() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-off-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-off");
    write_jsonl(&dir, "ok.jsonl", &[r#"{"type":"user"}"#]);
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "nope").unwrap();
    // projects 外 → 拒（守卫先于 seek）。
    assert!(read_session_from_offset(&tmp, &outside.to_string_lossy(), 0, None).is_err());
    // 合法 jsonl + offset 超长 → seek 过 EOF 读空、Ok（不 panic、不报错）。
    let ok = dir.join("ok.jsonl");
    assert!(read_session_from_offset(&tmp, &ok.to_string_lossy(), 9999, None).is_ok());
    std::fs::remove_dir_all(&tmp).ok();
}

/// 审计 quality/correctness-重要：**发货的 seek 路径本身**（`stream_from_offset`）的续拉字节
/// 直接断言（此前只测 `slice_from_offset` 助手 + path_guard 的 Ok/Err，生产 seek 输出无字节测
/// → dup-drift 风险）。用 byte_offset golden 同语料 `"你\r\nx\n"`，逐 offset 对拍 `slice_from_offset`
/// （= `tail -c +(offset+1)`），证生产 `File::seek`+`copy` 与助手语义**逐字节一致**。
#[test]
fn stream_from_offset_production_path_byte_parity() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-sfo-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let data = "你\r\nx\n".as_bytes(); // 7B；byte_offset golden 语料
    let p = tmp.join("s.jsonl");
    std::fs::write(&p, data).unwrap();
    for off in [0u64, 4, 5, 7, 100] {
        let mut f = std::fs::File::open(&p).unwrap();
        let mut got = Vec::new();
        let n = stream_from_offset(&mut f, off, None, &mut got).expect("stream ok");
        // 对拍纯助手（= 生产 seek 应吐的字节）：逐字节一致 + copy 返回字节数吻合。
        assert_eq!(got, slice_from_offset(data, off), "off={off} 字节不符");
        assert_eq!(n as usize, got.len(), "off={off} copy 计数不符");
    }
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
#[cfg(unix)]
fn list_sessions_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-sym-{}", std::process::id()));
    let projects = tmp.join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    // projects/ 外的目录，放一个 jsonl
    let outside = tmp.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("leak.jsonl"), r#"{"type":"user"}"#).unwrap();
    // projects/sneaky -> ../outside（名字合法、无分隔符、无 ..，旧 string 校验放行）
    symlink(&outside, projects.join("sneaky")).unwrap();
    // canonicalize 前缀校验解析 symlink 后落在 projects/ 外 → 拒绝
    assert!(list_sessions(&tmp, "sneaky").is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

// ════════════════════════════════════════════════════════════════════════════════════════
// 会话清单那一行从此是本机与远端共用的唯一口径 —— 补上的三格 ＋ 摘录清洗
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 要求：「本机后端 … 出成品 …（join 只一个家）」——
// 本机会话清单从 monitor 那份（经记录解析器）换到这一行，monitor 那份有的三格这里要有，否则本机用户换读者那一刻丢 fork 树与改名后的标题。

/// ★ fork 关系（首条带 `forkedFrom` 的 user / assistant）· `custom-title` 取最新 · 开始时刻取首条时间戳。
#[test]
fn c4d_the_row_carries_fork_parent_custom_title_and_first_timestamp() {
    let tmp = std::env::temp_dir().join(format!("ccm-c4d-row-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-c4d");
    let p = write_jsonl(
        &dir,
        "child.jsonl",
        &[
            r#"{"type":"file-history-snapshot","messageId":"x"}"#,
            r#"{"type":"user","timestamp":"2026-09-25T01:02:03.456Z","cwd":"/w","forkedFrom":{"sessionId":"parent-sid","messageUuid":"m-1"},"message":{"role":"user","content":"<system-reminder>注入</system-reminder>占位问题\n第二行"}}"#,
            r#"{"type":"assistant","timestamp":"2026-09-25T01:02:04.000Z","forkedFrom":{"sessionId":"other","messageUuid":"m-2"},"message":{"role":"assistant","content":[]}}"#,
            r#"{"type":"ai-title","aiTitle":"旧标题"}"#,
            r#"{"type":"custom-title","customTitle":"改过的名字"}"#,
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["forkedFromSessionId"], "parent-sid", "取首条那一个");
    assert_eq!(v["forkedFromMessageUuid"], "m-1");
    assert_eq!(v["aiTitle"], "改过的名字", "custom-title 在后 ⇒ 取它");
    assert_eq!(
        v["startedAtMs"],
        crate::common::time::parse_iso8601_ms("2026-09-25T01:02:03.456Z").unwrap()
    );
    assert_eq!(
        v["firstUserExcerpt"], "占位问题 第二行",
        "注入包装剥掉、换行折成空格"
    );
    // 没有 forkedFrom / 没有时间戳 ⇒ 两格 null、开始时刻退回文件时刻（不是 0）。
    let q = write_jsonl(
        &dir,
        "plain.jsonl",
        &[r#"{"type":"user","message":{"role":"user","content":"[Request interrupted by user]"}}"#],
    );
    let w = analyze_session(&q);
    assert_eq!(w["forkedFromSessionId"], serde_json::Value::Null);
    assert_eq!(w["forkedFromMessageUuid"], serde_json::Value::Null);
    assert!(
        w["startedAtMs"].as_i64().unwrap() > 0,
        "没有时间戳也不许报 0"
    );
    assert_eq!(w["firstUserExcerpt"], "", "纯中断标记不是用户说的话");
    // 摘录超长 ⇒ 120 个字符 ＋ `…`。
    let long = "字".repeat(130);
    let line = format!(r#"{{"type":"user","message":{{"role":"user","content":"{long}"}}}}"#);
    let r = write_jsonl(&dir, "long.jsonl", &[line.as_str()]);
    let x = analyze_session(&r);
    assert_eq!(x["firstUserExcerpt"], format!("{}…", "字".repeat(120)));
    std::fs::remove_dir_all(&tmp).ok();
}

/// ★ 这一行的键集合恒等（本机与远端两个出口读的是同一行；多一格 / 少一格 ⇒ 下游 join 那一层对不上）。
#[test]
fn c4d_the_row_has_exactly_these_keys() {
    let tmp = std::env::temp_dir().join(format!("ccm-c4d-keys-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-keys");
    let p = write_jsonl(
        &dir,
        "k.jsonl",
        &[r#"{"type":"user","message":{"content":"x"}}"#],
    );
    let v = analyze_session(&p);
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "aiTitle",
            "cwd",
            "firstUserExcerpt",
            "forkedFromMessageUuid",
            "forkedFromSessionId",
            "isBg",
            "jsonlPath",
            "messageCountApprox",
            "sessionId",
            "startedAtMs",
            "updatedAtMs",
        ]
    );
    std::fs::remove_dir_all(&tmp).ok();
}

// ── 按路径读会话的围栏也认各家合成历史面给的记录根 ─────────────────────
//
// 守的要求：「历史 / 账号 / tmux / MCP 四个面，本机与远端走同一条代码路径」——
// 本机冷读也改走后端的 `history-read` 之后，历史清单列得出的 Codex 会话必须经同一道围栏打得开。

fn loc1b_tree(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let tmp = std::env::temp_dir().join(format!("ccm-loc1b-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&tmp).ok();
    let home = tmp.join("claude");
    fixture_project(&home, "p");
    let codex_root = tmp.join("codex").join("sessions");
    std::fs::create_dir_all(codex_root.join("2026")).unwrap();
    (tmp, home, codex_root)
}

/// 两向：另外那几个根下的会话 ⇒ 放行；两类根都不在 ⇒ 拒（回 `projects/` 那一句）。
#[test]
fn loc1b_the_fence_admits_a_registered_history_root_and_nothing_else() {
    let (tmp, home, codex_root) = loc1b_tree("fence");
    let inside = write_jsonl(&codex_root.join("2026"), "rollout-x.jsonl", &["{}"]);
    let outside_dir = tmp.join("elsewhere");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let outside = write_jsonl(&outside_dir, "rollout-y.jsonl", &["{}"]);
    let roots = vec![codex_root.clone()];

    let ok = validate_session_path_among(&home, &roots, inside.to_str().unwrap());
    assert_eq!(ok.unwrap(), inside.canonicalize().unwrap());

    let refused = validate_session_path_among(&home, &roots, outside.to_str().unwrap());
    assert!(
        refused
            .as_ref()
            .is_err_and(|e| e.contains("outside projects dir")),
        "{refused:?}"
    );
    // 正控：不给另外那个根 ⇒ 同一份也拒（放行确实来自那个根，不是围栏本来就漏）。
    assert!(validate_session_path_among(&home, &[], inside.to_str().unwrap()).is_err());
    std::fs::remove_dir_all(&tmp).ok();
}

/// 另外那几个根不收相对路径（相对路径的意思只在 `projects/` 下有定义）；非 `.jsonl` 照拒；符号链接逃出根照拒。
#[test]
fn loc1b_the_extra_roots_take_absolute_jsonl_paths_that_stay_inside() {
    let (tmp, home, codex_root) = loc1b_tree("shape");
    let roots = vec![codex_root.clone()];
    write_jsonl(&codex_root, "rel.jsonl", &["{}"]);
    assert!(validate_session_path_among(&home, &roots, "rel.jsonl").is_err());

    let txt = write_jsonl(&codex_root, "notes.txt", &["x"]);
    let e = validate_session_path_among(&home, &roots, txt.to_str().unwrap()).unwrap_err();
    assert!(e.contains("non-jsonl"), "{e}");

    #[cfg(unix)]
    {
        let secret_dir = tmp.join("secret");
        std::fs::create_dir_all(&secret_dir).unwrap();
        let secret = write_jsonl(&secret_dir, "s.jsonl", &["{}"]);
        let link = codex_root.join("link.jsonl");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert!(validate_session_path_among(&home, &roots, link.to_str().unwrap()).is_err());
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 要求：「本机没有 projects 目录 ⇒ 整页加载失败，原话 read_dir … (os error 3)」·
/// 记录树根不在 ⇒ 零个项目、`Ok`（不是失败）；根在但读不了 ⇒ 说人话（期望取自文案表那一条，不含 `read_dir` / `os error`）。
#[test]
fn a_machine_without_a_projects_dir_lists_nothing_and_an_unreadable_one_says_so_plainly() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-noproj-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut out = Vec::new();
    assert_eq!(list_projects_into(&tmp, &mut out), Ok(false));
    assert!(out.is_empty(), "没有记录树却列出了东西");
    // 平铺清单当零个项目（不失败）；CLI 照旧出声 rc=2（S6-Z3），话是人话。
    assert_eq!(sessions_by_dir(&tmp), Ok(None));
    assert_eq!(run(&tmp, &["--list-projects".to_string()]), 2);
    // CLI 那一声带结构化的码（问它的那台后端认码画空态，不认话）。
    assert_eq!(
        list_projects_to(&tmp, &mut out).map_err(|(code, _)| code),
        Err(Some(NO_RECORD_TREE))
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let root = projects_root(&tmp);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000)).unwrap();
        // root 跑测试时权限位拦不住读 —— 那一格判不了，如实跳过而不是假绿（同 `tasks_query_tests`）。
        let perms_bite = std::fs::File::open(&root).is_err();
        let got = list_projects_into(&tmp, &mut out);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        if perms_bite {
            let e = got.expect_err("读不了的记录树却列成功了");
            let path = root.display().to_string();
            assert_eq!(e, copy_text("beHistory.dir.denied", &[("path", &path)]));
            assert!(!e.contains("read_dir") && !e.contains("os error"), "{e}");
        }
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #1/#2〕**扫清单那一行不复制用不上的大块**：一条记录里一段近 4 MiB 的工具输出，
/// 扫这一份时本线程未释放字节的高水位，不许比「只是逐行读一遍这份文件」高出一截 ——
/// 逐行解成完整 `Value` 的样子是行缓冲之外再复制一份那 4 MiB。
/// 第一行带 `cwd`：项目目录只读开头、读到第一个 `cwd` 就停（真记录第一行就有），不让它去读那一大行。
#[test]
fn listing_scan_does_not_copy_records_it_does_not_need() {
    use std::io::BufRead;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-alloc-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-alloc");
    let pad = "x".repeat(4 << 20);
    let big = format!(
        r#"{{"type":"user","timestamp":"2026-10-01T08:00:00Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{pad}"}}]}}}}"#
    );
    let lines = [
        r#"{"type":"user","cwd":"/w/alloc","timestamp":"2026-10-01T07:59:00Z","message":{"role":"user","content":"第一句"}}"#,
        big.as_str(),
        r#"{"type":"ai-title","aiTitle":"标题"}"#,
    ];
    let p = write_jsonl(&dir, "s-alloc.jsonl", &lines);
    // 地板：只逐行读一遍（行缓冲按倍数长，长多少由读法定，不由解析定）。
    let base = crate::alloc_probe::reset_peak();
    for l in std::io::BufReader::new(std::fs::File::open(&p).unwrap())
        .lines()
        .map_while(Result::ok)
    {
        std::hint::black_box(l);
    }
    let floor = crate::alloc_probe::peak_since(base);
    let base = crate::alloc_probe::reset_peak();
    let v = analyze_session(&p);
    let grew = crate::alloc_probe::peak_since(base);
    // 反空真：真扫到了这一份（三行都数到、标题与第一句都取到），地板真读到了那一大行。
    assert_eq!(v["messageCountApprox"], 3);
    assert_eq!(v["aiTitle"], "标题");
    assert_eq!(v["firstUserExcerpt"], "第一句");
    assert!(floor >= big.len(), "地板 {floor} 字节没读到那一大行");
    assert!(
        grew < floor + (256 << 10),
        "扫一份带 {} 字节记录的会话，高水位 {grew} 字节，比只逐行读一遍（{floor}）高出一截 —— 用不上的那一大段被复制了一份",
        big.len()
    );
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #1/#2〕扫清单的口径在怪形状上不变：每一格照「整行解成 JSON、按字段取串」那一套取。
/// 非串的 `type` / 标题 / 时刻当没有 · 同名键取最后一个 · 顶层不是对象只数行 · 坏行只数行 · 分叉来源两格都得是串。
#[test]
fn listing_scan_reads_odd_shapes_the_same_way() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-odd-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-odd");
    let p = write_jsonl(
        &dir,
        "s-odd.jsonl",
        &[
            r#"[1,2,3]"#,
            r#""just a string""#,
            r#"{not json"#,
            r#"{"type":7,"aiTitle":"类型不是串，不算标题"}"#,
            r#"{"type":"ai-title","aiTitle":{"x":1}}"#,
            r#"{"type":"user","timestamp":12345,"message":{"role":"user","content":"时刻不是串"}}"#,
            r#"{"type":"assistant","forkedFrom":{"sessionId":"src","messageUuid":9},"timestamp":"2026-10-01T08:00:00Z"}"#,
            r#"{"type":"assistant","forkedFrom":"bad","sessionKind":["bg"]}"#,
            r#"{"type":"user","type":"ai-title","aiTitle":"同名键取最后一个"}"#,
            "\u{feff}{\"type\":\"user\",\"forkedFrom\":{\"sessionId\":\"src2\",\"messageUuid\":\"m2\"},\"sessionKind\":\"bg\"}",
        ],
    );
    let v = analyze_session(&p);
    assert_eq!(v["messageCountApprox"], 10);
    assert_eq!(v["aiTitle"], "同名键取最后一个");
    assert_eq!(v["firstUserExcerpt"], "时刻不是串");
    // 第一条 user/assistant 的时刻不是串 ⇒ 取下一条带串的那一条。
    assert_eq!(
        v["startedAtMs"],
        crate::common::time::parse_iso8601_ms("2026-10-01T08:00:00Z").unwrap()
    );
    assert_eq!(v["forkedFromSessionId"], "src2");
    assert_eq!(v["forkedFromMessageUuid"], "m2");
    assert_eq!(v["isBg"], true);
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #3〕**正在写的会话变长，清单那一行只扫新增的那一段**，答案与整份重扫逐字相同；
/// 被截短 · 同长被改写 · 变长但前面被改写（见证对不上）· 半行写完 ⇒ 照样与整份重扫相同（退回整份扫）。
#[test]
fn a_growing_session_is_rescanned_only_for_what_was_appended() {
    use std::io::Write as _;
    let tmp = std::env::temp_dir().join(format!("ccm-hq-grow-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-grow");
    let p = dir.join("s-grow.jsonl");
    let rec = |i: usize| {
        format!(
            "{{\"type\":\"user\",\"cwd\":\"/w/grow\",\"timestamp\":\"2026-10-01T08:{:02}:00Z\",\"message\":{{\"role\":\"user\",\"content\":\"第 {i} 句 {}\"}}}}\n",
            i % 60,
            "填".repeat(2000)
        )
    };
    let body: String = (0..200).map(rec).collect();
    std::fs::write(&p, &body).unwrap();
    let bump = |secs: u64| {
        let f = std::fs::File::options().write(true).open(&p).unwrap();
        f.set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_900_000_000 + secs),
        )
        .unwrap();
    };
    bump(0);
    let last_bytes = || {
        SESSION_META
            .lock()
            .unwrap()
            .get(&p)
            .map(|e| e.last_bytes)
            .expect("清单缓存里有这一份")
    };
    let same = |what: &str| {
        let got = analyze_session_cached(&p);
        assert_eq!(
            got,
            analyze_session(&p),
            "{what}：缓存那一行与整份重扫不一样"
        );
        got
    };
    same("第一次");
    assert!(last_bytes() >= body.len() as u64, "第一次就该整份扫");
    // 追加一条：只读尾巴（见证 ＋ 新的那一行）。
    let one = rec(200);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(one.as_bytes())
        .unwrap();
    bump(1);
    let v = same("追加一条之后");
    assert_eq!(v["messageCountApprox"], 201);
    assert!(
        last_bytes() < (one.len() + 1024) as u64,
        "追加一条之后读了 {} 字节（整份 {}）—— 整份重扫了",
        last_bytes(),
        body.len() + one.len()
    );
    // 半行：先写一半（没有换行），再写完。
    let half = rec(201);
    let (a, b) = half.as_bytes().split_at(40); // 切在 ASCII 那一段：半行本身是合法 UTF-8（数作一行，与整份逐行读同）
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(a)
        .unwrap();
    bump(2);
    assert_eq!(same("写了半行")["messageCountApprox"], 202);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(b)
        .unwrap();
    bump(3);
    assert_eq!(same("半行写完")["messageCountApprox"], 202);
    // 截短：退回整份。
    std::fs::write(&p, (0..50).map(rec).collect::<String>()).unwrap();
    bump(4);
    assert_eq!(same("截短之后")["messageCountApprox"], 50);
    // 同长改写：标题换了。
    let mut text: String = (0..49).map(rec).collect();
    let bare = "{\"type\":\"ai-title\",\"aiTitle\":\"新标题\"}\n".len();
    let title = format!(
        "{{\"type\":\"ai-title\",\"aiTitle\":\"新标题\"{}}}\n",
        " ".repeat(rec(49).len() - bare)
    );
    assert_eq!(
        text.len() + title.len(),
        (0..50).map(rec).collect::<String>().len()
    );
    text.push_str(&title);
    std::fs::write(&p, &text).unwrap();
    bump(5);
    assert_eq!(same("同长改写之后")["aiTitle"], "新标题");
    // 变长、但前面被改写（见证对不上）：第一句换了。
    let mut text2 = rec(0).replace("第 0 句", "改过的第一句");
    text2.push_str(&(1..80).map(rec).collect::<String>());
    std::fs::write(&p, &text2).unwrap();
    bump(6);
    let v = same("变长但前面被改写");
    assert!(v["firstUserExcerpt"]
        .as_str()
        .unwrap()
        .starts_with("改过的第一句"));
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #4〕后台热缓存：热完之后整台清单一个字节都不再读（每份都是「没变」那一形），答案与现扫相同。
#[test]
fn warming_the_listing_leaves_nothing_to_rescan() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-warm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let mut paths = Vec::new();
    for d in 0..6 {
        let dir = fixture_project(&tmp, &format!("proj-warm-{d}"));
        for k in 0..4 {
            let line = format!(
                r#"{{"type":"user","cwd":"/w/warm{d}","message":{{"role":"user","content":"第 {k} 份"}}}}"#
            );
            paths.push(write_jsonl(
                &dir,
                &format!("s-warm-{d}-{k}.jsonl"),
                &[line.as_str()],
            ));
        }
    }
    let n = warm_listing(&tmp);
    assert!(n >= paths.len(), "热完缓存里只有 {n} 份");
    let tree = sessions_by_dir(&tmp).unwrap().unwrap();
    assert_eq!(
        tree.iter()
            .map(|(_, r)| r.as_ref().map_or(0, Vec::len))
            .sum::<usize>(),
        paths.len()
    );
    for p in &paths {
        let read = SESSION_META.lock().unwrap().get(p).map(|e| e.last_bytes);
        assert_eq!(read, Some(0), "热过之后再问清单，{} 又读了", p.display());
        assert_eq!(analyze_session_cached(p), analyze_session(p));
    }
    std::fs::remove_dir_all(&tmp).ok();
}

/// 〔perfC #4〕接线：`main.rs` 起来就热清单缓存，恰好一处（按行为量要起真二进制再从外面看缓存）。
#[test]
fn main_warms_the_listing_once() {
    let main = crate::guard_support::production_code(include_str!("../../../src/backend/main.rs"));
    guard_core::find_pinned(
        &main,
        "observe::history_query::warm_listing_in_background(agent_home.clone());",
    )
    .unwrap_or_else(|e| panic!("`main.rs` 里热清单缓存不是恰好一处：{e}"));
}

/// 〔perfC #4〕后台那两条热缓存的线程真降了优先级（Linux：这条线程的 nice 值），而且只降它自己。
#[cfg(target_os = "linux")]
#[test]
fn background_warmers_run_at_low_priority() {
    let nice_of_this_thread = || {
        let s = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        let rest: Vec<&str> = s[s.rfind(')').unwrap() + 2..].split(' ').collect();
        rest[16].parse::<i64>().unwrap()
    };
    let before = nice_of_this_thread();
    let (ok, after) = std::thread::spawn(move || {
        let ok = crate::platform::proc::lower_this_thread();
        (ok, nice_of_this_thread())
    })
    .join()
    .unwrap();
    assert!(ok, "降不了优先级");
    assert_eq!(
        after,
        i64::from(crate::platform::proc::BACKGROUND_NICE).max(before)
    );
    assert_eq!(nice_of_this_thread(), before, "降到了别的线程身上");
    for (file, src) in [
        (
            "observe/history_query.rs",
            include_str!("../../../src/backend/observe/history_query.rs"),
        ),
        (
            "observe/search_query.rs",
            include_str!("../../../src/backend/observe/search_query.rs"),
        ),
    ] {
        let src = crate::guard_support::production_code(src);
        assert!(
            src.contains("crate::platform::proc::lower_this_thread();"),
            "{file} 的后台热缓存线程没降优先级"
        );
    }
}
