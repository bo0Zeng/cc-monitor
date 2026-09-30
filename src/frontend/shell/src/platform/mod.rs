//! 壳（monitor）这一侧的**系统适配层**〔RE · 收尾重排 · `设计/15 §5.3 C6` ＋ `设计/90 §4` 阶段 H 的第一格〕。
//!
//! 与后端 `src/backend/platform/` 同一条纪律（`backend-split` C10）：平台原语与平台 cfg 的唯一住址。
//! 今天只有 [`fs`]（原 `platform_fs.rs`，只挪住址）；壳里别处的平台 cfg 还没收进来 —— 那是阶段 H 余下的解耦活，
//! 不是这一刀（诚实边界照 [`fs`] 头注「10g」：没有判据钉「平台原语只许住这里」）。

pub mod fs;
pub mod proc;
