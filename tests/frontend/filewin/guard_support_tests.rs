//! （住址反空真）—— 本包那三个住址各点名一样盘上真有的东西，指错地方当场红。
use super::*;

#[test]
fn every_address_points_at_something_we_can_name() {
    for (name, dir, probe) in [
        ("crate_root", crate_root(), "Cargo.toml"),
        ("crate_src_root", crate_src_root(), "lib.rs"),
        ("repo_root", repo_root(), "package.json"),
    ] {
        assert!(
            dir.join(probe).exists(),
            "{name}() = {dir:?} 下没有 {probe} —— 这个住址指错了地方"
        );
    }
}
