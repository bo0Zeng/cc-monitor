use std::path::{Path, PathBuf};

fn main() {
    emit_daemon_build_id();
    emit_daemon_capabilities();
    check_vendor_freshness();
    check_acct_iso_vendor_freshness();
    embed_daemons();
    embed_native_daemon();
    tauri_build::build()
}

/// daemon 源码的住址 —— 本文件里**四处**要它（build_id · capabilities · mtime · 内嵌校验）。
/// 抽出来的理由与 `local_extract_name` 同族：四份手抄的路径迟早有一份被漏改。
fn daemon_main_rs() -> PathBuf {
    Path::new("..")
        .join("remote-daemon-proto")
        .join("src")
        .join("main.rs")
}

/// daemon 源码里那个 `const BUILD_ID`。**这是本机与远端两条内嵌路共用的期望值。**
fn daemon_source_build_id() -> Option<String> {
    std::fs::read_to_string(daemon_main_rs())
        .ok()
        .and_then(|s| extract_build_id(&s))
}

/// F5：vendored cc-acct-iso 过期软检查（SS-10「过期看得见」）。从 `VENDOR.md` 抠上游仓路径
/// （`~` 展开为 $HOME），若上游存在则比对三个脚本与 vendored 副本，不一致则 `cargo:warning`。
/// 上游缺席 → no-op（同 `check_vendor_freshness`：开发期上游领先副本是常态，软警告非硬失败）。
fn check_acct_iso_vendor_freshness() {
    let vendor_dir = Path::new("vendor/cc-acct-iso");
    let vendor_md = vendor_dir.join("VENDOR.md");
    println!("cargo:rerun-if-changed={}", vendor_md.display());
    println!(
        "cargo:rerun-if-changed={}",
        vendor_dir.join(".vendor_id").display()
    );
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
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cat_cmd)
            .output()
        {
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
    let main_rs = daemon_main_rs();
    println!("cargo:rerun-if-changed={}", main_rs.display());
    let build_id = std::fs::read_to_string(&main_rs)
        .ok()
        .and_then(|s| extract_build_id(&s))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DAEMON_BUILD_ID={build_id}");
    // F05a：本机 sidecar 的文件名是 `<stem>-<target-triple>`（Tauri `externalBin` 的规矩），
    // 而 std 里没有「当前 target triple」这个常量 —— 只有 build script 拿得到 `TARGET`。
    println!(
        "cargo:rustc-env=CCM_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into())
    );
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
    let main_rs = daemon_main_rs();
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
    let src_mtime = std::fs::metadata(daemon_main_rs())
        .and_then(|m| m.modified())
        .ok();
    // U-1：mtime 之外再取 **build_id 字符串本身**，用于下面那条硬校验（mtime 漏掉过一次真事故）。
    let source_build_id = daemon_source_build_id();
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
            // ★ U-1（2026-08-01）：**mtime 不够，要比 build_id 字符串本身。**
            //
            // 上面那条 mtime 警告漏掉了真实发生过的一次：源码 bump 到 `p1v-attachable`，
            // 而 `embedded-daemons/*.build_id` 还是 `p1u-fork-session`（内嵌二进制**更新**
            // 但内容是旧版）。这就是发版纪律里说的「半 bump 比不 bump 更糟」。
            //
            // 为什么必须 **panic** 而不是 warning：monitor 判「远端该不该换 daemon」的唯一
            // 判据是 `reported_build_id != EXPECTED_DAEMON_BUILD_ID`。内嵌的是 p1u、期望的是
            // p1v ⇒ 装上去之后**永远判 stale** ⇒ **无限重装循环**。这不是「慢一点」，是坏的。
            // 出路只有两条：重跑 zigbuild 生成对得上的二进制，或删掉 embedded-daemons/
            // （那样自动部署诚实地关掉，不会装一个注定被判过期的东西）。
            // ★ Phase D 审计 I3：**缺清单不能只是 warning** —— 那正是本轮硬校验的静默旁路，
            // 而且是最危险的场景（有人手工塞了个陈旧二进制、没写清单）恰好绕开校验。
            // 顺带：`sftp.rs` 运行时对 x86_64 的身份识别**只能靠清单**
            //（编译器把 BUILD_ID 优化成立即数，字节里搜不到连续明文，`bytes_contain` 会误拒正品）。
            let expected = source_build_id.as_deref().unwrap_or_else(|| {
                // 抠不到源码 build_id ⇒ 单源链条已断，`DAEMON_BUILD_ID` 此刻是 "unknown"，
                // 装出去每台远端都会被判 StaleBuild。比 mismatch 更该拦。
                panic!(
                    "抠不到 daemon 源码的 `const BUILD_ID`（路径失效 / crate 改名 / const 写法变了）——\
                     单源链条已断，`DAEMON_BUILD_ID` 会静默退化成 \"unknown\"，\
                     装出去每台远端都会被判 StaleBuild 并无限重装。先修 build.rs 的提取逻辑。"
                )
            });
            if embedded_id.is_empty() {
                panic!(
                    "内嵌 daemon {arch} 有二进制但**缺 .build_id 清单**。\n\
                     清单是它在运行期的唯一身份来源（BUILD_ID 常被优化成立即数，字节里搜不到明文），\
                     缺了它 ⇒ 运行时回退字节启发式会**误拒正品**，且本轮的一致性校验被静默跳过。\n\
                     补：printf '%s\\n' '{expected}' > src-tauri/embedded-daemons/cc-monitor-remote-{arch}.build_id\n\
                     （前提是那个二进制**真的**是 {expected} 编出来的；不确定就删掉整个目录重来。）"
                );
            }
            // ★ 08-25（`K-H1` / 风险 `5v`）：**引 TLS 之后这条自救路分 arch 了。**
            // 下面那段原本逐字写着「不需要装 zig（U-1 实测）」——`rustls` → `ring` 进来之后
            // 那句话**变成了假话**：`ring` 的 build script 要**编 C**，而 `rust-lld` 只是个
            // 链接器，它替不了 C 编译器。08-25 把那两条命令逐条重打，读数分岔：
            //   x86_64  照做 **rc=0**
            //   aarch64 照做 **rc=101** ⇒ `error occurred in cc-rs:
            //           failed to find tool "aarch64-linux-musl-gcc"`
            let c_cross_note: &str = if arch == "aarch64" {
                "⚠ **这个 arch 还额外要一个 C 交叉编译器**（08-25 实测：不给就 rc=101，\n\
                        死在 `cc-rs: failed to find tool \"aarch64-linux-musl-gcc\"` ——\n\
                        `ring` 的 build script 要编 C，`rust-lld` 只是链接器，替不了它）。\n\
                        **实测走得通的一条**（zig 0.14.0，08-25 本机跑到 rc=0）：\n\
                        给上面那条 `cargo build` 前面加三个环境变量 ——\n\
                          CC_aarch64_unknown_linux_musl=\"zig cc\" \\\n\
                          CFLAGS_aarch64_unknown_linux_musl=\"--target=aarch64-linux-musl\" \\\n\
                          AR_aarch64_unknown_linux_musl=\"zig ar\" \\\n\
                        另一条是装一套提供 `aarch64-linux-musl-gcc` 的 musl 交叉工具链\n\
                        —— **我没量过**，别当成验过的路。"
            } else {
                "· 这个 arch **不额外要 C 交叉编译器**（08-25 实测 rc=0）：\n\
                        `ring` 的那点 C 用本机的 `x86_64-linux-musl-gcc` 就编过去了。"
            };
            if embedded_id != expected {
                panic!(
                    "内嵌 daemon {arch} 的 build_id 是 `{embedded_id}`，而 daemon 源码是 `{expected}` —— \
                     **半 bump**。装上去会被 monitor 永远判 StaleBuild 并无限重装。\n\
                     出路二选一：\n\
                     ① 重编并同步清单。三步都要做，\n\
                        **前两步在 `remote-daemon-proto/` 目录下跑**：\n\
                        cd remote-daemon-proto\n\
                        cargo build --release --target {arch}-unknown-linux-musl \\\n\
                          --config 'target.{arch}-unknown-linux-musl.linker=\"rust-lld\"'\n\
                        cp target/{arch}-unknown-linux-musl/release/cc-monitor-remote \\\n\
                           ../src-tauri/embedded-daemons/cc-monitor-remote-{arch}\n\
                        printf '%s\\n' '{expected}' \\\n\
                           > ../src-tauri/embedded-daemons/cc-monitor-remote-{arch}.build_id\n\
                        （x86_64 上不加 --config 也能链；aarch64 必须加，否则会挂在系统 ld 上。\n\
                         注意：这样编出来的形态与发版 CI 的 zigbuild 产物**不同**\n\
                         —— static-pie / 未 strip / 不同 rustc，只适合本机打包。）\n\
                        {c_cross_note}\n\
                        ⚠ 发版 CI（`release.yml`）走的是 `cargo zigbuild`，它自己装 zig，\n\
                         而 `zig cc` 能编 C ⇒ **那条路多半仍通，但我没量过**，别读成量过了。\n\
                     ② 直接 `rm -rf src-tauri/embedded-daemons/`：自动部署诚实关闭，\
                        编译立刻恢复（该目录已被 gitignore，删除零代价）。"
                );
            }
            let dst = Path::new(&out).join(format!("daemon-{arch}"));
            std::fs::copy(&src, &dst).expect("copy embedded daemon binary");
        } else {
            // U-1：原来这里**连 warn 都没有** —— 缺二进制就静默不置 cfg，
            // `sftp::daemon_binary()` 返回 None、远端自动部署整个消失而无人知晓。
            // 那正是 v2.19–v2.22 那批安装包的事故形状（见 release.yml 的账）。
            println!(
                "cargo:warning=缺少内嵌 daemon {arch}（{}）——远端自动部署将关闭（embedded_daemons cfg 不置）",
                src.display()
            );
            all = false;
        }
    }
    if all {
        println!("cargo:rustc-cfg=embedded_daemons");
    }
}

