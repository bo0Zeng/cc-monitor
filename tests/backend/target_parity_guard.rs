//! 〔PR1 · 2026-09-24〕**四个 target 横向对等** —— `设计/96 §2` 第 3 层的机器形态。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（各自只许有一句）
//!
//! - **它守的性质是**：四个编译 target 上「做得到」的能力集**两向相等**，每一格差异都在
//!   `lib.rs::TARGET_GAPS` 里有一行理由且分了档（结构上没有 · 欠着），登记表里也没有一行是现推看不出来的。
//! - **它扫的人群是**：`lib.rs::capabilities_on` 从声明现推出来的四份 `(面, 名)`（第 2 层汇总那三面 ＋
//!   帧面 `inbound::REGISTRY` ＋ CLI 面 `SUBCOMMANDS`）× `TARGET_GAPS` 全部行。
//!
//! # 两侧为什么不同源
//!
//! 现推那一侧读的是**实现侧的声明**（面的 `targets` · ccm 那几条的载体 · 命令自己的 `no_tmux` 码 ×
//! 平台档），**一个字都不读** `TARGET_GAPS`（[`the_derivation_never_reads_the_gap_table`] 从源码上钉零命中）。
//! 从登记表减出每个 target 的集合再拿去对登记表，是 `x == x`。
//!
//! # ⚠ 它**买不到**什么（逐条，别读宽）
//!
//! 1. **只看得见声明说得出来的差异。** 声明里没有逐 target 信息的地方，本条判「处处相等」：
//!    - `files-chmod` 在非 unix 上恒回 `io_failed`（`control/files_write.rs::change_mode` 的
//!      `#[cfg(not(unix))]` 那一支）—— 它的 `codes` 说不出「这个平台没有」，本条**看不见**这一格；
//!    - CLI 独有的子命令（如 `--tmux-notify`）没有任何逐 target 声明；
//!    - `files-read` 逐条的 `targets` 本段不读（开门在写区外，理由住 `lib.rs::TARGET_NARROWINGS` 头注）。
//! 2. **判的是「做得到」这根轴的声明，不是「编得过」，更不是「真机上跑得起来」。**
//!    编得过归门禁的 `winchk` / `winchk-backend` / `muslbuild`（**macOS 一格都没有**）；
//!    真机归 `真相源/106` 那种现打。产物层（每个 `[[bin]]` ↔ 入包路线）归
//!    `tests/evidence/K-R124-ruler.py` ⑭，本份不复制。
//! 3. **macOS 那一列是纯声明**：没有产线、没有编译门禁（`设计/96 §7.1.1b` 第 5、6 行），
//!    ⇒ 本条对它判的只是「声明上与 Linux 一样」。
//! 4. **档分得对不对判不了** —— 只判「`why` 的措辞与档不自相矛盾」（判准住 `lib.rs::GapKind` 头注）。
//! 5. **`CCM_TMUX_CARRIED` 是一条手写的机制声明**：往 `ccm-launcher` 加一条靠 tmux 活着的新能力
//!    却没登记进它 ⇒ 那条能力在 Windows 上被现推成「做得到」，本条看不见。
//!    帧面**同形**：一条新命令真起 `tmux` 却没在 `codes` 里声明 `no_tmux`，本条同样看不见。
//!    〔W5-AUX · 96 #7〕这两形的反方向判据已补在本文件末尾（本条仍只读声明，那两条从实现那一侧对声明）：
//!    [`every_frame_command_that_can_reach_tmux_declares_no_tmux`]（文件级可达 ＋ 登记「tmux 可选」）·
//!    [`every_ccm_capability_that_rides_tmux_is_declared_tmux_carried`]（真解析器 ＋ 真计划现推 ＋ 登记例外）。
//!    它们自己的「买不到」写在那一段的段首。

use super::super::{
    capabilities_on, parity_faces, tmux_platform_of, unix_mode_bits_on, GapKind, Target,
    CAPABILITY_FACES, COMMAND_FACES, NO_UNIX_MODE, TARGETS, TARGET_GAPS, TARGET_NARROWINGS,
    TMUX_PLATFORM,
};
use crate::control::ccm::CCM_TMUX_CARRIED;
use std::collections::{BTreeMap, BTreeSet};

/// 「结构上没有」那一档必须说出来的话（任一即可）。
const STRUCTURAL_MARKS: &[&str] = &["不该跨过去", "不跨过去", "不照搬"];
/// 「欠着」那一档必须说出来的话（任一即可）：谁来还 / 什么时候还。
const OWED_MARKS: &[&str] = &["暂时不做", "将来", "后面"];

