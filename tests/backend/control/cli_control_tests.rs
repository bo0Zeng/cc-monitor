//! # 要求住址：`设计/00 §1.6.2`（CLI 面从帧命令派生，几个口翻译成同一套内部调用）
//!
//! 核原文：`设计/00 §1.6.2` 逐字「帧命令会派生出同名的 CLI 面」；`设计/05 §11.2` 逐字「CLI 面从帧命令表自动派生」——
//! 闸门与分派同源、帧命令要么上 CLI 要么写明理由、CLI 入口只经注册表、探测口报的就是它能派发的，判的正是这件事。
//! 「CLI 入口不借用帧入口的安全理由」那一条没有设计原文，守的是生产头注里那段散文。〔JA1 点址 2026-09-24〕

/// ★★〔P4f 08-13〕**CLI 面的闸门与分派臂必须来自同一个源**。
///
/// # 它逮的是一条实测到的静默失效
///
/// 分派臂是派生的（`Some(f) if cli_control::handles(f)`），可 `main::is_query_mode`
/// 这道**闸门**读的是手写的 `SUBCOMMANDS`。于是往 `inbound::REGISTRY` 加一条命令：
/// 帧面立刻有了、`hello.commands` 也报了，而 CLI 面 —— 一行 warn
/// 「未知 flag，已忽略并照常进流模式」，然后 backend **进了流模式**。
///
/// 调用方看到的是：命令"存在"（能力探测报了它），调它却拿到一堆 jsonl 行。
/// P4d 的表里那段注释本来就写着「登记在这张表里是因为 `is_query_mode` 也读它」——
/// 是一条**要人记得**的纪律，而没有任何判据钉着。08-13 我就忘了，实测撞上。
#[test]
fn every_cli_exposed_command_is_in_the_query_mode_gate() {
    let missing: Vec<String> = REGISTRY
        .iter()
        .filter(|s| cli_exposed(s))
        .map(|s| flag_of(s.name))
        .filter(|f| !crate::SUBCOMMANDS.contains(&f.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "这些命令上了 CLI 面（分派臂认得），却不在 `main::SUBCOMMANDS` 这道闸门里：{missing:?}\n\
             后果是**静默的**：`is_query_mode` 把它当未知 flag ⇒ 打一行 warn 之后照常进流模式\n\
             ⇒ 调用方拿到的是一堆 jsonl 行，而不是它要的应答。把它加进 `SUBCOMMANDS`。"
    );
    assert!(
        REGISTRY.iter().filter(|s| cli_exposed(s)).count() >= 5,
        "CLI 面上不足 5 条 —— 本断言在空转"
    );
}
use super::*;

/// 帧面有、CLI 面没有的命令，**逐条登记理由**。
///
/// 形状抄 `readonly_guard::spawn_registry::ALLOWED`：把「为什么这条不上」写成**数据**，
/// 好让机检对着它比 —— 散文里说一遍，下一个人加命令时看不见。
const NOT_ON_CLI: &[(&str, &str)] = &[
    // 〔MIG-2〕本机起会话：计划里要读本进程的中转状态（`launch_relay`），一次性进程里没有中转 ⇒ 同 `launch-endpoint` 只上流面。
    (
        "launch-local",
        "它经 `accounts::upstream::endpoint::launch_relay` 读本进程的中转监听状态（中转住常驻后端进程里）；\
         一次性进程里没有中转 ⇒ 「非它不可」的号会被误拒、「有它更好」的会被说成直连 —— 同 `launch-endpoint` 那一格（`STREAM_ONLY`）。",
    ),
    (
        "cancel",
        "它取消的是**同一条连接上在飞的另一条命令**。一次性 exec 是「1 请求 1 响应 1 退出、\
         无 request-id」（`resolve_query` 头注逐字）⇒ 本进程里没有第二条命令可取消，\
         给它开 CLI 口只会回一条永远找不到目标的应答。要停一条 CLI 命令：杀那个进程。",
    ),
    // 〔SR1a〕链路四条：一条链路**活在一条流连接上**（每连接一张链路表，连接没了链路一条不留），
    // 一次性进程「1 请求 1 响应 1 退出」—— 开出来的链路在应答回去的那一刻就随进程一起没了。
    (
        "link-open",
        "链路活在一条流连接上（`dial/link.rs`：每连接一张表，连接没了链路全收）。\
         一次性进程回完应答就退出，开出来的链路当场随进程一起没了 ⇒ \
         CLI 口只能开出一条立刻死掉的链路，没有任何用处。",
    ),
    (
        "link-data",
        "它往**同一条连接上**某条开着的链路里送字节。一次性进程里没有开着的链路\
         （见 `link-open`），给它开 CLI 口只会回一条永远的 `no_such_link`。",
    ),
    (
        "link-credit",
        "它给**同一条连接上**某条开着的链路还信用（下行流控）。一次性进程里没有开着的链路，\
         也没有在读下行的那一方 ⇒ CLI 口没有意义。",
    ),
    (
        "link-close",
        "它关**同一条连接上**某条开着的链路。一次性进程里没有开着的链路（见 `link-open`），\
         进程一退它开过的一切本来就没了 ⇒ CLI 口没有意义。",
    ),
    // 〔SR1b〕传输四条：票表**活在一条流连接上**（每连接一张，连接没了在册的一律撤），
    // 进度走那条连接的出方向帧 —— 一次性进程两样都没有。
    (
        "transfer-upload",
        "它开的是一张**活在这条流连接上**的票（`control/transfer.rs`：每连接一张票表）。\
         一次性进程回完应答就退出，票当场随进程一起没了 ⇒ 之后没有谁能起跑它。",
    ),
    (
        "transfer-download",
        "同 `transfer-upload`：票活在流连接上，一次性进程开出来的票回完应答就没了。",
    ),
    (
        "transfer-start",
        "起跑之后的进度与终局走**同一条连接**的出方向 `transfer` 帧；一次性进程只回一条应答就退出，\
         进度没人收、终局也没人收（而且它手里压根没有那张票 —— 见 `transfer-upload`）。",
    ),
    (
        "ccm-probe",
        "〔E2 · V138〕派生名 `--ccm-probe` 是 ccm 自己的诊断口（同 `ccm-print`）；CLI 上要这几行就敲 `ccm --ccm-probe`，同一个函数。",
    ),
    (
        "ccm-print",
        "〔E2 · V138〕派生名 `--ccm-print` 是 ccm 自己的诊断口；二进制叫 `ccm` 时后端按 `SUBCOMMANDS` 分流，占了它就把 `ccm --ccm-print` 抢进后端。",
    ),
    (
        "apikey-routing",
        "〔DEL 续 · 主会话裁〕「中转在不在」读本进程的监听状态；一次性进程里没有中转，只能答 `running: false` —— 那是假话（`cli_control::STREAM_ONLY`）。",
    ),
    (
        "launch-endpoint",
        "〔DEL 续 · 主会话裁〕同 `apikey-routing`：一次性进程里只能答 `listening: false`（`cli_control::STREAM_ONLY`）。",
    ),
    (
        "resync",
        "〔RESYNC · 主会话 09-27 裁〕它对齐的是本进程里在跑的 watcher；一次性进程里一份都没有，只能答 `watchers: 0` —— 那是假话（`cli_control::STREAM_ONLY`）。",
    ),
    (
        "transfer-stop",
        "它撤的是**同一条连接上**在册的一趟传输；一次性进程里没有在册的票，只会回一条什么也没撤的 `ok`。",
    ),
];

/// ★ P4d-Y1：帧面的每一条命令，**要么有 CLI 入口，要么在 [`NOT_ON_CLI`] 里写明为什么没有**。
///
/// 钉的是**集合**，不是「加了 `--launch`」—— 后者选哪几条是我的主观，前者是数据对数据的镜子：
/// 以后帧面加一条命令，CLI 面不跟、又不写理由，这里就红。
#[test]
fn every_wire_command_is_either_on_the_cli_or_has_a_written_reason() {
    let mut exposed: Vec<&str> = Vec::new();
    let mut withheld: Vec<&str> = Vec::new();
    for spec in REGISTRY {
        if cli_exposed(spec) {
            exposed.push(spec.name);
        } else {
            withheld.push(spec.name);
        }
    }
    assert!(
        REGISTRY.len() >= 5,
        "REGISTRY 只有 {} 条 —— 本断言在空转",
        REGISTRY.len()
    );
    // ① 上了 CLI 面的，`spec_for` 必须真能查回来（否则 `run` 会落进 unknown_command）。
    for name in &exposed {
        let flag = flag_of(name);
        assert!(
            spec_for(&flag).is_some(),
            "{name} 判为上 CLI 面，但 `spec_for({flag})` 查不到 —— 入口是断的"
        );
    }
    // ② 没上的，必须逐条有理由；且理由表里不许有**已经上了**的命令（那种是过期的理由）。
    let reasons: Vec<&str> = NOT_ON_CLI.iter().map(|(n, _)| *n).collect();
    let mut missing: Vec<&&str> = withheld.iter().filter(|n| !reasons.contains(n)).collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "这些帧面命令没有 CLI 入口，也没写为什么：{missing:?}\n\
             要么让它上 CLI 面，要么进 `NOT_ON_CLI` 写明理由（写「以后再说」不算理由）。"
    );
    let stale: Vec<&&str> = reasons.iter().filter(|n| exposed.contains(n)).collect();
    assert!(
        stale.is_empty(),
        "`NOT_ON_CLI` 里这些命令**今天已经上了** CLI 面：{stale:?} —— 理由过期了，删掉它。"
    );
    for (name, why) in NOT_ON_CLI {
        assert!(
            why.chars().count() >= 40,
            "`NOT_ON_CLI` 里 {name} 的理由只有 {} 字 —— 那是占位不是理由",
            why.chars().count()
        );
    }
}

