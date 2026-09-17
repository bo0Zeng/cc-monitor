//! E77：**加了子命令就必须 bump `BUILD_ID`** —— 把这条从「记性」变成机检。
//!
//! # 为什么需要它
//!
//! monitor 判「远端那台的 daemon 该不该换」只有一条判据：
//! `reported_build_id != EXPECTED_DAEMON_BUILD_ID`（`ssh_source.rs`，值由 `build.rs`
//! 从本文件抠出）。**不 bump ⇒ 已部署的旧 daemon 报同一个 id ⇒ 不判 stale ⇒ 不自动重装
//! ⇒ 整轮改动在已部署的远端休眠。**
//!
//! 这一课在 `main.rs` 的版本谱系里被写过两遍（p1r 段、p1t 段），`doc/INVARIANTS.md` §41.5
//! 又写了一遍 —— **写了三遍，2026-08-01 的 Phase G 审计仍然逮到第三次漏做**（G2 加了
//! `--fork-session` 却没 bump）。⇒ 靠散文提醒是无效的。
//!
//! # 判据：子命令集的指纹 ↔ BUILD_ID 的历史表
//!
//! `SUBCOMMAND_HISTORY` 是一张**追加**的表：每行 = `(BUILD_ID, 那一版的子命令集指纹)`。
//! 本护栏断言两件事：
//!
//! 1. **当前算出来的指纹必须在表里**（不是「等于最后一行」——见下）；
//! 2. 表里**不许有重复的 BUILD_ID**。
//!
//! 于是「加一个子命令」⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行；
//! 而用**当前（未 bump 的）id** 追加会撞上第 2 条 ⇒ **只剩「bump + 追加」这一条路**，
//! 也就是本来就该做的那件事。
//!
//! **为什么不是「等于最后一行」**（头一版是那么写的）：那会让**因为别的原因 bump BUILD_ID**
//!（如 p1v 只加了个 wire 字段、子命令一个没动）也被逼着改这张表 ——
//! 而「改表」恰恰是本护栏最不想诱导的动作。
//!
//! # 这条护栏**挡不住**什么（说清楚，别让人以为它是证明）
//!
//! 1. **有人可以就地改最后一行的指纹而不 bump。** 表在同一个文件里，护栏读不到 git 历史。
//!    能做的只有把「正确动作」变成最省事的那个，并让错误信息把代价说清楚。
//!    真要堵死得靠 CI 比对 `git show HEAD~1` —— 那需要 CI 里有完整历史（`fetch-depth: 0`），
//!    成本高于收益，**如实登记为未做**。
//! 2. **「子命令集没变但行为变了」它不管**（p1r 那次就是：删轮询、加事件，argv 表面没动）。
//!    那类仍然要靠人判断。本护栏只覆盖 p1t / G2 这一类「加/改子命令」——
//!    而那恰好是本仓栽过的两次里的两次。
//!
//! # 🔴 `K-R70`（09-12）：本模块从此守**两件事**，别把它读成只守 bump
//!
//! 上面全篇讲的是「**该不该** bump」。`K-R68` 摸底逮出来的是另一格：
//! **拿起一份后端二进制，产品判断不了它是不是我们以为的那一份** ——
//! 三个载体的身份全靠旁边那个 `.build_id` 文本文件，而它是从同一处源码常量抠的标签
//! ⇒ 三份恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
//! 本模块下半段（`the_build_stamp_is_byte_scannable_in_this_very_binary` 起）守的是
//! **身份长在二进制自己身上**这条性质。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空。

