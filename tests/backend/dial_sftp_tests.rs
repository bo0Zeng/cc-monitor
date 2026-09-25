//! 〔SR1b · 2026-09-24〕`dial/sftp.rs` 的判据：远端写围栏（只许两处）· 解链接 · files 链路的一问一答。
//!
//! 台架是 `sftp_rig`（合成 SFTP 服务端，逐条记改动路径）。**判据看服务端的改动表，不信被测侧的自述。**

use super::rig::{self, HOME};
use super::{fence_lexical, open_for_write, serve_files, Intent, Refusal, REMOTE_WRITE_ROOTS};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ═══ B3 · 词法那一道：逐形正反 ════════════════════════════════════════════════════════

#[test]
fn the_lexical_fence_admits_exactly_the_two_roots_and_nothing_else() {
    let home = "/home/u";
    // (路径, 形, 期望：Some(根) = 放行且落在那个根；Some("") = 放行且是 `.cc-monitor` 本身；None = 拒)
    let cases: &[(&str, Intent, Option<&str>)] = &[
        (
            ".cc-monitor/bin/cc-monitor-backend",
            Intent::File,
            Some(".cc-monitor/bin"),
        ),
        (
            "/home/u/.cc-monitor/bin/.build_id",
            Intent::File,
            Some(".cc-monitor/bin"),
        ),
        (
            ".cc-monitor/staging/00ff.part",
            Intent::File,
            Some(".cc-monitor/staging"),
        ),
        (
            "/home/u/.cc-monitor/staging/a/b",
            Intent::File,
            Some(".cc-monitor/staging"),
        ),
        (
            ".cc-monitor/bin/cc-acct-iso/scripts",
            Intent::Dir,
            Some(".cc-monitor/bin"),
        ),
        (".cc-monitor/bin", Intent::Dir, Some(".cc-monitor/bin")),
        (
            ".cc-monitor/staging/",
            Intent::Dir,
            Some(".cc-monitor/staging"),
        ),
        (".cc-monitor", Intent::Dir, Some("")),
        // —— 拒 ——
        (".cc-monitor/bin", Intent::File, None),
        (".cc-monitor", Intent::File, None),
        (".cc-monitor/binx/y", Intent::File, None),
        (".cc-monitor/bin-old/y", Intent::Dir, None),
        (".cc-monitor/bin/../../.bashrc", Intent::File, None),
        (".cc-monitor/./bin/x", Intent::File, None),
        (".cc-monitor//bin/x", Intent::File, None),
        (".bashrc", Intent::File, None),
        (".local/bin/ccm", Intent::File, None),
        (".cc-monitor/other/x", Intent::Dir, None),
        ("/etc/passwd", Intent::File, None),
        ("/home/u2/.cc-monitor/bin/x", Intent::File, None),
        ("/home/uu/.cc-monitor/bin/x", Intent::File, None),
        ("/home/u", Intent::Dir, None),
        ("", Intent::File, None),
        ("   ", Intent::Dir, None),
    ];
    for (path, intent, want) in cases {
        let got = fence_lexical(home, path, *intent);
        match want {
            None => assert!(got.is_err(), "`{path}`（{intent:?}）该拒，却放行成 {got:?}"),
            Some(root) => {
                let (rel, r) =
                    got.unwrap_or_else(|e| panic!("`{path}`（{intent:?}）该放行，却拒了：{e}"));
                assert_eq!(
                    r.unwrap_or(""),
                    *root,
                    "`{path}` 落在的根不对（rel = {rel}）"
                );
                assert!(
                    !rel.starts_with('/') && !rel.contains(".."),
                    "归一化没做干净：{rel}"
                );
            }
        }
    }
    // SFTP 起始目录不是 POSIX 绝对路径（Windows 远端那一形）⇒ 绝对路径一律拒，不猜。
    assert!(fence_lexical(
        "C:\\Users\\u",
        "/C:/Users/u/.cc-monitor/bin/x",
        Intent::File
    )
    .is_err());
}

/// 🔴 写根**恰好两处**、就是题面那两处（异源：期望取自 V89 题面，不取自本文件的常量）。
#[test]
fn the_declared_write_roots_are_exactly_staging_and_bin() {
    let want: std::collections::BTreeSet<&str> = [".cc-monitor/staging", ".cc-monitor/bin"]
        .into_iter()
        .collect();
    let got: std::collections::BTreeSet<&str> = REMOTE_WRITE_ROOTS.into_iter().collect();
    assert_eq!(got, want);
    // 暂存区那一根与提交那一侧（`files_commit`）是同一个串 —— 同一个 crate，直接比。
    assert_eq!(
        super::STAGING_ROOT,
        crate::control::files_commit::STAGING_DIR
    );
}

// ═══ 解链接那一道 ══════════════════════════════════════════════════════════════════════

