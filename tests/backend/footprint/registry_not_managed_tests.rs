/// ★ **反向登记：考虑过、但刻意不收进 [`TOOLS`] 的东西**〔devbench F06, 08-10〕。
///
/// # 为什么要有这张表
///
/// [`TOOLS`] 收的是「**app 会碰的东西**」——装得了的、以及装不了但我们要去读/去查的
/// （`K-R60` 起，见模块头注那一节）。而「某个东西不在表里」有两种截然不同的原因：
///
/// | 原因 | 该怎么办 |
/// |---|---|
/// | **没人想起来** | 那是洞，补进 `TOOLS` |
/// | **它压根不是「装到别处的工具」** | 那是分类正确 —— 但**没有任何地方记着这个判断** |
///
/// 第二种今天全靠口口相传：devbench F01 的清单就把 `code-picture` 记成了「不在受管工具
/// 注册表」这个洞，而实测它**不该在**。⇒ 把判断落成表，下次有人问就有答案，
/// 且判据钉住那个理由还在。
///
/// ⚠ **这张表最容易变成许愿池**（什么都往里塞、理由写「暂不支持」）。
/// 所以判据要求每条理由**说清它为什么不属这张表的语义**，而不只是「还没做」。
/// ⚠ 但判据**判不了论证的质量** —— 只能判「有没有在论证那件事」。如实记。
const NOT_MANAGED: &[(&str, &str)] = &[
    (
        "code-picture",
        "**它在 cc-monitor 里零实现，是一个扩展**（一个 skill ＋ 一个 MCP server）—— 它确实「装到别处」，\
         但装它走扩展页已有的资产机制（skill 照 skill 装、MCP 那一项只往目标机写一个 server 条目，\
         程序本身由用户放到那台机器上），是**用法**不是新工具；monitor 与后端都不带它的任何字节。\n\
         ⚠ `K-R65` 当初补的那句道理**仍然成立**：一个名字可以同时是好几样东西，写「表上没有它」的同一拍要把那张表改对（纪律 ⑲）。",
    ),
    (
        "planned-build",
        "**不由 cc-monitor 安装** —— 它是用户自己装在 `<agent 家>/skills/planned-build/` 的 \
         skill，cc-monitor 只**读它的产物**（计划文件）。\n\
         ⇒ 它**不在**本表里（受管工具）。",
    ),
];

/// 受管工具 / 手写环境项的**全集**（本表 ＋ 注册表里各家足迹面带来的那一半）—— 判据按全集判，与搬家前同一个人群。
static TOOLS: std::sync::LazyLock<Vec<crate::agents::footprint_decl::ToolSpec>> =
    std::sync::LazyLock::new(|| {
        crate::footprint::registry::tools()
            .into_iter()
            .cloned()
            .collect()
    });
#[allow(dead_code)]
static UNMANAGED_ENV: std::sync::LazyLock<Vec<crate::agents::footprint_decl::UnmanagedEnv>> =
    std::sync::LazyLock::new(|| {
        crate::footprint::registry::unmanaged()
            .into_iter()
            .cloned()
            .collect()
    });

/// ★ **反向表不许变成许愿池：每条都要论证「为什么不属这张表的语义」。**
///
/// ⚠ 判据能判的只有「有没有在论证那件事」，**判不了论证对不对** —— 如实记在这里，
/// 别把它读成「这些分类判断都被验证过了」。
#[test]
fn every_not_managed_entry_argues_why_it_is_out_of_scope() {
    assert!(
        !NOT_MANAGED.is_empty(),
        "反向表空了 —— 要么真没有刻意排除的东西（那就删掉这张表与本条），\
             要么有人清空了它。本条不许零命中地绿。"
    );
    for (id, why) in NOT_MANAGED {
        assert!(!id.is_empty(), "反向表里有空 id");
        // 「不属本表语义」的论证，至少要谈到这张表管的是什么。
        // ⚠ 关键词是**或**关系：不同的东西有不同的出局理由（不装 / 没落点 / 归别处）。
        let argues = why.contains("语义")
            || why.contains("不由 cc-monitor 安装")
            || why.contains("vendored")
            || why.contains("装到别处");
        assert!(
            argues,
            "`{id}` 的理由没有论证「为什么它不属这张表的语义」，\
                 只说了「还没做」之类 —— 那种东西属于 `installable: false`（如 cc-bus），\
                 不属反向表。\n理由原文：{why}"
        );
    }
}

/// ★ **反向表与正表不许重叠** —— 一个 id 只能在一边。
#[test]
fn not_managed_never_overlaps_the_managed_table() {
    for (id, _) in NOT_MANAGED {
        assert!(
            !TOOLS.iter().any(|t| t.id == *id),
            "`{id}` 同时在 `TOOLS` 与 `NOT_MANAGED` 里 —— \
                 「受管」与「刻意不收」是互斥的，两边都写等于没有判断"
        );
    }
}
