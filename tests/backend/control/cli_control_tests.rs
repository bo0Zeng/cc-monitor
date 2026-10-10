//! # （CLI 面从帧命令派生，几个口翻译成同一套内部调用）
//!
//! 核原文：「帧命令会派生出同名的 CLI 面」；「CLI 面从帧命令表自动派生」——
//! 闸门与分派同源、帧命令要么上 CLI 要么写明理由、CLI 入口只经注册表、探测口报的就是它能派发的，判的正是这件事。
//! 「CLI 入口不借用帧入口的安全理由」那一条没有设计原文，守的是生产头注里那段散文。〔JA1 点址 2026-09-24〕

/// ★★**CLI 面的闸门与分派臂必须来自同一个源**。
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
use crate::control::cli_args::{
    read_input, MAX_ARGS_B64_LEN, MAX_CLI_STDIN, STDIN_LINE_FLAG, STDIN_QUIET,
};

/// 帧面有、CLI 面没有的命令，**逐条登记理由**（不在这里的另两张在生产里：`STREAM_ONLY` 「结构上答不了」·
/// `UI_ONLY` 「只有界面用得着」，理由同样是数据；这张只收 `Run::Builtin` 与占了 ccm 的词那几条）。
///
/// 形状抄 `readonly_guard::spawn_registry::ALLOWED`：把「为什么这条不上」写成**数据**，
/// 好让机检对着它比 —— 散文里说一遍，下一个人加命令时看不见。
const NOT_ON_CLI: &[(&str, &str)] = &[
    // 测试连接边拨边推进度：那几格走**本连接的应答通道**（`probe` 帧，界面订 `probe-progress/<票>`）。
    (
        "remote-probe",
        "它的进度格（握手那几行 · 走完的那几段 · 结局）是往**发起它的那条流连接**的应答通道推的帧；一次性进程「1 请求 1 响应 1 退出」\
         没有那条通道，结局也在进度流里而不在应答里 ⇒ 开 CLI 口只会回一个空应答。测一台：设置页的「测试连接」（经常驻后端的帧面）。",
    ),
    // 终端实时预览三条：画面帧往**订它的那条流连接**的应答通道推，订阅活到退订或连接走；一次性进程「1 请求 1 响应 1 退出」没有那条通道 ⇒ 只上帧面。
    (
        "terminal-follow",
        "订阅之后的画面帧（`terminal_screen`）是往发起它的那条流连接的应答通道推的，订阅活到退订 / 连接走；一次性进程没有那条通道，\
         回完应答就退、订阅当场没了 ⇒ 开 CLI 口只会回一个空应答。命令行看一眼画面用 `--terminal-preview`。",
    ),
    ("terminal-follow-ack", "回执的是某条流连接上的那张票；一次性进程里没有票 ⇒ 同 `terminal-follow` 只上帧面。"),
    ("terminal-unfollow", "退订的是某条流连接上的那张票；一次性进程里没有票 ⇒ 同 `terminal-follow` 只上帧面。"),
    (
        "cancel",
        "它取消的是**同一条连接上在飞的另一条命令**。一次性 exec 是「1 请求 1 响应 1 退出、\
         无 request-id」（`resolve_query` 头注逐字）⇒ 本进程里没有第二条命令可取消，\
         给它开 CLI 口只会回一条永远找不到目标的应答。要停一条 CLI 命令：杀那个进程。",
    ),
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
        "派生名 `--ccm-probe` 是 ccm 自己的诊断口（同 `ccm-print`）；CLI 上要这几行就敲 `ccm --ccm-probe`，同一个函数。",
    ),
    (
        "ccm-print",
        "派生名 `--ccm-print` 是 ccm 自己的诊断口；二进制叫 `ccm` 时后端按 `SUBCOMMANDS` 分流，占了它就把 `ccm --ccm-print` 抢进后端。",
    ),
    (
        "launch-endpoint",
        "同 `apikey-routing`：一次性进程里只能答 `listening: false`（`cli_control::STREAM_ONLY`）。",
    ),
    (
        "transfer-stop",
        "它撤的是**同一条连接上**在册的一趟传输；一次性进程里没有在册的票，只会回一条什么也没撤的 `ok`。",
    ),

];