/// 根底下一条**目录链接**指到根外（`bin/evil -> ~/.ssh`）：词法那一道放行，解链接那一道必须拒，服务端零改动。
#[tokio::test]
async fn a_symlinked_directory_under_a_root_cannot_carry_a_write_out() {
    let fs = rig::home(true, false);
    {
        let mut g = fs.lock().unwrap();
        g.dirs.insert(".ssh".to_string());
        g.links
            .insert(".cc-monitor/bin/evil".to_string(), ".ssh".to_string());
    }
    let s = rig::session_on(fs.clone()).await;
    let r = super::put_atomic(&s, ".cc-monitor/bin/evil/authorized_keys", b"k", 0o600).await;
    assert!(
        matches!(&r, Err(Refusal::Fenced(m)) if m.contains("跑出了写根")),
        "解链接那一道没拦住：{r:?}"
    );
    let r2 = super::make_dirs(&s, ".cc-monitor/bin/evil/deeper").await;
    assert!(
        matches!(r2, Err(Refusal::Fenced(_))),
        "建目录也要拦：{r2:?}"
    );
    assert!(
        fs.lock().unwrap().mutated.is_empty(),
        "被拒的请求在服务端留下了改动：{:?}",
        fs.lock().unwrap().mutated
    );
    // 正控：同一台上，根底下的真目录照写（拦的是「解出去」，不是「子目录」）。
    fs.lock()
        .unwrap()
        .dirs
        .insert(".cc-monitor/bin/sub".to_string());
    super::put_atomic(&s, ".cc-monitor/bin/sub/x", b"k", 0o600)
        .await
        .expect("根底下的真子目录该放行");
    assert_eq!(
        fs.lock().unwrap().bytes(".cc-monitor/bin/sub/x"),
        Some(b"k".to_vec())
    );
}

/// 开写之前问一次 `lstat`：最后一段是链接 ⇒ 拒（开写会跟过去）。
#[tokio::test]
async fn opening_a_symlink_for_write_is_refused() {
    let fs = rig::home(false, true);
    {
        let mut g = fs.lock().unwrap();
        g.files.insert(".bashrc".to_string(), rig::Entry::default());
        g.links.insert(
            ".cc-monitor/staging/k.part".to_string(),
            ".bashrc".to_string(),
        );
    }
    let s = rig::session_on(fs.clone()).await;
    let r = open_for_write(&s, ".cc-monitor/staging/k.part", false).await;
    assert!(
        matches!(r, Err(Refusal::Fenced(ref m)) if m.contains("链接")),
        "链接没拦住"
    );
    assert!(fs.lock().unwrap().mutated.is_empty());
}

// ═══ files 链路：一问一答 ══════════════════════════════════════════════════════════════

/// 起一条 files 链路（生产 `serve_files`）接在台架会话上，回 `(写端, 读端)`。
async fn link_on(
    fs: std::sync::Arc<std::sync::Mutex<rig::Fs>>,
) -> (
    tokio::io::WriteHalf<tokio::io::DuplexStream>,
    BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
) {
    let s = rig::session_on(fs).await;
    let (ours, theirs) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let (r, mut w) = tokio::io::split(theirs);
        serve_files(&s, r, &mut w).await;
    });
    let (r, w) = tokio::io::split(ours);
    (w, BufReader::new(r))
}

async fn ask(
    w: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    r: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    req: serde_json::Value,
    bytes: Option<&[u8]>,
) -> serde_json::Value {
    let mut line = req.to_string();
    line.push('\n');
    w.write_all(line.as_bytes()).await.unwrap();
    if let Some(b) = bytes {
        w.write_all(b).await.unwrap();
    }
    w.flush().await.unwrap();
    let mut back = String::new();
    r.read_line(&mut back).await.unwrap();
    serde_json::from_str(&back).unwrap_or_else(|e| panic!("应答不是 JSON（{e}）：{back:?}"))
}

