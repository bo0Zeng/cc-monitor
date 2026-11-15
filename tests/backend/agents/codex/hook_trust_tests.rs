//! Codex 钩子信任：信任键 · 信任哈希 · 版本门。
//! 哈希夹具是照 codex-rs `rust-v0.167.0` 的规则（`NormalizedHookIdentity` · 配置指纹 `codex_config::fingerprint::version_for_toml` · `hook_key`）
//! 用 Python `hashlib` 异源算的，不是本实现自己吐的；命令里的 ccm 入口 `/h/ccm` 是编的。

use super::*;
use serde_json::json;

/// 钩子命令（`'/h/ccm' -- --answer-hook codex`，挂法那一面拼的那一形）。
const COMMAND: &str = "'/h/ccm' -- --answer-hook codex";

/// codex-rs 配置指纹自带的那条测试向量：键序不同的两份同一个哈希。
#[test]
fn fingerprint_matches_codex_own_test_vector() {
    for v in [
        json!({"rows": [{"z": 2, "a": 1}, {"z": 4, "a": 3}], "nested": {"z": "text", "a": true}}),
        json!({"nested": {"a": true, "z": "text"}, "rows": [{"a": 1, "z": 2}, {"a": 3, "z": 4}]}),
    ] {
        assert_eq!(
            fingerprint(&v),
            "sha256:a5e4c8fbdbc75d185b0f2b5291bfb2de10e8c523d6c89248876fd78a97c83176"
        );
    }
}

/// 四种钩子在 `-c` 层第 0 组第 0 条的键：`/<session-flags>/config.toml:<事件>:0:0`，事件段小写下划线。
#[cfg(not(windows))]
#[test]
fn the_four_hooks_are_keyed_under_the_session_flags_layer() {
    assert_eq!(SESSION_SOURCE, "/<session-flags>/config.toml");
    for (label, want) in [
        ("stop", "/<session-flags>/config.toml:stop:0:0"),
        (
            "permission_request",
            "/<session-flags>/config.toml:permission_request:0:0",
        ),
        (
            "user_prompt_submit",
            "/<session-flags>/config.toml:user_prompt_submit:0:0",
        ),
        (
            "session_start",
            "/<session-flags>/config.toml:session_start:0:0",
        ),
    ] {
        assert_eq!(hook_key(label), want);
    }
}

/// 四种钩子各自的信任哈希逐字等于异源算的夹具（事件那三条 10 秒、答题那一条 3600 秒）。
#[test]
fn trusted_hashes_match_the_fixtures_computed_from_codex_rules() {
    for (label, timeout, want) in [
        (
            "permission_request",
            3600,
            "sha256:aa6b28cf0802574c967fce5bf982c0adeccbc7b3617f17a58cb6880ecc53189f",
        ),
        (
            "session_start",
            10,
            "sha256:630a699fa6078e01f0dd4403480450148ee674dd21168197549a1de40a8a9412",
        ),
        (
            "stop",
            10,
            "sha256:9877c679a2e0a155c55d114fb0fe1e68367b4ecb05d14abcc5b7315d755941e2",
        ),
        (
            "user_prompt_submit",
            10,
            "sha256:23cbc90c719a30230920b21aa6d8d93a89e42bb466a59dc26d6d8bb181142fb6",
        ),
        // 没写超时 Codex 补成 600 秒：写明 600 的那一份就是它规范化后的那一份。
        (
            "stop",
            600,
            "sha256:a764a5336f4dda11d051b5ba3cc7c0e32dd99e13f7f3be44713aa2d902c94c4c",
        ),
    ] {
        assert_eq!(trusted_hash(label, COMMAND, timeout), want, "{label}");
    }
}

/// 改一个字段（命令 · 超时 · 事件）⇒ 哈希变，变成的也是异源算的那一个。
#[test]
fn changing_the_command_or_the_timeout_changes_the_hash() {
    let base = trusted_hash("stop", COMMAND, 10);
    let longer = trusted_hash("stop", COMMAND, 11);
    let other_cmd = trusted_hash("stop", "'/h/ccm2' -- --answer-hook codex", 10);
    assert_eq!(
        longer,
        "sha256:4809025bc527ccbf5075e62406caa69e433ced770f1ea5a98d2656812694f32a"
    );
    assert_eq!(
        other_cmd,
        "sha256:438e791bea74e572efd6b65fbd828eb7f871e8795a45f9f0c3f64eca307c3624"
    );
    assert_ne!(base, longer);
    assert_ne!(base, other_cmd);
    assert_ne!(base, trusted_hash("session_start", COMMAND, 10));
}