/// [`STREAM_ONLY`] 每一条的理由（一次性进程里结构上答不了）。与生产那张表两向相等（`the_withheld_tables_…`）。
const STREAM_ONLY_WHY: &[(&str, &str)] = &[
    (
        "resync",
        "它对齐的是本进程里在跑的 watcher；一次性进程里一份都没有，只能答 `watchers: 0` —— 那是假话。",
    ),
    (
        "apikey-routing",
        "「中转在不在」读本进程的监听状态；一次性进程里没有中转（中转住常驻后端进程里），只能答 `running: false` —— 那是假话。",
    ),
    (
        "relay-optin",
        "「直接敲的也走中转」那一段的成品里「中转在不在」同样读本进程的监听状态，一次性进程里恒答「不在」—— 假话。",
    ),
    (
        "launch-local",
        "它经 `accounts::upstream_select::endpoint::launch_relay` 读本进程的中转监听状态；一次性进程里没有中转 ⇒ \
         「非它不可」的号会被误拒、「有它更好」的会被说成直连。",
    ),
    (
        "forward-start",
        "转发账住常驻那一个进程：一次性进程开出来的转发随进程退出就没了 —— 口放掉、账也没了，回的 `id` 是个死号。",
    ),
    (
        "forward-stop",
        "同 `forward-start`：转发账住常驻进程，一次性进程里的账恒空，只会回 `not_found`。",
    ),
    (
        "forward-list",
        "同 `forward-start`：转发账住常驻进程，一次性进程里的账恒空，只能答 `forwards: []` —— 那是假话。",
    ),
    (
        "ext-list",
        "它按本进程的可达表认出每台叫什么、连没连上；一次性进程里那张表是空的 ⇒ 只剩本机、别的台全画成「没连上」，是假话。\
         本机那一半另有 `ext-list-here`（同一份 `ext::answer_list`，目录先裁到这台一格 ⇒ 可达表无从参与），它上 CLI 面。",
    ),
    (
        "ext-hub-preview",
        "枢纽要经本进程的可达表够到来源那台与被写那台；一次性进程里那张表是空的 ⇒ 除了本机对本机什么都问不到。",
    ),
    (
        "ext-hub-apply",
        "同 `ext-hub-preview`：两头都要经本进程的可达表够到；一次性进程里那张表是空的。",
    ),
    (
        "session-restart",
        "停旧 ＋ 起新那一步拿的是**退出排空的票**（`inbound::DRAIN`），等压缩摘要与等新进程报出各一次有界等待（各 ≤ 1 h）——\
         「发起方走了那台照样做完」靠的就是那张票钉住**常驻进程**不退；一次性进程里那张票钉的是命令自己那个进程，\
         钉不住任何东西，而调用方还得把一次调用挂在那儿几分钟 ⇒ 真依赖流语义。命令行那一侧要同一件事：\
         `--sessions-stop` 再 `--sessions-start`，自己掌期限。",
    ),
];

