//! `K-P6b` `D2` 判据的**甲半**（界面这一侧）＋ 它的反向自检。
//!
//! # 它断的是哪一个性质
//!
//! **「daemon 那条长连接流」的那一跳 SSH 握手，由谁去跑。**
//!
//! 🔴 **它断的不是「`russh` 这个词还在不在」** —— `K-P6` 那一拍已经证过后者会量错集合
//! （**14 份在往外拨的文件里，11 份的 `russh` 代码态是 0**）。
//! 🔴 **它断的也不是「`connect_session` 这个名字还在不在」** —— 那个名字**一处都没少**：
//! 它的 7 处生产调用点里本件只覆盖 1 处，而覆盖的方式是**让入口不再走到它**，
//! 不是删掉它。量名字量到的会是「什么都没变」。
//!
//! ⇒ 判据落在**入口函数的函数体**上：`connect_and_exec` 里有没有一条把这一跳交出去的路，
//! 以及**回落那一条有没有被登记出来**。
//!
//! # 乙半在哪
//!
//! 乙半（代理这一侧：拨号只许住 `dial/`）住 `src/backend/dial/mod.rs`
//! 的测试模块。**两侧各扫各的 crate**，刻意不互相 `include_str!`
//! —— 那会新增一条跨轨编译期边，而那张登记表（`cross_half_edge_registry`）
//! 不在本轮写区里。**两侧各钉一半**，与 `build_id_guard` / `protocol_doc_guard` 同形。

use guard_core::production_code;

use super::RemoteConfig;

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
        4,
        false,
        "跳板（`connect_via_jump`）· 一次性 exec（`connect_and_exec_cmd`）· \
             收全输出的 exec（`connect_and_exec_capture`）· 测试连接。\
             ⚠ **`connect_and_exec` 不在这 4 处里** —— 它调的是 `connect_and_exec_cmd`，\
             而 `K-P6b` 动的正是「它还走不走那一条」，不是把 `connect_session` 从这几处删掉。\
             〔`K-R59` 09-11：**5 → 4**，退役的是 `daemonless` 轮询流那一处。\
             ⚠ **那不是「拨号搬走了」**，是那条路整个不存在了 ⇒ 第三栏仍是 `false`。〕",
        "第三阶段（收那 18 处 `connect_and_exec_cmd` 调用点）落地、\
             或测试连接那条也改走代理的那天。**本件明写不做**（`§2.1`）。",
    ),
    (
        "sftp.rs",
        1,
        false,
        "SFTP 会话（部署 / 传文件）。它要的是**原始字节**，而消费者不是界面 ⇒ 另一形状。",
        "`§2.2` 那条「不动 SFTP 与端口转发」被撤销、并且有人先答出\
             「SFTP 的字节怎么跨进程交回来」的那天。",
    ),
    (
        "port_forward.rs",
        1,
        false,
        "端口转发（每条转发一条独立 SSH 会话 + `channel_open_direct_tcpip`）。同上：\
             要原始字节，消费者不是界面。",
        "同 `sftp.rs` 那一条，两条一起解锁 —— `K-P7` 把它们判成同一形状。",
    ),
];

/// 🔴 **回落条件逐条登记** —— 「这台机器上拨号仍在界面进程里」的每一种成因。
///
/// `(判断它的源码片段, 它说的是什么, 解锁条件)`
///
/// 少了这张表，回落就成了一条只写在注释里的话；而注释腐烂之后，
/// 「本件把拨号搬出去了」这句过头话就没人拦得住。
const FALLBACKS: &[(&str, &str, &str)] = &[
    (
        "resolve_dial_proxy()",
        "拿不到代理二进制。⚠ **射程是「开发树」，不是「默认装机」**：\
             `externalBin` 住 `tauri.sidecar.conf.json`、只在发版那一步注入 ⇒ \
             `cargo run` 恒空；而发版包里 sidecar 就在 exe 旁边（`release.yml` 的 \
             `Build local backend sidecar (native)` + `Stage sidecar for externalBin`）⇒ 命中。",
        "把 `externalBin` 并进主配置的那天（今天刻意不并 —— \
             `release.yml` 头注写着并进去会让 `cargo test` 也要一份 daemon 二进制）。",
    ),
    (
        "has_key",
        "配置里没填 `keyPath` ⇒ 这台走的是 ssh-agent，而代理今天只会 publickey。\
             不回落就等于把一批今天能用的 Windows 用户弄坏。",
        "代理那侧把 ssh-agent 鉴权补上的那天（Windows 是命名管道、Unix 是 \
             `SSH_AUTH_SOCK`，界面侧今天也只有 Windows 那一半）。",
    ),
];

