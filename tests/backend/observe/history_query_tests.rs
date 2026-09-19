use super::*;

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

#[test]
fn truncate_is_char_safe() {
    assert_eq!(truncate_chars("中文字符串", 3), "中文字");
    assert_eq!(truncate_chars("ab", 120), "ab");
}

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

/// ★ 围栏只许有一处〔audit-0805 08-06，定框 E3〕。
///
/// # 它钉的不是「拒绝逃逸」，是「**只有一个地方在判逃逸**」
///
/// 上面那几条各自断言某条读路会 `is_err()` —— 那是**行为**。
/// 但行为对了不等于结构对了：此前 `list_sessions` 有一份**内联副本**，
/// 注释逐字写着「与 `read_session` 对齐」，也就是**靠手工对齐的两份**。
/// 两份都能通过各自的行为判据，而**强化其中一份时另一份不会跟** ——
/// 那正是本区 E3 反复要挡的形状。
///
/// 判准取「`canonicalize()` 在生产段出现在几处」：它是这套围栏的**核心动作**，
/// 收成一处之后，任何新写的「自己解析一下路径再判」都会让这个数变大。
#[test]
fn path_resolution_has_exactly_one_home() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/history_query.rs"
    ));
    let n = prod.matches("canonicalize()").count();
    // 抽取器自检：剥过头 / 抠不到 ⇒ 下面那条会零命中地绿。
    assert!(
        prod.len() > 3_000,
        "剥完生产段只剩 {} 字节 —— 剥法坏了，本条此刻无效",
        prod.len()
    );
    assert_eq!(
        n, 2,
        "`canonicalize()` 在生产段出现了 {n} 处（应恰好 2：`fence_under_projects` 里\n\
             一次解析 root、一次解析目标）。\n\
             多出来 = 又有人自己解析了一遍路径 —— 那就是第二份围栏，\n\
             它今天可能与 `fence_under_projects` 等价，但**强化一边时另一边不会跟**（E3）。\n\
             少了 = 围栏被简化了，去看它是不是还挡得住 symlink 逃逸。"
    );
    // 反向锚点：那两处确实在围栏函数里，不是散落在别处凑够了数。
    let f = prod
        .find("fn fence_under_projects")
        .expect("找不到围栏函数 —— 上面那个计数就失去了意义");
    // ⚠ **本文件里不许写「只含右大括号、没有左大括号」的字符串或注释**：
    // 括号配平扫描面里，一个不配对的大括号会把它的剥法提前收尾，
    // 于是测试段泄进「生产段」——第一版就是这么写的，当场把两条判据打红。
    // 改用「下一个顶层 fn」当边界，绕开大括号。
    let body_end = prod[f..].find("\nfn ").map_or(prod.len(), |k| f + k);
    assert_eq!(
        prod[f..body_end].matches("canonicalize()").count(),
        2,
        "两处 `canonicalize()` 不在 `fence_under_projects` 里 —— 计数凑对了，位置没对"
    );
}

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

/// daemon-02：offset 续拉的字节语义**逐字节对拍** watcher 发出的 `byte_offset`
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

/// daemon-02：offset 续拉沿用 `read_session` 的路径守卫（projects 外 / 非 jsonl 拒）。
#[test]
fn read_session_from_offset_path_guard() {
    let tmp = std::env::temp_dir().join(format!("ccm-hq-off-{}", std::process::id()));
    let dir = fixture_project(&tmp, "proj-off");
    write_jsonl(&dir, "ok.jsonl", &[r#"{"type":"user"}"#]);
    let outside = tmp.join("secret.jsonl");
    std::fs::write(&outside, "nope").unwrap();
    // projects 外 → 拒（守卫先于 seek）。
    assert!(read_session_from_offset(&tmp, &outside.to_string_lossy(), 0).is_err());
    // 合法 jsonl + offset 超长 → seek 过 EOF 读空、Ok（不 panic、不报错）。
    let ok = dir.join("ok.jsonl");
    assert!(read_session_from_offset(&tmp, &ok.to_string_lossy(), 9999).is_ok());
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
        let n = stream_from_offset(&mut f, off, &mut got).expect("stream ok");
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
