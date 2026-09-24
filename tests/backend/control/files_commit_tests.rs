//! 〔F7c · 第三波 · 2026-09-24〕`control/files_commit.rs` 的行为判据 —— **上传的提交**。
//!
//! 全部在本机临时目录上真跑（一个假的 `home` ＋ 一个目标根），不是源码扫描。
//!
//! # 买到什么
//!
//! - 提交**先过围栏**：目标是一份会话文件 / 带上跳段 ⇒ 拒，而且盘上零新增、暂存件原样留着
//!   （把提交改成「直接改名到拼出来的路径」⇒ 这里当场红，见 `设计/60 §13.6` 判据 3）；
//! - 两支各自的语义：不覆盖 ⇒ 目标已在就拒、目标**一个字节没动**；覆盖 ⇒ 整份换掉；
//! - 暂存件的路径**调用方指不到**：键不合法 ⇒ 拒（`../` · 大写 · 长度不对）；
//! - 暂存件被**消耗**（改名，不是复制）：提交成功后暂存区里那一份没了 —— 暂存区清理的「传完」那一格。
//!
//! # 买不到什么
//!
//! - 真远端（同写面那一族：全在本机文件系统上）；跨盘（`EXDEV`）那一支本机造不出来 —— 没判。

use super::*;

/// 一个本轮独占的临时目录（`tag` 区分用例，`pid` 区分并发跑的进程）。
fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-fc-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

/// 一个假 home（带好暂存区）＋ 一个目标根。回 `(home, root)`。
fn rig(tag: &str) -> (PathBuf, PathBuf) {
    let base = temp_dir(tag);
    let home = base.join("home");
    let root = base.join("root");
    std::fs::create_dir_all(home.join(STAGING_DIR)).expect("建暂存区");
    std::fs::create_dir_all(&root).expect("建目标根");
    (home, root)
}

const KEY: &str = "0123456789abcdef0123456789abcdef";

/// 在暂存区里摆一份「传完了」的件。
fn stage(home: &Path, key: &str, bytes: &[u8]) -> PathBuf {
    let p = home.join(STAGING_DIR).join(format!("{key}{PART_SUFFIX}"));
    std::fs::write(&p, bytes).expect("摆暂存件");
    p
}

/// ★ 键的判定两个方向都答（只验「该拒的拒了」是空真的一半）。
#[test]
fn the_key_is_exactly_32_lowercase_hex_both_ways() {
    assert!(is_key(KEY));
    assert!(is_key(&"f".repeat(KEY_LEN)));
    for bad in [
        "",
        "0123456789abcdef",                    // 短
        "0123456789abcdef0123456789abcdef0",   // 长
        "0123456789ABCDEF0123456789ABCDEF",    // 大写
        "../../../../etc/passwd/xxxxxxxxxxxx", // 上跳
        "0123456789abcdef0123456789abcde/",    // 分隔符
        "0123456789abcdef0123456789abcdeg",    // 非十六进制
    ] {
        assert!(!is_key(bad), "该拒的键放过了：{bad:?}");
    }
}

/// ★ 不覆盖 ⇒ 目标原来不在：落进去、字节逐一相同、暂存件被**消耗**（不是复制）。
#[test]
fn commit_without_overwrite_lands_the_staged_bytes_and_consumes_them() {
    let (home, root) = rig("land");
    let body = b"\x00staged\xffbytes\n".to_vec();
    let staged = stage(&home, KEY, &body);
    let (landed, n) = commit_upload(&home, KEY, &root, "a.bin", false).expect("该落进去");
    assert_eq!(n, body.len() as u64);
    assert_eq!(std::fs::read(&landed).expect("读落点"), body);
    assert!(
        !staged.exists(),
        "暂存件还在 —— 提交成了复制，暂存区「传完」那一格的清理没发生"
    );
}

/// ★ 不覆盖 ⇒ 目标已在：拒，而且**目标一个字节没动**、暂存件原样留着（可以换个名字再提交）。
#[test]
fn commit_without_overwrite_refuses_an_existing_target_and_touches_nothing() {
    let (home, root) = rig("clash");
    let staged = stage(&home, KEY, b"new");
    std::fs::write(root.join("a.txt"), b"old").unwrap();
    let e = commit_upload(&home, KEY, &root, "a.txt", false).expect_err("该拒");
    assert_eq!(
        e.code(),
        "io_failed",
        "目标已在是盘上的事实，不是围栏：{}",
        e.message()
    );
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"old");
    assert_eq!(std::fs::read(&staged).unwrap(), b"new", "暂存件被动了");
    // 换个名字再提交 ⇒ 成（暂存件还在，就是为这一步留的）。
    commit_upload(&home, KEY, &root, "b.txt", false).expect("换名再提交该成");
    assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"new");
}

