//! **Codex 的 `token_count` 事件 → token 增量**的唯一映射。
//!
//! # 这个 crate 今天是什么
//!
//! 它只有一件事：把 Codex 的 token 用量子对象（`last_token_usage` / `total_token_usage`）
//! 映射成三个量（`input` / `cache_read` / `output`），**并且替调用方做那个减法**
//! —— Codex 的 `input_tokens` **含** cached，而本仓的 token 口径 `input` **不含**。
//!
//! # 🔴 它原来叫 `usage-core`，头注讲的是另一件事 —— 那段话今天已经失效
//!
//! 原头注整篇在论证一件事：**同一套 Claude 用量口径写了两遍** ——
//! monitor 侧一份（走 `parse_line` / `JsonlRecord`）、daemon 侧一份
//! （在裸 `serde_json::Value` 上抽取），那个双写没有护栏、而且已经漂开
//! （daemon 剥 BOM、monitor 不剥）⇒ 建这个 crate 就是为了让那条口径只有一个家。
//!
//! **`设计/50`（删用量）把那两侧同时删了**：用量的**聚合轴**（②）与**探针轴**（③）
//! 整轴退役 —— `usage.rs` 与 `usage_query.rs` 两份文件都不存在了。
//! ⇒ 那段头注讲的病、它举的证、它给出的结论，**今天一个字都不成立**：
//! 没有双写点，因为两个写点都没了。整段随 Claude 那半一起删掉，不留在这里当考古 ——
//! 它论证的是一个 crate 为什么该存在，而这个 crate 今天存在的理由**换了**。
//!
//! # 于是这里只剩 Codex 这一半，crate 也跟着改名 `codex-token-core`
//!
//! 「usage」这个名字在本仓今天专指已经退役的那两轴；留着它会让下一个人以为
//! 这份 crate 还管着聚合。**名字必须说今天的实话**（`设计/91 R1`）。
//!
//! # 诚实边界：它今天的消费者有几个
//!
//! `src/backend/agents/codex/parse.rs::last_token_delta` 是**唯一**的生产调用点。
//! ⚠ 原头注还点名过 monitor 的 `codex_record.rs` 「各写一遍」——
//! 现打：那份文件今天**只有一句注释**提到 `codex_delta`，**没有调用**。
//! ⇒ 「跨半共用」这个说法今天不准确，如实写在这里。
//!
//! # 依赖约束（这一条原样保留，它仍然成立）
//!
//! **依赖只有 `serde_json`。** daemon 是 Linux-only 的静态 musl 二进制、且刻意不在
//! monitor 的 workspace 里；一旦这里引入 tauri / tokio / 平台相关的东西，共享就破了。

use serde_json::Value;

/// Codex 一次 `token_count` 事件映射进 Claude 口径后的**增量**。
///
/// 字段名与 `Totals` 同名同义，好让「哪个增量加到哪个字段」在调用处一眼可读 ——
/// 收口前两侧各写一遍 `b.input += inp.saturating_sub(cached)`，错位加也不会有人发现。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodexDelta {
    pub input: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl CodexDelta {
    /// 全零 = no-op 事件（真机见于会话起始、`turn_context` 之前）。
    /// 调用方**必须**先判它再 `entry().or_default()`，否则凭空造一个全零 ghost 桶。
    pub fn is_noop(self) -> bool {
        self.input == 0 && self.cache_read == 0 && self.output == 0
    }
}

/// ★ **Codex 用量口径的唯一权威源**：字段名 + 「input 不含 cached」这个减法。
///
/// 入参是 token 用量子对象（`last_token_usage` / `total_token_usage`），缺字段按 0。
///
/// Codex 的 `input_tokens` **含** cached，Claude 口径的 `input` 不含 ⇒ 必须减，
/// 否则 cached 既进 input 又进 cache_read，**用量凭空翻倍且不报错**。
/// `reasoning_output_tokens` 是 output 的子集、`total_tokens` 冗余 ⇒ 都不单列。
///
/// 来历：`U7-2` 把 Claude 口径收进本 crate 时**漏了 Codex 这一侧** ——
/// daemon 的 `agents/codex/parse.rs` 与 monitor 的 `codex_record.rs` 各写一份、逐字相同、
/// 无一条判据钉住，后来才收到这里。
/// 🔴 **〔`设计/50`〕守它「是唯一家」的那条判据没了**：它住 `src/bridge/src/usage.rs`
/// （`kou_jing_singleton`），而那份文件随用量 ② 轴整轴退役 ——  〔散文墓碑〕
/// **如实登记为射程边界**：今天挡「有人再写第二份」的只有下面那三条单测钉住的行为，
/// 没有任何东西在数「这个映射有几个家」。
pub fn codex_delta(usage: &Value) -> CodexDelta {
    let g = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
    let cached = g("cached_input_tokens");
    CodexDelta {
        input: g("input_tokens").saturating_sub(cached),
        cache_read: cached,
        output: g("output_tokens"),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/crates/codex-token-core/lib_tests.rs"]
mod tests;