/// ★ P4d-Y1 的另一半：**CLI 入口不许自己实现命令**。
///
/// 上一条钉的是「集合对上了」，钉不住「它跑的是同一个函数」—— CLI 分支里自己拼一套
/// tmux 调用，集合照样对得上。所以这里钉**数据流**：本模块的生产段
/// 不许出现任何一个具体处理器的路径，唯一的出口是 `REGISTRY` 上那条 `spec.run`。
///
/// ⚠ 与它互补的是 `readonly_guard::spawn_registry` 那条 `SPAWN_SITES_TODAY == 6`：
/// 真去起了进程，那边会红。两条各挡一种绕法。
#[test]
fn the_cli_entry_never_names_a_concrete_handler() {
    let raw = include_str!("../../../src/backend/control/cli_control.rs");
    let prod = crate::guard_support::production_code(raw);
    for banned in [
        "control::launch::",
        "control::kill::",
        "control::resolve_query::",
        "Command::new(",
    ] {
        assert!(
            !prod.contains(banned),
            "CLI 入口的生产段出现了 {banned:?} —— 那是绕开 `REGISTRY` 自己接了一条实现。\n\
                 本模块的唯一出口是 `spec.run`；要改某条命令的行为，改它在 `REGISTRY` 上那条登记。"
        );
    }
    assert!(
        prod.contains("Run::Blocking(f) => f(req)") && prod.contains("Run::Async(f) => f(req)"),
        "派发不再直接跑 `spec.run` 了 —— 「一份实现，两个入口」这个支点断在这里"
    );
}

