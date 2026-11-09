//! 需手动那一族在后端里传的格（不上线）：线上那一半住 `cmd_wire::needs`。
//! 计划那一侧（`plan::needs::need_items`）算好、观测面拼整份需手动清单（`observe::recognize::facts::needs_list`）时收；
//! 两边都引帧面零件、谁也不引谁 ⇒ 住这里。

/// 计划里要人看的一条：哪个工作区 · 哪一片 · 落在哪一格（没有格可落 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedAt {
    pub workspace: String,
    pub slice: String,
    pub cell: Option<cmd_wire::needs::NeedsCell>,
}