/// 语料地板：低于这个字节数就判「语料没喂进来」，而不是「一处都没有」。
///
/// 🔴 这条与函数体那条地板一起，是 `P6bM4` 的被测对象。
const CORPUS_FLOOR_BYTES: usize = 100_000;
/// 入口函数体的地板：切不出函数体（或切出个空壳）时判据必须**自己先红**。
const BODY_FLOOR_BYTES: usize = 200;

/// 语料：三份文件的**生产段**（剥掉 `#[cfg(test)]` 段）。
fn corpus() -> Vec<(&'static str, String)> {
    vec![
        (
            "ssh_source.rs",
            production_code(include_str!("../../src/bridge/src/ssh_source.rs")),
        ),
        ("sftp.rs", production_code(include_str!("../../src/bridge/src/sftp.rs"))),
        (
            "port_forward.rs",
            production_code(include_str!("../../src/bridge/src/port_forward.rs")),
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
/// 与 `local_daemon.rs::body_of` 同形 —— 本仓已经在用这一把尺子，不另发明一把。
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
/// 传进来的是 `connect_and_exec` 那个函数体。返回 `Err(说法)` = 判据红。
fn daemon_stream_dial_verdict(body: &str) -> Result<(), String> {
    if body.len() < BODY_FLOOR_BYTES {
        return Err(format!(
            "切出来的入口函数体只有 {} 字节（地板 {BODY_FLOOR_BYTES}）—— 本判据此刻在空转。\n\
                 「函数体里一处都没有」与「压根没切出函数体」在终端上一模一样，\
                 这条地板就是把它们分开的那一刀。",
            body.len()
        ));
    }
    let count = |p: &str| body.matches(p).count();
    let n_resolve = count("resolve_dial_proxy()");
    let n_proxy = count("spawn_dial_proxy(");
    let n_inproc = count("connect_and_exec_cmd(");
    let n_direct = count("connect_session(");

    if n_direct != 0 {
        return Err(format!(
            "入口函数体里直接出现了 {n_direct} 处 `connect_session(` —— \
                 那是**界面进程自己拨号**，本件要断的正是这一格。"
        ));
    }
    if n_proxy != 1 {
        return Err(format!(
            "入口函数体里 `spawn_dial_proxy(` 有 {n_proxy} 处（应当恰好 1 处）。\n\
                 0 处 ⇒ **拨号那一跳退回界面进程了**，`K-P6b` 买到的那一格没了；\n\
                 ≥2 处 ⇒ 有第二条交出去的路，而回落登记只认得一条。"
        ));
    }
    if n_resolve != 1 {
        return Err(format!(
            "入口函数体里 `resolve_dial_proxy()` 有 {n_resolve} 处（应当恰好 1 处）——\
                 「走不走代理」这个判断只许有一个地方做。"
        ));
    }
    // 🔴 回落那一条**必须在**，而且必须**登记出来**：它就是「默认装机上搬走 0 处」的落点。
    //
    // 〔订正 2026-09-10（v3.7.0）—— 动的是**失败文案**，不是判据。
    //  墓碑，原文逐字：「0 处 ⇒ 代理拿不到二进制时 daemon 流会直接断
    //  （**今天安装包没有 sidecar，那是 F05b**）」。括号里那句今天不成立。
    //  证伪它的读数：09-10 干净 win11 虚拟机上现打（PM，真安装包 + 真裸 exe 各一趟）——
    //  装出来那份 `C:\Program Files\cc-monitor\` 下 `cc-monitor-remote.exe` **2 个进程在跑**、
    //  裸 `monitor.exe` 那份 **0 个** ⇒ 发版包里 sidecar 就在 exe 旁边
    //  （本模块头注回落①那四环记的就是这件事，只是这条文案没跟着改）。
    //
    //  ⚠ **判据守的那个性质没跟着过期，仍然成立** —— 过期的只是文案里用来论证它的那个理由。
    //  「回落必须在、且恰好一条」今天仍有真人群，而且是**三**批，一批都不靠 F05b：
    //  ① 走 ssh-agent 的用户（`has_key` 为假 —— 代理只会 publickey，这条 F05b 一个字没动，
    //     登记在 `FALLBACKS` 的 `has_key` 那一行）；
    //  ② 开发树（`externalBin` 刻意不进基础 `tauri.conf.json` ⇒ `cargo run` 上 resolve 恒空）；
    //  ③ `CCM_DIAL_PROXY` 指到一个不是文件的路径（`resolve_dial_proxy` 出声后回 `None`）。
    //  ⇒ 删掉回落照旧是「把主平台弄坏」，只是弄坏的从「所有人」缩成这三批 ——
    //  **人群变窄了，性质没变**，所以这条不换靶。
    //
    //  ★ 与 `local_backend.rs` 那条「换靶」**不同形**，别读混：那条的**目的**随 F05b 过期
    //  （没人再需要「补上它」），必须换掉；本条的触发条件仍是 `n_inproc != 1`，一格没挪，
    //  改的只是红了之后说给人听的那句话。〕
    if n_inproc != 1 {
        return Err(format!(
            "入口函数体里 `connect_and_exec_cmd(` 有 {n_inproc} 处（应当恰好 1 处 = 那条**回落**）。\n\
                 0 处 ⇒ 代理拿不到二进制（开发树 · `CCM_DIAL_PROXY` 指错）、\
                 或这台走 ssh-agent 时，daemon 流会直接断 \
                 —— 那不是「搬出去了」，那是把主平台弄坏了；\n\
                 ≥2 处 ⇒ 回落不止一条，而这张表只认得一条。"
        ));
    }
    let (Some(i_proxy), Some(i_inproc)) = (
        body.find("spawn_dial_proxy("),
        body.find("connect_and_exec_cmd("),
    ) else {
        return Err("上面数到了，这里却找不到位置 —— 抽取器自相矛盾".to_string());
    };
    if i_proxy > i_inproc {
        return Err(
            "回落那一条排在代理那一条**前面** —— 那等于默认走进程内拨号，代理成了死代码。"
                .to_string(),
        );
    }
    Ok(())
}

/// ★ 甲半：**daemon 那条长连接流的入口，把拨号交给了代理进程；回落那一条被登记着。**
#[test]
fn the_daemon_stream_entry_hands_the_dial_to_another_process() {
    let (_, prod) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "ssh_source.rs")
        .expect("语料里没有 ssh_source.rs");
    let body = body_of(&prod, "pub async fn connect_and_exec(");
    if let Err(e) = daemon_stream_dial_verdict(&body) {
        panic!("{e}");
    }
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
        total, 6,
        "界面侧拨号调用点总数是 {total}，登记的是 **6**。\n\
             这两个数必须一起动 —— 本表就是那句话的家。\n\
             ⚠ 棘轮史：**7**（`K-P6b` 件文件 09-06 现打，逐处点名）→ **6**〔`K-R59` 09-11〕：\n\
             `daemonless` 轮询流那一处**退役**（`K35`：没有「没有后端」这回事）。\n\
             🔴 **本函数的名字里那个 `seven` 是 09-06 那一刻的数，刻意没改** ——\n\
             改名会打断 `K-P6b` 件文件（`:652` / `:803`）与 `audits/K-P6b-PM.md:281` 三处\n\
             按名字的引用，而那三份不在 `K-R59` 的写区里。**活的数住在本断言里，不在名字里。**"
    );
    let moved = DIAL_SITES.iter().filter(|(_, _, m, ..)| *m).count();
    assert_eq!(
        moved, 0,
        "登记表说有 {moved} 处的拨号已经搬走了。\n\
             🔴 **别把 `connect_and_exec` 那一处算进来** —— 它调的是 `connect_and_exec_cmd`，\
             那 5 处一处都没少；本件买到的是「入口不再无条件走到它」，\
             那一格由 `the_daemon_stream_entry_hands_the_dial_to_another_process` 钉，不由本表钉。"
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
    for (needle, what, unlock) in FALLBACKS {
        assert!(what.chars().count() >= 20, "回落 `{needle}` 的说法太短");
        assert!(
            unlock.trim().chars().count() >= 20,
            "回落 `{needle}` 没写解锁条件"
        );
    }
}

/// ★ **每一条登记的回落，在入口函数体里都真的有一个判断在做它。**
///
/// 这一条防的是「表在腐烂」的反方向：代码里把某条回落悄悄删了（于是那批用户被弄坏），
/// 而表还写着「我们照顾了他们」。
#[test]
fn every_registered_fallback_is_actually_decided_in_the_entry() {
    let (_, prod) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "ssh_source.rs")
        .expect("语料里没有 ssh_source.rs");
    let body = body_of(&prod, "pub async fn connect_and_exec(");
    assert!(
        body.len() >= BODY_FLOOR_BYTES,
        "切不出入口函数体（{} 字节）—— 本条在空转",
        body.len()
    );
    for (needle, what, _unlock) in FALLBACKS {
        assert!(
            body.contains(needle),
            "入口函数体里找不到 `{needle}` —— 这条回落被删了？\n\
                 它照顾的是：{what}\n\
                 真要删，先答「那批用户从此怎么办」，再把本表这一行一起删。"
        );
    }
}

/// 🔴 **本件改动之前**的 `connect_and_exec` 函数体，**逐字冻结**。
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
/// `guard_support` 那条 `every_daemon_file_strips_clean` 的头注逐字预告过这一形，
/// **这一轮把它撞出来了**。逐行存 ⇒ 那个 `}` 永远带着缩进，剥法看不见它。
const BODY_BEFORE_THIS_ITEM_LINES: &[&str] = &[
    "pub async fn connect_and_exec(",
    "    cfg: &RemoteConfig,",
    "    with_bg: bool,",
    "    tail_only: bool,",
    ") -> Result<russh::ChannelStream<client::Msg>, String> {",
    "    // 与 jsonl-watcher 不同，daemon 是长连接：inactivity_timeout=None → connect_session",
    "    // 自动启用 30s keepalive（见 FIX 1 注释），靠 keepalive + EOF 检死链，不靠定时拆链。",
    "    // Batch7-F24/Batch8-F26：两个流模式 flag 都由调用方决定（run_stream 里绑定",
    "    // 部署确认为当前版本，见该处注释）。tail_only=true → daemon 不重放历史",
    "    // （历史由本侧旁路 --read-session 快照拉取），实时通道流量趋零。",
    "    let mut cmd = shell_quote(&cfg.daemon_path);",
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
        !before.contains("spawn_dial_proxy("),
        "冻结的反例语料里居然有代理调用 —— 那它就不是「改动之前」了"
    );
    let e =
        daemon_stream_dial_verdict(&before).expect_err("旧形状（界面进程自己拨号）居然判绿了");
    assert!(
        e.contains("spawn_dial_proxy("),
        "判据红了，但**没点名是哪一处** —— 只说「有问题」的诊断等于没有诊断。实得：{e}"
    );
    assert!(
        e.contains("0 处"),
        "诊断没说清是「0 处」还是「多处」，那两种要修的东西完全不同。实得：{e}"
    );
}