/// ★ P4d-Y2：探测口报的命令集合 **== 它真能派发的集合**。
///
/// 失效方式很具体：有人在 `probe()` 里手抄一份清单。那样加命令时它不报错，
/// 只是**开始说谎** —— 而 skill 按它的话决定走不走新路 ⇒ 静默降级，没有任何报错。
#[test]
fn the_probe_reports_exactly_what_it_can_dispatch() {
    let raw = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/cli_control.rs"
    ));
    assert!(
        !raw.contains("\"--launch\"") && !raw.contains("\"--kill\""),
        "`probe()` 里出现了硬编码的命令字面量 —— 清单必须从 `REGISTRY` 派生"
    );
    let reported: Vec<String> = REGISTRY
        .iter()
        .filter(|s| cli_exposed(s))
        .map(|s| flag_of(s.name))
        .collect();
    assert!(!reported.is_empty(), "探测口报空清单 —— 本断言在空转");
    for flag in &reported {
        assert!(
            spec_for(flag).is_some(),
            "探测口报了 {flag}，但 `run` 派发不了它 —— 探测口在说谎"
        );
    }
    // 反面：真能派发的，一条都不许漏报。
    for spec in REGISTRY.iter().filter(|s| cli_exposed(s)) {
        let flag = flag_of(spec.name);
        assert!(
            reported.contains(&flag),
            "{flag} 能派发却没进探测口清单 —— skill 会以为这台不支持它"
        );
    }
}