/// [`UI_ONLY`] 每一条的理由（答得了，只是除了界面没人用得着）。与生产那张表两向相等。
const UI_ONLY_WHY: &[(&str, &str)] = &[
    (
        "session-terminals",
        "它答「此刻哪个终端在显示这个会话」，唯一的用处是界面点 ↗ 那一刻接着交本机后端对窗口、再把那个窗口拉到前面；\
         只有能拉前桌面窗口的那一方用得着，第二个前端也不用。",
    ),
    (
        "terminal-processes",
        "它答「那台报来的终端连接是这台电脑上哪个进程开的、往上是谁」，是 ↗ 那一问的本机一半，只给界面接着找窗口、拉前用；\
         同 `session-terminals`，只有能拉前桌面窗口的那一方用得着。",
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
    let reasons: Vec<&str> = NOT_ON_CLI
        .iter()
        .chain(STREAM_ONLY_WHY)
        .chain(UI_ONLY_WHY)
        .map(|(n, _)| *n)
        .collect();
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
    for (name, why) in NOT_ON_CLI.iter().chain(STREAM_ONLY_WHY).chain(UI_ONLY_WHY) {
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
    /// `bus-state` 是第三条：它与 `bus-list` 同族 —— **无输入、有输出字段**，
    /// 正是当年那个 `!fields.is_empty()` 代用品会判错的形状。
    ///
    /// 〔步 `24f` 第二刀 09-20〕`files-index-status` 是第四条，**同一形**：
    /// 它一个入参都没有（`files::CAPABILITIES` 里那条 `args` 就是空的），
    /// 出方向却有十个字段。声明成收输入 ⇒ `--files-index-status` 会挂在那儿等 EOF，
    /// 而它恰恰是「这台机器上的索引新鲜不新鲜」那条**探活式**问话。
    /// ⚠ 同族另外三条（`files-ls` / `files-stat` / `files-find`）**要**输入，不在这张表里。
    ///
    /// 只读查询面今天只有 `accounts-sessions` 在表里（无入参、有输出字段 `lines`；账号库目录跟着家走）。
    ///
    /// 〔条 66〕`exit-policy-read` 进来，**同一形**（无入参、有输出字段 `state` / `killOnExit` …）：
    /// 它是「那台机器上的值是什么」那一问，挂住等 EOF 就是把一句问话变成一次卡死。
    /// 同族 `exit-policy-set` 要输入（`killOnExit`），不在表里。
    /// `accounts-list` **出了这张表**：它从此收一格 `agent`（这次起会话的是哪一家，
    /// 并 apikey 表要看它）⇒ 要输入。
    const NO_INPUT_TODAY: &[&str] = &[
        // cc-bus 装到这台 / 查三态：落点由这台自己算（skills 根），不收任何参数。
        "cc-bus-install",
        "cc-bus-install-state",
        // 账号库核对：只读，问的就是「这台」的账号库，不收参数。
        "accounts-verify",
        // 各账号共用的用户级 MCP 此刻的样子：同上，问的就是「这台」，不收参数。
        "accounts-mcp-read",
        "accounts-sessions",
        // 这台上需手动的会话清单：问的就是「这台」此刻在等人的那几个，不收参数（手机一次问一台）。
        "sessions-needs",
        "bus-list",
        "bus-state",
        // `files-home`：问这台机器的 home，无入参、有输出字段 `path`。
        "files-home",
        "drift-report", // 这台后端的漂移账（纯读、不收输入）
        "exit-policy-read",
        "quota-read",          // 这台的额度账（纯读、不收输入）
        "rotation-rules-read", // 这台的规则表（纯读、不收输入）
        // `ccm-probe`：无入参（CLI 面没有，但「收不收输入」按帧面声明判）。
        "ccm-probe",
        "cells-catalog", // 格目录（纯计算、不收输入）
        // `apikey-read`：这台机器上那份凭据文件的状态，无入参。
        // 同族 `apikey-key-set` 要输入（`account` / `key`，key 从 stdin 进），不在表里。
        "apikey-read",
        "files-index-status",
        "ping",
        // 问这台机器登记了哪些插件市场：无入参，输出 `lines`。
        // 这台现扫一次资产、记进目录、回整份：无入参。
        "assets-catalog",
        // sid → 上次用哪个号起：无入参（读本机那份注解文件）。
        "history-last-accounts",
        // 列这台的 tmux 会话：无入参（问的就是「这台」）。
        // 列转发：无入参（问的就是本进程那张账）。
        "forward-list",
        // 这台 `~/.ssh/config` 的别名清单：无入参（`ssh-config-resolve` 要 `alias`、`ssh-config-import` 要 `known`，都收输入）。
        "ssh-config-aliases",
        // 这台的 cc-bus 钩子诊断：无入参（问的就是「这台」）。
        "hooks-diag",
        // `relay-optin` 出列：入参带 `agent`（那一家的那一份）。
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

// ── 「只读一行 stdin」的入口 ─────────────────────────────
//
// 要求：「远端命令走 POSIX shell 管道 …… 远端登录 shell 是 fish 之类就不成立。
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
    assert!(
        crate::assets::asset_sync::push_command().ends_with(&format!(
            " {}",
            shell_quote_core::posix_quote(STDIN_LINE_FLAG)
        ))
    );
}

// ── 第二个前端够得着 CLI 面：入参不再隐含「调用方写得了 stdin」 ─────────────
//
// 第二个前端的执行通道只有 stdout、写不了 stdin，而 CLI 面上要入参的命令占了绝大多数（出成品的 `history-read` 一族全在里面）。
// ⇒ 入参另有一个 argv 形的口（[`crate::ARGS_B64_FLAG`]），与 stdin 二选一；stdin 开着却一直不写，也得立即回码。

/// 读端：一直不给字节、也不关（真管道上「对面开着 stdin 不写」）。手里那个 `Sender` 一放，它就当 EOF 收尾 ——
/// 测试结束时放掉，免得线程一直挂着。
struct Silent(std::sync::mpsc::Receiver<()>);
impl std::io::Read for Silent {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        let _ = self.0.recv();
        Ok(0)
    }
}
fn silent() -> (std::sync::mpsc::Sender<()>, Silent) {
    let (tx, rx) = std::sync::mpsc::channel();
    (tx, Silent(rx))
}

fn strs(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// 跑一遍 CLI 入口，收 (退出码, stdout, stderr)。
fn run_cli<R: std::io::Read + Send + 'static>(args: &[String], stdin: R) -> (i32, String, String) {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rc = rt.block_on(run_io(
        args,
        &Default::default(),
        stdin,
        std::time::Duration::from_millis(300),
        &mut out,
        &mut err,
    ));
    (
        rc,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn err_code(stderr: &str) -> String {
    serde_json::from_str::<serde_json::Value>(stderr.trim())
        .ok()
        .and_then(|v| v["code"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("<stderr 不是信封：{stderr:?}>"))
}

/// ★ argv 载荷口与 stdin **答得逐字一样**（退出码 · stdout · stderr 三样都比）。挑的是出成品的那几条与几条别族的。
/// 真记录文件上的那一趟（`history-read` 出成品行）在真二进制的 e2e 里（`tests/e2e/backend-cc-bus.sh` 的 [CLI2]）——
/// 进程内拿不到一棵沙箱里的会话记录树。
#[test]
fn the_argv_payload_answers_byte_for_byte_like_stdin() {
    let cases: &[(&str, serde_json::Value)] = &[
        (
            "history-read",
            serde_json::json!({"path": "/nonexistent/cli2nd/a.jsonl"}),
        ),
        ("history-read", serde_json::json!({})),
        (
            "history-facts",
            serde_json::json!({"path": "/nonexistent/cli2nd/a.jsonl"}),
        ),
        (
            "history-page",
            serde_json::json!({"path": "/nonexistent/cli2nd/a.jsonl", "from": 0}),
        ),
        (
            "launch-render-cli",
            serde_json::json!({
                "action": {"kind": "new"},
                "account": {"kind": "base"},
                "container": {"kind": "none"},
                "cwd": "/w",
                "launcher": "claude",
            }),
        ),
        ("files-stat", serde_json::json!({"path": "/"})),
    ];
    let mut saw_ok = false;
    for (cmd, payload) in cases {
        let flag = flag_of(cmd);
        assert!(
            spec_for(&flag).is_some(),
            "{flag} 不在 CLI 面上 —— 本条挑错了命令"
        );
        let text = payload.to_string();
        let by_stdin = run_cli(
            &strs(&[&flag]),
            std::io::Cursor::new(text.clone().into_bytes()),
        );
        let b64 = crate::stream::wire::b64_encode(text.as_bytes());
        let (_hold, quiet) = silent();
        // stdin 开着不写：argv 那一形**不该去碰 stdin**（碰了就是挂住或 no_input）。
        let by_argv = run_cli(&strs(&[&flag, crate::ARGS_B64_FLAG, &b64]), quiet);
        // 失败信封的复制详情首行是出错那一刻的时刻（两趟各自取钟）：抹掉它再逐字比。
        let unclocked = |(rc, out, err): (i32, String, String)| {
            let err = match serde_json::from_str::<serde_json::Value>(err.trim()) {
                Ok(mut v) if v["detail"].is_string() => {
                    v["detail"] = clockless(v["detail"].as_str().unwrap()).into();
                    v.to_string()
                }
                _ => err,
            };
            (rc, out, err)
        };
        assert_eq!(
            unclocked(by_argv),
            unclocked(by_stdin.clone()),
            "{flag} {text}：argv 载荷口与 stdin 答得不一样"
        );
        saw_ok |= by_stdin.0 == 0;
    }
    assert!(
        saw_ok,
        "一条成功的都没有 —— 只比了错误信封，证明不了成品那条路逐字一样"
    );
}

/// ★ stdin 开着、一直不写：**立即回码**，不挂住（缺入参就说缺入参）；EOF 那一形照旧当 `{}` 交给命令自己判。
#[test]
fn a_silent_open_stdin_answers_at_once_instead_of_hanging() {
    for extra in [&[][..], &[crate::STDIN_LINE_FLAG][..]] {
        let (_hold, quiet) = silent();
        let mut args = strs(&["--history-read"]);
        args.extend(strs(extra));
        // 在另一条线程里跑、本线程掐表：挂住时这条红在 10 s，不是把整趟测试拖死。
        let (tx, rx) = std::sync::mpsc::channel();
        let a = args.clone();
        std::thread::spawn(move || {
            let _ = tx.send(run_cli(&a, quiet));
        });
        let (rc, out, err) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("{args:?}：stdin 开着不写，10 s 还没回 —— 那是挂住"));
        assert_eq!((rc, out.as_str()), (2, ""), "{args:?}：该回失败信封");
        assert_eq!(err_code(&err), "no_input", "{args:?}：码不对：{err}");
    }
    // EOF：照旧当空入参，由命令自己说缺什么（不是 no_input）。
    let (rc, _, err) = run_cli(&strs(&["--history-read"]), std::io::empty());
    assert_eq!((rc, err_code(&err).as_str()), (2, "bad_args"));
}

/// ★ `--stdin-line` 与 `--text` 一样**认任意位置**（原先只认紧跟子命令那一格，别处 ⇒ 静默退回「读到 EOF」、挂住）。
#[test]
fn the_one_line_flag_counts_wherever_it_sits() {
    let line = b"{\"path\":\"/nonexistent/cli2nd/a.jsonl\"}\n".to_vec();
    let at_one = run_cli(
        &strs(&["--history-read", crate::STDIN_LINE_FLAG]),
        ThenPanic(Some(line.clone())),
    );
    let at_end = run_cli(
        &strs(&[
            "--history-read",
            "--some-later-option",
            crate::STDIN_LINE_FLAG,
        ]),
        ThenPanic(Some(line)),
    );
    assert_eq!(at_end, at_one, "`--stdin-line` 不在第二格时答得不一样");
    assert_ne!(err_code(&at_one.2), "no_input");
}

/// ★ 两个口二选一；用法错一律 `bad_args`，坏载荷 `bad_request`，超大 `args_too_large` —— 都是立即回、不截断。
#[test]
fn the_argv_payload_refuses_clearly() {
    let ok = crate::stream::wire::b64_encode(b"{}");
    let refuse = |args: &[&str]| {
        let (_hold, quiet) = silent();
        let (rc, out, err) = run_cli(&strs(args), quiet);
        assert_eq!((rc, out.as_str()), (2, ""), "{args:?} 该被拒");
        err_code(&err)
    };
    // 两个都给。
    assert_eq!(
        refuse(&[
            "--history-read",
            crate::ARGS_B64_FLAG,
            &ok,
            crate::STDIN_LINE_FLAG
        ]),
        "bad_args"
    );
    assert_eq!(
        refuse(&[
            "--history-read",
            crate::STDIN_LINE_FLAG,
            crate::ARGS_B64_FLAG,
            &ok
        ]),
        "bad_args"
    );
    // 给了两次 · 缺值。
    assert_eq!(
        refuse(&[
            "--history-read",
            crate::ARGS_B64_FLAG,
            &ok,
            crate::ARGS_B64_FLAG,
            &ok
        ]),
        "bad_args"
    );
    assert_eq!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG]),
        "bad_args"
    );
    // 不收入参的命令带它：用法错，不悄悄忽略（同 `--text`）。
    assert_eq!(refuse(&["--ping", crate::ARGS_B64_FLAG, &ok]), "bad_args");
    // 坏 base64 · 解出来不是 UTF-8 · 不是 JSON。
    assert_eq!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG, "e30"]),
        "bad_request"
    );
    let not_utf8 = crate::stream::wire::b64_encode(&[0xff, 0xfe]);
    assert_eq!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG, &not_utf8]),
        "bad_request"
    );
    let not_json = crate::stream::wire::b64_encode(b"{nope");
    assert_eq!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG, &not_json]),
        "bad_request"
    );
    // 超大：argv 那一形自己的上限（比系统单个参数的上限小一点，系统放得进来的才轮得到它说）。
    let big = "A".repeat(MAX_ARGS_B64_LEN + 4);
    assert_eq!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG, &big]),
        "args_too_large"
    );
    let just = crate::stream::wire::b64_encode(&vec![b' '; MAX_ARGS_B64_LEN / 4 * 3]);
    assert!(just.len() <= MAX_ARGS_B64_LEN);
    assert_ne!(
        refuse(&["--history-read", crate::ARGS_B64_FLAG, &just]),
        "args_too_large",
        "刚好到上限的不该被拒成太大"
    );
    // stdin 那一形超大：同一个码。
    let (rc, _, err) = run_cli(
        &strs(&["--history-read"]),
        std::io::Cursor::new(vec![b' '; MAX_CLI_STDIN as usize + 1]),
    );
    assert_eq!((rc, err_code(&err).as_str()), (2, "args_too_large"));
}

