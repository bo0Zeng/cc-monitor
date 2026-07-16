//! Batch15-P1：code-picture 代码全景可视化后端。
//!
//! 引 vendored `code-picture-core` crate `use Engine`（**不走 MCP**——MCP 是 code-picture 的
//! Agent head,给 Claude 编码时用;这里是 Human head,cc-monitor 顶栏按钮 → invoke → 直调库画图）。
//!
//! **State 拓扑 = per-repo Engine 模块-static 池**（照 sftp_pool,**非 Tauri State、不进
//! STATE-MATRIX**——见 masterplan §9;融合手册推荐托管 State,但模块-static 绕开其警告的
//! app.manage 运行时 panic 坑 + 天然 per-repo 惰性多仓）。
//!
//! **线程模型（手册 §2）**：`Engine` 是 `Send` 但**非 `Sync`**（内含 rusqlite Connection +
//! RefCell 缓存;**连读查询也写 RefCell 缓存 → 必须 `Mutex` 独占,不能 `RwLock` 多读**）。
//! 所有命令 `async` + `spawn_blocking`（SQLite 阻塞）,闭包内 `arc.lock()` 用完即 drop,
//! **guard 不跨 await**。`std::MutexGuard` deref 成 `&mut Engine` → 读（`&self`）/写
//! （`&mut self`:index/reindex）方法无差别都能调。
//!
//! **索引时机**：`Engine::open` 加载既存 `.codepicture/index.db`（上次索引持久）或空;
//! `panorama_index`/`reindex` 显式建/重建（重活,前端开面板时调）。索引写被索引仓的
//! `.codepicture/` 派生侧车,与 §1 只读铁律无关（cc-monitor 自身被索引生成的已 gitignore）。

use code_picture_core::{model, Engine, EngineOpts};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// per-repo Engine 池（照 sftp_pool 两级锁）。key = repo 根路径。
fn pool() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<Engine>>>> {
    static P: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Engine>>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取/建某仓的 Engine（`open` 很轻,不索引）。**pool 全局锁只短暂持有取/回填,绝不跨
/// `Engine::open`**（照 sftp_pool R3 教训 + 手册 §2:慢活/SQLite 别持全局锁）——只在
/// `spawn_blocking` 线程里被 `with_engine` 调用（`Engine::open` 的 SQLite 打开也在阻塞线程）。
/// key 走 `canonicalize` 消化尾斜杠/`.`/相对-绝对差异,**防同仓不同写法建重复 Engine**
/// （否则两条 rusqlite 连接对同一 index.db 并发写 → SQLITE_BUSY + 缓存不一致,D-建议2）。
fn engine_for(repo: &str) -> Result<Arc<Mutex<Engine>>, String> {
    let key = std::fs::canonicalize(repo).map_err(|e| format!("仓路径无效（{repo}）: {e}"))?;
    // 快路径:池命中（短暂持锁,无 open）。
    if let Some(e) = pool().lock().unwrap().get(&key) {
        return Ok(Arc::clone(e));
    }
    // 慢路径:`open` 在池锁**外**（open 期间不持全局锁,别的仓可并发首开）。
    // F68 re-vendor:EngineOpts 从空壳变带 `store_dir`。`None` = 索引落被分析仓
    // `<repo>/.codepicture`(现状,.gitignore 已忽略);要集中到 cc-monitor 数据目录改 Some。
    let engine = Engine::open(&key, EngineOpts { store_dir: None })
        .map_err(|e| format!("打开 code-picture 引擎失败（{repo}）: {e}"))?;
    let arc = Arc::new(Mutex::new(engine));
    // 回填 + double-check:open 期间别的线程可能已建同 key → 先到者胜,弃本次多开的（罕见）。
    Ok(Arc::clone(pool().lock().unwrap().entry(key).or_insert(arc)))
}

/// 通用：在某仓 Engine 上跑闭包（`spawn_blocking` + 独占锁,不跨 await）。闭包收 `&mut Engine`
/// ——读（`&self`）/写（`&mut self`）方法都能调。**`engine_for`（含 `Engine::open` 的 SQLite/
/// 建目录）也在 `spawn_blocking` 内跑**,不占 async 执行器线程（手册 §2）。
async fn with_engine<T, F>(repo: String, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut Engine) -> T + Send + 'static,
{
    tokio::task::spawn_blocking(move || -> Result<T, String> {
        let arc = engine_for(&repo)?;
        let mut g = arc.lock().unwrap();
        Ok(f(&mut g))
    })
    .await
    .map_err(|e| format!("panorama 任务失败: {e}"))?
}

/// 索引状态（cc-monitor 自建 DTO → camelCase;core 直出类型保持 snake_case,见手册 §7.3）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanoramaStatus {
    stale: bool,
    indexed_at: Option<u64>, // unix 秒
    symbols: usize,
}

/// 建索引（重活:tree-sitter 解析全仓 → SQLite）。前端开面板时调 + loading。
#[tauri::command]
pub async fn panorama_index(repo: String) -> Result<model::IndexStats, String> {
    with_engine(repo, |e| e.index().map_err(|e| e.to_string())).await?
}

