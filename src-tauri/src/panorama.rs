//! Batch15-P0：code-picture 代码全景可视化后端（feasibility spike）。
//!
//! 引 vendored `code-picture-core` crate `use Engine`（**不走 MCP**——MCP 是 stdio agent
//! head,不适合宿主内嵌;README「嵌入为库」段即为此）。`Engine` 内含 `rusqlite::Connection`
//! + tree-sitter,是 `Send` 但**非 `Sync`** → 命令走 `spawn_blocking` 包索引/查询重活,绝不
//! 跨 await 持有;Engine 不在 await 边界存活(P0 每次 open 用完即弃,惰性池留 P1)。
//!
//! **P0 只落最小闭环**（`panorama_overview` = index + overview）验证 vendored 内核在 Windows
//! release 全链编译过 + 构建时间可接受。完整命令族（per-repo Engine 惰性池 + node/callers/
//! callees/impact/docs_for/search/symbols_touching）留 P1。

use code_picture_core::{Engine, EngineOpts, Overview, TokenBudget};
use std::path::Path;

/// P0 最小命令:对给定仓根建索引并返回 `Overview`(脊柱文件 / 子系统 / 入口点 / 覆盖信号）。
/// `Overview` 全字段 `#[derive(Serialize)]`（上游 F15 已备），可直接回前端。索引会往 `repo`
/// 写 `.codepicture/` 派生索引（非 Claude 数据源,与 §1 只读铁律无关;cc-monitor 自身被索引
/// 时生成的 `.codepicture/` 已 gitignore）。
#[tauri::command]
pub async fn panorama_overview(repo: String) -> Result<Overview, String> {
    tokio::task::spawn_blocking(move || {
        let mut engine = Engine::open(Path::new(&repo), EngineOpts {})
            .map_err(|e| format!("打开 code-picture 引擎失败（{repo}）: {e}"))?;
        engine.index().map_err(|e| format!("索引失败: {e}"))?;
        Ok::<Overview, String>(engine.overview(TokenBudget(8000)))
    })
    .await
    .map_err(|e| format!("全景任务失败: {e}"))?
}