/// ★ 反向自检 · **阴性方向**（`P6bM4`）：喂空输入，判据必须**自己先红**。
#[test]
fn an_empty_body_makes_the_judge_red_by_itself() {
    let e = daemon_stream_dial_verdict("").expect_err("空函数体居然判绿 —— 地板断言没接上");
    assert!(
        e.contains("空转"),
        "空输入红了，但红的理由不是「空转」—— 说明它被别的分支拦下了，地板没生效。实得：{e}"
    );
    // 再补一刀：非空但远小于地板，同样要以「空转」红。
    let e2 = daemon_stream_dial_verdict("fn f() {}\n").expect_err("小输入居然判绿");
    assert!(e2.contains("空转"), "小输入红的理由不对：{e2}");
}

/// ★ `P6bM1` 的**粗细追问**：只改一个注释字，判据**不许**红。
///
/// 照红 ⇒ 刀太粗（它其实在钉「这段文本一个字都不许动」，而不是钉那个性质）。
#[test]
fn a_comment_only_edit_does_not_move_the_verdict() {
    let (_, prod) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "ssh_source.rs")
        .expect("语料里没有 ssh_source.rs");
    let body = body_of(&prod, "pub async fn connect_and_exec(");
    daemon_stream_dial_verdict(&body).expect("真身就该是绿的");
    let edited = body.replace(
        "    let mut cmd = shell_quote(&cfg.daemon_path);",
        "    // 这一行是本判据现加的注释，只为证明它不按文本相等判\n\
             \x20   let mut cmd = shell_quote(&cfg.daemon_path);",
    );
    assert_ne!(edited, body, "注释没插进去 —— 本条在空转");
    daemon_stream_dial_verdict(&edited)
        .expect("只加了一行注释，判据就红了 ⇒ 刀太粗，它钉的是文本不是性质");
}

