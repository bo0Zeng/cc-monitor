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
pub(crate) fn measure(script: &str) -> Strength {
    let report = scan_t_targets(script);
    Strength {
        needles: REQUIRED_NEEDLES
            .iter()
            .filter(|n| script.contains(*n))
            .count(),
        channel_a: CHANNEL_A_LITERALS
            .iter()
            .filter(|n| script.contains(*n))
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
    // ⇒ 今天读数 10 == 阈值 `MIN_CHECKED_T_TARGETS` 10，**余量为 0**：再正当地删掉一处
    // tmux 命令，`require(10)` 就会自己红。那不是 bug，是「来想一想」的信号
    // —— 详见下面`MIN_CHECKED_T_TARGETS >= 10` 那条编译期钉子的注释。
    t_targets_checked: 10,
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
        // ② 命名避让必须还在（不复用 ≠ 不避让）。
        guard_core::find_pinned(&src, "while tmux has-session").unwrap_or_else(|e| {
            panic!(
                "{e}\n                 ⇒ 命名避让没了。把「不复用」做成「不避让」会让同名直接建、撞上别人的会话，\n                 那正是 `C14` 要消灭的东西的另一面。"
            )
        });
        // ③ `--new` 保留为 no-op：外面可能有人在传，让它报错等于把别人的脚本弄坏。
        // ⚠ needle 要**唯一确定那个事实**：`--new` 在用法串与注释里也出现
        //（`find_pinned` 当场报「命中 2 处，断言指不明是哪一处」）。钉那条 case 臂本身。
        guard_core::find_pinned(&src, "--new)  shift;;").unwrap_or_else(|e| {
            panic!("{e}\n⇒ `--new` 整个删掉了。它该留成 no-op —— 兼容外面已有的调用方。")
        });
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
}