/// ★ 覆盖 ⇒ 整份换掉；目标原来不在也照样落（「许覆盖」不等于「必须在」）。
#[test]
fn commit_with_overwrite_replaces_the_target_whole() {
    let (home, root) = rig("over");
    stage(&home, KEY, b"new-content");
    std::fs::write(root.join("a.txt"), b"old-and-longer-content").unwrap();
    commit_upload(&home, KEY, &root, "a.txt", true).expect("该换掉");
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"new-content");
    stage(&home, KEY, b"fresh");
    commit_upload(&home, KEY, &root, "c.txt", true).expect("目标不在也该落");
    assert_eq!(std::fs::read(root.join("c.txt")).unwrap(), b"fresh");
}

/// 🔴🔴 **提交先过围栏**：目标是一份会话文件 / 带上跳段 ⇒ 拒；盘上零新增；暂存件原样。
///
/// 这一条就是 `设计/60 §13.6` 判据 3 的行为那一半：把 `commit_upload` 里那句
/// 围栏换成「直接拼 `root.join(rel)`」⇒ 会话文件那一格会真的被写进去 ⇒ 红。
#[test]
fn commit_goes_through_the_fence_and_leaves_the_disk_alone_when_refused() {
    let (home, base) = rig("fence");
    // 目标根本身合法地落在 `~/.claude/projects/-x` 底下（用户 09-23 裁「可以」），
    // 而写点拼出来恰好是一份会话文件（`projects/` 下恰 2 段的 jsonl）。
    let proj = base.join(".claude").join("projects").join("-x");
    std::fs::create_dir_all(&proj).unwrap();
    let staged = stage(&home, KEY, b"would clobber a session");
    // ⚠ 「盘上零新增」逐条看**那一次若没被拦会落到哪**（不列目录：扫描型判据不许在测试段裸遍历）。
    for (root, rel, would_land) in [
        (proj.as_path(), "abc.jsonl", proj.join("abc.jsonl")),
        (
            base.as_path(),
            "../escape.bin",
            base.parent().expect("临时目录有上一级").join("escape.bin"),
        ),
        (base.as_path(), "/abs.bin", PathBuf::from("/abs.bin")),
    ] {
        let e = commit_upload(&home, KEY, root, rel, true).expect_err("该被围栏拒");
        assert_eq!(e.code(), "refused", "`{rel}` 不是围栏拒的：{}", e.message());
        assert!(
            !would_land.exists(),
            "`{rel}` 被拒了，{} 却落了东西",
            would_land.display()
        );
    }
    assert!(!proj.join("abc.jsonl").exists());
    assert_eq!(std::fs::read(&staged).unwrap(), b"would clobber a session");
}

/// ★ 暂存件不在 ⇒ `io_failed`；暂存件是一条链接 ⇒ `refused`（不许挪走链接指向之外的东西）。
#[test]
fn a_missing_or_linked_staged_file_is_refused() {
    let (home, root) = rig("missing");
    let e = commit_upload(&home, KEY, &root, "a.bin", false).expect_err("没有暂存件");
    assert_eq!(e.code(), "io_failed", "{}", e.message());
    assert!(
        !root.join("a.bin").exists(),
        "暂存件不在，目标上却留了一个占位"
    );
    #[cfg(unix)]
    {
        let secret = root.join("secret.txt");
        std::fs::write(&secret, b"not yours").unwrap();
        std::os::unix::fs::symlink(
            &secret,
            home.join(STAGING_DIR).join(format!("{KEY}{PART_SUFFIX}")),
        )
        .unwrap();
        let e = commit_upload(&home, KEY, &root, "b.bin", true).expect_err("链接当源");
        assert_eq!(e.code(), "refused", "{}", e.message());
        assert!(!root.join("b.bin").exists());
        assert_eq!(std::fs::read(&secret).unwrap(), b"not yours");
    }
}

/// ★ 键不合法 ⇒ 拒，而且拒在碰盘之前（目标上连占位都没有）。
#[test]
fn a_bad_key_is_refused_before_anything_lands() {
    let (home, root) = rig("badkey");
    for bad in ["../../x", "ABCDEF0123456789ABCDEF0123456789", "short"] {
        let e = commit_upload(&home, bad, &root, "a.bin", false).expect_err("该拒");
        assert_eq!(e.code(), "refused", "{bad:?}：{}", e.message());
    }
    assert!(
        !root.join("a.bin").exists(),
        "键不合法，目标上却落了东西（哪怕是一个占位）"
    );
}

/// ★ 线上那一面：覆盖策略**必须显式给**（不给 ⇒ `bad_args`，不默认成哪一边）；
/// 未知命令名 ⇒ `bad_args`；命令表与分派两向对得上。
#[test]
fn the_wire_face_requires_an_explicit_overwrite_and_knows_only_its_command() {
    let e = answer_wire(
        "files-commit-upload",
        &serde_json::json!({"key": KEY, "root": "/tmp", "rel": "a"}),
    )
    .expect_err("没给 overwrite 该拒");
    assert_eq!(e.0, "bad_args");
    assert!(e.1.contains("overwrite"), "{}", e.1);
    let e = answer_wire("files-create", &serde_json::json!({})).expect_err("不是这一面的");
    assert_eq!(e.0, "bad_args");
    assert_eq!(commit_command_names(), vec!["files-commit-upload"]);
}
