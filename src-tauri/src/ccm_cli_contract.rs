//! U1a（2026-08-01）：`shared/ccm` 这份 CLI 脚本的**强度契约**，做成可测量的数据。
//!
//! # 为什么要有这个模块
//!
//! 账本 S11：`sftp.rs::ccm_cli_has_required_elements` 在 U9 要迁到 `control/` 的命令构造点。
//! **迁移是强度悄悄下降的经典时机** —— 断言被「顺手重写」一遍，少一条 needle、
//! `require` 的阈值改小一点，全绿，没人知道。这个仓已经栽过同型的：
//! T01 审计实测过，`-t` 那条判据的**固定 needle 版本是空转的**（把 CLI 里的 `=名:`
//! 全改回裸目标，`cargo test` 依旧全绿），而那正是 F01 修掉的「杀错兄弟会话」生产事故。
//!
//! # 它做什么、不做什么
//!
//! **做**：把「这份脚本有多强」变成一个可比较的读数 [`Strength`]，并钉一条基线。
//! 迁移前后**用同一个 [`measure`] 跑两份脚本文本**，逐字段 `>=` —— 这才是「不许降强度」
//! 的可执行形式。
//!
//! **不做**：不改任何判据的强度。三张表是从 `sftp.rs` **逐字搬出**的，一条没加没减。
//!
//! # 读数 ≠ 阈值（这条区分是本模块的要点）
//!
//! `report.require(10, …)` 里的 `10` 是**阈值**，是「允许低到多少」；
//! [`Strength::t_targets_checked`] 是**读数**，是「实际扫到了多少」。
//! 只钉阈值挡不住「读数掉一格但仍 ≥ 阈值」那种情况 —— 迁移时少搬一条 tmux 命令恰好长这样。
//! 所以基线钉的是**读数**。
//!
//! ⚠ 顺带查出来的事实：`sftp.rs` 原注释写「真实脚本 checked=11 …… 往下留 1 的余量」，
//! **U1a 实测是 10 —— 余量早被吃光了**（追溯见 [`BASELINE`]）。这正是「读数没人盯」的后果：
//! 阈值一直绿着，而它与真值之间的距离已经归零，没有任何东西会告诉你。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。

#![cfg(test)]

/// `shared/ccm` 必须含有的关键要素。**从 `sftp.rs` 逐字搬出，一条没动。**
pub(crate) const REQUIRED_NEEDLES: &[&str] = &[
    "--ccm-probe",
    "--print",
    "--tmux",
    "--account",
    "--agent",
    "--ccm-sid",
    "CLAUDE_CONFIG_DIR",
    "@ccm_sid",
    "@ccm_sid_expect",
    "@ccm_agent",
    "exec",
];

/// 一条关键要素今天**住在哪**。
///
/// # 为什么要有「住址」这一维〔`K-P2` C 阶段第四拍，09-03；PM `§11` 丙〕
///
/// [`BASELINE`] 的 `needles >= 11` 守的是「`ccm` 的强度不许悄悄降低」，它买的是
/// **让流失变响**。而本件要做的**不是流失，是搬家** —— 一次记了账、有去处的移动。
/// 下限本身没错，错的是**它分不出这两件事**：两种改动在这把尺子上读出来一模一样
/// （都是 `needles` 掉 1）。那正是本工作区一直在治的形状：**尺子枚举的集合 ≠ 标签说的集合**。
///
/// ⇒ 加一维住址，把读数从「这条要素在不在 `shared/ccm` 里」换成
/// 「这条要素**在不在它自己登记的那个住址**」：
/// **搬家（同拍改这张表 + 代码真的到了新住址）照样绿，流失照样红。**
///
/// ⚠ **这不是下调阈值**：`BASELINE.needles` 与 [`MIN_CHECKED_T_TARGETS`] 两个数一个字节没动
/// （`KP2B` 🔴 逐字「不许 agent 自己动」）。换的是**读数的量法**，方向是把两件事分开，
/// 与 `K-P2` `KP2B` 那一拍把 `needles` 换成「剥注释之后还在」同族。
///
/// ⚠⚠ **它一条也不许自己给分**：[`NeedleHome::Backend`] 那一分是**记了账**换来的，
/// 账的真伪由 [`tests::every_needle_that_moved_is_actually_at_its_new_home`] 判
/// —— 两条**必须成对读**，只留一条的话「守恒」就退化成免检章
/// （把表里某条改成 `Backend` 就能让读数永远是 11）。
#[derive(Debug, Clone, Copy)]
pub(crate) enum NeedleHome {
    /// 还住 `shared/ccm` 的生产段。
    Ccm,
    /// 已搬进后端：`file` 是仓根相对路径，`anchor` 是那份文件**生产段**里必须逐字出现的锚点。
    ///
    /// ⚠ `anchor` 刻意与 needle 分开：needle 是 shell 那边的写法（`@ccm_agent`），
    /// 而后端那边可能是 argv 字面量（`"@ccm_agent"`）—— **同一件事换了语言就换了形状**，
    /// 用同一个串去两个语言里找，找到的多半不是同一件事。
    Backend {
        file: &'static str,
        anchor: &'static str,
    },
}

/// [`REQUIRED_NEEDLES`] 每一条的住址。**与那张表一一对应**
/// （长度由编译期钉子、逐条同名由 `the_home_ledger_covers_every_needle_exactly_once` 钉）。
///
/// **今天 11 条全住 `Ccm`** —— 一条都还没搬。搬一条的动作是**同一个提交里**：
/// ① 把这一行改成 `Backend { file, anchor }`；② 代码真的到那个住址；
/// ③ `shared/ccm` 那边不许再留一份（`KP2C` ①）。三样缺一样都会红，各自有各自的文案。
///
/// # 为什么名字是**下标引用**而不是再抄一遍
///
/// 两个理由，第二个是被现场逮出来的：
/// ① 抄一遍 = 两张手写清单互证，那正是 `inbound.rs::CommandSpec::fields` 头注逐字反对的形状；
///    引下标之后「两张表的名字对不上」在结构上**不可能发生**（只剩下标写错这一种，
///    由 `the_home_ledger_covers_every_needle_exactly_once` 判）。
/// ② 🔴 现打：抄一遍会让 `local_read_surface_registry` **当场红** ——
///    它按 `.claude` / `CLAUDE_CONFIG_DIR` 这几个针逐行数「碰本机 claude 面」的行，
///    本文件登记的是 `("src/ccm_cli_contract.rs", "non-read", 1, …)`（逐字「只在契约清单里
///    出现这个**变量名**，不读文件」）⇒ 再写一行同名字面量就把那个 1 顶成 2，
///    而那张登记表**不在本件写区**。⇒ 不新增那一处提及（行尾注释不算：`hits()` 先砍 `//`）。
pub(crate) const NEEDLE_HOMES: &[(&str, NeedleHome)] = &[
    (REQUIRED_NEEDLES[0], NeedleHome::Ccm),  // --ccm-probe
    (REQUIRED_NEEDLES[1], NeedleHome::Ccm),  // --print
    (REQUIRED_NEEDLES[2], NeedleHome::Ccm),  // --tmux
    (REQUIRED_NEEDLES[3], NeedleHome::Ccm),  // --account
    (REQUIRED_NEEDLES[4], NeedleHome::Ccm),  // --agent
    (REQUIRED_NEEDLES[5], NeedleHome::Ccm),  // --ccm-sid
    (REQUIRED_NEEDLES[6], NeedleHome::Ccm),  // 账号注入那个环境变量名
    (REQUIRED_NEEDLES[7], NeedleHome::Ccm),  // @ccm_sid（事实标记）
    (REQUIRED_NEEDLES[8], NeedleHome::Ccm),  // @ccm_sid_expect（意图标记，通道 A）
    (REQUIRED_NEEDLES[9], NeedleHome::Ccm),  // @ccm_agent
    (REQUIRED_NEEDLES[10], NeedleHome::Ccm), // exec
];

/// F04（结构性，防 D6 复发）：两处「通道 A 立刻打标」必须写 `@ccm_sid_expect`，
/// **不得**写裸 `@ccm_sid` —— 否则一个从未被确认过的意图声明会永久冒充「事实」。
/// 用带引号的完整 `set-option … @ccm_sid_expect` 片段做锚点，防未来改动悄悄改回去。
pub(crate) const CHANNEL_A_LITERALS: &[&str] = &[
    "tmux set-option -t $t @ccm_sid_expect $(sq \"$ccm_sid\")",
    "tmux set-option @ccm_sid_expect \"$ccm_sid\"",
];

/// 唯一允许的间接 tmux 目标变量 `$t` 的定义。**逐字钉死**：不钉的话它可以被改成裸值，
/// 从而绕过下面的 `-t` 结构性扫描（T01 审计 S3 已独立复现）。
pub(crate) const EXACT_T_DEF: &str = r#"t="$(sq "=$tmux_name:")""#;

/// `-t` 目标结构性扫描的**阈值**（允许低到多少）。
///
/// 设立时刻意比读数低 1、留一格余量免得正常增删命令误红；**今天余量为 0**（读数也是 10，
/// 见 [`BASELINE`] 的追溯）。**不因此下调阈值** —— 下调等于把「少一处 tmux 命令」这件事
/// 重新变成无声的。
pub(crate) const MIN_CHECKED_T_TARGETS: usize = 10;

/// 一份脚本文本的强度**读数**。字段都是「扫到了多少」，不是「要求至少多少」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Strength {
    /// [`REQUIRED_NEEDLES`] 里命中了几条。
    pub(crate) needles: usize,
    /// [`CHANNEL_A_LITERALS`] 里命中了几条。
    pub(crate) channel_a: usize,
    /// `-t` 结构性扫描实际检查过几个目标 token。
    pub(crate) t_targets_checked: usize,
    /// `-t` 扫描发现的**违规**数（目标不是 `=名:` 精确形态）。
    ///
    /// # 这个字段是 Phase D 审计补的，缺了它整条基线对 F01 是瞎的
    ///
    /// 初版 [`Strength`] 只取 `report.checked`。审计实测：把 CLI 里 4 处非 `$t` 的精确目标
    /// 全改成裸目标（**不动 `t=` 定义**），`checked` 仍是 10、`t_def_pinned` 仍是 true
    /// ⇒ **四个字段逐字段相等，`assert_at_least` 全绿**。
    ///
    /// 今天挡住它的是 `sftp.rs` 那个 `require()`（它看 violations）——**而那正是 U9 要搬走、
    /// 要重写的那一段**。照账本 S11「迁移前后同一个 `measure()` 逐字段 `>=`」照做，
    /// U9 完全可以只带走 `measure()`/`BASELINE`，把 F01 的防线丢在原地而对拍全绿。
    /// 这就是本模块 doc 自己引用的「固定 needle 是空转的」那个病，在读数层复发。
    pub(crate) t_violations: usize,
    /// `$t` 的定义是否逐字存在**且只被赋值一次**。
    pub(crate) t_def_pinned: bool,
}

/// tmux 目标必须是 `=名:` 精确形态（`=` 在前、`:` 在后）的谓词。
///
/// 裸目标会「精确→名字开头→glob」三级解析、打错兄弟会话；`=名`（无尾冒号）在
/// send-keys / capture-pane / set-option 上 rc=1 完全失效。
///
/// **只看紧跟的那一个 token**（T01 审计 S2：看整个窗口时，同一行里出现
/// `"export A=b:c"` 这种诱饵就能让裸目标零违规）。
pub(crate) fn t_target_is_exact(tok: &str) -> Result<(), String> {
    let eq = tok.find('=');
    let colon = tok.find(':');
    if eq.is_some() && colon.is_some() && eq < colon {
        Ok(())
    } else {
        Err("tmux 目标必须是 `=名:` 精确形态（= 在前、: 在后）。\
             裸目标会「精确→名字开头→glob」三级解析、打错兄弟会话；\
             `=名`（无尾冒号）在 send-keys/capture-pane/set-option 上 rc=1 完全失效"
            .to_string())
    }
}

/// 跑一遍 `-t` 目标的结构性扫描。抽出来是因为**基线测量与真断言必须用同一次扫描口径**
/// —— 两处各写一遍，迟早漂。
pub(crate) fn scan_t_targets(script: &str) -> crate::structural_scan::ScanReport {
    crate::structural_scan::scan_after_marker(
        script,
        // **marker 是 `-t` 而非 `-t `**：带空格会漏掉 `-t$name` 紧贴形态（T01 审计 S1，
        // 实测那样能把裸目标塞回来而 require 照样通过）。紧贴/带空格由扫描器统一处理，
        // 并排除 `-tmux` 这类更长选项名的误命中。
        "-t",
        Some("#"),
        48,
        // `$t` 是已钉死定义的间接变量，放行（但仍计数）。
        &|rest: &str| crate::structural_scan::first_token(rest) == "$t",
        &t_target_is_exact,
    )
}

/// 钉死 `$t` 的定义。抽出来的理由与 [`scan_t_targets`] 同：**参数两处各写一遍，迟早漂**
/// （Phase D 审计指出初版就是那样）。真断言在 `sftp.rs` 里 `.expect(…)`，读数这边取 `.is_ok()`。
pub(crate) fn pin_t_def(script: &str) -> Result<(), String> {
    crate::structural_scan::pin_definition(script, EXACT_T_DEF, "t=", "间接目标变量 $t")
}

