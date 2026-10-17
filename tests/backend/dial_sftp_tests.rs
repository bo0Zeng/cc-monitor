//! `ssh-dial/sftp.rs` 的判据：远端写围栏（只许暂存区一处）· 解链接 · 部署与传输那几下只写暂存区。
//!
//! 台架是 `sftp_rig`（合成 SFTP 服务端，逐条记改动路径）。**判据看服务端的改动表，不信被测侧的自述。**

use super::rig::{self, HOME};
use super::{fence_lexical, open_for_write, Intent, Refusal, STAGING_ROOT};

// ═══ B3 · 词法那一道：逐形正反 ════════════════════════════════════════════════════════

#[test]
fn the_lexical_fence_admits_exactly_the_staging_root_and_nothing_else() {
    let home = "/home/u";
    // (路径, 形, 期望：Some(根) = 放行且落在那个根；Some("") = 放行且是 `.cc-monitor` 本身；None = 拒)
    let cases: &[(&str, Intent, Option<&str>)] = &[
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
            ".cc-monitor/staging/sub/dir",
            Intent::Dir,
            Some(".cc-monitor/staging"),
        ),
        (
            ".cc-monitor/staging/",
            Intent::Dir,
            Some(".cc-monitor/staging"),
        ),
        (".cc-monitor", Intent::Dir, Some("")),
        // —— 拒（安装位置那一层也是：换进去 · 删掉都由传上去的那一份在那台上自己做）——
        (".cc-monitor/bin/cc-monitor-backend", Intent::File, None),
        ("/home/u/.cc-monitor/bin/ccm", Intent::File, None),
        (".cc-monitor/bin", Intent::Dir, None),
        (".cc-monitor/staging", Intent::File, None),
        (".cc-monitor", Intent::File, None),
        (".cc-monitor/stagingx/y", Intent::File, None),
        (".cc-monitor/staging-old/y", Intent::Dir, None),
        (".cc-monitor/staging/../../.bashrc", Intent::File, None),
        (".cc-monitor/./staging/x", Intent::File, None),
        (".cc-monitor//staging/x", Intent::File, None),
        (".bashrc", Intent::File, None),
        (".local/bin/ccm", Intent::File, None),
        (".cc-monitor/other/x", Intent::Dir, None),
        ("/etc/passwd", Intent::File, None),
        ("/home/u2/.cc-monitor/staging/x", Intent::File, None),
        ("/home/uu/.cc-monitor/staging/x", Intent::File, None),
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
        "/C:/Users/u/.cc-monitor/staging/x",
        Intent::File
    )
    .is_err());
}

/// 🔴 写根**恰好暂存区一处**（异源：期望手写，不取自本文件的常量）。
#[test]
fn the_declared_write_root_is_exactly_staging() {
    assert_eq!(STAGING_ROOT, ".cc-monitor/staging");
    // 暂存区那一根与提交那一侧（后端 `files_commit`）是同一个串：那一条住后端（`dial_tests`），这里够不到后端。
}

// ═══ 解链接那一道 ══════════════════════════════════════════════════════════════════════

