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
//!    帧面**同形**：一条新命令真起 `tmux` 却没在 `codes` 里声明 `no_tmux`，本条同样看不见 ——
//!    今天没有判据拿 `codes` 去对「谁真 `Command::new("tmux")`」（现打：`main_fourth_face_tests`
//!    只判「声明了的码在 `codes` 里」，不判反方向）。

use super::super::{
    capabilities_on, parity_faces, tmux_platform_of, GapKind, Target, CAPABILITY_FACES,
    CCM_TMUX_CARRIED, COMMAND_FACES, TARGETS, TARGET_GAPS, TARGET_NARROWINGS, TMUX_PLATFORM,
};
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
        let structural = STRUCTURAL_MARKS.iter().any(|m| g.why.contains(m));
        let owed_word = g.why.contains("暂时不做");
        let owed = OWED_MARKS.iter().any(|m| g.why.contains(m));
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
        (2, 12),
        "差异登记表两档现打 结构 {s} · 欠着 {o}（PR1 落地时 2 · 12，逐条见本条头注）。\n\
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
