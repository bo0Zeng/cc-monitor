//! 出站 TCP 只此一口：后端源码树（本包 · 拨号核心 · 部署 · 出网口 …）＋ 中转 crate 的生产段里，开 TCP 的写法只许住出网口（`src/backend/egress/`）。
//! 中转 · 拨号 · 探活都经 `egress::open`；谁在别处自己开一条，代理那一层就被绕过了（代理永远在最外层）。
//!
//! 买不到：经第三方 crate 间接开的连接（russh 的 `client::connect` 之类）—— 那几处不由这一条数，拨号核心今天自己拨 TCP 再交给 russh。

/// 开 TCP 的写法（std 与 tokio 两家）。
const OPENERS: &[&str] = &["TcpStream::connect", "TcpSocket::new"];

/// 后端源码树 ＋ 中转 crate 里每一份 `.rs` 的生产段：`(仓根相对的路径, 文本)`。
fn production() -> Vec<(String, String)> {
    let repo = guard_core::addr::repo_root();
    let mut out = Vec::new();
    for root in [
        guard_core::addr::backend_src_root(),
        guard_core::addr::relay_root(),
    ] {
        for (path, src) in guard_core::scan_tree!(&root, &["rs"]) {
            let rel = path
                .strip_prefix(&repo)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, guard_core::production_code(&src)));
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// 一份文本里开 TCP 的那几处：`(写法, 次数)`。
fn openers_in(text: &str) -> Vec<(&'static str, usize)> {
    OPENERS
        .iter()
        .map(|o| (*o, text.matches(o).count()))
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// ★ 出网口以外，生产段一处都不开 TCP；出网口里真有那一处（反空真：尺子量得到）。
#[test]
fn only_the_egress_port_opens_outbound_tcp() {
    let corpus = production();
    assert!(
        corpus.len() >= 200,
        "只扫到 {} 份源文件 —— 抽取坏了",
        corpus.len()
    );
    let mut stray = Vec::new();
    let mut home = 0;
    for (rel, text) in &corpus {
        for (o, n) in openers_in(text) {
            if rel.starts_with("src/backend/egress/") {
                home += n;
            } else {
                stray.push(format!("{rel}: {o} ×{n}"));
            }
        }
    }
    assert!(
        stray.is_empty(),
        "出网口以外有生产段自己开 TCP（改走 `egress::open`，照这台或那个会话的出网描述）：\n  {}",
        stray.join("\n  ")
    );
    assert!(
        home >= 1,
        "出网口里一处 `TcpStream::connect` 都没量到 —— 尺子在空转"
    );
}

/// 反向自检：同一把尺子放到一份别处的生产段上，量得出来。
#[test]
fn the_ruler_catches_a_connect_outside_the_egress_port() {
    let text = guard_core::production_code(
        "fn f() { let _ = std::net::TcpStream::connect((\"h\", 1)); }\n// TcpSocket::new 只在注释里\n",
    );
    assert_eq!(openers_in(&text), vec![("TcpStream::connect", 1)]);
}
