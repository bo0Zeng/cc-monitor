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
    let at = Resolved::vouch(d.join("a.txt"));
    face.create_new(&at)
        .and_then(|mut f| f.write_all(b"one"))
        .expect("新建");
    let again = face.create_new(&at).expect_err("已在必须拒");
    assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"one");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(d.join("elsewhere"), d.join("l")).unwrap();
        face.create_new(&Resolved::vouch(d.join("l")))
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
        .overwrite(&Resolved::vouch(d.join("l")))
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