/// 🔴 **两档各守各的措辞，而且不许串档。**
///
/// - 结构上没有 ⇒ 要说「不跨过去 / 不照搬」，**不许**说「暂时不做」
///   （说了就是在对一条永远不会补的东西许诺，读表的人会去等它）；
/// - 欠着 ⇒ 要说谁来还、什么时候还，**不许**说「不跨过去 / 不照搬」
///   （说了就是把一笔欠账写成了「本来就这样」—— 那正是本枚举要挡的合档）。
#[test]
fn every_gap_speaks_in_the_voice_of_its_own_tier() {
    let mut bad = Vec::new();
    for g in TARGET_GAPS {
        let structural = STRUCTURAL_MARKS.iter().any(|m| g.rationale.contains(m));
        let owed_word = g.rationale.contains("暂时不做");
        let owed = OWED_MARKS.iter().any(|m| g.rationale.contains(m));
        let ok = match g.kind {
            GapKind::Structural => structural && !owed_word,
            GapKind::Owed => owed && !structural,
        };
        if !ok {
            bad.push(format!(
                "{} / {} / {:?} 登记成 {:?}：措辞「结构」{structural} ·「暂时不做」{owed_word} ·「将来怎么还」{owed}",
                g.family, g.capability, g.target, g.kind
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "\n🔴 **差异登记表里有行的措辞与它的档对不上：**\n{}\n\n\
         判准住 `lib.rs::GapKind` 头注：结构上没有 ⇒ 说「不跨过去 / 不照搬」、不许再许「暂时不做」；\n\
         欠着 ⇒ 说谁来还、什么时候还，不许写成「不跨过去」。\n\
         🚫 **不许**为了绿只改措辞不改档（或反过来）—— 先想清楚它到底是哪一档。",
        bad.join("\n")
    );
}

/// 〔TL3 · 审计 F 🔴-11〕V109 点名的那 14 行：`ccm-launcher × Windows` 8 行 ＋ 命令面 × Windows 6 行
/// （**手抄自 V109 原文**「`TARGET_GAPS` 里 `ccm-launcher × Windows` 那 8 行 ＋ 命令面 6 行继续记欠账」，与理由串异源）。
const V109_ROWS: &[(&str, &str)] = &[
    ("ccm-launcher", "tmux"),
    ("ccm-launcher", "attach"),
    ("ccm-launcher", "detach"),
    ("ccm-launcher", "tmux-size"),
    ("ccm-launcher", "tmux-base"),
    ("ccm-launcher", "bus-register"),
    ("ccm-launcher", "ccm-sid"),
    ("ccm-launcher", "base-url-across-tmux"),
    ("wire-commands", "capture-pane"),
    ("wire-commands", "kill"),
    ("wire-commands", "launch"),
    ("cli-subcommands", "--capture-pane"),
    ("cli-subcommands", "--kill"),
    ("cli-subcommands", "--launch"),
];

/// 理由串里一旦出现就等于替用户选了 V109 三选一里的某一种（丙 · 乙 · 甲）。
const MECHANISM_WORDS: &[&str] = &["后台服务", "ConPTY", "控制台窗口本身"];

/// 🔴 〔TL3 · 审计 F 🔴-11〕**理由串不替用户选 Windows 那一族的机制。**
///
/// # 守的要求（住址）
///
/// `设计/99 §1` **V109**〔选〕「先不做 Windows 这一族」—— 题：Windows 上会话要能放后台 / 接回 / 看一眼画面 / 往里送字，
/// 用哪种机制（甲 · 控制台窗口本身就是容器 / 乙 · 常驻后端用 ConPTY 托管 / 丙 · 真 Windows 服务）⇒ 都先不做。
/// `设计/96 §2.3`：「`TARGET_GAPS` 里各行 `why` 的原文仍引 09-21 那句，以 V109 为准」。
/// 出处：审计 F 🔴-11 —— 9 行理由串写着「将来由 **Windows 自己的后台服务** 承担」，等于替用户选了丙。
///
/// # 判（两向相等）
///
/// - 理由串里引 `V109` 的行集合 == [`V109_ROWS`]（V109 原文点名的 14 行）—— 同时是正控：扫描器读得到理由串、认得出子串。
/// - 理由串里点名任一机制（[`MECHANISM_WORDS`]）的行 == ∅。
///
/// 买不到：换一个没登记的说法预设机制（新词）看不见；V109 裁定之后这张表要跟着改（选了哪种，哪一格就能写它）。
#[test]
fn no_gap_rationale_picks_the_windows_mechanism() {
    let cites: BTreeSet<(&str, &str)> = TARGET_GAPS
        .iter()
        .filter(|g| g.target == Target::Windows && g.rationale.contains("V109"))
        .map(|g| (g.family, g.capability))
        .collect();
    let want: BTreeSet<(&str, &str)> = V109_ROWS.iter().copied().collect();
    assert_eq!(
        cites, want,
        "引 V109 的理由串那几行 ≠ V109 原文点名的那 14 行（`ccm-launcher × Windows` 8 ＋ 命令面 6）"
    );
    let picks: Vec<String> = TARGET_GAPS
        .iter()
        .flat_map(|g| {
            MECHANISM_WORDS
                .iter()
                .filter(|w| g.rationale.contains(**w))
                .map(move |w| format!("{} / {} / {:?}：「{w}」", g.family, g.capability, g.target))
        })
        .collect();
    assert!(
        picks.is_empty(),
        "\n🔴 理由串替用户选了 Windows 那一族的机制（V109：甲 · 乙 · 丙都先不做，**机制未定**）：\n{}\n\
         ⇒ 改成「等 V109 那一族选定机制」这一类不预设的说法；用户真裁了哪一种，先改本条的 `MECHANISM_WORDS`。",
        picks.join("\n")
    );
}

/// ★ **两档各要有真成员，而且条数钉死**（恒等计数，动了连理由一起改）。
///
/// 只剩一档时 [`GapKind`] 就是装饰（同 `CapabilityKind` 那条「一个只有一个成员的枚举
/// 承载不了相等断言」）。
///
/// 现打（PR1 落地时）：
/// - **结构 2**：`ccm-launcher` × Windows 的 `tmux-base`（原文逐字「不是推后，是这一条本身不该跨过去」）·
///   `tmux-size`（原文逐字「不照搬这一条」；PR1 把它句尾那个自相矛盾的「暂时不做」摘了，改写成「另立一行」）；
/// - **欠着 12**：同一格的 `tmux` · `attach` · `detach` · `bus-register` · `ccm-sid` · `base-url-across-tmux`
///   （原文都答了「暂时不做 / 将来」—— 用户 09-21 裁「Windows 用 Windows 自己的后台服务，后面再做」）；
///   ＋ 命令面 × Windows 6 条（帧面 `capture-pane` / `kill` / `launch` ＋ CLI 面同名 3 条，子步 3 被横向对等现推出来）。
/// - 〔FW5 · 09-24〕**结构 2 → 4**：多了帧面 `files-chmod` 与 CLI 面 `--files-chmod`（× Windows）。
///   它们不是新裁的差异：`change_mode` 在非 unix 上从来就改不了；FW5 给那条命令声明了 `no_unix_mode` 码，
///   现推段（`unix_mode_bits_on` × 码）才第一次看见它们（`设计/96 §8.4` 买不到 1 · `§8.5` 待拍 3）。
///   档判结构：能力的定义就是「改 unix 权限位」，Windows 没有那套位；那边改访问权限是另一条能力。
#[test]
fn both_tiers_have_real_members_and_their_sizes_are_pinned() {
    let count = |k: GapKind| TARGET_GAPS.iter().filter(|g| g.kind == k).count();
    let (s, o) = (count(GapKind::Structural), count(GapKind::Owed));
    assert_eq!(
        s + o,
        TARGET_GAPS.len(),
        "有一行既不是结构也不是欠着 —— 枚举长出了第三档，本条跟着重判"
    );
    assert_eq!(
        (s, o),
        (4, 12),
        "差异登记表两档现打 结构 {s} · 欠着 {o}（PR1 落地时 2 · 12；FW5 结构 +2 → 4 · 12，逐条见本条头注）。\n\
         这个数本身没有对错，但它变了说明有裁决动过 —— 连理由一起看、一起改。"
    );
}

// ══════════════════ 人群：每个 target 上那一份是**从声明现推**的 ══════════════════

/// 🔴🔴 **现推那一段一个字都不读差异登记表** —— 否则下面那条两向相等是 `x == x`。
///
/// 读的是 `lib.rs` 的**生产段**（剥过注释与测试段）：从 `tmux_platform_of` 起、到
/// `capabilities_on` 那个函数体收口为止，这一整段里 `TARGET_GAPS` **零命中**。
/// ★ 正控：同一份生产段里 `TARGET_GAPS` 确实找得到（那张表的定义本身）——
/// 否则「零命中」可能只是剥法把它剥没了。
#[test]
fn the_derivation_never_reads_the_gap_table() {
    let path = crate::guard_support::src_root().join("lib.rs");
    let raw = std::fs::read_to_string(&path).expect("读 lib.rs");
    let prod = crate::guard_support::production_code(&raw);
    let needle = "TARGET_GAPS";
    assert!(
        prod.matches(needle).count() >= 1,
        "正控失败：生产段里一处 `{needle}` 都找不到 —— 剥法坏了或那张表搬家了，下面的零命中不作数"
    );
    let start = prod
        .find("pub const fn tmux_platform_of")
        .expect("找不到现推那一段的起点 `tmux_platform_of` —— 改名 / 搬家了，本条跟着改锚");
    let head = prod[start..]
        .find("pub fn capabilities_on")
        .expect("找不到现推那一段的终点 `capabilities_on`")
        + start;
    let open = prod[head..]
        .find('{')
        .expect("`capabilities_on` 没有函数体")
        + head;
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in prod[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let section = &prod[start..end.expect("`capabilities_on` 的花括号不配平")];
    // 反空真：这一段真是现推本体，而不是被切成了一截空壳。
    for must in [
        "unavailable_from",
        "TARGET_NARROWINGS",
        "CCM_TMUX_CARRIED",
        "fn capabilities_on",
    ] {
        assert!(
            section.contains(must),
            "现推那一段里没有 `{must}` —— 切段切歪了，下面的零命中不作数"
        );
    }
    assert_eq!(
        section.matches(needle).count(),
        0,
        "\n🔴 **现推那一段读了差异登记表 `{needle}`。**\n\
         那条横向对等断言的两侧是「现推出来的每个 target 上各有哪些」与「登记表」——\n\
         现推要是从登记表减出来，两侧同源，断言恒真（本仓逐字「恒等两侧同源会恒真」）。\n\
         ⇒ 逐 target 的信息只许从实现侧的声明来：面的 `targets` · 那一族自己的逐条 target ·\n\
           命令自己声明的码 × 平台那一维。"
    );
}

/// ★ **收窄只许收，不许编**：每条收窄指着一个真面、一面最多一条，
/// 交出来的名字是那一面 `declares()` 的子集、且不重复。
#[test]
fn every_narrowing_names_a_real_face_and_only_narrows() {
    let faces: BTreeMap<&str, Vec<&str>> =
        parity_faces().map(|f| (f.family, (f.declares)())).collect();
    assert_eq!(
        faces.len(),
        CAPABILITY_FACES.len() + COMMAND_FACES.len(),
        "两张面表之间有族名撞了 —— 按族名找收窄会找到另一面"
    );
    let mut seen = BTreeSet::new();
    for n in TARGET_NARROWINGS {
        assert!(seen.insert(n.family), "`{}` 登记了两条收窄", n.family);
        let declared = faces.get(n.family).unwrap_or_else(|| {
            panic!(
                "收窄 `{}` 指着一个不存在的面 —— 它永远不会被用上，而它看起来像「这一面有逐 target 信息」",
                n.family
            )
        });
        for &t in TARGETS {
            let on = (n.on)(t);
            let set: BTreeSet<&str> = on.iter().copied().collect();
            assert_eq!(
                set.len(),
                on.len(),
                "`{}` 在 {t:?} 上交了重复的名字",
                n.family
            );
            let invented: Vec<_> = set.iter().filter(|x| !declared.contains(x)).collect();
            assert!(
                invented.is_empty(),
                "`{}` 在 {t:?} 上交了它没声明过的名字：{invented:?} —— 收窄只许收，不许编",
                n.family
            );
        }
    }
}

/// ★ **两根平台轴在本机这一格对得上**：[`Target`] 轴给本机的档 == 本机真编进去的 `TMUX_PLATFORM`。
///
/// Windows 那一格由 `lib.rs` 里 `#[cfg(windows)]` 那条编译期断言钉（本机门禁上它不存在）。
#[test]
fn the_target_axis_agrees_with_the_host_tmux_platform() {
    let host = if cfg!(windows) {
        Target::Windows
    } else if cfg!(target_os = "macos") {
        Target::MacOs
    } else if cfg!(target_env = "musl") {
        Target::LinuxMusl
    } else {
        Target::LinuxGnu
    };
    assert_eq!(
        tmux_platform_of(host),
        TMUX_PLATFORM,
        "本机是 {host:?}：Target 轴说它是一档，编进去的 `TMUX_PLATFORM` 说是另一档 —— 两根平台轴分叉了"
    );
}

/// ★〔FW5〕**unix 权限位那根轴在本机这一格对得上**：[`unix_mode_bits_on`] 给本机的答案 ==
/// 这份测试二进制真编进去的 `cfg(unix)`（`change_mode` 走哪一支看的就是它）。
///
/// Windows 那一格由 `lib.rs` 里 `#[cfg(windows)]` 那条编译期断言钉（本机门禁上它不存在）。
#[test]
fn the_unix_mode_axis_agrees_with_what_this_binary_was_compiled_with() {
    let host = if cfg!(windows) {
        Target::Windows
    } else if cfg!(target_os = "macos") {
        Target::MacOs
    } else if cfg!(target_env = "musl") {
        Target::LinuxMusl
    } else {
        Target::LinuxGnu
    };
    assert_eq!(
        unix_mode_bits_on(host),
        cfg!(unix),
        "本机是 {host:?}：Target 轴说它{}有 unix 权限位，编进去的 `cfg(unix)` 说相反 —— 两根轴分叉了",
        if unix_mode_bits_on(host) { "" } else { "没" }
    );
    // 码的两份字面量逐字相等（`lib.rs` 那份给现推用，写面那份给 `WriteRefusal::code` 用 ——
    // 后者不借前者，理由住 `control/files_write.rs::NO_UNIX_MODE` 头注）。
    assert_eq!(
        crate::control::files_write::WriteRefusal::Unsupported(String::new()).code(),
        NO_UNIX_MODE,
        "写面回的那个码与 target 轴现推读的那个码不是同一个串 —— 两份字面量漂开了"
    );
    // 那个码真有人声明（否则现推那一维对空集恒成立）：恰好 `files-chmod` 一条。
    let declaring: Vec<&str> = crate::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&NO_UNIX_MODE))
        .map(|s| s.name)
        .collect();
    assert_eq!(
        declaring,
        vec!["files-chmod"],
        "声明 `{NO_UNIX_MODE}` 的命令不是恰好 `files-chmod` 一条 —— 多了要连登记表一起看，少了现推那一维就空转"
    );
}

/// ★ `CCM_TMUX_CARRIED` 里每一条都是 `ccm-launcher` 真声明过的能力（不许有幽灵）。
#[test]
fn every_tmux_carried_ccm_capability_really_exists() {
    let ghosts: Vec<_> = CCM_TMUX_CARRIED
        .iter()
        .filter(|c| !crate::control::ccm::CAPABILITIES.contains(c))
        .collect();
    assert!(
        ghosts.is_empty(),
        "`CCM_TMUX_CARRIED` 里这几条 `ccm-launcher` 根本没声明：{ghosts:?} —— 改名 / 删了之后这里没跟"
    );
}

/// ★ 命令面那两面也要说自己在哪些 target 上、依据是什么（同 `CAPABILITY_FACES` 那条地板），
/// 而且每一面**四个 target 上都不是空的**（反空真：人群空了，横向对等对空集恒成立）。
#[test]
fn every_parity_face_is_populated_on_every_target() {
    for f in COMMAND_FACES {
        assert!(
            f.target_basis.chars().count() >= 30,
            "命令面 `{}` 的 `target_basis` 太短 —— 写不出依据的话，那个 `targets` 是抄上去的",
            f.family
        );
        assert!(
            crate::guard_support::src_root()
                .join(f.declared_in)
                .is_file(),
            "命令面 `{}` 说它住 `{}`，盘上没有这份文件",
            f.family,
            f.declared_in
        );
    }
    for &t in TARGETS {
        let on = capabilities_on(t);
        for f in parity_faces() {
            assert!(
                on.iter().any(|(fam, _)| *fam == f.family),
                "`{}` 在 {t:?} 上一条都不剩 —— 那不是差异，是这一面在这个 target 上整面不在，\
                 该从它的 `targets` 里摘掉（整面摘与逐条豁免在读数上长得一样，只有一个说的是真话）",
                f.family
            );
        }
    }
}

// ══════════════════ 🔴🔴 正题：四个 target 的能力集两向相等，差异 == 登记表 ══════════════════

/// 四个 target 以 `设计/96` 为准（`§2.9` 那张表：Linux musl / native · Windows · macOS）。
///
/// ⚠ 这里用一个**穷尽的 `match`** 而不是再抄一份名单：[`Target`] 长出第五个成员时本函数编不过，
/// 于是「`TARGETS` 忘了加它」这一形（新 target 整列不在人群里，对等断言对它全称成立）没有机会发生。
fn every_variant_is_in_targets(t: Target) -> bool {
    match t {
        Target::LinuxGnu | Target::LinuxMusl | Target::Windows | Target::MacOs => {
            TARGETS.contains(&t)
        }
    }
}

/// 🔴🔴 **`设计/96 §2` 第 3 层**：四个 target 的能力集**完全相等**，
/// 不相等的每一格都要在 `TARGET_GAPS` 里有一行（理由 ＋ 档），**两向**。
///
/// # 两侧从哪来（刻意异源）
///
/// | 一侧 | 来源 | 谁会改它 |
/// |---|---|---|
/// | **现推的差异** | `capabilities_on(t)`：面的 `targets` · ccm 那几条的载体 · 命令自己声明的 `no_tmux` 码 × 平台档 | 加命令 / 加能力 / 改实现的那一路 |
/// | **登记的差异** | `TARGET_GAPS` | 裁「为什么做不到、哪一档」的那一拍 |
///
/// 现推那一段不读登记表，由 [`the_derivation_never_reads_the_gap_table`] 从源码上钉。
///
/// # 两向各治一形
///
/// - **现推有、登记没有** ⇒ 有一个 target 上少了一条能力，**没有人说过为什么** ——
///   本波加命令的那几路，谁的新命令声明了 `no_tmux`，合并那一拍就红在这里（红对了）；
/// - **登记有、现推没有** ⇒ 登记表说做不到，声明侧却看不出差异 ⇒ 要么已经做到了
///   （删那一行 —— 同 `P19` 删 `agent` 那一形），要么声明侧把这件事丢了。
#[test]
fn every_target_has_the_same_capabilities_except_the_registered_gaps() {
    for &t in TARGETS {
        assert!(every_variant_is_in_targets(t));
    }
    assert_eq!(
        TARGETS.len(),
        4,
        "target 全集现打 {} 个（`设计/96` 定的是四个：Linux gnu · Linux musl · Windows · macOS）。\n\
         变了就回那一篇核「四个」还成不成立，再改这个数。",
        TARGETS.len()
    );
    for t in [
        Target::LinuxGnu,
        Target::LinuxMusl,
        Target::Windows,
        Target::MacOs,
    ] {
        assert!(every_variant_is_in_targets(t), "`TARGETS` 里没有 {t:?}");
    }

    let per: BTreeMap<Target, BTreeSet<(&str, &str)>> = TARGETS
        .iter()
        .map(|&t| {
            let v = capabilities_on(t);
            let set: BTreeSet<_> = v.iter().copied().collect();
            assert_eq!(set.len(), v.len(), "{t:?} 上现推出来有重复的 `(面, 名)`");
            (t, set)
        })
        .collect();
    let union: BTreeSet<(&str, &str)> = per.values().flatten().copied().collect();

    // ★ 反空真 ①：并集就是「声明过的全部」—— 一条声明过、却在**每一个** target 上都被收窄掉的能力，
    //   不在并集里，也就不会出现在任何差异里（它在四个 target 上「都没有」也算「相等」）。
    let declared: BTreeSet<(&str, &str)> = parity_faces()
        .flat_map(|f| (f.declares)().into_iter().map(move |n| (f.family, n)))
        .collect();
    let nowhere: Vec<_> = declared.difference(&union).collect();
    assert!(
        nowhere.is_empty(),
        "这几条声明了、却在四个 target 上**一个都不在**：{nowhere:?}\n\
         ⇒ 它们不会出现在任何差异里（处处没有也是「处处相等」）—— 要么收窄写错了，要么这条能力该删。"
    );
    // ★ 反空真 ②：人群不是空的，而且三类面都在里面（第 2 层汇总 · 帧面 · CLI 面）。
    for fam in [
        "ccm-launcher",
        "files-read",
        "stream-flags",
        "wire-commands",
        "cli-subcommands",
    ] {
        assert!(
            union.iter().any(|(f, _)| *f == fam),
            "并集里没有 `{fam}` 这一面 —— 人群缺了一块，横向对等对缺的那块全称成立"
        );
    }

    let derived: BTreeSet<(&str, &str, Target)> = per
        .iter()
        .flat_map(|(&t, here)| union.difference(here).map(move |&(f, n)| (f, n, t)))
        .collect();
    let registered: BTreeSet<(&str, &str, Target)> = TARGET_GAPS
        .iter()
        .map(|g| (g.family, g.capability, g.target))
        .collect();
    assert_eq!(
        registered.len(),
        TARGET_GAPS.len(),
        "`TARGET_GAPS` 里有重复的 (面, 能力, target) —— 一格登记了两次，条数会虚报"
    );

    let unexplained: Vec<_> = derived.difference(&registered).collect();
    let unwitnessed: Vec<_> = registered.difference(&derived).collect();
    assert!(
        unexplained.is_empty() && unwitnessed.is_empty(),
        "\n🔴 **四个 target 的能力集对不上，而差异登记表没有说清。**\n\n\
         现推有、登记没有（**有个 target 少了一条能力，没人说过为什么**）：\n  {unexplained:?}\n\
         ⇒ 往 `lib.rs::TARGET_GAPS` 补一行：理由（为什么做不到 ＋ 将来怎么办）与档\n\
           （`GapKind::Structural` 结构上没有 · `GapKind::Owed` 欠着，判准在 `GapKind` 头注）。\n\
           或者 —— 它其实做得到 ⇒ 回去改声明（命令的 `codes` / `CCM_TMUX_CARRIED` / 面的 `targets`）。\n\n\
         登记有、现推没有（**登记说做不到，声明侧看不出差异**）：\n  {unwitnessed:?}\n\
         ⇒ 已经做到了 ⇒ 删那一行（同 `P19` 删 `agent`：减一条要写清根因没了、真机那一维买没买到）；\n\
           或者声明侧把这件事丢了 ⇒ 把它补回声明里。\n\n\
         🚫 **不许**把登记表改成从 `capabilities_on` 算出来的东西 —— 两侧同源，本条当场恒真。"
    );
}

// ══════════════ 〔W5-AUX · 96 #7〕机制声明的反方向（本文件头注「买不到」第 5 条的兑现）══════════════
//
// 要求住址：`设计/96 §2.3`「买不到」第 2 条逐字「**机制声明没有反方向判据**：`CCM_TMUX_CARRIED` 漏登一条靠 tmux 的新能力、
// 或一条新命令真起 `tmux` 却没声明 `no_tmux`，看不见」。现推（`capabilities_on`）只读声明 —— 声明漏一格，
// 那条能力在 Windows 上就被现推成「做得到」，而上面那条两向相等照样绿。下面两条从**实现**那一侧去对声明：
//
// - **帧面**：`inbound::REGISTRY` 每条命令的处理器（`run:` 闭包里点名的 `crate::…` 路径）所在文件，沿**文件级引用图**
//   （`use` / `crate::` / `super::` / 本文件声明的子模块）能不能走到一份**真起 `tmux`** 的生产文件（`Command::new("tmux")`，
//   或经 `platform::shell::posix_shell` 送一段带 `tmux ` 的脚本）。够得着的 == 声明 `no_tmux` 的 ∪ 登记的「够得着但 tmux 只是可选的」。
// - **ccm 面**：`CAPABILITIES` 每一条配一个最小探针 argv，经**真解析器 ＋ 真计划**（`argv::parse` → `plan::build`，纯、不起进程）
//   看它落在哪条路上：接回 / 容器 ⇒ 靠 tmux；直路 ⇒ 它在直路上必须**有效果**（计划与基线不同），否则它离了 tmux 什么都不做。
//   靠 tmux 的 ∪ 登记的例外 == `CCM_TMUX_CARRIED`。
//
// ⚠ 买不到（如实）：
// - 帧面的引用图是**文件级**的（拿不到函数级调用图）：同一份文件里「只用了一个常量」也算够得着 ⇒ 多判不少判，
//   多出来的进登记表逐条写理由；经函数指针 / trait 对象注入的调用（`inbound` 递给处理器的闭包）看不见。
// - `Run::Builtin` 与就地应答的那几条没有 `crate::` 路径，不在射程（逐条登记，新来一条没路径的会红）。
// - ccm 面判的是「落在哪条路上」与「直路上有没有效果」，**不是**「在 Windows 上真跑得起来」（真机维一格没有）；
//   平台原语那一层（读别的进程的环境）计划里看不见 —— `ccm-sid` 正是这一形，登记在例外表里。

/// 一棵源码树：相对路径 → 生产段（剥注释与测试段）。纯数据，判据与合成正控共用下面那几个函数。
type Tree = std::collections::BTreeMap<String, String>;

/// 后端 crate 的库那棵树（`main.rs` 是二进制根，不在库的模块树里 ⇒ 不收）。
fn backend_tree() -> Tree {
    // 走仓里唯一的遍历口径（`scanning_guard_registry` 那条纪律）；本文件住 `tests/backend/`，不在被扫的树里。
    let root = crate::guard_support::src_root();
    guard_core::scan_tree!(&root, &["rs"])
        .into_iter()
        .map(|(p, raw)| {
            let rel = p
                .strip_prefix(&root)
                .expect("在根下")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, guard_core::production_code(&raw))
        })
        .filter(|(rel, _)| rel != "main.rs")
        .collect()
}

/// `control/gate.rs` → `["control", "gate"]`；`files/mod.rs` → `["files"]`；`lib.rs` → `[]`。
fn module_of(rel: &str) -> Vec<String> {
    let mut segs: Vec<String> = rel
        .trim_end_matches(".rs")
        .split('/')
        .map(String::from)
        .collect();
    if matches!(segs.last().map(String::as_str), Some("mod") | Some("lib")) {
        segs.pop();
    }
    segs
}

/// 一条路径的最长模块前缀落在哪份文件。
fn file_of(
    mods: &std::collections::BTreeMap<Vec<String>, String>,
    path: &[String],
) -> Option<String> {
    (1..=path.len())
        .rev()
        .find_map(|n| mods.get(&path[..n]).cloned())
}

/// `use a::{b, c::{d, e}};` 展开成 `a::b` · `a::c::d` · `a::c::e`（输入已去空白）。
fn expand_use(u: &str) -> Vec<String> {
    let Some(open) = u.find('{') else {
        return vec![u.to_string()];
    };
    let Some(close) = u.rfind('}').filter(|c| *c > open) else {
        return vec![u.to_string()];
    };
    let (pre, inner) = (&u[..open], &u[open + 1..close]);
    let mut parts = Vec::new();
    let (mut depth, mut cur) = (0i32, String::new());
    for c in inner.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
        .iter()
        .flat_map(|p| expand_use(&format!("{pre}{p}")))
        .collect()
}

/// 一份文件的生产段里提到的路径（`crate::…` / `super::…` / `self::…` / 本文件 `mod x;` 声明的子模块 `x::…`），
/// 以及 `use` 语句展开后的每一条。返回解析成绝对模块路径之后的段列表。
fn paths_in(me: &[String], src: &str) -> Vec<Vec<String>> {
    let children: std::collections::BTreeSet<String> = src
        .split("mod ")
        .skip(1)
        .filter_map(|t| {
            let name: String = t
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            t[name.len()..]
                .trim_start()
                .starts_with(';')
                .then_some(name)
        })
        .collect();
    let absolutize = |segs: Vec<String>| -> Option<Vec<String>> {
        let first = segs.first()?.as_str();
        match first {
            "crate" => Some(segs[1..].to_vec()),
            "super" | "self" => {
                let mut base = me.to_vec();
                let mut i = 0;
                while i < segs.len() && (segs[i] == "super" || segs[i] == "self") {
                    if segs[i] == "super" {
                        base.pop();
                    }
                    i += 1;
                }
                base.extend(segs[i..].iter().cloned());
                Some(base)
            }
            c if children.contains(c) => {
                let mut base = me.to_vec();
                base.extend(segs.iter().cloned());
                Some(base)
            }
            _ => None,
        }
    };
    let mut out = Vec::new();
    // ① `use …;` 展开（花括号分组）。只认语句开头的 `use`（前一个字符是空白 / `;` / `{` / `}` / `)`），
    //    而且 `;` 之前只许是路径字符 —— 字符串里一句 `… use …` 的散文不算。
    let bytes = src.as_bytes();
    let mut from = 0;
    while let Some(off) = src[from..].find("use ") {
        let at = from + off;
        from = at + 4;
        if at > 0
            && !matches!(
                bytes[at - 1],
                b' ' | b'\n' | b'\t' | b';' | b'{' | b'}' | b')'
            )
        {
            continue;
        }
        let Some(end) = src[at + 4..].find(';') else {
            break;
        };
        let u: String = src[at + 4..at + 4 + end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if u.is_empty()
            || !u.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            || !u
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_:{},*".contains(c))
        {
            continue;
        }
        for e in expand_use(&u) {
            let segs: Vec<String> = e
                .split("::")
                .filter(|s| !s.is_empty() && *s != "*")
                .map(String::from)
                .collect();
            if let Some(p) = absolutize(segs) {
                out.push(p);
            }
        }
    }
    // ② 代码里的 `a::b::c` 链。
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let start_ok = c.is_ascii_alphabetic()
            && (i == 0
                || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b':'));
        if !start_ok {
            i += 1;
            continue;
        }
        let mut segs = Vec::new();
        let mut j = i;
        loop {
            let s = j;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            segs.push(src[s..j].to_string());
            if j + 2 < b.len()
                && b[j] == b':'
                && b[j + 1] == b':'
                && (b[j + 2].is_ascii_alphabetic() || b[j + 2] == b'_')
            {
                j += 2;
                continue;
            }
            break;
        }
        if segs.len() > 1 {
            if let Some(p) = absolutize(segs) {
                out.push(p);
            }
        }
        i = j.max(i + 1);
    }
    out
}