/// 测量一份脚本文本的强度。**纯函数** —— U9 迁移后拿新的构造点文本再跑一次即可。
///
/// # `needles` / `channel_a` 只看**生产段**〔`K-P2` `KP2B`，08-29〕
///
/// 首版这两个字段是裸 `script.contains()`：读的是**整份文件、含注释**。
/// `K-P2` 摸底（08-28）现打出它的失效方式，逐字：11 条 needle 里**除 `@ccm_agent` 外
/// 全都有注释备份**（`--tmux` 全文件 67 次而代码行只 18 次 · `--print` 27/2 ·
/// `@ccm_sid` 27/7）⇒ **把实现搬走之后，在注释里写一句就能把 needles 补回 11**，判据照绿。
/// 那正是本模块头注自己点名的「固定 needle 是空转的」在**读数层**的复发。
///
/// ⇒ 两个 `contains` 字段先过 `guard_core::strip_hash_comment_lines`
/// —— 那是仓里给 `.sh` 用的**那一份**剥注释器（`plugin_class_registry.rs::ccm_agent_arms`
/// 与 `plugin_class_registry.rs::ccm_probe_values` 读的就是同一份 `shared/ccm`），
/// **不另发明第二份**（`structural_scan` 的登记表禁的正是这个）。
///
/// 现打（08-29，`shared/ccm` 1258 行 → 生产段 536 行）：11 条 needle **逐条**在生产段仍有命中
/// （最少的一条是 `@ccm_agent`，1 次），两条通道 A 字面量也都在
/// ⇒ **[`BASELINE`] 一格没降，判据严格变强**。
///
/// ⚠ `t_*` 三个字段**不跟着改**：[`scan_t_targets`] 自己带注释标记（`Some("#")`），
/// 在外面再剥一次就是第二份剥注释口径。
///
/// # `needles` 数的是「在不在**它自己的住址**」，不是「在不在这份脚本里」〔C 第四拍，09-03〕
///
/// 见 [`NeedleHome`]：搬走一条要素**同拍改 [`NEEDLE_HOMES`] 那一行**，读数不掉；
/// 而**没记账地少一条**（流失）读数照掉。⚠ `Backend` 那一分是记账换来的，
/// **账的真伪不在这里判** —— 在 `every_needle_that_moved_is_actually_at_its_new_home`。
/// 本函数仍是**纯函数**（只读 `script` 与一张 const 表），迁移对拍那条纪律不变。
pub(crate) fn measure(script: &str) -> Strength {
    measure_with(NEEDLE_HOMES, script)
}

/// 拿**指定的**住址账本测一份脚本。
///
/// ⚠ 抽这一层出来不是为了好看，是 `7u` 逼的：全局账本今天 `Backend` **零人群**
/// ⇒ 把下面那个 `Backend => true` 退回「只数 ccm」，本模块新加的三条判据**一条都不红**
/// （实测）。⇒ 让 `the_conservation_ledger_tells_a_move_from_a_loss` 拿夹具账本
/// 直接判「同一条要素、同一份脚本，只因住址不同而读数不同」——**那一分才有人守**。
pub(crate) fn measure_with(homes: &[(&str, NeedleHome)], script: &str) -> Strength {
    let report = scan_t_targets(script);
    let prod = guard_core::strip_hash_comment_lines(script);
    Strength {
        needles: homes
            .iter()
            .filter(|(n, home)| match home {
                NeedleHome::Ccm => prod.contains(*n),
                // 记了账的移动：这一分归账本，不归这份脚本。
                NeedleHome::Backend { .. } => true,
            })
            .count(),
        channel_a: CHANNEL_A_LITERALS
            .iter()
            .filter(|n| prod.contains(*n))
            .count(),
        // 一次扫描出两个字段 —— 扫两遍就是两份口径，迟早漂。
        t_targets_checked: report.checked,
        t_violations: report.violations.len(),
        t_def_pinned: pin_t_def(script).is_ok(),
    }
}

/// **迁移前的实测基线**（2026-08-01，U1a）。
///
/// 数字不要手打 —— 先把某个字段写成明显偏高的值跑一次，从失败信息里读真值。
/// 想降低任何一个字段之前先回答：**是这份脚本真的不再需要那个要素，
/// 还是搬家时漏搬了？** 后者正是 S11 要防的那件事。
pub(crate) const BASELINE: Strength = Strength {
    needles: 11,
    channel_a: 2,
    // **10 不是 11。** `sftp.rs` 的注释此前逐字写着「真实脚本 checked=11 …… 往下留 1 的余量」，
    // U1a 实测是 10 —— 那个余量早就被吃光了。追到 `666cc14`（「终端里无名 `--tmux` 改为无条件
    // 新建会话，不再 attach 进别人正用着的」）：它删掉两处 `tmux display-message -p -t "=…"`、
    // 加回一处 `has-session -t "=…"`，净 −1。**是真实行为变更的正当结果，不是护栏被悄悄丢了**
    // （逐行 diff 核过）。
    // ⇒ 〔08-01 当时〕读数 10 == 阈值 `MIN_CHECKED_T_TARGETS` 10，**余量为 0**：再正当地删掉一处
    // tmux 命令，`require(10)` 就会自己红。那不是 bug，是「来想一想」的信号
    // —— 详见下面`MIN_CHECKED_T_TARGETS >= 10` 那条编译期钉子的注释。
    //
    // ★★ **订正 08-29（`K-P2` C 阶段第一拍，`KP2B`）：上面那句「余量为 0」是 08-01 的快照，
    // 已经馊了。** 现打 `measure(CCM_CLI_SCRIPT).t_targets_checked = 11`（真跑，不是推演），
    // 而这里钉着 10 ⇒ **读数已经悄悄涨回 11，基线底下多出了一格没人看着的余量**。
    // 那正是本模块头注反复论证的那件事换了个方向：「只钉阈值挡不住『读数掉一格但仍 ≥ 阈值』」——
    // 基线低于真值一格，效果与阈值低一格**一模一样**：`shared/ccm` 现在可以**静默地少一处
    // `-t` 用法**（F01 那条生产事故的形状）而这条判据一个字都不说。
    // ⇒ 把基线抬到真值 **11**。**这是收紧，不是放宽**（`assert_at_least` 是 `>=`）。
    //
    // ⚠ **只动这一个字段**：`MIN_CHECKED_T_TARGETS`（阈值）**一个字节没动**，
    // 它是件计划 `KP2B` 🔴 点名「不许 agent 自己动」的两条编译期下限之一。
    // 抬基线之后 `BASELINE.t_targets_checked >= MIN_CHECKED_T_TARGETS` 由 10>=10 变成 11>=10，
    // 那条编译期钉子照旧成立。
    //
    // ⚠ 读数为什么从 10 涨到 11：本轮**没查**（本件的题目不是它）。
    // 只登记「量于 08-29、量具是 `measure(crate::sftp::CCM_CLI_SCRIPT)`、
    // 在工作树 `.claude/worktrees/k-p2` 上」。要追就用 `git log -S` 追 `-t ` 的增删，
    // 别拿这里的数当常量。
    t_targets_checked: 11,
    // **必须是 0，而且这个字段的比较方向与其他三个相反**（见 `assert_at_least`）。
    t_violations: 0,
    t_def_pinned: true,
};

// ───────────────────────────────────────────────────────────────────────────
// 基线与阈值的**编译期**钉子
//
// 这几条最初写成了 `#[test]` 里的运行期 `assert!`，clippy 当场指出
// 「this assertion has a constant value」—— 它说得对：两边都是 `const`，
// 判定在编译期就能做完。做成 `const _: () = assert!(…)` 严格更强：
//   · 改坏了**编不过**，不是「跑测试才发现」；
//   · 测试过滤器（`cargo test <名>`）绕不开它；
//   · 顺带消掉 clippy 噪音 —— 而噪音本身会让人对告警脱敏。
// 本仓已有先例：daemon 的 `RETIRE_MISS_THRESHOLD >= 2` 就是编译期断言。
// ───────────────────────────────────────────────────────────────────────────

/// 关键要素基线不得低于 11（账本 S11 逐字写死的下限）。
const _: () = assert!(BASELINE.needles >= 11);
/// 表长与基线必须一致 —— 否则「表里 10 条、基线写 11」会让基线永远达不到、或永远达到。
const _: () = assert!(REQUIRED_NEEDLES.len() == BASELINE.needles);
/// 住址账本与要素表**等长**〔C 第四拍〕。逐条同名由
/// `the_home_ledger_covers_every_needle_exactly_once` 钉（`&str` 比较不是 const fn，
/// 编译期只钉得住长度这一半 —— **如实登记，别把它读成「两张表对上了」**）。
const _: () = assert!(NEEDLE_HOMES.len() == REQUIRED_NEEDLES.len());
/// 通道 A 恰好两处，见 F04。
const _: () = assert!(CHANNEL_A_LITERALS.len() == BASELINE.channel_a && BASELINE.channel_a == 2);
/// 读数基线不得低于阈值 —— 低了就意味着 `require(MIN_CHECKED_T_TARGETS)` 当下就跑不过，
/// 两个数至少有一个是错的。
///
/// **今天两者相等（都是 10），余量为 0**（追溯见 [`BASELINE`]）。这个含义要说清楚，
/// 别让下一个人误读：`require(10)` 不会因此更容易误红 —— 它红的条件是「CLI 真的少了
/// 一处 `-t` 用法」，而那**本来就该有人看一眼**。没了的是「悄悄少一处也不响」这个缓冲。
/// **这是收紧不是缺陷**，故不下调阈值。
const _: () = assert!(BASELINE.t_targets_checked >= MIN_CHECKED_T_TARGETS);
/// **阈值本身也要钉。**
///
/// U1a 实现期发现计划写错了一条：原 DoD 说「把 `require` 的阈值改小 ⇒ **不影响**，
/// 因为阈值不是强度、读数才是」。**那是错的**，而且当场用变异证了：把
/// `MIN_CHECKED_T_TARGETS` 从 10 改成 3，全套依旧 4 passed。
///
/// 读数与阈值是**两个都能被单独放水**的旋钮：读数是「脚本里有多少处 `-t`」，
/// 阈值是「护栏肯为多少处负责」。U9 迁移时写个 `require(1)`，`-t` 那条判据当场退化成
/// 近乎无效，而读数基线完全管不着。⇒ 两个都得钉。
/// CLI 若正当地减少了 tmux 命令，改的是 [`BASELINE`] 并在那里写明理由，不是这个数。
const _: () = assert!(MIN_CHECKED_T_TARGETS >= 10);
/// 违规基线**必须是 0**。写成非 0 等于「允许存在裸目标」，那是把 F01 的事故形状合法化。
const _: () = assert!(BASELINE.t_violations == 0);

