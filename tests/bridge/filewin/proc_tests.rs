//! 〔第十三刀 2026-09-23〕`super::proc` 那一摞判据 —— **进程形态**。
//!
//! # 这一摞逐条买什么
//!
//! | 判据 | 它钉的那一形 | 少了它会怎样 |
//! |---|---|---|
//! | [`a_seed_survives_the_trip_through_a_process_boundary`] | 那一屏 ＋ 源 ＋ cwd ＋ reveal **整份**过得去（相等断言） | 种子漂一格 ⇒ 窗口开在别处 / 少几行，而没有一句话 |
//! | [`a_seed_that_cannot_be_read_is_a_loud_failure_not_a_default_window`] | 解不出来的种子**出声**，不补默认值（含阴性对照） | 「开了个空窗」与「成功」在屏幕上分不开 |
//! | [`the_window_binary_is_never_guessed`] | 那份二进制**只有两处**来路，都不在就出声并说出看过哪儿 | `D11`：静默回落 / `D7`：归因说成「窗口画不出来」 |
//! | 🔴 [`opening_a_window_three_times_really_starts_three_independent_processes`] | **一趟一个进程、pid 互不相同、三趟结局逐字相同** | 那正是旧形态的病：第二趟被一个**进程级**标志挡回来 |
//! | [`a_window_process_that_dies_at_once_comes_back_as_a_reason`] | 当场死掉的那一形回的是一句非空的原因 | 静默成功那一形又回来了 |
//! | [`the_entry_command_has_no_in_process_fallback_left`] | 零命中型：入口那条路上**没有**「同进程开一个」的写法 | `D11` 逐字：不许留退路 |
//!
//! # ⚠ 这一摞**买不到**什么（逐条）
//!
//! - **「窗口真的出现在屏幕上」一格都没有。** 本机 `XDG_SESSION_TYPE=tty`
//!   （`真相源/99 §一`）。这里量的是**进程**：起没起来、pid 是不是新的、退出码是什么。
//!   实景那一维住 `shell_tests` 那个 Xvfb 台架（它自己的头注写清了它买不到的四样）。
//! - **「三趟都成功」买不到**，只买到「**三趟都一样**」：没有图形会话时三趟都失败，
//!   而本摞钉的是「失败的**理由与形状**逐趟相同」⇒ 结构上没有任何进程级状态
//!   让第二趟与第一趟不同。**那正是旧形态缺的那一格**（旧形态下第二趟与第一趟
//!   必然不同），但它**不是**「第二趟开窗成功」的证据。后者只有实景那一格买得到。
//! - **后端那条通道一格都没量。** 窗口进程里它够不着（逐条理由住 `proc` 头注 §四），
//!   而「够不着之后窗口上那行橙字说得对不对」是 `source` / `find` 那两摞的活。

use super::*;
use crate::filewin::source::Row;
use crate::ssh_source::RemoteConfig;

/// 改环境变量那几条**必须串行**：`std::env` 是进程级的，而 `cargo test` 默认并行。
///
/// ⚠ 不加这道的症状是**偶发**假红（另一条判据正好在读同一个变量）——
/// 本仓对「偶发红会被人学会重跑绕过」记过一笔，所以这里直接串起来。
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 一台**字段全填满**的合成远端 —— 种子对拍要的是「每一格都过得去」。
fn synthetic_cfg() -> RemoteConfig {
    RemoteConfig {
        host: "10.0.0.7".to_string(),
        label: "台架-远端".to_string(),
        port: 2222,
        user: "zbl".to_string(),
        key_path: Some("/home/zbl/.ssh/id_ed25519".to_string()),
        backend_path: "/opt/cc-monitor-backend".to_string(),
        host_key_fingerprint: Some("SHA256:xxxx".to_string()),
        addresses: vec!["10.0.0.7".to_string(), "fd00::7".to_string()],
        jump: None,
    }
}