/// 文件级引用图：文件 → 它引用的别的文件。
fn file_edges(
    tree: &Tree,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let mods: std::collections::BTreeMap<Vec<String>, String> =
        tree.keys().map(|r| (module_of(r), r.clone())).collect();
    tree.iter()
        .map(|(rel, src)| {
            let me = module_of(rel);
            let to = paths_in(&me, src)
                .into_iter()
                .filter_map(|p| file_of(&mods, &p))
                .filter(|f| f != rel)
                .collect();
            (rel.clone(), to)
        })
        .collect()
}

/// 真起 `tmux` 的生产文件：直呼 `Command::new("tmux")`，或经 shell 适配口送一段带 `tmux ` 的脚本。
fn tmux_spawning_files(tree: &Tree) -> std::collections::BTreeSet<String> {
    tree.iter()
        .filter(|(_, src)| {
            let squeezed: String = src.chars().filter(|c| !c.is_whitespace()).collect();
            squeezed.contains("Command::new(\"tmux\")")
                || (src.contains("posix_shell(") && src.contains("tmux "))
        })
        .map(|(r, _)| r.clone())
        .collect()
}

/// 从一组起点文件沿引用图走完，交回够得着的文件（含起点）。
fn reachable(
    edges: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    from: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut todo: Vec<String> = from.iter().cloned().collect();
    while let Some(f) = todo.pop() {
        if seen.insert(f.clone()) {
            todo.extend(edges.get(&f).into_iter().flatten().cloned());
        }
    }
    seen
}

