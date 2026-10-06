//! （TOCTOU）：「能用原子原语闭合的都闭合 —— 不覆盖改名用 `renameat2(RENAME_NOREPLACE)`、新建用 `O_EXCL`、复制先写同目录临时件再 `RENAME_NOREPLACE`、全程 `O_NOFOLLOW`」。
//!
//! 每一条闭合各造一次竞争：在「判过」与「动手」之间插进别人的东西（同名文件 / 换成链接），断言不盖、不跟。
//! 插的口是各函数的 `between`（生产传空）或直接喂「解析之后被换掉」的那个路径。
//! 买不到：退回先看后改那一支（要一块不认 `RENAME_NOREPLACE` 的盘：NFS / 部分 FUSE）· 真并发的时序 · Windows 臂。

use super::*;

fn temp_root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-toctou-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&p).ok();
    std::fs::create_dir_all(&p).expect("建临时目标根");
    p
}

/// 这一趟留下的暂存旁名（`.<名>.ccm-<tag>-<pid>-<序>.part`，序号取到那个计数器此刻的值）。
fn leftovers(dir: &Path, name: &str, tag: &str, seq: &std::sync::atomic::AtomicU64) -> Vec<String> {
    let upto = seq.load(std::sync::atomic::Ordering::Relaxed);
    (0..upto)
        .map(|n| format!(".{name}.ccm-{tag}-{}-{n}.part", std::process::id()))
        .filter(|n| std::fs::symlink_metadata(dir.join(n)).is_ok())
        .collect()
}