/// argv 那一形的上限**留在系统单个参数的上限之内**：Linux `MAX_ARG_STRLEN` ＝ 32 页 ＝ 131072 字节（含结尾 NUL），
/// 经 ssh 时整行命令是登录 shell `-c` 的**一个**参数 ⇒ 同一个上限管整行。比它大，本后端根本起不来、轮不到它回码。
#[test]
fn the_argv_cap_sits_inside_the_systems_single_argument_cap() {
    const LINUX_MAX_ARG_STRLEN: usize = 32 * 4096;
    assert!(
        MAX_ARGS_B64_LEN < LINUX_MAX_ARG_STRLEN,
        "argv 载荷口的上限超过了系统单个参数的上限 —— 那一段永远轮不到本后端回码"
    );
    assert_eq!(
        MAX_ARGS_B64_LEN % 4,
        0,
        "上限不是 4 的倍数 —— 刚好到上限的合法 base64 也会被拒"
    );
    assert!(
        (MAX_ARGS_B64_LEN as u64) / 4 * 3 <= MAX_CLI_STDIN,
        "argv 口解出来的上限比 stdin 那一形还大"
    );
}

/// ★ 协议文档写的数与代码里的常量是同一个（上限 · 静默窗 · 两个旗标 · 码）。改了常量不改文档，这里红。
#[test]
fn the_protocol_doc_states_the_input_limits_the_code_enforces() {
    let doc = include_str!("../../../src/doc/IPC-PROTOCOL.md");
    let sec = doc
        .split("## 8. CLI 一次性调用")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .expect("IPC-PROTOCOL.md 没有「8. CLI 一次性调用」那一节");
    let mib = MAX_CLI_STDIN / (1024 * 1024);
    assert_eq!(
        MAX_CLI_STDIN,
        mib * 1024 * 1024,
        "MAX_CLI_STDIN 不是整 MiB，下面这条比法要改"
    );
    for want in [
        crate::ARGS_B64_FLAG.to_string(),
        crate::STDIN_LINE_FLAG.to_string(),
        format!("{mib} MiB（{MAX_CLI_STDIN} 字节）"),
        format!("{MAX_ARGS_B64_LEN} 字节"),
        format!("{} ms", STDIN_QUIET.as_millis()),
        "`args_too_large`".to_string(),
        "`no_input`".to_string(),
        "任意位置".to_string(),
    ] {
        assert!(
            sec.contains(&want),
            "协议文档 §8 没写 {want:?}（代码里是这个数 / 这个词）"
        );
    }
}

