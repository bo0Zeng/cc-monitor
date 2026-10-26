//! `K-P6b` `D2` 判据的**甲半**（界面这一侧）＋ 它的反向自检 —— 改成「拨号全搬走了」的形状。
//!
//! # 它断的是哪一个性质
//!
//! **界面进程里还有几处自己拨 SSH，以及拿链路的原语由谁拨。**
//!
//! 🔴 **它断的不是「`russh` 这个词还在不在」** —— `K-P6` 那次已经证过后者会量错集合。
//! ⇒ 判据落在**拿链路那个原语的函数体**上（`connect_and_exec_cmd` 只经拨号代理的宿主拿链路，零进程内回落），
//! 整份生产段里零处 `connect_session(`。
//!
//! C2 之后：入口走 `dial_host::open_stream`，**回落删了**（`D11`：「后端是给定的，不要退路」），
//! `FALLBACKS` 那张表整张删了（它的两行 —— 拿不到代理二进制 · 没配 `keyPath` —— 前者改成「报」，
//! 后者的根因在代理那侧补上了 ssh-agent）。进程内拨号只剩 SFTP 那一份（`inproc_dial.rs`）。
//!
//! # 乙半在哪
//!
//! 乙半（代理这一侧：拨号只许住 `dial/`）住 `tests/backend/dial_tests.rs`。**两侧各扫各的 crate**。

use guard_core::production_code;

/// 入口函数体的地板：切不出函数体（或切出个空壳）时判据必须**自己先红**。
/// **200 → 120**：判据的靶换成了拿链路的原语 `connect_and_exec_cmd`，它今天只转一句
/// （现打 178 字节）；120 仍比「切出来的是个空壳」（签名 ＋ 一对括号，约 90 字节）大。
const BODY_FLOOR_BYTES: usize = 120;

/// 语料：`stream_source/` 的**生产段**（剥掉 `#[cfg(test)]` 段）。
fn corpus() -> String {
    guard_core::addr::stream_source_production()
}

/// 按**行**取一个函数体：从 `head` 那一行起，到第一行**恰好是 `}`** 为止。
///
/// 与 `local_backend_host_tests.rs::body_of` 同形 —— 本仓已经在用这一把尺子，不另发明一把。
fn body_of(code: &str, head: &str) -> String {
    let Some(at) = code.find(head) else {
        return String::new();
    };
    let mut out = String::new();
    for line in code[at..].lines() {
        out.push_str(line);
        out.push('\n');
        if line == "}" {
            break;
        }
    }
    out
}

/// 🔴 **甲半判据本体。纯函数** —— 语料由调用方给 ⇒ 阳性/阴性两个方向都切得动。
///
/// 传进来的是拿链路那个原语的函数体。返回 `Err(说法)` = 判据红。
fn backend_stream_dial_verdict(body: &str) -> Result<(), String> {
    if body.len() < BODY_FLOOR_BYTES {
        return Err(format!(
            "切出来的入口函数体只有 {} 字节（地板 {BODY_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「函数体里一处都没有」与「压根没切出函数体」在终端上一模一样，\
                 这条地板就是把它们分开的那一刀。",
            body.len()
        ));
    }
    let count = |p: &str| body.matches(p).count();
    let n_host = count("dial_host::open_stream(");
    let n_direct = count("connect_session(");
    let n_channel = count("channel_open_session(");
    if n_direct + n_channel != 0 {
        return Err(format!(
            "拿链路的原语里直接出现了 {n_direct} 处 `connect_session(` / {n_channel} 处 `channel_open_session(` —— \
                 那是**界面进程自己拨号**，这里要断的正是这一格。"
        ));
    }
    if n_host != 1 {
        return Err(format!(
            "拿链路的原语里 `dial_host::open_stream(` 有 {n_host} 处（应当恰好 1 处）。\n\
                 0 处 ⇒ **拨号那一跳不经拨号代理了**（`K-P6b` 之前 / 进程内回落那一形）；\
                 ≥2 处 ⇒ 有第二条拿链路的路。"
        ));
    }
    Ok(())
}