/// 目标平台的可执行后缀。
///
/// 🔴 **由 `TARGET` 算，不许用 `std::env::consts::EXE_SUFFIX`** —— build script 跑在
/// **构建机**上（交叉编译 Windows 产物时那台是 Linux）⇒ 那个常量给的是**宿主**的后缀，
/// 而这里要的是**目标**的。同一个坑 `emit_daemon_build_id` 里那条 `CCM_TARGET_TRIPLE`
/// 已经踩过一次（「std 里没有『当前 target triple』这个常量，只有 build script 拿得到 `TARGET`」）。
fn target_exe_suffix(target: &str) -> &'static str {
    if target.contains("windows") {
        ".exe"
    } else {
        ""
    }
}

/// 本机内嵌 daemon 的落点 —— `release.yml` 那一步按同一条路径铺，
/// `local_backend::native_embedded_daemon` 用**同一条路径的字面量** `include_bytes!` 它。
///
/// # 🔴 为什么是**定死的名字**，而不是像 `embedded-daemons/` 那样把 triple 编进文件名
///
/// 因为消费侧那个 `include_bytes!` 的参数**必须是字面量**：
/// `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason` 默认拒绝
/// 非字面量的 `include_*!`，而它的登记表**不在本件写区**。
/// ⇒ 名字定死、**用一个旁挂清单把 triple 记下来再核**（`.target`，下面那条硬校验）。
/// 这一换其实更强：文件名是**没人核的约定**，清单是**每次构建都核的断言**。
///
/// # 🔴 为什么不铺进已有的 `embedded-daemons/`
///
/// 那个目录名是 `local_daemon.rs::every_test_that_starts_the_real_daemon_demands_a_private_tmux`
/// 认「谁会起真 daemon」的**来历串之一**。字面量路径写进 `local_backend.rs` 的生产段，
/// 会把整段生产代码拖进那条判据的人群（实测：人群里多出一条 `local_backend.rs::default`，
/// 而那条连测试都不是；同时 `the_local_daemon_really_registers_an_inbound_client`
/// 因为切块规则一起掉出人群，那条判据的地板当场红）。
/// ⇒ 换一个目录，两条判据都不被误伤，而且它本来也不是同一类东西
/// （那边是**远端部署产物**，这边是**本机原生**）。
/// ✅ **那笔欠账 09-10 当天就还了**：`src-tauri/.gitignore` 里已经有 `/native-daemon/`。
/// 〔墓碑 —— 本行原话逐字：「⚠ **一条如实记着的欠账**：`src-tauri/.gitignore` 里还没有这一行，
///  而它**不在本件写区** —— 见 `K-R42` 件文件的上报口。」它写下时是真的，
///  补上那一行的那一拍（`track/alarm`）当场把它变成了假话，**而那一拍也点名了它**。〕
/// ★ 记这一笔的形状：**造了债的那一拍自己报出来，并分清它是新债还是存量** ——
/// 这条正是**它自己造的新债**，不是从别处继承来的。
const NATIVE_DAEMON_DIR: &str = "native-daemon";
const NATIVE_DAEMON_FILE: &str = "cc-monitor-native";

