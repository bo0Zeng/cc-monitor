use std::path::Path;

fn main() {
    emit_daemon_build_id();
    emit_daemon_capabilities();
    embed_daemons();
    tauri_build::build()
}

/// 从 daemon 源码（`remote-daemon-proto/src/main.rs`）提取 `const BUILD_ID`，emit 成编译期
/// env `DAEMON_BUILD_ID`，让 monitor 的 `EXPECTED_DAEMON_BUILD_ID` 与内嵌二进制的 build_id
/// **单一事实源**（SS-B：消除 F06 时的手工同步）。
fn emit_daemon_build_id() {
    let main_rs = Path::new("..")
        .join("remote-daemon-proto")
        .join("src")
        .join("main.rs");
    println!("cargo:rerun-if-changed={}", main_rs.display());
    let build_id = std::fs::read_to_string(&main_rs)
        .ok()
        .and_then(|s| extract_build_id(&s))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DAEMON_BUILD_ID={build_id}");
}

/// 从源码里抠出 `const BUILD_ID: &str = "<x>";` 的 `<x>`。
fn extract_build_id(src: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains("const BUILD_ID"))?;
    let start = line.find('"')? + 1;
    let rel_end = line[start..].find('"')?;
    Some(line[start..start + rel_end].to_string())
}

/// F66（#58③）：从 daemon 源码提取 `const CAPABILITIES`，emit 成编译期 env
/// `DAEMON_CAPABILITIES`（逗号分隔），让 monitor 的 `embedded_daemon_capabilities()` 与
/// daemon 声明的能力**单一事实源**——同 build_id 的 SS-B，杜绝手工同步债（审计 B1/S1：
/// 否则两份手抄常量漂移时，乐观路径可能声明当前 daemon 不剥离的 flag → §26 死循环窄窗）。
fn emit_daemon_capabilities() {
    let main_rs = Path::new("..")
        .join("remote-daemon-proto")
        .join("src")
        .join("main.rs");
    // rerun-if-changed 已由 emit_daemon_build_id 对同一文件登记，无需重复。
    let caps = std::fs::read_to_string(&main_rs)
        .ok()
        .and_then(|s| extract_capabilities(&s))
        .unwrap_or_default();
    println!("cargo:rustc-env=DAEMON_CAPABILITIES={caps}");
}

/// 从源码里抠出 `const CAPABILITIES: &[&str] = &["a", "b"];` 的所有字符串，逗号拼接
/// （`a,b`）。**取 `=` 右侧再抠数组**——否则 `line.find('[')` 会命中类型标注 `&[&str]`
/// 的 `[`（里面 `&str` 无引号 → 抠成空，此坑由 `embedded_capabilities_single_source_wired`
/// 测试抓出）。
fn extract_capabilities(src: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains("const CAPABILITIES"))?;
    let rhs = &line[line.find('=')? + 1..]; // 跳过 `: &[&str]` 类型标注里的 `[`
    let inner_start = rhs.find('[')? + 1;
    let inner_end = rhs[inner_start..].find(']')? + inner_start;
    let inner = &rhs[inner_start..inner_end];
    let mut tokens = Vec::new();
    let mut rest = inner;
    while let Some(q1) = rest.find('"') {
        let after = &rest[q1 + 1..];
        let q2 = after.find('"')?;
        tokens.push(&after[..q2]);
        rest = &after[q2 + 1..];
    }
    Some(tokens.join(","))
}

/// 把交叉编译好的 musl daemon 二进制
/// （`src-tauri/embedded-daemons/cc-monitor-remote-<arch>`）复制进 OUT_DIR 并置
/// `embedded_daemons` cfg；任一缺失则不置 cfg（`sftp::daemon_binary` 返回 None → 自动部署
/// 优雅 no-op，沿用手动部署）。二进制由 `cargo zigbuild --target *-unknown-linux-musl` 产出后
/// 放进 `embedded-daemons/`（见 doc/REMOTE-PHASE0-DEPLOY 的 F08b 段）。
fn embed_daemons() {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_daemons)");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dir = Path::new("embedded-daemons");
    // staleness 安全网（审计 SUGGESTION-1）：daemon 源码 mtime，用于提示「bump BUILD_ID 后
    // 忘了 re-zigbuild」——否则内嵌旧二进制 build_id 与源码不符 → 永不收敛的重复部署。
    let src_mtime = std::fs::metadata(
        Path::new("..")
            .join("remote-daemon-proto")
            .join("src")
            .join("main.rs"),
    )
    .and_then(|m| m.modified())
    .ok();
    let mut all = true;
    for arch in ["x86_64", "aarch64"] {
        let src = dir.join(format!("cc-monitor-remote-{arch}"));
        println!("cargo:rerun-if-changed={}", src.display());
        // Batch9 E2E 发现：编译器可把 BUILD_ID 优化成立即数指令（字符串在字节里
        // **不连续**），运行时 bytes_contain 启发式会误拒正品二进制。根治 = 旁挂
        // `.build_id` 清单文件（构建放置二进制时一并写入，= 字节的真实身份）：
        // 有清单 → env DAEMON_EMBEDDED_ID_<arch>；无 → 空串（运行时回退启发式）。
        let manifest = dir.join(format!("cc-monitor-remote-{arch}.build_id"));
        println!("cargo:rerun-if-changed={}", manifest.display());
        let embedded_id = std::fs::read_to_string(&manifest)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        println!(
            "cargo:rustc-env=DAEMON_EMBEDDED_ID_{}={embedded_id}",
            arch.to_uppercase()
        );
        if src.exists() {
            if let (Ok(bin_mtime), Some(sm)) = (
                std::fs::metadata(&src).and_then(|m| m.modified()),
                src_mtime,
            ) {
                if bin_mtime < sm {
                    println!(
                        "cargo:warning=内嵌 daemon {arch} 比 daemon 源码旧——若刚 bump 了 BUILD_ID，请重跑 `cargo zigbuild --target {arch}-unknown-linux-musl` 并更新 embedded-daemons/"
                    );
                }
            }
            let dst = Path::new(&out).join(format!("daemon-{arch}"));
            std::fs::copy(&src, &dst).expect("copy embedded daemon binary");
        } else {
            all = false;
        }
    }
    if all {
        println!("cargo:rustc-cfg=embedded_daemons");
    }
}