/// 请求行按**蛇形键**写出去。**两侧各钉一半** —— 那边钉「按蛇形读得动」。
#[test]
fn the_request_line_is_written_with_snake_case_keys() {
    let cfg = RemoteConfig {
        host: "h".into(),
        label: String::new(),
        port: 2222,
        user: "u".into(),
        key_path: Some("/k".into()),
        daemon_path: "/d".into(),
        host_key_fingerprint: Some("SHA256:x".into()),
        addresses: Vec::new(),
        jump: None,
    };
    let line = super::dial_request_json(&cfg, "/d --tail-only");
    assert_eq!(
        line.matches('\n').count(),
        0,
        "请求 JSON 里有换行 —— 它要塞进一个环境变量，换行只会让下一个人以为它还是行协议：{line:?}"
    );
    let v: serde_json::Value = serde_json::from_str(&line).expect("请求不是合法 JSON");
    assert_eq!(v["host"], "h");
    assert_eq!(v["port"], 2222);
    assert_eq!(v["user"], "u");
    assert_eq!(v["key_path"], "/k");
    assert_eq!(v["host_key_fingerprint"], "SHA256:x");
    assert_eq!(v["command"], "/d --tail-only");
    // 🔴 **私钥本体一个字节都不许出现在这一行里** —— 凭据面 `K11` 挡着，
    //    给代理的只有**路径**。这一条钉的是那句话，不是措辞。
    assert!(
        v.get("key").is_none() && v.get("private_key").is_none(),
        "请求行里出现了私钥字段：{line}"
    );
    // camelCase 一个都不许有（免得哪天有人「顺手」改成前端那套而两端悄悄漂开）。
    assert!(
        v.get("keyPath").is_none() && v.get("hostKeyFingerprint").is_none(),
        "请求行里出现了 camelCase 键 —— 两端的键名契约是蛇形独占：{line}"
    );
}

