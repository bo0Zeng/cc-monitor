//! 〔HX1 · 4D · 主会话裁 HX1 拍板项 4〕**后端建自家目录的那一个函数** —— 判据。
//!
//! 守的要求（住址）：主会话裁「建自家目录收成一个小函数，五处（`exit_policy` · `asset_catalog` · `relay/door` · 远端 SFTP 部署建目录 ·
//! `local_backend.rs` 释放目录）都走它（0700、已存在不动），判据：生产段建 `~/.cc-monitor` 的调用点 == 那个函数一处（两向，带正控）」；
//! RK1 报备 §5.4（首建按 umask）。设计与读数住 `调研/第四波记录/HX1.md` §6。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | O1 | 新建 ⇒ 0700；已在的 0755 ⇒ 仍 0755；父目录不在 ⇒ 照实报错 | 真目录，两向 |
//! | O2 | 后端生产段**每一处建目录**（`fs::create_dir` · `create_dir_all` · `DirBuilder` · SFTP `.create_dir(`）的所在 (文件, 函数) 集合 == 登记表；登记表里建「后端自家目录」的那一格 == {`own_dir.rs::ensure_private_dir` · 远端 `dial/sftp.rs::make_dir`} | 两向相等 ＋ 正控（合成语料里多一处 `create_dir` 必被认出） |
//! | O3 | 五个调用方（四份第四层 ＋ 暂存区）各经它恰好一处；远端那一处用的是同一个权限位常量 | 文本 |

#[cfg(unix)]
use super::*;

#[test]
#[cfg(unix)]
fn o1_born_private_existing_left_alone_missing_parent_refused() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = std::env::temp_dir().join(format!("ccm-hx1-owndir-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("base");
    let fresh = base.join(".cc-monitor");
    ensure_private_dir(&fresh).expect("建");
    assert_eq!(mode(&fresh), PRIVATE_DIR_MODE);
    ensure_private_dir(&fresh).expect("再建一次（已在）不算错");
    let old = base.join("old");
    std::fs::create_dir(&old).expect("预置");
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    ensure_private_dir(&old).expect("已在");
    assert_eq!(mode(&old), 0o755, "已在的那一层被改了权限");
    assert!(
        ensure_private_dir(&base.join("no/such")).is_err(),
        "父目录不在也建出来了（只建一层）"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// 一处建目录的调用：(文件, 所在函数)。
fn dir_creations(rel: &str, prod: &str) -> Vec<(String, String)> {
    let needles = [
        "fs::create_dir(",
        "fs::create_dir_all(",
        "fs::DirBuilder::new(",
        ".create_dir(",
    ];
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in prod.lines() {
        let t = line.trim_start();
        for kw in [
            "pub(crate) async fn ",
            "pub async fn ",
            "pub(crate) fn ",
            "pub fn ",
            "async fn ",
            "fn ",
        ] {
            if let Some(rest) = t.strip_prefix(kw) {
                cur = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                break;
            }
        }
        if needles.iter().any(|n| line.contains(n)) {
            out.push((rel.to_string(), cur.clone()));
        }
    }
    out
}

/// 登记表：`(文件, 函数, 建的是什么)`。第三列恰为 [`OWN_HOME`] 的那几行 = 建后端自家目录的地方。
const OWN_HOME: &str = "后端自家目录";
const DIR_CREATORS: &[(&str, &str, &str)] = &[
    ("own_dir.rs", "ensure_private_dir", OWN_HOME),
    ("dial/sftp.rs", "make_dir", OWN_HOME),
    (
        "control/files_write.rs",
        "make_dir",
        "用户的目录（`files-mkdir`，文件管理写面）",
    ),
    (
        "control/files_write.rs",
        "make_parents",
        "用户的目录（`files-put` 显式要了父目录）",
    ),
    // 〔W5-FILES 与 HX1 合并〕复制目录的执行趟逐条建目录（`files-copy` 的 `recursive: true`，`设计/60 §7 #6`）。
    //   HX1 立本表时它还在另一棵树上 ⇒ 两边各自绿、合起来才红，按实数 +1。
    (
        "control/files_write.rs",
        "copy_planned",
        "用户的目录（`files-copy` 复制目录，逐条建）",
    ),
    (
        "history_annotations.rs",
        "write_at",
        "monitor 数据目录那一层（注解文件由 monitor 交路径）",
    ),
    (
        "accounts/upstream/file_face.rs",
        "write_at",
        "agent 家目录下 `claudecode-frontend/` 那一层（凭据文件）",
    ),
];

#[test]
fn o2_every_dir_creation_is_registered_and_only_the_helper_builds_our_home() {
    let root = crate::guard_support::src_root();
    let mut found: Vec<(String, String)> = Vec::new();
    let mut scanned = 0usize;
    for (path, src) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        scanned += 1;
        let prod = crate::guard_support::production_side_of(&path, &src);
        found.extend(dir_creations(&rel, &prod));
    }
    assert!(scanned >= 60, "只扫到 {scanned} 份 —— 遍历坏了");
    found.sort();
    found.dedup();
    let mut want: Vec<(String, String)> = DIR_CREATORS
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    want.sort();
    assert_eq!(
        found, want,
        "后端生产段建目录的地方与登记表对不上 —— 新长出来的那一处若建的是 `~/.cc-monitor` 一族，改走 `own_dir::ensure_private_dir`"
    );
    let home: Vec<&str> = DIR_CREATORS
        .iter()
        .filter(|(_, _, what)| *what == OWN_HOME)
        .map(|(f, n, _)| if *f == "own_dir.rs" { *n } else { *f })
        .collect();
    assert_eq!(
        home,
        vec!["ensure_private_dir", "dial/sftp.rs"],
        "建后端自家目录的只该是那个函数（本机）＋ 远端部署那一处"
    );
    // 正控：数法认得出合成语料里多出来的一处。
    let planted = dir_creations(
        "x.rs",
        "fn sneaky() {\n    std::fs::create_dir(p).ok();\n}\n",
    );
    assert_eq!(planted, vec![("x.rs".to_string(), "sneaky".to_string())]);
}

#[test]
fn o3_the_five_callers_go_through_it_and_the_remote_one_shares_the_mode() {
    let src = |p: &str| {
        crate::guard_support::production_code(
            &std::fs::read_to_string(crate::guard_support::src_root().join(p)).expect("读"),
        )
    };
    for f in [
        "control/exit_policy.rs",
        "asset_catalog.rs",
        "relay/door.rs",
        "skill_ledger.rs",
        "control/files_commit.rs",
    ] {
        assert_eq!(
            src(f)
                .matches("crate::own_dir::ensure_private_dir(")
                .count(),
            1,
            "{f} 建自家目录不是经 own_dir 恰好一处"
        );
    }
    let sftp = src("dial/sftp.rs");
    assert_eq!(
        sftp.matches("permissions: Some(crate::own_dir::PRIVATE_DIR_MODE)")
            .count(),
        1,
        "远端部署建目录没用同一个权限位"
    );
}