/// ★ 不上 CLI 面的理由分三种、各住一张表，**互不重叠**：`STREAM_ONLY`（一次性进程里结构上答不了：答出来是假话 / 活不过进程）·
/// `UI_ONLY`（答得了，只是除了界面没人用得着 —— 这是关于调用方是谁的产品判断，第二个前端来了就该重看）· 测试里的 `NOT_ON_CLI`
/// （`Run::Builtin` 与占了 ccm 的词）。混成一张表时，后一种理由被读成前一种，第二个前端就够不着（审计 §4）。
#[test]
fn the_withheld_tables_do_not_overlap_and_each_holds_its_own_kind() {
    fn names(t: &[(&'static str, &'static str)]) -> Vec<&'static str> {
        t.iter().map(|(n, _)| *n).collect()
    }
    let (s, u, n) = (
        names(STREAM_ONLY_WHY),
        names(UI_ONLY_WHY),
        names(NOT_ON_CLI),
    );
    // 理由表与生产那两张表两向相等（一条都不多、一条都不少）。
    let sorted = |mut v: Vec<&'static str>| {
        v.sort_unstable();
        v
    };
    assert_eq!(
        sorted(s.clone()),
        sorted(STREAM_ONLY.to_vec()),
        "`STREAM_ONLY_WHY` 与生产的 `STREAM_ONLY` 对不上"
    );
    assert_eq!(
        sorted(u.clone()),
        sorted(UI_ONLY.to_vec()),
        "`UI_ONLY_WHY` 与生产的 `UI_ONLY` 对不上"
    );
    for (a, an, b, bn) in [
        (&s, "STREAM_ONLY", &u, "UI_ONLY"),
        (&s, "STREAM_ONLY", &n, "NOT_ON_CLI"),
        (&u, "UI_ONLY", &n, "NOT_ON_CLI"),
    ] {
        let both: Vec<_> = a.iter().filter(|x| b.contains(x)).collect();
        assert!(
            both.is_empty(),
            "{an} 与 {bn} 都登记了 {both:?} —— 一条命令只有一种理由"
        );
    }
    // 两张生产表收的都是**跑得起来**的命令（`Builtin` 那一种本来就跑不了，理由住 `NOT_ON_CLI`）。
    for name in s.iter().chain(u.iter()) {
        let spec = REGISTRY
            .iter()
            .find(|x| x.name == *name)
            .unwrap_or_else(|| panic!("{name} 不在 REGISTRY 里 —— 过期的登记"));
        assert!(
            !matches!(spec.run, Run::Builtin),
            "{name} 是 `Run::Builtin`，理由该住 `NOT_ON_CLI`"
        );
    }
    // `NOT_ON_CLI` 只收那两种。
    for name in &n {
        if let Some(spec) = REGISTRY.iter().find(|x| x.name == *name) {
            assert!(
                matches!(spec.run, Run::Builtin) || crate::control::ccm::argv::is_ccm_word(&flag_of(name)),
                "`NOT_ON_CLI` 里的 {name} 跑得起来、也不是 ccm 的词 —— 它的理由该住 `STREAM_ONLY` 或 `UI_ONLY`"
            );
        }
    }
    // 「只有界面用得着」那一张里，理由**不许**是「命令行那一侧直接敲 `ccm` 就是它」—— 那挡的是第二个入口，挡不住第二个前端。
    for (name, why) in UI_ONLY_WHY {
        assert!(
            !why.contains("直接敲"),
            "UI_ONLY 里 {name} 的理由是「直接敲 ccm」那一种 —— 那不是理由"
        );
    }
}

