//! `K-P6b` `D2` 判据的**甲半**（界面这一侧）＋ 它的反向自检 —— 〔C2 · `设计/05 §13`〕改成「拨号全搬走了」的形状。
//!
//! # 它断的是哪一个性质
//!
//! **界面进程里还有几处自己拨 SSH，以及拿链路的原语由谁拨。**
//!
//! 🔴 **它断的不是「`russh` 这个词还在不在」** —— `K-P6` 那一拍已经证过后者会量错集合。
//! ⇒ 判据落在两处：**拿链路那个原语的函数体**（`connect_and_exec_cmd` 只经拨号代理的宿主拿链路，零进程内回落）
//! ＋ **每一份文件里 `connect_session(` 的调用点**逐份登记（[`DIAL_SITES`]）。
//!
//! 〔墓碑 —— `K-P6b` 那一版的要点逐字：「它的 7 处生产调用点里本件只覆盖 1 处，而覆盖的方式是
//!  **让入口不再走到它**」「回落那一条有没有被登记出来」（`FALLBACKS` 那张表）。〕
//! C2 之后：入口走 `dial_host::open_stream`，**回落删了**（`D11`：「后端是给定的，不要退路」），
//! `FALLBACKS` 那张表整张删了（它的两行 —— 拿不到代理二进制 · 没配 `keyPath` —— 前者改成「报」，
//! 后者的根因在代理那侧补上了 ssh-agent）。进程内拨号只剩 SFTP 那一份（`inproc_dial.rs`）。
//!
//! # 乙半在哪
//!
//! 乙半（代理这一侧：拨号只许住 `dial/`）住 `tests/backend/dial_tests.rs`。**两侧各扫各的 crate**。

use guard_core::production_code;

/// 界面进程里**每一处**往外拨号的入口，逐处登记。
///
/// `(文件, 生产段里的调用点处数, 这一处的拨号搬走了没有, 它是什么, 解锁条件)`
///
/// 🔴 **`pub(crate)` 是承重的，别顺手收回去**〔`K-R74` 09-12〕：
/// `dial_home_registry` 那条递减棘轮拿本表 `moved == false` 的**处数合计**当今天的读数，
/// 再对着 `ssh_source.rs` 的 git 历史比「历史上出现过的最低档」。
/// 收回成私有 ⇒ 那条棘轮编不过；改成在那边抄一份数字 ⇒ 同一个值两个家，本区最贵的那条病。
///
/// 🔴 **这张表就是「7 处里搬走 1 处」那句话的机器形态。** 第三栏 `false` 的每一行
/// 都是一句「本件**没有**买到这里」——散文里那句话可以腐烂，这张表不行：
/// 处数由下面的判据**从源码派生**再逐格比对，多一处少一处都红。
pub(crate) const DIAL_SITES: &[(&str, usize, bool, &str, &str)] = &[
    (
        "ssh_source.rs",
        0,
        true,
        "〔C2 09-24〕**搬走了**：跳板 · 一次性 exec · 收全 exec · 测试连接 · 后端长连接流全部改经拨号代理的宿主 \
             （`dial_host`），拨号本身在后端 `dial/`。\
             〔棘轮史：`K-P6b` 09-06 登记 5 处 → `K-R59` 09-11 退役 `daemonless` 一处 → 4 → C2 → **0**。〕",
        "已经是 0。这一行留着是为了让「又长回来一处」当场红（处数钉成 0，不是删行）。",
    ),
    (
        "sftp.rs",
        0,
        true,
        "〔SR1b 09-24〕**搬走了**：SFTP 进了本机常驻后端（用户 V89），与其它 SSH 同一条连接；\
             部署经那条 `files` 链路（`dial_host::RemoteFs`），传输经 `transfer-*`。\
             〔从前：「SFTP 会话要一个 russh 连接句柄，不是一条字节流 ⇒ 仍在界面进程里拨，用 `inproc_dial.rs` 那一份」。〕",
        "已经是 0。这一行留着是为了让「又长回来一处」当场红（处数钉成 0，不是删行）。",
    ),
    // 〔MIG-1 · `99 §2.1 ⑬`〕`port_forward.rs` 那一行（C2 起处数钉 0）随文件删了：转发账也进了本机常驻后端
    //   （`src/backend/dial/forwards.rs`），界面 crate 里没有这份文件了。先例同下面 `inproc_dial.rs` 那一行。
    // 〔SR1b 09-24〕`inproc_dial.rs` 那一行（进程内那一份自己的跳板，只服务 `sftp.rs`）**兑现了它写的解锁条件**：
    //   「与 `sftp.rs` 那一行同一天删：SFTP 换走 ⇒ 本文件整份删 ⇒ `russh` 出 `Cargo.toml`」—— 三件同拍。文件不在了，行随之删。
];