/// `inbound.rs` 生产段里 `REGISTRY` 每一条的 `run:` 闭包点名的 `crate::…` 路径所在文件（命令名 → 文件集）。
fn handler_files(
    tree: &Tree,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    let src = tree.get("inbound.rs").expect("后端树里没有 inbound.rs");
    let at = src
        .find("pub const REGISTRY: &[CommandSpec] = &[")
        .expect("`inbound.rs` 里锚不住 REGISTRY");
    let mods: std::collections::BTreeMap<Vec<String>, String> =
        tree.keys().map(|r| (module_of(r), r.clone())).collect();
    let mut out = std::collections::BTreeMap::new();
    for block in src[at..].split("CommandSpec {").skip(1) {
        let Some(n) = block.find("name: \"") else {
            continue;
        };
        let name: String = block[n + 7..].chars().take_while(|c| *c != '"').collect();
        let Some(r) = block.find("run:") else {
            continue;
        };
        let run = &block[r..];
        let files: std::collections::BTreeSet<String> = paths_in(&[], run)
            .into_iter()
            .filter_map(|p| file_of(&mods, &p))
            .filter(|f| f != "inbound.rs")
            .collect();
        out.insert(name, files);
    }
    out
}

/// 够得着 tmux、却**不**声明 `no_tmux` 的命令 —— 逐条写理由（tmux 在它那里是可选的：问不到就降级，命令本身照做）。
const REACHES_TMUX_WITHOUT_NO_TMUX: &[(&str, &str)] = &[
    ("bus-broadcast", "同 `bus-list`：挑在线的那一步读 `live`，问不到 tmux 就当「不知道谁在线」如实回，不回 `no_tmux`"),
    ("bus-kill", "同 `bus-list`（转调 `cc-kill`；tmux 只用来挂 `live`）"),
    ("bus-list", "`control/cc_bus.rs::agents_via_cc_list` 经 `gate::list_sessions().ok()` 挂「还活着吗」那一栏 —— 问不到回 `live: null`（「不假装知道」），命令本身照做；它的能力是转调 cc-bus，不是 tmux"),
    ("bus-send", "同 `bus-list`（投递转调 `cc-send`；tmux 只用来挂 `live`）"),
    ("bus-spawn", "同 `bus-list`（转调 `cc-spawn`；那个子进程自己起 tmux，本进程只经 gate 挂 `live`）"),
    ("bus-state", "同 `bus-list`（`agents` 那一半就是 `bus-list` 那一个函数）"),
    (
        "ccm-print",
        "〔合并主线时本条当场点出〕W5-ALIAS 的别名预览：`control/ccm/mod.rs::plan_of` 经 \
         `session_snapshot::global().taken_names().ok()` 问一次会话快照做铸名避让 —— 问不到 ⇒ `None` ⇒ 不退让\
         （`plan::build` 头注的诚实降级），预览照出；它的能力是渲计划，不是 tmux",
    ),
];

