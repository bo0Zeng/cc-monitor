//! 界面进程零拨号：拨号与端口转发全在本机常驻后端里（`src/backend/dial/`），
//! 问别台一律经本机客户端 `call_at`，各台的流由本机后端转来（`relayed`）。
//! 这里钉三件：界面 crate 生产段里没有一处自己拨 SSH / 开通道、没有一处点名 `russh`，
//! 界面 crate 的 manifest 里没有 `russh` 家族的直接依赖。
//!
//! 后端那一侧（拨号只许住 `dial/`）住 `tests/backend/dial_tests.rs`。两侧各扫各的 crate。

use guard_core::production_code;

/// 界面 crate 生产段的每一份 `.rs`（相对 `src/` 的路径 → 剥掉测试段的源码）。
fn monitor_production() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(path, raw)| {
            (
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
                production_code(&raw),
            )
        })
        .collect()
}

/// →**界面 crate 生产段里自己拨 SSH / 开通道的：零**（带正控）。
#[test]
fn the_monitor_dials_nothing() {
    // 运行时拼：写成字面量本文件会被自己扫到。
    let needles = [
        format!("connect_{}(", "session"),
        format!("channel_open_{}(", "session"),
        format!("client::{}(", "connect"),
        format!("client::{}(", "connect_stream"),
        format!("channel_open_{}(", "direct_tcpip"),
    ];
    let hit = |prod: &str| needles.iter().any(|n| prod.contains(n.as_str()));
    for sample in [
        "let (session, _fp) = connect_session(cfg, None, None).await?;\n",
        "let channel = session.channel_open_session().await?;\n",
        "let s = russh::client::connect(cfg, addr, h).await?;\n",
        "let s = russh::client::connect_stream(cfg, io, h).await?;\n",
        "let ch = s.channel_open_direct_tcpip(host, port, \"127.0.0.1\", 0).await?;\n",
    ] {
        assert!(hit(sample), "针在正控语料上不亮：{sample:?} —— 本条是瞎的");
    }
    let files = monitor_production();
    assert!(
        files.len() >= 100,
        "只扫到 {} 份 `.rs` —— 遍历坏了，本条在空转",
        files.len()
    );
    let users: Vec<&str> = files
        .iter()
        .filter(|(_, prod)| hit(prod))
        .map(|(p, _)| p.as_str())
        .collect();
    assert!(
        users.is_empty(),
        "界面进程又自己拨号了：{users:?} —— 拨号归本机常驻后端，问别台经本机客户端 `call_at`"
    );
}

/// →**界面 crate 里点名 `russh` / `russh_sftp` 的文件：零**（带正控）。
///
/// ⚠ 数的是代码里点名（`russh::` / `use russh` / `russh_sftp`）；经宏或别名间接用到它，这一条看不见，
/// 由下一条（manifest 里没有这一族依赖）兜住。
#[test]
fn russh_is_named_nowhere_in_the_monitor_crate() {
    let needles = [
        format!("russh{}", "::"),
        format!("use {}", "russh"),
        format!("russh{}sftp", "_"),
    ];
    let hit = |prod: &str| -> bool {
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .any(|l| needles.iter().any(|n| l.contains(n.as_str())))
    };
    for sample in [
        "use russh::client;\n",
        "fn f(s: &russh_sftp::client::SftpSession) {}\n",
        "let h: russh::client::Handle<X>;\n",
    ] {
        assert!(hit(sample), "针在正控语料上不亮：{sample:?} —— 本条是瞎的");
    }
    let files = monitor_production();
    assert!(
        files.len() >= 100,
        "只扫到 {} 份 `.rs` —— 遍历坏了，本条在空转",
        files.len()
    );
    let users: Vec<&str> = files
        .iter()
        .filter(|(_, prod)| hit(prod))
        .map(|(p, _)| p.as_str())
        .collect();
    assert!(
        users.is_empty(),
        "界面 crate 里又点名了 `russh` / `russh_sftp`：{users:?}\n\
         ⇒ 界面进程里又拨起 SSH / 开起 SFTP 来了 —— SSH 与 SFTP 都归本机常驻后端（`src/backend/dial/`），\
         界面经本机客户端（`call_at` · `deploy-*` · `transfer-*`）够到它们。"
    );
}

/// 界面 crate 的 manifest。
const INTERFACE_MANIFEST: &str = include_str!("../../../../src/frontend/shell/Cargo.toml");

/// manifest 里 SSH 那一族的直接依赖名：`russh` 开头的一整族（`russh-sftp` · `russh-keys` …），
/// 加只为它们而钉的 `primefield`。注释行不算。
fn ssh_family_deps(manifest: &str) -> Vec<String> {
    let live = guard_core::strip_hash_comment_lines(manifest);
    let mut out: Vec<String> = Vec::new();
    for line in live.lines() {
        let t = line.trim_start();
        let Some(eq) = t.find('=') else { continue };
        let name = t[..eq].trim();
        let is_name = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if is_name && (name.starts_with("russh") || name == "primefield") {
            out.push(name.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// →**界面 crate 的 manifest 里零 SSH 家族直接依赖**（带正控）。
#[test]
fn the_interface_manifest_has_no_ssh_dependency() {
    assert_eq!(
        ssh_family_deps(
            "[dependencies]\nrussh = \"0.4\"\nrussh-keys = \"0.4\"\nrussh_sftp = \"2\"\n\
             # russh-cryptovec = \"1\"\nprimefield = \"0.1\"\nserde = \"1\"\n"
        ),
        vec![
            "primefield".to_string(),
            "russh".to_string(),
            "russh-keys".to_string(),
            "russh_sftp".to_string(),
        ],
        "manifest 依赖名的认法坏了"
    );
    assert!(
        INTERFACE_MANIFEST
            .lines()
            .any(|l| l.trim_start().starts_with("tokio = ")),
        "清单里连 `tokio = ` 都抠不到 —— 读错了文件，下面那条零命中在空转"
    );
    let deps = ssh_family_deps(INTERFACE_MANIFEST);
    assert!(
        deps.is_empty(),
        "`src/frontend/shell/Cargo.toml` 里又有 SSH 家族的直接依赖了：{deps:?} —— 界面进程零 SSH"
    );
}