/// 根底下一条**目录链接**指到根外（`staging/evil -> ~/.ssh`）：词法那一道放行，解链接那一道必须拒，服务端零改动。
#[tokio::test]
async fn a_symlinked_directory_under_a_root_cannot_carry_a_write_out() {
    let fs = rig::home(false, true);
    {
        let mut g = fs.lock().unwrap();
        g.dirs.insert(".ssh".to_string());
        g.links
            .insert(".cc-monitor/staging/evil".to_string(), ".ssh".to_string());
    }
    let s = rig::session_on(fs.clone()).await;
    let r = super::put_atomic(&s, ".cc-monitor/staging/evil/authorized_keys", b"k", 0o600).await;
    assert!(
        matches!(&r, Err(Refusal::Fenced { said: m, .. }) if copy_core::copy_matches("beSftp.fence.escaped", &m)),
        "解链接那一道没拦住：{r:?}"
    );
    let r2 = super::make_dirs(&s, ".cc-monitor/staging/evil/deeper").await;
    assert!(
        matches!(r2, Err(Refusal::Fenced { .. })),
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
        .insert(".cc-monitor/staging/sub".to_string());
    super::put_atomic(&s, ".cc-monitor/staging/sub/x", b"k", 0o600)
        .await
        .expect("根底下的真子目录该放行");
    assert_eq!(
        fs.lock().unwrap().bytes(".cc-monitor/staging/sub/x"),
        Some(b"k".to_vec())
    );
}

/// 写根**本身**是链接（`staging -> ~/.ssh` · `.cc-monitor -> 别处`）：解链接之后「在根底下」恒成立，只问父目录拦不住；
/// 围栏问根本身 ⇒ 拒、出声、服务端零改动。
#[tokio::test]
async fn a_write_root_that_is_itself_a_link_is_refused() {
    for (link, target, make) in [
        (".cc-monitor/staging", ".ssh", vec![".ssh"]),
        (
            ".cc-monitor",
            "elsewhere",
            vec!["elsewhere", "elsewhere/staging"],
        ),
    ] {
        let fs = rig::home(false, false);
        {
            let mut g = fs.lock().unwrap();
            if link == ".cc-monitor" {
                g.dirs.remove(".cc-monitor");
            }
            for d in make {
                g.dirs.insert(d.to_string());
            }
            g.links.insert(link.to_string(), target.to_string());
        }
        let s = rig::session_on(fs.clone()).await;
        let r = super::put_atomic(&s, ".cc-monitor/staging/ccm", b"k", 0o700).await;
        assert!(
            matches!(&r, Err(Refusal::Fenced { said: m, .. }) if copy_core::copy_matches("beSftp.fence.rootNotOwn", m)),
            "`{link}` 是链接，写却没拦住：{r:?}"
        );
        let r2 = super::make_dirs(&s, ".cc-monitor/staging").await;
        assert!(
            matches!(r2, Err(Refusal::Fenced { .. })),
            "`{link}` 是链接，建目录没拦住：{r2:?}"
        );
        assert!(
            fs.lock().unwrap().mutated.is_empty(),
            "被拒的请求在服务端留下了改动：{:?}",
            fs.lock().unwrap().mutated
        );
    }
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
        matches!(r, Err(Refusal::Fenced { said: ref m, .. }) if m.contains(copy_core::copy_static!("rsFilewinKind.label.link"))),
        "链接没拦住"
    );
    assert!(fs.lock().unwrap().mutated.is_empty());
}

// ═══ 部署 · 传输那几下：改动只落在暂存区 ═════════════════════════════════════════════════

/// 🔴🔴 **B2：部署那几下（建目录 · 放 ＋ 读回 · 盖掉旧的 · 删）与传输开写跑下来，服务端记下的改动路径全在暂存区里**
/// （非空、全在根下）；越界的五种（含往安装位置那一层写）一律 `Fenced`、服务端改动表**零增长**。
#[tokio::test]
async fn deploy_and_transfer_writes_touch_only_staging_and_refuse_everything_else() {
    let fs = rig::home(false, false);
    let s = rig::session_on(fs.clone()).await;

    // 首次部署：暂存区还不在 ⇒ 逐级建（`deploy::install` 那一下）。
    super::make_dirs(&s, &format!("{HOME}/.cc-monitor/staging"))
        .await
        .expect("建暂存区");
    let bin = rig::corpus(100_000);
    super::put_verified(
        &s,
        &format!("{HOME}/.cc-monitor/staging/cc-monitor-backend"),
        &bin,
        0o700,
    )
    .await
    .expect("读回逐字节应当相同");
    assert_eq!(
        fs.lock()
            .unwrap()
            .bytes(".cc-monitor/staging/cc-monitor-backend"),
        Some(bin.clone())
    );
    // 再放一次（盖掉旧的）：旧的改名成 `.bak` 再删，临时件上位 —— 目录里只剩那一份。
    super::put_atomic(&s, ".cc-monitor/staging/cc-monitor-backend", b"new", 0o700)
        .await
        .expect("盖掉旧的");
    // 传输那一下：暂存件开写、再删（撤了那一形）。
    drop(
        open_for_write(&s, ".cc-monitor/staging/aa.part", true)
            .await
            .expect("开暂存件"),
    );
    assert_eq!(
        super::remove(&s, ".cc-monitor/staging/aa.part").await.ok(),
        Some(true)
    );

    // —— 越界：五种形，一律拒，改动表零增长 ——
    let before = fs.lock().unwrap().mutated.len();
    let refusals = [
        super::put_atomic(&s, ".bashrc", b"x", 0o644).await.err(),
        super::put_atomic(&s, &format!("{HOME}/.local/bin/ccm"), b"x", 0o755)
            .await
            .err(),
        super::remove(&s, "/etc/passwd").await.err(),
        super::make_dirs(&s, ".cc-monitor/staging/../../.ssh")
            .await
            .err(),
        super::put_atomic(&s, ".cc-monitor/bin/ccm", b"x", 0o700)
            .await
            .err(),
    ];
    for (i, r) in refusals.iter().enumerate() {
        assert!(
            matches!(r, Some(Refusal::Fenced { .. })),
            "第 {i} 种越界没被围栏拒：{r:?}"
        );
    }
    assert_eq!(
        fs.lock().unwrap().mutated.len(),
        before,
        "被拒的请求在服务端留下了改动"
    );

    // 🔴 改动全在暂存区里（非空：这一趟确实写到了）。
    let g = fs.lock().unwrap();
    let touched = g.touched();
    assert!(
        !touched.is_empty(),
        "这一趟一处都没写到（全称在空集上成立）"
    );
    for p in &touched {
        assert!(
            p == STAGING_ROOT || p.starts_with(&format!("{STAGING_ROOT}/")),
            "服务端记下了一处暂存区之外的改动：{p}"
        );
    }
    let staged: Vec<&String> = g
        .files
        .keys()
        .filter(|k| k.starts_with(".cc-monitor/staging/"))
        .collect();
    assert_eq!(
        staged,
        vec![".cc-monitor/staging/cc-monitor-backend"],
        "盖掉旧的之后不许留临时件 / 备份"
    );
}

