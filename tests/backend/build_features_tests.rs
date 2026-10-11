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

/// 发布档按体积编（size1 报告 ①）：opt-level "s" · lto · codegen-units 1 · strip；panic 刻意不改 abort。
///
/// - 不写这一段时后端走 cargo 默认（opt-level 3、不开 LTO、16 个编译单元、不去符号），每份大约多 40%，而它在 monitor 里内嵌三份。
/// - "s" 不是 "z"：量过 "z" 冷扫慢 1.3–1.9 倍（报告 ⑦）。
/// - panic 留 unwind：常驻后端里一个任务 panic 靠监督接住；abort 会把整个进程带走（报告 ⑧）。
#[test]
fn the_backend_release_profile_is_built_for_size_and_keeps_unwinding() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("读不到 {}：{e}", manifest.display()));
    let doc: toml_edit::DocumentMut = text.parse().expect("后端 Cargo.toml 解不动");
    let release = doc
        .get("profile")
        .and_then(|p| p.get("release"))
        .unwrap_or_else(|| panic!("后端 Cargo.toml 没有 `[profile.release]` —— 发布档会走 cargo 默认（不按体积编）"));
    let field = |k: &str| release.get(k).map(|v| v.to_string().trim().to_string());
    assert_eq!(field("opt-level").as_deref(), Some("\"s\""), "opt-level 要 \"s\"（\"z\" 冷扫慢，3 不按体积）");
    assert_eq!(field("lto").as_deref(), Some("true"), "lto 要开");
    assert_eq!(field("codegen-units").as_deref(), Some("1"), "codegen-units 要 1");
    assert_eq!(field("strip").as_deref(), Some("true"), "strip 要开（deb 那份本机后端曾带着 6.6 MB 符号）");
    assert!(
        matches!(field("panic").as_deref(), None | Some("\"unwind\"")),
        "panic 实得 {:?}：后端发布档不许 abort —— 一个任务 panic 会带走整个常驻后端",
        field("panic")
    );
}