#[test]
fn the_platform_rename_refuses_an_existing_target_and_moves_onto_an_absent_one() {
    let base = temp_root("prim");
    std::fs::write(base.join("a"), b"A").unwrap();
    std::fs::write(base.join("b"), b"B").unwrap();
    let e = crate::platform::fs::rename_noreplace(&base.join("a"), &base.join("b"))
        .expect_err("目标已在却改成了");
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "{e}");
    assert_eq!(std::fs::read(base.join("a")).unwrap(), b"A");
    assert_eq!(std::fs::read(base.join("b")).unwrap(), b"B");
    crate::platform::fs::rename_noreplace(&base.join("a"), &base.join("c"))
        .expect("目标不在却没改成");
    assert_eq!(std::fs::read(base.join("c")).unwrap(), b"A");
    assert!(!base.join("a").exists());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_rename_does_not_clobber_a_file_that_appears_after_the_paths_were_resolved() {
    let base = temp_root("mv");
    std::fs::write(base.join("mine"), b"mine").unwrap();
    let target = base.join("theirs");
    let got = rename_entry_racing(&base, Path::new("mine"), Path::new("theirs"), &mut || {
        std::fs::write(&target, b"theirs").unwrap();
    });
    assert!(matches!(got, Err(WriteRefusal::Exists(_))), "{got:?}");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"theirs",
        "判完之后冒出来的那一份被盖了"
    );
    assert_eq!(std::fs::read(base.join("mine")).unwrap(), b"mine", "源没了");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_copy_lands_whole_or_not_at_all_and_does_not_clobber_a_newcomer() {
    let base = temp_root("cp");
    std::fs::write(base.join("src"), b"source bytes").unwrap();
    let target = base.join("dst");
    let mut seen_early = None;
    let got = copy_entry_racing(
        &base,
        Path::new("src"),
        Path::new("dst"),
        false,
        &mut || {
            seen_early = Some(std::fs::symlink_metadata(&target).is_ok());
            std::fs::write(&target, b"theirs").unwrap();
        },
    );
    assert_eq!(
        seen_early,
        Some(false),
        "字节写满之前目标那一格已经露出来了（半份）"
    );
    assert!(matches!(got, Err(WriteRefusal::Io(_))), "{got:?}");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"theirs",
        "写的中途冒出来的那一份被盖了"
    );
    assert_eq!(
        leftovers(&base, "dst", "copy", &COPY_SEQ),
        Vec::<String>::new(),
        "暂存旁名留下了"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_put_that_expects_absence_does_not_clobber_a_file_that_appears_after_the_check() {
    let base = temp_root("put");
    let target = base.join("new.txt");
    let got = put_text_racing(
        &base,
        Path::new("new.txt"),
        b"ours",
        None,
        false,
        false,
        &mut || {
            std::fs::write(&target, b"theirs").unwrap();
        },
    );
    assert!(matches!(got, Err(WriteRefusal::Stale(_))), "{got:?}");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"theirs",
        "CAS 判完「不在」之后冒出来的那一份被盖了"
    );
    assert_eq!(
        leftovers(&base, "new.txt", "put", &PUT_SEQ),
        Vec::<String>::new(),
        "暂存旁名留下了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// ＋ 盘不认 `RENAME_NOREPLACE` ⇒ 不退回先看后改 —— 普通文件 `link ＋ unlink`，目录拒并出声。
/// `force_link` 模拟那块盘（`EINVAL`）。
#[cfg(unix)]
#[test]
fn a_disk_without_noreplace_moves_files_by_link_and_refuses_directories() {
    let base = temp_root("link");
    std::fs::write(base.join("a"), b"A").unwrap();
    let (_, _, done) = rename_no_clobber(&base, Path::new("a"), Path::new("b"), &mut || {}, true)
        .expect("路径解析拒了");
    done.expect("普通文件走 link 那一支没改成");
    assert!(!base.join("a").exists(), "源的名字还在");
    assert_eq!(std::fs::read(base.join("b")).unwrap(), b"A");
    // 目标在「解析之后、动手之前」冒出来 ⇒ link 原子失败，不盖、源不动。
    let target = base.join("c");
    let (_, _, done) = rename_no_clobber(
        &base,
        Path::new("b"),
        Path::new("c"),
        &mut || {
            std::fs::write(&target, b"theirs").unwrap();
        },
        true,
    )
    .expect("路径解析拒了");
    assert_eq!(
        done.expect_err("盖了").kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"theirs",
        "冒出来的那一份被盖了"
    );
    assert_eq!(std::fs::read(base.join("b")).unwrap(), b"A", "源没了");
    // 目录：没有这条路 ⇒ 拒并出声，一个字节不动。
    std::fs::create_dir(base.join("d")).unwrap();
    let said = rename_no_clobber(&base, Path::new("d"), Path::new("e"), &mut || {}, true)
        .expect_err("目录被改名了");
    let d = std::fs::canonicalize(base.join("d")).unwrap();
    assert_eq!(
        said,
        copy_text(
            "beFilesWrite.rename.dirNoNoreplace",
            &[("path", &d.display().to_string())]
        )
    );
    assert!(base.join("d").is_dir() && !base.join("e").exists());
    std::fs::remove_dir_all(&base).ok();
}

#[cfg(unix)]
#[test]
fn a_source_swapped_for_a_link_after_resolving_is_not_followed() {
    let base = temp_root("nf");
    let root = base.join("root");
    std::fs::create_dir(&root).unwrap();
    let secret = base.join("secret");
    std::fs::write(&secret, b"outside the root").unwrap();
    // 解析那一刻它是 `root/src` 这份普通文件；动手之前被换成一条指到根外的链接。
    let src = root.join("src");
    std::os::unix::fs::symlink(&secret, &src).unwrap();
    let perms = std::fs::metadata(&secret).unwrap().permissions();
    let got = land_copy(&root, Path::new("dst"), &src, perms, false, &mut || {});
    assert!(matches!(got, Err(WriteRefusal::Io(_))), "{got:?}");
    assert!(!root.join("dst").exists(), "跟着链接把根外那一份抄进来了");
    assert_eq!(
        leftovers(&root, "dst", "copy", &COPY_SEQ),
        Vec::<String>::new(),
        "暂存旁名留下了"
    );
    assert!(read_nofollow(&src).is_err(), "读那一口跟了链接");
    std::fs::remove_dir_all(&base).ok();
}
