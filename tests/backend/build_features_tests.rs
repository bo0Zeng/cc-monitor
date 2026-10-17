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
        .unwrap_or_else(|| {
            panic!(
                "后端 Cargo.toml 没有 `[profile.release]` —— 发布档会走 cargo 默认（不按体积编）"
            )
        });
    let field = |k: &str| release.get(k).map(|v| v.to_string().trim().to_string());
    assert_eq!(
        field("opt-level").as_deref(),
        Some("\"s\""),
        "opt-level 要 \"s\"（\"z\" 冷扫慢，3 不按体积）"
    );
    assert_eq!(field("lto").as_deref(), Some("true"), "lto 要开");
    assert_eq!(
        field("codegen-units").as_deref(),
        Some("1"),
        "codegen-units 要 1"
    );
    assert_eq!(
        field("strip").as_deref(),
        Some("true"),
        "strip 要开（deb 那份本机后端曾带着 6.6 MB 符号）"
    );
    assert!(
        matches!(field("panic").as_deref(), None | Some("\"unwind\"")),
        "panic 实得 {:?}：后端发布档不许 abort —— 一个任务 panic 会带走整个常驻后端",
        field("panic")
    );
}

/// 测试档里给几样依赖开优化（10-15 量过）：快检的测试段，每个测试二进制的墙钟 ＝ 它最慢的那一条，而最慢的那几条
/// 慢在**依赖**不开优化 —— 后端轮换一致性的推演（s22 · s26 · s36）跑在沙箱里的 Rhai 上，壳那棵文件窗口的 hover 扫读与滚动区跑在 egui 上。
/// - 后端：rhai（＋ 它热路径上的 smartstring · ahash）开 3 ⇒ 后端测试二进制 89–93 s → 30–36 s、CPU 少四成六；冷编多一成。
/// - 壳：egui 那一族开 3 ⇒ 文件窗口测试二进制 40–42 s → 28–31 s、CPU 少四成；冷编多不到一成。
/// - 只给这几样、不给 `"*"`：全部依赖开 2 时测试一样快，但壳冷编的 CPU 涨到近四倍。工作区自己的 crate 照旧不开（内循环的增量编译不受影响）。
/// 判法：两棵 workspace 的 `[profile.dev.package.*]` 两向等于下面这张表，且每一项都在那棵的 `Cargo.lock` 里
/// （名字对不上时 cargo 只警告一声、什么都不做 ⇒ 改名 / 换掉依赖以后这条就红，不会安静地失效）。
#[test]
fn the_test_build_optimizes_the_dependencies_the_slowest_tests_run_on() {
    let backend = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let cases: [(&std::path::Path, &[&str]); 2] = [
        (backend, &["ahash", "rhai", "smartstring"]),
        (
            &backend.join("../frontend/shell"),
            &[
                "ab_glyph",
                "ab_glyph_rasterizer",
                "ecolor",
                "egui",
                "egui_extras",
                "emath",
                "epaint",
            ],
        ),
    ];
    for (dir, want) in cases {
        let read = |f: &str| {
            let p = dir.join(f);
            std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
        };
        let doc: toml_edit::DocumentMut = read("Cargo.toml").parse().expect("Cargo.toml 解不动");
        let lock: toml_edit::DocumentMut = read("Cargo.lock").parse().expect("Cargo.lock 解不动");
        let locked: BTreeSet<String> = lock
            .get("package")
            .and_then(|p| p.as_array_of_tables())
            .expect("Cargo.lock 里没有 [[package]] —— 量具坏了")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
            .collect();
        let overrides = doc
            .get("profile")
            .and_then(|p| p.get("dev"))
            .and_then(|d| d.get("package"))
            .and_then(|p| p.as_table_like());
        let got: BTreeSet<String> = overrides
            .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();
        let want: BTreeSet<String> = want.iter().map(|s| s.to_string()).collect();
        assert_eq!(got, want, "{}：测试档开优化的依赖与表对不上", dir.display());
        for name in &got {
            assert!(
                locked.contains(name),
                "{}：`[profile.dev.package.{name}]` 不在 Cargo.lock 里 —— cargo 只警告、不生效",
                dir.display()
            );
            let level = overrides
                .and_then(|t| t.get(name))
                .and_then(|v| v.get("opt-level"))
                .map(|v| v.to_string().trim().to_string());
            assert_eq!(
                level.as_deref(),
                Some("3"),
                "{}：{name} 的 opt-level",
                dir.display()
            );
        }
    }
}
