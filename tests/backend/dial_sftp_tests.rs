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
        matches!(&r, Err(Refusal::Fenced(m)) if m.contains("落到了可写目录外面")),
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
    // 〔MIG-3b 续 · V41〕`stat` 那一问删了（零调用方）⇒ 对端当不认识的一问回 `unknown_op`，链路照常往下走。
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"stat","path": ".cc-monitor/bin/nope"}),
        None,
    )
    .await;
    assert_eq!(v["code"], "unknown_op", "{v}");
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

/// ★★〔步 23b · 2026-09-19〕**SFTP 那条依赖真的换成了 `russh-sftp` 3.x，而且钉住了。**
///
/// 〔SR1b · 2026-09-24〕**从 monitor 搬来**（`tests/bridge/sftp_tests.rs`）：`russh-sftp` 出了界面清单，今天只在本 crate ——
/// 判据跟着依赖走，读的是本 crate 的清单与 lock。正文逐字未动（除了两份文件的住址）。
///
/// `设计/60 §6.5.3` 现打核过 5 个候选（crates.io / GitHub / docs.rs 三处 API），
/// 结论逐字是「选定的库是 **`russh-sftp` 3.0.0**」。而本仓此前钉的是 `russh-sftp = "2"`
/// —— 本件把它抬到 `"3"`，本条判据就是那一刀的钉子。
///
/// # 为什么要一条判据，而不是「改完就算」
///
/// `Cargo.toml` 里的一个版本区间**掉回去是无声的**：一次 `cargo update` 的误操作、
/// 一次 merge 取错边，都能把 `"3"` 变回 `"2"` 而全仓一条不红 ——
/// 而 `§6.5.3` 那张候选表里全部的论证（传输泛型 `new<S>` · `tokio` 没开 `net` feature
/// 所以它结构上开不了 socket · 扩展集）都是**对着 3.0.0** 做的。
///
/// # 量法：**两侧各读一遍，再互相对账**
///
/// 只钉声明面（`Cargo.toml`），锁定面掉了无声；只钉锁定面，声明面放宽了也无声。
/// ⇒ 两侧都现打抠出来，各自要求**恰好命中一次**（抽取器坏了会零命中地绿），
/// 再断言两边的主版本号相等。
///
/// # 🔴 它买到的与**买不到**的（逐字，不许含糊）
///
/// - **买到**：盘上这两份文本都说「3.x」，且互相对得上。加上 `cargo test` 本身
///   （本条判据要跑起来，整棵树就得先用这一版**编过**），
///   ⇒ 「3.x 编得过、API 形状对得上」这件事是**本条所在的这一趟**顺带证明的。
/// - 🔴 **买不到：「连上一台真远端跑过一次 SFTP」。** 本仓**没有真远端** ——
///   本条只读盘上两份文本，**一个字节都没过网**。`设计/60 §7` 那条「未实测」照旧成立。
/// - 🔴 **买不到：3.x 与 2.x 的行为差异有没有被消化。**（比如 3.0.0 给
///   `read` / `write` 各补了一次收尾、`io::ErrorKind::TimedOut` 现在会映射成超时错。）
///   那几条要真跑才量得出来，本条**不出声**。
/// - 🔴 **买不到：许可。** `russh-sftp` 3.0.0 是 Apache-2.0（本件现打核过它那份
///   `Cargo.toml` 与 `LICENSE`），但**本条判据不读许可** —— 它只读版本号。
///
/// # ⚠ 死值验的覆盖面，如实登记（09-19 现打）
///
/// **声明面那条断言死值验过**：把 `russh-sftp = "3"` 改回 `"2"` ＋ `cargo update --precise 2.3.0`
/// ⇒ 本条当场红，报「声明面掉版本了」。逐字节还原后 sha256 对上。
///
/// 🔴 **锁定面那两条（包块版本 · 两侧主版本对账）没能单独死值验**，理由写清楚：
/// 门禁跑的是 `--locked`，**声明面与锁定面不一致时 cargo 自己就先失败了**
/// ⇒ 构造不出「声明面还是 3、锁定面掉到 2」这个状态。
/// ⇒ 那两条今天是**加固**，不是被验过的牙。**别把它们读成「验过了」。**
#[test]
fn the_sftp_dependency_is_really_on_russh_sftp_three() {
    const CRATE: &str = "russh-sftp";
    // ── 声明面：`src/backend/Cargo.toml` 里那一行 ──────────────────────
    let manifest = include_str!("../../src/backend/Cargo.toml");
    let declared: Vec<&str> = manifest
        .lines()
        .filter_map(|l| l.strip_prefix(&format!("{CRATE} = ")))
        .collect();
    assert_eq!(
        declared.len(),
        1,
        "在 `src/backend/Cargo.toml` 里抠到 {} 行 `{CRATE} = …`（要恰好 1 行）—— \
         抽取器坏了或者那条依赖没了，本条会零命中地绿",
        declared.len()
    );
    let declared = declared[0].trim().trim_matches('"');
    assert!(
        declared.starts_with('3'),
        "声明面掉版本了：`{CRATE} = {declared}` —— `设计/60 §6.5.3` 选定的是 **3.0.0**，\
         那张候选表里全部的论证都是对着 3.x 做的"
    );

    // ── 锁定面：`src/backend/Cargo.lock` 里那个包块 ────────────────────
    let lock = include_str!("../../src/backend/Cargo.lock");
    let name_line = format!("name = \"{CRATE}\"");
    let locked: Vec<&str> = lock
        .split(&name_line)
        .skip(1)
        .filter_map(|after| {
            after
                .lines()
                .find_map(|l| l.strip_prefix("version = "))
                .map(|v| v.trim().trim_matches('"'))
        })
        .collect();
    assert_eq!(
        locked.len(),
        1,
        "在 `src/backend/Cargo.lock` 里抠到 {} 个 `{CRATE}` 包块（要恰好 1 个）：{locked:?} —— \
         两份说明两个版本同时在树上，那正是 `lockfile_conflict_guard` 那一族要治的病",
        locked.len()
    );
    let locked = locked[0];
    assert!(
        locked.starts_with("3."),
        "锁定面掉版本了：lock 里是 `{CRATE} {locked}`，而声明面写着 `{declared}` —— \
         声明放宽 / lock 没跟上，两者任一单独看都像没事"
    );

    // ── 两侧对账：主版本号必须相等 ──────────────────────────────────
    let major = |v: &str| {
        v.trim_start_matches(['^', '=', '~'])
            .split('.')
            .next()
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(
        major(declared),
        major(locked),
        "声明面（{declared}）与锁定面（{locked}）的主版本对不上 —— \
         只钉一侧的话，另一侧掉下去是无声的"
    );
}

// ═══ 〔HX2 · 主会话 D-b「临时件名唯一」〕两个部署者同一个落点 ═══════════════════════════════
//
// 要求住址：主会话 4D 裁 D-b 逐字「多个 monitor 连同一远端：部署只在「我的比盘上的新」时才换（BUILD_ID 可比序），临时件名唯一」；
// 审计 `E-compat.md §2.7` 冲突场景 2（「B 删掉 A 正在写的 `.tmp`」）。

/// 🔴 B3a：落点旁边已经躺着**别人的**固定名 `.tmp` / `.bak`（另一个部署者正在写的那一份）⇒ 这一趟一个都不碰：
/// 改动表里零处固定名（正控：带这一趟后缀的临时件与备份件各数得到一处），完了之后自己的临时件 / 备份件一个不剩。
#[tokio::test]
async fn hx2_a_deploy_never_touches_another_deployers_temp_or_backup() {
    let fs = rig::home(true, false);
    let target = ".cc-monitor/bin/cc-monitor-backend";
    let (their_tmp, their_bak) = (format!("{target}.tmp"), format!("{target}.bak"));
    {
        let mut g = fs.lock().unwrap();
        for (p, b) in [
            (target.to_string(), b"old".to_vec()),
            (their_tmp.clone(), b"theirs".to_vec()),
            (their_bak.clone(), b"theirs".to_vec()),
        ] {
            g.files.insert(p, rig::Entry { bytes: b });
        }
    }
    let s = rig::session_on(fs.clone()).await;
    super::put_atomic(&s, target, b"new", 0o700)
        .await
        .expect("放不上");
    let g = fs.lock().unwrap();
    assert_eq!(g.bytes(target), Some(b"new".to_vec()));
    assert_eq!(
        g.bytes(&their_tmp),
        Some(b"theirs".to_vec()),
        "别人的临时件被动了"
    );
    assert_eq!(
        g.bytes(&their_bak),
        Some(b"theirs".to_vec()),
        "别人的备份件被动了"
    );
    let touched = g.touched();
    assert!(
        !touched.contains(&their_tmp) && !touched.contains(&their_bak),
        "这一趟碰了固定名：{touched:?}"
    );
    let ours = |suffix: &str| {
        touched
            .iter()
            .filter(|p| {
                p.starts_with(&format!("{target}."))
                    && p.ends_with(suffix)
                    && p.len() > target.len() + suffix.len() + 1
            })
            .count()
    };
    assert_eq!(
        ours(".tmp"),
        1,
        "这一趟的临时件（带后缀）该恰一处：{touched:?}"
    );
    assert_eq!(
        ours(".bak"),
        1,
        "这一趟的备份件（带后缀）该恰一处：{touched:?}"
    );
    let left: Vec<&String> = g
        .files
        .keys()
        .filter(|k| k.starts_with(&format!("{target}.")) && *k != &their_tmp && *k != &their_bak)
        .collect();
    assert!(
        left.is_empty(),
        "这一趟留下了自己的临时件 / 备份件：{left:?}"
    );
}

/// 🔴 B3b：两个部署者**真交错**（台架每条写让出几次）往同一个落点放不同的字节 ⇒ 至少一趟成；落点上是两份之一、逐字节；
/// 目录里一个临时件 / 备份件都不剩（失败那一趟把自己挪走的旧件挪回或删掉）。
#[tokio::test]
async fn hx2_two_interleaved_deploys_leave_one_whole_copy_and_no_litter() {
    let fs = rig::home(true, false);
    let target = ".cc-monitor/bin/cc-monitor-backend";
    {
        let mut g = fs.lock().unwrap();
        g.files.insert(
            target.to_string(),
            rig::Entry {
                bytes: b"old".to_vec(),
            },
        );
        g.yield_per_write = 3;
    }
    let (a_bytes, b_bytes) = (rig::corpus(200_000), vec![7u8; 150_000]);
    let (sa, sb) = (
        rig::session_on(fs.clone()).await,
        rig::session_on(fs.clone()).await,
    );
    let (ra, rb) = tokio::join!(
        super::put_atomic(&sa, target, &a_bytes, 0o700),
        super::put_atomic(&sb, target, &b_bytes, 0o700)
    );
    assert!(ra.is_ok() || rb.is_ok(), "两趟都没成：{ra:?} / {rb:?}");
    let g = fs.lock().unwrap();
    let on_disk = g.bytes(target).expect("落点空了");
    assert!(
        on_disk == a_bytes || on_disk == b_bytes,
        "落点上不是两份之一（{} 字节）",
        on_disk.len()
    );
    let litter: Vec<&String> = g
        .files
        .keys()
        .filter(|k| k.starts_with(&format!("{target}.")))
        .collect();
    assert!(litter.is_empty(), "留下了临时件 / 备份件：{litter:?}");
}

/// 〔HX1 · 主会话裁 HX1 拍板项 4〕**部署这一趟建出来的远端目录收成只给本人**（`own_dir::PRIVATE_DIR_MODE`）；
/// **已在的那一层一个字节不碰**（不对它发 SETSTAT）。守的要求：主会话裁「建自家目录 …… 远端 SFTP 部署建目录 …… 0700、已存在不动」。
/// 形状：合成 SFTP 服务端逐条记改动（台架 `sftp_rig`），判服务端看到的，不信被测侧的自述。
#[tokio::test]
async fn hx1_the_dirs_a_deploy_creates_are_made_private_and_an_existing_one_is_left_alone() {
    let fs = rig::home(false, false);
    let (mut w, mut r) = link_on(fs.clone()).await;
    let v = ask(
        &mut w,
        &mut r,
        serde_json::json!({"op":"mkdirs","path": ".cc-monitor/bin"}),
        None,
    )
    .await;
    assert!(v.get("code").is_none(), "{v}");
    let g = fs.lock().unwrap();
    assert_eq!(
        g.dir_modes.get(".cc-monitor/bin").copied(),
        Some(crate::own_dir::PRIVATE_DIR_MODE),
        "新建的 bin 没收成只给本人：{:?}",
        g.mutated
    );
    assert!(
        !g.mutated
            .iter()
            .any(|(verb, p)| verb == "setstat" && p == ".cc-monitor"),
        "已在的 ~/.cc-monitor 被改了权限位：{:?}",
        g.mutated
    );
}
