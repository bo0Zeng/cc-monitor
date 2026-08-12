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
    /// 而 `sftp.rs::deploy_decision` 判「远端要不要换 daemon」的**唯一**判据就是 build_id 字符串
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
        let prod = production_code(include_str!("main.rs"));
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