/// ★★**SFTP 那条依赖真的换成了 `russh-sftp` 3.x，而且钉住了。**
///
/// **从 monitor 搬来**（`tests/frontend/shell/sftp_tests.rs`）：`russh-sftp` 出了界面清单，今天只在本 crate ——
/// 判据跟着依赖走，读的是本 crate 的清单与 lock。正文逐字未动（除了两份文件的住址）。
///
/// 现打核过 5 个候选（crates.io / GitHub / docs.rs 三处 API），
/// 结论逐字是「选定的库是 **`russh-sftp` 3.0.0**」。而本仓此前钉的是 `russh-sftp = "2"`
/// —— 这里把它抬到 `"3"`，本条判据就是那一刀的钉子。
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
///   本条只读盘上两份文本，**一个字节都没过网**。那条「未实测」照旧成立。
/// - 🔴 **买不到：3.x 与 2.x 的行为差异有没有被消化。**（比如 3.0.0 给
///   `read` / `write` 各补了一次收尾、`io::ErrorKind::TimedOut` 现在会映射成超时错。）
///   那几条要真跑才量得出来，本条**不出声**。
/// - 🔴 **买不到：许可。** `russh-sftp` 3.0.0 是 Apache-2.0（实测核过它那份
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
/// 认残件的形状 == `put_atomic` · 暂存那一份真起的名字（异源 = 生产那个 `trip_tag`）：临时件 · 备份件 · 暂存件都认；
/// 同一个名字换掉后缀 / 去掉一段 ⇒ 不认。
#[test]
fn the_leftover_shape_is_exactly_what_put_atomic_names() {
    let tag = super::trip_tag();
    for suffix in ["tmp", "bak"] {
        assert!(
            super::is_trip_leftover(&format!("ccm.{tag}.{suffix}")),
            "{tag}.{suffix}"
        );
    }
    let staged = super::staged_name("cc-monitor-backend");
    assert!(super::is_trip_leftover(&staged), "{staged}");
    let short = tag.rsplit_once('-').unwrap().0;
    for not in [
        format!("ccm.{tag}.part"),
        format!("ccm.{short}.tmp"),
        format!(".{tag}.tmp"),
    ] {
        assert!(!super::is_trip_leftover(&not), "{not}");
    }
}

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
        "声明面掉版本了：`{CRATE} = {declared}` —— 选定的是 **3.0.0**，\
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

// ═══ 〔临时件名唯一〕两个部署者同一个落点 ═══════════════════════════════
//
// 要求：「多个 monitor 连同一远端：部署只在「我的比盘上的新」时才换（BUILD_ID 可比序），临时件名唯一」；
// 冲突场景：B 删掉 A 正在写的 `.tmp`。

