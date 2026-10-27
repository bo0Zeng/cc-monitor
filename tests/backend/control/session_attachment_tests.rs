//! `control/files_commit/session_attachment.rs` 的行为判据 —— 聊天附件的落 · 撤 · 清 · 开传前那一问。
//! 全部在本机临时目录上真跑（一个假的 `home` 带暂存区 ＋ 一个会话工作目录）。

use super::*;
use crate::common::staging::PART_SUFFIX;
use crate::control::files_commit::STAGING_DIR;
use crate::platform::fs::Face;
use cmd_wire::files::{Sha256Expect, Sha256Hex};

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-att-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

/// 假 home（带暂存区）＋ 会话工作目录。
fn rig(tag: &str) -> (PathBuf, PathBuf) {
    let base = temp_dir(tag);
    let home = base.join("home");
    let cwd = base.join("work");
    std::fs::create_dir_all(home.join(STAGING_DIR)).expect("建暂存区");
    std::fs::create_dir_all(&cwd).expect("建工作目录");
    (home, cwd)
}

const KEY: &str = "0123456789abcdef0123456789abcdef";
/// 2026-10-23 15:29:00 UTC。
const AT: i64 = 1_792_769_340;

fn sha(b: &[u8]) -> String {
    crate::control::files_write::content_sha256(b)
}

fn stage(home: &Path, key: &str, bytes: &[u8]) {
    std::fs::write(
        home.join(STAGING_DIR).join(format!("{key}{PART_SUFFIX}")),
        bytes,
    )
    .expect("摆暂存件");
}

fn req(name: &str, body: &[u8]) -> SessionAttachCommitReq {
    SessionAttachCommitReq {
        sid: cmd_wire::Sid::try_from("0b3f9a52-1111-4222-8333-444455556666".to_string())
            .expect("合形的 sid"),
        key: KEY.to_string(),
        name: name.to_string(),
        bytes: body.len() as u64,
        chunks: None,
        expect: Sha256Expect {
            sha256: Sha256Hex(sha(body)),
        },
    }
}

fn utc() -> crate::common::time::Tz {
    crate::common::time::Tz::default()
}

/// ★ 落一份：进 `.ccm-attach/<编号>/<原名>`，编号是那一刻的月日时分 ＋ 摘要前四位；`.gitignore` 只一行 `*`；回的路径 · 摘要 · 种类对得上。
#[test]
fn a_committed_attachment_lands_in_its_own_numbered_dir_with_the_ignore_file() {
    let (home, cwd) = rig("land");
    let body = b"\x89PNG fake bytes".to_vec();
    stage(&home, KEY, &body);
    let got = commit(
        &Face::issue(),
        &home,
        &cwd,
        req("login narrow.png", &body),
        AT,
        &utc(),
    )
    .expect("落成");
    let tail = &sha(&body)[..4];
    assert_eq!(got.id, format!("1023-1529-{tail}"));
    assert_eq!(
        got.rel,
        format!(".ccm-attach/1023-1529-{tail}/login narrow.png")
    );
    let landed = cwd.join(&got.rel);
    assert_eq!(std::fs::read(&landed).expect("读落点"), body);
    assert_eq!(PathBuf::from(&got.path), landed);
    assert_eq!(got.sha256.0, sha(&body));
    assert_eq!(got.bytes, body.len() as u64);
    assert_eq!(got.class, cmd_wire::files::FileClass::Image);
    assert_eq!(
        std::fs::read_to_string(cwd.join(".ccm-attach/.gitignore")).expect("读忽略规则"),
        "*\n"
    );
    assert!(
        !home
            .join(STAGING_DIR)
            .join(format!("{KEY}{PART_SUFFIX}"))
            .exists(),
        "暂存件被消耗"
    );
}

/// 同一分钟、同一份内容再落一次 ⇒ 换尾巴，两份各一个目录、都在。
#[test]
fn the_same_file_twice_in_one_minute_gets_two_dirs() {
    let (home, cwd) = rig("twice");
    let body = b"same".to_vec();
    stage(&home, KEY, &body);
    let a = commit(&Face::issue(), &home, &cwd, req("a.txt", &body), AT, &utc()).expect("头一份");
    stage(&home, KEY, &body);
    let b = commit(&Face::issue(), &home, &cwd, req("a.txt", &body), AT, &utc()).expect("第二份");
    assert_ne!(a.id, b.id);
    assert!(cwd.join(&a.rel).is_file() && cwd.join(&b.rel).is_file());
}

/// 摘要对不上 ⇒ 不落、刚建的编号目录也删掉（附件目录里只剩那份 `.gitignore`）。
#[test]
fn a_corrupt_stage_leaves_no_numbered_dir_behind() {
    let (home, cwd) = rig("corrupt");
    stage(&home, KEY, b"actual");
    let err = commit(
        &Face::issue(),
        &home,
        &cwd,
        req("a.txt", b"expected"),
        AT,
        &utc(),
    )
    .expect_err("摘要不等该拒");
    assert_eq!(err.code, "stale");
    let tried = format!("1023-1529-{}", &sha(b"expected")[..4]);
    assert!(cwd.join(".ccm-attach/.gitignore").is_file());
    assert!(
        !cwd.join(".ccm-attach").join(&tried).exists(),
        "没落成，刚建的编号目录 {tried} 该删掉"
    );
}