/// 代理二进制的解析面**只有两处**，而且都不伸手进家目录。
///
/// 钉它的理由：多加一处「聪明」的查找（比如去 `~/.cc-monitor/bin/` 翻）会
/// 新增一处本机读面，而那张表（`local_read_surface_registry`）按
/// 「生产段里的 `home_dir()`」取人群 —— 本条让那件事在这里先红一次。
#[test]
fn the_proxy_is_resolved_from_exactly_two_places_and_never_from_home() {
    let (_, prod) = corpus()
        .into_iter()
        .find(|(n, _)| *n == "ssh_source.rs")
        .expect("语料里没有 ssh_source.rs");
    let body = body_of(&prod, "pub(crate) fn resolve_dial_proxy()");
    assert!(
        body.len() > BODY_FLOOR_BYTES,
        "切不出 `resolve_dial_proxy` 的函数体（{} 字节）—— 本条在空转",
        body.len()
    );
    assert_eq!(
        body.matches("DIAL_PROXY_ENV").count(),
        2,
        "环境变量那一处的形状变了（一次 `var_os`、一次诊断里回显）：{body}"
    );
    assert_eq!(
        body.matches("resolve_beside_this_exe(").count(),
        1,
        "exe 旁那一处不再是恰好一次：{body}"
    );
    assert!(
        !body.contains("home_dir("),
        "`resolve_dial_proxy` 伸手进家目录了 —— 那会新增一处本机读面，\
             而 `local_read_surface_registry` 是按 `home_dir()` 取人群的。\
             真要加，先去那张表上登记。"
    );
}