/// `run:` 里没有 `crate::…` 路径的命令（`Run::Builtin` 与就地应答）—— 不在射程，逐条登记。
const NO_HANDLER_PATH: &[(&str, &str)] = &[
    (
        "cancel",
        "`Run::Builtin`：`inbound::dispatch` 的硬臂，只动在飞表",
    ),
    (
        "link-close",
        "`Run::Builtin`：链路四条住 `inbound::dispatch` 的硬臂（〔SR1a〕）",
    ),
    ("link-credit", "同上"),
    ("link-data", "同上"),
    ("link-open", "同上"),
    ("ping", "就地应答"),
    (
        "transfer-download",
        "传输台那几条住 `inbound::dispatch` 的硬臂",
    ),
    ("transfer-start", "同上"),
    ("transfer-stop", "同上"),
    ("transfer-upload", "同上"),
];

/// 🔴 帧面：够得着 tmux 的命令 == 声明 `no_tmux` 的 ∪ 登记的「tmux 可选」。
#[test]
fn every_frame_command_that_can_reach_tmux_declares_no_tmux() {
    use std::collections::BTreeSet;
    let tree = backend_tree();
    assert!(
        tree.len() >= 60,
        "后端树只收到 {} 份 —— 遍历坏了，下面全称恒真",
        tree.len()
    );
    let edges = file_edges(&tree);
    let spawners = tmux_spawning_files(&tree);
    // 地板只防人群塌成空集；主锚是下面两向相等。现打（立格那一拍）：7 份直呼 + 1 份经 shell（`observe/watcher.rs`）。
    assert!(
        spawners.len() >= 5,
        "真起 tmux 的文件只找到 {spawners:?} —— 取法坏了"
    );
    let handlers = handler_files(&tree);
    let names: BTreeSet<&str> = crate::inbound::REGISTRY.iter().map(|s| s.name).collect();
    assert_eq!(
        handlers.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        names,
        "从源码里抽到的命令名与 `REGISTRY` 不一致 —— 抽块坏了"
    );
    let unresolved: BTreeSet<&str> = handlers
        .iter()
        .filter(|(_, f)| f.is_empty())
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        unresolved,
        NO_HANDLER_PATH.iter().map(|(n, _)| *n).collect::<BTreeSet<_>>(),
        "`run:` 里没有 `crate::…` 路径的命令变了 —— 新来的那条它的处理器住哪、够不够得着 tmux，本条看不见：\
         登记进 `NO_HANDLER_PATH` 并写清为什么不起 tmux，或者把处理器挪进一个模块"
    );
    let reach: BTreeSet<&str> = handlers
        .iter()
        .filter(|(_, from)| !reachable(&edges, from).is_disjoint(&spawners))
        .map(|(n, _)| n.as_str())
        .collect();
    let declared: BTreeSet<&str> = crate::inbound::REGISTRY
        .iter()
        .filter(|s| s.codes.contains(&super::super::NO_TMUX))
        .map(|s| s.name)
        .collect();
    let excused: BTreeSet<&str> = REACHES_TMUX_WITHOUT_NO_TMUX
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert!(
        declared.is_disjoint(&excused),
        "登记成「tmux 可选」的命令又声明了 `no_tmux`：{:?} —— 两句话自相矛盾，删一句",
        declared.intersection(&excused).collect::<Vec<_>>()
    );
    let want: BTreeSet<&str> = declared.union(&excused).copied().collect();
    let undeclared: Vec<_> = reach.difference(&want).collect();
    let ghosts: Vec<_> = want.difference(&reach).collect();
    assert!(
        undeclared.is_empty() && ghosts.is_empty(),
        "\n帧面「真够得着 tmux」与声明对不上。\n\
         ① 够得着、却既没声明 `no_tmux` 也没登记：{undeclared:?}\n\
            ⇒ 🔴 它在 Windows 上会被现推成「做得到」（`capabilities_on` 只读声明）。tmux 是它的必需品 ⇒ 在 `codes` 里声明 `no_tmux`；\
         只是可选的（问不到就降级、命令照做）⇒ 登记进 `REACHES_TMUX_WITHOUT_NO_TMUX` 写清降级成什么。\n\
         ② 声明了 / 登记了、却根本够不着：{ghosts:?}\n\
            ⇒ 声明是鬼影（它会把一条在 Windows 上做得到的命令报成做不到），或者引用图取法坏了。\n\
         真起 tmux 的文件：{spawners:?}"
    );
}

