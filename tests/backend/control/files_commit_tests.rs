//! `control/files_commit.rs` 的行为判据 —— **上传的提交**。
//!
//! 全部在本机临时目录上真跑（一个假的 `home` ＋ 一个目标根），不是源码扫描。
//!
//! # 买到什么
//!
//! - 提交**先过围栏**：目标是一份会话文件 / 带上跳段 ⇒ 拒，而且盘上零新增、暂存件原样留着
//!   （把提交改成「直接改名到拼出来的路径」⇒ 这里当场红，见）；
//! - 两支各自的语义：不覆盖 ⇒ 目标已在就拒、目标**一个字节没动**；覆盖 ⇒ 整份换掉；
//! - 暂存件的路径**调用方指不到**：键不合法 ⇒ 拒（`../` · 大写 · 长度不对）；
//! - 暂存件被**消耗**（改名，不是复制）：提交成功后暂存区里那一份没了 —— 暂存区清理的「传完」那一格。
//!
//! # 买不到什么
//!
//! - 真远端（同写面那一族：全在本机文件系统上）；跨盘（`EXDEV`）那一支本机造不出来 —— 没判。

use super::*;
use crate::control::files_write::overwrite_text;

/// 此刻暂存区里这个键那份暂存件的整份摘要（提交时的 `expect`）。没有 / 键不合法 ⇒ 全零串（那几条判的是别的拒）。
fn staged_sha(home: &Path, key: &str) -> String {
    staged_path(home, key)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .map(|b| content_sha256(&b))
        .unwrap_or_else(|| "0".repeat(crate::control::files_write::SHA256_HEX_LEN))
}

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
    let (landed, n) = commit_upload(&home, KEY, &root, "a.bin", false, &staged_sha(&home, KEY))
        .expect("该落进去");
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
    let e = commit_upload(&home, KEY, &root, "a.txt", false, &staged_sha(&home, KEY))
        .expect_err("该拒");
    assert_eq!(
        e.code(),
        "io_failed",
        "目标已在是盘上的事实，不是围栏：{}",
        e.message()
    );
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"old");
    assert_eq!(std::fs::read(&staged).unwrap(), b"new", "暂存件被动了");
    // 换个名字再提交 ⇒ 成（暂存件还在，就是为这一步留的）。
    commit_upload(&home, KEY, &root, "b.txt", false, &staged_sha(&home, KEY))
        .expect("换名再提交该成");
    assert_eq!(std::fs::read(root.join("b.txt")).unwrap(), b"new");
}

/// ★ 覆盖 ⇒ 整份换掉；目标原来不在也照样落（「许覆盖」不等于「必须在」）。
#[test]
fn commit_with_overwrite_replaces_the_target_whole() {
    let (home, root) = rig("over");
    stage(&home, KEY, b"new-content");
    std::fs::write(root.join("a.txt"), b"old-and-longer-content").unwrap();
    commit_upload(&home, KEY, &root, "a.txt", true, &staged_sha(&home, KEY)).expect("该换掉");
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"new-content");
    stage(&home, KEY, b"fresh");
    commit_upload(&home, KEY, &root, "c.txt", true, &staged_sha(&home, KEY))
        .expect("目标不在也该落");
    assert_eq!(std::fs::read(root.join("c.txt")).unwrap(), b"fresh");
}

/// 🔴🔴 **提交先过路径解析**：目标带上跳段 / 是绝对路径 ⇒ 拒；盘上零新增；暂存件原样。
///
/// 这一条是行为那一半：把 `commit_upload` 里那句
/// 路径解析换成「直接拼 `root.join(rel)`」⇒ 上跳那一格会真的落到根外 ⇒ 红。
/// 从前第一格是「目标是一份会话文件 ⇒ 拒」；用户「文件管理器全部都可以改. 不需要任何围栏」
/// ⇒ 那一格翻成正控（提交**落得进**会话文件那个位置），见本条末尾。
#[test]
fn commit_goes_through_the_fence_and_leaves_the_disk_alone_when_refused() {
    let (home, base) = rig("fence");
    let proj = base.join(".claude").join("projects").join("-x");
    std::fs::create_dir_all(&proj).unwrap();
    let staged = stage(&home, KEY, b"would clobber a session");
    // ⚠ 「盘上零新增」逐条看**那一次若没被拦会落到哪**（不列目录：扫描型判据不许在测试段裸遍历）。
    for (root, rel, would_land) in [
        (
            base.as_path(),
            "../escape.bin",
            base.parent().expect("临时目录有上一级").join("escape.bin"),
        ),
        (base.as_path(), "/abs.bin", PathBuf::from("/abs.bin")),
    ] {
        let e = commit_upload(&home, KEY, root, rel, true, &staged_sha(&home, KEY))
            .expect_err("该被围栏拒");
        assert_eq!(e.code(), "refused", "`{rel}` 不是围栏拒的：{}", e.message());
        assert!(
            !would_land.exists(),
            "`{rel}` 被拒了，{} 却落了东西",
            would_land.display()
        );
    }
    assert_eq!(std::fs::read(&staged).unwrap(), b"would clobber a session");
    // 正控：同一份暂存件提交到会话文件那个位置 ⇒ 落得进去。
    commit_upload(
        &home,
        KEY,
        &proj,
        "abc.jsonl",
        false,
        &staged_sha(&home, KEY),
    )
    .expect("🔴 提交到会话文件的位置被拒了");
    assert_eq!(
        std::fs::read(proj.join("abc.jsonl")).unwrap(),
        b"would clobber a session"
    );
}

