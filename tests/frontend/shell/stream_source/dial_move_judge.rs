//! `K-P6b` `D2` 判据的**甲半**（界面这一侧）：**界面进程零拨号**。
//!
//! 拨号全在本机常驻后端里（`src/backend/dial/`）；界面这一侧连「经本机后端开一条到远端的链路」那一形也没有了
//! （链路那一族随各台的流归本机后端一起删）：问别台一律经本机客户端 `call_at`，各台的流由本机后端转来（`relayed`）。
//! 剩下要钉的只有一件：界面 crate 里没有一处自己拨 SSH、也没有一处点名 `russh`。
//!
//! # 乙半在哪
//!
//! 乙半（后端这一侧：拨号只许住 `dial/`）住 `tests/backend/dial_tests.rs`。**两侧各扫各的 crate**。

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
    // 运行时拼：写成字面量本文件会被自己扫到（本文件整份是测试段、剥法会剥掉，仍按先例从严）。
    let needles = [
        format!("connect_{}(", "session"),
        format!("channel_open_{}(", "session"),
    ];
    let hit = |prod: &str| needles.iter().any(|n| prod.contains(n.as_str()));
    for sample in [
        "let (session, _fp) = connect_session(cfg, None, None).await?;\n",
        "let channel = session.channel_open_session().await?;\n",
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
/// ⚠ 数的是**代码里点名**（`russh::` / `use russh` / `russh_sftp`）；买不到：经宏或别名间接用到它。
/// 清单那一面（manifest 里没有这几条依赖）同条钉；「依赖树里有没有它」那面旗归 `dial_home_registry::russh_deps_in`。
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
    // 正控：从前那两份的写法，每一形都认得出。
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
         ⇒ 有人在界面进程里又拨起 SSH / 开起 SFTP 来了 —— SSH 与 SFTP 都归本机常驻后端（`src/backend/dial/`），\
         界面经本机客户端（`call_at` · `deploy-*` · `transfer-*`）够到它们。"
    );
    // 清单那一面：界面 crate 的依赖段里没有这两条（也没有只为它们而钉的 `primefield`）。
    let manifest = include_str!("../../../../src/frontend/shell/Cargo.toml");
    for dep in ["russh", "russh-sftp", "primefield"] {
        let line = format!("{dep} = ");
        assert!(
            !manifest
                .lines()
                .any(|l| l.trim_start().starts_with(line.as_str())),
            "`src/frontend/shell/Cargo.toml` 里又声明了 `{dep}` —— 界面进程零 SSH"
        );
    }
    assert!(
        manifest
            .lines()
            .any(|l| l.trim_start().starts_with("tokio = ")),
        "清单里连 `tokio = ` 都抠不到 —— 读错了文件，上面那条零命中在空转"
    );
}
