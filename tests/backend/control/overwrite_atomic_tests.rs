//! **覆盖写原子化** —— 判据。
//!
//! 守的要求（住址）：
//! - 要求：「退出时在飞的写：覆盖写一律『临时件 ＋ rename』原子化」；
//! - 审计 E §E2：「`overwrite_text` 是先截断再写 …… 远端那份文件被截成半份或 0 字节」；
//! - 审计 E §3.3 吞错：「`files_commit.rs` 上传提交失败后删 0 字节占位也失败了 ⇒ 用户目录里留一个 0 字节文件，报错里没提」。
//! 设计与读数住 §2。⚠ 与「存盘不是原子的 …… 刻意不换语义」冲突，按 D-a 做、已报备。
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
    // 不剩旁名：旁名的形状是确定的（`.<名>.ccm-put-<pid>-<序>.part`），序号从本进程的计数器来 ⇒ 按这个进程到目前为止发出去的号逐个看。
    let upto = PUT_SEQ.load(std::sync::atomic::Ordering::Relaxed);
    let left: Vec<u64> = (0..upto)
        .filter(|seq| {
            root.join(format!(
                ".real.txt.ccm-put-{}-{seq}.part",
                std::process::id()
            ))
            .exists()
        })
        .collect();
    assert!(left.is_empty(), "剩了旁名（序号 {left:?}）");
    assert!(upto > 0, "正控：这一趟确实发过旁名的号");
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
        note.contains("/x/y/a.bin")
            && copy_core::copy_matches("beFilesCommit.upload.placeholderLeft", &note)
            && copy_core::copy_matches("beFilesCommit.upload.placeholderLeft", &note),
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

/// 就地写在一段生产代码里的位置：`fs::write(` · 不跟链接的截断开（`.truncate(true)`，`files_write::opener` 那条链）。
fn write_calls(code: &str) -> Vec<usize> {
    let mut at: Vec<usize> = code
        .match_indices("fs::write(")
        .chain(code.match_indices(".truncate(true)"))
        .map(|(i, _)| i)
        .collect();
    at.sort_unstable();
    at
}

