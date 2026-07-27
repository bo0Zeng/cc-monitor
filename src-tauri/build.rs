use std::path::Path;

fn main() {
    emit_daemon_build_id();
    emit_daemon_capabilities();
    check_vendor_freshness();
    check_acct_iso_vendor_freshness();
    embed_daemons();
    tauri_build::build()
}

/// F5：vendored cc-acct-iso 过期软检查（SS-10「过期看得见」）。从 `VENDOR.md` 抠上游仓路径
/// （`~` 展开为 $HOME），若上游存在则比对三个脚本与 vendored 副本，不一致则 `cargo:warning`。
/// 上游缺席 → no-op（同 `check_vendor_freshness`：开发期上游领先副本是常态，软警告非硬失败）。
fn check_acct_iso_vendor_freshness() {
    let vendor_dir = Path::new("vendor/cc-acct-iso");
    let vendor_md = vendor_dir.join("VENDOR.md");
    println!("cargo:rerun-if-changed={}", vendor_md.display());
    println!("cargo:rerun-if-changed={}", vendor_dir.join(".vendor_id").display());
    // D 审计 S1/S5：指纹须覆盖**全部被部署文件**，故过期检查也逐个比这 6 个（不只 3 脚本）。
    // 顺序须与 VENDOR.md 菜谱 / `.vendor_id` 计算一致（自洽校验按同一顺序拼接）。
    const DEPLOYED: [&str; 6] = [
        "scripts/cc-acct-iso",
        "scripts/lib.sh",
        "scripts/cc-acct-iso-install.sh",
        "scripts/test/run-tests.sh",
        "SKILL.md",
        "examples/config",
    ];
    for f in DEPLOYED {
        println!("cargo:rerun-if-changed={}", vendor_dir.join(f).display());
    }

    // (a) 自洽校验：vendored 6 文件的 sha256 前 16 位是否等于 `.vendor_id`（防「改了 vendored
    //     脚本却忘了重算指纹」→ 远端 Skip 不更新而 build 期无声）。用 sha256sum shell-out（同
    //     VENDOR.md 菜谱），缺 sha256sum 则跳过该项。
    if let Ok(recorded) = std::fs::read_to_string(vendor_dir.join(".vendor_id")) {
        let recorded = recorded.trim();
        let cat_cmd = format!(
            "cat {} | sha256sum | cut -c1-16",
            DEPLOYED
                .iter()
                .map(|f| format!("'{}'", vendor_dir.join(f).display()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if let Ok(out) = std::process::Command::new("sh").arg("-c").arg(&cat_cmd).output() {
            let computed = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !computed.is_empty() && computed != recorded {
                println!(
                    "cargo:warning=vendor cc-acct-iso 指纹不自洽:.vendor_id={recorded} 但脚本实际 sha={computed}。改了 vendored 文件后请按 VENDOR.md 菜谱重算 .vendor_id。"
                );
            }
        }
    }

    // (b) 与上游比对（上游缺席 → no-op）。
    let Ok(text) = std::fs::read_to_string(&vendor_md) else {
        return;
    };
    let Some(up_raw) = extract_backtick_after(&text, "上游仓:") else {
        return;
    };
    let up = if let Some(rest) = up_raw.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest),
            None => return,
        }
    } else {
        Path::new(&up_raw).to_path_buf()
    };
    if !up.exists() {
        return; // 上游缺席 → no-op
    }
    // 上游布局：脚本在 scripts/、test 在 scripts/test/、SKILL.md 在根、config 在 examples/。
    let mut stale = 0usize;
    for f in DEPLOYED {
        let vb = std::fs::read(vendor_dir.join(f)).ok();
        let ub = std::fs::read(up.join(f)).ok();
        if let (Some(vb), Some(ub)) = (vb, ub) {
            if vb != ub {
                stale += 1;
            }
        }
    }
    if stale > 0 {
        println!(
            "cargo:warning=vendor cc-acct-iso 过期:上游有 {stale} 个文件与 vendored 副本不一致。见 src-tauri/vendor/cc-acct-iso/VENDOR.md 的 re-vendor 菜谱。"
        );
    }
}

/// F68：vendor 副本过期检查（SS-10「过期看得见」）。从 `VENDOR.md` **单源**抠 pin + 上游
/// 仓路径，若上游 sibling 仓存在则比对 `pin..HEAD` 有没有未 re-vendor 的 core 改动，非空
/// 发**可见的 `cargo:warning`**。**上游仓缺席（CI/Windows）→ 静默 no-op，绝不拖垮构建**
/// （同 `embed_daemons` 二进制缺席 no-op）。软警告非硬失败——开发期上游领先副本是常态。
fn check_vendor_freshness() {
    let vendor_md = Path::new("vendor/code-picture-core/VENDOR.md");
    println!("cargo:rerun-if-changed={}", vendor_md.display());
    let Ok(text) = std::fs::read_to_string(vendor_md) else {
        return;
    };
    let (Some(pin), Some(up)) = (
        extract_backtick_after(&text, "vendored commit:"),
        extract_backtick_after(&text, "上游仓:"),
    ) else {
        return;
    };
    let up = Path::new(&up);
    if !up.join(".git").exists() {
        return; // 上游仓缺席 → no-op
    }
    let Ok(out) = std::process::Command::new("git")
        .arg("-C")
        .arg(up)
        .args([
            "log",
            "--oneline",
            &format!("{pin}..HEAD"),
            "--",
            // 只比对**真被 vendor 的内容**（src + Cargo.toml）；tests/ 不 vendor，
            // 上游只改 tests 的提交不该触发"过期"（审计建议收窄）。
            "crates/code-picture-core/src",
            "crates/code-picture-core/Cargo.toml",
        ])
        .output()
    else {
        return;
    };
    let n = out
        .stdout
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    if n > 0 {
        println!(
            "cargo:warning=vendor code-picture-core 过期:上游 core 有 {n} 个未 re-vendor 的提交(pin={pin})。见 vendor/code-picture-core/VENDOR.md 的 re-vendor 菜谱。"
        );
    }
}

/// 从含 `label` 的行里抠出**第一个反引号包裹**的内容（pin / 上游路径）。
fn extract_backtick_after(text: &str, label: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains(label))?;
    let after = &line[line.find(label)? + label.len()..];
    let start = after.find('`')? + 1;
    let rel_end = after[start..].find('`')?;
    Some(after[start..start + rel_end].to_string())
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