/// ★ 暂存件不在 ⇒ `io_failed`；暂存件是一条链接 ⇒ `refused`（不许挪走链接指向之外的东西）。
#[test]
fn a_missing_or_linked_staged_file_is_refused() {
    let (home, root) = rig("missing");
    let e = commit_upload(&home, KEY, &root, "a.bin", false, &staged_sha(&home, KEY))
        .expect_err("没有暂存件");
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
        let e = commit_upload(&home, KEY, &root, "b.bin", true, &staged_sha(&home, KEY))
            .expect_err("链接当源");
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
        let e = commit_upload(&home, bad, &root, "a.bin", false, &staged_sha(&home, bad))
            .expect_err("该拒");
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
    assert_eq!(
        commit_command_names(),
        vec![
            "files-commit-upload",
            "files-stage-chunk",
            "files-commit-text"
        ]
    );
    // 表里每一条，分派都够得到（空参数 ⇒ 拒在参数那一关、码在它自己的登记里，不是「不认这条命令」那句）。
    for c in COMMIT_COMMANDS {
        let e = answer_wire(c.name, &serde_json::json!({})).expect_err("空参数该拒");
        assert!(
            ["bad_args", "bad_path"].contains(&e.0) && c.codes.contains(&e.0),
            "{}：{e:?}",
            c.name
        );
        assert!(
            !e.1.contains("不是上传提交那一面的命令"),
            "{} 分派不到：{}",
            c.name,
            e.1
        );
    }
}

// ═══ 孤儿扫（暂存区清理只靠事件 —— 这一格的事件是「一次提交成功」）═══

/// 把一份暂存件的修改时间拨到 `secs`（判据自己定时间，不等墙钟）。
fn set_mtime(p: &Path, secs: u64) {
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    std::fs::File::options()
        .write(true)
        .open(p)
        .expect("开暂存件")
        .set_modified(t)
        .expect("拨修改时间");
}

/// ★ 老的删；新的留；调用方手上那一份留（哪怕老）；不是我们形状的名字一个不碰。
#[test]
fn the_sweep_removes_only_stale_parts_of_our_own_shape() {
    let (home, _root) = rig("sweep");
    let now: u64 = 2_000_000_000;
    let old = now - STAGING_STALE_SECS - 1;
    let fresh = now - STAGING_STALE_SECS + 60;
    let k_old = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let k_fresh = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    for (k, t) in [(k_old, old), (k_fresh, fresh), (KEY, old)] {
        set_mtime(&stage(&home, k, b"z"), t);
    }
    let stranger = home.join(STAGING_DIR).join("notes.part");
    std::fs::write(&stranger, b"not ours").unwrap();
    set_mtime(&stranger, old);
    let gone = sweep_stale(&home, now, KEY);
    assert_eq!(gone, vec![format!("{k_old}{PART_SUFFIX}")]);
    let staged = |k: &str| home.join(STAGING_DIR).join(format!("{k}{PART_SUFFIX}"));
    assert!(!staged(k_old).exists());
    assert!(staged(k_fresh).exists(), "新的被扫了 —— 一趟正在传的会被删");
    assert!(staged(KEY).exists(), "调用方手上那一份被扫了");
    assert!(stranger.exists(), "不是我们形状的名字被动了");
}

/// ★ 暂存区里一条**指出去的链接**（名字是我们的形状、而且很老）不删 —— 删之前先过围栏。
#[cfg(unix)]
#[test]
fn the_sweep_never_follows_a_link_out_of_the_staging_area() {
    let (home, root) = rig("sweeplink");
    let outside = root.join("precious.txt");
    std::fs::write(&outside, b"keep me").unwrap();
    let link = home.join(STAGING_DIR).join(format!("{KEY}{PART_SUFFIX}"));
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let gone = sweep_stale(&home, u64::MAX / 2, "ffffffffffffffffffffffffffffffff");
    assert!(gone.is_empty(), "扫到了一条链接：{gone:?}");
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep me");
}

