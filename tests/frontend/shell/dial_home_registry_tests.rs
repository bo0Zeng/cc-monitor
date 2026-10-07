use std::path::Path;

// ════════════════════════════════════════════════════════════════════════
//  界面进程零 SSH：拨号只住后端（`src/backend/dial/`）
// ════════════════════════════════════════════════════════════════════════

/// 拨号锚点：与远端跑 SSH 握手 / 开隧道的那几个调用。运行时拼，免得命中本文件自己的说明。
fn anchors() -> Vec<String> {
    [
        ("client::conn", "ect("),
        ("client::conn", "ect_stream("),
        ("channel_open_direct", "_tcpip("),
    ]
    .iter()
    .map(|(a, b)| format!("{a}{b}"))
    .collect()
}

/// 一份生产段里拨号锚点的处数。
fn anchor_hits(code: &str) -> usize {
    anchors()
        .iter()
        .map(|p| code.matches(p.as_str()).count())
        .sum()
}

/// 本 crate `src/` 递归全部 `.rs` 的**生产段**，相对 `src/` 的路径 + 正文（走 `guard_core::scan_tree!`）。
fn crate_sources() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out: Vec<(String, String)> = Vec::new();
    for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        out.push((rel, guard_core::production_code(&src)));
    }
    out.sort();
    out
}

/// ★ **monitor 生产段里一处拨号锚点都没有**（零命中，带正控）。
///
/// 要拨号就问本机常驻后端（拨号代理 `dial/`）；界面进程自己拨 = 又把 SSH 拉回界面那一半。
#[test]
fn the_dial_only_happens_at_its_registered_home() {
    // 正控：锚点认得出来；遍历走到了入口 `lib.rs`。
    assert_eq!(
        anchor_hits(&format!(
            "let s = russh::{}cfg, addr, h).await?;",
            anchors()[0]
        )),
        1,
        "拨号锚点的认法坏了"
    );
    let corpus = crate_sources();
    assert!(
        corpus.iter().any(|(n, _)| n == "lib.rs"),
        "遍历没走到 `lib.rs`（走到 {} 份）—— 本条在空转",
        corpus.len()
    );
    let strays: Vec<String> = corpus
        .iter()
        .filter_map(|(name, code)| {
            let n = anchor_hits(code);
            (n > 0).then(|| format!("  {name}：{n} 处"))
        })
        .collect();
    assert!(
        strays.is_empty(),
        "monitor 生产段里又有拨号了：\n{}\n界面进程零 SSH —— 拨号与端口转发归后端拨号代理（`src/backend/dial/`）。",
        strays.join("\n")
    );
}

/// 界面 crate 的 manifest。
const INTERFACE_MANIFEST: &str = include_str!("../../../src/frontend/shell/Cargo.toml");

/// manifest 里 `russh` 家族的直接依赖名（注释行不算）。
fn russh_deps_in(manifest: &str) -> Vec<String> {
    let live = guard_core::strip_hash_comment_lines(manifest);
    let mut out: Vec<String> = Vec::new();
    for line in live.lines() {
        let t = line.trim_start();
        if !t.starts_with("russh") {
            continue;
        }
        let Some(eq) = t.find('=') else { continue };
        let name = t[..eq].trim();
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            out.push(name.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// ★ **终点那面二值旗**：界面 crate 的 manifest 里零 `russh` 家族直接依赖（回弹 = 界面进程又拨起 SSH / 又开起 SFTP）。
#[test]
fn the_interface_side_ssh_debt_is_one_number_and_it_is_printed() {
    assert_eq!(
        russh_deps_in("[dependencies]\nrussh = \"0.4\"\n# russh-keys = \"1\"\nserde = \"1\"\n"),
        vec!["russh".to_string()],
        "manifest 依赖名的认法坏了"
    );
    let deps = russh_deps_in(INTERFACE_MANIFEST);
    assert!(
        deps.is_empty(),
        "界面 crate 的 manifest 里又有 `russh` 家族的直接依赖了：{deps:?} —— 界面进程零 SSH"
    );
}