/// 语料地板：低于这个字节数就判「语料没喂进来」，而不是「一处都没有」。
///
/// 🔴 这条与函数体那条地板一起，是 `P6bM4` 的被测对象。
const CORPUS_FLOOR_BYTES: usize = 80_000;
/// 入口函数体的地板：切不出函数体（或切出个空壳）时判据必须**自己先红**。
/// 〔C2 09-24〕**200 → 120**：判据的靶换成了拿链路的原语 `connect_and_exec_cmd`，它今天只转一句
/// （现打 178 字节）；120 仍比「切出来的是个空壳」（签名 ＋ 一对括号，约 90 字节）大。
const BODY_FLOOR_BYTES: usize = 120;

/// 语料：四份文件的**生产段**（剥掉 `#[cfg(test)]` 段）。
fn corpus() -> Vec<(&'static str, String)> {
    vec![
        (
            "ssh_source.rs",
            production_code(include_str!("../../src/bridge/src/ssh_source.rs")),
        ),
        (
            "sftp.rs",
            production_code(include_str!("../../src/bridge/src/sftp.rs")),
        ),
    ]
}

/// `connect_session(` 的**调用点**处数 —— 剔掉定义行本身（`fn connect_session(`）。
///
/// ⚠ 这把尺子**数不到**：`use` 别名、函数指针、宏里拼出来的调用。
/// 与 `K-P6-dial-census.py` 头注是同一个洞，**不声称堵住**。
fn call_sites(code: &str) -> usize {
    let needle = "connect_session(";
    let mut n = 0usize;
    let mut from = 0usize;
    while let Some(rel) = code[from..].find(needle) {
        let at = from + rel;
        from = at + needle.len();
        if code[..at].ends_with("fn ") {
            continue; // 定义行，不是调用点
        }
        n += 1;
    }
    n
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
                 那是**界面进程自己拨号**，本件要断的正是这一格。"
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
/// 〔C2 09-24〕判据的靶是 `connect_and_exec_cmd` 的函数体：十来处一次性查询共用的那个原语，钉住它就钉住了全部。
/// 收全 exec 那个原语另钉一条（走 `dial_host::capture(`）。
/// 〔DEL〕远端后端长连接流的入口今天是 `remote_resident::attach`（capture ＋ `dial_host::tunnel`），不再经本文件起流。
#[test]
fn the_backend_stream_entry_hands_the_dial_to_another_process() {
    let (_, prod) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "ssh_source.rs")
        .expect("语料里没有 ssh_source.rs");
    // 〔MIG-3b 续 · V41〕流那一个原语（`connect_and_exec_cmd`）随最后一个调用方（公钥推送）进本机后端删了 ⇒
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

/// ★ **「7 处里搬走 1 处」是机器数的，不是散文。**
///
/// 这一条与上一条是**两半**：上一条证「入口交出去了」，本条证
/// 「**别处一处都没搬**」。少了本条，「拨号搬出去了」这句话就没人拦得住。
#[test]
fn six_of_the_seven_dial_sites_are_still_in_this_process() {
    let c = corpus();
    let bytes: usize = c.iter().map(|(_, s)| s.len()).sum();
    assert!(
        bytes >= CORPUS_FLOOR_BYTES,
        "语料只有 {bytes} 字节（地板 {CORPUS_FLOOR_BYTES}）—— 抽取坏了，本条在空转"
    );
    let mut total = 0usize;
    for (file, want, _moved, what, _unlock) in DIAL_SITES {
        let (_, code) = c
            .iter()
            .find(|(n, _)| n == file)
            .unwrap_or_else(|| panic!("语料里没有 {file} —— 登记表指向一份不在的文件"));
        let got = call_sites(code);
        assert_eq!(
            got, *want,
            "{file} 里 `connect_session(` 的调用点有 {got} 处，登记的是 {want} 处。\n\
                 那一处是什么：{what}\n\
                 🔴 **这个数变了要先回答「搬走了没有」**：搬走了就把第三栏改成 `true` \
                 并在件文件里同轮改掉「7 处里搬走 1 处」那句话；只是重构就把数字改掉。"
        );
        total += got;
    }
    assert_eq!(
        total, 0,
        "界面侧拨号调用点总数是 {total}，登记的是 **0**（〔SR1b 09-24〕**2 → 0**：SFTP 那一家 —— `sftp.rs` 1 ＋ \
             它用的 `inproc_dial.rs` 跳板 1 —— 进了本机常驻后端，界面进程零 SSH）。\n\
             ⚠ 〔C2 09-24〕**6 → 2**：`ssh_source.rs` 4 与 `port_forward.rs` 1 搬进拨号代理；\
             `inproc_dial.rs` 那 1 处是 `ssh_source.rs` 原来那 4 处里的跳板一处**原样搬过去**的（只服务 SFTP）。\n\
             这两个数必须一起动 —— 本表就是那句话的家。\n\
             ⚠ 棘轮史：**7**（`K-P6b` 件文件 09-06 现打，逐处点名）→ **6**〔`K-R59` 09-11〕：\n\
             `daemonless` 轮询流那一处**退役**（`K35`：没有「没有后端」这回事）。\n\
             🔴 **本函数的名字里那个 `seven` 是 09-06 那一刻的数，刻意没改** ——\n\
             改名会打断 `K-P6b` 件文件（`:652` / `:803`）与 `audits/K-P6b-PM.md:281` 三处\n\
             按名字的引用，而那三份不在 `K-R59` 的写区里。**活的数住在本断言里，不在名字里。**"
    );
    // 「搬走了」那一栏与处数必须对得上：搬走的行处数恰好为 0，没搬走的行处数 > 0。
    for (file, n, moved, ..) in DIAL_SITES {
        assert_eq!(
            *moved,
            *n == 0,
            "{file}：登记着「搬走了={moved}」而处数是 {n} —— 两栏自相矛盾"
        );
    }
    let moved = DIAL_SITES.iter().filter(|(_, _, m, ..)| *m).count();
    assert_eq!(
        moved, 2,
        "登记表说有 {moved} 份文件的拨号已经搬走了（C2 之后 `ssh_source.rs` 与 `port_forward.rs` 两份；〔SR1b〕+ `sftp.rs`；\
         〔MIG-1〕3 → 2：`port_forward.rs` 整份删了、那一行随之删）"
    );
}

/// 登记表每条都要有非空理由与**解锁条件**（同 `no_timer_guard` 那张表的纪律）。
#[test]
fn every_registered_dial_site_says_what_it_is_and_when_it_could_go() {
    for (file, _n, _moved, what, unlock) in DIAL_SITES {
        assert!(
            what.chars().count() >= 20,
            "{file} 的说法太短，说不清那一处是什么"
        );
        assert!(
            unlock.trim().chars().count() >= 20,
            "{file} 没写**解锁条件** —— 要写的是「什么条件满足之后这一行就能删」，\
                 不是「为什么现在不能删」"
        );
    }
}

// 〔C2 09-24〕原来这里有一条 `every_registered_fallback_is_actually_decided_in_the_entry` 〔散文墓碑〕
// （逐条回落在入口函数体里真的有一个判断在做它）—— 回落整张删了，它随之删。

/// 🔴 **本件改动之前**的远端起流入口函数体，**逐字冻结**（那个入口〔DEL〕随远端流模式一形删了，冻结的是历史文本）。
///
/// 出处：`git show f10581c:src/bridge/src/ssh_source.rs` 的 `:1304-1321`
/// （分支尖 `f10581c` = 本件第二轮的最后一个提交，那时生产段一个字节都还没动）。
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
/// `guard_support` 那条 `every_backend_file_strips_clean` 的头注逐字预告过这一形，
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

/// 〔C2〕**C2 之前**的 `connect_and_exec_cmd` 函数体，逐字冻结（出处：`git show aede6f5d:src/bridge/src/ssh_source.rs`）。
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
    // 〔C2〕第二份冻结反例：C2 之前的那个**原语**本体（进程内 `connect_session` ＋ 开通道 ＋ exec）。
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
    // 〔MIG-3b 续〕真身删了（流那一个原语随公钥推送进本机后端）⇒ 喂它删之前那一版的形状（合成，逐字抄当时的函数体）。
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

// 〔C2 09-24〕原来这里还有两条：请求行按蛇形键写（`the_request_line_is_written_with_snake_case_keys`）〔散文墓碑〕
// 与代理二进制只从两处解析（`the_proxy_is_resolved_from_exactly_two_places_and_never_from_home`）〔散文墓碑〕。
// 请求的造法与解析搬进了宿主 `dial_host`，前一条的性质由
// `dial_host_tests::the_request_keys_are_the_ones_the_proxy_reads`〔散文墓碑〕接住（而且改成与后端源码异源对拍；〔MIG-1 收尾〕今天是 `the_request_hands_over_the_machine_as_is`）；
// 后一条的性质**改了**：`D11` 之后找不到代理要报而不是回落，开发树上要能找到，于是解析多了
// 「本机后端自释放那一份」这一处（读 `~/.cc-monitor/bin`，登记在 `local_read_surface_registry`）。

// ── P28：给这条源码扫描型守卫立**负对照** ──
//
// 判的不是产品性质，是「**剥法没把我要扫的那一段剥掉**」。
// 失效形状是现打过的：便宜近似 `src.split("\n#[cfg(test)]").next()` 只在
// 「第一个测试模块之后再没有生产代码」时才对。`ssh_source.rs` 今天 4490 行，
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
        "ssh_source_dial_move_judge · ssh_source.rs",
        include_str!("../../src/bridge/src/ssh_source.rs"),
        // 〔MIG-3b 续〕锚点换成收全那一个原语（流那一个删了）；它同样在第一个测试模块之后。
        &["pub async fn connect_and_exec_capture("],
    );
    // ⚠ 本判据的语料是**四份**文件，这里只立了 `ssh_source.rs` 那一份的对照 ——
    //    另两份（`sftp.rs` / `port_forward.rs`；〔SR1b〕`inproc_dial.rs` 那份整份删了）的针在它们各自第一个测试模块**之前**，
    //    便宜近似留得住 ⇒ 立对照会恒真。**那不是「已守住」，是「这一形在那两份上不成立」。**
}