fn synthetic_request() -> OpenRequest {
    OpenRequest {
        source: Source::remote(synthetic_cfg()),
        cwd: "/home/zbl/带空格 的目录".to_string(),
        rows: vec![
            // 🔴〔补齐五项〕链接与时间两格**也进种子对拍**：它们过不了这条边界的话，
            //    窗口第一屏就画不出那两列（上一版就是这样，登记成代价挂着）。
            Listed {
                row: Row {
                    name: "子目录".to_string(),
                    path: "/home/zbl/带空格 的目录/子目录".to_string(),
                    is_dir: true,
                    size: 0,
                    lossy_name: false,
                },
                link: true,
                mtime_secs: Some(1_700_000_000),
            },
            Listed::plain(Row {
                // 🔴 有损那一格**必须进种子对拍**：它是一个**事实**（那串字节不是合法
                //    UTF-8），而 JSON 只装得下 `String` ⇒ 不把这一格带过去，
                //    窗口那侧就会把一个有损名字画成一个正常名字。
                name: "坏\u{FFFD}名字".to_string(),
                path: "/home/zbl/带空格 的目录/坏\u{FFFD}名字".to_string(),
                is_dir: false,
                size: 4_097,
                lossy_name: true,
            }),
        ],
        reveal: Some("坏\u{FFFD}名字".to_string()),
        handoff: synthetic_handoff(),
    }
}

/// 一份合成交接件：回环地址 ＋ 一把**合成**钥匙（不是任何一个真口的钥匙）。
fn synthetic_handoff() -> crate::chan::host::Handoff {
    crate::chan::host::Handoff {
        addr: "127.0.0.1:9".parse().expect("合法地址"),
        key: crate::chan::wire::Key("k".repeat(64)),
        frame: 1 << 20,
    }
}

/// 🔴 **那一屏整份过得去 —— 相等断言，不是「大致对得上」。**
#[test]
fn a_seed_survives_the_trip_through_a_process_boundary() {
    let want = synthetic_request();
    let wire = encode_request(&want).expect("种子序列化不了 —— 那这条路整条不通");
    let got = decode_request(&wire).expect("自己写的种子自己读不动");
    // ── `OpenRequest` 没有 `PartialEq`（`RemoteConfig` 也没有）⇒ 逐格比。
    //    🔴 **逐格比不是偷懒**：它让「新加一个字段而没进种子」当场红在下面那条
    //    字段数自检上，而一个 `PartialEq` 的 `assert_eq!` 在**字段被漏掉时恒真**
    //    （漏掉的那一格两侧都是默认值）。
    assert_eq!(got.cwd, want.cwd, "cwd 漂了 —— 窗口会开在别处");
    assert_eq!(got.reveal, want.reveal, "reveal 漂了 —— 高亮落在别的行上");
    assert_eq!(got.rows, want.rows, "那一屏漂了");
    // 🔴〔2026-09-23 本机侧退役〕**这里少了一次「判别式过得去吗」的比对。**
    //    从前 `Source` 是个两格枚举，这一段要先 `match` 出两侧都是 `Remote`
    //    （对不上就 `panic!("源的判别式没过得去")`），下面还单独喂一份
    //    `Source::Local` 的种子对拍它那一格。`Source` 收成 newtype 之后
    //    **判别式这个概念不存在了** ⇒ 那两处不是被删掉的判据，是它们判的东西没了。
    {
        let (a, b) = (got.source.cfg(), want.source.cfg());
        {
            assert_eq!(
                (
                    &a.host,
                    &a.label,
                    a.port,
                    &a.user,
                    &a.key_path,
                    &a.backend_path,
                    &a.host_key_fingerprint,
                    &a.addresses
                ),
                (
                    &b.host,
                    &b.label,
                    b.port,
                    &b.user,
                    &b.key_path,
                    &b.backend_path,
                    &b.host_key_fingerprint,
                    &b.addresses
                ),
                "远端配置漂了 —— 窗口会去连另一台机器（或者连不上而说不清为什么）"
            );
        }
    }
    // 🔴〔F2〕交接件整份过得去（地址 · 帧长 · 钥匙），否则窗口拨不回来。
    assert_eq!(got.handoff.addr, want.handoff.addr, "交接件的地址漂了");
    assert_eq!(got.handoff.frame, want.handoff.frame, "交接件的帧长漂了");
    assert!(got.handoff.key == want.handoff.key, "交接件的钥匙漂了");
    // 🔴 钥匙**不进日志**：整份种子的 `Debug` 里找不到它（`Handoff` / `Key` 的 `Debug` 手写成不打印）。
    assert!(
        !format!("{want:?}").contains(&want.handoff.key.0),
        "开窗种子的 `Debug` 把钥匙打出来了 —— 哪天一条 `tracing!(\"{{:?}}\")` 就把它带进日志"
    );
    // ★ 反向自检：**线上那份字节里真的装着那几个值**。
    //   少了这一比，一个「原样回传入参」的假实现照样绿（encode/decode 都不走 serde）。
    for needle in ["10.0.0.7", "带空格 的目录", "id_ed25519"] {
        assert!(
            wire.contains(needle),
            "线上那份字节里找不到 {needle:?} —— encode/decode 可能压根没经过 serde：{wire}"
        );
    }
}

