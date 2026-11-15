use super::*;

/// `O_EXCL` 新建：已在 ⇒ 拒、原样不动；最后一段是链接 ⇒ 也拒（不穿过链接去建）。
#[test]
fn create_new_refuses_an_existing_name_and_a_link() {
    let d = test_home::scratch("create");
    let face = Face::issue();
    let at = Resolved::vouch(d.join("a.txt")).unwrap();
    face.create_new(&at)
        .and_then(|mut f| f.write_all(b"one"))
        .expect("新建");
    let again = face.create_new(&at).expect_err("已在必须拒");
    assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"one");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(d.join("elsewhere"), d.join("l")).unwrap();
        face.create_new(&Resolved::vouch(d.join("l")).unwrap())
            .expect_err("链接那一名必须拒");
        assert!(!d.join("elsewhere").exists(), "穿过链接建出了目标");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 覆盖写不跟链接：最后一段是链接 ⇒ 失败，链接指着的那一份一个字节不动。
#[cfg(unix)]
#[test]
fn overwrite_does_not_follow_a_link() {
    let d = test_home::scratch("over");
    std::fs::write(d.join("real"), b"keep").unwrap();
    std::os::unix::fs::symlink(d.join("real"), d.join("l")).unwrap();
    Face::issue()
        .overwrite(&Resolved::vouch(d.join("l")).unwrap())
        .expect_err("覆盖写穿过了链接");
    assert_eq!(std::fs::read(d.join("real")).unwrap(), b"keep");
    let _ = std::fs::remove_dir_all(&d);
}

/// 分叉那只手：只会新建；已在 ⇒ 拒在建那一步、旧的一个字节不动。
#[test]
fn new_file_only_creates() {
    let d = test_home::scratch("newfile");
    let nf = NewFile::issue();
    nf.create(&d.join("s.jsonl"), b"{}\n").expect("新建");
    match nf.create(&d.join("s.jsonl"), b"other") {
        Err(NewFileFail::Create(e)) => assert_eq!(e.kind(), io::ErrorKind::AlreadyExists),
        other => panic!("已在必须拒在建那一步：{other:?}"),
    }
    assert_eq!(std::fs::read(d.join("s.jsonl")).unwrap(), b"{}\n");
    let _ = std::fs::remove_dir_all(&d);
}

/// 改名顶掉一份别的程序开着的文件（Windows）：说「被占用」，不说「无权限」。
/// 错号（Windows 的）→ 归哪一类是纯函数：共享冲突 32 · 锁冲突 33 本身就是占用；拒绝访问 5 而目标此刻按写打开回的是 32 / 33 ⇒ 也是占用
/// （换名到独占着的文件，系统回的是拒绝访问）；拒绝访问而目标打得开（真的没权限）· 别的错号 ⇒ 照原错。期望手写。
#[test]
fn replacing_a_file_another_program_holds_is_busy_not_denied() {
    use std::io::ErrorKind as K;
    let kind = |rename: i32, probe: Option<i32>| super::super::replace_refusal(rename, probe);
    assert_eq!(kind(32, None), Some(K::ResourceBusy));
    assert_eq!(kind(33, None), Some(K::ResourceBusy));
    assert_eq!(kind(5, Some(32)), Some(K::ResourceBusy));
    assert_eq!(kind(5, Some(33)), Some(K::ResourceBusy));
    assert_eq!(kind(5, None), None, "目标打得开 ⇒ 真的没权限");
    assert_eq!(
        kind(5, Some(5)),
        None,
        "目标按写也打不开、回的是拒绝访问 ⇒ 真的没权限"
    );
    assert_eq!(kind(2, Some(32)), None, "别的错号不探");
    // 原因词照它走：被占用那一类说「被占用」。
    assert_eq!(
        copy_core::reason::io_reason(K::ResourceBusy),
        copy_core::copy_text("reason.io.busy", &[])
    );
}

