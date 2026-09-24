//! 〔PR1 · 2026-09-24〕**四个 target 横向对等** —— `设计/96 §2` 第 3 层的机器形态。
//!
//! # `K-G6` `KG62`：性质与人群，两行逐字（各自只许有一句）
//!
//! - **它守的性质是**：`lib.rs::TARGET_GAPS` 这张差异登记表里的每一行都分了档（结构上没有 · 欠着），
//!   而且每一档的措辞与它的档对得上。
//! - **它扫的人群是**：`lib.rs::TARGET_GAPS` 的全部行。
//!
//! # ⚠ 它**买不到**什么
//!
//! 1. **档分得对不对，本份判不了** —— 它只判「`why` 的措辞与档不自相矛盾」。
//!    一条欠账被登记成「结构上没有」而措辞也跟着改了，本份照样绿；
//!    那一格要人读（`GapKind` 头注写着判准）。
//! 2. 「这条能力在那个 target 上真做不到」要真机，本份同 `capability_ledger_guard` 一样够不着。

use super::super::{GapKind, TARGET_GAPS};

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
/// - **欠着 6**：同一格的 `tmux` · `attach` · `detach` · `bus-register` · `ccm-sid` · `base-url-across-tmux`
///   （原文都答了「暂时不做 / 将来」—— 用户 09-21 裁「Windows 用 Windows 自己的后台服务，后面再做」）。
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
        (2, 6),
        "差异登记表两档现打 结构 {s} · 欠着 {o}（PR1 落地时 2 · 6，逐条见本条头注）。\n\
         这个数本身没有对错，但它变了说明有裁决动过 —— 连理由一起看、一起改。"
    );
}