/// **解不出来的种子出声，不补默认值。** 含阴性对照。
#[test]
fn a_seed_that_cannot_be_read_is_a_loud_failure_not_a_default_window() {
    for (tag, raw) in [
        ("空的", ""),
        ("只有空白", "   \n\t "),
        ("不是 JSON", "{这不是 JSON"),
        ("是 JSON 但形状不对", r#"{"cwd":"/tmp"}"#),
        (
            "截断在一半",
            r#"{"source":"Local","cwd":"/tmp","rows":[{"name""#,
        ),
    ] {
        let e = decode_request(raw).err().unwrap_or_else(|| {
            panic!(
                "{tag}：这样的种子居然读出了一个窗口 —— \
                                       那意味着某处在补默认值，而用户会看到一个开在别处的窗口"
            )
        });
        assert!(!e.is_empty(), "{tag}：报了错但原因是空的");
    }
    // ── 阴性对照：**合法的那一份必须过** ────────────────────────────
    //    少了它，一个「恒回 Err」的实现照样绿，而那时**每一次**开窗都失败。
    let ok = encode_request(&synthetic_request()).expect("序列化");
    assert!(
        decode_request(&ok).is_ok(),
        "合法种子被拒了 —— 那会让每一次开窗都失败"
    );
}

/// **那份二进制只有两处来路，都不在就出声并说出看过哪儿。**
#[test]
fn the_window_binary_is_never_guessed() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("filewin-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("造不出临时目录");
    // ① 纯函数那一格：落点的形状（Windows 上带 `.exe`）。
    let p = window_bin_in(&dir);
    assert_eq!(
        p,
        dir.join(format!("{BIN_STEM}{}", std::env::consts::EXE_SUFFIX)),
        "落点的拼法变了"
    );
    // ② 环境变量指着一个**不存在**的路径 ⇒ 出声，而且**把看过的都说出来**。
    let missing = dir.join("根本没有这个文件");
    std::env::set_var(BIN_ENV, &missing);
    let e = resolve_window_bin().expect_err("指着一个不存在的文件居然解出来了");
    assert!(
        e.contains("根本没有这个文件"),
        "报错里没有那条被看过的路径 —— `D7`：归因得说准是「二进制不在」而不是「窗口画不出来」：{e}"
    );
    assert!(e.contains(BIN_STEM), "报错里没点名要找的是哪个二进制：{e}");
    // ③ 阴性对照：指着一个**真存在**的文件 ⇒ 原样回它，不再往别处找。
    let real = dir.join("假装是那个二进制");
    std::fs::write(&real, b"#!/bin/true\n").expect("写不出那个文件");
    std::env::set_var(BIN_ENV, &real);
    assert_eq!(
        resolve_window_bin().expect("指着一个真文件却解不出来"),
        real,
        "环境变量指的那一份没被用上 —— 判据从此指不动它"
    );
    std::env::remove_var(BIN_ENV);
    std::fs::remove_dir_all(&dir).ok();
}

/// 一个**一定在**的替身二进制：它读 stdin 到 EOF 然后退出。
///
/// 🔴 **为什么用替身，而不用那份真的 `cc-monitor-filewin`** —— 不是省事：
/// 门禁那一格逐字跑 `cargo test --workspace --exclude code-picture-core --lib`，
/// 而 **`--lib` 不构建 bin** ⇒ 那份二进制在门禁跑的时候**根本不存在**。
/// 照它写的判据在门禁上会恒红，而「判据恒红」与「判据不存在」一样没用。
///
/// ⇒ 三格分开，各自说清买到什么：
/// - **进程模型**（一趟一个进程 · pid 互不相同 · 种子真的从 stdin 进去了）⇒ 本替身；
/// - **那份 bin 真的托管了窗口进程的躯体** ⇒ [`the_window_binary_target_really_hosts_the_body`]（源码型）；
/// - **第二、第三趟窗口真的摆上屏幕** ⇒ `shell_tests` 那个 Xvfb 台架。
///
/// ⚠ 找不到时**当场 panic 并说清「这一格判不了」** —— 不是 `return`。
/// 「判不了」被写成「过了」是本仓的头号病形，这一条自己不许犯。
#[cfg(unix)]
fn stdin_eating_stand_in() -> std::path::PathBuf {
    for p in ["/bin/cat", "/usr/bin/cat"] {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return p;
        }
    }
    panic!(
        "本机上 `/bin/cat` 与 `/usr/bin/cat` 都不在 —— **这一格判不了，不是过了**。\n\
         本条要的只是一个「把 stdin 读到 EOF 然后退出」的二进制当替身。"
    )
}