/// ★ **哪些命令不收输入 —— 登记在此，加一条就要来这儿被看一眼。**
///
/// ⚠ 本条的**上一版是恒真的**：那时 `reads_stdin` 就是 `!fields.is_empty()`，
/// 而判据写的是「`fields` 空 ⇒ 不读 stdin；非空 ⇒ 读」—— 两个分支各自是同一个表达式的
/// 复述，**永远不可能红**。`bus-list` 挂死那次它一声没吭。
/// ⇒ 现在钉的是**登记表与声明对不对得上**（两个独立来源），且真不真由行为判据验
///（`tests/e2e/backend-cc-bus.sh`：声明无输入的命令，stdin 不关时必须秒回）。
#[test]
fn the_no_input_commands_are_registered_and_declared_consistently() {
    /// 不收入方向载荷的命令。**加一条就来这里写一行**。
    ///
    /// 〔`K-R113` 09-13〕`bus-state` 是第三条：它与 `bus-list` 同族 —— **无输入、有输出字段**，
    /// 正是当年那个 `!fields.is_empty()` 代用品会判错的形状。
    ///
    /// 〔步 `24f` 第二刀 09-20〕`files-index-status` 是第四条，**同一形**：
    /// 它一个入参都没有（`files::CAPABILITIES` 里那条 `args` 就是空的），
    /// 出方向却有十个字段。声明成收输入 ⇒ `--files-index-status` 会挂在那儿等 EOF，
    /// 而它恰恰是「这台机器上的索引新鲜不新鲜」那条**探活式**问话。
    /// ⚠ 同族另外三条（`files-ls` / `files-stat` / `files-find`）**要**输入，不在这张表里。
    ///
    /// 〔`C1` · 09-24〕只读查询面进来三条，**同一形**（无入参、有输出字段 `lines`）：
    /// `history-projects`（列全部项目）· `accounts-list` · `accounts-sessions`
    /// （账号库目录走默认解析，帧面不收 `--accts-dir`）。同族另外五条要输入，不在表里。
    ///
    /// 〔B2 · 条 66〕`exit-policy-read` 进来，**同一形**（无入参、有输出字段 `state` / `killOnExit` …）：
    /// 它是「那台机器上的值是什么」那一问，挂住等 EOF 就是把一句问话变成一次卡死。
    /// 同族 `exit-policy-set` 要输入（`killOnExit`），不在表里。
    /// 〔C4c · 第四波 4B〕`accounts-list` **出了这张表**：它从此收一格 `agent`（这次起会话的是哪一家，
    /// 并 apikey 表要看它）⇒ 要输入。
    const NO_INPUT_TODAY: &[&str] = &[
        // 〔MIG-3a · 子步 3〕cc-bus 装到这台 / 查三态：落点由这台自己算（skills 根），不收任何参数。
        "cc-bus-install",
        "cc-bus-install-state",
        "accounts-sessions",
        "bus-list",
        "bus-state",
        // 〔F7a · 第三波 09-24〕`files-home`：问这台机器的 home，无入参、有输出字段 `path`。
        "files-home",
        "exit-policy-read",
        // 〔E2〕`ccm-probe`：无入参（CLI 面没有，但「收不收输入」按帧面声明判）。
        "ccm-probe",
        // 〔RM1a · 第四波〕`apikey-read`：这台机器上那份凭据文件的状态，无入参。
        // 同族 `apikey-key-set` 要输入（`account` / `key`，key 从 stdin 进），不在表里。
        "apikey-read",
        "files-index-status",
        // 〔C4d · 第四波 4B〕`history-projects` 出列：它从此收 `origin`（缺席 = 这台），要读 stdin。
        "ping",
        // 〔RM1b · 第四波〕问这台机器登记了哪些插件市场：无入参，输出 `lines`。
        "plugins-marketplaces",
        // 〔AS2 · 第四波 4B〕这台现扫一次资产、记进目录、回整份：无入参。
        "assets-catalog",
        // 〔C4d · 第四波 4B〕sid → 上次用哪个号起：无入参（读本机那份注解文件）。
        "history-last-accounts",
        // 〔SU1 · 第四波 4C〕这台记着的、从别处装来的 skill：无入参（读本机那份装记录）。
        "skill-installs",
        // 〔LOC1a · 第四波 4D〕这台机器的 `cc-acct-iso` 两问：无入参（问的就是「这台」）。
        "acct-iso-status",
        "acct-iso-shellinit",
        // 〔SH1〕列这台的 tmux 会话：无入参（问的就是「这台」）。
        "tmux-list",
        // 〔MIG-3b〕这台的 cc-bus 钩子诊断：无入参（问的就是「这台」）。
        "hooks-diag",
    ];
    let declared: Vec<&str> = REGISTRY
        .iter()
        .filter(|s| !s.takes_input)
        .map(|s| s.name)
        .collect();
    let mut want: Vec<&str> = NO_INPUT_TODAY.to_vec();
    want.sort();
    let mut got = declared.clone();
    got.sort();
    assert_eq!(
        got, want,
        "「不收输入」的命令集变了。\n\
             ⚠ 这不是改数字了事：一条**要**输入的命令若声明成不收，它的 `args` 永远是 {{}}；\n\
             一条**不要**输入的命令若声明成收，它会**挂在那儿等 EOF** —— \n\
             CLI 面是给第三方 skill 调的，那是所有失败里最坏的一种（`--ping` 与 `--bus-list` 各栽过一次）。"
    );
    assert!(!declared.is_empty(), "一条无输入命令都没有 —— 本断言在空转");
    // `run` 必须用的就是这个决定，不是另写一份判断。
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/cli_control.rs"
    ));
    assert!(
        prod.contains("if reads_stdin(spec) {"),
        "`run` 不再按 `reads_stdin` 决定读不读 stdin —— 上面那条断言此刻钉的是一个没人用的函数"
    );
}

