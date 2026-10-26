//! 编译期交一个变量 `CC_MONITOR_VERSION`：这份二进制出自哪一版，取仓根 `package.json` 顶层那行 `"version": "X.Y.Z",`
//! （发版改的就是它；手机的 versionName 读同一行）。认不出 ⇒ 编不过，不填默认值。

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo 没给 CARGO_MANIFEST_DIR");
    let path = Path::new(&manifest).join("../../package.json");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("读不到仓根 package.json（{}）：{e}", path.display()));
    let version = top_level_version(&text).unwrap_or_else(|| {
        panic!(
            "仓根 package.json 里认不出顶层那行 `\"version\": \"X.Y.Z\",`（{}）",
            path.display()
        )
    });
    println!("cargo:rustc-env=CC_MONITOR_VERSION={version}");
}

/// 顶层（缩进两格）那行 `"version": "…"` 的值；值得是 `数字.数字.数字`，可带 `-` 后缀。
fn top_level_version(text: &str) -> Option<&str> {
    let line = text
        .lines()
        .find(|l| l.starts_with("  \"version\"") && !l.starts_with("   "))?;
    let rest = line.split_once(':')?.1.trim().trim_end_matches(',');
    let v = rest.strip_prefix('"')?.strip_suffix('"')?;
    let core = v.split('-').next()?;
    let ok = core.split('.').count() == 3
        && core
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    ok.then_some(v)
}