/// ★ 审计 §4 复核后放出的那几条（理由原是「只有界面用得着」，一次性进程里答得出真话）今天在 CLI 面上。
#[test]
fn the_commands_released_from_ui_only_are_on_the_cli() {
    for name in [
        "session-new",
        "session-new-facts",
        "session-new-dir",
        "sessions-stop",
        "sessions-start",
        "sessions-where",
        "terminal-name-mint",
        "terminal-ssh",
        "history-search-merge",
    ] {
        assert!(spec_for(&flag_of(name)).is_some(), "{name} 没上 CLI 面");
    }
}

/// 失败信封只有一种（`{code, message, detail, data?}`）：调用方交来的下层原话（`Said.raw`）进复制详情的「原话」那一项，不上句子、不另立一格。
#[test]
fn the_raw_words_go_into_the_detail_not_a_field_of_their_own() {
    let f = failed_of(
        "resident-ensure",
        "spawn_failed",
        copy_core::said::Said::with_raw("那一句".to_string(), "Permission denied (os error 13)"),
    );
    let v = serde_json::to_value(&f).unwrap();
    assert_eq!(v["message"], "那一句", "{v}");
    assert!(v.get("raw").is_none(), "信封又多了一格 raw：{v}");
    let raw_label = copy_core::copy_text("detail.label.raw", &[]);
    assert!(
        f.detail
            .contains(&format!("{raw_label}：Permission denied (os error 13)")),
        "原话没进复制详情：{}",
        f.detail
    );
    let without = failed_of(
        "resident-ensure",
        "no_home",
        copy_core::said::Said::from("那一句".to_string()),
    );
    assert!(
        !without.detail.contains(&format!("{raw_label}：")),
        "没有原话也写了原话那一项：{}",
        without.detail
    );
}