/// 🔴 正反两控：引用图在合成树上认得出 `use` 分组 / `super::` / `crate::` 链 / 子模块，也不乱连。
#[test]
fn the_tmux_reach_ruler_works_on_a_synthetic_tree() {
    let tree: Tree = [
        ("lib.rs", "pub mod a; pub mod b; pub mod c; pub mod d;"),
        ("a.rs", "use crate::{c, b::inner};\npub fn h() { c::go(); }"),
        ("b/mod.rs", "pub mod inner;\npub fn quiet() {}"),
        ("b/inner.rs", "pub fn go() { super::super::d::spawn(); }"),
        ("c.rs", "pub fn go() {}"),
        (
            "d.rs",
            "pub fn spawn() { let _ = std::process::Command::new(\"tmux\"); }",
        ),
        ("e.rs", "pub fn lonely() {}"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let edges = file_edges(&tree);
    let spawners = tmux_spawning_files(&tree);
    assert_eq!(
        spawners.into_iter().collect::<Vec<_>>(),
        vec!["d.rs".to_string()]
    );
    let from = |f: &str| std::iter::once(f.to_string()).collect();
    // 阳：a 经 `use crate::{…, b::inner}` → inner 经 `super::super::d` → d。
    assert!(
        reachable(&edges, &from("a.rs")).contains("d.rs"),
        "分组 use ＋ super 链没接上：{edges:?}"
    );
    // 阴：c 与 e 谁也不引用 ⇒ 够不着。
    assert!(!reachable(&edges, &from("c.rs")).contains("d.rs"));
    assert!(!reachable(&edges, &from("e.rs")).contains("d.rs"));
    // 阴：b/mod.rs 只声明了子模块 `inner`、没用它 ⇒ 不连（「声明子模块」不等于「引用它」）。
    assert!(
        !edges["b/mod.rs"].contains("b/inner.rs"),
        "只声明没使用的子模块被当成了引用"
    );
}

/// ccm 面每条能力的最小探针（位置动作 ＋ 旗标；`carriers` = 探针里为了让它起得来而**必须**带上的 tmux 载体旗标）。
const CCM_PROBES: &[(&str, &[&str], &[&str])] = &[
    ("account", &["--account", "acct-x"], &[]),
    ("account-via-backend", &["--account", "acct-x"], &[]),
    ("agent", &["--agent", "codex"], &[]),
    ("attach", &["attach", "foo"], &[]),
    (
        "backend-discover",
        &["resume", "11111111-2222-3333-4444-555555555555"],
        &[],
    ),
    (
        "bus-register",
        &["--tmux", "--detach", "--bus-register"],
        &["--tmux", "--detach"],
    ),
    ("cwd", &["--cwd", "/srv"], &[]),
    ("detach", &["--tmux", "--detach"], &["--tmux"]),
    ("launcher", &["--launcher", "claude-nightly"], &[]),
    ("model", &["--model", "opus"], &[]),
    (
        "resume",
        &["resume", "11111111-2222-3333-4444-555555555555"],
        &[],
    ),
    ("tmux", &["--tmux"], &[]),
    ("tmux-base", &["--tmux-base", "proj"], &[]),
    (
        "tmux-size",
        &["--tmux", "--tmux-size", "80x24"],
        &["--tmux"],
    ),
];

/// 计划那一层判不了的几条：`(能力, 算不算靠 tmux, 为什么)`。
const CCM_TMUX_EXCEPTIONS: &[(&str, bool, &str)] = &[
    (
        "base-url-across-tmux",
        true,
        "不是一个旗标，是容器路的一条性质（`plan.rs` 容器分支把 `ANTHROPIC_BASE_URL` 显式带进载荷内侧）—— 名字就是跨 tmux 的边界，探针非带 `--tmux` 不可、带了就恒判「靠 tmux」，判了等于没判",
    ),
    (
        "ccm-sid",
        true,
        "直路上**有**效果（〔S5〕启动期令牌，计划与基线不同 ⇒ 计划层判它「不靠 tmux」），但 Windows 上后端读不到别的进程的环境 ⇒ 认不回 agent 进程 —— 平台原语那一层计划里看不见，归 tmux 档（`control/ccm/mod.rs::CCM_TMUX_CARRIED` 那一行的依据）",
    ),
    ("new", false, "默认动作本身：探针与基线同形是定义使然（「直路上有没有效果」对它不成立）"),
    ("print", false, "渲染方式（吐出计划还是执行它），不进计划 —— 计划与基线同形是定义使然"),
];

/// 🔴 ccm 面：真靠 tmux 的能力（真解析器 ＋ 真计划现推）∪ 登记的例外 == `CCM_TMUX_CARRIED`。
#[test]
fn every_ccm_capability_that_rides_tmux_is_declared_tmux_carried() {
    use crate::control::ccm::argv::{parse, Parsed};
    use crate::control::ccm::plan::{build, Account, AccountTable, Env, Plan};
    use std::collections::BTreeSet;
    let env = Env {
        home: "/home/pi".into(),
        pwd: "/p".into(),
        accts_manifest: "/nonexistent/accounts.json".into(),
        account_env: "CLAUDE_CONFIG_DIR".into(),
        self_argv: vec!["/usr/local/bin/ccm".into()],
        ..Default::default()
    };
    // 账号那一格要一个**真在盘上**的目录（`resolve_account` 会看它在不在）：借后端源码根（一定在，不建不删 ——
    // 建临时目录会让本文件在测试层分区里从「源码扫描」升成「集成」，`test_tiers` 那张表跟着要挪）。
    let acct_dir = crate::guard_support::src_root();
    let table = AccountTable::from_accounts(vec![Account {
        name: "acct-x".into(),
        config_dir: Some(acct_dir.to_string_lossy().into_owned()),
        is_default: false,
    }]);
    let plan_of = |args: &[&str]| -> Result<Plan, String> {
        let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse(&a) {
            Ok(Parsed::Opts(o)) => build(&o, &env, &table, None).map_err(|d| d.0),
            Ok(Parsed::Early(e)) => Err(format!("落进了立即结束那一支：{e:?}")),
            Err(d) => Err(d.0),
        }
    };
    let caps: BTreeSet<&str> = crate::control::ccm::CAPABILITIES.iter().copied().collect();
    let probed: BTreeSet<&str> = CCM_PROBES.iter().map(|(c, ..)| *c).collect();
    let excepted: BTreeSet<&str> = CCM_TMUX_EXCEPTIONS.iter().map(|(c, ..)| *c).collect();
    assert!(
        probed.is_disjoint(&excepted),
        "同一条能力既有探针又登记成例外：{:?}",
        probed.intersection(&excepted).collect::<Vec<_>>()
    );
    assert_eq!(
        probed.union(&excepted).copied().collect::<BTreeSet<_>>(),
        caps,
        "探针表 ∪ 例外表 与 `ccm-launcher` 的 `CAPABILITIES` 不相等 —— 新加的能力要配一个探针（或进例外表写清为什么计划层判不了）"
    );
    let baseline = plan_of(&[]).expect("基线（什么都不给）起不来 —— 夹具坏了");
    let mut derived: BTreeSet<&str> = BTreeSet::new();
    for (cap, argv, carriers) in CCM_PROBES {
        let p = plan_of(argv)
            .unwrap_or_else(|e| panic!("能力 `{cap}` 的探针 {argv:?} 起不来：{e} —— 探针写错了"));
        for c in *carriers {
            let without: Vec<&str> = argv.iter().copied().filter(|t| t != c).collect();
            assert!(
                plan_of(&without).is_err(),
                "能力 `{cap}` 的探针带了载体旗标 `{c}`，拿掉它照样起得来 ⇒ 那个旗标是白带的，本条会把它判成「靠 tmux」而冤枉它"
            );
        }
        let rides = matches!(p, Plan::Attach { .. } | Plan::Container(_));
        if !rides {
            assert_ne!(
                p, baseline,
                "能力 `{cap}` 的探针 {argv:?} 落在直路上，而计划与基线一模一样 ⇒ 离了 tmux 它什么都不做：\
                 要么它其实靠 tmux（登记进 `CCM_TMUX_CARRIED`），要么它是个声明了却不起作用的鬼影"
            );
        }
        if rides {
            derived.insert(cap);
        }
    }
    let want: BTreeSet<&str> = derived
        .union(
            &CCM_TMUX_EXCEPTIONS
                .iter()
                .filter(|(_, t, _)| *t)
                .map(|(c, ..)| *c)
                .collect(),
        )
        .copied()
        .collect();
    let declared: BTreeSet<&str> = CCM_TMUX_CARRIED.iter().copied().collect();
    assert_eq!(
        want, declared,
        "\n真靠 tmux 的 ccm 能力（真解析器 ＋ 真计划现推，∪ 登记的例外）与 `CCM_TMUX_CARRIED` 不相等。\n\
         多出来的（现推靠 tmux、却没登记）⇒ 🔴 它在 Windows 上被现推成「做得到」—— 补进 `control/ccm/mod.rs::CCM_TMUX_CARRIED`。\n\
         少了的（登记了、现推却落在直路上且有效果）⇒ 那一行登记冤枉了它，或者它的依据在计划层看不见（进例外表写清）。"
    );
}