/// 🔴🔴 **一趟一个进程：三趟的 pid 互不相同，三趟的结局逐字相同。**
///
/// # 它为什么是这一摞的承重墙
///
/// 旧形态的病**不是**「开窗会失败」，是「**第二趟与第一趟不一样**」：
/// winit 那个「事件循环已经建过了」的标志是**进程级**的
/// ⇒ 第一趟能开、第二趟必然被它挡回来。
/// ⇒ 换成一趟一个进程之后，要买的性质就是「**趟与趟之间没有共享状态**」，
/// 而它在一台**没有图形会话**的机器上也量得到：三趟读数相同就是它的读数。
///
/// # ⚠ 它买不到什么
///
/// - **买不到「窗口起来了」** —— 起的是替身，不是窗口。那一维住 Xvfb 台架。
/// - **买不到 Windows 上的任何一格**（本条 `cfg(unix)`；手上没有 Windows 机器）。
#[cfg(unix)]
#[test]
fn opening_a_window_three_times_really_starts_three_independent_processes() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(BIN_ENV, stdin_eating_stand_in());
    let req = OpenRequest {
        source: Source::remote(synthetic_cfg()),
        cwd: "/tmp".to_string(),
        rows: Vec::new(),
        reveal: None,
        handoff: synthetic_handoff(),
    };
    let mut pids: Vec<u32> = Vec::new();
    let mut codes: Vec<String> = Vec::new();
    for trip in 1..=3 {
        let child = spawn_window(&req).unwrap_or_else(|e| {
            panic!("第 {trip} 趟连进程都起不来：{e}\n⚠ 这一形是台架坏了，不是被测性质红了")
        });
        pids.push(child.id());
        let st = child
            .wait_for_status()
            .unwrap_or_else(|e| panic!("第 {trip} 趟等不到它退出：{e}"));
        codes.push(format!("{st}"));
    }
    println!("  三趟现打：pid {pids:?} · 结局 {codes:?}");
    // ① **三个 pid 互不相同** —— 「一趟一个进程」在这一格上是可判的。
    let uniq: std::collections::BTreeSet<u32> = pids.iter().copied().collect();
    assert_eq!(
        uniq.len(),
        3,
        "三趟只起了 {} 个不同的进程（{pids:?}）—— 「一趟一个进程」没成立",
        uniq.len()
    );
    // ② 🔴 **三趟结局逐字相同** —— 没有任何进程级状态让第二趟与第一趟不同。
    assert_eq!(
        codes[0], codes[1],
        "第二趟的结局与第一趟不同（{codes:?}）—— 那正是旧形态的病：\
         某个**进程级**状态把第二趟挡回去了"
    );
    assert_eq!(codes[1], codes[2], "第三趟的结局又变了（{codes:?}）");
    std::env::remove_var(BIN_ENV);
}

