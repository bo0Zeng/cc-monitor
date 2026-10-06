//! 门禁测的就是发出去的那份：判据构建与发布构建解析出的 `serde_json` 特性集相同。
//!
//! 测试构建会把 dev-dependency 打开的特性统一进整个 workspace（例如 `preserve_order` 把 `Map` 换成插入序），
//! 发布构建没有它们 ⇒ 判据会在一份行为不同的二进制上绿。用 `cargo tree --frozen`（不联网、不写锁、不碰 target）
//! 现取两种构建的特性集比相等。壳那个 workspace：判据构建 = `--workspace`（门禁 `cargo` 格），发布构建 =
//! `-p monitor` 的普通与构建期依赖（两个二进制都在这个包里）；后端：全部边 vs 普通与构建期依赖。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("guard-core 的上三级 = 仓根")
}

/// `dir` 那个 workspace 在 `extra` 那种构建下解析出的每一份 `serde_json`（`名 版本|特性`）。
fn serde_json_features(dir: &str, extra: &[&str]) -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .current_dir(repo().join(dir))
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
fn the_shell_test_build_resolves_serde_json_like_the_release_build() {
    assert_eq!(
        serde_json_features("src/frontend/shell", &["--workspace"]),
        serde_json_features("src/frontend/shell", &["-p", "monitor", "-e", "normal,build"]),
        "壳的判据构建（--workspace）与发布构建（-p monitor）的 serde_json 特性不同 —— 多半是某个 dev-dependency 打开了特性"
    );
}

#[test]
fn the_backend_test_build_resolves_serde_json_like_the_release_build() {
    assert_eq!(
        serde_json_features("src/backend", &[]),
        serde_json_features("src/backend", &["-e", "normal,build"]),
        "后端的判据构建与发布构建的 serde_json 特性不同 —— 多半是某个 dev-dependency 打开了特性"
    );
}
