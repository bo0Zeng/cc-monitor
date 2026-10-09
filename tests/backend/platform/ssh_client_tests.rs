//! 本机 ssh 客户端在哪：四种情况各一条（在 PATH · 不在 PATH 但在 System32\OpenSSH · 真没有 · 查本身出错），
//! 外加「查出错但别处找到了 ⇒ 照样用找到的那个」与真盘上那一格。查文件那一下注入（`probe`），不碰这台机器的真 PATH。

use super::*;
use std::ffi::OsStr;
use std::io;
use std::path::Path;

/// 按一张「哪些路径有那个文件 · 哪些路径一查就出错」的表答。
fn probe_with<'a>(
    present: &'a [&'a str],
    broken: &'a [&'a str],
) -> impl Fn(&Path) -> io::Result<bool> + 'a {
    move |p: &Path| {
        let s = p.to_string_lossy();
        if broken.iter().any(|b| s == *b) {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        } else {
            Ok(present.iter().any(|h| s == *h))
        }
    }
}

fn sysroot_ssh() -> String {
    Path::new("/sr")
        .join(WINDOWS_OPENSSH_REL)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn windows_on_path_is_found_by_its_full_path() {
    let got = locate_with(
        true,
        Some(OsStr::new("/sr")),
        Some(OsStr::new("/a:/tools")),
        &probe_with(&["/tools/ssh.exe"], &[]),
    );
    assert_eq!(got, SshClient::At("/tools/ssh.exe".into()));
}

#[test]
fn windows_system32_openssh_counts_even_when_path_lacks_it_and_wins_over_path() {
    let only_sysroot = locate_with(
        true,
        Some(OsStr::new("/sr")),
        Some(OsStr::new("/a")),
        &probe_with(&[&sysroot_ssh()], &[]),
    );
    assert_eq!(only_sysroot, SshClient::At(sysroot_ssh()));
    let both = locate_with(
        true,
        Some(OsStr::new("/sr")),
        Some(OsStr::new("/tools")),
        &probe_with(&[&sysroot_ssh(), "/tools/ssh.exe"], &[]),
    );
    assert_eq!(
        both,
        SshClient::At(sysroot_ssh()),
        "系统可选功能装的那一份先查"
    );
    // PATH 读不到也不妨碍：系统那一份在就用它。
    let no_path = locate_with(
        true,
        Some(OsStr::new("/sr")),
        None,
        &probe_with(&[&sysroot_ssh()], &[]),
    );
    assert_eq!(no_path, SshClient::At(sysroot_ssh()));
}

#[test]
fn truly_missing_only_when_every_place_was_checked_cleanly() {
    assert_eq!(
        locate_with(
            true,
            Some(OsStr::new("/sr")),
            Some(OsStr::new("/a:/b")),
            &probe_with(&[], &[]),
        ),
        SshClient::Missing
    );
    assert_eq!(
        locate_with(
            false,
            None,
            Some(OsStr::new("/usr/bin:/bin")),
            &probe_with(&["/usr/bin/ssh.exe"], &[]),
        ),
        SshClient::Missing,
        "POSIX 找的是无后缀的 ssh"
    );
}

#[test]
fn a_failing_check_is_unknown_not_missing() {
    let broken = locate_with(
        true,
        Some(OsStr::new("/sr")),
        Some(OsStr::new("/a:/locked")),
        &probe_with(&[], &["/locked/ssh.exe"]),
    );
    assert!(
        matches!(&broken, SshClient::Unknown(why) if why.contains("/locked/ssh.exe")),
        "{broken:?}"
    );
    // 读不到 PATH / SystemRoot ⇒ 没法说没有。
    assert!(matches!(
        locate_with(false, None, None, &probe_with(&[], &[])),
        SshClient::Unknown(_)
    ));
    assert!(matches!(
        locate_with(false, None, Some(OsStr::new("")), &probe_with(&[], &[])),
        SshClient::Unknown(_)
    ));
    assert!(matches!(
        locate_with(true, None, Some(OsStr::new("/a")), &probe_with(&[], &[])),
        SshClient::Unknown(_)
    ));
    // 出错的那一项之外找到了 ⇒ 照样用找到的那个。
    assert_eq!(
        locate_with(
            true,
            Some(OsStr::new("/sr")),
            Some(OsStr::new("/locked:/tools")),
            &probe_with(&["/tools/ssh.exe"], &["/locked/ssh.exe"]),
        ),
        SshClient::At("/tools/ssh.exe".into())
    );
}

#[test]
fn posix_on_path_finds_bare_ssh() {
    assert_eq!(
        locate_with(
            false,
            None,
            Some(OsStr::new("/opt/x:/usr/bin")),
            &probe_with(&["/usr/bin/ssh"], &[]),
        ),
        SshClient::At("/usr/bin/ssh".into())
    );
}

#[cfg(unix)]
#[test]
fn the_real_check_wants_a_runnable_file_and_reads_absence_as_absence() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("ssh-client-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let plain = dir.join("plain");
    std::fs::write(&plain, b"").unwrap();
    std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
    let runnable_file = dir.join("run");
    std::fs::write(&runnable_file, b"").unwrap();
    std::fs::set_permissions(&runnable_file, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!runnable(&plain).unwrap(), "没有执行位不算");
    assert!(runnable(&runnable_file).unwrap());
    assert!(
        !runnable(&dir.join("nope")).unwrap(),
        "不在就是不在，不是出错"
    );
    assert!(
        !runnable(&plain.join("under-a-file")).unwrap(),
        "父路径是文件也是不在"
    );
    assert!(!runnable(&dir).unwrap(), "目录不算");
    std::fs::remove_dir_all(&dir).unwrap();
}