/// 逐字段断言 `got >= floor`。**不是相等** —— 强度只许涨不许跌。
pub(crate) fn assert_at_least(got: &Strength, floor: &Strength, who: &str) {
    let mut bad: Vec<String> = Vec::new();
    if got.needles < floor.needles {
        bad.push(format!(
            "关键要素命中 {} < 基线 {}（少了哪条看 REQUIRED_NEEDLES）",
            got.needles, floor.needles
        ));
    }
    if got.channel_a < floor.channel_a {
        bad.push(format!(
            "通道 A 字面量命中 {} < 基线 {} —— 意图声明被改回裸 @ccm_sid 了？",
            got.channel_a, floor.channel_a
        ));
    }
    if got.t_targets_checked < floor.t_targets_checked {
        bad.push(format!(
            "`-t` 目标扫描 checked={} < 基线 {} —— **读数掉了不是阈值掉了**：\
             多半是搬家时少搬了一条 tmux 命令，那条从此不受 §31a 精确目标判据管",
            got.t_targets_checked, floor.t_targets_checked
        ));
    }
    // **比较方向与其他字段相反**：violations 是「坏东西」，只许少不许多。
    if got.t_violations > floor.t_violations {
        bad.push(format!(
            "`-t` 目标违规 {} > 基线 {} —— 有 tmux 目标不是 `=名:` 精确形态。\
             裸目标会「精确→名字开头→glob」三级解析、**打错兄弟会话**（F01 修过的生产事故）",
            got.t_violations, floor.t_violations
        ));
    }
    if floor.t_def_pinned && !got.t_def_pinned {
        bad.push("`$t` 的定义不再被钉死 —— 它可以被改成裸值，从而绕过整条 `-t` 扫描".into());
    }
    assert!(
        bad.is_empty(),
        "{who} 的强度低于基线（S11：迁移不许降强度）：\n  {}\n\
         若确属正当下降，改 BASELINE **并在这里写明理由** —— 别只改数字。",
        bad.join("\n  ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 读数与基线的对拍。U9 迁移后，同一条断言换成喂新的构造点文本。
    #[test]
    fn ccm_cli_strength_is_at_or_above_baseline() {
        let got = measure(crate::sftp::CCM_CLI_SCRIPT);
        assert_at_least(&got, &BASELINE, "shared/ccm（迁移前）");
    }

    /// 反向自检：`assert_at_least` 真的会咬。
    ///
    /// 直接喂构造出来的读数，而不是去改真文件 —— 后者要么污染工作区，
    /// 要么因为改不进去而**假绿**。
    #[test]
    fn the_baseline_comparison_actually_bites() {
        let mut weak = BASELINE;
        weak.needles -= 1;
        let r = std::panic::catch_unwind(|| assert_at_least(&weak, &BASELINE, "夹具"));
        assert!(r.is_err(), "少一条 needle 必须红");

        let mut weak = BASELINE;
        weak.t_targets_checked -= 1;
        let r = std::panic::catch_unwind(|| assert_at_least(&weak, &BASELINE, "夹具"));
        assert!(
            r.is_err(),
            "`-t` 读数掉 1 必须红（阈值余量不该掩盖读数下降）"
        );

        // **方向相反的那个字段**：violations 变多才是退化。
        let mut weak = BASELINE;
        weak.t_violations += 1;
        let r = std::panic::catch_unwind(|| assert_at_least(&weak, &BASELINE, "夹具"));
        assert!(
            r.is_err(),
            "出现一处裸目标必须红 —— 这正是 F01 的生产事故形状，而初版读数对它完全是瞎的"
        );

        let mut weak = BASELINE;
        weak.t_def_pinned = false;
        let r = std::panic::catch_unwind(|| assert_at_least(&weak, &BASELINE, "夹具"));
        assert!(r.is_err(), "`$t` 不再钉死必须红");

        // 反向的反向：读数**高于**基线不许被误判成退化。
        let mut strong = BASELINE;
        strong.needles += 1;
        strong.t_targets_checked += 3;
        assert_at_least(&strong, &BASELINE, "夹具（强度上涨）");
    }

    /// 〔audit-0805 08-06〕**凡是要值的 flag 都必须过 `need_val`。**
    ///
    /// # 它治的是一个真机踩过的事故，而那个修法此前没有守卫
    ///
    /// `shared/ccm` 自己的头注逐字记着：盲取下一个 token 会把后面的 flag 当成值吃掉 ——
    /// **`ccm --tmux --account --print` 里 `--account` 吞掉了 `--print`，账号名变成字面量
    /// `"--print"`，会话照建、claude 照起，用户只在一闪而过的一行里看到报错。**
    /// 修法是给每个带值 flag 加一道 `need_val`。
    ///
    /// 08-06 变异实测：把 `--account` 那行的 `need_val "--account" "${1:-}";` **删掉** ——
    /// monitor 1000 · vitest 1288 · shellcheck **三样全绿**。
    /// ⇒ 事故修了，但「修法还在不在」没有任何东西看着，改回去不会红。
    ///
    /// # 判法：按**形状**而不是按 flag 名单
    ///
    /// 名单挡不住第 N+1 个 flag（`needle_anchor_registry` 头注反复记过这一点）。
    /// 这里钉的是形状：**参数解析段里凡是 `shift;` 消费下一个 token 的分支，
    /// 同一行必须出现 `need_val`** —— 新加一个带值 flag 时照抄邻居就自动满足，
    /// 而「顺手写成盲取」会当场红。
    #[test]
    fn every_value_taking_flag_goes_through_need_val() {
        /// 合法地 `shift` 却不取值的分支，逐条写清为什么。
        const EXCEPTIONS: &[(&str, &str)] = &[(
            "--)",
            "POSIX 的「选项到此为止」：它 `shift` 之后把**剩下全部**收进 passthru，             不是取一个值，没有「下一个 token 是不是 flag」这个问题",
        )];

        let src = crate::sftp::CCM_CLI_SCRIPT;
        // 只看参数解析那一段 —— 别处的 `shift` 与本条无关。
        let beg = src.find("while [ $# -gt 0 ]; do").unwrap_or_else(|| {
            panic!("`shared/ccm` 里找不到参数解析循环的开头 —— 段界读法坏了，本条会零命中地绿")
        });
        let end = src[beg..]
            .find("\n  esac")
            .map(|k| beg + k)
            .unwrap_or_else(|| panic!("找不到参数解析 `case` 的收尾 —— 段界读法坏了"));
        let block = &src[beg..end];

        let shifting: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#') && l.contains("shift;"))
            .collect();
        // ★ 抽取器自检 ①：数量地板。
        assert!(
            shifting.len() >= 8,
            "参数解析段里只抽到 {} 个 `shift;` 分支 —— 剥法坏了（建判据当天实测 10 个）",
            shifting.len()
        );
        // ★ 抽取器自检 ②：锚点。只有地板不够 —— 本会话实测过「人群被换掉、地板照样过」。
        assert!(
            shifting.iter().any(|l| l.starts_with("--account)")),
            "锚点 `--account)` 分支不在抽到的清单里 —— 收的多半不是解析分支了。\n\
             （它正是那次真机事故里被 `--print` 撑坏的那个 flag。）"
        );

        let bad: Vec<&&str> = shifting
            .iter()
            .filter(|l| !l.contains("need_val"))
            .filter(|l| !EXCEPTIONS.iter().any(|(e, _)| l.starts_with(e)))
            .collect();
        assert!(
            bad.is_empty(),
            "这些分支 `shift` 之后**盲取**下一个 token，没过 `need_val`：\n  {:?}\n\n\
             ⚠ 真机踩过：`ccm --tmux --account --print` 里 `--account` 吞掉 `--print`，\n\
             账号名变成字面量 `\"--print\"`，**会话照建、claude 照起**，\n\
             用户只在一闪而过的一行里看到报错。照邻居那行写上 `need_val \"<flag>\" \"${{1:-}}\";`。",
            bad
        );

        // ★ 例外保鲜：例外是欠账不是免检章。
        for (pat, why) in EXCEPTIONS {
            assert!(
                shifting.iter().any(|l| l.starts_with(pat)),
                "例外 `{pat}` 在解析段里已经找不到了 —— 删掉这一行。（当初的理由：{why}）"
            );
        }
    }

    /// 〔audit-0805 08-06〕**`cc-spawn` 找 `ccm` 时必须 `-f` —— 只 `-x` 会被同名目录劫持。**
    ///
    /// # 为什么这条住在「ccm 的契约」里
    ///
    /// 它钉的不是 cc-bus 的内部逻辑，是**别人怎么找到 ccm 这个二进制** —— 那是 ccm
    /// 对外契约的一部分：解析错了，后面整条 CLI 契约都无从谈起。
    ///
    /// # 事故与修法（脚本自己记着，逐字）
    ///
    /// 真实部署形态下 `~/.local/bin/cc-spawn` 经 `readlink -f` 后
    /// `SELFDIR = ~/.claude/skills/cc-bus/scripts`，于是 `../../ccm` = **`~/.claude/skills/ccm`**
    /// —— 碰撞面是 skills 目录。而 **`[ -x <目录> ]` 为真**，`skills/` 下全是目录，
    /// 「哪天有人装个名叫 `ccm` 的 skill 就会劫持解析（实测报『是一个目录』然后整体失败）」。
    /// ⇒ 修法是同时要 `-f`。
    ///
    /// # 08-06 实测：修法在，守卫没有
    ///
    /// 把那行改回只 `-x` —— `shellcheck` 0 · monitor 1001 · vitest 1288 **三样全绿**。
    /// 而这个脚本**跑不了**（要真 tmux，红线禁）⇒ 行为判据在这里不可能有，只能钉源码形态。
    ///
    /// # 为什么**不**做成「全 `shared/` 的 `-x` 必须配 `-f`」的通用形状判据
    ///
    /// 先量了人群再决定：`shared/` 下带 shebang 的脚本里 `[ -x ]` 一共 **3 处**，
    /// 另两处（`shared/ccm` 的 `CCM_DAEMON_BIN`）**不是同一个坑** ——
    /// 那两处后面跟着 `2>/dev/null` 且**有本地回落**，指到目录只会得到空输出、
    /// 然后按定框 §5「诚实降级」走本地那条；而 cc-spawn 这处**没有回落分支**，
    /// 目录会让整体失败。⇒ 3 个人群里 2 个要写例外，那种判据是仪式不是防护。
    /// 仓内那份 `cc-spawn` 的路径。
    ///
    /// ★★ **诚实边界（P4b-Y3）：它钉的是仓内那份，而本机真正在跑的不是它。**
    ///
    /// 实测：`~/.local/bin/cc-*` 全是指向 `~/.claude/skills/cc-bus/scripts/` 的 symlink，
    /// 而那份是 07-18 的 7699 字节；仓内这份是 08-07 的 9713 字节 —— **两者差 167 行**。
    /// `tool_registry.rs` 声明了 `shared/cc-bus` → `.claude/skills/cc-bus` 的部署映射，
    /// 但按 `PS1` 的读数那张表是**纯声明表**，**没有任何东西真的按它部署**。
    ///
    /// ⇒ 本文件里所有读这个路径的判据，**证明的是仓内那份的性质，不是本机行为**。
    /// 别把它们读成「机器上就是这样」。两份何时同步是 `U9` 第二问 + `PS1` 的题目。
    /// **shell 的生产段** = 剥掉 `#` 注释行 —— 直接用共享原语 `strip_hash_comment_lines`。
    ///
    /// ⚠ 不能用 `guard_core::production_code`：它剥的是 Rust 的 `//`，对 shell 一行都剥不掉。
    /// 08-13 实测：`ccm` 的判据用它取「生产段」，结果被**我自己写的一句解释性注释**判红
    /// （那句里提到了 `spawned.tsv`）。★ 剥注释器**选错了语言，等于没剥**。
    ///
    /// ⚠ 而首版在这里内联了一份 `filter(starts_with('#'))` —— `structural_scan` 的
    /// 「剥注释实现只许一份」登记表**当场逮住**，逐字问「共享原语为什么不够」。
    /// 答案是：**够**（`strip_hash_comment_lines` 就是给 `.sh`/`.yml` 用的那份）。⇒ 改成调它。
    /// `ccm` 里那行**真的会被执行**的 `capabilities=`（不是散文里提到它的那些）。
    ///
    /// ⚠ 本会话**同一族撞了四次**：`ccm-session=` 的位置比对、`while tmux has-session` 的
    /// 诚实注释、以及这里的两处 —— 都是「`find` 的第一处命中是**注释**」。
    /// 前三次各自就地修，第四次才明白该做的是**把取法收成一份**：
    /// 判据要问的是「代码里怎么写的」，而 `lines().find(contains(…))` 问的是「文件里有没有」。
    fn capabilities_line(ccm: &str) -> &str {
        ccm.lines()
            .find(|l| !l.trim_start().starts_with('#') && l.contains("capabilities="))
            .expect("`--ccm-probe` 的 capabilities= 那行 —— 它是能力协商的单一事实源")
    }

    fn shell_production(src: &str) -> String {
        guard_core::strip_hash_comment_lines(src)
    }

    fn cc_spawn_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join("shared/cc-bus/scripts/cc-spawn")
    }

    /// ★ P4b-Y1/Y2（`C14`〔用 08-12〕「spawn 就是起, 就是 creat」）。
    ///
    /// 那个 `or` 的**第四份**实现住在这里：`P3s` 数出两份（`ccm` 的、TS 的），
    /// `P4d` 的冒烟又撞出 daemon 的 wire mode（第三份），这里是第四份。
    ///
    /// **删复用与留避让必须同时钉**：把「不复用」做成「不避让」的话，同名直接建会撞上
    /// 别人的会话 —— 那正是 `C14` 要消灭的东西的另一面。
    #[test]
    fn cc_spawn_creates_it_never_reuses_a_live_session() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        assert!(
            src.lines().count() >= 50,
            "`cc-spawn` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // ① 复用那条路必须没了。钉的是**形状**不是某句文案：
        //    「探到活会话就 `exit 0`」这条路一旦回来，下面任一条都会命中。
        for gone in ["复用已有会话", "new_flag"] {
            assert!(
                !src.contains(gone),
                "`cc-spawn` 里又出现了 {gone:?} —— 默认复用回潮了。\n                 `C14`〔用 08-12〕逐字：「所有起会话就是起会话……**spawn 就是起, 就是 creat**」。"
            );
        }
        // ② 命名避让必须还在（不复用 ≠ 不避让）—— 但 `C15`〔用@08-13〕之后它**搬进 ccm 了**。
        //    ⚠ 这条判据 08-12 原本钉 `cc-spawn` 里那句 `while tmux has-session`。
        //    搬家之后**不能只是删掉它**：那样「避让还在不在」就没人钉了。改钉两件 ——
        //    （a）cc-spawn 把基名交出去；（b）避让在 ccm 那边（另一条判据 `the_avoidance_lives_in_ccm_now`）。
        // ⚠ 下面两条都判**剥掉 `#` 注释之后**的正文。首版没剥，当场被自己写的那句
        //   「原来这里是 `while tmux has-session …`」（记录搬走了什么的**诚实注释**）判红。
        //   ★ 这是本拍第二次撞上同一族：**匹配单位比事实大** —— 判据要判的是「代码里有没有」，
        //   而 `contains` 判的是「文件里有没有」。散文里提一句被删掉的东西是**好事**，不该被拦。
        let prod_spawn = shell_production(&src);
        guard_core::find_pinned(&prod_spawn, "--tmux-base=\"$base\"").unwrap_or_else(|e| {
            panic!(
                "{e}\n                 ⇒ cc-spawn 不再把**基名**交给 ccm 了。`C15` 之后避让归 ccm：\n                 cc-spawn 自己探一遍名字再传 `--tmux=<名>`，等于同一个事实算两遍，\n                 而且探完到真建之间有窗口期（`P3sc` 之后显式名撞名是 exit 3 响亮失败）。"
            )
        });
        // ⚠ 用 `contains_word` 而不是裸 `contains`：`needle_anchor_registry` 是**递减棘轮**，
        //   本拍新加的两处裸 `contains` 当场把它从 33 顶到 35（「不许把上限调上去让今天好过」）。
        assert!(
            !guard_core::contains_word(&prod_spawn, "while tmux has-session"),
            "`cc-spawn` 里又长出了自己的命名避让 —— `C15`〔用@08-13「cc-bus收进ccm」〕之后\n                          那件事归 `ccm`（`--tmux-base`）。留两份 = 改一处漏一处。"
        );
        // ③ `--new` 保留为 no-op：外面可能有人在传，让它报错等于把别人的脚本弄坏。
        // ⚠ needle 要**唯一确定那个事实**：`--new` 在用法串与注释里也出现
        //（`find_pinned` 当场报「命中 2 处，断言指不明是哪一处」）。钉那条 case 臂本身。
        guard_core::find_pinned(&src, "--new)  shift;;").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `--new` 整个删掉了。它该留成 no-op —— 兼容外面已有的调用方。")
        });
    }

    /// ★ P4b D 阶段补审：**`SKILL.md` 不许再教「复用」**。
    ///
    /// 那份文件是 agent 会读的**指令**，而它逐字写着「默认"到就用、没有才建"……
    /// 该目录已有活会话就**复用**……**不会误建重复会话**」——
    /// 删掉代码里的复用之后，这句话当天就成了假话，而**读它的是自动化，不是人**。
    ///
    /// 这正是本仓一路在治的「散文与代码说的不是一件事」。⇒ 立一条禁词守卫。
    #[test]
    fn the_cc_bus_skill_no_longer_teaches_session_reuse() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join("shared/cc-bus/SKILL.md");
        let src = std::fs::read_to_string(&path).expect("读 cc-bus SKILL.md");
        assert!(
            src.lines().count() >= 20,
            "`SKILL.md` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // ⚠ 钉的是**关于 cc-spawn 的那句教法**，不是「复用」这两个字本身 ——
        // `cc-busd` / `cc-bus-lib.sh` 里讲「PID 被复用」是另一回事，禁掉它是误伤。
        for banned in ["到就用、没有才建", "已有活会话就"] {
            assert!(
                !src.contains(banned),
                "`cc-bus/SKILL.md` 里又出现了 {banned:?} —— 那是 `cc-spawn` 复用活会话的教法，\n                 而代码里那条路已经删了（`C14`〔用 08-12〕「spawn 就是起, 就是 creat」）。\n                 ★ 读这份文件的是**自动化**，不是人：一句过期的指令会让 agent 按不存在的行为办事。"
            );
        }
    }

    /// ★ P4b-Y3：那条诚实边界**必须写在源码里**，不能只活在计划文件里。
    ///
    /// 这是「禁词守卫」的反面 —— **必需词**守卫：删掉那段话的人会被拦一次。
    /// 它防的不是笔误，是**下一个人把这些判据读成「机器上就是这样」**。
    #[test]
    fn the_cc_spawn_judges_say_out_loud_they_pin_the_repo_copy_not_the_running_one() {
        let me = include_str!("ccm_cli_contract.rs");
        // ⚠ **数次数，不是 `contains`**：本判据自己的数组里就写着这两句
        // ⇒ `contains` 恒真，改掉头注它照样绿（实测 `M4` 第一次就是这么绿的）。
        // 本会话已经第三次栽在「判据被自己要钉的名字命中」上（`P3s-Y2` / `P4d-Y4`）。
        // ⇒ 要求出现 **≥ 2 次**：一次是这里的字面量，另一次必须在头注里。
        for must in ["本机真正在跑的不是它", "没有任何东西真的按它部署"] {
            assert!(
                me.matches(must).count() >= 2,
                "`cc_spawn_path` 的头注里少了 {must:?}。\n                 那段话记的是一条**结构性假绿**：判据钉的是仓内那份，而 `~/.local/bin/cc-*` \n                 指向的是 `~/.claude/skills/` 那份（实测差 167 行）。删掉它，\n                 下一个人就会把这些判据读成「机器上就是这样」。"
            );
        }
    }

    #[test]
    fn cc_spawn_resolves_a_real_ccm_file_not_a_directory() {
        let path = cc_spawn_path();
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {path:?}: {e}"));
        // ★ 抽取器自检：文件被掏空/改名时，下面那条会零命中地绿。
        assert!(
            src.lines().count() >= 50,
            "`cc-spawn` 只剩 {} 行 —— 读法坏了或文件被掏空",
            src.lines().count()
        );
        // ⚠ **钉性质不钉拼法**〔第一版是 `pin_line` 整行相等，变异当场暴露它过严〕：
        // 把顺序换成 `[ -x X ] && [ -f X ]` 语义完全一样、同样安全，而整行相等会误红。
        // 本会话立过的标准是「合法微调不许误红」（bash 折叠阈值那条），这里照它办：
        // 只要求**两个测试都落在解析那一行上**，先后随意。
        for probe in ["-f \"$SELFDIR/../../ccm\"", "-x \"$SELFDIR/../../ccm\""] {
            guard_core::find_pinned(&src, probe).unwrap_or_else(|e| {
                panic!(
                    "{e}\n\
                     ⇒ `cc-spawn` 解析 `ccm` 时缺了 `{probe}` 这一半。**只 `-x` 不够**：\n\
                     `[ -x <目录> ]` 为真，而它找的 `../../ccm` 落在 `~/.claude/skills/` 下、那里全是目录，\n\
                     装一个名叫 `ccm` 的 skill 就会劫持解析（脚本头注记着实测：报「是一个目录」后整体失败）。\n\
                     这一处**没有本地回落分支**，所以失败是硬的，不是诚实降级。"
                )
            });
        }
        // 两个测试必须在**同一行**：分散到两处会让「其中一处被删」看起来仍然合规。
        let same_line = src.lines().any(|l| {
            l.contains("-f \"$SELFDIR/../../ccm\"") && l.contains("-x \"$SELFDIR/../../ccm\"")
        });
        assert!(
            same_line,
            "`-f` 与 `-x` 不在同一行了 —— 解析分支被拆开，其中一半可能已经不在那条判断上。"
        );
    }

    /// `P3sc`：**显式名撞了要响亮失败，不许静默接回别人的会话**〔`C15` 解除红线后落地〕。
    ///
    /// # 它防的那件事，后果与 `#76` 逐字相同
    ///
    /// 原来那一行是「幂等建闸」：`new-session` 失败**被吞**、`&&` 短路跳过 `send-keys`
    /// → 直接 `attach`。⇒ 调用方铸完名到真跑之间若有人抢占了这个名字，
    /// **用户被静默接进别人的会话，而载荷一个字都没送**（send-keys 被短路跳过了）。
    /// `C14`〔用@08-12〕逐字：「所有起会话就是起会话……**spawn 就是起, 就是 creat**」。
    ///
    /// # 判据钉三件（缺一件都能从缝里溜过去）
    ///
    /// ① 失败分支**在**（`|| { … exit 3; }`）；② 它**报得出是哪个名字**（不带名字的报错
    /// 等于没报）；③ **`2>/dev/null` 仍在** —— 那是给 tmux 自己那句噪声用的，
    /// 我们要的是**自己那句**说清楚，不是把 tmux 的原文糊到用户脸上。
    #[test]
    fn an_explicit_tmux_name_collision_fails_loudly() {
        let ccm = include_str!("../../shared/ccm");
        let line = ccm
            .lines()
            .find(|l| !l.trim_start().starts_with('#') && l.contains("seq=\"{ tmux new-session"))
            .expect("找不到构造 new-session 的那一行 —— 抽取器坏了或 ccm 改了形态");
        assert!(
            line.contains(" -d "),
            "建会话必须 detached（同 `base-flag-contract-guard` 那条，这里顺带钉住）"
        );
        assert!(
            line.contains("2>/dev/null"),
            "tmux 自己那句噪声仍该吞掉 —— 我们要的是**自己**那句说清楚"
        );
        let fail_branch = ccm
            .lines()
            .find(|l| l.contains("已被占用"))
            .expect("撞名必须有**响亮失败**那一支 —— 静默接回别人的会话与 #76 后果逐字相同");
        assert!(
            fail_branch.contains("exit 3"),
            "失败要有**非零退出码**，否则调用方（cc-spawn / monitor）判不出失败：{fail_branch}"
        );
        assert!(
            fail_branch.contains("%s"),
            "报错必须**带上是哪个名字** —— 不带名字的报错等于没报：{fail_branch}"
        );
    }

    /// ★ `P4b①`②〔`C15` 08-13〕：**避让搬进 `ccm` 之后，它在那边、且只有一份。**
    ///
    /// 这条与 `cc_spawn_creates_it_never_reuses_a_live_session` 的 ② 是**一对**：
    /// 那边钉「cc-spawn 不再自己算」，这边钉「那么它由谁算」。少任何一条，
    /// 「不复用 ≠ 不避让」这件事就会在某次重构里悄悄丢掉。
    #[test]
    fn the_avoidance_lives_in_ccm_now() {
        let ccm = include_str!("../../shared/ccm");
        // ① 实现在，且**恰好一处**：`find_pinned` 同时管「在不在」与「是不是只有一份」。
        guard_core::find_pinned(&ccm, "avoid_name_collision() {").unwrap_or_else(|e| {
            panic!("{e}\n⇒ 撞名避让的实现不见了或有两份。`C15` 把它从 cc-spawn 收进 ccm，收进来就该只有一份。")
        });
        // ② 生产段里不许再有**第二个**裸避让循环（本仓的老病：同一个事实几份实现）。
        // ⚠ 用 `shell_production` 而不是 `guard_core::production_code`：后者剥的是 Rust 的 `//`，
        //   对 shell 一行都剥不掉（本条先前能过纯属侥幸 —— ccm 的注释里刚好没提这个词）。
        let prod = shell_production(ccm);
        //   「恰好一处」正是 `find_pinned` 的语义 —— 比 `.matches().count()` 更贴事实，
        //   而且它连**两侧边界**一起管（`matches` 会把 `while tmux has-sessionX` 也数进去）。
        guard_core::find_pinned(&prod, "while tmux has-session").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `ccm` 生产段里的裸避让循环不是恰好一处（应当只有 `avoid_name_collision` 内那一处）。\n   多一处 = 避让又分叉了。")
        });
        // ③ 三条取名路都要**过同一份形状校验**。
        //    只校验显式名那条等于给避让路开后门：基名不撞时**逐字变成**会话名。
        guard_core::find_pinned(&ccm, "validate_tmux_name() {")
            .expect("形状校验该是一个函数、且只有一份");
        for (mode, call) in [
            ("--tmux-base", "validate_tmux_name \"$tmux_base\""),
            ("--tmux=<名>", "validate_tmux_name \"$tmux_name\""),
        ] {
            assert!(
                prod.contains(call),
                "取名模式 {mode} 没过形状校验（找不到 `{call}`）—— 那条路能建出 `*`/`:`/`=` 的名字，\n                              而 daemon 的 kill 主路**按设计拒收**这些字符 ⇒ 建得出来、UI 上杀不掉（F15 实测过）。"
            );
        }
        // ④ 能力协商：新模式必须出现在 `capabilities=`，否则老 ccm 上 cc-spawn 报的会是
        //    `未知选项: --tmux-base` + `建会话失败`，把「版本太旧」说成「建会话失败」。
        let caps = capabilities_line(ccm);
        assert!(
            guard_core::contains_word(caps, "tmux-base"),
            "`capabilities=` 里没有 `tmux-base` —— 调用方无从协商。实得：{caps}"
        );
        let spawn = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        guard_core::find_pinned(&spawn, "for _c in detach tmux-size tmux-base bus-register;")
            .unwrap_or_else(|e| {
                panic!(
                    "{e}\n⇒ `cc-spawn` 没把 `tmux-base` 列进能力协商 —— 它现在硬依赖这个模式了。"
                )
            });
    }

    /// ★ `P4b①`②：cc-spawn **从 ccm 读回名字**，且**读不到就停**。
    ///
    /// 读不到还往下走的话，`cc-register` 与 `spawned.tsv` 会写进**空名字** ——
    /// 总线上多一个叫 `""` 的幽灵，`cc-list` 显示在线、`cc-send` 石沉大海。
    /// 本仓给这种形状起过名字：**假成功比失败更坏**（B02 审计阻塞-2 那次逐字同款）。
    #[test]
    fn cc_spawn_reads_the_name_back_instead_of_computing_it() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        // ⚠ 光钉「摘取表达式在」**不够**：D 阶段 M2 实测把它改成
        //   `name="$base"; _unused=$(… ccm-session= …)` —— 表达式还在，名字却是自己拍的，判据全绿。
        //   ⇒ 钉的必须是**赋值**：`name=` 恰好一处，且那一处就是摘取。
        let name_assigns: Vec<&str> = prod
            .lines()
            .map(|l| l.trim_start())
            .filter(|l| l.starts_with("name="))
            .collect();
        assert_eq!(
            name_assigns.len(),
            1,
            "`cc-spawn` 生产段里给 `name` 赋值 {} 处（应恰好 1 处）。多一处 = 名字有第二个来源，\n                          而只有 `ccm` 知道它真建出了哪个。实得：{name_assigns:?}",
            name_assigns.len()
        );
        // ⚠ 还不够（M2 第二次实测）：`name="$base"; _unused=$(… ccm-session= …)` **也是一行**，
        //   于是「唯一那处赋值里有摘取表达式」照样成立。⇒ 钉**整个赋值就是那次摘取**：
        //   赋值右手边必须直接是命令替换（`name=$(`），不是先拍一个名字再顺手跑个表达式。
        //   ⚠ 第三轮：`name=$(printf "%s" "$base"); _x=$(… ccm-session= …)` 仍然过（一行两条命令，
        //   前两条判据都成立）。⇒ 那行必须是**纯赋值**：不许用 `;`/`&&` 在同一行串第二条命令。
        //   ★ 到此为止不再加码 —— 判据钉的是**事实的形状**（「name 的唯一来源是 ccm 报的名字」），
        //   不是跟人斗智。真要绕过它得先把这段注释一起改掉，那已经是明知故犯了。
        assert!(
            !name_assigns[0].contains(';') && !name_assigns[0].contains("&&"),
            "给 `name` 赋值那行串了第二条命令 —— 那就分不清 `name` 到底来自哪一条了。实得：{}",
            name_assigns[0]
        );
        assert!(
            name_assigns[0].starts_with("name=$(")
                && name_assigns[0].contains("s/^ccm-session=//p"),
            "`name` 不是从 ccm 报的 `ccm-session=` 摘来的 —— 那它拿什么名字去登记总线？实得：{}",
            name_assigns[0]
        );
        // 读不到就停：`[ -n "$name" ] || { … exit 1; }`
        let bails = prod
            .lines()
            .any(|l| l.contains("[ -n \"$name\" ] ||") && l.contains("exit 1"));
        assert!(
            bails,
            "`cc-spawn` 拿不到会话名时没有停 —— 再往下 `cc-register` 与台账会写进空名字，\n                          产出一个总线上叫 \"\" 的幽灵（`cc-list` 显示在线、`cc-send` 石沉大海）。"
        );
    }

    /// ★★ `ccm` 写**用户文件**的两处，形状**故意不同** —— 各自钉住〔08-13〕。
    ///
    /// # 为什么不进 `atomic_replace_registry`
    ///
    /// 那张表只扫 `src-tauri/src`（Rust）。shell 侧写用户文件的点**一共两处**，
    /// 都在 `ccm` 的预信任里 —— 为两处扩一张跨语言登记表是过度工程
    /// （那张表自己的头注也写着「刻意不建统一原子写入器」，理由同族：
    /// **两类文件的正确行为本来就不同**，硬统一只会把决定藏起来）。
    /// ⇒ 在这里钉形状，并把「为什么这两处不一样」写在明处。
    ///
    /// # 两处
    ///
    /// | 文件 | 写法 | 为什么 |
    /// |---|---|---|
    /// | `~/.claude.json` | **临时文件 + `jq -e` 校验 + 原子 `mv`** | 它是**整份 JSON**：写坏了 claude 起不来。原子替换 ⇒ 要么旧的要么新的，没有中间态 |
    /// | `~/.codex/config.toml` | **追加**（`>>`）+ 备份兜底 | 它是 TOML 段落追加，重写整份要解析 TOML（shell 里没有可靠的 TOML 解析器）⇒ 只能追加；**代价**是没有原子性，所以必须有备份，且**备份不成就不许写**（08-13 修） |
    ///
    /// ⚠ 这条判据防的是**把两者搞混**：让 claude 那条退化成追加（写坏整份 JSON），
    /// 或让 codex 那条在没有退路时硬写（写一半没人补）。
    #[test]
    fn the_two_user_file_writes_keep_their_different_shapes() {
        let ccm = include_str!("../../shared/ccm");
        let prod = shell_production(ccm);
        // ① claude：必须是 temp → 校验 → 原子 mv。三件缺一不可。
        for (needle, why) in [
            ("$tmpj", "没有临时文件 —— 那就是就地改用户的 claude.json，写坏了他起不来"),
            ("jq -e . \"$tmpj\"", "写完没校验 —— jq 产出坏 JSON 时会把坏文件搬过去"),
            ("mv \"$tmpj\" \"$cj\"", "不是原子替换 —— 中间态会被 claude 读到"),
        ] {
            assert!(prod.contains(needle), "claude 预信任那条：{why}（找不到 `{needle}`）");
        }
        // ② codex：追加之前必须先备份成功。
        guard_core::find_pinned(&prod, "if ! cp -p \"$ct\" \"$ct.bak-ccm.$$\"").unwrap_or_else(|e| {
            panic!("{e}\n⇒ codex 预信任又变成「备份失败照样追加」了。\n                       它是**追加写**，而失败恢复依赖那份备份存在 ⇒ 备份没了就等于\n                       「写一半、没人补」（08-13 实测：用 NAME_MAX 让 cp 失败即可复现）。")
        });
        // ③ 反向：claude 那条**不许**退化成追加（那会把整份 JSON 写坏）。
        assert!(
            !prod.contains(">> \"$cj\""),
            "claude 的 `~/.claude.json` 出现了**追加写** —— 那是整份 JSON，追加即写坏"
        );
    }

    /// ★ **`capabilities=` 报出去的每个能力，用法块里都要有一行**〔08-13〕。
    ///
    /// # 为什么这条值得钉
    ///
    /// `capabilities=` 是**给机器看**的协商面（`cc-spawn` / TS 侧 `CLI_REQUIRED_CAPS` 都在读），
    /// 用法块是**给人看**的。本会话一口气加了 `tmux-base` / `bus-register` 三个旗标，
    /// **机器那面立刻齐了、人那面一个字都没有** —— 用户敲 `ccm --help` 找不到它们。
    /// ⇒ 加旗标时「协商面」与「用法块」是同一件事的两份表达，抽不成一份 ⇒ 只能钉一致。
    ///
    /// ⚠ 反向不钉：用法块里可以有**不进能力集**的东西（如 `--print`、`--help` 本身、
    /// 位置动作 `new`/`resume`/`attach`），那不是遗漏。**只钉「能力有、文档无」这一向。**
    ///
    /// ⚠ 例外逐条列，不放宽判据：`ccm-sid` 的用法行写作 `--ccm-sid`、`tmux-size` 写作
    /// `--tmux-size`，token 与旗标名之间的映射就是「加两个横杠」——不成立的那几个在下面点名。
    #[test]
    fn every_advertised_capability_has_a_usage_line() {
        let ccm = include_str!("../../shared/ccm");
        let caps_line = capabilities_line(ccm);
        let caps: Vec<&str> = caps_line
            .split("capabilities=")
            .nth(1)
            .unwrap_or("")
            .split("\\n")
            .next()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        assert!(caps.len() >= 10, "只解析出 {} 个能力 —— 抽取器坏了（本条此刻是空转的）", caps.len());
        // 用法块 = 文件头那段 `#` 注释（`--help` 打的就是它）。
        let usage: String = ccm
            .lines()
            .take_while(|l| l.starts_with('#') || l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        // 这些 token 不是旗标：`new`/`resume`/`attach` 是**位置动作**，用法块里以裸词出现。
        const POSITIONAL: &[&str] = &["new", "resume", "attach"];
        let mut missing: Vec<&str> = Vec::new();
        for c in &caps {
            let ok = if POSITIONAL.contains(c) {
                usage.contains(*c)
            } else {
                usage.contains(&format!("--{c}"))
            };
            if !ok {
                missing.push(c);
            }
        }
        assert!(
            missing.is_empty(),
            "`capabilities=` 报了这些能力，而**用法块里一行都没有**：{missing:?}\n             \
             机器那面（协商）齐了、人那面（`ccm --help`）没有 ⇒ 用户找不到它们。\n             \
             ⇒ 加旗标时两处一起加。"
        );
    }

    /// ★ `P4e`〔实@08-12 立件，08-13 落地〕：**查找 daemon 的规则只有一份**，
    /// 且 `--print` 吐的是**配方**而不是查找结果。
    ///
    /// # 为什么这条判据的正题是「一份」
    ///
    /// 规则要在两处成立：真跑（`resolve_from_daemon`）与 `--print`（`resolve_recipe`）。
    /// 本仓既有的做法是「两份逐行同构、改一边必须改另一边」（`BUS_ID_RECIPE`）——
    /// 这次改成**同一个字面量**：真跑那侧 `eval` 它。⇒ 同构不再靠人的记性。
    ///
    /// ⚠ D 阶段的 M17 证明了这条判据不是装饰：把配方那侧偷偷退回老规则（只认 env），
    /// **exec 路的判据全绿** —— 因为它根本不走配方。逮住它的是「print↔exec 一致」那一格。
    #[test]
    fn the_daemon_lookup_rule_exists_exactly_once() {
        let ccm = include_str!("../../shared/ccm");
        let prod = shell_production(ccm);
        guard_core::find_pinned(&prod, "DAEMON_BIN_RECIPE=").unwrap_or_else(|e| {
            panic!("{e}\n⇒ 查找 daemon 的规则不是恰好一份。两份 = 改一边漏一边（`--print` 与真跑说的不是一件事）。")
        });
        // 真跑那侧必须**用**它，而不是自己再写一遍。
        assert!(
            prod.contains("eval \"$DAEMON_BIN_RECIPE\""),
            "`resolve_from_daemon` 没 eval 那份配方 —— 那它用的是第二份规则。"
        );
        // 配方那侧（`--print`）也必须原样吐出它。
        assert!(
            prod.contains("$DAEMON_BIN_RECIPE if [ -n"),
            "`resolve_recipe` 没把配方原样吐出来 —— `--print` 与真跑会说两件事。\n                          ★ D 阶段 M17 实测：只钉 exec 路的话，这个变异**存活**。"
        );
        // 逃生口在（套件靠它把「这台机器装没装 daemon」从测量里拿掉；用户靠它退回老行为）。
        assert!(
            prod.contains("CCM_NO_DAEMON"),
            "`CCM_NO_DAEMON` 逃生口没了 —— 那「日常行为变更」就没有退路，\n                          判据也没法把机器状态从测量里拿掉（`P4e §3` 提前记下的失效方式）。"
        );
        // 查找次序里**部署落点**必须在（`P4e` 的正题：不许只靠调用方注入 env）。
        assert!(
            prod.contains(".cc-monitor/bin/cc-monitor-remote"),
            "查找次序里没有部署落点 —— 那就退回「只认 `CCM_DAEMON_BIN`」，\n                          而那个变量的唯一生产注入点是 cc-monitor 起子进程时 ⇒ skill 在普通 shell 里够不着。"
        );
    }

    /// ★ `P4b①`③〔`C15` 08-13〕：**总线登记 + 台账也搬进 `ccm`**，且**格式仍归 cc-bus**。
    ///
    /// # 这条判据的正题是「没有第二份格式实现」
    ///
    /// 「收进 ccm」最容易做歪的一步，是让 `ccm` 自己 `printf '%s\t%s\t…' >> spawned.tsv`。
    /// 那样 TSV 的列、分隔符、锁文件命名就有了**两份**实现（cc-bus 一份、ccm 一份），
    /// 改一处漏一处 —— 正是本仓一路在收的那一族。
    /// ⇒ `ccm` 只负责**找到 cc-bus 的脚本、把事实告诉它**。
    #[test]
    fn ccm_delegates_bus_bookkeeping_instead_of_reimplementing_it() {
        let ccm = include_str!("../../shared/ccm");
        let prod = shell_production(ccm);
        // ① 两件事都**委派**给 cc-bus 自己的脚本。
        for script in ["cc-register", "cc-spawned-record"] {
            assert!(
                guard_core::contains_word(&prod, script),
                "`ccm` 的 `--bus-register` 没调 cc-bus 的 `{script}` —— 那它是自己写的格式吗？"
            );
        }
        // ② **不许自己拼 TSV**：`spawned.tsv` / `agents.tsv` 这两个文件名不该出现在 ccm 的生产段。
        //    钉文件名而不是钉 `printf`：文件名是「谁拥有这个格式」的最短证据。
        for owned_by_cc_bus in ["spawned.tsv", "agents.tsv"] {
            assert!(
                !prod.contains(owned_by_cc_bus),
                "`ccm` 生产段里出现了 {owned_by_cc_bus:?} —— 那是 **cc-bus 的**文件格式。\n                              `C15` 要的是「把事实告诉总线」，不是「让 ccm 也学会写总线的文件」：\n                              两份实现改一处漏一处（本仓一路在收的那一族）。"
            );
        }
        // ③b 台账脚本**单独查一次**〔08-13〕：定位用的是 `cc-register`，而这两个脚本
        //     **不是同时进来的**（`cc-spawned-record` 是 `C15` 才加的）。用户盘上那份 cc-bus
        //     只要旧一点（`exec-bit-guard` 早就在警告仓内与 `~/.claude/skills/cc-bus` 已漂移），
        //     就会是「登记照常、**台账悄悄没了**」—— 而整段是 best-effort，失败一个字都不会说。
        guard_core::find_pinned(&prod, "if [ -x \"$bus_scripts/cc-spawned-record\" ]; then")
            .unwrap_or_else(|e| {
                panic!("{e}\n⇒ 台账脚本没被单独查 —— 旧版 cc-bus 上会「登记上了总线、台账没写」，且一声不吭。")
            });
        // ③ 找不到 cc-bus 时不许**一声不吭**地跳过。
        //    「你要了登记却没登上」= 会话在跑、却不在总线上 ⇒ `cc-send` 石沉大海（假成功比失败更坏）。
        assert!(
            prod.contains("没有登记"),
            "找不到 cc-bus 脚本时 `ccm` 没吭声 —— 那会产出一个在跑、却不在总线上的孤儿。"
        );
        // ④ `--bus-register` 只在 `--detach` 那条路上成立（不 detach 随后 `exec` 进 attach）。
        guard_core::find_pinned(&prod, "--bus-register 需要配合 --detach").unwrap_or_else(|e| {
            panic!("{e}\n⇒ 那道前提没了。不 detach 的话本进程随后 exec 进 attach，登记做不成。")
        });
    }

    /// ★ 撞名重试**只对撞名**〔08-13〕—— 别的失败重试多少次都一样，还会掩盖真错误。
    ///
    /// # 为什么这条只能用判据钉
    ///
    /// e2e 看不见它：`cc-spawn-uplift` 里唯一的失败用例是「ccm 版本太旧」，而那条在
    /// **调 ccm 之前**就被能力协商挡了 ⇒ 根本走不到重试循环。变异「什么错都重试」
    /// 在套件上**存活**（差别只剩 5 次重试带来的约 1 秒延迟）——**射程之内的诚实结果**，
    /// 不是判据漏了。⇒ 那条性质只能在这里钉形状。
    ///
    /// ⚠ 顺带钉**退出码的取法**：`if _out=$(cmd); then …; fi` 之后的 `$?` 是
    /// **if 语句自己的状态**（条件为假且无 else ⇒ 0），**不是 cmd 的退出码**。
    /// 08-13 首版就这么写 ⇒ 循环把撞名读成 0、当场 break ⇒ 并发下原本 3 个**响亮失败**
    /// 变成 3 个**假成功**（都报同一个名字，而会话只有 1 个）。**比修之前更坏。**
    #[test]
    fn the_spawn_retry_is_for_name_collisions_only() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        // ① 退出码直接取，不经 `if`。
        guard_core::find_pinned(&prod, "&& _rc=0 || _rc=$?").unwrap_or_else(|e| {
            panic!("{e}\n⇒ 退出码没有直接取。`if cmd; then …; fi` 之后的 `$?` 是 if 自己的状态（0），\n                       那会把「撞名失败」读成成功 —— 08-13 实测因此产生过三个假成功。")
        });
        // ② 只对 3 重试；别的立刻停。
        guard_core::find_pinned(&prod, r#"[ "$_rc" = 3 ] || break"#).unwrap_or_else(|e| {
            panic!("{e}\n⇒ 重试不再只针对撞名（exit 3）。什么错都重试会**掩盖真错误**，\n                       而且重试多少次结果都一样（ccm 没装 / 参数错 / 预信任炸了）。")
        });
        // ③ 有上界（不许无限重试）。
        assert!(
            prod.contains("for _try in 1 2 3 4 5;"),
            "重试没有上界 —— 并发极密时会永远转下去，而那时该做的是**告诉用户**。"
        );
    }

    /// ★ `P4b①`③：`cc-spawn` **一件专属逻辑都不剩**了。
    ///
    /// `C15` 的验收就是这句话能不能说出口。三件（命名避让 / 总线登记 / 台账）搬完之后，
    /// 它剩下的只有 **cc-bus 的 id 规则**（`<basename>_cc`，`cc-whoami resolve` 与之对齐）
    /// 与参数转发 —— 那两件本来就该留在 cc-bus 这边。
    #[test]
    fn cc_spawn_no_longer_does_bus_bookkeeping_itself() {
        let src = std::fs::read_to_string(cc_spawn_path()).expect("读 cc-spawn");
        let prod = shell_production(&src);
        for gone in ["spawned.tsv", "cc-register", "list-panes"] {
            assert!(
                !prod.contains(gone),
                "`cc-spawn` 生产段里还有 {gone:?} —— `C15`〔用@08-13「cc-bus收进ccm」〕之后\n                              登记与台账归 `ccm --bus-register`。留在这里 = 两处各做一遍（还会各写一行）。"
            );
        }
        guard_core::find_pinned(&prod, "--detach --bus-register").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `cc-spawn` 不再要求 ccm 做登记了 —— 那新会话就**不在总线上**：\n   `cc-list` 看不到它，`cc-send` 打过去石沉大海。")
        });
        // ⚠ `--bus-note` 必须是**条件给**的：`need_val` 拒收空串（带值旗标的统一纪律），
        //   无条件写 `--bus-note "$task"` 在**没有初始任务**时会让 ccm 当场 die。
        //   ★ 首版就是无条件的：单测全绿、`cc-spawn-uplift` 当场红四条（6/7/8 号不带任务）。
        //   判据钉得了「那个旗标在」，钉不了「不带任务时它还能跑」——**那一格只有 e2e 够得到**。
        guard_core::find_pinned(&prod, "[ -n \"$task\" ] && ccm_args+=(--bus-note").unwrap_or_else(
            |e| panic!("{e}\n⇒ `--bus-note` 又变成无条件给了。任务为空时 ccm 会 die「--bus-note 需要一个值」。"),
        );
    }

    /// `P4b①` 的前置：**建会话的人要把会话名说出来**〔`C15` 08-13〕。
    ///
    /// # 为什么这是「收进 ccm」的第一块砖
    ///
    /// `C15` 要把 cc-bus 的三件（命名避让 / 总线登记 / 台账）收进 `ccm`。
    /// 而**第一件搬走的瞬间，调用方就不知道会话叫什么了** —— `cc-spawn` 今天是
    /// 自己算出 `$name` 再拿它去 `cc-register`、写 `spawned.tsv`、打提示。
    /// ⇒ 名字必须由**建它的人**报出来，否则同一个事实会被两处各算一遍
    /// （本仓一路在收的那一族）。
    ///
    /// # 判据钉三件
    ///
    /// ① 那行**在**；② 形状可解析（`ccm-session=`）；
    /// ③ **只在 `--detach` 时打**（不 detach 那条紧接着 `exec` 进 attach，
    ///    stdout 归被 attach 的程序，插一行会污染用户屏幕）。
    #[test]
    fn ccm_reports_the_session_name_it_created() {
        let ccm = include_str!("../../shared/ccm");
        let line = ccm
            .lines()
            .find(|l| !l.trim_start().starts_with('#') && l.contains("ccm-session="))
            .expect("`ccm` 必须把建出来的会话名报出来 —— 否则 `C15` 的三件一件都搬不动");
        assert!(
            line.contains("$detach") && line.contains("= 1"),
            "那行必须**只在 `--detach` 时**打：不 detach 那条随后 `exec` 进 attach，\
             往 stdout 插一行会污染用户屏幕。实得：{line}"
        );
        // ③ 位置：必须排在 `exec bash -c` **之前** —— exec 之后这个进程就没了。
        // ⚠ **按可执行行的行号比，不用 `find` 找第一处**：首版就栽了 —— `find` 命中的是
        //   我自己写的**注释**（它比代码行更靠前/更靠后都可能），于是位置断言指的是别人。
        //   本仓给这一族起过名字：「匹配单位比事实小」/「第一处命中是注释」，
        //   `base-flag-contract-guard` 头注里逐字记着同一次教训。
        let code_lines: Vec<(usize, &str)> = ccm
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim_start().starts_with('#'))
            .collect();
        let at = code_lines
            .iter()
            .find(|(_, l)| l.contains("ccm-session="))
            .map(|(i, _)| *i)
            .expect("上面已确认存在");
        let exec_at = code_lines
            .iter()
            .find(|(_, l)| l.contains("exec bash -c"))
            .map(|(i, _)| *i)
            .expect("找不到那句 exec —— 抽取器坏了");
        assert!(
            at < exec_at,
            "报名字那行排到了 `exec` 后面 —— exec 之后进程已被替换，永远打不出来"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // `K-P2`（`ccm` 基础命令进后端）在**本模块**立的四条〔08-29，C 阶段第一拍〕
    //
    // 为什么它们住在这里而不是 `launch_wire.rs`：`KP2A` 逐字记着现成那条判据的失效方式
    // —— 「**它的扫描面只有 `src-tauri/src/**\/*.rs`**（`env!("CARGO_MANIFEST_DIR")).join("src")`）
    // ⇒ **扫不到 `shared/ccm`**。若本件走「ccm 直接问 daemon」，接线落在 `shared/ccm` 里
    // ⇒ 这条判据**零命中地绿**，而事情做成了它一个字都不说。」
    // 而 `§0d`〔PM 08-29〕已裁定本件就走那条：**`ccm <子命令>` = 后端二进制以一次性模式跑**。
    // ⇒ 缺的那个扫描面在这里补：本模块的台子本来就是 `include_str!("../../shared/ccm")`。
    // ───────────────────────────────────────────────────────────────────────

    /// 仓根下某个文件的全文。`CARGO_MANIFEST_DIR` = `src-tauri`，它的上级就是仓根
    /// （同 [`cc_spawn_path`] 的取法，不另开第二种）。
    fn read_repo_file(rel: &str) -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join(rel);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读 {p:?} 失败: {e}"))
    }

    /// daemon 的**一次性子命令**表（`remote-daemon-proto/src/main.rs::SUBCOMMANDS`）。
    ///
    /// ⚠ 从**源码**里抽，不在这里手抄一份 —— 手抄的镜子本身就是新的漂移源
    /// （`inbound.rs::CommandSpec::fields` 的头注逐字论证过同一件事）。
    fn daemon_one_shot_subcommands() -> Vec<String> {
        let src = guard_core::production_code(&read_repo_file("remote-daemon-proto/src/main.rs"));
        let beg = src
            .find("const SUBCOMMANDS: &[&str] = &[")
            .expect("`main.rs` 里找不到 `SUBCOMMANDS` 表 —— 抽取器坏了，下面全会零命中地绿");
        let rest = &src[beg..];
        let end = rest
            .find("\n];")
            .expect("`SUBCOMMANDS` 表找不到收尾 `];` —— 段界读法坏了");
        let mut out: Vec<String> = Vec::new();
        // `"` 分段：奇数段在引号里。再按 `--` 起头过滤，注释里的引号扰不动它。
        let mut inside = false;
        for seg in rest[..end].split('"') {
            if inside && seg.starts_with("--") {
                out.push(seg.to_string());
            }
            inside = !inside;
        }
        out
    }

    /// `shared/ccm` 的**生产段**（剥 `#` 注释行）。
    fn ccm_production() -> String {
        shell_production(include_str!("../../shared/ccm"))
    }

    /// `shared/ccm` 的生产段里**真的把哪几条子命令发给了后端二进制**。
    ///
    /// # ★ 这个取法是变异逼出来的，第一版是错的〔`K-P2` C 阶段第一拍，08-29〕
    ///
    /// 首版量的是「子命令 token 在生产段里作为完整的词出现过」。变异 `M3`
    /// （把真调用点 `"$_ccm_db" --list-accounts …` 改成 `--list-accountsZ`）**存活** ——
    /// 因为同一段里还有一行**错误文案**：`why="daemon（$_ccm_db）答不出 --list-accounts"`。
    /// ⇒ 判据数的是「**提到**那个词的行」，不是「**发出**那条命令的行」。
    ///
    /// ★ 这正是件计划 `§0b-1` 排除项里点名过的同一族：
    /// 「`mcp.rs` 8 处、`hooks_diag.rs` 7 处那两个数里**包含错误文案的行**
    /// （登记表自己逐条注明了，`hits()` 数的是「提到那几个词的行」）⇒ **数字大不等于活多**」。
    ///
    /// ⇒ 改成钉**调用形状**：二进制的唯一引用形式是 `"$_ccm_db"`
    /// （`the_daemon_lookup_rule_exists_exactly_once` 钉着「查找规则只有一份」），
    /// 紧跟它的第一个 `--` token 才算一次调用。
    /// `--print` 那条路把同一次调用**再写一遍成文本**（`resolve_recipe`），
    /// 于是要先把 shell 转义的 `\` 抹平 —— 两种表示因此都数得到，且数成同一条。
    fn daemon_invocations(prod: &str) -> std::collections::BTreeSet<String> {
        const AFTER_BIN: &str = "_ccm_db\" ";
        let flat = prod.replace('\\', "");
        let mut out = std::collections::BTreeSet::new();
        for (i, _) in flat.match_indices(AFTER_BIN) {
            if let Some(tok) = flat[i + AFTER_BIN.len()..].split_whitespace().next() {
                // `[ -n "$_ccm_db" ]` 这类判空也会命中前缀 —— 只有 `--` 开头的才是子命令。
                if tok.starts_with("--") {
                    out.insert(tok.to_string());
                }
            }
        }
        out
    }

    /// `KP2B`〔`K-P2` 08-29〕**`needles` 的量法必须「剥注释之后还在」** —— 非空对照在这里。
    ///
    /// # 它买的是什么
    ///
    /// [`measure`] 的头注写了病理，这一条是它的**牙**：只改量法不配非空对照，
    /// 「改了」和「没改」在终端上一模一样（两种量法今天给出的都是 11）。
    /// ⇒ 喂一份**只有注释**的脚本：旧量法读 11，新量法必须读 0。
    #[test]
    fn the_needle_measurement_ignores_comment_only_occurrences() {
        // ① 真脚本：一格没降（这一条与 `ccm_cli_strength_is_at_or_above_baseline` 同源，
        //    但那条比的是 `>=`，这条钉的是**等于**——换了量法之后读数**恰好**没动。
        let real = measure(crate::sftp::CCM_CLI_SCRIPT);
        assert_eq!(
            real.needles, BASELINE.needles,
            "换成「剥注释之后还在」的量法之后，真脚本的 needles 从 {} 掉到了 {} ——\n\
             要么是某条要素只剩注释里那一份（那就是 `KP2B` 要抓的东西），\n\
             要么是剥注释器的口径变了。**别直接改 BASELINE**：先回答是哪一种。",
            BASELINE.needles, real.needles
        );
        assert_eq!(
            real.channel_a, BASELINE.channel_a,
            "通道 A 字面量在生产段只剩 {} 条（基线 {}）—— 意图标记被搬走或被改回裸 `@ccm_sid` 了？",
            real.channel_a, BASELINE.channel_a
        );

        // ② 非空对照：每条要素都**只**写在注释里的一份脚本。
        let mut comments_only = String::new();
        for n in REQUIRED_NEEDLES {
            comments_only.push_str("# ");
            comments_only.push_str(n);
            comments_only.push('\n');
        }
        for c in CHANNEL_A_LITERALS {
            comments_only.push_str("#   ");
            comments_only.push_str(c);
            comments_only.push('\n');
        }
        // ★ 夹具自检：那份夹具**真的**把每条都写进去了 —— 否则下面那两条是空真。
        assert!(
            REQUIRED_NEEDLES.iter().all(|n| comments_only.contains(*n))
                && CHANNEL_A_LITERALS.iter().all(|c| comments_only.contains(*c)),
            "夹具没把全部要素写进去 —— 下面两条会因为「本来就没有」而绿"
        );
        let got = measure(&comments_only);
        // ⚠ 期望值不是硬写的 0，是**今天已搬走的条数**〔C 第四拍〕：搬走的那些由住址账本
        //    记分，与这份夹具无关（夹具量的是 ccm 这一侧）。今天账本里 0 条 `Backend`
        //    ⇒ 这个期望仍是 0，而它**会随搬家自己走**，不必回来改数字。
        let moved = NEEDLE_HOMES
            .iter()
            .filter(|(_, h)| !matches!(h, NeedleHome::Ccm))
            .count();
        assert_eq!(
            got.needles, moved,
            "一份**只有注释**的脚本被读出 {} 条要素，而账本里只有 {moved} 条登记为已搬走 —— \
             剥注释没生效。\n\
             ⚠ 这正是 `KP2B` 登记的失效方式：实现搬走之后，**在注释里写一句就能把 needles 补回来**。",
            got.needles
        );
        assert_eq!(
            got.channel_a, 0,
            "通道 A 字面量写在注释里也被算了 {} 条 —— 同上",
            got.channel_a
        );
    }

    /// 某份**后端**文件的生产段。剥注释器按扩展名选 —— **选错语言等于没剥**
    /// （`the_two_user_file_writes_keep_their_different_shapes` 头注记过这个学费）。
    fn backend_production(rel: &str) -> String {
        let raw = read_repo_file(rel);
        if rel.ends_with(".rs") {
            guard_core::production_code(&raw)
        } else {
            shell_production(&raw)
        }
    }

    /// 住址账本（[`NEEDLE_HOMES`]）的违规清单。
    ///
    /// # 为什么抽成函数而不是直接写死在测试里
    ///
    /// 今天真账本里 **`Backend` 那一支一条人群都没有**（11 条全 `Ccm`）——
    /// 只钉真账本的话，「搬家判得了吗」这半条判据**从落地那天起就是空转的**，
    /// 而空转与绿在终端上一模一样。⇒ 抽出来，让 `the_conservation_ledger_tells_a_move_from_a_loss`
    /// 拿**夹具账本**把三种形态各打一遍：真搬家 / 假搬家（账记了东西没到）/ 两侧各留一份。
    ///
    /// ⚠ **如实边界**：第三条用的是 `ccm_prod.contains(needle)`，而 needle 之间**有包含关系**
    /// （`@ccm_sid` 是 `@ccm_sid_expect` 的前缀）⇒ 真要搬 `@ccm_sid` 那条时它会**误红**。
    /// 今天没人搬它，登记下来不动手：**误红是可发现的，漏判不是**。
    fn conservation_violations(
        homes: &[(&str, NeedleHome)],
        ccm_prod: &str,
        backend_prod: &dyn Fn(&str) -> String,
    ) -> Vec<String> {
        let mut bad: Vec<String> = Vec::new();
        for (n, home) in homes {
            match home {
                NeedleHome::Ccm => {
                    if !ccm_prod.contains(*n) {
                        bad.push(format!(
                            "`{n}` 登记住在 `shared/ccm`，而它的生产段里已经没有了 —— \
                             **这是流失，不是搬家**。\n\
                             要么把实现放回去；要么在 `NEEDLE_HOMES` 里写清它搬去了哪 \
                             （**同一个提交里**，且新住址真的有它）。"
                        ));
                    }
                }
                NeedleHome::Backend { file, anchor } => {
                    if !backend_prod(file).contains(*anchor) {
                        bad.push(format!(
                            "`{n}` 登记为已搬进 `{file}`，而那份文件的**生产段**里找不到锚点 \
                             {anchor:?} —— **账记了、东西没到**。\n\
                             守恒不是免检章：登记一条 `Backend` 就必须有人能在新住址看见它。"
                        ));
                    }
                    if ccm_prod.contains(*n) {
                        bad.push(format!(
                            "`{n}` 登记为已搬进 `{file}`，而 `shared/ccm` 的生产段里**还留着一份**。\n\
                             `KP2C` ① 逐字：「搬走的那一块，两侧不许各留一份」—— 留两份 = 改一处漏一处。"
                        ));
                    }
                }
            }
        }
        bad
    }

    /// 账本与要素表**逐条同名、一条不重不漏**。
    ///
    /// 名字引的是 `REQUIRED_NEEDLES[i]` ⇒ 「名字抄错」在结构上没了，
    /// **但下标表有它自己的失效方式**：同一个下标引两次、漏掉一个下标、顺序错位 ——
    /// 三种都会让某条要素**从此没人守**，而长度那条编译期钉子一个字都不会说。
    /// 这一条正是为它们而立的（不是为「名字对不对」）。
    #[test]
    fn the_home_ledger_covers_every_needle_exactly_once() {
        let names: Vec<&str> = NEEDLE_HOMES.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            REQUIRED_NEEDLES.to_vec(),
            "`NEEDLE_HOMES` 与 `REQUIRED_NEEDLES` 对不上（顺序也算）—— 多半是下标写重了或漏了。\n\
             任何一条不一一对应，`measure` 数的就不是 `REQUIRED_NEEDLES` 那 11 条了，\
             而 `sftp.rs::ccm_cli_has_required_elements` 仍在按后者判 ⇒ 两把尺子各说各话。"
        );
    }

    /// `KP2B`〔`K-P2` C 第四拍，09-03；PM `§11` 丙〕**搬家照样绿，流失照样红。**
    ///
    /// 这是「守恒」的另一半：[`measure`] 给 `Backend` 那一条**记账分**，
    /// 而账的真伪在这里判。两条**必须成对读** —— 只留 `measure` 那半，
    /// 把表里任何一行改成 `Backend` 就能让读数永远是 11（免检章）。
    #[test]
    fn every_needle_that_moved_is_actually_at_its_new_home() {
        let prod = ccm_production();
        // ★ 抽取器自检：剥注释器把代码也剥了的话，下面每条 `Ccm` 都会红成一片，
        //   那不是「流失」而是读法坏了 —— 先在这里分开。
        assert!(
            prod.lines().count() >= 300,
            "`shared/ccm` 的生产段只剩 {} 行 —— 剥注释器坏了，下面那些「流失」是假的",
            prod.lines().count()
        );
        let bad = conservation_violations(NEEDLE_HOMES, &prod, &backend_production);
        assert!(
            bad.is_empty(),
            "住址账本对不上现场（{} 条）：\n  {}\n\n\
             ⚠ `BASELINE.needles >= 11` 与 `MIN_CHECKED_T_TARGETS >= 10` 两条编译期下限\
             **不许下调**（`KP2B`）。要搬东西就改这张账本，不是改那两个数。",
            bad.len(),
            bad.join("\n  ")
        );
        // 读数对拍：账本说「11 条各在其位」，`measure` 就必须读出 11。
        assert_eq!(
            measure(crate::sftp::CCM_CLI_SCRIPT).needles,
            NEEDLE_HOMES.len(),
            "账本一条违规都没有，而 `measure` 读出来不是满分 —— 两处量法漂了"
        );
    }

    /// 上一条的**牙**：拿夹具账本把三种形态各打一遍（今天真账本里 `Backend` 零人群）。
    #[test]
    fn the_conservation_ledger_tells_a_move_from_a_loss() {
        // 假后端：只有 `moved.rs` 里有锚点。
        let backend = |f: &str| -> String {
            if f == "moved.rs" {
                "fn x() { tmux(&[\"set-option\", \"NEEDLE_ANCHOR\"]); }".to_string()
            } else {
                String::new()
            }
        };
        let moved = |file: &'static str| {
            [(
                "NEEDLE_X",
                NeedleHome::Backend {
                    file,
                    anchor: "NEEDLE_ANCHOR",
                },
            )]
        };

        // ① **搬家**：ccm 里没了、新住址有了 ⇒ 一条违规都没有。
        assert!(
            conservation_violations(&moved("moved.rs"), "echo hi\n", &backend).is_empty(),
            "记了账、东西也到了新住址 —— 这是搬家，不该红"
        );

        // ② **假搬家**：账记了，新住址空的。
        let v = conservation_violations(&moved("nowhere.rs"), "echo hi\n", &backend);
        assert_eq!(v.len(), 1, "假搬家该恰好红一条：{v:?}");
        assert!(
            v[0].contains("账记了、东西没到"),
            "假搬家红的不是那一格：{v:?}"
        );

        // ③ **两侧各留一份**：新住址有了，旧住址也还在。
        let v = conservation_violations(&moved("moved.rs"), "echo NEEDLE_X\n", &backend);
        assert_eq!(v.len(), 1, "两侧各留一份该恰好红一条：{v:?}");
        assert!(v[0].contains("还留着一份"), "红的不是那一格：{v:?}");

        // ④ **流失**：没记账地少一条。
        let stays = [("NEEDLE_Y", NeedleHome::Ccm)];
        let v = conservation_violations(&stays, "echo hi\n", &backend);
        assert_eq!(v.len(), 1, "流失该恰好红一条：{v:?}");
        assert!(
            v[0].contains("这是流失，不是搬家"),
            "流失红的不是那一格：{v:?}"
        );

        // ⑤ **反向的反向**：好好待在原地的那条不许被误判成流失。
        assert!(
            conservation_violations(&stays, "echo NEEDLE_Y\n", &backend).is_empty(),
            "要素还在原住址，不该红 —— 否则这条判据是「凡账本必红」，不是守恒"
        );

        // ⑥ ★★ **记账那一分本身**（`measure` 里 `Backend => true` 的唯一标准判据）。
        //    `7u` 实测：把那一支退回「只数 ccm」，上面五格**一条都不红** ——
        //    也就是说守恒的读数那一半当时只有变异证过、没有站着的判据。这一格补上。
        //    同一份脚本、同一条要素，**只因住址不同而读数不同**：这就是「搬家不掉分」。
        let script = "# 一份没有 NEEDLE_X 的脚本\n";
        assert_eq!(
            measure_with(&[("NEEDLE_X", NeedleHome::Ccm)], script).needles,
            0,
            "登记住在 ccm 而脚本里没有 ⇒ 读数必须掉（**流失照样红**）"
        );
        assert_eq!(
            measure_with(&moved("moved.rs"), script).needles,
            1,
            "登记为已搬走 ⇒ 读数**不掉**（**搬家照样绿**）。\n\
             这一分掉了，就意味着搬一块东西必须去动 `BASELINE.needles`／那两条编译期下限 —— \
             而 `KP2B` 逐字禁止 agent 动它们。"
        );
    }

    /// `KP2A`②〔`K-P2` 08-29〕**「进后端」的进度尺**：`shared/ccm` 的**生产段**里，
    /// 真发给后端二进制的一次性子命令有几条。**递增棘轮。**
    ///
    /// # 为什么换掉原来那把尺
    ///
    /// 件计划 `§0b-1` 逐字：拿 `capabilities=` 的 token 数当「进后端的进度尺」是**量错了对象**
    /// —— `capabilities=` 是**宿主渲染器 ↔ 远端 ccm 的 CLI 语法协商词表**，
    /// token 的加法逻辑是「宿主要据此改变渲染才加」，不是「ccm 多了个功能就加」。
    /// 实测：三件已经搬进后端的东西里**只有一件有 token**。
    ///
    /// # 量法与它排除了什么
    ///
    /// 分母取自 daemon **源码里那张表**（`main.rs::SUBCOMMANDS`），不是这里手抄的清单。
    /// 分子取自 [`daemon_invocations`]：**紧跟在 `"$_ccm_db"` 后面的那个 `--` token**
    /// —— 「发出了这条命令」，不是「提到了这个词」（那个错法被变异 `M3` 当场逮住，
    /// 头注在 [`daemon_invocations`] 上）。
    ///
    /// ⚠ **诚实边界**：这仍是**源码形态**判据，不是行为判据 —— 它证明的是「ccm 的生产段里
    /// 有一处**把这条子命令发给后端二进制**的调用点」，**不是**「那条路在运行时真的被走到」。
    /// 行为那一半只可能在 `e2e/ccm-*.sh` 里（件计划 `§1` 逐字：那五套**一条都不在门里**，
    /// 只在 CI 的 `assert-pass-floor.sh` 那 23 条里）⇒ 接线那一拍必须同时把 e2e 的地板抬上去。
    #[test]
    fn ccm_reaches_the_backend_through_one_shot_subcommands() {
        let table = daemon_one_shot_subcommands();
        // ★ 抽取器自检 ①：分母没缩水（建判据当天 23 条）。
        assert!(
            table.len() >= 20,
            "只从 `main.rs` 抽到 {} 条一次性子命令 —— 表的段界读法坏了，下面那个比值不算数",
            table.len()
        );
        // ★ 抽取器自检 ②：锚点。只有地板不够 —— 人群被换掉、地板照样过。
        for anchor in ["--resolve", "--list-accounts", "--launch"] {
            assert!(
                table.iter().any(|t| t == anchor),
                "锚点 `{anchor}` 不在抽到的子命令表里 —— 收的多半不是那张表了"
            );
        }

        let prod = ccm_production();
        // ★ 抽取器自检 ③：**另一侧**也要自检（建判据当天生产段 536 行 / 全文 1258 行）。
        assert!(
            prod.lines().count() >= 300,
            "`shared/ccm` 的生产段只剩 {} 行 —— 剥注释器把代码也剥了？下面那条会零命中地绿",
            prod.lines().count()
        );

        let used = daemon_invocations(&prod);
        // 递增棘轮。**接线成功的那一拍，这个数要跟着抬**（不抬 = 没接上）。
        assert!(
            used.len() >= 2,
            "`shared/ccm` 生产段里只发得出 {} 条后端子命令（{used:?}），少于登记的 2 条。\n\
             登记的两条：`--resolve`（F06b，resume 的 argv）· `--list-accounts`（`K-C1`，账号表）。\n\
             ⇒ 有人把 ccm 与后端之间的路拆了。",
            used.len()
        );
        for anchor in ["--resolve", "--list-accounts"] {
            assert!(
                used.contains(anchor),
                "已经搬进后端的 `{anchor}` 在 ccm 的生产段里没有调用点了（现有 {used:?}）—— \
                 退回本机实现了？（`KP2C`：两侧不许各留一份，而「两边都没有」也满足不了它）"
            );
        }
        // ★ 每一条发出去的子命令都必须在 daemon 的表里 —— 否则那条路**静默失效**。
        //   `main.rs` 的头注逐字记着这个形状：不在 `SUBCOMMANDS` 里 ⇒ `is_query_mode` 当成
        //   未知 flag ⇒ 打一行 warn 之后**照常进流模式**，「CLI 面看上去存在却永远调不到」。
        for u in &used {
            assert!(
                table.iter().any(|t| t == u),
                "ccm 发的 `{u}` 不在 daemon 的 `SUBCOMMANDS` 表里 —— 那条路是**静默失效**的：\n\
                 daemon 会把它当未知 flag、warn 一行然后进流模式，ccm 拿到的是一堆 jsonl 而不是答案。"
            );
        }
        // ★ 今天的读数，逐字钉住。
        assert!(
            !used.contains("--launch"),
            "ccm 的生产段开始发 `--launch` 了 —— **这多半是好事**：\n\
             「起会话」那格可能真接到后端一次性口上了 ⇒ 回 `K-P2` 的 `KP2A`/`KP2C`/`KP2D`：\n\
             ① 把上面那条棘轮从 2 抬到 3；② `KP2D` 的通道 A/B 冲突**必须已经解掉**\n\
             （daemon 的 `create-or-attach` 今天写的是**裸 `@ccm_sid`**，见\n\
             `the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move`）；\n\
             ③ e2e 那五套的地板要同轮抬。"
        );
    }

    /// `KP2C`〔`K-P2` 08-29〕**搬走的那一块，两侧不许各留一份；留退路就必须出声。**
    ///
    /// # 表的形状
    ///
    /// 每条 = （后端子命令, ccm 侧那条退路的**可观测**锚点, 退路**出声**的锚点）。
    /// 第三格是 `None` ⇒ 那条退路**今天是静默的**，记进下面的递减棘轮。
    ///
    /// # 为什么第三格要单列而不是直接断言「都出声」
    ///
    /// 件计划 `§0b-3` 的三条「为什么必然如此」里第 ③ 条逐字：「降级必须**出声**
    /// （`K-C1` 的 `§0b` 裁定）⇒ 每条退路配一段文案」。而现打：**两条里只有一条出声**。
    /// 直接断言「都出声」= 今天就红 ⇒ 那不是判据，是坏的门禁。
    /// ⇒ 照本仓既有的**递减棘轮**写法（`local_read_surface_registry` 那一族）：
    /// 把欠账登记成一个**只许变小**的数，欠账因此有名有姓、且不会悄悄变多。
    const BACKEND_BACKED_PATHS: &[(&str, &str, Option<&str>)] = &[
        (
            "--list-accounts",
            // 走了哪条路是**可观测**的（`K-C1` 的 `_ccm_acct_src` 先例）。
            "_ccm_acct_src=file",
            Some("账号解析已降级"),
        ),
        (
            "--resolve",
            // 退路在（拿不到就回空串，由调用方走本地 resume 串），但**一个字都不说**。
            "[ -n \"$_ccm_db\" ] || { printf ''; return 0; }",
            None,
        ),
    ];

    #[test]
    fn every_backend_backed_path_in_ccm_keeps_an_observable_fallback() {
        let prod = ccm_production();
        let used = daemon_invocations(&prod);
        // ① **完备性**：扫出来的每一条都必须在表里登记 —— 新接一条路而不登记，这里当场红。
        //    （这就是 `KP2C` 的「成对判」在本条上的形态：接线与退路登记同一拍。）
        for u in &used {
            assert!(
                BACKEND_BACKED_PATHS.iter().any(|(c, _, _)| *c == u.as_str()),
                "`shared/ccm` 生产段里发了 `{u}`，而它没登记在 `BACKEND_BACKED_PATHS` 里。\n\
                 ⇒ 补一条：（子命令, 退路的可观测锚点, 出声锚点或 None）。\n\
                 `KP2C` 逐字：「搬走的那一块，两侧不许各留一份；**留退路就必须出声**」。"
            );
        }
        // ② 反向：表里登记的每一条都要真的还在被用 —— 例外是欠账不是免检章。
        for (cmd, _, _) in BACKEND_BACKED_PATHS {
            assert!(
                used.contains(*cmd),
                "登记着 `{cmd}` 而 ccm 的生产段里已经找不到它的调用点（现有 {used:?}）—— \
                 删掉这一行，或者说明退回本机了"
            );
        }
        // ③ 每条锚点都要**真在生产段里**（非空对照：锚点烂掉 ⇒ 这条判据就是空真）。
        let mut silent: Vec<&str> = Vec::new();
        for (cmd, fallback, speaks) in BACKEND_BACKED_PATHS {
            assert!(
                prod.contains(fallback),
                "`{cmd}` 的退路锚点 {fallback:?} 在生产段里找不到了 —— \
                 要么退路没了（那 `ccm` 在没有后端的机器上就废了：本文件经 `sftp.rs` 的 \
                 `include_str!` 部署到**任意**远端，「daemon 不在」是常态），\
                 要么锚点该更新了。"
            );
            match speaks {
                Some(line) => {
                    // ★★ **「出声」判的是 stderr，不是「文件里有这句话」**〔铁律 15 自查，08-29〕。
                    // 本轮 M3 刚证过同一族：判据数到的是**提到那个词的行**而不是**做那件事的行**。
                    // 只断言 `prod.contains(line)` 会被「把同一句文案挪进一段不发 stderr 的代码」
                    // 满足 ⇒ 这里改成：文案所在的那一行**及其后 3 行生产行之内**必须有 `>&2`。
                    // （现打：`shared/ccm` 那句 `printf` 与它的 `>&2` **不在同一行** ——
                    //  `printf '…' \` 续行，`>&2` 在下一行 ⇒ 同行断言会当场误红。窗口 3 是量出来的，不是猜的。）
                    let lines: Vec<&str> = prod.lines().collect();
                    let at = lines.iter().position(|l| l.contains(*line)).unwrap_or_else(|| {
                        panic!(
                            "`{cmd}` 登记为「降级出声」，而它的文案锚点 {line:?} 不在生产段里 —— \
                             出声那段被删了？（`K-C1` 的 `§0b` 裁定：**降级必须出声**）"
                        )
                    });
                    let window = &lines[at..(at + 4).min(lines.len())];
                    assert!(
                        window.iter().any(|l| l.contains(">&2")),
                        "`{cmd}` 的降级文案还在，**但那一段不再往 stderr 说话**了\
                         （文案行及其后 3 行生产行里找不到 `>&2`）：\n  {window:?}\n\
                         ⇒ 「出声」变成了往 stdout 说 —— 而 ccm 的 stdout 是**机器读的**\
                         （`ccm-session=` 那一行、`--print` 的整条串）⇒ 那不是出声，是**污染载荷**。"
                    );
                }
                None => silent.push(cmd),
            }
        }
        // ④ **递减棘轮**：静默降级的条数只许变小。今天 1 条（`--resolve`）。
        assert!(
            silent.len() <= 1,
            "静默降级的后端路涨到了 {} 条（{silent:?}）—— 只许减不许加。\n\
             `K-C1` 的 `§0b` 裁定逐字：**降级必须出声**；\n\
             不出声的降级会产出「用了旧实现却以为用的是后端」——\n\
             件计划 `§0b-4` 逐字记着这个形状：**搬家之后它变得更容易静默**。",
            silent.len()
        );
        assert_eq!(
            silent,
            vec!["--resolve"],
            "静默那条换人了 —— 今天登记的欠账是 `--resolve`（`resolve_from_daemon` 拿不到就\
             回空串，一个字不说）。换了人就把理由与新住址写进 `K-P2` 的 `§4`。"
        );
    }

    /// `KP2D`〔`K-P2` 08-29〕**通道 A（意图）与通道 B（事实）不许在搬家时被合并。**
    ///
    /// # 现打的冲突（件计划 `§0b-1` 第 10 行 + `KP2D` 逐字）
    ///
    /// 建会话那一刻，`shared/ccm` 写 `@ccm_sid_expect`（**意图**，F04 通道 A），
    /// 而 daemon 的 `launch` 写**裸 `@ccm_sid`**（**事实**，通道 B）。
    /// 破坏性动作（`kill`）**只认 `@ccm_sid`** ⇒ 起会话一旦改走 daemon 的 `launch`，
    /// 「声明了但从未真正跑起来」的会话会**当场获得事实身份**，F04 修掉的那个形状（`R10`）原路回来。
    ///
    /// # 这条判据补的是哪个洞
    ///
    /// `KP2D` 逐字记着它的失效方式：「`CHANNEL_A_LITERALS` 钉的是 **`shared/ccm` 里那两条
    /// shell 字面量**。实现搬去 daemon ⇒ 那两条字面量随代码一起消失 ⇒ **判据因为没有靶子
    /// 而不再红**（不是变绿，是变空真）。」
    /// ⇒ 这里加两样：**靶子自检**（两条字面量各恰好一处）＋ **成对判**
    /// （ccm 一旦开始发 `create-or-attach`，daemon 那侧的裸 `@ccm_sid` 必须已经归零）。
    ///
    /// ⚠ **本条不改 daemon 的行为**：`control/launch.rs::run` 的 `Mode::CreateOrAttach` 臂里
    /// 写裸 `@ccm_sid` 这件事该由谁改，
    /// 件计划 `§4` 第 4 条逐字「**本件不自裁**」（改它会动到 `K-P5` 的地面）。
    /// 本条只做一件事：**把那个冲突钉成机器看得见的**，并让它在接线那一拍变成硬闸。
    #[test]
    fn the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move() {
        let prod = ccm_production();
        // ① **靶子自检**：两条通道 A 字面量各**恰好一处**，且在生产段里。
        //    `channel_a` 那个读数只数「命中几条」——靶子整个消失时它会掉到 0 而 `assert_at_least`
        //    会红，但**换个地方写一份**它同样是 2 ⇒ 用 `find_pinned` 钉「恰好一处、不许被撑大」。
        for lit in CHANNEL_A_LITERALS {
            guard_core::find_pinned(&prod, lit).unwrap_or_else(|e| {
                panic!(
                    "{e}\n⇒ 通道 A 的意图标记 {lit:?} 不再是恰好一处。\n\
                     搬家把它带走了 ⇒ **这条判据会因为没有靶子而不再红**（空真，不是绿）：\n\
                     照 `KP2D` 把它钉在**新住址**上，并把这条自检一起搬过去。"
                )
            });
        }

        // ② daemon 那一侧今天写的是什么 —— **读源码，不跑它**（红线：不许起真 daemon）。
        let launch =
            guard_core::production_code(&read_repo_file("remote-daemon-proto/src/control/launch.rs"));
        assert!(
            launch.contains("create-or-attach"),
            "`control/launch.rs` 的生产段里找不到 `create-or-attach` —— 抽取器坏了，下面全是空真"
        );
        let bare = launch
            .match_indices("@ccm_sid")
            .filter(|(i, _)| !launch[*i..].starts_with("@ccm_sid_expect"))
            .count();
        let intent = launch.matches("@ccm_sid_expect").count();

        // ★★ **写点与读点必须分开数**〔C 第四拍订正，09-03〕。
        //
        // # 本条上一版把两件事装进了同一个数，而那让它的成对判**无解**
        //
        // 上一版 `bare == 2` 数的是「`@ccm_sid` 这个词在生产段里出现几次」，而现打那 2 处
        // 是**两件不同的事**：
        //   · `tmux(&["set-option", …, "@ccm_sid", sid])` —— **写**事实标记（`:292` 那一处）；
        //   · `set-titles-string "ccm-rbind-#{@ccm_sid}"` —— tmux 的**格式串**，
        //     它在**显示时读**这个标记（`:299` 那一处）。
        //
        // 而 ③ 那条成对判要求接线后 `bare == 0` ⇒ 照上一版的算法，**连标题格式串也得删掉**。
        // 那不是解冲突，那是把 F04 的标题回填功能删了：`e2e/ccm-rbind-title.sh`（地板 8）
        // 守的正是「窗口标题由 tmux 从 `@ccm_sid` 自己合成」，而 `shared/ccm:1247` 那一份
        // 还带 `#{?@ccm_sid,…,#T}` 的回退分支。**读一个标记不会让任何会话获得身份。**
        //
        // ⇒ 分成两个数。**这是收紧不是放宽**：上一版一个数说不出「写点变成了读点」，
        // 而下面这条 `(writes, reads, intent)` 三元组对那种调包**当场红**。
        // ⚠ 危险的一直是**写**：破坏性动作认的是事实标记，冒名靠的是写。
        let writes = launch.matches("\"@ccm_sid\"").count();
        let reads = launch.matches("#{@ccm_sid}").count();
        // ★ **分类完备性自检**：出现第三种形状（既不是 argv 字面量、也不是格式串）时，
        //   上面两个数会加不满 `bare` ⇒ 在这里响，而不是在别处静默地少数一处。
        assert_eq!(
            writes + reads,
            bare,
            "`control/launch.rs` 生产段里出现了本条分不出类的 `@ccm_sid` 用法\
             （裸命中 {bare}，其中 argv 写点 {writes} + 格式串读点 {reads}）。\n\
             ⇒ 回来补一类，别让它落在分类之外 —— 那正是本条上一版栽的那个形状\
             （**一个数装了两件事**）。"
        );
        assert_eq!(
            (writes, reads, intent),
            (1, 1, 0),
            "daemon `control/launch.rs` 生产段里「事实标记**写点** / 事实标记**读点** / \
             意图标记」的处数从 (1, 1, 0) 变成了 ({writes}, {reads}, {intent})。\n\
             登记的读数：写点 = `Mode::CreateOrAttach` 里那句 `set-option … \"@ccm_sid\" sid`；\
             读点 = `set-titles-string \"ccm-rbind-#{{@ccm_sid}}\"`（**显示时读**，不授予身份）。\n\
             ⇒ 写点变 0 = **冲突解掉了**，把 `K-P2 §4` 第 4 条结掉并改这个期望值；\n\
             ⇒ 写点变多 = 事实标记又多了一处写点，回 F04 看通道 A/B 的分家；\n\
             ⇒ 读点变 0 = 标题回填没了（`e2e/ccm-rbind-title.sh` 地板 8 守的就是它）。"
        );

        // ③ **成对判**（`KP2C` ③ 逐字「本条必须与 `KP2A` 成对判」）：
        //    ccm 一旦开始发 `create-or-attach`，②那个冲突必须**已经**解掉。
        //    今天 ccm 生产段里零命中 ⇒ 这一支是**待触发**的，不是空真：上面 ① 是它的靶子自检。
        // ⚠ 判的是**写点**，不是裸命中数 —— 理由在上面那段：把读点也算进来，
        //    这条判据就只有「删掉标题功能」一种满足法，那是**判不了也做不对**。
        if guard_core::contains_word(&prod, "create-or-attach")
            || daemon_invocations(&prod).contains("--launch")
        {
            assert_eq!(
                writes, 0,
                "`shared/ccm` 开始发 `create-or-attach` 了，而 daemon 建会话时**仍在写事实标记 \
                 `@ccm_sid`**（{writes} 处）。\n\
                 ⇒ 「声明了但从未真正跑起来」的会话会当场获得**事实**身份，而 `kill` 只认它 —— \
                 F04 修掉的 `R10` 原路回来。\n\
                 先解 `K-P2 §4` 第 4 条（「过了后端之后 `@ccm_sid` 由谁写」），再接线。"
            );
        }
    }
}
