//! 〔HX1 · 4D〕**覆盖写原子化** —— 判据。
//!
//! 守的要求（住址）：
//! - 主会话 4D 裁 D-a（`4d-lanes.md`「主会话本批裁的」）逐字：「退出时在飞的写：覆盖写一律『临时件 ＋ rename』原子化」；
//! - 审计 E §E2：「`overwrite_text` 是先截断再写 …… 远端那份文件被截成半份或 0 字节」；
//! - 审计 E §3.3 吞错：「`files_commit.rs` 上传提交失败后删 0 字节占位也失败了 ⇒ 用户目录里留一个 0 字节文件，报错里没提」。
//! 设计与读数住 `调研/第四波记录/HX1.md` §2。⚠ 与 `设计/60 §5.5`「存盘不是原子的 …… 刻意不换语义」冲突，按 D-a 做、已报备。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | W1 | 写到一半进程被内核收掉（`ulimit -f` ⇒ `SIGXFSZ`，死在 `write` 中间），目标仍是旧的整份；正控：同一台架换成就地写，目标变成半份 | re-exec 测试二进制当子进程 |
//! | W2 | 内容换新 · 权限位沿用 · 最后一段是链接 ⇒ 链接还是链接、真文件换新 · 不剩旁名 · inode 换了 | 真文件 |
//! | W3 | 提交失败撤占位也失败 ⇒ 报错带占位路径与「删不掉」；撤得掉 ⇒ 一个字不加 | 两向 |
//! | W4 | 后端生产段 `fs::write(` 恰好一处，且在 `swap_in` 的 `cfg(windows)` 臂里 | 文本，零富余 ＋ 正控 |

use super::*;

const CHILD_MARK: &str = "CCM_HX1_OW_CHILD";
const CHILD_ROOT: &str = "CCM_HX1_OW_ROOT";
const CHILD_TEST_NAME: &str = "control::files_write::overwrite_atomic_tests::w1_child";
/// 新内容：远大于 `ulimit -f` 放行的那几 KiB。
const NEW_LEN: usize = 1 << 20;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "ccm-hx1-ow-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&p).expect("临时目录");
    p
}

/// 子进程那一半：按 `CCM_HX1_OW_CHILD` 选一种写法，把 1 MiB 写进 `<root>/t.txt`。
/// 父进程用 `ulimit -f` 起它 ⇒ 写过线那一刻内核发 `SIGXFSZ`，进程死在写的中间（与「写到一半被 SIGKILL」同一形）。
#[test]
#[ignore = "HX1 W1 的子进程那一半：只由 w1_* 父进程 re-exec 起来"]
fn w1_child() {
    let Ok(mode) = std::env::var(CHILD_MARK) else {
        return;
    };
    let root = PathBuf::from(std::env::var(CHILD_ROOT).expect("root"));
    let body = vec![b'N'; NEW_LEN];
    match mode.as_str() {
        // 生产那一个原语。
        "atomic" => {
            let _ = overwrite_text(&root, "t.txt", &body);
        }
        // 正控：旧形状（先截断再写）—— 台架看得见「半份」才说明它看得见原子与否。
        "inplace" => {
            let _ = std::fs::write(root.join("t.txt"), &body);
        }
        other => panic!("不认得的模式 {other}"),
    }
}