/// 改权限不跟链接：普通文件与目录改得上；那一格是链接 ⇒ 失败，链接指着的那一份权限不动。
#[cfg(unix)]
#[test]
fn set_permissions_changes_the_cell_and_refuses_a_link() {
    use std::os::unix::fs::PermissionsExt as _;
    let d = test_home::scratch("chmod");
    let face = Face::issue();
    let mode =
        |p: &std::path::Path| std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777;
    std::fs::write(d.join("f"), b"x").unwrap();
    std::fs::set_permissions(d.join("f"), Perms::from_mode(0o644)).unwrap();
    face.set_permissions(
        &Resolved::vouch(d.join("f")).unwrap(),
        Perms::from_mode(0o751),
    )
    .expect("改普通文件");
    assert_eq!(mode(&d.join("f")), 0o751);
    std::fs::create_dir(d.join("sub")).unwrap();
    face.set_permissions(
        &Resolved::vouch(d.join("sub")).unwrap(),
        Perms::from_mode(0o700),
    )
    .expect("改目录");
    assert_eq!(mode(&d.join("sub")), 0o700);
    std::os::unix::fs::symlink(d.join("f"), d.join("l")).unwrap();
    face.set_permissions(
        &Resolved::vouch(d.join("l")).unwrap(),
        Perms::from_mode(0o600),
    )
    .expect_err("改权限穿过了链接");
    assert_eq!(mode(&d.join("f")), 0o751, "链接指着的那一份被改了");
    std::os::unix::fs::symlink(d.join("gone"), d.join("dangling")).unwrap();
    face.set_permissions(
        &Resolved::vouch(d.join("dangling")).unwrap(),
        Perms::from_mode(0o600),
    )
    .expect_err("断链也要失败");
    let _ = std::fs::remove_dir_all(&d);
}

/// 「不跟链接」自己做，不交给 `fchmodat` 的 `AT_SYMLINK_NOFOLLOW`：glibc 2.32 以前带这个标志一律回 EOPNOTSUPP，
/// 链 glibc 的后端在旧发行版上每次改权限都失败。改权限那一下只经 `O_PATH` 开那一格 ＋ `/proc/self/fd/<N>`。
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn chmod_does_its_own_nofollow_instead_of_the_fchmodat_flag() {
    let src = include_str!("../../../../src/backend/platform/fs/face.rs");
    let start = src.find("pub(super) fn chmod(").expect("找不到 at::chmod");
    let body = &src[start..];
    let body = &body[..body.find("\n    }\n").expect("at::chmod 的结尾")];
    assert!(
        !body.contains("AT_SYMLINK_NOFOLLOW"),
        "at::chmod 仍带 AT_SYMLINK_NOFOLLOW"
    );
    assert!(!body.contains("fchmodat"), "at::chmod 仍调 fchmodat");
    assert!(
        body.contains("O_PATH") && body.contains("/proc/self/fd/"),
        "at::chmod 没走 O_PATH ＋ /proc"
    );
}

/// 没挂 /proc 的环境（容器 · chroot）：经 `/proc/self/fd/<N>` 改权限那一步打不开，原因照实说「不支持」，
/// 不说成「路径不存在」（那一格开着、好好的在）。造法：把那一路的根换成一个不在的目录。权限原样不动。
#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn chmod_without_proc_says_unsupported_not_missing() {
    use std::os::unix::fs::PermissionsExt as _;
    let d = test_home::scratch("chmod-noproc");
    std::fs::write(d.join("f"), b"x").unwrap();
    std::fs::set_permissions(d.join("f"), Perms::from_mode(0o644)).unwrap();
    let cell: std::os::fd::OwnedFd = std::fs::File::open(d.join("f")).unwrap().into();
    let gone = d.join("no-proc/self/fd/");
    let err = super::at::chmod_held(&cell, 0o600, gone.to_str().unwrap())
        .expect_err("那一路打不开还改成了");
    assert_eq!(err.kind(), io::ErrorKind::Unsupported, "{err}");
    assert_eq!(
        copy_core::reason::io_reason(err.kind()),
        copy_core::copy_text("machineState.word.unsupported", &[])
    );
    let mode = std::fs::metadata(d.join("f")).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o644);
    let _ = std::fs::remove_dir_all(&d);
}
