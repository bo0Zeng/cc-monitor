//! 〔RM1c · 第四波〕代码全景**经那台机器的后端**走（用户 09-24 V108 选 B）。
//!
//! 一条命令 `panorama_call(origin, op, repo, args)`：按 origin 找那台的长连接，发帧命令 `panorama`，
//! 拿回 `data.result` 原样交给前端。后端那一侧经插件通用调用口起只装引擎的独立小程序
//! （`cc-monitor-panorama`），解析在那台机器上、那个进程里；线上只走查询语义与结构化结果。
//!
//! # 这一拍接谁
//!
//! 前端今天只在**远端** origin 上用它（`src/panorama/api.ts`）；本机那一条仍是进程内那 23 条
//! （`panorama.rs`），第二拍（本机对称、monitor 摘内嵌引擎）才改走这里 —— 那一拍要先答
//! 「本机的批注写走哪」（记录在 `调研/第四波记录/RM1c.md`）。命令本身对 origin 不做假设。
//!
//! # 期限
//!
//! 帧那一跳的期限必须**长于**后端给子进程的期限（后端那一侧先说 `timed_out`，这边才听得到原因，
//! 而不是自己先超时、说一句「没回来」）。两档与后端适配层那两档对拍
//! （`tests::the_budgets_outlast_the_backend_deadlines`，运行时读后端源码，异源）。
//!
//! # 诚实边界
//!
//! - 打不断：后端那一侧是阻塞档（`cancel` 回 `not_cancellable`）；这边等到期限为止。
//! - 远端第一拍**只读**：批注 / 文档关联的写不在后端 op 词表里（前端那一侧当场拒并说清）。

use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 建索引那一档的 op（后端给子进程 900 s）。
pub(crate) const BUILD_OPS: &[&str] = &["index", "reindex"];

/// 建索引那一档：后端给子进程 900 s，再留 60 s 给回程。
const BUILD_BUDGET: Duration = Duration::from_secs(960);
/// 其余查询：后端给子进程 60 s（另有 10 s 探测），再留 30 s。
const QUERY_BUDGET: Duration = Duration::from_secs(100);

/// 这个 op 等多久。
pub(crate) fn budget_for(op: &str) -> Duration {
    if BUILD_OPS.contains(&op) {
        BUILD_BUDGET
    } else {
        QUERY_BUDGET
    }
}

/// 拼帧载荷（纯函数）。`repo` / `args` 缺席就不带这一格（后端对缺席与 `null` 同一口径）。
pub(crate) fn frame_args(op: &str, repo: Option<&str>, args: Option<Value>) -> Value {
    let mut a = json!({ "op": op });
    if let Some(r) = repo {
        a["repo"] = json!(r);
    }
    if let Some(x) = args {
        a["args"] = x;
    }
    a
}

/// 问 `origin` 那台机器的后端做一次全景查询，拿回 `result`。
#[tauri::command]
pub async fn panorama_call(
    origin: Origin,
    op: String,
    repo: Option<String>,
    args: Option<Value>,
) -> Result<Value, String> {
    let _ = origin.route("panorama_call")?;
    let data = crate::backend::control::frame_query::call(
        &origin,
        "panorama",
        frame_args(&op, repo.as_deref(), args),
        budget_for(&op),
    )
    .await?;
    data.get("result").cloned().ok_or_else(|| {
        format!(
            "远端 [{}] `panorama` 的应答没有 `result` —— 两端契约对不上",
            origin.as_wire_str()
        )
    })
}

#[cfg(test)]
#[path = "../../../tests/bridge/panorama_call_tests.rs"]
mod tests;