#[cfg(test)]
mod tests {
    /// `(BUILD_ID, 子命令集指纹)`，**只追加不修改**。
    ///
    /// 指纹 = 排序去重后的子命令名用 `\n` 连起来（见 `subcommand_fingerprint`）——
    /// 刻意用**明文**而不是哈希：出错时的诊断信息直接就是「多了/少了哪个」，
    /// 而哈希只会告诉你「不一样」。
    const SUBCOMMAND_HISTORY: &[(&str, &str)] = &[
        // p1t 及之前：--fork-session 还没有。留一行历史，让「加了一个」这件事在 diff 里看得见。
        (
            "p1t-removal-cause",
            "--account-trust\n--account-trust-zero\n--list-accounts\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage",
        ),
        // p1u：G2/G6 加 --fork-session（daemon 第一次有写盘能力）。
        // ⚠ 这一行（及上一行）是**指纹只覆盖 CLI 那一面**时代的记录，没有 `#channel` 段。
        //   扩面之后它们永远不会再等于当前指纹 —— 那是对的：它们记的就是「那时候只有这一面」。
        (
            "p1u-fork-session",
            "--account-trust\n--account-trust-zero\n--fork-session\n--list-accounts\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage",
        ),
        // ★ p1w：**指纹第一次覆盖两个命令面**〔audit-0805 F02〕。
        //
        // CLI 那面与 p1u **一字未改**；变的是把 `inbound::COMMANDS` 纳进来了。
        // ⚠ **这一行不是「新加了 5 条通道命令」的记录** —— 那 5 条是 08-02/08-04 加的，
        //   当时 `BUILD_ID` 是 `p1v-attachable` 且**没有 bump**，而指纹看不见它们。
        //   本行记的是「从此以后看得见了」，同时把那笔欠账结清：p1w 之后报 p1v 的远端
        //   会被判 stale 并重装 —— 这正是 08-02 起就该发生、却因为指纹半瞎而没发生的事。
        // ⚠ **没有 p1v 行**是刻意的：p1v 那一版的真实通道面是**空集**，
        //   而我们没有一个能证明「某个 p1v 二进制到底带不带通道面」的可靠办法
        //   （本机那份实测是空的，但那只是本机那一份）。造一行假历史比缺一行更坏。
        (
        // ★ U-2（08-06）：**指纹的 CLI 那半换了抽取口径 —— 子命令集本身一条没动。**
        //
        // 旧口径从 `main.rs` 里 scrape `Some("--`，只抠到 **9** 条；
        // 新口径直接读权威登记表 `crate::SUBCOMMANDS`，是 **14** 条。
        // 差的 5 条（`--list-projects` / `--list-sessions` / `--read-session{,-from-offset,-tail}`）
        // 分别是 07-03 与 08-02 进表的，**都早于 p1w（08-05）** ——
        // 即：报 p1w/p1x 的二进制本来就带着它们，本行改的是**量法**，不是历史。
        // ⇒ 因此**不 bump `BUILD_ID`**：没有「已部署的远端缺这些能力」这笔欠账，
        //   而无谓的 bump 会让所有远端被判 stale、白重装一轮（那是 F02 那次才该付的代价）。
            "p1w-inbound-in-fingerprint",
            "--account-trust\n--account-trust-zero\n--fork-session\n--list-accounts\n--list-projects\n--list-sessions\n--read-session\n--read-session-from-offset\n--read-session-tail\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p1y（P4d，08-12）：**控制面开了第二个入口 —— 一次性 CLI。**
        //
        // 新增 4 条：`--launch` / `--kill` / `--ping`（帧面已有的命令换个调用法）
        // 与 `--daemon-probe`（能力探测口，回 `{proto, buildId, commands}`）。
        // ⚠ 这 4 条**都不是新语义** —— 实现仍是 `inbound::REGISTRY` 上那几条 `run`，
        //   CLI 面只是不经 SSH 帧地调它们（`control/cli_control.rs`）。
        // ⇒ 但**必须 bump**：已部署的旧 daemon 没有这几个口，而 skill 会按
        //   `--daemon-probe` 的回答决定走不走新路 —— 不 bump 就不判 stale、不重装，
        //   探测口在旧机器上直接 exit 2，整轮能力静默休眠（`branch-anywhere` 那次的形状）。
        (
            "p1y-cli-control-face",
            "--account-trust\n--account-trust-zero\n--daemon-probe\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p1z（P7c-1，08-12）：新增 `--list-subagents` —— 列一个父会话的 subagent 候选。
        //
        // ⚠ **必须 bump**：远端会话的 subagent 展开就靠这一条；旧 daemon 没有它，
        //   而 monitor 判 stale 只看 build_id ⇒ 不 bump 就不重装，功能在已部署的机器上休眠。
        // ★ 它**只列不挑**：description 匹配与时间戳排序留在 monitor（定框 C1：别长第二套语义）。
        (
            "p1z-list-subagents",
            "--account-trust\n--account-trust-zero\n--daemon-probe\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2b（P4f，08-13）：帧面新增 `bus-list` / `bus-send` —— cc-bus 的基础命令。
        //
        // ⚠ **必须 bump**：集成方（skill / monitor）判「这台机器有没有这条能力」看的是
        //   `hello` 的 `commands`，而它随 daemon 二进制走。旧 daemon 报同一个 build_id
        //   ⇒ 不判 stale ⇒ 不重装 ⇒ 这两条命令在已部署的机器上休眠。
        // ★ CLI 面这一半**也变了**（`--bus-list` / `--bus-send` 进了 `SUBCOMMANDS`）。
        //   ⚠ 我第一版只加了 `ch:` 那两条就以为完事，还给自己写了句「CLI 指纹看不见它，
        //   是取法的射程」—— **那句话是错的**：CLI 指纹取自 `SUBCOMMANDS`，而 `SUBCOMMANDS`
        //   正是 `is_query_mode` 的闸门。指纹没变**恰恰是**「这条命令的 CLI 面根本没接上」
        //   的症状，而我把症状读成了取法的局限。实测才逮到（daemon 当场进了流模式）。
        //   ⇒ 现在那条纪律由 `cli_control::tests::every_cli_exposed_command_is_in_the_query_mode_gate`
        //   钉住：漏加就红，不再靠人记得。
        (
            "p2b-cc-bus-basics",
            "--account-trust\n--account-trust-zero\n--bus-list\n--bus-send\n--daemon-probe\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2c（P4f 续，08-13）：新增 `bus-kill` —— 收掉一个总线成员（转调 `cc-kill`）。
        //
        // ⚠ **必须 bump**：monitor/skill 判「这台有没有这条能力」看的是 `hello.commands`，
        //   它随二进制走；不 bump ⇒ 旧 daemon 报同一个 id ⇒ 不判 stale ⇒ 这条在远端休眠。
        // ★ 与 `kill` 的分工写在 IPC-PROTOCOL：那条只杀 tmux 会话（§34 三道门，证据弱）；
        //   这条还要清名册/台账/状态，且门在 `cc-kill` 自己那儿（证据强：登记的 pane 根进程 pid）。
        (
            "p2c-bus-kill",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--daemon-probe\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2d（`K-H1`，08-25）：新增 `--relay` —— HTTP 中转（搬字节那半）。
        //
        // ⚠ **必须 bump**：中转是**新的进程形态**（常驻、只听回环、按路径前缀分流）。
        //   已部署的旧 daemon 根本没有这个口，而 monitor/skill 判 stale 只看 build_id
        //   ⇒ 不 bump 就不重装，整件能力在已部署的远端休眠（p1r / p1t / G2 那三次的形状）。
        // ★ 这一半是**源码半**。`main.rs::BUILD_ID` 那段头注逐字警告过另一半：
        //   「只 bump 源码不 re-embed = 源码 build_id 与内嵌清单不一致的**半 bump**，更糟」。
        //   本护栏对「半 bump」是瞎的 ⇒ re-embed 归发版那一拍（CI 交叉编译），本轮没做，
        //   已在件文件的「没做到」里点名。
        (
            "p2d-relay",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--daemon-probe\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2e（`K-P6b`，09-06）：新增 `--dial` —— 把 **daemon 那条长连接流**的 SSH 握手
        //   搬进一个由界面起的子进程（候选 E 的字节代理）。
        //
        // ⚠ **必须 bump**：又一个**新的进程形态**（常驻、一条管子进一条管子出、不进
        //   `listen` 那条常驻路）。已部署的旧 daemon 没有这条臂，而 monitor 判 stale
        //   只看 build_id ⇒ 不 bump 就不重装 ⇒ 整件能力在已部署的远端休眠。
        //   —— 与 p2d 那条逐字同一个理由，这已经是本表第五次写它了。
        // 🔴 **这一行不许被读成「拨号搬出去了」**：`connect_session` 的生产调用点
        //   **7 处 / 3 份**，本件只覆盖 daemon 长连接流那 **1** 处；
        //   SFTP · 端口转发 · 跳板 · 其余 exec 路径**界面仍然自己拨**。
        // ★ 同 p2d：这一半是**源码半**，`main.rs::BUILD_ID` 头注警告的那个「半 bump」
        //   （只 bump 源码不 re-embed）归发版那一拍（CI 交叉编译），本轮没做。
        //   ⚠ 而且本轮**连 musl 交叉编译都没验**（沙箱门禁不做交叉编译）——
        //   理由与读数住 `Cargo.toml` 里 `russh` 那条依赖的块头注。
        (
            "p2e-dial",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--daemon-probe\n--dial\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2g（`K-R86`，09-13）：新增 `--capture-pane` —— **一条只读的一次性抓屏原语**。
        //
        // ⚠ **没有 p2f 行**，而这**不是漏**：`p2f-build-stamp`（`K-R70` 09-12）动的是
        //   「这份二进制说不说得出自己是谁」，**子命令集一条没动** ⇒ 它的指纹与 `p2e-dial`
        //   逐字相同，按本表的口径（「当前指纹在不在表里」，不是「等于最后一行」）**不该追加**。
        //   头注那段逐字写着为什么不是「等于最后一行」：因为别的原因 bump 也被逼着改这张表，
        //   而改表恰恰是本护栏最不想诱导的动作。
        //
        // ⚠ **必须 bump**：新增子命令 ⇒ 已部署的旧 daemon 上 `--capture-pane` 落进
        //   `unknown argument` + exit 2，而调用方判 stale 只看 build_id
        //   ⇒ 不 bump 就不重装，这条能力在已部署的远端休眠（p1r / p1t / G2 / p2d / p2e 同形）。
        // ★ 同 p2d / p2e 那条如实登记：这一半是**源码半**，re-embed（CI 交叉编译）归发版那一拍，
        //   本轮**没做**；本护栏对「半 bump」是瞎的。
        (
            "p2g-capture-pane",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--capture-pane\n--daemon-probe\n--dial\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2h（`K-R87`，09-13）：新增 `--oneshot-session` —— **带看门狗的一次性会话**。
        //
        // ⚠ **必须 bump**：又一条新增的子命令 ⇒ 已部署的旧 daemon 上它落进
        //   `unknown argument` + exit 2，而调用方判 stale 只看 build_id
        //   ⇒ 不 bump 就不重装，这条能力在已部署的远端休眠
        //   （p1r / p1t / G2 / p2d / p2e / p2g 同形，这已经是本表第七次写这个理由）。
        // ★ 通道面（`inbound::COMMANDS`）**一条没动**：本件只出 CLI 那一面，
        //   帧面要不要有它是另一件事（今天没有需求，不先造）。
        // ★ 同 p2d / p2e / p2g 那条如实登记：这一半是**源码半**，re-embed 归发版那一拍，
        //   本轮**没做**；本护栏对「半 bump」是瞎的。
        (
            "p2h-oneshot-session",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--capture-pane\n--daemon-probe\n--dial\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--oneshot-session\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:kill\nch:launch\nch:ping\nch:resolve",
        ),
        // ★ p2i（`K-R104`，09-13）：**CLI 那一面一个字没动，变的全在通道面** ——
        //   `ch:capture-pane` ＋ `ch:oneshot-session`。这是本表**第一次**只有 `#channel`
        //   那半变了（p1w 那次是「第一次把通道面纳进指纹」，形状不同，别读成同一件事）。
        //
        // ⚠ **必须 bump**，而这一次的理由比前七次更硬：
        //   前七次是「新子命令在旧 daemon 上落进 `unknown argument` + exit 2」；
        //   这一次是**帧面**：旧 daemon 的 `hello.commands` 里没有这两条 ⇒ monitor 的
        //   `InboundClient::accepts` 当场判 `CallError::Unsupported`、**一个字节都不发**
        //   （`bus-send` 那条是现成先例）。⇒ 用量探针在已部署的旧远端上**整条不可用**，
        //   而调用方判 stale 只看 build_id ⇒ 不 bump 就不重装。
        //   ★ 那不是静默失败：`Unsupported` 的 `Display` 逐字说「多半是旧版本，请重装该机器的 daemon」。
        // ★ 同 p2d / p2e / p2g / p2h 那条如实登记：这一半是**源码半**，
        //   re-embed（CI 交叉编译）归发版那一拍，本轮**没做**；本护栏对「半 bump」是瞎的。
        (
            "p2i-frame-tmux-primitives",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--capture-pane\n--daemon-probe\n--dial\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--oneshot-session\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:cancel\nch:capture-pane\nch:kill\nch:launch\nch:oneshot-session\nch:ping\nch:resolve",
        ),
        // ★ p2j（`K-R113`，09-13）：新增 `bus-state` —— **两个命令面同拍都动**
        //   （`--bus-state` 进 `SUBCOMMANDS` 26 → 27；`ch:bus-state` 进 `inbound::COMMANDS`
        //   10 → 11）。这是本表**第一次**两半一起变：p2h 只动 CLI 那半、p2i 只动通道那半。
        //
        // ⚠ **必须 bump**，而这一次前八次的两个失效形状**同时**成立：
        //   ① CLI 面 —— 旧 daemon 上 `--bus-state` 落进 `unknown argument` + exit 2；
        //   ② 帧面 —— 旧 daemon 的 `hello.commands` 里没有它 ⇒ monitor 的
        //      `InboundClient::accepts` 当场判 `CallError::Unsupported`、一个字节都不发。
        //   两条路都止于「调用方判 stale 只看 build_id」⇒ 不 bump 就不重装，能力在远端休眠。
        // ★ 同 p2d / p2e / p2g / p2h / p2i 那条如实登记：这一半是**源码半**，
        //   re-embed（CI 交叉编译）归发版那一拍，本轮**没做**；本护栏对「半 bump」是瞎的。
        (
            "p2j-bus-state",
            "--account-trust\n--account-trust-zero\n--bus-kill\n--bus-list\n--bus-send\n--bus-state\n--capture-pane\n--daemon-probe\n--dial\n--fork-session\n--kill\n--launch\n--list-accounts\n--list-projects\n--list-sessions\n--list-subagents\n--oneshot-session\n--ping\n--read-session\n--read-session-from-offset\n--read-session-tail\n--relay\n--resolve\n--search\n--session-accounts\n--tmux-notify\n--usage\n#channel\nch:bus-kill\nch:bus-list\nch:bus-send\nch:bus-state\nch:cancel\nch:capture-pane\nch:kill\nch:launch\nch:oneshot-session\nch:ping\nch:resolve",
        ),
    ];

    use crate::guard_support::{assert_no_test_code, production_code};

    /// 从 `main.rs` 的生产段抠出 `Some("--x")` 形态的子命令，排序去重。
    ///
    /// **剥测试段与剥注释两样都要**：`main.rs` 的散文里成篇地提到这些字面量（版本谱系那一大段
    /// 就是），不剥的话「注释里写了它」也能把指纹喂饱 —— 那正是安慰剂。
    ///
    /// # U-1（2026-08-01）：剥法换成 `guard_support`，指纹的来源变了
    ///
    /// 旧剥法锚 `"\n#[cfg(test)]\nmod tests"`，而 `main.rs` 的测试模块叫 `mod stream_flag_tests`
    /// ⇒ 匹配不上 ⇒ 整个文件被当生产段。**于是本护栏数到的九个子命令，一直来自
    /// `stream_flag_tests` 里那份副本，不是 `:275-291` 的真 dispatch。**
    /// （两个 bug 凑出一个看起来正确的结果：不剥 ⇒ 真 dispatch 也在里面 ⇒ 一直绿。）
    /// 换成逐个剥测试模块之后，指纹来自真 dispatch，**集合实测不变**（同为那九个）。
    /// ★★★ **两个命令面都要进指纹**〔audit-0805 F02〕。
    ///
    /// # 它此前只覆盖一半，而漏掉的那半从 0 长到了 5
    ///
    /// 本函数原来只抠 `main.rs` 里的 `Some("--`，也就是**一次性子命令**那一面。
    /// 而 daemon 还有第二个命令面：[`crate::inbound::COMMANDS`]（常驻通道命令）。
    /// 实测（`audit-0805` 的只读核实）：
    ///
    /// - `BUILD_ID` 从 `4617f34`（07-31，`p1v-attachable`）之后**再没变过**；
    /// - 而入方向从**零条**长到 **5 条**：`cancel`/`ping`/`resolve`（`8a13ba9`+`a361ff9`，08-02）、
    ///   `kill`（`899538a`，08-04）。`git merge-base --is-ancestor` 三条全 YES。
    ///
    /// ⇒ **加了整整一个命令面，一次 bump 都没被逼出来**，因为指纹结构上看不见它。
    /// 而 `sftp.rs::deploy_decision` 判「远端要不要换 daemon」时，**版本那一维**的唯一判据就是 build_id 字符串
    /// （〔K-W4 09-04〕daemon 那条部署路今天走 `deploy_decision_at`，另加了「落点文件在不在」这一维；
    /// 本句的实质警告不变：**stale 但文件在**的 daemon 仍只凭 build_id 判换不换）
    /// ⇒ 已部署的旧 daemon 报同一个 id ⇒ 判 `Skip` ⇒ **整个控制面在远端静默不可用**。
    ///
    /// # 这不只是结构缺陷，本机实测到了它的后果
    ///
    /// 本机 `embedded-daemons/cc-monitor-remote-x86_64`（08-01 构建）里
    /// **找不到入方向那一面会发射的任何一个错误码**（`not_cancellable` / `unknown_command` /
    /// `duplicate_id` / `handler_panicked` / `wrong_owner` / `too_many_windows`，`.rodata` 全 0 命中），
    /// 而它的清单写着 `p1v-attachable` = 期望值。
    /// ⚠ 那次探测**带对照组才算数**：同一批里「**会被发射**的串」9/9 全命中
    /// （`session_removed`/`overflow`/`hello`/`capabilities`…），
    /// 而「只被比较、从不发射」的串命中不稳（`--list-projects` 就是 0）——
    /// 上面那批错误码属前者（它们是写进应答 JSON 的值），所以 0 命中是可信的。
    ///
    /// # 为什么通道那半直接引用 const，而不照 CLI 那半去 scrape 文本
    ///
    /// CLI 那面只能 scrape（`main.rs` 的 dispatch 是 `match` 字面量，没有集中的 const）。
    /// 通道那面**有**单一真相源常量 ⇒ 直接引用它更强：**没有抽取器可坏**，
    /// 改名/增删会自动反映到指纹里。两半的取法不同是刻意的，不是遗漏。
    fn subcommand_fingerprint() -> String {
        let prod = production_code(include_str!("../../src/backend/main.rs"));
        // 反向自检：剥完还得剩下真代码，否则下面数出来的空集会「恰好等于」某个错误期望。
        assert!(
            prod.len() > 3_000,
            "剥完 main 生产段只剩 {} 字节 —— 剥法坏了，本护栏此刻是无效的",
            prod.len()
        );
        // 反向自检之二：**测试段真的剥掉了**。旧的 `len` 自检光靠剥注释就满足，
        // 与测试段有没有剥掉毫无关系 —— 那正是本护栏扫了几个月测试代码没人发现的原因。
        assert_no_test_code("build_id_guard/main.rs", &prod);
        // 〔audit-0805 08-06〕**改用权威登记表 `SUBCOMMANDS`，不再自己抠 `Some("--`。**
        //
        // 原来那种抠法有两个洞，都是实测出来的：
        // ① **只认一种写法**：把新子命令写成 `Some(x) if x == "--foo" =>`，指纹不变
        //    （本条全绿；逮住它的是隔壁 `argv_table_guard` 与 `protocol_doc_guard`）；
        // ② **更要命的一条**：那种抠法今天只抠到 **9** 条，而 `SUBCOMMANDS` 登记着 **14** ——
        //    `--list-projects` / `--list-sessions` / `--read-session*` 这几条**改了也不会让指纹变**。
        //    也就是说 E77 的正题（「加了子命令就必须 bump」）对其中 5 条**本来就不成立**。
        //
        // 〔audit-0805 08-06 复核〕**这个前提已被实测验过，不再只是断言**：
        // 把一条子命令的 token 字面量搬到 `DISPATCH_FILES` 之外（`wire.rs`）再分派，
        // `protocol_doc_guard::dispatch_registry_is_complete` **当场红** ——
        // 因为「哪些文件参与分派」那一层是**派生**的，不是手写清单。
        // ⚠ 边界：token 连 `"--` 字面量都不出现（`concat!` 拼）时全绿，已登记（见那边头注）。
        // `SUBCOMMANDS` 是那一面的权威登记表，而且**有人守着它别漏**：
        // `argv_table_guard::every_dispatched_token_is_classified` 要求每个被分派的 token
        // 都在 `SUBCOMMANDS` / `SUBCOMMAND_OPTIONS` / `STREAM_FLAGS` 三张表之一里。
        // ⇒ 新子命令必须先进那张表，才轮得到本条 —— 「漏一条」这件事从此有两道门。
        let mut subs: Vec<String> = crate::SUBCOMMANDS.iter().map(|s| s.to_string()).collect();
        subs.sort_unstable();
        subs.dedup();
        // 反向自检：登记表被掏瘪 ⇒ 指纹退化，本护栏静默失效。
        assert!(
            subs.len() >= 10,
            "从 `SUBCOMMANDS` 只拿到 {} 条子命令 —— 登记表被掏了（08-06 实测 14 条）：{subs:?}",
            subs.len()
        );
        // ── 第二个命令面：常驻通道命令（`inbound::COMMANDS` 是它的单一真相源）──────
        // 排序后写成 `ch:<名>`，与 `--x` 那一面在同一个字符串里但**不会混淆**。
        let mut chans: Vec<String> = crate::inbound::COMMANDS
            .iter()
            .map(|c| format!("ch:{c}"))
            .collect();
        chans.sort_unstable();
        // 反向自检：通道面空了 ⇒ 指纹会退化回「只覆盖一半」那个老样子而没人发现。
        assert!(
            !chans.is_empty(),
            "`inbound::COMMANDS` 抽到空集 —— 指纹会静默退回只覆盖 CLI 那一面"
        );
        let mut out = subs.join("\n");
        out.push_str("\n#channel\n");
        out.push_str(&chans.join("\n"));
        out
    }

    /// ★ E77 的正题。
    #[test]
    fn adding_a_subcommand_forces_a_build_id_bump() {
        let now = subcommand_fingerprint();
        // **判据是「当前指纹在不在表里」**，不是「等于最后一行」。
        //
        // 头一版写成「必须等于最后一行的指纹、且那行的 id 必须等于当前 BUILD_ID」——
        // 那会让**因为别的原因 bump BUILD_ID**（比如 p1v 只是加了个 wire 字段、子命令一个没动）
        // 也被逼着改这张表，而改表恰恰是本护栏最不想诱导的动作。
        //
        // 现在：加子命令 ⇒ 指纹是新的 ⇒ 不在表里 ⇒ 红。要弄绿只能追加一行；
        // 而**用当前（未 bump 的）id 追加会撞上 `history_has_no_duplicate_build_ids`** ⇒
        // 只剩「bump + 追加」这一条路。
        let (last_id, last_fp) = *SUBCOMMAND_HISTORY
            .last()
            .expect("SUBCOMMAND_HISTORY 不能为空");
        let _ = last_id;

        if !SUBCOMMAND_HISTORY.iter().any(|(_, fp)| *fp == now) {
            let old: Vec<&str> = last_fp.split('\n').collect();
            let new: Vec<&str> = now.split('\n').collect();
            let added: Vec<&&str> = new.iter().filter(|s| !old.contains(s)).collect();
            let removed: Vec<&&str> = old.iter().filter(|s| !new.contains(s)).collect();
            panic!(
                "daemon 的子命令集变了（+{added:?} / -{removed:?}），而 BUILD_ID 还是 `{}`。\n\
                 \n\
                 **别只改这张表**。monitor 判「远端该不该换 daemon」只有一条判据：\n\
                 `reported_build_id != EXPECTED_DAEMON_BUILD_ID`。不 bump ⇒ 已部署的旧 daemon\n\
                 报同一个 id ⇒ 不判 stale ⇒ 不自动重装 ⇒ **你这一轮的改动在已部署的远端休眠**，\n\
                 用户只会拿到「版本过旧」。本仓已经因为这个栽过三次（p1r / p1t / G2）。\n\
                 \n\
                 正确动作：① 在 main.rs 里 bump `BUILD_ID`（并在版本谱系里加一段说清改了什么）；\n\
                 ② 在 `SUBCOMMAND_HISTORY` **追加**一行 `(\"<新 id>\", \"{now}\")`。",
                super::super::BUILD_ID
            );
        }
    }

    /// 历史表不许有重复 id —— 重复意味着「同一个 id 对应过两套子命令集」，
    /// 那正是本护栏要防的那件事被绕过去了。
    #[test]
    fn history_has_no_duplicate_build_ids() {
        let mut ids: Vec<&str> = SUBCOMMAND_HISTORY.iter().map(|(id, _)| *id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "历史表里有重复的 BUILD_ID：{ids:?}");
        assert!(n >= 2, "至少要留一行历史，否则「加了一个」在 diff 里看不见");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 🔴 `K-R70`（09-12）：**后端说得出自己是谁** —— 身份从字节里读得出，不是从旁边抄
    // ═══════════════════════════════════════════════════════════════════════
    //
    // 上面那几条守的是「**该不该** bump」。本节守的是另一件事，`K-R68` 摸底才逮出来的：
    // **拿起一份后端二进制，产品今天没有办法判断它是不是我们以为的那一份。**
    // 三个载体（`native-daemon/` · `embedded-daemons/` · `binaries/`）的身份
    // 全靠旁边那个 `.build_id` 文本文件，而那个文件是**从同一处源码常量抠出来的标签**
    // ⇒ 三份恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
    //
    // 出路是两条**都长在二进制自己身上**的路，同源于 `BUILD_ID`：
    //   ① 跑得动它的人 —— `--ccm-probe` 的 `build=` 那一行；
    //   ② 跑不动它的人（交叉编译的 musl 二进制在 Windows 构建机上执行不了）——
    //      扫字节里的 `CC_MONITOR_BUILD_STAMP`。

    /// 把一段字节里的身份戳扫出来（**去重后的全部取值**）。
    ///
    /// ⚠ 空串与非法字符一律不收：debug 构建里 `BUILD_STAMP_OPEN` / `BUILD_STAMP_CLOSE`
    /// 这两个常量本身可能被并排放进 `.rodata`（本条第一版实测撞到过，扫出 `["", "p2e-dial"]`）
    /// —— 那是**两个字面量挨着**，不是一个戳。收它就会把「有几个身份」这个读数变假。
    fn build_ids_in(bytes: &[u8]) -> Vec<String> {
        let open = crate::BUILD_STAMP_OPEN.as_bytes();
        let close = crate::BUILD_STAMP_CLOSE.as_bytes();
        let mut out: Vec<String> = Vec::new();
        let mut i = 0usize;
        while i + open.len() <= bytes.len() {
            if &bytes[i..i + open.len()] == open {
                let rest = &bytes[i + open.len()..];
                let win = &rest[..rest.len().min(96)];
                if let Some(e) = win.windows(close.len()).position(|w| w == close) {
                    if let Ok(s) = std::str::from_utf8(&win[..e]) {
                        let ok = !s.is_empty()
                            && s.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_')
                            });
                        if ok {
                            out.push(s.to_string());
                        }
                    }
                }
                i += open.len();
            } else {
                i += 1;
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// 反向自检：**扫描器真的在扫**，不是恒答一个好看的答案。
    ///
    /// 一个「永远返回 `BUILD_ID`」的扫描器会让上面两条判据全绿而什么都没证。
    /// ⇒ 三格：没有戳 ⇒ 空 · 换个 id ⇒ 认出那个 id · 两个不同的戳 ⇒ 认出两个。
    #[test]
    fn the_stamp_scanner_actually_bites() {
        assert!(
            build_ids_in(b"nothing to see here").is_empty(),
            "没有戳的字节里也扫出东西 —— 扫描器在编答案"
        );
        let fake = format!(
            "{}zz-not-us{}",
            crate::BUILD_STAMP_OPEN,
            crate::BUILD_STAMP_CLOSE
        );
        assert_eq!(
            build_ids_in(fake.as_bytes()),
            vec!["zz-not-us".to_string()],
            "换一份字节，它必须报出**那一份**的身份，而不是本进程的"
        );
        let real = String::from_utf8_lossy(crate::CC_MONITOR_BUILD_STAMP.as_slice()).to_string();
        let two = format!("头{fake}中{real}尾");
        let mut want = vec![super::super::BUILD_ID.to_string(), "zz-not-us".to_string()];
        want.sort();
        assert_eq!(build_ids_in(two.as_bytes()), want, "两个戳要认出两个");
        // 界标挨着（debug 构建 `.rodata` 里真会出现的那一形）不许被当成一个戳。
        let adjacent = format!("{}{}", crate::BUILD_STAMP_OPEN, crate::BUILD_STAMP_CLOSE);
        assert!(
            build_ids_in(adjacent.as_bytes()).is_empty(),
            "两个界标挨着被读成了一个身份 —— 那会让「有几个身份」这个读数变假"
        );
    }

    /// ★★ `KR70D1` 的正题：**身份真的在这一份二进制的字节里** —— 扫这个进程自己。
    ///
    /// # 为什么扫 `current_exe()` 而不是扫一份夹具
    ///
    /// 本条要证的性质是「**编译之后它还在、而且连续**」。夹具证不了这一点：
    /// 夹具是我们自己拼的字节，编译器没插手。⇒ 唯一说得上话的被测对象是
    /// **一份真的编出来的二进制**，而手边最便宜的那一份就是**跑着本测试的这个壳**
    /// （它由 `main.rs` 编来，生产段的 `#[used] static` 一样在里面）。
    ///
    /// # 它证的与不证的
    ///
    /// ✅ 证：这一份二进制里**恰好一个**身份戳，值 = `BUILD_ID`。
    /// ⚠ 不证：**发版那份 release 二进制**也如此 —— 那是另一套 profile
    ///   （`lto` / `strip` / `opt-level`）。开工时在沙箱里用一份 `lto=true, strip=true`
    ///   的 release 壳单独打过一趟、同样扫出恰好一处，**但那是一次实验不是一条常驻判据**；
    ///   常驻的那条在 `src-tauri/build.rs`：内嵌任何一个载体之前都要从**它的字节**里
    ///   把身份扫出来，扫不出当场 panic ⇒ release 那一侧由发版路自己守。
    #[test]
    fn the_build_stamp_is_byte_scannable_in_this_very_binary() {
        let exe = std::env::current_exe().expect("拿不到本测试壳自己的路径");
        let bytes = std::fs::read(&exe).unwrap_or_else(|e| panic!("读不到 {}：{e}", exe.display()));
        assert!(
            bytes.len() > 100_000,
            "读到的字节只有 {} —— 读错文件了，本条会零命中地绿",
            bytes.len()
        );
        let ids = build_ids_in(&bytes);
        assert_eq!(
            ids,
            vec![super::super::BUILD_ID.to_string()],
            "从这一份二进制的字节里扫出来的身份是 {ids:?}，而 `BUILD_ID` 是 `{}`。\n\
             \n\
             · 扫到 **0 个** ⇒ 那个 `#[used] static CC_MONITOR_BUILD_STAMP` 被优化掉 / 被删了\n\
               ⇒ 「拿到一份二进制问得出它是谁」这条性质当场没了，而**旁边那个 `.build_id`\n\
               文件不算数**：它是从源码常量抄的标签，三个载体永远一致，一格证据都不提供\n\
               （`K-R68` 摸底 · `DECISIONS.md#R26` 裁定零）。\n\
             · 扫到 **多个** ⇒ 有第二处也在往二进制里写这个形状的串，身份不再唯一。\n\
             · 值不对 ⇒ 拼戳那条 `const fn` 与 `BUILD_ID` 脱钩了。",
            super::super::BUILD_ID
        );
    }

    /// ★★ `KR70D1` 的另一半：**跑得动它的人，直接问它**。
    ///
    /// `--ccm-probe` 那条握手此前只答得出 `version=`（**CLI 的契约版本**，`5`），
    /// 它答不出「你是哪一次构建」。本条钉住 `build=` 那一行**取自这个进程编进来的常量**。
    ///
    /// ⚠ 与 `control::ccm` 里那条形状判据**不重**：那边钉的是整份输出的**行序与行数**
    /// （外部契约），这边钉的是**这一行的值从哪来**（身份）。
    #[test]
    fn the_probe_answers_which_build_this_is() {
        let out = crate::control::ccm::probe_output("/somewhere/ccm");
        let line = out
            .lines()
            .find_map(|l| l.strip_prefix("build="))
            .unwrap_or_else(|| {
                panic!(
                    "`--ccm-probe` 的输出里没有 `build=` 那一行 —— \
                     「问一份二进制它是谁」这条路断了。实得：\n{out}"
                )
            });
        assert_eq!(
            line,
            super::super::BUILD_ID,
            "`build=` 报的是 `{line}`，而这一份的 `BUILD_ID` 是 `{}` —— \
             它没有取自本进程的常量（抄了别处 = 又一个标签）",
            super::super::BUILD_ID
        );
    }

    /// ★★ `KR70D2`：**`version = "0.0.0"` 是刻意的，而且有人在数它。**
    ///
    /// # 选的是件计划里那条 ②，代价写在这里
    ///
    /// ①（给它一个真版本、进 `release.yml` 那道「四处版本号与 tag 一致」的检查）**没选**。
    /// 代价现打：那道检查比的是**四处 == tag**，而后端换不换由**行为变没变**决定
    /// （`BUILD_ID` 的 bump 纪律，`SUBCOMMAND_HISTORY` 那张表就是它的账本），
    /// 不由发版节奏决定。把它绑上 tag ⇒ 每次发版这个 crate 的版本都动一格，
    /// 而 `BUILD_ID` 不动 ⇒ **后端从此有两个版本号，谁都不是权威** ——
    /// 那正是 `K33`「后端只有一个」在身份这一维要避开的东西。
    ///
    /// ②（明写刻意不用 + 判据钉住真身份住址）**选了**。它的代价也如实写：
    /// 后端的身份**不出现在** `cargo metadata` / `Cargo.lock` 那一面 ⇒
    /// 靠 crate 版本号做依赖管理的工具看它永远是 `0.0.0`。
    /// 今天没有任何消费者走那条路（本 crate 不发布、不被别的 crate 依赖，
    /// `[[bin]]` 是它唯一的产物），所以这一格是**已知且今天为空**的代价，不是漏。
    ///
    /// # 死值验：把身份住址改坏 ⇒ 必须红
    ///
    /// 三处住址逐个核：`const BUILD_ID` · 拼戳那段 · `--ccm-probe` 的 `build=`。
    /// 少任何一处，本条或它上面那两条当场红。
    #[test]
    fn the_crate_version_is_deliberately_zero_and_the_real_identity_has_a_home() {
        // ── ① `version = "0.0.0"` 还在，而且**恰好一处** ────────────────────
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let toml = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("读不到 {}：{e}", manifest.display()));
        let version_lines: Vec<&str> = toml
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("version = ") || l.starts_with("version="))
            .collect();
        assert_eq!(
            version_lines,
            vec![r#"version = "0.0.0""#],
            "本 crate 的 `[package] version` 实得 {version_lines:?}。\n\
             它**刻意**是 `0.0.0`（真身份住 `src/main.rs` 的 `BUILD_ID`，理由逐字写在\n\
             `Cargo.toml` 那一行上方）。要改成别的数字，得先答一个问题：\n\
             **谁在数它？** 把一个假值换成另一个假值，是 `KR70D2` 逐字点名的失效方向。"
        );
        // ── ② 「刻意」这件事在盘上写着，不是只住在我脑子里 ──────────────────
        //    锚是**一句中性的话**，不取自任何夹具名（`brief` 12 那条）。
        const DELIBERATE: &str = "本 crate 刻意不用 Cargo.toml 的版本";
        assert!(
            toml.contains(DELIBERATE),
            "`Cargo.toml` 里找不到逐字「{DELIBERATE}」—— \n\
             `0.0.0` 于是退回成一个**没人解释过的假值**，而下一个人看到它只会顺手改掉。"
        );
        // ── ③ 真身份的住址逐个还在 ─────────────────────────────────────────
        let main_rs = include_str!("../../src/backend/main.rs");
        let decls = main_rs
            .lines()
            .filter(|l| l.trim_start().starts_with("const BUILD_ID"))
            .count();
        assert_eq!(
            decls, 1,
            "`main.rs` 里 `const BUILD_ID` 的声明有 {decls} 处（应当 1）—— \n\
             ⚠ `src-tauri/build.rs::extract_build_id` 与 `release.yml` 两处都按\n\
             「含 `const BUILD_ID` 的那一行」去抠它；0 处 ⇒ 抠出 `unknown`，\n\
             多处 ⇒ 抠到哪一个看运气。"
        );
        assert!(
            !super::super::BUILD_ID.is_empty(),
            "`BUILD_ID` 是空串 —— 落到用户盘上的名字会变成 `cc-monitor-local-`（不带版本）"
        );
        // 戳的两个界标也是身份住址的一部分：它们一变，扫字节那一侧全瞎。
        assert!(
            !crate::BUILD_STAMP_OPEN.is_empty() && !crate::BUILD_STAMP_CLOSE.is_empty(),
            "身份戳的界标是空串 —— 扫字节那条路会把整份二进制当成一个戳"
        );
        assert!(
            build_ids_in(crate::CC_MONITOR_BUILD_STAMP.as_slice())
                == vec![super::super::BUILD_ID.to_string()],
            "那段 `static` 自己都扫不出 `BUILD_ID` —— 拼戳的 `const fn` 与常量脱钩了"
        );
    }

    /// 反向自检：判据**真的会抓人**。喂一个「多了一个子命令」的假指纹，比对必须不等。
    ///
    /// 直接喂字符串而不是去改 `main.rs` —— 后者要么污染工作区，要么因为改不进去而假绿
    /// （本仓已栽过：变异没落地却被当成「没覆盖」）。
    #[test]
    fn the_comparison_actually_bites() {
        let now = subcommand_fingerprint();
        let tampered = format!("{now}\n--brand-new-subcommand");
        assert_ne!(now, tampered, "比对形同虚设");
        // 也确认它认得出「少了一个」
        let shortened = now.split('\n').skip(1).collect::<Vec<_>>().join("\n");
        assert_ne!(now, shortened);
        // 以及：注释里的字面量不该被算进去（剥注释这一步是有效的）
        assert!(
            !now.contains("--account-trust-zero\n--account-trust-zero"),
            "同一个子命令被数了两次 —— 去重坏了"
        );
    }
}
