//! 一次失败**给人看的那一句 ＋ 下层原话**：类型住 `copy_core::said`（后端与共享契约 crate 同一份）；
//! 这里只多一口要日志库的 [`IntoNote`]。
//!
//! 后端里原来回 `String` 的失败要带原话时换成它：`?` 从 `String` 进来（没有原话的那几形）不改调用点。

pub(crate) use copy_core::said::Said;

/// 这次失败**不成应答**、只成别处的一句（成功应答里的提示格 · 状态格 · 日志行里拼的一段，那一格没有详情位）：
/// 那一句交出去，原话记一行日志（不上句子、也不丢）。成应答的那几形走 `Fail::from((码, Said))`。
pub(crate) trait IntoNote {
    fn into_note(self) -> String;
}

impl IntoNote for Said {
    fn into_note(self) -> String {
        if let Some(r) = &self.raw {
            tracing::warn!("{}: {r}", self.said);
        }
        self.said
    }
}