/// ★ P4d-Y4：帧入口那条安全边界的**理由**不许被搬到这里。
///
/// 钉的不是「有没有写边界」（写段散文最容易过），而是**那句借来的依据不许出现** ——
/// 逼下一个人写他自己的理由。
#[test]
fn the_cli_entry_does_not_borrow_the_frame_entrys_boundary_reason() {
    let raw = include_str!("../../../src/backend/control/cli_control.rs");
    // 下面两串是 `launch.rs` 那句依据的核心措辞（讲对端的 SSH 身份那句）。
    // ⚠ 本注释**刻意不复述它们** —— 复述一遍，本判据自己就会把自己判红。
    for borrowed in ["握着这台机器的 SSH 会话", "对端本来就握着"] {
        let hits = raw.matches(borrowed).count();
        assert!(
            hits <= 1,
            "本模块出现了 {borrowed:?} {hits} 次 —— 那是帧入口的依据。\n\
                 CLI 入口的调用方是本机任意进程，那条依据在这里不成立；写你自己的。\n\
                 （一次是本判据自己的字面量，多于一次就是被搬过来了。）"
        );
    }
    assert!(
        raw.contains("§安全边界"),
        "本模块没有 `§安全边界` 那一节 —— P4d-Y4 要求 CLI 入口写下它自己的理由"
    );
}

