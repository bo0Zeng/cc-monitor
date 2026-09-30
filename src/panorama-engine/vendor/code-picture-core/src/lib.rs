//! code-picture 只读内核。Phase 1:符号索引 + 锚点解析,不写任何用户文件(仅 `.codepicture/`)。
//!
//! 批注 / 文档关联的写拆成「算」与「写盘」两层(`edits`):别的写者只拿算好的内容,自己落盘。

pub mod anchor;
pub mod annotations;
pub mod cfg;
pub mod diagram;
pub mod docs;
pub mod edits;
pub mod engine;
pub mod git;
pub mod graph;
pub mod index;
pub mod lang;
pub mod mcp;
pub mod model;
pub mod pdg;
pub mod precise;
pub mod rank;
pub mod scan;
pub mod symbols;
#[cfg(test)]
mod wire_schema;

pub use engine::{Engine, EngineOpts};
pub use model::*;