/// 帧面跑一条命令，取它那一帧应答（JSON）。`within_ms` 照帧面信封那一格原样带。
fn frame_reply(cmd: &str, args: &serde_json::Value, within_ms: Option<u64>) -> serde_json::Value {
    let mut req = serde_json::json!({"id": "x", "cmd": cmd, "args": args});
    if let Some(ms) = within_ms {
        req["within_ms"] = ms.into();
    }
    let line = format!("{req}\n");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let (tx, mut rx) =
            tokio::sync::mpsc::channel(crate::stream::inbound::REPLY_CHANNEL_CAPACITY);
        crate::stream::inbound::spawn(
            std::io::Cursor::new(line.into_bytes()),
            tx,
            crate::stream::wire::HelloFlushed::for_tests(),
        )
        .await
        .unwrap();
        let mut got = None;
        while let Some(f) = rx.recv().await {
            let v = serde_json::to_value(&f).unwrap();
            if v["kind"] == "reply" && v["id"] == "x" {
                got = Some(v);
            }
        }
        got.expect("帧面没回应答")
    })
}

/// CLI 面跑同一条（入参走 argv 口），取 stderr 那一行信封。
fn cli_failure(cmd: &str, args: &serde_json::Value, extra: &[&str]) -> (i32, String) {
    let mut argv = vec![
        flag_of(cmd),
        crate::ARGS_B64_FLAG.to_string(),
        crate::stream::wire::b64_encode(args.to_string().as_bytes()),
    ];
    argv.extend(extra.iter().map(|s| s.to_string()));
    let (rc, out, err) = run_cli(&argv, std::io::empty());
    assert!(out.is_empty(), "{cmd} 失败时 stdout 该是空的：{out}");
    (rc, err)
}

/// 复制详情首行是出错那一刻的时刻（两个面各自取钟）：比之前抹成同一个值，别的行逐字比。
fn clockless(detail: &str) -> String {
    let at = format!("{}：", copy_core::copy_text("detail.label.at", &[]));
    detail
        .lines()
        .map(|l| if l.starts_with(&at) { at.as_str() } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 失败那一份里给人和程序读的四格（`detail` 抹掉时刻）。
fn failure_face(v: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "code": v["code"],
        "message": v["message"],
        "detail": v["detail"].as_str().map(clockless),
        "data": v.get("data").cloned().unwrap_or(serde_json::Value::Null),
    })
}