#[test]
fn w4_the_only_in_place_write_left_is_the_windows_arm_of_swap_in() {
    // ① 全后端生产段的人群：`fs::write(` 只住 `control/files_write.rs`。
    let roots = crate::guard_support::code_roots();
    let mut files: Vec<String> = Vec::new();
    for (path, src) in guard_core::scan_tree_excluding(&roots[0], &["rs"], &[]) {
        let prod = crate::guard_support::production_side_of(&path, &src);
        for _ in write_calls(&prod) {
            files.push(
                path.strip_prefix(&roots[0])
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    // 两处：`swap_in` 的 Windows 臂 ＋ `overwrite_text` 的「硬链接 / 别人的属主」那一支
    // （后者从 `fs::write` 换成不跟链接的截断开，处数不变）。
    assert_eq!(
        files,
        vec![
            "control/files_write.rs".to_string(),
            "control/files_write.rs".to_string()
        ],
        "后端生产段就地写的集合变了"
    );
    // ② 那一处在 `swap_in` 那一段里、且那一段恰好一个 `#[cfg(windows)]`、就地写跟在它后面。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/files_write.rs"
    ));
    assert_eq!(
        prod.matches("fn swap_in(").count(),
        1,
        "swap_in 不是恰好一处"
    );
    let seg = &prod[prod.find("fn swap_in(").expect("swap_in")..];
    let seg = &seg[..seg.find("\n}\n").expect("swap_in 的尾")];
    assert_eq!(
        seg.matches("#[cfg(windows)]").count(),
        1,
        "swap_in 里 Windows 臂不是恰好一处"
    );
    assert_eq!(write_calls(seg).len(), 1, "swap_in 里就地写不是恰好一处");
    let win = seg.find("#[cfg(windows)]").expect("Windows 臂");
    assert!(win < write_calls(seg)[0], "就地写不在 Windows 臂里");
    // ③ 另一处在 `overwrite_text` 里、且在「该不该原子换」那一问（`in_place_reason(`）的答案那一支之内（之后、`swap_in(` 之前）。
    assert_eq!(prod.matches("pub fn overwrite_text(").count(), 1);
    let ow = &prod[prod.find("pub fn overwrite_text(").expect("overwrite_text")..];
    let ow = &ow[..ow.find("\n}\n").expect("overwrite_text 的尾")];
    assert_eq!(
        write_calls(ow).len(),
        1,
        "overwrite_text 里就地写不是恰好一处"
    );
    let asked = ow.find("in_place_reason(").expect("那一问");
    let swap = ow.find("swap_in(").expect("原子换那一支");
    let w = write_calls(ow)[0];
    assert!(asked < w && w < swap, "就地写不在「不该原子换」那一支里");
    // 正控：数法认得出两形就地写。
    assert_eq!(write_calls("std::fs::write(&p, b)").len(), 1);
    assert_eq!(
        write_calls("opener().write(true).truncate(true).open(&p)").len(),
        1
    );
}

/// **有硬链接的目标退回就地写**：两个名字都看得见新内容、inode 不换、链接数不变。
/// 守的要求：「目标 `nlink > 1` 或属主不是后端用户 ⇒ 退回就地写（保住硬链接与属主）」。
/// 对照：同一目录里没有硬链接的那一份照旧原子换（inode 换了 —— W2 那一形）。
#[test]
#[cfg(unix)]
fn h1_a_hardlinked_target_is_written_in_place_so_both_names_see_it() {
    use std::os::unix::fs::MetadataExt as _;
    let root = temp_dir("h1");
    std::fs::write(root.join("a.txt"), b"old").expect("铺");
    std::fs::hard_link(root.join("a.txt"), root.join("b.txt")).expect("硬链接");
    std::fs::write(root.join("solo.txt"), b"old").expect("铺");
    let ino = |p: &str| std::fs::metadata(root.join(p)).expect("meta").ino();
    let (a0, solo0) = (ino("a.txt"), ino("solo.txt"));
    overwrite_text(&root, "a.txt", b"new").expect("覆盖写");
    overwrite_text(&root, "solo.txt", b"new").expect("覆盖写");
    assert_eq!(
        std::fs::read(root.join("b.txt")).expect("读"),
        b"new",
        "另一个名字没看见新内容 —— 硬链接被拆开了"
    );
    assert_eq!(ino("a.txt"), a0, "有硬链接的那一份 inode 换了");
    assert_eq!(
        std::fs::metadata(root.join("a.txt")).expect("meta").nlink(),
        2
    );
    assert_ne!(
        ino("solo.txt"),
        solo0,
        "对照失效：没有硬链接的那一份也没原子换"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 「该不该原子换」那一问逐格（属主那一格要 root 才造得出真文件，只在纯函数上量 —— 如实登记）；
/// 接线：`overwrite_text` 问它时交的恰是 `links_and_owner` 与 `current_uid`。
#[test]
fn h2_the_in_place_reasons_are_exactly_links_and_owner() {
    assert_eq!(in_place_reason(1, 1000, 1000), None);
    let l = in_place_reason(2, 1000, 1000).expect("硬链接");
    assert!(l.contains("2 个硬链接") && !l.contains("属主"), "{l}");
    let o = in_place_reason(1, 0, 1000).expect("属主");
    assert!(
        o.contains("uid 0") && o.contains("uid 1000") && !o.contains("硬链接"),
        "{o}"
    );
    let both = in_place_reason(3, 0, 1000).expect("两样");
    assert!(
        both.contains("3 个硬链接") && both.contains("属主"),
        "{both}"
    );
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/files_write.rs"
    ));
    assert_eq!(
        prod.matches("in_place_reason(links, owner, crate::platform::paths::current_uid())")
            .count(),
        1,
        "overwrite_text 问的不是这台进程的 uid"
    );
    assert_eq!(
        prod.matches("let links_owner = links_and_owner(&real);")
            .count(),
        1
    );
}
