//! `super::proc` 那一摞判据 —— **进程形态**。
//!
//! # 这一摞逐条买什么
//!
//! | 判据 | 它钉的那一形 | 少了它会怎样 |
//! |---|---|---|
//! | [`a_seed_survives_the_trip_through_a_process_boundary`] | 源 ＋ cwd ＋ reveal ＋ 交接件 **整份**过得去（相等断言） | 种子漂一格 ⇒ 窗口开在别处，而没有一句话 |
//! | 🔴 [`the_window_process_lists_first_and_the_parent_carries_its_words`] | 〔09-28 裁 3〕就绪那一行：列到 N 行 ⇒ 回 N；列不出来 ⇒ 原话原样回、窗口不开；一句不说 ⇒ 进程层的错 | 「列不出来就不开窗、带原话」搬进窗口进程后没人钉 |
//! | [`the_first_screen_asks_home_only_when_told_nothing`] | 〔09-28 裁 3〕窗口进程自己问第一屏：没给目录才问 home、问不到带原话 | 给了目录也去问 home（多一趟、先失败盖掉真原因） |
//! | [`a_seed_that_cannot_be_read_is_a_loud_failure_not_a_default_window`] | 解不出来的种子**出声**，不补默认值（含阴性对照） | 「开了个空窗」与「成功」在屏幕上分不开 |
//! | [`the_window_binary_is_never_guessed`] | 那份二进制**只有两处**来路，都不在就出声并说出看过哪儿 | `D11`：静默回落 / `D7`：归因说成「窗口画不出来」 |
//! | 🔴 [`opening_a_window_three_times_really_starts_three_independent_processes`] | **一趟一个进程、pid 互不相同、三趟结局逐字相同** | 那正是旧形态的病：第二趟被一个**进程级**标志挡回来 |
//! | [`a_window_process_that_dies_at_once_comes_back_as_a_reason`] | 当场死掉的那一形回的是一句非空的原因 | 静默成功那一形又回来了 |
//! | [`the_entry_command_has_no_in_process_fallback_left`] | 零命中型：入口那条路上**没有**「同进程开一个」的写法 | `D11` 逐字：不许留退路 |
//!
//! # ⚠ 这一摞**买不到**什么（逐条）
//!
//! - **「窗口真的出现在屏幕上」一格都没有。** 本机 `XDG_SESSION_TYPE=tty`。
//!   这里量的是**进程**：起没起来、pid 是不是新的、退出码是什么。
//!   实景那一维住 `shell_tests` 那个 Xvfb 台架（它自己的头注写清了它买不到的四样）。
//! - **「三趟都成功」买不到**，只买到「**三趟都一样**」：没有图形会话时三趟都失败，
//!   而本摞钉的是「失败的**理由与形状**逐趟相同」⇒ 结构上没有任何进程级状态
//!   让第二趟与第一趟不同。**那正是旧形态缺的那一格**（旧形态下第二趟与第一趟
//!   必然不同），但它**不是**「第二趟开窗成功」的证据。后者只有实景那一格买得到。
//! - **后端那条通道一格都没量。** 窗口进程里它够不着（逐条理由住 `proc` 头注 §四），
//!   而「够不着之后窗口上那行橙字说得对不对」是 `source` / `find` 那两摞的活。

#[path = "../../filewin/theme_testing.rs"]
mod theme_testing;

use super::*;

/// 改环境变量那几条**必须串行**：`std::env` 是进程级的，而 `cargo test` 默认并行。
///
/// ⚠ 不加这道的症状是**偶发**假红（另一条判据正好在读同一个变量）——
/// 本仓对「偶发红会被人学会重跑绕过」记过一笔，所以这里直接串起来。
static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 一台合成远端的名字（种子里只剩名字：窗口不再拿整份 `RemoteConfig`）—— 带中文与连字符，同 `cwd` 那一格的理由。
fn synthetic_cfg() -> String {
    "台架-远端".to_string()
}