/// 单份超上限 ⇒ 开传前那一问就拒，落的那一下也拒（`too_big`），盘上零新增。
#[test]
fn an_oversized_file_is_refused_before_and_at_commit() {
    let (home, cwd) = rig("big");
    let items = [AttachItem {
        name: "dataset.zip".into(),
        bytes: MAX_ONE_BYTES + 1,
        dir: false,
    }];
    let v = check(&cwd, &items);
    assert_eq!(v[0].why, Some(AttachRefused::TooBig));
    assert!(v[0].said.is_some());
    let mut r = req("dataset.zip", b"x");
    r.bytes = MAX_ONE_BYTES + 1;
    let err = commit(&Face::issue(), &home, &cwd, r, AT, &utc()).expect_err("该拒");
    assert_eq!(err.code, "too_big");
    assert!(!cwd.join(".ccm-attach").exists());
}

/// 开传前那一问：目录 ⇒ `is_dir`；第 11 份 ⇒ `too_many`；合计过 100 MB 的那一份 ⇒ `too_much`；不收的不占份数与总量。
#[test]
fn the_check_answers_each_item_in_order_against_the_message_limits() {
    let (_home, cwd) = rig("check");
    let mb = 1024 * 1024;
    let mut items = vec![AttachItem {
        name: "assets".into(),
        bytes: 0,
        dir: true,
    }];
    for i in 0..9 {
        items.push(AttachItem {
            name: format!("f{i}"),
            bytes: 10 * mb,
            dir: false,
        });
    }
    items.push(AttachItem {
        name: "over".into(),
        bytes: 20 * mb,
        dir: false,
    });
    items.push(AttachItem {
        name: "tenth".into(),
        bytes: mb,
        dir: false,
    });
    items.push(AttachItem {
        name: "eleventh".into(),
        bytes: 1,
        dir: false,
    });
    let got: Vec<Option<AttachRefused>> = check(&cwd, &items).into_iter().map(|v| v.why).collect();
    let mut want = vec![Some(AttachRefused::IsDir)];
    want.extend(std::iter::repeat_n(None, 9));
    want.extend([
        Some(AttachRefused::TooMuch),
        None,
        Some(AttachRefused::TooMany),
    ]);
    assert_eq!(got, want);
}

/// 工作目录不在 ⇒ 逐份 `no_session`；只读 ⇒ `not_writable`，落的那一下也拒、不回落别处。
#[cfg(unix)]
#[test]
fn a_missing_or_read_only_workdir_is_refused_without_falling_back() {
    use std::os::unix::fs::PermissionsExt as _;
    let (home, cwd) = rig("ro");
    let one = [AttachItem {
        name: "a".into(),
        bytes: 1,
        dir: false,
    }];
    assert_eq!(
        check(&cwd.join("nope"), &one)[0].why,
        Some(AttachRefused::NoSession)
    );
    std::fs::set_permissions(&cwd, std::fs::Permissions::from_mode(0o555)).expect("改只读");
    assert_eq!(check(&cwd, &one)[0].why, Some(AttachRefused::NotWritable));
    stage(&home, KEY, b"a");
    let err = commit(&Face::issue(), &home, &cwd, req("a", b"a"), AT, &utc()).expect_err("该拒");
    std::fs::set_permissions(&cwd, std::fs::Permissions::from_mode(0o755)).expect("改回");
    assert_eq!(err.code, "not_writable");
    assert!(!cwd.join(".ccm-attach").exists());
}

/// 撤一份 ⇒ 那个编号目录没了；再撤 ⇒ `false`；编号不合形 ⇒ `bad_args`。
#[test]
fn dropping_removes_the_numbered_dir_and_is_idempotent() {
    let (home, cwd) = rig("drop");
    stage(&home, KEY, b"x");
    let a = commit(&Face::issue(), &home, &cwd, req("a.txt", b"x"), AT, &utc()).expect("落成");
    assert!(drop_one(&Face::issue(), &cwd, &a.id).expect("撤"));
    assert!(!cwd.join(".ccm-attach").join(&a.id).exists());
    assert!(!drop_one(&Face::issue(), &cwd, &a.id).expect("再撤"));
    assert_eq!(
        drop_one(&Face::issue(), &cwd, "../x")
            .expect_err("不合形")
            .code,
        "bad_args"
    );
}

/// 清理：修改时间超过七天的编号目录删掉；别的名字（用户自己放的）· `.gitignore` 一个不碰。
#[test]
fn the_sweep_drops_numbered_dirs_older_than_a_week_and_nothing_else() {
    let (home, cwd) = rig("sweep");
    stage(&home, KEY, b"old");
    let old = commit(
        &Face::issue(),
        &home,
        &cwd,
        req("old.txt", b"old"),
        AT,
        &utc(),
    )
    .expect("落成");
    std::fs::create_dir_all(cwd.join(".ccm-attach/mine")).expect("用户的目录");
    // 那个旧目录的修改时间是盘上真钟落下那一刻；「此刻」往后拨过七天。
    let now_on_disk = std::fs::metadata(cwd.join(".ccm-attach").join(&old.id))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let swept = sweep(
        &Face::issue(),
        &cwd,
        now_on_disk + STALE_SECS + 1,
        "1023-1529-0000",
    );
    assert_eq!(swept, vec![old.id.clone()]);
    assert!(cwd.join(".ccm-attach/mine").is_dir());
    assert!(cwd.join(".ccm-attach/.gitignore").is_file());
}