/// 🔴 **端到端：那份种子真的从 stdin 进到了另一个进程里。**
///
/// 分成独立一条（而不是并进上面那条）的理由：上面那条钉「三趟一样」，
/// 而**三趟一样也可能是三趟都什么都没收到**。这一条是那条的反空真锚。
///
/// ⚠ **它走的不是 [`spawn_window`] 整条，而是那一跳本身**（`write_seed`）：
/// 生产那条路刻意**不接** stdout（接成管子而没人读，对面一写满就卡死）
/// ⇒ 判据要把这一跳量成端到端的，只能自己接一根。
/// ⇒ 本条买「那份字节真的到了对面」，`spawn_window` 整条由上面那条买。
///
/// ⚠ 本条自己那句 `.stdin(piped())` 是**第二份**写法，而那件耦合**有人守**：
/// 生产那侧漏了它 ⇒ `write_seed` 当场拿不到管子 ⇒ `spawn_window` 回 `Err`
/// ⇒ 上面那条三趟判据会**响亮地**红（「连进程都起不来」）。如实记，别读成零判据。
#[cfg(unix)]
#[test]
fn the_seed_really_arrives_on_the_child_process_stdin() {
    let req = synthetic_request();
    let seed = encode_request(&req).expect("序列化");
    let mut cmd = std::process::Command::new(stdin_eating_stand_in());
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = crate::spawn_managed::spawn_managed_cmd(
        &mut cmd,
        crate::spawn_managed::ConsolePolicy::Hidden,
        crate::spawn_managed::Lifetime::Detached,
        crate::spawn_managed::StderrSink::Inherit,
    )
    .expect("起不了替身进程 —— 台架坏了");
    write_seed(&mut child, &seed).expect("种子送不进替身的 stdin");
    let mut seen = String::new();
    std::io::Read::read_to_string(
        &mut child.stdout.take().expect("替身没有 stdout 管子"),
        &mut seen,
    )
    .expect("读不到替身吐回来的东西");
    let _ = child.wait_for_status();
    // 🔴 **相等断言**：它收到的**就是**我们编出来的那一份，不多不少。
    assert_eq!(
        seen, seed,
        "另一个进程从 stdin 收到的种子与我们送出去的不是同一份字节"
    );
    // ★ 反向自检：那份字节**不是空的**，而且真的装着东西（否则上面是「空 == 空」）。
    assert!(
        seen.contains("10.0.0.7") && seen.len() > 100,
        "送到对面的种子看起来是空的（{} 字节）—— 上面那条相等此刻是空真：{seen}",
        seen.len()
    );
}

/// **那个 `[[bin]]` 真的托管着窗口进程的躯体。**
///
/// 🔴 它是源码型的，而且它补的缝很具体：`Cargo.toml` 里那一行 `path` 指到一个
/// **空的 `main`** 上，`cargo` 一样编得过、一样产出一个叫那个名字的二进制，
/// 而生产那条路起它之后**什么都不会发生**（窗口永远不出来）。
/// ⇒ 这一条把「那个名字」「那份文件」「那个函数」三者钉成一条链。
#[test]
fn the_window_binary_target_really_hosts_the_body() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let toml = std::fs::read_to_string(&manifest).expect("读不到本包的 Cargo.toml");
    // ① 那个 bin 的名字与本模块持有的主干**是同一个串**（抄第二份必漂）。
    assert!(
        guard_core::pin_line(&toml, &format!("name = \"{BIN_STEM}\"")).is_ok(),
        "`Cargo.toml` 里没有一个叫 `{BIN_STEM}` 的目标 —— \
         而生产那条路正是按这个名字去 exe 旁边找它的"
    );
    // ② 它的入口文件是那一份，而且那一份在盘上。
    let rel = "src/filewin/win_main.rs";
    assert!(
        guard_core::pin_line(&toml, &format!("path = \"{rel}\"")).is_ok(),
        "`Cargo.toml` 里那个 bin 的 `path` 不是 `{rel}` —— 入口搬家了，本条的第 ③ 比跟着失效"
    );
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let body = std::fs::read_to_string(&entry)
        .unwrap_or_else(|e| panic!("`{rel}` 读不动（{e}）—— `Cargo.toml` 指着一个不存在的入口"));
    // ③ 🔴 它真的调那个躯体。**这一比是本条的全部价值**：
    //    一个空 `main` 会让上面两比照样绿，而窗口永远不出来。
    assert!(
        guard_core::find_pinned(&body, "child_main()").is_ok(),
        "`{rel}` 里没有调 `child_main()` —— 那个二进制编得过、也产得出，\
         但起它之后什么都不会发生（窗口永远不出来）。\n它是这样的：{body}"
    );
}

