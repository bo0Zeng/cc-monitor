//! 门禁测的就是发出去的那份（后端）：判据构建与发布构建解析出的 `serde_json` 特性集相同。
//!
//! 测试构建会把 dev-dependency 打开的特性统一进来，发布构建没有它们 ⇒ 判据会在一份行为不同的二进制上绿。
//! `cargo tree --frozen`（不联网、不写锁、不碰 target）现取：全部边 vs 普通与构建期依赖。

#![cfg(test)]

use std::collections::BTreeSet;

fn serde_json_features(extra: &[&str]) -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "--frozen", "-i", "serde_json", "--depth", "0"])
        .args(["--prefix", "none", "--format", "{p}|{f}"])
        .args(extra)
        .output()
        .expect("跑不动 `cargo tree`");
    assert!(
        out.status.success(),
        "`cargo tree {extra:?}` 退出码 {:?}：{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let got: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with("serde_json "))
        .map(str::to_string)
        .collect();
    assert!(
        !got.is_empty(),
        "`cargo tree {extra:?}` 一份 serde_json 都没取到"
    );
    got
}

#[test]
fn the_backend_test_build_resolves_serde_json_like_the_release_build() {
    assert_eq!(
        serde_json_features(&[]),
        serde_json_features(&["-e", "normal,build"]),
        "后端的判据构建与发布构建的 serde_json 特性不同 —— 多半是某个 dev-dependency 打开了特性"
    );
}