/// ★ **同一个失败，帧面与 CLI 面说的一字不差**：`code` · `message` · `detail`（复制详情）· `data` 出自同一份失败载体。
/// 挑的三形：有「码 → 句」表的（句子换成表里那句、处理器原话进详情）· 带系统原话的 · 普通的。
#[test]
fn a_failure_reads_the_same_on_the_frame_face_and_the_cli_face() {
    let base = std::env::temp_dir().join(format!("ccm-cliface-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let file = base.join("f.txt");
    std::fs::write(&file, b"x").unwrap();
    let cases = [
        ("kill", serde_json::json!({})),
        (
            "files-ls",
            serde_json::json!({"path": file.to_str().unwrap()}),
        ),
        (
            "history-facts",
            serde_json::json!({"path": "/nonexistent/cliface/a.jsonl"}),
        ),
    ];
    for (cmd, args) in &cases {
        let frame = frame_reply(cmd, args, None);
        assert_eq!(frame["ok"], false, "{cmd} 在帧面没失败：{frame}");
        let (rc, err) = cli_failure(cmd, args, &[]);
        assert_eq!(rc, 2, "{cmd}：{err}");
        let cli: serde_json::Value = serde_json::from_str(err.trim())
            .unwrap_or_else(|e| panic!("{cmd} 的 stderr 不是一行 JSON（{e}）：{err:?}"));
        assert!(
            cli["detail"].as_str().is_some_and(|d| !d.trim().is_empty()),
            "{cmd} 的 CLI 信封没带复制详情：{cli}"
        );
        assert_eq!(
            failure_face(&cli),
            failure_face(&frame),
            "{cmd}：CLI 面与帧面对同一个失败说得不一样"
        );
    }
    std::fs::remove_dir_all(&base).ok();
}

/// CLI 入口自己拒的（用法错 · 入参坏）也带复制详情：「命令」那一项是这条子命令的名字，「码」那一项是那个码。
#[test]
fn a_refusal_at_the_cli_door_carries_a_detail_too() {
    let (rc, _out, err) = run_cli(&strs(&["--ping", "--args-b64", "e30="]), std::io::empty());
    assert_eq!(rc, 2);
    let v: serde_json::Value = serde_json::from_str(err.trim()).unwrap();
    assert_eq!(v["code"], "bad_args", "{v}");
    let detail = v["detail"]
        .as_str()
        .unwrap_or_else(|| panic!("没带 detail：{v}"));
    let label = |k: &str| copy_core::copy_text(k, &[]);
    assert!(
        detail.contains(&format!("{}：ping", label("detail.label.command"))),
        "{detail}"
    );
    assert!(
        detail.contains(&format!("{}：bad_args", label("detail.label.code"))),
        "{detail}"
    );
}

/// `--text`（给人看那一形）的失败：stderr 是那一句 ＋ 下面原样接复制详情（同界面［复制详情］复制出去的那一段），不是 JSON；退出码照旧 2。
#[test]
fn the_text_form_of_a_failure_is_the_sentence_over_its_detail() {
    let (rc, _out, err) = run_cli(
        &strs(&["--quota-read", "--args-b64", "e30="]),
        std::io::empty(),
    );
    let json: serde_json::Value = serde_json::from_str(err.trim()).unwrap();
    let (rc_t, out_t, err_t) = run_cli(
        &strs(&["--quota-read", "--text", "--args-b64", "e30="]),
        std::io::empty(),
    );
    assert_eq!((rc, rc_t), (2, 2));
    assert!(out_t.is_empty(), "{out_t}");
    assert!(
        serde_json::from_str::<serde_json::Value>(err_t.trim()).is_err(),
        "--text 的失败不该还是 JSON：{err_t}"
    );
    let (said, detail) = err_t
        .trim_end()
        .split_once('\n')
        .unwrap_or_else(|| panic!("没有详情那几行：{err_t:?}"));
    assert_eq!(said, json["message"].as_str().unwrap());
    assert_eq!(
        clockless(detail),
        clockless(json["detail"].as_str().unwrap())
    );
}

/// ★ CLI 面的 `--within-ms` 与帧面信封的 `within_ms` 同名同义：同一个期限、同一条命令，到点回的是同一份失败（码 · 句 · 详情）。
/// 1 ms 不够余量 ⇒ 总期限此刻就到 ⇒ 第一发子进程不起、直接回超时（两个面都不碰 tmux）。
#[test]
fn the_cli_deadline_times_out_exactly_like_the_frame_one() {
    let args = serde_json::json!({});
    let frame = frame_reply("terminals-list", &args, Some(1));
    assert_eq!(
        frame["code"],
        crate::platform::child::TIMED_OUT,
        "帧面没按期限超时：{frame}"
    );
    let (rc, err) = cli_failure("terminals-list", &args, &[crate::WITHIN_MS_FLAG, "1"]);
    assert_eq!(rc, 2, "{err}");
    let cli: serde_json::Value = serde_json::from_str(err.trim())
        .unwrap_or_else(|e| panic!("stderr 不是一行 JSON（{e}）：{err:?}"));
    assert_eq!(failure_face(&cli), failure_face(&frame));
}

/// `--within-ms` 的用法：缺值 · 给两次 ⇒ `bad_args`（同 `--args-b64`）；值不是正整数 ⇒ 当没带（同帧面那一格的宽读），命令照常答。
#[test]
fn the_cli_deadline_option_refuses_like_the_other_options_and_reads_like_the_frame() {
    for argv in [
        vec!["--ping", crate::WITHIN_MS_FLAG],
        vec![
            "--ping",
            crate::WITHIN_MS_FLAG,
            "5000",
            crate::WITHIN_MS_FLAG,
            "5000",
        ],
    ] {
        let (rc, _out, err) = run_cli(&strs(&argv), std::io::empty());
        assert_eq!(
            (rc, err_code(&err)),
            (2, "bad_args".to_string()),
            "{argv:?}"
        );
    }
    for v in ["0", "-1", "1.5", "soon", "5000"] {
        let (rc, out, err) = run_cli(
            &strs(&["--ping", crate::WITHIN_MS_FLAG, v]),
            std::io::empty(),
        );
        assert_eq!(rc, 0, "{v}：{err}");
        assert!(!out.trim().is_empty(), "{v}");
    }
}
