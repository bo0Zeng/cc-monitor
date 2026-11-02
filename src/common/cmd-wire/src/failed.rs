//! **交出去的那一形失败**：全仓只此一份定义。帧面的失败应答 · CLI 面的失败信封 · 通道上的拒绝体 · 壳命令的失败 ·
//! 一组文件操作没成的那一下 · 手机核心交给 Kotlin 的那一行都是它（界面与手机的类型从这里生成）。
//! 进程里的失败是 `copy_core::said::Said`（码可缺 · 原话另放）；投出去那一下才成这一形（码补齐 · 原话进复制详情）。

use serde::{Deserialize, Serialize};

/// 码缺着的那一次失败投成应答时补的码（协议文档里的兜底码）：线上那一格的值，全仓只此一处。
pub const FALLBACK_CODE: &str = "failed";

/// 码给程序认；`said` 是给人看的那一句（不含码与下层原话）；`detail` 是复制详情那几行（时刻 · 机器 · 命令 · 码 · 原话，
/// 写的那一端排好、读的那一端原样复制；这一侧自己说的、没有详情 ⇒ 空串）；`data` 只给按码定了形的那几个码。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Failed {
    pub code: String,
    pub said: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "unknown"))]
    pub data: Option<serde_json::Value>,
}

impl Failed {
    /// 只有一句话（这一侧说的：没有对端给的码 ⇒ 兜底码；没有详情）。
    pub fn local(said: String) -> Failed {
        Failed {
            code: FALLBACK_CODE.to_string(),
            said,
            detail: String::new(),
            data: None,
        }
    }

    /// 对端给的码缺着（没走到命令那一层）⇒ 兜底码。
    pub fn code_or_fallback(code: Option<String>) -> String {
        code.filter(|c| !c.is_empty())
            .unwrap_or_else(|| FALLBACK_CODE.to_string())
    }
}

/// 这一侧自己说的那一句（[`Failed::local`]）。
impl From<String> for Failed {
    fn from(said: String) -> Failed {
        Failed::local(said)
    }
}

impl From<&str> for Failed {
    fn from(said: &str) -> Failed {
        Failed::local(said.to_string())
    }
}

/// 写出来就是给人看的那一句（详情另取）。
impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/failed_tests.rs"]
mod tests;
