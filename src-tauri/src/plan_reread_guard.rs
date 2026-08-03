//! **铁律 11 的机检**：那四条定框原则不许从主计划里消失，且「待裁决」必须指名 §。
//!
//! # 为什么要机检一条「流程纪律」
//!
//! Phase F 要求「回看并更新主计划」。实际执行了 15 轮，**做成了单向写入** ——
//! 每轮只往主计划追加（账本行、变更记录），**§0.0 与 §1.1–§1.5 从头到尾没被完整读过一次**。
//! 后果：连着四次把主计划**逐字写着的原则**当成「待用户裁决」上报，
//! 给 U10 / U11 / U9b / U12 各挂了一条**假阻塞**。
//!
//! 「以后我仔细点」不是修法（散文纪律 = 没有纪律）。本模块钉两件能钉的：
//!
//! 1. **四条定框原则的原句还在主计划里** —— 它们是判断「这是不是假阻塞」的标尺，
//!    标尺被人删掉/改写，铁律 11 就空转了；
//! 2. **功能件里写「待裁决」/「阻塞」的，必须在同一份文件里指名 `§`** ——
//!    指不出来就是把「还没做」写成了「还没定」。
//!
//! ⚠ **它钉不住「读没读」** —— 那件事在源码里不可表示。它钉的是**读了才写得出的东西**
//! （引用 §）与**读了就不会丢的东西**（原则原句）。**比没有强，别读成证明。**

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn plan_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .join(".claude/planned-build/unified-backend")
    }

    /// 四条定框原则的**原句片段**（2026-08-01 定框、2026-08-03 被我当成待裁决那四条）。
    ///
    /// 取的是短而独特的片段 —— 太长会被正常润色打断，太短会误命中。
    const FRAMING_CLAUSES: &[(&str, &str)] = &[
        (
            "ccm 是 backend 的客户端（不是第三个宿主）",
            "不是第三个宿主",
        ),
        ("ccm 是执行臂不是宿主（§1.3）", "它是执行臂，不是宿主"),
        (
            "改状态的 tmux 命令一律归 control/（§1.1）",
            "一律归 `control/`",
        ),
        ("一份代码两种承载（§0.0）", "两种承载"),
        ("本机 backend 是安装包的一部分（§1.2）", "安装包的一部分"),
    ];

    /// ★ 标尺不许消失：四条定框原则的原句必须还在主计划里。
    #[test]
    fn the_framing_clauses_are_still_in_the_masterplan() {
        let plan =
            fs::read_to_string(plan_dir().join("MASTERPLAN.md")).expect("读不到 MASTERPLAN.md");
        assert!(
            plan.len() > 50_000,
            "主计划只有 {} 字节，像是抽错了文件",
            plan.len()
        );
        let missing: Vec<&str> = FRAMING_CLAUSES
            .iter()
            .filter(|(_, needle)| !plan.contains(needle))
            .map(|(name, _)| *name)
            .collect();
        assert!(
            missing.is_empty(),
            "主计划里找不到这些**定框原句**：{missing:?}\n\
             它们是判断「这是不是假阻塞」的标尺（铁律 11 §0.0b）。\
             真要改措辞，把本表一起改 —— 但先确认你不是在删掉一条已经定过的原则。"
        );
    }

    /// ★ 铁律 11：功能件里写「待裁决」/「阻塞」的，必须在同一份文件里指名 `§`。
    ///
    /// 指不出 §，就说明没去对过定框 —— 那多半是把「还没做」写成了「还没定」。
    #[test]
    fn every_feature_doc_that_claims_a_blocker_cites_a_section() {
        let dir = plan_dir().join("features");
        let mut checked = 0usize;
        let mut offenders = Vec::new();
        for e in fs::read_dir(&dir).expect("features/ 读不到").flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x != "md") {
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let src = fs::read_to_string(&p).unwrap_or_default();
            checked += 1;
            let claims = src.contains("待裁决") || src.contains("阻塞");
            if claims && !src.contains('§') {
                offenders.push(name);
            }
        }
        assert!(
            checked >= 10,
            "只扫到 {checked} 份功能件 —— 遍历坏了，本条会零命中变绿"
        );
        assert!(
            offenders.is_empty(),
            "这些功能件声称有「待裁决/阻塞」，却一处 `§` 都没引：{offenders:?}\n\
             铁律 11：**「未实现」≠「未裁定」** —— 指名 §0/§1 里**哪一条没覆盖它**；\n\
             指不出来就说明它是**活**，不是阻塞，本轮就该做。"
        );
    }

    /// ★ 执行口还在：`STATUS.md` 的**恢复入口顶部**必须写着那条「每轮必做」。
    ///
    /// loop 每轮唯一必读的就是 STATUS —— 这条掉了，铁律 11 就没有触达点。
    #[test]
    fn the_status_entry_point_still_carries_the_reread_rule() {
        let status = fs::read_to_string(plan_dir().join("STATUS.md")).expect("读不到 STATUS.md");
        let head: String = status.lines().take(40).collect::<Vec<_>>().join("\n");
        for needle in ["每轮开工前必做", "MASTERPLAN.md", "铁律 11"] {
            assert!(
                head.contains(needle),
                "STATUS.md 的**前 40 行**里找不到 `{needle}` —— \
                 那条「每轮通读 §0.0 + §1.1–§1.5」的触达点没了。\
                 它必须在恢复入口**顶部**（loop 每轮唯一必读的地方），埋在中间等于没有。"
            );
        }
    }
}