/// 〔C2 · `设计/05 §13.8 ①`〕→〔SR1b · 2026-09-24〕**界面 crate 里点名 `russh` / `russh_sftp` 的文件：零**（带正控）。
///
/// C2 那一版钉的是「== {SFTP 那一家}」（`inproc_dial.rs` · `sftp.rs`），逐字写着「SFTP 换走了 ⇒ 把 `inproc_dial.rs` 整份删掉、
/// `Cargo.toml` 的 `russh` 一起删，本条改成零命中」—— 这一拍就是那一刀（用户 V89：SFTP 进本机常驻后端，界面进程零 SSH）。
/// 〔墓碑 —— 旧名 `russh_lives_only_where_sftp_still_needs_it`〔散文墓碑〕：名字说的是「只剩 SFTP 还要它」，今天没有谁要它了。〕
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
         界面经 `dial_host`（链路 · `RemoteFs`）与 `transfer-*` 够到它们。"
    );
    // 清单那一面：界面 crate 的依赖段里没有这两条（也没有只为它们而钉的 `primefield`）。
    let manifest = include_str!("../../src/bridge/Cargo.toml");
    for dep in ["russh", "russh-sftp", "primefield"] {
        let line = format!("{dep} = ");
        assert!(
            !manifest
                .lines()
                .any(|l| l.trim_start().starts_with(line.as_str())),
            "`src/bridge/Cargo.toml` 里又声明了 `{dep}` —— 界面进程零 SSH（V89）"
        );
    }
    assert!(
        manifest
            .lines()
            .any(|l| l.trim_start().starts_with("tokio = ")),
        "清单里连 `tokio = ` 都抠不到 —— 读错了文件，上面那条零命中在空转"
    );
}
