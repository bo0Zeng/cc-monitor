//! 要求：新传上去的那一份后端自己核字节、再原子换进落点（SFTP 只写暂存区，落点那一下只有这里）。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | P1 | 暂存区里一份自报这一版的 ⇒ 换进落点（盖掉旧的）、暂存件不在了、落点那层目录只给本人 | 真目录 |
//! | P2 | 字节自报的不是这一版 ⇒ `not_self`、暂存件当场删、落点一个字节不动 | 真目录 |
//! | P3 | 不在暂存区里（落点上那一份自己跑了这条）⇒ `outside_staging`、那份文件不删、落点不动 | 真目录 |
//! | P4 | 部署起的那个暂存名被叫成这条子命令 ⇒ 进后端（不被当成一段配置） | 纯函数 |
//! | P5 | `--unplace-self`：暂存区里那一份删掉落点那一份（不认身份）、再删自己；落点本来不在 ⇒ `false`；不在暂存区里 ⇒ `outside_staging`、什么都不删 | 真目录 |
//! | P6 | 换版本：落点上原来那一份留进 `bin/kept/ccm-<它自报的身份>`、字节一个不变、只给本人；头一回装（落点不在）不留；身份读不出 ⇒ `ccm-unstamped` | 真目录 |
//! | P7 | 留存区最多 [`KEPT_MAX`] 份、过 [`KEPT_MAX_AGE_SECS`] 删：留完收拾，只剩最新的那几份 | 真目录 |
//! | P8 | 留不成（那一格被一个目录占着）⇒ `place_failed`、不换、落点一个字节不动 | 真目录 |
//! | P9 | 卸载那一趟连留存区里那几份一起删 | 真目录 |

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

/// 留存区（相对家目录）：落点那层 `bin` 底下的那一层。
fn kept_rel() -> String {
    let bin = Path::new(relay_route_core::BACKEND_LANDING_REL)
        .parent()
        .unwrap();
    format!(
        "{}/{}",
        bin.display(),
        relay_route_core::BACKEND_KEPT_DIR_NAME
    )
}

fn kept(home: &Path, name: &str) -> PathBuf {
    home.join(kept_rel()).join(name)
}

fn kept_names(home: &Path) -> Vec<String> {
    let mut n: Vec<String> = Ro::read_dir(home.join(kept_rel()))
        .map(|d| {
            d.filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    n.sort();
    n
}

fn staged_at(home: &Path, bytes: &[u8]) -> PathBuf {
    put(
        home,
        &format!(
            "{}/cc-monitor-backend.1-2-3.new",
            relay_route_core::STAGING_DIR_REL
        ),
        bytes,
    )
}

fn age(p: &Path, secs_ago: u64) {
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago);
    std::fs::File::open(p).unwrap().set_modified(t).unwrap();
}

#[test]
fn p6_the_copy_it_replaces_is_kept_byte_for_byte() {
    let home = sandbox("p6");
    let own = Own::issue_at(&home);
    // 头一回装：落点不在 ⇒ 不留。
    place(&own, &staged_at(&home, &backend("old")), "old").expect("该换进落点");
    assert!(kept_names(&home).is_empty(), "头一回装不该留");
    // 落点上那一份有身份；换上新的一份 ⇒ 那一份原样留着、只给本人。
    place(&own, &staged_at(&home, &backend("mine")), "mine").expect("该换进落点");
    assert_eq!(landing(&home), Some(backend("mine")));
    assert_eq!(kept_names(&home), vec!["ccm-old".to_string()]);
    assert_eq!(
        std::fs::read(kept(&home, "ccm-old")).ok(),
        Some(backend("old"))
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let m = std::fs::metadata(kept(&home, "ccm-old")).unwrap();
        assert_eq!(m.permissions().mode() & 0o077, 0, "留下的那一份只给本人");
    }
    // 身份读不出的那一份 ⇒ `ccm-unstamped`。
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        b"no stamp at all",
    );
    place(&own, &staged_at(&home, &backend("mine")), "mine").expect("该换进落点");
    assert_eq!(
        std::fs::read(kept(&home, "ccm-unstamped")).ok(),
        Some(b"no stamp at all".to_vec())
    );
}

#[test]
fn p7_kept_copies_are_capped_and_expire() {
    let home = sandbox("p7");
    let own = Own::issue_at(&home);
    // 留存区里已有：一份过期的 · 几份没过期的（越往后越新）。
    put(&home, &format!("{}/ccm-ancient", kept_rel()), b"a");
    age(&kept(&home, "ccm-ancient"), KEPT_MAX_AGE_SECS + 3600);
    let fresh: Vec<String> = (0..KEPT_MAX).map(|i| format!("ccm-v{i}")).collect();
    for (i, n) in fresh.iter().enumerate() {
        put(&home, &format!("{}/{n}", kept_rel()), b"v");
        age(&kept(&home, n), 1000 * (KEPT_MAX - i) as u64);
    }
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        &backend("prev"),
    );
    place(&own, &staged_at(&home, &backend("mine")), "mine").expect("该换进落点");
    let mut want: Vec<String> = fresh[1..].to_vec();
    want.push("ccm-prev".into());
    want.sort();
    assert_eq!(kept_names(&home), want, "过期的删、最旧的那一份让位");
}

#[test]
fn p8_no_swap_when_the_old_copy_cannot_be_kept() {
    let home = sandbox("p8");
    let own = Own::issue_at(&home);
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        &backend("old"),
    );
    std::fs::create_dir_all(kept(&home, "ccm-old").join("x")).unwrap();
    let staged = staged_at(&home, &backend("mine"));
    let (code, _) = place(&own, &staged, "mine").unwrap_err();
    assert_eq!(code, "place_failed");
    assert_eq!(landing(&home), Some(backend("old")), "落点该一个字节不动");
}

#[test]
fn p9_uninstall_clears_the_kept_copies() {
    let home = sandbox("p9");
    let own = Own::issue_at(&home);
    put(
        &home,
        relay_route_core::BACKEND_LANDING_REL,
        &backend("old"),
    );
    put(&home, &format!("{}/ccm-older", kept_rel()), b"o");
    let staged = staged_at(&home, &backend("any"));
    assert_eq!(unplace(&own, &staged), Ok(true));
    assert!(kept_names(&home).is_empty(), "卸载该连留存区一起删");
}
