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

// ═════════════════════════════════════════════════════════════════════════════
// `设计/60 §5.4c`：`sftp_chmod` 发上线的那个 `SETSTAT` 包，**不许带 size**
// ═════════════════════════════════════════════════════════════════════════════
//
// 🔴 **它治的是本仓真机上被咬过的那一口，不是一条假想的风险。**
//
// `sftp.rs::upload_atomic` 的尾注逐字记着：「在 OpenSSH sftp-server 上 setstat
// （即便只设 permissions、`size=None`）会把刚 rename 好的文件**截断成 0 字节**」——
// 后端因此变 0 字节不可 exec → 连接 EOF → marker 变空 → 无限重部署。
// 那条注释因此逐字禁掉了「rename 之后 `set_metadata` 兜底 chmod」这一整个动作。
//
// 而 `设计/60 §5.4c` 裁定 `sftp_chmod` 时逐字写的是「协议侧没有障碍
// （`russh_sftp::client::SftpSession::set_metadata` 现成）」—— **那句话没有提到这一口**。
// ⇒ 落这条命令之前把它读到底（现打 `russh-sftp` 3.0.0 那份属性块序列化器；
// ⚠ 住址刻意不写成「文件::符号」那一形 —— 它在依赖树里不在本仓，写成那一形会被
//   `structural_scan` 当成一处本仓符号地址）：`SSH_FILEXFER_ATTR_SIZE`（`0x1`）
// 这个标志位**只在 `size.is_some()` 时才置**，`size` 字段也只在那时才写进包里。
// ⇒ 只要 `size` 是 `None`，线上那个包里既没有 SIZE 标志也没有 size 字段。
//
// ## 判法：**逐字节相等**，不是「不含某个子串」
//
// 序列化整个属性块，把**字节**与手写的期望逐字节比。这样三种改法都会红：
// ① 有人给 `chmod_attrs` 补一个 `size: Some(..)`（那正是事故成因）；
// ② 有人顺手加 uid/gid/atime/mtime（包变长、标志位变）；
// ③ `russh-sftp` 升版改了线上编码（那时该回来重读一遍这一口，而不是静默放行）。
//
// ## ⚠ 它买不到什么（如实登记，不假装覆盖）
//
// 它买的是「**线上那个包的形状**」。它**买不到**「真机上 OpenSSH 收到这个包不会截断」——
// 那要一趟真机，本仓今天没有（`sftp.rs` 那条注释所依据的 e2e 是当时跑的，今天复现不了）。
// ⇒ 本条排除的是**本仓那次事故的成因**（把 size 一起送上去），不是一个更大的声称。

#[test]
fn the_chmod_attrs_never_put_a_size_on_the_wire() {
    let wire = russh_sftp::ser::to_bytes(&chmod_attrs(0o644)).expect("属性块序列化不出来");
    // 期望的那 8 个字节，逐字写出来（不从被测对象那边算，否则就是它跟自己一致）：
    //   前 4 字节 = attrs 标志位（大端 u32）= `SSH_FILEXFER_ATTR_PERMISSIONS`（0x4）**且仅有它**；
    //   后 4 字节 = permissions（大端 u32）= 0o644 = 0x1A4。
    let want: Vec<u8> = vec![0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x01, 0xA4];
    assert_eq!(
        wire.to_vec(),
        want,
        "`chmod_attrs` 发上线的 SETSTAT 属性块变了。\n\
         ★ **最贵的那一种变法是多一个 `size`**：本仓真机 e2e 实证过，OpenSSH sftp-server\n\
           收到带 size 的 setstat 会把文件**截断成 0 字节**（`sftp.rs::upload_atomic` 尾注）。\n\
           那一口咬掉的是远端后端二进制，症状是「无限重部署」，而不是一句报错。\n\
         ⇒ 如果是 `russh-sftp` 升版改了编码，回去重读一遍那一口再改这里的期望值；\n\
           如果是有人给属性块加了字段，先回答「面板改权限为什么要动那个字段」。"
    );
    // 反向自检：把 size 放进去，线上那个包**一定**不一样 —— 否则上面那条对
    // 「多一个 size」这一形是瞎的（而那正是它唯一真正在防的东西）。
    let with_size = russh_sftp::protocol::FileAttributes {
        size: Some(0),
        permissions: Some(0o644),
        ..Default::default()
    };
    let poisoned = russh_sftp::ser::to_bytes(&with_size).expect("属性块序列化不出来");
    assert_ne!(
        poisoned.to_vec(),
        want,
        "带 size 的属性块与不带的序列化成了同一串字节 —— 那上面那条相等断言对\n\
         「有人补了一个 size」这一形是瞎的，本条此刻在空转"
    );
}

/// `mode` 的高位（文件类型位）不许上线 —— 面板改权限不该能改文件类型。
#[test]
fn the_chmod_mode_is_masked_down_to_permission_bits() {
    // `FileMode::REG`（0x8000）＋ 0o644：掩完必须只剩 0o644。
    assert_eq!(
        chmod_attrs(0o100644).permissions,
        Some(0o644),
        "文件类型位漏上线了 —— 那不是 chmod 的语义"
    );
    // 粘滞位/setuid 那一档（0o7000）**是** chmod 的语义，不许被掩掉。
    assert_eq!(
        chmod_attrs(0o4755).permissions,
        Some(0o4755),
        "setuid/setgid/sticky 被掩掉了 —— 掩码收得过紧"
    );
}
