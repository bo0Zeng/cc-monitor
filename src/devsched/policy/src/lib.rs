//! 开发调度的排除清单 · 送出清单 · 信任级别：什么出本机、出到哪台。

mod exclude;
mod send;
mod trust;

pub use exclude::ExcludeList;
pub use send::send_list;
pub use trust::{may_dispatch, Trust};
