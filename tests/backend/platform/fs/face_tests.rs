use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-face-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// `O_EXCL` 新建：已在 ⇒ 拒、原样不动；最后一段是链接 ⇒ 也拒（不穿过链接去建）。
#[test]
fn create_new_refuses_an_existing_name_and_a_link() {
    let d = scratch("create");
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
    let d = scratch("over");
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
    let d = scratch("newfile");
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