/// **当场就死掉的那一形，回的是一句非空的原因** —— 不是 `Ok`。
#[test]
fn a_window_process_that_dies_at_once_comes_back_as_a_reason() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("filewin-dead-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("造不出临时目录");
    // 一个**存在但根本不是可执行文件**的落点 —— 起进程那一下就该失败。
    let fake = dir.join("不是一个二进制");
    // ⚠ 内容刻意是 ASCII：它只需要**不是**一个可执行文件的头。
    std::fs::write(&fake, b"not an ELF at all\n").expect("写不出来");
    std::env::set_var(BIN_ENV, &fake);
    let e = open_in_new_process(&OpenRequest {
        source: Source::remote(synthetic_cfg()),
        cwd: dir.to_string_lossy().to_string(),
        rows: Vec::new(),
        reveal: None,
        handoff: synthetic_handoff(),
    })
    .expect_err("拿一个不是二进制的文件当窗口进程，居然报了成功");
    println!("  现打：{e}");
    assert!(!e.is_empty(), "报了错但原因是空的 —— 上层连话都没有");
    std::env::remove_var(BIN_ENV);
    std::fs::remove_dir_all(&dir).ok();
}

/// 🔴 **零命中型：入口那条路上没有「同进程开一个」的写法。**
///
/// `设计/01 §5 D11`（用户 2026-09-22 裁决）逐字「**不要退路** /
/// 所有东西都不要假设后端没起来」。在这一格上它的样子是：
/// 起不了独立进程**不许**退回同进程开一个 —— 那条路已知第二趟必然失败
/// （winit 一个进程只许一个事件循环）。
/// ⚠〔反订正 · 2026-09-24 · X1〕上一版这里还有一句「而且关窗时可能把整个 app 一起带走」——
/// 那条读数来自台架的 `xdotool windowclose`（`XDestroyWindow`，产品里不存在），
/// 出处 `真相源/107 §2` 的〔反订正〕块与 `设计/60 §「Xvfb 抖动」`。
///
/// ⚠ 它是**源码代理**，不是行为判据：它买的是「那个写法不在盘上」。
/// 「起不来时真的报错」由 [`a_window_process_that_dies_at_once_comes_back_as_a_reason`] 买。
#[test]
fn the_entry_command_has_no_in_process_fallback_left() {
    let src =
        std::fs::read_to_string(crate::guard_support::crate_src_root().join("filewin/entry.rs"))
            .expect("entry.rs 读不动");
    let prod = guard_core::production_code(&src);
    // ★ 反向自检：剥完之后生产段**不是空的**，否则下面那条零命中恒真。
    assert!(
        guard_core::find_pinned(&prod, "pub async fn open_file_window").is_ok(),
        "剥生产段把入口那条命令一起剥掉了 —— 下面那条零命中此刻恒真"
    );
    // 运行时拼，免得命中本行自己。
    let needle = format!("open_{}", "detached");
    assert!(
        !prod.contains(needle.as_str()),
        "`entry.rs` 的生产段里出现了 `{needle}` —— 那是**同进程**开窗那条路。\n\
         `D11` 逐字不许留退路：起不了独立进程就是错，照实报。\n\
         那条路已知第二趟必然失败（winit 一个进程只许一个事件循环）。"
    );
    // 而独立进程那一条**恰好一处**，且它的结果没有被丢掉。
    assert_eq!(
        prod.matches("open_in_new_process(").count(),
        1,
        "`entry.rs` 生产段里起窗口进程不是恰好一处"
    );
    assert!(
        !prod.contains("let _ = open_in_new_process("),
        "起窗口进程的结果又被 `let _ = …` 丢掉了 —— 那就回到了「静默成功」那一形"
    );
}