/// 重建索引（改代码后刷新;只写 `.codepicture/index.db`,非侵入）。core 里 `reindex()` 当前
/// 就是 `index()` 全量重建（D-建议3:功能同 `panorama_index`）;两命令名对前端语义更清晰
/// （index=首建 / reindex=刷新）,保留两个。
#[tauri::command]
pub async fn panorama_reindex(repo: String) -> Result<model::IndexStats, String> {
    with_engine(repo, |e| e.reindex().map_err(|e| e.to_string())).await?
}

/// 索引新鲜度 + 上次索引时间 + 符号总数（陈旧提示 / 状态栏）。
#[tauri::command]
pub async fn panorama_status(repo: String) -> Result<PanoramaStatus, String> {
    with_engine(repo, |e| PanoramaStatus {
        stale: e.is_stale(),
        indexed_at: e.indexed_at(),
        symbols: e.symbol_count(),
    })
    .await
}

/// 项目全景（脊柱文件 + 子系统聚类 + 入口点 + 覆盖信号）。`budget` 控 token 预算裁剪。
#[tauri::command]
pub async fn panorama_overview(
    repo: String,
    budget: Option<usize>,
) -> Result<model::Overview, String> {
    with_engine(repo, move |e| {
        e.overview(model::TokenBudget(budget.unwrap_or(4000)))
    })
    .await
}

/// 单符号详情（符号 + 直接 callers/callees + 关联文档 + 批注）。`symbol` 用全限定 id。
#[tauri::command]
pub async fn panorama_node(
    repo: String,
    symbol: String,
) -> Result<Option<model::NodeView>, String> {
    with_engine(repo, move |e| e.node(&symbol)).await
}

/// 以某符号为心的双向邻域子图（节点集 + 边集,画局部调用图）。
#[tauri::command]
pub async fn panorama_subgraph(
    repo: String,
    symbol: String,
    depth: u32,
) -> Result<model::SubGraph, String> {
    with_engine(repo, move |e| e.subgraph(&symbol, depth)).await
}

/// 反向调用边（谁调用了它,BFS 到 depth）。
#[tauri::command]
pub async fn panorama_callers(
    repo: String,
    symbol: String,
    depth: u32,
) -> Result<Vec<model::Edge>, String> {
    with_engine(repo, move |e| e.callers(&symbol, depth)).await
}

/// 正向调用边（它调用了谁,BFS 到 depth）。
#[tauri::command]
pub async fn panorama_callees(
    repo: String,
    symbol: String,
    depth: u32,
) -> Result<Vec<model::Edge>, String> {
    with_engine(repo, move |e| e.callees(&symbol, depth)).await
}

/// 改动某符号的 blast-radius（反向可达的全部传递调用者 + 最短反向 depth）。
#[tauri::command]
pub async fn panorama_impact(repo: String, symbol: String) -> Result<model::ImpactSet, String> {
    with_engine(repo, move |e| e.impact(&symbol)).await
}

/// 按名子串搜符号 → 拿全限定 id（前端拿 id 后再查 node/callers/…;裸名不解析）。
#[tauri::command]
pub async fn panorama_search(
    repo: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<model::Symbol>, String> {
    with_engine(repo, move |e| e.search(&query, limit.unwrap_or(30))).await
}

/// 覆盖某符号的 `.md` 文档链接（显示关联文档）。
#[tauri::command]
pub async fn panorama_docs_for(
    repo: String,
    symbol: String,
) -> Result<Vec<model::DocLink>, String> {
    with_engine(repo, move |e| e.docs_for(&symbol)).await
}

/// ⭐ P3 护城河缝：一组文件/行 → 命中的符号 id。cc-monitor 从 jsonl 的 Edit/Write 拿「agent
/// 刚改了哪些文件行」喂进来 → 前端把这些 id 在全景图上高亮 =「agent 正在改这几个节点」。
/// `ranges` 空 → 视作整文件所有符号（v1 无精确行号时的回退）。只读。
#[tauri::command]
pub async fn panorama_touching(
    repo: String,
    files: Vec<String>,
    ranges: Vec<(usize, usize)>, // 1-based [start,end];空则整文件
) -> Result<Vec<String>, String> {
    with_engine(repo, move |e| {
        let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
        let ranges: Vec<model::LineRange> = ranges
            .into_iter()
            .map(|(start, end)| model::LineRange { start, end })
            .collect();
        e.symbols_touching(&files, &ranges)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_pool_same_repo_same_arc_distinct_repo_distinct_arc() {
        // 池 get-or-create 语义:同 repo 返同一 Arc（命中,不重复 open）;异 repo 返不同 Arc。
        // 用临时目录当 repo 根（Engine::open 会在其下建 .codepicture/,临时目录 OS 自清）。
        let base = std::env::temp_dir();
        let r1 = base.join("cc-monitor-b15-pool-a");
        let r2 = base.join("cc-monitor-b15-pool-b");
        std::fs::create_dir_all(&r1).ok();
        std::fs::create_dir_all(&r2).ok();
        let a1 = engine_for(r1.to_str().unwrap()).expect("open r1");
        let a1b = engine_for(r1.to_str().unwrap()).expect("open r1 again");
        let a2 = engine_for(r2.to_str().unwrap()).expect("open r2");
        assert!(
            Arc::ptr_eq(&a1, &a1b),
            "同 repo → 同一 Arc（池命中,不重复 open）"
        );
        assert!(!Arc::ptr_eq(&a1, &a2), "异 repo → 不同 Arc");
    }
}
