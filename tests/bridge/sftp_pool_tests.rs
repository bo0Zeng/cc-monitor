use super::*;

#[test]
fn guard_write_rejects_claude_data_allows_normal() {
    assert!(guard_write("/home/pi/.claude/projects/-x/s.jsonl").is_err());
    assert!(guard_write("/home/pi/.claude/sessions/1.json").is_err());
    assert!(guard_write("/home/pi/proj/main.rs").is_ok());
    assert!(guard_write("/home/pi/.claude/settings.json").is_ok()); // 非受保护
}

// F49：编辑护栏(数据安全红线)——拒编优于截断/乱码。
#[test]
fn decode_editable_guards() {
    assert_eq!(
        decode_editable(b"hello\nworld"),
        Some("hello\nworld".into())
    );
    assert_eq!(
        decode_editable("中文 UTF-8".as_bytes()).as_deref(),
        Some("中文 UTF-8")
    );
    assert_eq!(decode_editable(&[]), Some(String::new())); // 空文件可编辑
    assert_eq!(decode_editable(b"a\0b"), None); // 含 NUL → 疑二进制,拒编
    assert_eq!(decode_editable(&[0xff, 0xfe]), None); // 非 UTF-8,拒编
                                                      // >256KB → 拒编(不截断)
    assert_eq!(decode_editable(&vec![b'x'; MAX_EDIT_BYTES + 1]), None);
    assert!(decode_editable(&vec![b'x'; MAX_EDIT_BYTES]).is_some()); // 恰好上限可编辑
}

#[test]
fn protected_path_guard() {
    assert!(is_protected_claude_data_path(
        "/home/pi/.claude/projects/-x/abc.jsonl"
    ));
    assert!(is_protected_claude_data_path(
        "/home/u/.claude/sessions/123.json"
    ));
    // 普通用户文件不受守卫
    assert!(!is_protected_claude_data_path("/home/pi/proj/main.rs"));
    assert!(!is_protected_claude_data_path(
        "/home/pi/.claude/settings.json"
    )); // 非 sessions/ 下
    assert!(!is_protected_claude_data_path(
        "/home/pi/notclaude/projects/x.jsonl"
    )); // 非 /.claude/projects/
        // 反斜杠归一
    assert!(is_protected_claude_data_path(
        "C:\\Users\\me\\.claude\\projects\\p\\s.jsonl"
    ));
    // batch20 审计修：CLAUDE_CONFIG_DIR 重定位（.claude 挪到 ~/mydata）后仍受保护（结构判定，闭字面缺口）
    assert!(is_protected_claude_data_path(
        "/home/pi/mydata/projects/-x/abc.jsonl"
    ));
    assert!(is_protected_claude_data_path(
        "/home/u/mydata/sessions/123.json"
    ));
    // 但 projects 下只 1 段（非 <proj>/<sid>.jsonl 结构）不误伤普通文件
    assert!(!is_protected_claude_data_path(
        "/home/pi/x/projects/a.jsonl"
    ));
    // sessions 下再嵌目录（非 Claude 单层结构）不误伤
    assert!(!is_protected_claude_data_path("/x/sessions/sub/y.json"));
}

#[test]
fn lossy_name_detection() {
    assert!(is_lossy_name("bad\u{FFFD}name"));
    assert!(!is_lossy_name("good_name.txt"));
}

#[test]
fn dead_conn_classification() {
    assert!(looks_like_dead_conn("channel closed by peer"));
    assert!(looks_like_dead_conn("Broken pipe (os error 32)"));
    assert!(looks_like_dead_conn("connection reset"));
    assert!(!looks_like_dead_conn("No such file or directory"));
    assert!(!looks_like_dead_conn("permission denied"));
}

#[test]
fn list_dir_sort_dirs_first_then_lowercase() {
    // 直接测排序契约（不需真连接）。
    let mut v = vec![
        SftpEntry {
            name: "Zebra".into(),
            path: "/Zebra".into(),
            is_dir: false,
            is_symlink: false,
            size: 0,
            lossy_name: false,
        },
        SftpEntry {
            name: "apple".into(),
            path: "/apple".into(),
            is_dir: false,
            is_symlink: false,
            size: 0,
            lossy_name: false,
        },
        SftpEntry {
            name: "src".into(),
            path: "/src".into(),
            is_dir: true,
            is_symlink: false,
            size: 0,
            lossy_name: false,
        },
    ];
    sort_entries(&mut v); // 用生产比较器,改它测试即跟着变(不再假信心)
    assert_eq!(
        v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        vec!["src", "apple", "Zebra"]
    );
}

#[test]
fn cancel_guard_ptr_eq_no_cross_delete() {
    // D 审计 R2/S4:同 id 两次注册,第一个 guard drop 用 ptr_eq 不误删第二个的 flag。
    let (_f1, g1) = register_cancel("dup-id");
    let (f2, _g2) = register_cancel("dup-id"); // 覆盖 map 里的 flag = f2
    drop(g1); // g1 的 flag != map 现存(f2)→ ptr_eq 不成立→不删
    assert!(
        cancels().lock().unwrap().contains_key("dup-id"),
        "g1 drop 不该误删 f2 的注册项"
    );
    // f2 仍可被取消(标志翻转可达)
    f2.store(true, std::sync::atomic::Ordering::SeqCst);
    drop(_g2);
    assert!(
        !cancels().lock().unwrap().contains_key("dup-id"),
        "g2 drop 摘除自己的项"
    );
}