/// ★ 甲半：**拿链路的原语只经拨号代理的宿主；零回落。**
///
/// 判据的靶是 `connect_and_exec_cmd` 的函数体：十来处一次性查询共用的那个原语，钉住它就钉住了全部。
/// 收全 exec 那个原语另钉一条（走 `dial_host::capture(`）。
/// 远端后端长连接流的入口今天是 `remote_resident::attach`（capture ＋ `dial_host::tunnel`），不再经本文件起流。
#[test]
fn the_backend_stream_entry_hands_the_dial_to_another_process() {
    let prod = corpus();
    // 流那一个原语（`connect_and_exec_cmd`）随最后一个调用方（公钥推送）进本机后端删了 ⇒
    //   本条的靶只剩收全那一个；「界面进程零拨号」照旧钉：整份生产段里零处自己拨、零处开流用法的链路。
    assert_eq!(
        (
            prod.matches("pub async fn connect_and_exec_cmd(").count(),
            prod.matches("dial_host::open_stream(").count(),
            prod.matches("connect_session(").count(),
        ),
        (0, 0, 0),
        "流那一个原语 / 开流链路 / 进程内拨号回来了"
    );
    let capture = body_of(&prod, "pub async fn connect_and_exec_capture(");
    assert_eq!(
        (
            capture.matches("dial_host::capture(").count(),
            capture.matches("connect_session(").count()
        ),
        (1, 0),
        "收全 exec 的原语没有恰好一次经拨号代理（或者自己拨了）"
    );
}

/// 🔴 **搬家之前**的远端起流入口函数体，**逐字冻结**（那个入口随远端流模式一形删了，冻结的是历史文本）。
///
/// 出处：`git show f10581c:src/frontend/shell/src/ssh_source.rs` 的 `:1304-1321`
/// （分支尖 `f10581c` = 搬家第二轮的最后一个提交，那时生产段一个字节都还没动）。
///
/// ⚠ **为什么不在测试里跑 `git show`**：那会让判据依赖「测试跑在一棵有 `.git` 的树里」，
/// 而门禁那个沙箱里跑测试的条件不该多这一条。**冻结的历史文本不会腐烂** ——
/// 它是已经被删掉的代码，没有第二个版本去和它漂。
///
/// 🔴 **为什么逐行存而不是一个 `r#"…"#`**：那份原文里有一行**列 0 的 `}`**，
/// 而本仓共用的剥法（`guard_core`）正是按「列 0 的右大括号」找测试模块的结尾
/// —— 一个原始字符串里塞进这么一行，**整棵树的剥法当场坏掉**
/// （实打：`structural_scan` / `cross_half_edge_registry` / `write_half_guard` 三族
/// 一起红，报的都是 `guard_core` 里那条剥法的断言）。
/// `every_backend_file_strips_clean` 的头注逐字预告过这一形，
/// **这一轮把它撞出来了**。逐行存 ⇒ 那个 `}` 永远带着缩进，剥法看不见它。
const BODY_BEFORE_THIS_ITEM_LINES: &[&str] = &[
    "pub async fn connect_and_exec(",
    "    cfg: &RemoteConfig,",
    "    with_bg: bool,",
    "    tail_only: bool,",
    ") -> Result<russh::ChannelStream<client::Msg>, String> {",
    "    // 与 jsonl-watcher 不同，backend 是长连接：inactivity_timeout=None → connect_session",
    "    // 自动启用 30s keepalive（见 FIX 1 注释），靠 keepalive + EOF 检死链，不靠定时拆链。",
    "    // Batch7-F24/Batch8-F26：两个流模式 flag 都由调用方决定（run_stream 里绑定",
    "    // 部署确认为当前版本，见该处注释）。tail_only=true → backend 不重放历史",
    "    // （历史由本侧旁路 --read-session 快照拉取），实时通道流量趋零。",
    "    let mut cmd = shell_quote(&cfg.backend_path);",
    "    if with_bg {",
    "        cmd.push_str(\" --with-bg\");",
    "    }",
    "    if tail_only {",
    "        cmd.push_str(\" --tail-only\");",
    "    }",
    "    connect_and_exec_cmd(cfg, &cmd).await",
    "}",
];

/// 上面那份逐行快照拼回一段文本。
fn body_before_this_item() -> String {
    let mut s = BODY_BEFORE_THIS_ITEM_LINES.join("\n");
    s.push('\n');
    s
}

