//! 要求：新传上去的那一份后端自己核字节、再原子换进落点（SFTP 只写暂存区，落点那一下只有这里）。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | P1 | 暂存区里一份自报这一版的 ⇒ 换进落点（盖掉旧的）、暂存件不在了、落点那层目录只给本人 | 真目录 |
//! | P2 | 字节自报的不是这一版 ⇒ `not_self`、暂存件当场删、落点一个字节不动 | 真目录 |
//! | P3 | 不在暂存区里（落点上那一份自己跑了这条）⇒ `outside_staging`、那份文件不删、落点不动 | 真目录 |
//! | P4 | 部署起的那个暂存名被叫成这条子命令 ⇒ 进后端（不被当成一段配置） | 纯函数 |
//! | P5 | `--unplace-self`：暂存区里那一份删掉落点那一份（不认身份）、再删自己；落点本来不在 ⇒ `false`；不在暂存区里 ⇒ `outside_staging`、什么都不删 | 真目录 |

use super::*;
use std::path::{Path, PathBuf};

fn sandbox(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "ccm-place-self-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(base.join(relay_route_core::STAGING_DIR_REL)).expect("sandbox");
    base
}

/// 一份带身份戳的「后端」字节（戳恰一个）。
fn backend(id: &str) -> Vec<u8> {
    let mut b = b"\x7fELF-ish prefix ".to_vec();
    b.extend(
        format!(
            "{}{id}{}",
            deploy_contract::STAMP_OPEN,
            deploy_contract::STAMP_CLOSE
        )
        .bytes(),
    );
    b.extend(b" tail");
    b
}

fn put(home: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = home.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}

fn landing(home: &Path) -> Option<Vec<u8>> {
    std::fs::read(home.join(relay_route_core::BACKEND_LANDING_REL)).ok()
}

#[test]
fn p1_a_staged_copy_of_this_build_takes_the_landing() {
    let home = sandbox("p1");
    let own = Own::issue_at(&home);
    let stage = |bytes: &[u8]| {
        put(
            &home,
            &format!(
                "{}/cc-monitor-backend.1-2-3.new",
                relay_route_core::STAGING_DIR_REL
            ),
            bytes,
        )
    };
    // 那台还没有落点那层目录 ⇒ 建出来只给本人。
    let staged = stage(&backend("old"));
    place(&own, &staged, "old").expect("该换进落点");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let bin = home.join(relay_route_core::BACKEND_LANDING_REL);
        let m = std::fs::metadata(bin.parent().unwrap()).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o700);
    }
    // 落点上已有一份 ⇒ 盖掉它。
    let mine = backend("mine");
    let staged = stage(&mine);
    place(&own, &staged, "mine").expect("该换进落点");
    assert_eq!(landing(&home), Some(mine));
    assert!(!staged.exists(), "暂存件还在");
}

#[test]
fn p2_bytes_that_do_not_say_this_build_are_dropped() {
    let home = sandbox("p2");
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        &backend("old"),
    );
    let staged = put(
        &home,
        &format!(
            "{}/cc-monitor-backend.1-2-3.new",
            relay_route_core::STAGING_DIR_REL
        ),
        &backend("other"),
    );
    let own = Own::issue_at(&home);
    let (code, _) = place(&own, &staged, "mine").unwrap_err();
    assert_eq!(code, "not_self");
    assert!(!staged.exists(), "暂存件该当场删");
    assert_eq!(landing(&home), Some(backend("old")));
}

#[test]
fn p3_a_copy_outside_staging_is_refused_and_left_alone() {
    let home = sandbox("p3");
    let at_landing = put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        &backend("mine"),
    );
    let own = Own::issue_at(&home);
    let (code, _) = place(&own, &at_landing, "mine").unwrap_err();
    assert_eq!(code, "outside_staging");
    assert_eq!(landing(&home), Some(backend("mine")));
    // 暂存区底下再深一层也不算（只认恰在暂存区里的一份）。
    let deeper = put(
        &home,
        &format!(
            "{}/sub/cc-monitor-backend.1-2-3.new",
            relay_route_core::STAGING_DIR_REL
        ),
        &backend("mine"),
    );
    let (code, _) = place(&own, &deeper, "mine").unwrap_err();
    assert_eq!(code, "outside_staging");
    assert!(deeper.exists());
}

#[test]
fn p4_the_staged_name_routes_to_the_backend() {
    let name = ssh_dial::sftp::staged_name(deploy::STAGED_STEM);
    let argv0 = format!("/home/x/{}/{name}", relay_route_core::STAGING_DIR_REL);
    let args: Vec<String> = vec!["--".into(), "--place-self".into()];
    assert_eq!(
        crate::control::ccm::route(&argv0, &args),
        crate::control::ccm::Entry::Backend(vec!["--place-self".into()])
    );
}

#[test]
fn p5_a_staged_copy_removes_the_landing_and_itself() {
    let home = sandbox("p5");
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        b"no stamp at all",
    );
    let staged_rel = format!(
        "{}/cc-monitor-backend.1-2-3.new",
        relay_route_core::STAGING_DIR_REL
    );
    let own = Own::issue_at(&home);
    let staged = put(&home, &staged_rel, &backend("any"));
    assert_eq!(unplace(&own, &staged), Ok(true));
    assert_eq!(landing(&home), None);
    assert!(!staged.exists(), "暂存件还在");
    let staged = put(&home, &staged_rel, &backend("any"));
    assert_eq!(unplace(&own, &staged), Ok(false), "落点本来就不在");
    assert!(!staged.exists(), "暂存件还在");
    // 不在暂存区里（落点上那一份自己跑了这条）⇒ 拒、一个字节不动。
    let at_landing = put(&home, relay_route_core::BACKEND_LANDING_REL, b"x");
    let (code, _) = unplace(&own, &at_landing).unwrap_err();
    assert_eq!(code, "outside_staging");
    assert!(at_landing.exists());
}