/// 🔴 B3a：落点旁边已经躺着**别人的**固定名 `.tmp` / `.bak`（另一个部署者正在写的那一份）⇒ 这一趟一个都不碰：
/// 改动表里零处固定名（正控：带这一趟后缀的临时件与备份件各数得到一处），完了之后自己的临时件 / 备份件一个不剩。
#[tokio::test]
async fn hx2_a_deploy_never_touches_another_deployers_temp_or_backup() {
    let fs = rig::home(false, true);
    let target = ".cc-monitor/staging/cc-monitor-backend";
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
    let fs = rig::home(false, true);
    let target = ".cc-monitor/staging/cc-monitor-backend";
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

/// **部署这一趟建出来的远端目录收成只给本人**（`own_dir::PRIVATE_DIR_MODE`）；
/// **已在的那一层一个字节不碰**（不对它发 SETSTAT）。守的要求：「建自家目录 …… 远端 SFTP 部署建目录 …… 0700、已存在不动」。
/// 形状：合成 SFTP 服务端逐条记改动（台架 `sftp_rig`），判服务端看到的，不信被测侧的自述。
#[tokio::test]
async fn hx1_the_dirs_a_deploy_creates_are_made_private_and_an_existing_one_is_left_alone() {
    let fs = rig::home(false, false);
    let s = rig::session_on(fs.clone()).await;
    super::make_dirs(&s, ".cc-monitor/staging")
        .await
        .expect("建暂存区");
    let g = fs.lock().unwrap();
    assert_eq!(
        g.dir_modes.get(".cc-monitor/staging").copied(),
        Some(crate::guard_support::TEST_DIR_MODE),
        "新建的暂存区没收成只给本人：{:?}",
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

// ═══ 原因词：SFTP 那一层的失败落到哪个词 ═════════════════════════════════════════════

/// 屏上那一格原因要说准：SFTP 文件句柄的读写把状态包成 `io::Error`（种类多是 Other，状态码只剩服务端那句话），
/// 别的路把 `russh_sftp` 的错整个包进去 —— 两形都按 SFTP 状态认出原因词；认不出才「原因不明」。
#[test]
fn sftp_failures_wrapped_in_io_errors_still_land_on_the_right_reason_word() {
    use russh_sftp::client::error::Error as E;
    use russh_sftp::protocol::{Status, StatusCode as C};
    use std::io::{Error as Io, ErrorKind as K};
    let st = |code: C| {
        E::Status(Status {
            id: 1,
            status_code: code,
            error_message: String::new(),
            language_tag: String::new(),
        })
    };
    let word = |k: &str| copy_core::copy_text(k, &[]);
    let cases: Vec<(Io, String)> = vec![
        // 句柄读写那一形：服务端那句标准话（OpenSSH sftp-server）。
        (Io::other("No such file"), word("reason.io.notFound")),
        (Io::other("Permission denied"), word("reason.io.denied")),
        (
            Io::other("Operation unsupported"),
            word("reason.sftp.unsupported"),
        ),
        (Io::other("Bad message"), word("reason.sftp.badMessage")),
        (Io::other("Failure"), word("reason.io.unknown")),
        // 整个包进去那一形。
        (Io::from(st(C::NoSuchFile)), word("reason.io.notFound")),
        (Io::from(st(C::PermissionDenied)), word("reason.io.denied")),
        (
            Io::from(st(C::OpUnsupported)),
            word("reason.sftp.unsupported"),
        ),
        (Io::from(E::Timeout), word("reason.sftp.noAnswer")),
        (
            Io::from(st(C::ConnectionLost)),
            word("reason.sftp.connectionLost"),
        ),
        (
            Io::other("Connection lost"),
            word("reason.sftp.connectionLost"),
        ),
        (Io::from(K::BrokenPipe), word("reason.sftp.connectionLost")),
        (
            Io::from(K::ConnectionReset),
            word("reason.sftp.connectionLost"),
        ),
        (
            Io::from(E::Limited("n".into())),
            word("reason.sftp.overLimit"),
        ),
        // 本来就是 IO 种类的照 IO 那张表。
        (Io::from(K::PermissionDenied), word("reason.io.denied")),
        (Io::other("something else"), word("reason.io.unknown")),
    ];
    for (e, want) in cases {
        assert_eq!(super::why_of_io(&e), want, "{e:?}");
    }
}

// ═══ 放 ＋ 读回（`put_verified`：读回不对当场删）══════════════════════════════════════════

/// 读回逐字节相同 ⇒ 过；落点上是那一份。
#[tokio::test]
async fn a_verified_put_that_reads_back_the_same_bytes_passes() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let bin = rig::corpus(70_000);
    super::put_verified(&s, ".cc-monitor/staging/ccm", &bin, 0o700)
        .await
        .expect("读回相同该过");
    assert_eq!(
        fs.lock().unwrap().bytes(".cc-monitor/staging/ccm"),
        Some(bin)
    );
}

/// 要求：「读回不对就删」。等长、首字节翻了一位（只比长度查不出来）⇒ 失败那一句说清坏在哪 ＋ 已删掉；落点上不留那一份。
#[tokio::test]
async fn a_verified_put_that_reads_back_wrong_bytes_removes_what_it_put() {
    let fs = rig::home(false, true);
    fs.lock().unwrap().corrupt_reads = true;
    let s = rig::session_on(fs.clone()).await;
    let bin = rig::corpus(70_000);
    let e = super::put_verified(&s, ".cc-monitor/staging/ccm", &bin, 0o700)
        .await
        .unwrap_err();
    assert!(
        matches!(&e, Refusal::Io { said, .. } if copy_core::copy_matches("rsSftp.upload.badRemoved", said)
            && said.contains(&copy_core::copy_text("rsSftp.verify.contentDiffers", &[
                ("path", ".cc-monitor/staging/ccm"), ("expectedLen", &bin.len().to_string()), ("at", "0")]))),
        "{e:?}"
    );
    assert_eq!(
        fs.lock().unwrap().bytes(".cc-monitor/staging/ccm"),
        None,
        "传坏的那一份还在"
    );
    assert!(
        fs.lock()
            .unwrap()
            .mutated
            .iter()
            .any(|(v, p)| v == "remove" && p == ".cc-monitor/staging/ccm"),
        "没删落点那一份：{:?}",
        fs.lock().unwrap().mutated
    );
}

/// 读回不对、删也删不掉 ⇒ 那一句照实说「未删掉」，删那一步的原话进详情；不许说成「已删掉」。
#[tokio::test]
async fn a_bad_readback_that_cannot_be_removed_says_so() {
    let fs = rig::home(false, true);
    let s = rig::session_on(fs.clone()).await;
    let bin = rig::corpus(5_000);
    {
        let mut g = fs.lock().unwrap();
        g.corrupt_reads = true;
        g.refuse_remove = true;
    }
    let e = super::put_verified(&s, ".cc-monitor/staging/ccm", &bin, 0o700)
        .await
        .unwrap_err();
    assert!(
        matches!(&e, Refusal::Io { said, raw: Some(_) } if copy_core::copy_matches("rsSftp.upload.badKept", said)),
        "{e:?}"
    );
}

/// 判读回的话（随判定从 monitor `sftp_tests` 搬来，期望不变）：等长一字节不同要指出位置；截短 · 读不回各一句；逐字节相同才过。
#[test]
fn the_readback_verdict_names_what_went_wrong() {
    let want = b"#!/bin/sh\nexec ccm \"$@\"\n";
    let mut bad = want.to_vec();
    let k = want.len() / 2;
    bad[k] ^= 1;
    let e = super::verify_readback("/r/x", want, Some(&bad)).unwrap_err();
    assert!(
        copy_core::copy_matches_with(
            "rsSftp.verify.contentDiffers",
            &[("at", &k.to_string())],
            &e
        ),
        "要指出位置：{e}"
    );
    assert!(e.contains("/r/x"), "{e}");
    let e = super::verify_readback("/r/x", &[0u8; 10], Some(&[0u8; 5])).unwrap_err();
    assert!(
        copy_core::copy_matches_with("rsSftp.verify.lengthDiffers", &[("expectedLen", "10")], &e),
        "{e}"
    );
    let e = super::verify_readback("/r/x", &[0u8; 10], None).unwrap_err();
    assert!(
        copy_core::copy_matches("rsSftp.verify.unreadable", &e),
        "{e}"
    );
    assert!(super::verify_readback("/r/d", &[0, 0xff, 7], Some(&[0, 0xff, 7])).is_ok());
    assert!(super::verify_readback("/r/d", &[], Some(&[])).is_ok());
}