/// 🔴🔴 **B2：一趟部署 ＋ 暂存区写跑下来，服务端记下的改动路径落在的根 == 两处**（相等，不是 ⊆）；
/// 越界的四种请求一律 `fenced`、服务端改动表**零增长**。
#[tokio::test]
async fn a_deploy_shaped_session_touches_exactly_the_two_roots_and_refuses_everything_else() {
    let fs = rig::home(false, true);
    let (mut w, mut r) = link_on(fs.clone()).await;

    let home = ask(&mut w, &mut r, serde_json::json!({"op":"home"}), None).await;
    assert_eq!(home["home"], HOME);

    // 首次部署：`bin` 还不在 ⇒ 逐级建。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"mkdirs","path": format!("{HOME}/.cc-monitor/bin")}),
        None,
    )
    .await;
    assert!(v.get("code").is_none(), "{v}");
    let bin = rig::corpus(100_000);
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"put","path": format!("{HOME}/.cc-monitor/bin/cc-monitor-backend"),
                           "size": bin.len(), "mode": 0o700, "verify": true}),
        Some(&bin),
    )
    .await;
    assert_eq!(v["readback"]["len"], bin.len(), "{v}");
    assert!(
        v["readback"]["first_diff"].is_null(),
        "读回逐字节应当相同：{v}"
    );
    // 再放一次（盖掉旧的）：旧的改名成 `.bak` 再删，临时件上位 —— 目录里只剩那一份。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"put","path": ".cc-monitor/bin/cc-monitor-backend","size": 3,"mode": 0o700,"verify": false}),
        Some(b"new"),
    )
    .await;
    assert!(v["readback"].is_null(), "{v}");
    // 暂存区一份。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"put","path": ".cc-monitor/staging/aa.part","size": 2,"mode": 0o600}),
        Some(b"xy"),
    )
    .await;
    assert!(v.get("code").is_none(), "{v}");
    // 读 · 看 · 删。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"read","path": ".cc-monitor/bin/cc-monitor-backend","max": 1024}),
        None,
    )
    .await;
    assert_eq!(v["data"], crate::wire::b64_encode(b"new"));
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"stat","path": ".cc-monitor/bin/nope"}),
        None,
    )
    .await;
    assert!(v["meta"].is_null() && v["exists"] == false, "{v}");
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"remove","path": ".cc-monitor/staging/aa.part"}),
        None,
    )
    .await;
    assert_eq!(v["removed"], true);

    // —— 越界：四种形，一律拒，改动表零增长 ——
    let before = fs.lock().unwrap().mutated.len();
    for (req, bytes) in [
        (
            serde_json::json!({"op":"put","path": ".bashrc","size": 1,"mode": 0o644}),
            Some(&b"x"[..]),
        ),
        (
            serde_json::json!({"op":"put","path": format!("{HOME}/.local/bin/ccm"),"size": 1,"mode": 0o755}),
            Some(&b"x"[..]),
        ),
        (
            serde_json::json!({"op":"remove","path": "/etc/passwd"}),
            None,
        ),
        (
            serde_json::json!({"op":"mkdirs","path": ".cc-monitor/bin/../../.ssh"}),
            None,
        ),
    ] {
        let v = ask(&mut w, &mut r, req.clone(), bytes).await;
        assert_eq!(v["code"], "fenced", "越界请求没被围栏拒：{req} ⇒ {v}");
    }
    assert_eq!(
        fs.lock().unwrap().mutated.len(),
        before,
        "被拒的请求在服务端留下了改动"
    );
    // 协议上的坏形：说一声，链路照旧能用。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"chmod","path":"x"}),
        None,
    )
    .await;
    assert_eq!(v["code"], "unknown_op");

    // 🔴 相等：改动落在的根 == 两处。
    let g = fs.lock().unwrap();
    let roots: std::collections::BTreeSet<&str> = g
        .touched()
        .iter()
        .map(|p| {
            REMOTE_WRITE_ROOTS
                .into_iter()
                .find(|root| p == root || p.starts_with(&format!("{root}/")))
                .unwrap_or_else(|| panic!("服务端记下了一处两根之外的改动：{p}"))
        })
        .collect();
    assert_eq!(
        roots,
        REMOTE_WRITE_ROOTS.into_iter().collect(),
        "这一趟应当两个根都写到（否则这个相等在一侧空集上成立）"
    );
    let bin_files: Vec<&String> = g
        .files
        .keys()
        .filter(|k| k.starts_with(".cc-monitor/bin/"))
        .collect();
    assert_eq!(
        bin_files,
        vec![".cc-monitor/bin/cc-monitor-backend"],
        "盖掉旧的之后不许留临时件 / 备份"
    );
}

/// 请求行不是 JSON ⇒ 回一条 `bad_request`，链路照旧活着（下一问照答）。
#[tokio::test]
async fn a_garbled_request_line_is_answered_and_the_link_keeps_going() {
    let fs = rig::home(true, false);
    let (mut w, mut r) = link_on(fs).await;
    w.write_all(b"not json\n").await.unwrap();
    let mut back = String::new();
    r.read_line(&mut back).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&back).unwrap();
    assert_eq!(v["code"], "bad_request");
    let v = ask(&mut w, &mut r, serde_json::json!({"op":"home"}), None).await;
    assert_eq!(v["home"], HOME);
}

/// `read` 超过这一问给的上限 ⇒ `too_big`，不把整份塞回来。
#[tokio::test]
async fn a_read_over_the_callers_cap_is_refused_not_truncated() {
    let fs = rig::home(true, false);
    fs.lock().unwrap().files.insert(
        ".cc-monitor/bin/big".to_string(),
        rig::Entry {
            bytes: rig::corpus(5000),
        },
    );
    let (mut w, mut r) = link_on(fs).await;
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"read","path":".cc-monitor/bin/big","max": 4999}),
        None,
    )
    .await;
    assert_eq!(v["code"], "too_big", "{v}");
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"read","path":".cc-monitor/bin/none","max": 10}),
        None,
    )
    .await;
    assert!(
        v["data"].is_null() && v["exists"] == false,
        "读不出 ⇒ 补问在不在：{v}"
    );
}
