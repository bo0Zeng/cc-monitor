//! `attach_stage.rs` 的判据 —— 壳只把那一份字节放进本机暂存区、回那条路径（分块 · 问那台都是本机后端的事）。
//!
//! | 判据 | 钉的那一形 |
//! |---|---|
//! | [`the_bytes_land_in_the_staging_area_under_a_key_named_part`] | 落在暂存区里、名字是 `<键>.part`、字节原样；暂存区不在就建 |
//! | [`a_key_already_there_is_not_overwritten`] | 同一个键再写 ⇒ 拒，已在的那一份原样 |

use super::put_into;

fn staging(tag: &str) -> std::path::PathBuf {
    let at = test_home::sandbox_root()
        .join(format!("attach-stage-{tag}"))
        .join(".cc-monitor")
        .join("staging");
    let _ = std::fs::remove_dir_all(&at);
    at
}

#[test]
fn the_bytes_land_in_the_staging_area_under_a_key_named_part() {
    let dir = staging("land");
    let key = "ab".repeat(16);
    let at = put_into(&dir, &key, b"hello").expect("写得进");
    assert_eq!(at.parent(), Some(dir.as_path()), "落在暂存区里");
    assert_eq!(at.file_name().unwrap(), format!("{key}.part").as_str());
    assert_eq!(std::fs::read(&at).unwrap(), b"hello");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&at).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "只给本人");
    }
}

#[test]
fn a_key_already_there_is_not_overwritten() {
    let dir = staging("again");
    let key = "cd".repeat(16);
    put_into(&dir, &key, b"first").expect("头一次写得进");
    assert!(
        put_into(&dir, &key, b"second").is_err(),
        "同一个键再写 ⇒ 拒"
    );
    assert_eq!(
        std::fs::read(dir.join(format!("{key}.part"))).unwrap(),
        b"first"
    );
}
