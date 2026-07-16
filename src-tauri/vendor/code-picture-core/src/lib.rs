//! code-picture 只读内核。Phase 1:符号索引 + 锚点解析,不写任何用户文件(仅 `.codepicture/`)。

pub mod anchor;
pub mod annotations;
pub mod docs;
pub mod engine;
pub mod git;
pub mod graph;
pub mod index;
pub mod lang;
pub mod mcp;
pub mod model;
pub mod rank;
pub mod scan;
pub mod symbols;

pub use engine::{Engine, EngineOpts};
pub use model::*;
