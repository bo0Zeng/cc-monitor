use super::NOT_MANAGED;

/// 受管工具 / 手写环境项的**全集**（本表 ＋ 注册表里各家足迹面带来的那一半）—— 判据按全集判，与搬家前同一个人群。
static TOOLS: std::sync::LazyLock<Vec<crate::footprint::registry::ToolSpec>> =
    std::sync::LazyLock::new(|| {
        crate::footprint::registry::tools()
            .into_iter()
            .cloned()
            .collect()
    });
#[allow(dead_code)]
static UNMANAGED_ENV: std::sync::LazyLock<Vec<crate::footprint::registry::UnmanagedEnv>> =
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
        assert!(
            why.len() > 80,
            "`{id}` 的理由只有 {} 字节 —— 分类判断要写清楚，否则下一个人还得重新查一遍",
            why.len()
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
