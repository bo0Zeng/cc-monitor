//! 托管进程从哪个程序跑（A3-3-23）：按版本的副本 —— 没有就复制一份、有了不再复制、别的版本顺手删、正被用着的删不掉就留着。

use super::super::run::tests::sandbox_own;
use super::*;

fn fake_me(dir: &std::path::Path, body: &[u8]) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).expect("建夹具目录");
    let p = dir.join(format!("me{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&p, body).expect("写夹具程序");
    p
}

#[test]
fn the_first_start_copies_itself_into_the_version_dir_and_later_ones_reuse_it() {
    let (own, data) = sandbox_own("bin-copy");
    let me = fake_me(&data.join("fixture"), b"build one");
    let got = versioned_copy(&own, "9.9.9", &me).expect("复制得了");
    let want = data
        .join("hosts")
        .join(".bin")
        .join("9.9.9")
        .join(format!("ccm{}", std::env::consts::EXE_SUFFIX));
    assert_eq!(got, want);
    assert_eq!(std::fs::read(&got).expect("副本在"), b"build one");
    // 同一个版本再起一次：已有就用它，不再复制（正跑着的那一份在 Windows 上也盖不掉）。
    std::fs::write(&me, b"build two").expect("改夹具");
    assert_eq!(versioned_copy(&own, "9.9.9", &me).expect("再要一次"), want);
    assert_eq!(std::fs::read(&want).expect("副本在"), b"build one");
}

#[test]
fn other_versions_are_swept_by_the_next_start() {
    let (own, data) = sandbox_own("bin-sweep");
    let me = fake_me(&data.join("fixture"), b"new");
    let old = data.join("hosts").join(".bin").join("1.0.0");
    std::fs::create_dir_all(&old).expect("建旧版本目录");
    std::fs::write(
        old.join(format!("ccm{}", std::env::consts::EXE_SUFFIX)),
        b"old",
    )
    .expect("写旧副本");
    versioned_copy(&own, "2.0.0", &me).expect("复制得了");
    assert!(!old.exists(), "没人用的旧版本删了");
    assert!(data.join("hosts").join(".bin").join("2.0.0").exists());
}

#[test]
fn a_version_name_that_is_not_one_plain_name_is_refused() {
    let (own, data) = sandbox_own("bin-bad");
    let me = fake_me(&data.join("fixture"), b"x");
    assert!(versioned_copy(&own, "../x", &me).is_err());
    assert!(versioned_copy(&own, "", &me).is_err());
}

/// Windows：旧版本那一份正被一个托管进程跑着（系统不许删跑着的程序）⇒ 留着；它退了，下一次起托管时删掉。
#[cfg(windows)]
#[test]
fn an_old_copy_still_running_is_left_until_it_exits() {
    let (own, data) = sandbox_own("bin-busy");
    let me = fake_me(&data.join("fixture"), b"new");
    let old = data.join("hosts").join(".bin").join("1.0.0");
    std::fs::create_dir_all(&old).expect("建旧版本目录");
    let running = old.join("ccm.exe");
    std::fs::copy(r"C:\Windows\System32\ping.exe", &running).expect("放一份能跑的旧副本");
    let n = (4 * guard_core::wall::CEILING_SECS).to_string();
    let mut child = std::process::Command::new(&running)
        .args(["-n", &n, "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("跑旧副本");
    versioned_copy(&own, "2.0.0", &me).expect("复制得了");
    assert!(running.exists(), "正跑着的旧副本被删了");
    let _ = child.kill();
    let _ = child.wait();
    versioned_copy(&own, "2.0.0", &me).expect("再起一次");
    assert!(!old.exists(), "它退了之后下一次起托管时删掉");
}

/// A3-3-34：起托管那一下用哪个程序 —— Windows 上是按版本的副本（跑着的 `ccm.exe` 盖不掉），别处就是被叫成的那个词。
#[test]
fn the_host_program_is_the_versioned_copy_only_where_running_programs_are_locked() {
    let (own, data) = sandbox_own("bin-pick");
    let me = fake_me(&data.join("fixture"), b"me");
    let got = host_program(&own, "ccm", &me).expect("定得了");
    if crate::platform::child::RUNNING_PROGRAM_IS_LOCKED {
        let want = data
            .join("hosts")
            .join(".bin")
            .join(env!("CARGO_PKG_VERSION"))
            .join(program_name());
        assert_eq!(std::path::PathBuf::from(&got), want);
    } else {
        assert_eq!(got, "ccm");
    }
}