/// **C2 之前**的 `connect_and_exec_cmd` 函数体，逐字冻结（出处：`git show bf3cdc83:src/frontend/shell/src/ssh_source.rs`）。
/// 逐行存的理由与上面那份一样（列 0 的 `}` 会弄坏共用剥法）。
const PRIM_BEFORE_C2_LINES: &[&str] = &[
    "pub async fn connect_and_exec_cmd(",
    "    cfg: &RemoteConfig,",
    "    cmd: &str,",
    ") -> Result<russh::ChannelStream<client::Msg>, String> {",
    "    // 长连接/exec 路径不 emit 分阶段事件（F46 仅测试连接路径,避免每次重连刷屏）。",
    "    let (session, _fp) = connect_session(cfg, None, None).await?;",
    "",
    "    let channel = session",
    "        .channel_open_session()",
    "        .await",
    "        .map_err(|e| format!(\"打开 session channel 失败: {e}\"))?;",
    "",
    "    // want_reply = true：等远端确认 exec 成功再继续。",
    "    channel",
    "        .exec(true, cmd.as_bytes())",
    "        .await",
    "        .map_err(|e| format!(\"exec {cmd} 失败: {e}\"))?;",
    "",
    "    // into_stream 把 channel 变成 AsyncRead+AsyncWrite；读端就是 backend stdout 流。",
    "    Ok(channel.into_stream())",
    "}",
];

/// ★ 反向自检 · **阳性方向**：把改动之前那份函数体喂给判据，它必须**红并点名**。
///
/// 这一条买的是「判据真的会咬人」。少了它，上面那条绿只证明了
/// 「今天的代码没让它红」，证不出「它红得起来」。
#[test]
fn the_shape_before_this_item_is_caught_and_named() {
    // 先确认反例语料**真的是**那个旧形状（免得哪天有人把它改成新形状而没人发现）。
    let before = body_before_this_item();
    assert!(
        before.contains("connect_and_exec_cmd(cfg, &cmd).await"),
        "冻结的反例语料已经不是旧形状了 —— 它是历史文本，不该被改"
    );
    assert!(
        !before.contains("dial_host::open_stream("),
        "冻结的反例语料里居然有宿主调用 —— 那它就不是「改动之前」了"
    );
    let e = backend_stream_dial_verdict(&before).expect_err("旧形状（界面进程自己拨号）居然判绿了");
    assert!(
        e.contains("dial_host::open_stream(") && e.contains("0 处"),
        "判据红了，但**没点名是哪一处** —— 只说「有问题」的诊断等于没有诊断。实得：{e}"
    );
    // 第二份冻结反例：C2 之前的那个**原语**本体（进程内 `connect_session` ＋ 开通道 ＋ exec）。
    let prim_before = PRIM_BEFORE_C2_LINES.join("\n") + "\n";
    let e = backend_stream_dial_verdict(&prim_before)
        .expect_err("C2 之前的原语（进程内拨号）居然判绿了");
    assert!(
        e.contains("connect_session("),
        "判据红了，但没点名是进程内拨号那一处。实得：{e}"
    );
}

/// ★ 反向自检 · **阴性方向**（`P6bM4`）：喂空输入，判据必须**自己先红**。
#[test]
fn an_empty_body_makes_the_judge_red_by_itself() {
    let e = backend_stream_dial_verdict("").expect_err("空函数体居然判绿 —— 地板断言没接上");
    assert!(
        e.contains("空转"),
        "空输入红了，但红的理由不是「空转」—— 说明它被别的分支拦下了，地板没生效。实得：{e}"
    );
    // 再补一刀：非空但远小于地板，同样要以「空转」红。
    let e2 = backend_stream_dial_verdict("fn f() {}\n").expect_err("小输入居然判绿");
    assert!(e2.contains("空转"), "小输入红的理由不对：{e2}");
}

/// ★ `P6bM1` 的**粗细追问**：只改一个注释字，判据**不许**红。
///
/// 照红 ⇒ 刀太粗（它其实在钉「这段文本一个字都不许动」，而不是钉那个性质）。
#[test]
fn a_comment_only_edit_does_not_move_the_verdict() {
    // 真身删了（流那一个原语随公钥推送进本机后端）⇒ 喂它删之前那一版的形状（合成，逐字抄当时的函数体）。
    let body = "pub async fn connect_and_exec_cmd(\n    cfg: &RemoteConfig,\n    cmd: &str,\n) -> Result<crate::dial_host::DialStream, String> {\n    \
                // 〔C2 → SR1a〕拨号在本机常驻后端里；这里拿到的是它开的一条链路（读端 = 远端命令的 stdout）。\n\
                \x20   crate::dial_host::open_stream(cfg, cmd).await\n}\n"
        .to_string();
    backend_stream_dial_verdict(&body).expect("删之前那一版就该是绿的");
    let edited = body.replace(
        "    crate::dial_host::open_stream(cfg, cmd).await",
        "    // 这一行是本判据现加的注释，只为证明它不按文本相等判\n\
             \x20   crate::dial_host::open_stream(cfg, cmd).await",
    );
    assert_ne!(edited, body, "注释没插进去 —— 本条在空转");
    backend_stream_dial_verdict(&edited)
        .expect("只加了一行注释，判据就红了 ⇒ 刀太粗，它钉的是文本不是性质");
}