fn synthetic_request() -> OpenRequest {
    OpenRequest {
        origin: synthetic_cfg(),
        cwd: Some("/home/zbl/带空格 的目录".to_string()),
        // 〔09-28 裁 3〕「那一屏」（`rows`，含链接 · 时间 · 有损名原始字节几格）不再过这条边界：
        //   窗口进程自己列（`first_screen`）；那几格的解法由 `source_tests` 的 `row_from_ls_entry` 那几条判。
        reveal: Some("坏\u{FFFD}名字".to_string()),
        handoff: synthetic_handoff(),
        // 书签文件那一格也进种子对拍（带空格 ＋ 多字节，同 `cwd` 那一格的理由）。
        bookmarks: Some(std::path::PathBuf::from(
            "/tmp/书签 目录/filewin-bookmarks.json",
        )),
        // 机器名单也进种子对拍（带中文与空格）。
        machines: vec!["<local>".to_string(), "台架 远端".to_string()],
        // 工作区那一格也进种子对拍（负坐标：主屏左边那台副屏）。
        work_area: Some(host_core::WorkArea {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1040,
        }),
        theme: theme_testing::default_theme(),
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
    // ── `OpenRequest` 没有 `PartialEq` ⇒ 逐格比。
    //    🔴 **逐格比不是偷懒**：它让「新加一个字段而没进种子」当场红在下面那条
    //    字段数自检上，而一个 `PartialEq` 的 `assert_eq!` 在**字段被漏掉时恒真**
    //    （漏掉的那一格两侧都是默认值）。
    assert_eq!(got.cwd, want.cwd, "cwd 漂了 —— 窗口会开在别处");
    assert_eq!(got.reveal, want.reveal, "reveal 漂了 —— 高亮落在别的行上");
    assert!(
        want.cwd.is_some(),
        "夹具里 cwd 得是 `Some`，否则两侧都是 `None` 恒相等"
    );
    // 书签文件那一格：漂了 ⇒ 窗口读写的是另一份书签。
    assert_eq!(got.bookmarks, want.bookmarks, "书签文件的路径漂了");
    assert!(
        want.bookmarks.is_some(),
        "夹具里这一格得是 `Some`，否则两侧都是 `None` 恒相等"
    );
    // 机器名单（「复制到另一台」的下拉）：漂了 ⇒ 下拉里列的不是配置里那几台。
    assert_eq!(got.machines, want.machines, "机器名单漂了");
    assert!(
        !want.machines.is_empty(),
        "夹具里这一格得非空，否则两侧都是空恒相等"
    );
    // 工作区：漂了 ⇒ 窗口夹进的是别的一块。
    assert_eq!(got.work_area, want.work_area, "工作区漂了");
    assert!(
        want.work_area.is_some(),
        "夹具里这一格得是 `Some`，否则两侧都是 `None` 恒相等"
    );
    // 样子：漂了 ⇒ 窗口画的不是主界面那一套。
    assert_eq!(got.theme, want.theme, "样子漂了");
    // 🔴〔2026-09-23 本机侧退役〕**这里少了一次「判别式过得去吗」的比对。**
    //    从前 `Source` 是个两格枚举，这一段要先 `match` 出两侧都是 `Remote`
    //    （对不上就 `panic!("源的判别式没过得去")`），下面还单独喂一份
    //    `Source::Local` 的种子对拍它那一格。`Source` 收成 newtype 之后
    //    **判别式这个概念不存在了** ⇒ 那两处不是被删掉的判据，是它们判的东西没了。
    // 那台的名字（从前是整份远端配置：主机 · 口 · 用户 · 钥匙路径 · 地址表逐格比；窗口今天只拿名字）。
    assert_eq!(
        got.origin, want.origin,
        "那台的名字漂了 —— 窗口会问另一台机器（或者谁都没登记过的名字）"
    );
    // 🔴交接件整份过得去（地址 · 帧长 · 钥匙），否则窗口拨不回来。
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
    for needle in ["台架-远端", "带空格 的目录", "书签 目录"] {
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

/// **那份二进制只有三处来路，都给不出就出声并说出看过哪儿。**（注入形：不读真环境变量、不碰真 `~/.cc-monitor`。）
#[test]
fn the_window_binary_is_never_guessed() {
    let dir = std::env::temp_dir().join(format!("filewin-bin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("造不出临时目录");
    let landing = dir.join("home-bin");
    let mk = |_: &Path| -> Result<(), String> { Ok(()) };
    let ensure = crate::platform::fs::ensure_private_dir;
    // ① 纯函数那一格：落点的形状（Windows 上带 `.exe`）。
    let p = window_bin_in(&dir);
    assert_eq!(
        p,
        dir.join(format!("{BIN_STEM}{}", std::env::consts::EXE_SUFFIX)),
        "落点的拼法变了"
    );
    // ② 环境变量指着一个**不存在**的路径、旁边没有、这一份没带 ⇒ 出声，而且**把看过的都说出来**。
    let missing = dir.join("根本没有这个文件");
    let e = resolve_window_bin_in(
        Some(missing.clone()),
        &dir,
        None,
        Some(&landing),
        &mk,
        &ensure,
    )
    .expect_err("指着一个不存在的文件居然解出来了");
    assert!(
        e.contains("根本没有这个文件"),
        "报错里没有那条被看过的路径 —— `D7`：归因得说准是「二进制不在」而不是「窗口画不出来」：{e}"
    );
    assert!(e.contains(BIN_STEM), "报错里没点名要找的是哪个二进制：{e}");
    assert!(!landing.exists(), "没带那一份却往落点里建了东西");
    // ③ 阴性对照：指着一个**真存在**的文件 ⇒ 原样回它，不再往别处找。
    let real = dir.join("假装是那个二进制");
    std::fs::write(&real, b"#!/bin/true\n").expect("写不出那个文件");
    assert_eq!(
        resolve_window_bin_in(
            Some(real.clone()),
            &dir,
            Some(b"carried"),
            Some(&landing),
            &mk,
            &ensure
        )
        .expect("指着一个真文件却解不出来"),
        real,
        "环境变量指的那一份没被用上 —— 判据从此指不动它"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 🔴 **旁边没有、monitor 自带着 ⇒ 放到落点再回那一份**（单文件的 monitor 开得了文件窗口，靠的就是这一支）。
///
/// 期望手写自落盘规则（不取自被测函数）：落点 = `<落点目录>/<窗口程序名>`、字节 = 自带那份；
/// 同一份再开零写；盘上是别的字节 ⇒ 换成自带那份；原地换不掉（Windows 上旧的那份正在跑）⇒ 先挪开再上位；
/// 旁边有 ⇒ 用旁边的、不碰落点；家目录问不到 / 放不下来 ⇒ 各说各的那一句，不说成「没带」。
#[test]
fn with_nothing_beside_the_exe_the_carried_window_binary_is_placed_and_returned() {
    let root = std::env::temp_dir().join(format!("filewin-carried-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let exe_dir = root.join("exe");
    std::fs::create_dir_all(&exe_dir).expect("造不出 exe 目录");
    let landing = root.join("home").join(".cc-monitor").join("bin");
    let made = std::cell::Cell::new(0usize);
    let mk = |_: &Path| -> Result<(), String> {
        made.set(made.get() + 1);
        Ok(())
    };
    let ensure = crate::platform::fs::ensure_private_dir;
    let want = landing.join(format!("{BIN_STEM}{}", std::env::consts::EXE_SUFFIX));
    let resolve = |carried: &[u8]| {
        resolve_window_bin_in(None, &exe_dir, Some(carried), Some(&landing), &mk, &ensure)
    };
    // ① 第一次：放下来、置可执行位、回落点那一份。
    assert_eq!(resolve(b"carried-v1").expect("自带着却没放下来"), want);
    assert_eq!(std::fs::read(&want).expect("落点上没有文件"), b"carried-v1");
    assert_eq!(made.get(), 1, "放下来的那一份没置可执行位");
    // ② 同一份再开一次：零写。
    assert_eq!(resolve(b"carried-v1").expect("第二次没解出来"), want);
    assert_eq!(made.get(), 1, "逐字节相等还重写了一次");
    // ③ 盘上是别的版本（同长不同字节）⇒ 换成自带那份。
    std::fs::write(&want, b"other-ver!").expect("写不出旧版替身");
    assert_eq!(resolve(b"carried-v1").expect("换版没解出来"), want);
    assert_eq!(std::fs::read(&want).expect("落点上没有文件"), b"carried-v1");
    assert_eq!(made.get(), 2, "字节不同却没换");
    // ④ 原地换不掉（落点被一个非空目录占着，`rename` 必失败 —— Windows 上正在跑的那份同形）⇒ 挪开再上位，挪开的那份留着。
    std::fs::remove_file(&want).expect("删不掉上一份");
    std::fs::create_dir_all(want.join("占着")).expect("造不出占位目录");
    assert_eq!(resolve(b"carried-v2").expect("原地换不掉就没解出来"), want);
    assert_eq!(std::fs::read(&want).expect("挪开之后没上位"), b"carried-v2");
    let aside = landing.join(format!(
        ".{BIN_STEM}{}.{}.old",
        std::env::consts::EXE_SUFFIX,
        std::process::id()
    ));
    assert!(
        aside.join("占着").is_dir(),
        "占着的那一份不是被挪开的：{aside:?}"
    );
    // ⑤ 旁边有 ⇒ 用旁边那份，落点那一份一个字节不动。
    let beside = window_bin_in(&exe_dir);
    std::fs::write(&beside, b"beside").expect("写不出旁边那份");
    assert_eq!(resolve(b"carried-v3").expect("旁边有却没解出来"), beside);
    assert_eq!(std::fs::read(&want).expect("落点上没有文件"), b"carried-v2");
    std::fs::remove_file(&beside).expect("删不掉旁边那份");
    // ⑥ 家目录问不到 ⇒ 「不知道放哪」那一句；放不下来 ⇒ 带着原话的那一句 —— 都不是「没带」。
    assert_eq!(
        resolve_window_bin_in(None, &exe_dir, Some(b"x"), None, &mk, &ensure)
            .expect_err("没有落点也解出来了"),
        copy_text("rsFilewinProc.bin.noHome", &[])
    );
    let no_dir = |_: &Path| -> Result<(), String> { Err("不许建".to_string()) };
    let fresh = root.join("fresh");
    assert_eq!(
        resolve_window_bin_in(None, &exe_dir, Some(b"x"), Some(&fresh), &mk, &no_dir)
            .expect_err("建不了目录也解出来了"),
        copy_text(
            "rsFilewinProc.bin.placeFailed",
            &[("dir", &fresh.display().to_string()), ("e", "不许建")]
        )
    );
    std::fs::remove_dir_all(&root).ok();
}

/// 自带那份的名字**一处定、处处同**：`build.rs` 的 `NATIVE_BACKEND_DIR` ＋ `NATIVE_FILEWIN_FILE` == 本模块的 [`BIN_STEM`]
/// == `byte_table.rs` 的 `include_bytes!` 字面量 == `re-embed.sh --native` 铺的 == `release.yml` 两个 job 铺的。
/// 漂一处就是「嵌进去一个空 cfg」或「铺了没人吃」。异源：五份文件现读。
#[test]
fn the_carried_window_binary_is_spelled_the_same_in_every_place() {
    let build = include_str!("../../../../src/frontend/shell/build.rs");
    let konst = |name: &str| -> String {
        let at = guard_core::find_pinned(build, &format!("const {name}: &str = \""))
            .unwrap_or_else(|e| panic!("`build.rs` 里的 `{name}`：{e}"));
        let rest = &build[at..];
        let open = rest.find('"').expect("常量没有字面量") + 1;
        let close = rest[open..].find('"').expect("字面量没闭合");
        rest[open..open + close].to_string()
    };
    let dir = konst("NATIVE_BACKEND_DIR");
    let file = konst("NATIVE_FILEWIN_FILE");
    assert_eq!(
        file, BIN_STEM,
        "内嵌那份的名字与 exe 旁边找的那个名字不是同一个词"
    );
    let landing = format!("{dir}/{file}");
    let bytes_src = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/byte_table.rs"
    ));
    // 针在运行时拼（整串写死在本文件里，`cross_half_edge_registry` 会把它当成一处解析不出路径的内嵌）。
    let needle = format!("{}!(\"../{landing}\")", "include_bytes");
    assert!(
        guard_core::find_pinned(&bytes_src, &needle).is_ok(),
        "消费侧的内嵌字面量不是 `../{landing}`"
    );
    let reembed = include_str!("../../../../tests/scripts/re-embed.sh");
    for line in [
        format!("cp \"$ROOT/.build/shell/release/{file}$exe\" \"$NATIVE_DIR/{file}\""),
        format!("printf '%s\\n' \"$triple\" > \"$NATIVE_DIR/{file}.target\""),
    ] {
        assert!(
            guard_core::find_pinned(reembed, &line).is_ok(),
            "`re-embed.sh --native` 没有恰好一行 `{line}`"
        );
    }
    let yml = include_str!("../../../../.github/workflows/release.yml");
    for line in [
        format!("$dst = \"src/frontend/shell/{landing}\""),
        format!("dst=src/frontend/shell/{landing}"),
    ] {
        assert!(
            guard_core::find_pinned(yml, &line).is_ok(),
            "`release.yml` 没有恰好一处铺 `{landing}` 的 `{line}`（Windows 那一格 `$dst = …` · Linux 那一格 `dst=…`）"
        );
    }
}

/// 一个**一定在**的替身二进制：它读 stdin 到 EOF 然后退出。
///
/// 🔴 **为什么用替身，而不用那份真的 `cc-monitor-filewin`** —— 不是省事：
/// 门禁那一格逐字跑 `cargo test --workspace --lib`，
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
        origin: synthetic_cfg(),
        cwd: Some("/tmp".to_string()),
        reveal: None,
        handoff: synthetic_handoff(),
        bookmarks: None,
        machines: Vec::new(),
        work_area: None,
        theme: theme_testing::default_theme(),
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
/// 〔09-28 裁 3〕生产那条路今天**接了** stdout（就绪那一行从那里回来）⇒ 本条走的就是 [`spawn_window`] 整条：
/// 替身 `cat` 把 stdin 原样吐到 stdout，读回来就是它收到的那一份。上一版这里自己拼一根 `.stdin(piped())`（第二份写法）的缘由没了。
#[cfg(unix)]
#[test]
fn the_seed_really_arrives_on_the_child_process_stdin() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var(BIN_ENV, stdin_eating_stand_in());
    let req = synthetic_request();
    let seed = encode_request(&req).expect("序列化");
    let mut child = spawn_window(&req).expect("起不了替身进程 —— 台架坏了");
    std::env::remove_var(BIN_ENV);
    let mut seen = String::new();
    std::io::Read::read_to_string(
        &mut child
            .stdout
            .take()
            .expect("生产那条路没接 stdout —— 就绪那一行回不来"),
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
        seen.contains("台架-远端") && seen.len() > 100,
        "送到对面的种子看起来是空的（{} 字节）—— 上面那条相等此刻是空真：{seen}",
        seen.len()
    );
}

/// 一个替身窗口进程：读完种子、在 stdout 上说 `say`（原样一行）、再睡 `linger` 秒退出。
#[cfg(unix)]
fn scripted_stand_in(
    dir: &std::path::Path,
    tag: &str,
    say: &str,
    linger: u32,
) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(format!("stand-in-{tag}.sh"));
    let body = if say.is_empty() {
        format!("#!/bin/sh\ncat >/dev/null\nsleep {linger}\nexit 3\n")
    } else {
        format!("#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{say}'\nsleep {linger}\n")
    };
    std::fs::write(&p, body).expect("写不出替身脚本");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    p
}

/// 🔴**第一屏由窗口进程列，父进程只读它那一行。**
///
/// ① 它说「列到 7 行」、然后还活着（窗口开着）⇒ 回 `(pid, 7)`；
/// ② 它说「列不出来：原话」⇒ **原话原样**回（[`Unopened::Said`]），而且那是它不开窗就退的那一形；
/// ③ 它一句不说就退 ⇒ 进程层的错（[`Unopened::Process`]），不是「列到 0 行」；
/// ④ 它说了一句不是约定形状的话 ⇒ 进程层的错（两端契约漂了）。
/// ⚠ 替身是 shell 脚本，不是那份真窗口进程：「真窗口进程在列不出来时真的不开窗」由 `child_main` 的行序代理
///   （[`the_window_dials_back_with_the_handoff_and_refuses_to_open_without_it`] ④⑤）钉。
#[cfg(unix)]
#[test]
fn the_window_process_lists_first_and_the_parent_carries_its_words() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("filewin-ready-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("造不出临时目录");
    let req = OpenRequest {
        origin: synthetic_cfg(),
        cwd: None,
        reveal: None,
        handoff: synthetic_handoff(),
        bookmarks: None,
        machines: Vec::new(),
        work_area: None,
        theme: theme_testing::default_theme(),
    };
    let run = |tag: &str, say: &str, linger: u32| {
        std::env::set_var(BIN_ENV, scripted_stand_in(&dir, tag, say, linger));
        let r = open_in_new_process(&req, Box::new(|_| {}));
        std::env::remove_var(BIN_ENV);
        r
    };
    // ①
    let (_pid, n) = run(
        "listed",
        &encode_ready(&Ready::Listed(7)).trim().to_string(),
        2,
    )
    .expect("说了列到 7 行、人还活着，却回了错");
    assert_eq!(n, 7, "行数不是它说的那个");
    // ②
    let said = "列不出来：那台说没有这个目录 /srv/不在";
    assert_eq!(
        run(
            "failed",
            encode_ready(&Ready::Failed(said.into())).trim(),
            0
        ),
        Err(Unopened::Said(said.into())),
        "列不出来的原话没原样带回来"
    );
    // ③
    match run("silent", "", 0) {
        Err(Unopened::Process(e)) => assert!(!e.is_empty()),
        other => panic!("一句不说就退，却回了：{other:?}"),
    }
    // ④
    match run("garbled", "{\"ok\":1}", 0) {
        Err(Unopened::Process(e)) => assert!(!e.is_empty()),
        other => panic!("说了一句不是约定形状的话，却回了：{other:?}"),
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// 要求：「窗口出现后 166 ms 进程退出，monitor 判成功、用户零提示（早失败检测是竞速）」·
/// 要求「判成功之后很快退出也要报」。替身说「列到 3 行」、活过开窗预算（300 ms）再退：
/// ① 退出码非零 ⇒ 开窗照回成功，**之后**收尸线程交来那一句（带退出码）；② 退出码 0（用户关窗那一形）⇒ 一句都不交。
#[cfg(unix)]
#[test]
fn a_window_that_dies_after_being_judged_open_is_still_reported() {
    use std::os::unix::fs::PermissionsExt;
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("filewin-late-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("造不出临时目录");
    let req = OpenRequest {
        origin: synthetic_cfg(),
        cwd: None,
        reveal: None,
        handoff: synthetic_handoff(),
        bookmarks: None,
        machines: Vec::new(),
        work_area: None,
        theme: theme_testing::default_theme(),
    };
    let listed = encode_ready(&Ready::Listed(3)).trim().to_string();
    let run = |code: u32| {
        let p = dir.join(format!("late-{code}.sh"));
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' '{listed}'\nsleep 0.6\nexit {code}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var(BIN_ENV, &p);
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let r = open_in_new_process(&req, Box::new(move |said| tx.send(said).unwrap()));
        std::env::remove_var(BIN_ENV);
        (r, rx.recv_timeout(std::time::Duration::from_secs(10)))
    };
    let (r, said) = run(7);
    assert_eq!(r.map(|(_, n)| n), Ok(3), "活过预算的那一形该先回成功");
    let said = said.expect("判成功之后退出码 7 退了，却一句都没交");
    let why = copy_text("rsFilewinProc.late.exited", &[("st", "exit status: 7")]);
    assert_eq!(
        said,
        copy_text("rsFilewinProc.open.seeStderr", &[("why", &why)])
    );
    let (r, said) = run(0);
    assert!(r.is_ok());
    assert!(said.is_err(), "体面退出（用户关窗）也报了：{said:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// 就绪那一行的线上形：两种来回过得去；别的形状一律拒（不猜）。**纯函数**。
#[test]
fn the_ready_line_has_exactly_two_shapes() {
    for r in [
        Ready::Listed(0),
        Ready::Listed(50_000),
        Ready::Failed("原话 \n 带换行".into()),
    ] {
        let line = encode_ready(&r);
        assert!(
            line.ends_with('\n') && line.matches('\n').count() == 1,
            "不是恰好一行：{line:?}"
        );
        assert_eq!(decode_ready(&line), Ok(r));
    }
    for bad in [
        "",
        "{}",
        "{\"listed\":-1}",
        "{\"failed\":1}",
        "{\"listed\":1,\"failed\":\"x\"}",
        "listed 3",
    ] {
        assert!(decode_ready(bad).is_err(), "{bad:?} 被认成了就绪");
    }
    let mut eof = std::io::Cursor::new(Vec::<u8>::new());
    assert_eq!(read_ready(&mut eof), Ok(None), "EOF 不是「一句没说」");
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
    // 躯体住独立包 `cc_monitor_filewin`：链多一跳 —— 入口调包的 `run()`，`run()` 调 `proc::child_main()`。
    assert!(
        guard_core::find_pinned(&body, "cc_monitor_filewin::run()").is_ok(),
        "`{rel}` 里没有调 `cc_monitor_filewin::run()` —— 那个二进制编得过、也产得出，\
         但起它之后什么都不会发生（窗口永远不出来）。\n它是这样的：{body}"
    );
    let lib =
        guard_core::production_code(include_str!("../../../../src/frontend/filewin/src/lib.rs"));
    assert!(
        guard_core::find_pinned(&lib, "pub fn run() -> i32 {").is_ok()
            && guard_core::find_pinned(&lib, "proc::child_main()").is_ok(),
        "窗口包的 `run()` 不再转调 `proc::child_main()` —— 入口那一跳接上了，躯体那一跳断了"
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
    let e = open_in_new_process(
        &OpenRequest {
            origin: synthetic_cfg(),
            cwd: Some(dir.to_string_lossy().to_string()),
            reveal: None,
            handoff: synthetic_handoff(),
            bookmarks: None,
            machines: Vec::new(),
            work_area: None,
            theme: theme_testing::default_theme(),
        },
        Box::new(|_| {}),
    )
    .expect_err("拿一个不是二进制的文件当窗口进程，居然报了成功");
    println!("  现打：{e:?}");
    let Unopened::Process(e) = e else {
        panic!("起不来那一形被说成了窗口进程的话：{e:?}")
    };
    assert!(!e.is_empty(), "报了错但原因是空的 —— 上层连话都没有");
    std::env::remove_var(BIN_ENV);
    std::fs::remove_dir_all(&dir).ok();
}

/// 🔴 **零命中型：入口那条路上没有「同进程开一个」的写法。**
///
/// 用户原话「**不要退路** /
/// 所有东西都不要假设后端没起来」。在这一格上它的样子是：
/// 起不了独立进程**不许**退回同进程开一个 —— 那条路已知第二趟必然失败
/// （winit 一个进程只许一个事件循环）。
/// ⚠〔反订正〕上一版这里还有一句「而且关窗时可能把整个 app 一起带走」——
/// 那条读数来自台架的 `xdotool windowclose`（`XDestroyWindow`，产品里不存在），
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

// 窗口进程拨回 monitor 那一下 · 第一屏那两条（窗口那一侧的躯体）随它搬去了 `tests/frontend/filewin/proc_tests.rs`。

// ════════════════════════════════════════════════════════════════════════
// 🔴「当场就死了」不再被报成成功（原住 `shell_tests`：`early_failure` 随起进程那一侧留在 monitor）
//    下面两条**照旧喂线程**：它们钉的是那条轮询本身的两个方向（「结束了」认得出 · 「还在跑」不误判），
//    线程是这两个方向最便宜的合成输入；进程那一侧的行为判据是上面那几条（真起进程）。
// ════════════════════════════════════════════════════════════════════════

/// 当场就失败的那条线程，`early_failure` 认得出来。
#[test]
fn a_thread_that_dies_at_once_is_recognised_as_a_failure() {
    let h: std::thread::JoinHandle<Result<(), String>> =
        std::thread::spawn(|| Err("开窗失败: 事件循环不能重建".into()));
    assert!(
        early_failure(|| h.is_finished(), std::time::Duration::from_millis(500)),
        "一条立刻就回 Err 的线程没被认出来 —— 那一形会被报成「窗口起来了」"
    );
    // 原因拿得回来（上层要把它交给用户）。
    match h.join() {
        Ok(Err(e)) => assert!(e.contains("事件循环"), "{e}"),
        other => panic!("{other:?}"),
    }
}

/// 🔴 **阴性对照**：还在跑的那条线程**不许**被当成失败。
///
/// 少了它，一个「恒回 true」的实现照样绿 —— 而那时**每一次**开窗都会被报成失败，
/// 连真起来的那次也是。
#[test]
fn a_thread_still_running_is_not_mistaken_for_a_failure() {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let h: std::thread::JoinHandle<Result<(), String>> = std::thread::spawn(move || {
        // 一直占着这条线程，直到判据放它走 —— 这就是「窗口起来了」那一形的形状
        //（`run_native` 占着线程直到窗口关闭）。
        let _ = rx.recv();
        Ok(())
    });
    assert!(
        !early_failure(|| h.is_finished(), std::time::Duration::from_millis(120)),
        "还占着线程的那一条被当成了失败 —— 那会让每一次真开窗都报错"
    );
    let _ = tx.send(());
    let _ = h.join();
}

/// 开窗那条路**真的**经这一跳走，而且排在起进程之后。
///
/// ⚠ 判源码是代理（同族先例住 `entry_tests` 那条「空路径那一支」）。
/// 买的是：起进程的结果不再被 `let _ = …` 丢掉，而且那一跳有东西可看时才跑。
///
/// 🔴**射程从 `entry.rs` 换到了 `proc.rs`。**
/// 上一版这一条扫的是 `entry.rs`，因为那时「起线程 ＋ 看它死没死」两步都写在入口里。
/// 今天入口只剩一句 `open_in_new_process(…)?`，那两步整块搬进了 `filewin::proc`
/// ⇒ 继续扫 `entry.rs` 的话，这一条会在一个**恒为零**的人群上报绿。
/// ⚠ 入口那一侧**没有失去判据**：`proc_tests` 里那条零命中型盯着
/// 「入口那条路上不许再有『同进程开一个』的写法」（`D11`）。
#[test]
fn the_spawn_result_is_never_thrown_away() {
    let prod = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/filewin/proc.rs"
    ));
    // ★ 反向自检：剥完不是空的，否则下面几比全在空人群上。
    assert!(
        prod.contains("pub fn open_in_new_process"),
        "剥生产段把那条路一起剥掉了 —— 下面几比此刻不可信"
    );
    // 定义随「起进程那一侧」搬进本文件（从前住窗口的 `shell.rs`）⇒ 数调用处：总数减掉定义那一行。
    assert_eq!(
        (
            prod.matches("fn early_failure(").count(),
            prod.matches("early_failure(").count() - prod.matches("fn early_failure(").count()
        ),
        (1, 1),
        "`proc.rs` 生产段里 `early_failure` 不是「定义一处 ＋ 调用一处」—— \
         这一族轮询全仓只许一处（`rust_timer_registry` 登记的就是它）"
    );
    let spawn_needle = format!("{}_window(", "spawn");
    assert!(
        !prod.contains(&format!("let _ = {spawn_needle}")),
        "起进程的结果又被 `let _ = …` 丢掉了 —— 那就回到了「静默成功」那一形"
    );
    // 🔴 **先把那个函数项切出来，再比先后** —— 而这一刀是死值验逼出来的，不是洁癖。
    //
    // 上一版在**整份生产段**上比 `find(spawn_window()` 与 `find(early_failure()`。
    // 死值验现打：把那一跳原地挪到起进程**之前**，这一条**照旧报绿**。
    // 病根是 `spawn_window` 的**定义**（`pub fn spawn_window(`）就在文件里更靠前的位置
    // ⇒ `at_spawn` 拿到的是定义的偏移，恒小于任何一处调用 ⇒ **那一比恒真**。
    // ⇒ 人群必须收到「`open_in_new_process` 这一个函数项」里面。
    let at_fn = prod
        .find("pub fn open_in_new_process")
        .expect("`open_in_new_process` 不在生产段里 —— 抽取器坏了");
    let rest = &prod[at_fn..];
    let body_end = rest
        .find("\n}\n")
        .expect("`open_in_new_process` 的花括号没收口 —— 抽取器看不懂它了");
    let body = &rest[..body_end];
    // ★ 反向自检：切出来的那一段里**两者都在**。任一缺席 ⇒ 下面那一比是空转的。
    let at_spawn = body
        .find(spawn_needle.as_str())
        .expect("切出来的那个函数项里没有起进程那一句 —— 切法坏了，下面那一比此刻恒真");
    let at_check = body
        .find("early_failure(")
        .expect("切出来的那个函数项里没有那一跳 —— 切法坏了");
    assert!(
        at_spawn < at_check,
        "那一跳排在起进程**之前** —— 那时还没有进程可看"
    );
}