/// 版本门只声明：`PATH` 上的 `codex` · npm 包名 `@openai/codex`（读法是通用的 `agents::installed_version`，版本串怪形那几条钉在 `agents_tests`）。
#[test]
fn the_gate_declares_the_codex_command_and_package() {
    assert_eq!(GATE.command, super::super::resume::DEFAULT_COMMAND);
    assert_eq!(GATE.package, "@openai/codex");
}

/// 照 npm 全局装的布局造一份：`<前缀>/bin/codex` 链到 `<前缀>/lib/node_modules/<包>/bin/codex.js`，
/// 包目录里放那一份 `package.json`。交回 `PATH`（只那个 bin）。
#[cfg(unix)]
fn npm_install(tag: &str, package_json: Option<&str>) -> (std::path::PathBuf, String) {
    let root = test_home::scratch(tag);
    let pkg = root.join("lib/node_modules/@openai/codex");
    std::fs::create_dir_all(pkg.join("bin")).unwrap();
    std::fs::write(pkg.join("bin/codex.js"), "#!/usr/bin/env node\n").unwrap();
    if let Some(text) = package_json {
        std::fs::write(pkg.join("package.json"), text).unwrap();
    }
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(
        "../lib/node_modules/@openai/codex/bin/codex.js",
        bin.join("codex"),
    )
    .unwrap();
    let path = bin.to_string_lossy().into_owned();
    (root, path)
}

/// 版本门给不给：读装着的那一版，够得着下限才给（键与哈希只在给的时候挂出去）。
/// 比的那一下就是门自己那一句（`HookGate::admits`；核心起会话那一支也照它判）。
#[cfg(unix)]
fn gate_gives(path: Option<&str>) -> bool {
    GATE.admits(crate::agents::installed_version(&GATE, path))
        .is_ok()
}

/// 0.167.0 · 0.167.3 · 0.168.0 ⇒ 给；0.166.9 ⇒ 不给。
#[cfg(unix)]
#[test]
fn the_gate_admits_0_167_0_and_later() {
    assert_eq!(GATE.min_version, MIN_VERSION);
    for (version, gives) in [
        ("0.167.0", true),
        ("0.167.3", true),
        ("0.168.0", true),
        ("0.166.9", false),
    ] {
        let (root, path) = npm_install(
            "codex-npm",
            Some(&format!(
                r#"{{"name":"@openai/codex","version":"{version}"}}"#
            )),
        );
        assert_eq!(gate_gives(Some(&path)), gives, "{version}");
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// 读不出 ⇒ 不给：别的装法（独立二进制、没有那一家的 `package.json`）· 包名不对 · `PATH` 上没有 `codex` · 没有 `PATH`。
#[cfg(unix)]
#[test]
fn an_unreadable_version_gets_nothing() {
    // 独立二进制：`PATH` 上就是真身，往上几层都没有 `package.json`。
    let root = test_home::scratch("codex-native");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("codex"), "\x7fELF").unwrap();
    let path = bin.to_string_lossy().into_owned();
    assert_eq!(crate::agents::installed_version(&GATE, Some(&path)), None);
    assert!(!gate_gives(Some(&path)));
    let _ = std::fs::remove_dir_all(&root);

    // npm 布局但包目录里没有 `package.json`。
    let (root, path) = npm_install("codex-nopkg", None);
    assert_eq!(crate::agents::installed_version(&GATE, Some(&path)), None);
    let _ = std::fs::remove_dir_all(&root);

    // 包名不对的 `package.json`（别人家的包）。
    let (root, path) = npm_install(
        "codex-other",
        Some(r#"{"name":"codex","version":"0.168.0"}"#),
    );
    assert_eq!(crate::agents::installed_version(&GATE, Some(&path)), None);
    let _ = std::fs::remove_dir_all(&root);

    // `PATH` 上没有 `codex` · 没有 `PATH`。
    let empty = test_home::scratch("codex-none");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(!gate_gives(Some(&empty.to_string_lossy())));
    assert!(!gate_gives(None));
    let _ = std::fs::remove_dir_all(&empty);
}