// ── 〔W5-AUX · `设计/96 §3.6`〕「只读一行 stdin」的入口 ─────────────────────────────
//
// 要求住址：`设计/96 §3.6` 逐字「远端命令走 POSIX shell 管道 …… 远端登录 shell 是 fish 之类就不成立。
// 根治要给 CLI 面一个『只读一行 stdin』的入口」。capture 那一跳不关远端 stdin ⇒ 这个入口的全部价值是
// **读到换行就停、不再多要一个字节**（多要一个就挂住，与 `--ping` 那次同一族病）。

/// 读端：先交一段字节（按调用方的缓冲大小分几次交），交完**再被读就炸**
/// （代替「stdin 永远不关」—— 真管道上那一刻是永远挂住，测试里换成当场红）。
struct ThenPanic(Option<Vec<u8>>);
impl std::io::Read for ThenPanic {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(mut bytes) = self.0.take() else {
            panic!("读过了第一行还在要字节 —— 真管道上这一刻就是永远挂住");
        };
        let n = bytes.len().min(buf.len());
        buf[..n].copy_from_slice(&bytes[..n]);
        let rest = bytes.split_off(n);
        if !rest.is_empty() {
            self.0 = Some(rest);
        }
        Ok(n)
    }
}

#[test]
fn the_one_line_entry_stops_at_the_newline_and_never_asks_for_another_byte() {
    let line = b"{\"path\":\"/\"}\n".to_vec();
    let got = read_input(std::io::BufReader::new(ThenPanic(Some(line.clone()))), true)
        .expect("一行入参读不出来");
    assert_eq!(got.trim(), "{\"path\":\"/\"}");
    // 阴性对照：默认那一形（读到 EOF）在同一个读端上**会**再要字节 —— 否则上面那条证明不了「只读一行」是旗标买来的。
    let default_form = std::panic::catch_unwind(|| {
        let _ = read_input(std::io::BufReader::new(ThenPanic(Some(line))), false);
    });
    assert!(
        default_form.is_err(),
        "读到 EOF 那一形居然也没再要字节 —— 读端替身坏了，上面那条是空转"
    );
    // 上限：没有换行、超过上限 ⇒ 拒（不截断），也不许一直读下去。
    let big = vec![b'x'; MAX_CLI_STDIN as usize + 10];
    let r = read_input(std::io::BufReader::new(ThenPanic(Some(big))), true);
    assert_eq!(r.map_err(|e| e.0), Err("args_too_large"));
    // 旗标字面量与 `asset_sync` 推那一趟拼的是同一个常量（远端认的就是它）。
    assert!(crate::asset_sync::push_command().ends_with(&format!(
        " {}",
        shell_quote_core::posix_quote(STDIN_LINE_FLAG)
    )));
}

/// 〔FIX · `设计/96 §3.6`〕argv 一族的一行形：恰好 `--<老子命令> --stdin-line` 才读，读到换行就停；
/// 别的形状（没有修饰词 · 多一个词 · 帧命令自己的 `--stdin-line`）原样返回、一个字节不读。
#[test]
fn the_argv_family_reads_its_tail_from_one_stdin_line_and_only_in_that_shape() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let never = || std::io::BufReader::new(ThenPanic(None));
    let got = expand_stdin_argv(
        v(&["--list-sessions", STDIN_LINE_FLAG]),
        std::io::BufReader::new(ThenPanic(Some(b"[\"-home-u-it's\"]\n".to_vec()))),
    )
    .expect("一行读得动");
    assert_eq!(got, v(&["--list-sessions", "-home-u-it's"]));
    for untouched in [
        v(&["--list-projects"]),
        v(&["--list-sessions", "-w-alpha"]),
        v(&["--list-sessions", STDIN_LINE_FLAG, "x"]),
        v(&["--assets-catalog-merge", STDIN_LINE_FLAG]),
    ] {
        assert_eq!(
            expand_stdin_argv(untouched.clone(), never()).expect("原样"),
            untouched
        );
    }
    let bad = expand_stdin_argv(
        v(&["--list-sessions", STDIN_LINE_FLAG]),
        std::io::BufReader::new(ThenPanic(Some(b"{\"not\":\"an array\"}\n".to_vec()))),
    );
    assert_eq!(bad.map_err(|e| e.0), Err("bad_request"));
}