/// 起子进程（`ulimit -f 8` 之下），等它死，回目标文件此刻的内容与退出状态。
fn run_child(mode: &str, root: &Path) -> (Vec<u8>, std::process::ExitStatus) {
    let exe = std::env::current_exe().expect("测试二进制");
    let st = std::process::Command::new("sh")
        .arg("-c")
        .arg(r#"ulimit -f 8 && exec "$0" "$@""#)
        .arg(exe)
        .args([CHILD_TEST_NAME, "--exact", "--ignored", "--test-threads=1"])
        .env(CHILD_MARK, mode)
        .env(CHILD_ROOT, root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("起子进程");
    (std::fs::read(root.join("t.txt")).expect("读目标"), st)
}

#[test]
#[cfg(unix)]
fn w1_a_write_killed_halfway_leaves_the_old_whole_file() {
    use std::os::unix::process::ExitStatusExt as _;
    let old: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();

    // 正控：就地写被收在半路 ⇒ 目标变成半份（台架真能看见「不原子」）。
    let d = temp_dir("inplace");
    std::fs::write(d.join("t.txt"), &old).expect("铺旧内容");
    let (got, st) = run_child("inplace", &d);
    assert_eq!(
        st.signal(),
        Some(libc::SIGXFSZ),
        "台架没让子进程死在写的中间：{st:?}"
    );
    assert!(
        got != old && got.len() < NEW_LEN && got.iter().all(|b| *b == b'N'),
        "正控失效：就地写被收在半路，目标本该是半份新内容，实得 {} 字节",
        got.len()
    );
    std::fs::remove_dir_all(&d).ok();

    // 生产原语：同一台架 ⇒ 目标原封不动（逐字节）。
    let d = temp_dir("atomic");
    std::fs::write(d.join("t.txt"), &old).expect("铺旧内容");
    let (got, st) = run_child("atomic", &d);
    assert_eq!(st.signal(), Some(libc::SIGXFSZ), "{st:?}");
    assert_eq!(
        got, old,
        "覆盖写被收在半路之后目标不是旧的整份 —— 不是原子的"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
#[cfg(unix)]
fn w2_the_swap_keeps_mode_follows_the_link_and_leaves_no_side_file() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let root = temp_dir("w2");
    let real = root.join("real.txt");
    std::fs::write(&real, b"old").expect("铺");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).expect("chmod");
    std::os::unix::fs::symlink("real.txt", root.join("link.txt")).expect("链接");
    let ino_before = std::fs::metadata(&real).expect("meta").ino();

    let got = overwrite_text(&root, "link.txt", b"new content").expect("覆盖写");
    assert_eq!(
        std::fs::canonicalize(&got).expect("canon"),
        std::fs::canonicalize(&real).expect("canon")
    );
    assert_eq!(std::fs::read(&real).expect("读"), b"new content");
    let md = std::fs::metadata(&real).expect("meta");
    assert_eq!(md.permissions().mode() & 0o777, 0o640, "权限位没沿用");
    assert_ne!(
        md.ino(),
        ino_before,
        "inode 没换 —— 还是就地写，不是换名上位"
    );
    assert!(
        std::fs::symlink_metadata(root.join("link.txt"))
            .expect("meta")
            .file_type()
            .is_symlink(),
        "链接被顶掉了"
    );
    let names: Vec<String> = std::fs::read_dir(&root)
        .expect("列")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(names, vec!["link.txt", "real.txt"], "剩了旁名");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn w3_a_placeholder_that_could_not_be_removed_is_named_in_the_refusal() {
    let dest = Path::new("/x/y/a.bin");
    assert_eq!(
        crate::control::files_commit::placeholder_note(dest, &Ok(())),
        "",
        "撤掉了还多说"
    );
    let err = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    let note = crate::control::files_commit::placeholder_note(dest, &Err(err));
    assert!(
        note.contains("/x/y/a.bin") && note.contains("0 字节占位") && note.contains("删不掉"),
        "{note}"
    );
    // 接线：提交那一支拿「撤占位」的真结局去问它（不是 `let _ =` 吞掉）。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/files_commit.rs"
    ));
    let undo = prod
        .find("let undo = std::fs::remove_file(&dest);")
        .expect("撤占位那一句");
    let tail = &prod[undo..];
    assert!(
        tail.find("placeholder_note(&dest, &undo)")
            .is_some_and(|i| i < 400),
        "撤占位的结局没交给 placeholder_note"
    );
    assert!(
        !prod.contains("let _ = std::fs::remove_file(&dest)"),
        "撤占位的结局又被吞了"
    );
}

/// `fs::write(` 在一段生产代码里的位置。
fn write_calls(code: &str) -> Vec<usize> {
    code.match_indices("fs::write(").map(|(i, _)| i).collect()
}

#[test]
fn w4_the_only_in_place_write_left_is_the_windows_arm_of_swap_in() {
    let roots = crate::guard_support::code_roots();
    let mut hits: Vec<(String, usize)> = Vec::new();
    for (path, src) in guard_core::scan_tree_excluding(&roots[0], &["rs"], &[]) {
        let prod = crate::guard_support::production_side_of(&path, &src);
        for at in write_calls(&prod) {
            hits.push((
                path.strip_prefix(&roots[0])
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                at,
            ));
        }
        if path.ends_with("control/files_write.rs") {
            // 那一处必须在 `swap_in` 里、`cfg(windows)` 之后。
            let at = write_calls(&prod);
            assert_eq!(
                at.len(),
                1,
                "files_write.rs 生产段 `fs::write(` 不是恰好一处：{at:?}"
            );
            let fn_at = prod.find("fn swap_in(").expect("swap_in");
            let win = prod[fn_at..].find("#[cfg(windows)]").map(|i| fn_at + i);
            assert!(
                win.is_some_and(|w| w < at[0]) && fn_at < at[0],
                "那一处 `fs::write(` 不在 swap_in 的 Windows 臂里"
            );
        }
    }
    let files: Vec<&str> = hits.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        files,
        vec!["control/files_write.rs"],
        "后端生产段就地写的集合变了：{hits:?}"
    );
    // 正控：数法认得出一处就地写。
    assert_eq!(write_calls("std::fs::write(&p, b)").len(), 1);
}
