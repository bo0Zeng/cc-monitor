//! 认行层：给一行（`serde_json::Value` / 原字节）认出它是什么 —— 纯函数 ＋ 小状态，不碰文件。

pub(crate) mod facts;
pub(crate) mod index_row;
pub(crate) mod turns;
pub(crate) mod user_inputs;