/// 🔴 **提交成功就是扫的那个事件**：经线上那一面提交一次 ⇒ 老孤儿没了。
///
/// ⚠ 从 `answer_commit_at` 进（家由判据给）：不改进程的 `HOME` —— 那是全进程共享的。
#[test]
fn a_successful_commit_is_the_event_that_sweeps() {
    let (home, root) = rig("sweepwire");
    let orphan = "cccccccccccccccccccccccccccccccc";
    set_mtime(&stage(&home, orphan, b"old"), 1);
    stage(&home, KEY, b"payload");
    answer_commit_at(
        &home,
        &serde_json::json!({
            "key": KEY,
            "root": root.to_string_lossy(),
            "rel": "landed.bin",
            "overwrite": false,
            "expect": {"sha256": content_sha256(b"payload")},
        }),
    )
    .expect("提交该成");
    assert_eq!(std::fs::read(root.join("landed.bin")).unwrap(), b"payload");
    assert!(
        !home
            .join(STAGING_DIR)
            .join(format!("{orphan}{PART_SUFFIX}"))
            .exists(),
        "提交成功了，老孤儿还在 —— 「提交」那个事件没接上扫"
    );
}

// ═══ 存盘的块：`files-stage-chunk` ＋ `files-commit-text` ═══

/// 一台还**没有**暂存区的 home ＋ 一个目标根（块那一条要自己建暂存区）。
fn bare_rig(tag: &str) -> (PathBuf, PathBuf) {
    let base = temp_dir(tag);
    let home = base.join("home");
    let root = base.join("root");
    std::fs::create_dir_all(&home).expect("建 home");
    std::fs::create_dir_all(&root).expect("建目标根");
    (home, root)
}

/// 经线上那一面送一块（`content` 走 `b16`：块里可以有任何字节）。
fn send_chunk(home: &Path, key: &str, seq: u64, bytes: &[u8]) -> Answer {
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    answer_stage_at(
        home,
        &serde_json::json!({ "key": key, "seq": seq, "content": { "b16": hex } }),
    )
}

/// 经线上那一面提交。
fn send_commit(home: &Path, key: &str, chunks: u64, bytes: u64, root: &Path, rel: &str) -> Answer {
    // CAS 必给：交「此刻盘上那一份」的摘要（读不到就交一个全零串 —— 那一形由判据自己看码）。
    let expect = std::fs::read(root.join(rel))
        .map(|b| content_sha256(&b))
        .unwrap_or_else(|_| "0".repeat(crate::control::files_write::SHA256_HEX_LEN));
    answer_commit_text_at(
        home,
        &serde_json::json!({
            "key": key, "chunks": chunks, "bytes": bytes,
            "root": root.to_string_lossy(), "rel": rel,
            "expect": {"sha256": expect},
        }),
    )
}

/// 暂存区里这个键还剩哪几块（按名字逐个问 —— 本族用例块号都在 0..16 里；
/// 不列目录：测试段裸遍历目录归 `scanning_guard_registry` 管，这里用不着）。
fn chunks_left(home: &Path, key: &str) -> Vec<String> {
    (0..16)
        .map(|seq| chunk_name(key, seq))
        .filter(|n| std::fs::symlink_metadata(home.join(STAGING_DIR).join(n)).is_ok())
        .collect()
}