/// `K-R42`：**把「本机后端」也内嵌进 exe**，让裸 `monitor.exe` 自己带得上一份。
///
/// # 它与 `embed_daemons` 是两件事，别合并
///
/// | | `embed_daemons`（远端那条） | 本函数（本机这条） |
/// |---|---|---|
/// | 内嵌什么 | `cc-monitor-remote-{x86_64,aarch64}`，**musl Linux**，按 **arch** 分派 | `cc-monitor-native-<target triple>`，**当前 TARGET 的原生二进制** |
/// | 给谁用 | SFTP 推到远端主机（远端就是 Linux ⇒ musl 是对的） | 本机自释放（`local_backend::start_or_extract`） |
/// | 认不认 OS | **不认**（只看 arch） | **由 TARGET 定死**，编译期就选好了 |
///
/// 🔴 **「不认 OS」正是 09-10 那个真机读数的根因**：`local_daemon.rs` 里那道
/// `if cfg!(target_os = "linux")` 的闸（D 阶段补审 08-11 加的）挡的就是
/// 「往 Windows 上释放一个 Linux ELF、然后报告『已起』」——
/// 那道闸**是对的**，它挡住的是**没有 Windows 版可嵌**这件事，不是「自释放这条路不该走」。
/// ⇒ 本函数补的就是那个缺口：**给 Windows 一份能跑的字节**。
/// 缺口本身一年前就登记在 `devbench/ROADMAP.md` 的 `5f⁗` 上（逐字：「要 release 流程产
/// Windows daemon 并内嵌」）—— 今天才第一次有人在真机上撞到它。
///
/// # 为什么这里 panic 而不是 warning（与 `embed_daemons` 同一条理由，射程不同）
///
/// 远端那条怕的是「装上去永远判 stale ⇒ 无限重装」。本机这条**不会**无限重装
/// （文件名带 build_id，见 `local_backend::local_extract_name`），但它会
/// **把一份贴错标签的二进制留在用户机器上**：清单说它是 X，字节其实是 Y ⇒
/// `DAEMON_CAPABILITIES` 那套乐观路径按 X 谈能力、跑起来的是 Y。
/// ⇒ 半 bump 一样比不 bump 更糟，一样当场拦下。
///
/// # 缺席 = 不置 cfg + **可见的** warning
///
/// 沿用 `embed_daemons` 的 U-1 那条账：**静默不置 cfg 正是 v2.19–v2.22 那批安装包的事故形状**。
/// 缺席时 `local_backend::native_embedded_daemon()` 返回 `None`，自释放这条路诚实关掉。
fn embed_native_daemon() {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_native_daemon)");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into());
    // ⚠ **无条件 emit**：`local_backend::local_extract_name` 是**无条件**的生产代码，
    // 它 `env!` 这个名字 —— 只在某些分支 emit 会让别的分支编不过。
    println!(
        "cargo:rustc-env=CCM_TARGET_EXE_SUFFIX={}",
        target_exe_suffix(&target)
    );

    let dir = Path::new(NATIVE_DAEMON_DIR);
    let src = dir.join(NATIVE_DAEMON_FILE);
    let id_manifest = dir.join(format!("{NATIVE_DAEMON_FILE}.build_id"));
    let target_manifest = dir.join(format!("{NATIVE_DAEMON_FILE}.target"));
    for f in [&src, &id_manifest, &target_manifest] {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    let embedded_id = read_trimmed(&id_manifest);
    // ⚠ **无条件 emit**（同上那条理由）。
    println!("cargo:rustc-env=DAEMON_NATIVE_ID={embedded_id}");

    if !src.exists() {
        println!(
            "cargo:warning=没有本机内嵌 daemon（src-tauri/{}）——**裸可执行文件起不了本机后端**，\
             只有安装包那份带 sidecar 的能起。开发构建里这是正常的；\
             发版构建里出现这一行 = 那一版的裸 exe 又回到 09-10 那个读数（0 个本机后端进程）。",
            src.display()
        );
        return;
    }
    // ── ① 它是**给这个 target 编的**吗 ─────────────────────────────────────
    //
    // 名字定死（理由见 `NATIVE_DAEMON_DIR` 头注）⇒ 「这份字节属于哪个平台」**只能靠这个清单**。
    // 缺清单 / 对不上都当场拦：放它过去等于把一个别的平台的二进制内嵌进来，
    // 而那正是 08-11 补审逮到的那个阻塞级缺陷（往 Windows 上释放 Linux ELF 再报「已起」）。
    let staged_target = read_trimmed(&target_manifest);
    if staged_target != target {
        panic!(
            "本机内嵌 daemon（src-tauri/{}）是给 `{}` 编的，而这一趟的 TARGET 是 `{target}`。\n\
             （清单读作 `{staged_target}`；空串 = 根本没有 `{}.target` 这个文件。）\n\
             内嵌一个别的平台的二进制 = 释放到用户盘上再起，起不来 —— \
             而 `Resolved::Found` 会先撒一次谎（它只证明文件落地了）。\n\
             出路二选一：① 为这个 target 重编并重铺那三个文件；\
             ② `rm -rf src-tauri/{}`：自释放诚实关闭，编译立刻恢复。",
            src.display(),
            staged_target,
            NATIVE_DAEMON_FILE,
            NATIVE_DAEMON_DIR
        );
    }
    // ── ② 它的 build_id 与 daemon 源码对得上吗 ─────────────────────────────
    let expected = daemon_source_build_id().unwrap_or_else(|| {
        panic!(
            "抠不到 daemon 源码的 `const BUILD_ID`（路径失效 / crate 改名 / const 写法变了）——\
             内嵌进去的那份就没有可信身份了。先修 build.rs 的提取逻辑。"
        )
    });
    if embedded_id.is_empty() {
        panic!(
            "本机内嵌 daemon（src-tauri/{}）有二进制但**缺 `.build_id` 清单**。\n\
             清单是它落到用户盘上时的文件名来源（`cc-monitor-local-<build_id>`），\
             缺了它这条自释放路根本拼不出落点。\n\
             补：printf '%s\\n' '{expected}' > src-tauri/{}",
            src.display(),
            id_manifest.display()
        );
    }
    if embedded_id != expected {
        panic!(
            "本机内嵌 daemon（src-tauri/{}）的清单是 `{embedded_id}`，而 daemon 源码是 `{expected}` \
             —— **半 bump**。\n\
             这一份会以 `cc-monitor-local-{embedded_id}` 之名落到用户盘上，而字节其实是别的版本：\
             能力协商按清单谈、跑起来的是另一个。\n\
             出路二选一：① 重编并同步清单（在 `remote-daemon-proto/` 下 `cargo build --release`，\
             产物铺成 `src-tauri/{}` 那三个文件）；\
             ② `rm -rf src-tauri/{}`：自释放诚实关闭，编译立刻恢复。",
            src.display(),
            src.display(),
            NATIVE_DAEMON_DIR
        );
    }
    // ⚠ **不往 `OUT_DIR` 拷一份**（`embed_daemons` 那条是拷的）——消费侧那个 `include_bytes!`
    // 直接按**字面量相对路径**读得到，而拷贝会让本函数变成一个**写盘落点**，
    // 那要在 `write_site_registry::WRITE_SITES` 里申报（`fs::copy(` 是它的 needle 之一），
    // 而那张表**不在本件写区**。少一次拷贝同时也少一条要申报的写点 —— 两头都更干净。
    println!("cargo:rustc-cfg=embedded_native_daemon");
}

/// 读一份单值清单，读不到就给空串（「没有这个文件」与「文件是空的」在这里同义：都不可信）。
fn read_trimmed(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