// 请求的造法与解析住宿主 `dial_host`：请求键由 `dial_host_tests::the_request_hands_over_only_the_name` 与后端源码异源对拍；
// 找不到代理要报而不是回落，解析含「本机后端自释放那一份」（读 `~/.cc-monitor/bin`，登记在 `local_read_surface_registry`）。

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 失效形状是现打过的：便宜近似 `src.split("\n#[cfg(test)]").next()` 只在
// 「第一个测试模块之后再没有生产代码」时才对。`stream_source/` 今天 4490 行，
// 第一个测试模块在 **909** 行 ⇒ 那个近似把扫描面砍到前 908 行，
// 而本文件要扫的东西全在它**后面**（逐针行号写在下面）。
// ⇒ 扫描面一旦静默缩水，本文件的判据会**零命中地绿**。
//
// 原语与它买不到什么：`guard_core::assert_stripper_keeps` 的头注。
// 一句话：它不买「针还是那个针」—— 下面这张表必须从本文件真正用的针里抄。

/// ★ 扫描面自检：共享剥法留住了本文件要扫的那几段，而便宜近似留不住。
#[test]
fn the_shared_stripper_keeps_the_entry_body_this_guard_must_scan() {
    // ⚠ **不能拿 `fn connect_session(` 当锚点**：它今天在 **695** 行 —— 在第一个测试模块
    //    （909 行）**之前** ⇒ 便宜近似也留得住它 ⇒ 那样这条对照会被填成恒真的
    //    （`assert_stripper_keeps` 会为此当场红，而不是静默放过）。
    //    本条真正会缩水的那一段是原语的函数体（在第一个测试模块之后）。
    guard_core::assert_stripper_keeps(
        "stream_source::dial_move_judge · stream_source/",
        &guard_core::addr::stream_source_raw(),
        // 锚点换成收全那一个原语（流那一个删了）；它同样在第一个测试模块之后。
        &["pub async fn connect_and_exec_capture("],
    );
    // ⚠ 本判据的语料是**四份**文件，这里只立了 `stream_source/` 那一份的对照 ——
    //    另两份（`sftp.rs` / `port_forward.rs`；`inproc_dial.rs` 那份整份删了）的针在它们各自第一个测试模块**之前**，
    //    便宜近似留得住 ⇒ 立对照会恒真。**那不是「已守住」，是「这一形在那两份上不成立」。**
}

/// →**界面 crate 里点名 `russh` / `russh_sftp` 的文件：零**（带正控）。
///
/// C2 那一版钉的是「== {SFTP 那一家}」（`inproc_dial.rs` · `sftp.rs`），逐字写着「SFTP 换走了 ⇒ 把 `inproc_dial.rs` 整份删掉、
/// `Cargo.toml` 的 `russh` 一起删，本条改成零命中」—— 今天就是那样（SFTP 进本机常驻后端，界面进程零 SSH）。
/// ⚠ 数的是**代码里点名**（`russh::` / `use russh` / `russh_sftp`）；买不到：经宏或别名间接用到它。
/// 清单那一面（manifest 里没有这几条依赖）同条钉；「依赖树里有没有它」那面旗归 `dial_home_registry::russh_deps_in`。
#[test]
fn russh_is_named_nowhere_in_the_monitor_crate() {
    // 运行时拼：写成字面量的话本文件自己会被扫进去（虽然本文件整份是测试段、剥法会剥掉，仍按先例从严）。
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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut users: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut scanned = 0usize;
    for (path, raw) in guard_core::scan_tree!(&root, &["rs"]) {
        scanned += 1;
        if hit(&production_code(&raw)) {
            users.insert(
                path.strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(
        scanned >= 100,
        "只扫到 {scanned} 份 `.rs` —— 遍历坏了，本条在空转"
    );
    assert!(
        users.is_empty(),
        "界面 crate 里又点名了 `russh` / `russh_sftp`：{users:?}\n\
         ⇒ 有人在界面进程里又拨起 SSH / 开起 SFTP 来了 —— SSH 与 SFTP 都归本机常驻后端（`src/backend/dial/`），\
         界面经 `dial_host`（链路）与 `deploy-*` · `transfer-*` 够到它们。"
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