/// 一段形状不平凡的合成字节（每块长度不同、含 0 与高位字节）。
fn body(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// ★ 块名两向：规范形认得、键与块号解得回；别的形状（非规范块号 · 坏键 · 上传件）一个不认。
#[test]
fn the_chunk_name_is_recognised_exactly_both_ways() {
    for seq in [0u64, 1, 9, 10, 4096, u64::MAX] {
        let n = chunk_name(KEY, seq);
        assert_eq!(parse_chunk_name(&n), Some((KEY, seq)), "{n}");
    }
    for bad in [
        format!("{KEY}.01{CHUNK_SUFFIX}"),
        format!("{KEY}.{CHUNK_SUFFIX}"),
        format!("{KEY}.-1{CHUNK_SUFFIX}"),
        format!("{KEY}.1a{CHUNK_SUFFIX}"),
        format!("{KEY}{PART_SUFFIX}"),
        format!("{KEY}.0.part"),
        format!("{}.0{CHUNK_SUFFIX}", KEY.to_uppercase()),
        format!("x.0{CHUNK_SUFFIX}"),
        format!("{KEY}.99999999999999999999999{CHUNK_SUFFIX}"),
    ] {
        assert_eq!(parse_chunk_name(&bad), None, "该拒的块名放过了：{bad}");
    }
}

/// 🔴 **拼回来 == 原文，而且与 `files-write-text` 写出同一个结果**（同一个原语：原地覆盖）。
///
/// 另一侧异源：同样的字节经写面 `overwrite_text` 直接写进一份孪生文件，逐项比 —— 内容、权限位、
/// 链接仍是链接且写穿到它指向的那份。成功之后这一键的块零剩；暂存区由第一块自己建。
#[cfg(unix)]
#[test]
fn staged_chunks_commit_back_byte_for_byte_like_a_plain_write() {
    use std::os::unix::fs::PermissionsExt as _;
    let (home, root) = bare_rig("ctext");
    let parts = [body(700, 1), body(1, 2), body(65_537, 3), body(12, 4)];
    let whole: Vec<u8> = parts.concat();
    for rel in ["plain.txt", "twin.txt", "real.txt"] {
        std::fs::write(root.join(rel), b"old contents").unwrap();
        std::fs::set_permissions(root.join(rel), std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    std::os::unix::fs::symlink("real.txt", root.join("link.txt")).unwrap();

    for (rel, key) in [
        ("plain.txt", KEY),
        ("link.txt", "fedcba9876543210fedcba9876543210"),
    ] {
        for (i, p) in parts.iter().enumerate() {
            let got = send_chunk(&home, key, i as u64, p).expect("送块该成");
            assert_eq!(got["bytes"], p.len() as u64);
        }
        let got = send_commit(
            &home,
            key,
            parts.len() as u64,
            whole.len() as u64,
            &root,
            rel,
        )
        .expect("提交该成");
        assert_eq!(got["bytes"], whole.len() as u64);
        assert!(
            chunks_left(&home, key).is_empty(),
            "提交成功了，块还在：{:?}",
            chunks_left(&home, key)
        );
    }
    overwrite_text(&root, "twin.txt", &whole).expect("孪生那一份直接写");

    let mode = |rel: &str| {
        std::fs::metadata(root.join(rel))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    assert_eq!(
        std::fs::read(root.join("plain.txt")).unwrap(),
        whole,
        "拼回来的不是原文"
    );
    assert_eq!(
        std::fs::read(root.join("plain.txt")).unwrap(),
        std::fs::read(root.join("twin.txt")).unwrap()
    );
    assert_eq!(
        mode("plain.txt"),
        mode("twin.txt"),
        "权限位与直接写的那一份不同 —— 不是原地覆盖"
    );
    assert_eq!(mode("plain.txt"), 0o640);
    assert!(
        std::fs::symlink_metadata(root.join("link.txt"))
            .unwrap()
            .file_type()
            .is_symlink(),
        "链接被换成了一份普通文件 —— 那是改名上位，不是原地覆盖"
    );
    assert_eq!(
        std::fs::read(root.join("real.txt")).unwrap(),
        whole,
        "没写穿链接"
    );
}

/// 🔴 **块对不上 ⇒ 拒，目标一个字节没动，而且这一键的块照样删干净**。
///
/// 五形：缺中间一块 · 报的总长多了 · 报的总长少了 · 多出第 `chunks` 块 · 某一块是一条链接。
#[cfg(unix)]
#[test]
fn a_mismatched_set_of_chunks_is_refused_and_the_target_is_untouched() {
    let (home, root) = bare_rig("cmis");
    let target = root.join("t.txt");
    std::fs::write(&target, b"keep me exactly").unwrap();
    let outside = root.join("outside.bin");
    std::fs::write(&outside, b"xyz").unwrap();
    let a = body(10, 7);
    let b = body(20, 8);
    let c = body(30, 9);
    type Setup<'a> = Box<dyn Fn(&Path, &str) + 'a>;
    let stage = |h: &Path, k: &str, seq: u64, p: &[u8]| {
        send_chunk(h, k, seq, p).expect("送块");
    };
    let cases: Vec<(&str, Setup, u64, u64, &str)> = vec![
        (
            "缺中间一块",
            Box::new(|h: &Path, k: &str| {
                stage(h, k, 0, &a);
                stage(h, k, 2, &c);
            }),
            3,
            60,
            "io_failed",
        ),
        (
            "总长报多了",
            Box::new(|h: &Path, k: &str| {
                stage(h, k, 0, &a);
                stage(h, k, 1, &b);
            }),
            2,
            31,
            "io_failed",
        ),
        (
            "总长报少了",
            Box::new(|h: &Path, k: &str| {
                stage(h, k, 0, &a);
                stage(h, k, 1, &b);
            }),
            2,
            29,
            "io_failed",
        ),
        (
            "多出一块",
            Box::new(|h: &Path, k: &str| {
                stage(h, k, 0, &a);
                stage(h, k, 1, &b);
                stage(h, k, 2, &c);
            }),
            2,
            30,
            "io_failed",
        ),
        (
            "一块是链接",
            Box::new(|h: &Path, k: &str| {
                stage(h, k, 0, &a);
                std::os::unix::fs::symlink(&outside, h.join(STAGING_DIR).join(chunk_name(k, 1)))
                    .unwrap();
            }),
            2,
            13,
            "refused",
        ),
    ];
    for (i, (what, setup, chunks, bytes, code)) in cases.iter().enumerate() {
        let key = format!("{:032x}", i + 1);
        setup(&home, &key);
        let e = send_commit(&home, &key, *chunks, *bytes, &root, "t.txt").expect_err(what);
        assert_eq!(&e.0, code, "{what}：{}", e.1);
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"keep me exactly",
            "{what}：目标被动了"
        );
        assert!(
            chunks_left(&home, &key).iter().all(|n| {
                std::fs::symlink_metadata(home.join(STAGING_DIR).join(n))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            }),
            "{what}：拒了之后这一键的块还在：{:?}",
            chunks_left(&home, &key)
        );
    }
    assert_eq!(
        std::fs::read(&outside).unwrap(),
        b"xyz",
        "链接指向的那份被动了"
    );
}

/// ★ 同一块只写一次（`O_EXCL`）：重发 ⇒ `io_failed`，第一次写进去的原样；块的落点上摆一条链接 ⇒ 不跟过去。
#[cfg(unix)]
#[test]
fn a_chunk_is_written_once_and_never_through_a_link() {
    let (home, root) = bare_rig("conce");
    send_chunk(&home, KEY, 0, b"first").expect("第一次");
    let e = send_chunk(&home, KEY, 0, b"second").expect_err("重发该拒");
    assert_eq!(e.0, "io_failed", "{}", e.1);
    assert_eq!(
        std::fs::read(home.join(STAGING_DIR).join(chunk_name(KEY, 0))).unwrap(),
        b"first"
    );

    let outside = root.join("precious.txt");
    std::fs::write(&outside, b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, home.join(STAGING_DIR).join(chunk_name(KEY, 1))).unwrap();
    let e = send_chunk(&home, KEY, 1, b"evil").expect_err("落点是链接该拒");
    assert_eq!(e.0, "io_failed", "{}", e.1);
    assert_eq!(
        std::fs::read(&outside).unwrap(),
        b"keep",
        "跟着链接写出去了"
    );
}

/// ★ 提交先过写面那道路径解析：上跳 / 不存在的目标 ⇒ 拒、盘上零改动、块照样删掉。
/// 从前第一格是「会话文件 ⇒ 拒」；今天那一格翻成正控：存盘改得动会话文件。
#[test]
fn a_text_commit_goes_through_the_write_fence() {
    let (home, root) = bare_rig("cfence");
    let proj = root.join("projects").join("-x");
    std::fs::create_dir_all(&proj).unwrap();
    let session = proj.join("s.jsonl");
    std::fs::write(&session, b"{}\n").unwrap();
    // 不存在的目标从前是 `refused`（解不到底）；带了 CAS 之后是 `stale`（「你打开的时候还在」）—— 逐格钉码。
    for (i, (rel, code)) in [("../escape.txt", "refused"), ("nope.txt", "stale")]
        .iter()
        .enumerate()
    {
        let key = format!("{:032x}", 0xa0 + i);
        send_chunk(&home, &key, 0, b"payload").unwrap();
        let e = send_commit(&home, &key, 1, 7, &root, rel).expect_err(rel);
        assert_eq!(e.0, *code, "{rel}：{e:?}");
        assert!(chunks_left(&home, &key).is_empty(), "{rel}：块没删");
    }
    let key = format!("{:032x}", 0xaf);
    send_chunk(&home, &key, 0, b"payload").unwrap();
    send_commit(&home, &key, 1, 7, &root, "projects/-x/s.jsonl").expect("🔴 存盘改不动会话文件");
    assert!(chunks_left(&home, &key).is_empty(), "块没删");
    assert_eq!(std::fs::read(&session).unwrap(), b"payload");
    assert!(
        !root.join("nope.txt").exists(),
        "不存在的目标被新建了 —— 存盘只改已在的文件"
    );
    assert!(!root.parent().unwrap().join("escape.txt").exists());
}

/// ★ 参数边：坏键拒在碰盘之前（暂存区都不建）· 空块 · 缺块号 · 超过读得回的天花板 · 块数越界。
#[test]
fn the_chunk_commands_refuse_bad_arguments_before_touching_the_disk() {
    let (home, root) = bare_rig("cargs");
    let e = send_chunk(&home, "../../../../etc/passwd/xxxxxxxxxxx", 0, b"x").expect_err("坏键");
    assert_eq!(e.0, "refused");
    assert!(!home.join(".cc-monitor").exists(), "坏键也把暂存区建出来了");
    let e = answer_stage_at(
        &home,
        &serde_json::json!({"key": KEY, "seq": 0, "content": ""}),
    )
    .expect_err("空块");
    assert_eq!(e.0, "bad_args");
    let e = answer_stage_at(&home, &serde_json::json!({"key": KEY, "content": "x"}))
        .expect_err("缺块号");
    assert_eq!(e.0, "bad_args");
    let cap = crate::files::READ_TEXT_MAX_BYTES as u64;
    for (chunks, bytes) in [(1, cap + 1), (0, 5), (6, 5)] {
        let e = send_commit(&home, KEY, chunks, bytes, &root, "t.txt").expect_err("该拒");
        assert_eq!(e.0, "bad_args", "chunks={chunks} bytes={bytes}：{}", e.1);
    }
    // 正控：恰好到天花板的那一个数本身放得过参数这一关（后面才因为块不在而拒）。
    let e = send_commit(&home, KEY, 1, cap, &root, "t.txt").expect_err("块不在");
    assert_eq!(e.0, "io_failed", "{}", e.1);
}

/// ★ 孤儿扫认得块的形状：老的删；新的留；调用方那个键的留；非规范块号不碰。
#[test]
fn the_sweep_also_collects_stale_chunks() {
    let (home, _root) = rig("sweepchunk");
    let now: u64 = 2_000_000_000;
    let old = now - STAGING_STALE_SECS - 1;
    let fresh = now - STAGING_STALE_SECS + 60;
    let dir = home.join(STAGING_DIR);
    let k_old = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let k_fresh = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let names = [
        (chunk_name(k_old, 3), old),
        (chunk_name(k_fresh, 0), fresh),
        (chunk_name(KEY, 0), old),
        (format!("{k_old}.03{CHUNK_SUFFIX}"), old),
    ];
    for (n, t) in &names {
        std::fs::write(dir.join(n), b"z").unwrap();
        set_mtime(&dir.join(n), *t);
    }
    let gone = sweep_stale(&home, now, KEY);
    assert_eq!(gone, vec![chunk_name(k_old, 3)]);
    for (n, _) in &names[1..] {
        assert!(dir.join(n).exists(), "{n} 被扫了");
    }
}

/// **分块那一支同一道 CAS**（两支存盘同一种结果）：盘上那份在读之后被改了 ⇒ `stale`，
/// 目标一个字节没动、块照样删掉；拿「此刻那一份」的摘要 ⇒ 写成，应答交新摘要 == 写进去那份的。
#[test]
fn a_chunked_save_over_a_changed_file_is_stale_like_the_one_line_save() {
    let (home, root) = bare_rig("fw1-commit-cas");
    std::fs::write(root.join("t.txt"), b"old").unwrap();
    let seen = content_sha256(b"old");
    std::fs::write(root.join("t.txt"), b"old + someone else").unwrap();
    let whole = body(3000, 7);
    for (i, p) in whole.chunks(1000).enumerate() {
        send_chunk(&home, KEY, i as u64, p).expect("送块");
    }
    let e = answer_commit_text_at(
        &home,
        &serde_json::json!({
            "key": KEY, "chunks": 3, "bytes": 3000,
            "root": root.to_string_lossy(), "rel": "t.txt",
            "expect": {"sha256": seen},
        }),
    )
    .expect_err("盘上已经变了，竟然提交成了");
    assert_eq!(e.0, "stale", "{e:?}");
    assert_eq!(
        std::fs::read(root.join("t.txt")).unwrap(),
        b"old + someone else"
    );
    assert!(chunks_left(&home, KEY).is_empty(), "stale 之后块没删");
    for (i, p) in whole.chunks(1000).enumerate() {
        send_chunk(&home, KEY, i as u64, p).expect("再送块");
    }
    let got =
        send_commit(&home, KEY, 3, 3000, &root, "t.txt").expect("拿此刻那一份的摘要提交，该成");
    assert_eq!(got["sha256"], content_sha256(&whole));
    assert_eq!(std::fs::read(root.join("t.txt")).unwrap(), whole);
    // 缺 `expect` ⇒ `bad_args`（没有「不问就盖」这一形）。
    let e = answer_commit_text_at(
        &home,
        &serde_json::json!({"key": KEY, "chunks": 1, "bytes": 1, "root": root.to_string_lossy(), "rel": "t.txt"}),
    )
    .expect_err("没给 expect 竟然收了");
    assert_eq!(e.0, "bad_args");
}

/// 提交的整份摘要：**必给**（缺 ⇒ `bad_args`）；对不上 ⇒ `stale`、目标一个字节没动、坏暂存件删掉；
/// 对得上 ⇒ 照旧上位。要求：「`files-commit-upload` 必给 `expect:{sha256}` → 远端后端改名上位前核，
/// 不等 ⇒ stale、删坏暂存件」。
#[test]
fn the_commit_checks_the_whole_staged_file_against_the_digest_it_is_given() {
    let (home, root) = rig("fw1-digest");
    let body = b"the whole staged file".to_vec();
    let staged = stage(&home, KEY, &body);
    let e = answer_commit_at(
        &home,
        &serde_json::json!({"key": KEY, "root": root.to_string_lossy(), "rel": "a.bin", "overwrite": false}),
    )
    .expect_err("没给 expect 竟然上位了");
    assert_eq!(e.0, "bad_args");
    assert!(staged.exists(), "参数拒了却动了暂存件");
    let mut wrong = body.clone();
    wrong[3] ^= 1;
    let e = commit_upload(&home, KEY, &root, "a.bin", false, &content_sha256(&wrong))
        .expect_err("摘要对不上竟然上位了");
    assert_eq!(e.code(), "stale", "{}", e.message());
    assert!(!root.join("a.bin").exists(), "摘要对不上却落进了目标");
    assert!(!staged.exists(), "坏暂存件没删");
    let staged = stage(&home, KEY, &body);
    commit_upload(&home, KEY, &root, "a.bin", false, &content_sha256(&body)).expect("对得上该上位");
    assert_eq!(std::fs::read(root.join("a.bin")).unwrap(), body);
    assert!(!staged.exists());
}

/// 〔RK1 小尾巴〕后端**这一趟建出来的** `~/.cc-monitor` 与暂存区是 0700（不按 umask）；**已在的**不动。
/// 守的要求：RK1 报备 §5.4 最后一条「`~/.cc-monitor` 这一层目录若由中转第一个建出来，权限是 umask 默认（本机现打 0775）」，
/// 要求：「`~/.cc-monitor` 首建权限按 umask ⇒ 0700」。形状：两向（新建 ⇒ 0700 · 已在 0755 ⇒ 仍 0755）。
#[test]
#[cfg(unix)]
fn hx1_the_staging_dirs_are_born_private_and_an_existing_one_is_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let key = "0123456789abcdef0123456789abcdef";

    let home = std::env::temp_dir().join(format!("ccm-hx1-stg-new-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(&home).expect("家");
    stage_chunk(&home, key, 0, b"x").expect("写一块");
    assert_eq!(
        mode(&home.join(".cc-monitor")),
        crate::common::own_dir::PRIVATE_DIR_MODE
    );
    assert_eq!(
        mode(&home.join(STAGING_DIR)),
        crate::common::own_dir::PRIVATE_DIR_MODE
    );
    std::fs::remove_dir_all(&home).ok();

    let home = std::env::temp_dir().join(format!("ccm-hx1-stg-old-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(home.join(".cc-monitor")).expect("预置");
    std::fs::set_permissions(
        home.join(".cc-monitor"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
    stage_chunk(&home, key, 0, b"x").expect("写一块");
    assert_eq!(
        mode(&home.join(".cc-monitor")),
        0o755,
        "已在的那一层被改了权限"
    );
    assert_eq!(
        mode(&home.join(STAGING_DIR)),
        crate::common::own_dir::PRIVATE_DIR_MODE
    );
    std::fs::remove_dir_all(&home).ok();
}

/// 要求：「跨盘提交与非标准 SFTP 起始目录 —— `staging/` 与目标不在同一个盘 ⇒ `EXDEV`，
/// 上传提交失败原样带回（要做就得『复制 ＋ 删』，一步复制在禁表里）」＋ `§5.1` 那张表「暂存区与目标不同盘 ⇒ `EXDEV` 失败」。
///
/// 注入「改名上位回 `EXDEV`」（跨盘在测试里造不出来，如实）：两支（不覆盖 · 覆盖）各一趟 ⇒ 目标逐字节等于暂存件、暂存件被消耗、
/// 目标目录里不留暂存旁名；覆盖那一支顶掉旧内容。阴性：不注入 ⇒ 走同盘改名那一支（结果同形，证明注入口没把生产那一支换掉）。
#[test]
fn a_cross_device_commit_falls_back_to_copy_and_delete() {
    for (overwrite, tag) in [(false, "xdev-new"), (true, "xdev-over")] {
        let (home, root) = rig(tag);
        let body = b"\x00across\xffdisks\n".to_vec();
        let staged = stage(&home, KEY, &body);
        if overwrite {
            std::fs::write(root.join("a.bin"), b"old").unwrap();
        }
        let (landed, n) = commit_upload_in(
            &home,
            KEY,
            &root,
            Path::new("a.bin"),
            overwrite,
            &staged_sha(&home, KEY),
            true,
        )
        .unwrap_or_else(|e| panic!("跨盘那一支没落进去（overwrite={overwrite}）：{e:?}"));
        assert_eq!(n, body.len() as u64);
        assert_eq!(
            std::fs::read(&landed).expect("读落点"),
            body,
            "跨盘落进去的字节不对"
        );
        assert!(!staged.exists(), "跨盘那一支没消耗暂存件");
        let side = root.join(format!(".a.bin.ccm-commit-{KEY}.part"));
        assert!(
            std::fs::symlink_metadata(&side).is_err(),
            "目标目录里留下了暂存旁名"
        );
    }
    // 阴性：不注入 ⇒ 同盘改名那一支（结果同形）。
    let (home, root) = rig("xdev-none");
    let staged = stage(&home, KEY, b"same");
    commit_upload_in(
        &home,
        KEY,
        &root,
        Path::new("c.bin"),
        false,
        &staged_sha(&home, KEY),
        false,
    )
    .expect("同盘那一支该落进去");
    assert_eq!(std::fs::read(root.join("c.bin")).unwrap(), b"same");
    assert!(!staged.exists());
}

/// 线上那一臂：`files-commit-upload` 带 `chunks` ⇒ 先把块拼成暂存件再走同一条提交（整份摘要照核）；
/// 摘要对不上 ⇒ `stale`、目标不在（块形与 SFTP 那条路同一道 CAS）。。
#[test]
fn the_commit_face_assembles_chunks_when_asked() {
    let h = std::env::temp_dir().join(format!("ccm-fc-chunks-{}", std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(h.join(".cc-monitor")).expect("铺家");
    std::fs::create_dir_all(h.join("dst")).expect("铺目标");
    let key = "c".repeat(32);
    stage_chunk(&h, &key, 0, b"hello ").expect("块 0");
    stage_chunk(&h, &key, 1, b"world").expect("块 1");
    let args = |sha: &str, rel: &str| {
        serde_json::json!({
            "key": key, "root": h.join("dst").to_string_lossy(), "rel": rel, "overwrite": false,
            "expect": {"sha256": sha}, "chunks": 2, "bytes": 11,
        })
    };
    answer_commit_at(
        &h,
        &args(&crate::files::content_sha256(b"hello world"), "a.txt"),
    )
    .expect("块形提交被拒");
    assert_eq!(
        std::fs::read(h.join("dst/a.txt")).expect("目标"),
        b"hello world"
    );
    stage_chunk(&h, &key, 0, b"hello ").expect("块 0");
    stage_chunk(&h, &key, 1, b"world").expect("块 1");
    let e = answer_commit_at(&h, &args(&crate::files::content_sha256(b"other"), "b.txt"))
        .expect_err("摘要对不上却上位了");
    assert_eq!(e.0, "stale", "{e:?}");
    assert!(std::fs::symlink_metadata(h.join("dst/b.txt")).is_err());
    std::fs::remove_dir_all(&h).ok();
}

/// `rel` 收 `{"b16": …}`：本机落点是非 UTF-8 名（有损名下载在 Linux 上按原始字节落名）⇒ 落出来的名字逐字节就是它。
#[test]
#[cfg(unix)]
fn the_commit_face_lands_under_a_byte_named_rel() {
    use std::os::unix::ffi::OsStrExt as _;
    let h = std::env::temp_dir().join(format!("ccm-fc-b16-{}", std::process::id()));
    std::fs::remove_dir_all(&h).ok();
    std::fs::create_dir_all(h.join(".cc-monitor")).expect("铺家");
    std::fs::create_dir_all(h.join("dst")).expect("铺目标");
    let key = "d".repeat(32);
    stage_chunk(&h, &key, 0, b"raw").expect("块 0");
    let args = serde_json::json!({
        "key": key, "root": h.join("dst").to_string_lossy(), "rel": {"b16": "66fe"}, "overwrite": false,
        "expect": {"sha256": crate::files::content_sha256(b"raw")}, "chunks": 1, "bytes": 3,
    });
    answer_commit_at(&h, &args).expect("字节名的提交被拒");
    let want = h.join("dst").join(std::ffi::OsStr::from_bytes(b"f\xfe"));
    assert_eq!(std::fs::read(&want).expect("字节名的那份不在"), b"raw");
    std::fs::remove_dir_all(&h).ok();
}
