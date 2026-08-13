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
//! **索引时机 + 落点（F69 补 D20）**：`Engine::open` 很轻（建目录 + 空 index.db,**不扫描**);
//! 真正的扫描（tree-sitter 解析全仓）只在 `panorama_index`/`reindex`——**前端只在用户显式点
//! 「建立索引」才调**（D20:代码分析每仓手动开启、默认关）。索引落 **cc-monitor 数据目录**
//! `~/.claude/claudecode-frontend/panorama/`（`panorama_store_dir`）,**不再写进用户仓**——消灭
//! 「点🗺就在你仓里凭空建 `.codepicture/`」灰区;纯缓存、可删、与 §1 只读铁律正交。

//! # `#79` 的另一半（「test 工程管理」）**不是这里的工作面**〔`P8b` 08-12 实测〕
//!
//! `#79` 把 code-picture 拆成两块能力：**代码分析**（这里做的）与 **test 工程管理**。
//! 被指派「把 test 工程管理集成进来」的人会落在本文件 —— 先读这段，能省下几天：
//!
//! **那半不是「我们还没做」，是「上游还没有」。** 两者差一个量级：前者听起来像排期问题，
//! 后者说明**这里根本没有可集成的东西**。实测（08-12）：
//! 上游仓 HEAD 就是 `VENDOR.md` 记的 vendored commit（`d558e47`，07-16 至今没动）；
//! 上游全仓搜 `test 工程 / 测试工程 / test_project / TestSuite / test_runner` **0 命中**；
//! 三个 crate 是 `core` / `lsp` / `mcp`，**没有 test 那一块**。
//! 而 `#79` 正文自己写的也是「（**新增，后续加入**）……**后面要加入**」——
//! 它是一个**意向**，不是一份需求：**没有动作、没有对象、没有验收**。
//!
//! ⇒ 在这里动手 = 替上游发明一块它自己都还没定义的能力，而 vendored 副本是
//! 「上游的镜子，不是分身」（`VENDOR.md` 的 SS-10 铁律：**绝不在副本里改出自己的版本**）。
//! ⇒ 该等的绊线**已经有了**：上游一领先副本，`build.rs::check_vendor_freshness` 就发
//! `cargo:warning`。**别新造第二份**「上游加没加」的检查。
//! ⇒ 范围与落仓两问登记在 `control-parity` 的待决 `U10e`。
//!
//! ⚠ 别把这段读成「`#79` 做完了」：**代码分析那半齐了**（`P7b` 08-12 补上最后缺的
//! 函数级调用子图 + 影响面），**整条 issue 没完** —— 缺的正是上面这半。

use code_picture_core::{model, Engine, EngineOpts};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// per-repo Engine 池（照 sftp_pool 两级锁）。key = repo 根路径。
fn pool() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<Engine>>>> {
    static P: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Engine>>>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// **F69（补 D20）**：索引落哪。返回 cc-monitor 数据目录下的 `panorama/`——core 会在其下
/// 自己按仓 canonical 路径 hash 追加 `.codepicture/<仓名>-<hash>/`（`engine.rs:42-50`），所以
/// **传父目录即可,别 per-repo 再嵌套**。这样索引**不再落进用户仓**（消灭「点🗺就在你仓里凭空
/// 建 `.codepicture/`」灰区，SS-7 + D20）；纯缓存、可删、gitignore 无关（不在用户仓了）。
/// **`None`（`resolve_monitor_data_dir` 失败 = 连 home_dir 都拿不到）→ `Engine::open` 回退落被
/// 分析仓 = 静默复活 D20 灰区;但那种环境下 config/history（`lib.rs` 靠同一 home_dir）早已不可用、
/// 根本走不到开全景面板这步,实际不可达——故不为它加门,只在此注明。**
fn panorama_store_dir() -> Option<PathBuf> {
    crate::paths::resolve_monitor_data_dir().map(|d| d.join("panorama"))
}

/// 取/建某仓的 Engine（`open` 很轻,**不扫描**,只建目录 + 空 index.db,见 F69 审计）。生产入口:
/// store_dir = `panorama_store_dir()`（落 cc-monitor 数据目录,不碰用户仓）。
fn engine_for(repo: &str) -> Result<Arc<Mutex<Engine>>, String> {
    engine_for_with_store(repo, panorama_store_dir())
}

/// 内部:显式 store_dir 版（供测试注入临时目录,免污染真实数据目录）。**pool 全局锁只短暂持有
/// 取/回填,绝不跨 `Engine::open`**（照 sftp_pool R3 教训 + 手册 §2:慢活/SQLite 别持全局锁）。
/// key 走 `canonicalize` 消化尾斜杠/`.`/相对-绝对差异,**防同仓不同写法建重复 Engine**（否则两条
/// rusqlite 连接对同一 index.db 并发写 → SQLITE_BUSY + 缓存不一致,D-建议2）。pool 只按 repo key,
/// 与 store_dir 无关（同仓恒同 Engine）。
fn engine_for_with_store(
    repo: &str,
    store_dir: Option<PathBuf>,
) -> Result<Arc<Mutex<Engine>>, String> {
    let key = std::fs::canonicalize(repo).map_err(|e| format!("仓路径无效（{repo}）: {e}"))?;
    // 快路径:池命中（短暂持锁,无 open）。
    if let Some(e) = pool().lock().unwrap().get(&key) {
        return Ok(Arc::clone(e));
    }
    // 慢路径:`open` 在池锁**外**（open 期间不持全局锁,别的仓可并发首开）。
    let engine = Engine::open(&key, EngineOpts { store_dir })
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PanoramaStatus {
    stale: bool,
    // **C03 大整数策略**：量纲是**秒或毫秒时间戳**（索引建立时刻）——
    // 2^53-1 无论按 ms（≈28.5 万年）还是按 s 都远超任何真实取值 ⇒ f64 精度足够。
    // **写 "number | null" 而不是 "number"**：它是 `Option` 且**无** `skip_serializing_if`
    // ⇒ 线上 None 序列化成显式 `null`（批 4 的 `duration_ms` 立的规则，已有守卫机检）。
    #[cfg_attr(test, ts(type = "number | null"))]
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

/// F71：列某文件的所有符号（点文件气泡 → 展开符号列表 → 点符号进详情，补「点文件不能列符号」
/// 遗留）。core 无 pub by-file 查询口（`Index::symbols_in_file` 挂在私有 `idx` 上），故走 public
/// `symbols_touching`（`ranges` 空 = 整文件所有符号 id）+ 逐 id `find_symbol` 取完整 `Symbol`。
/// 单文件符号量小，N+1 够用（要更快的单条 SQL 得暴露 `Engine::symbols_in_file`=改上游 re-vendor，
/// 非必需，defer）。
#[tauri::command]
pub async fn panorama_symbols_in_file(
    repo: String,
    file: String,
) -> Result<Vec<model::Symbol>, String> {
    with_engine(repo, move |e| collect_symbols_in_file(e, &file)).await
}

/// `symbols_in_file` 的核心（抽出以便单测——不 spawn 任务也能在真索引的 Engine 上验证）。
fn collect_symbols_in_file(e: &Engine, file: &str) -> Vec<model::Symbol> {
    let ids = e.symbols_touching(&[PathBuf::from(file)], &[]);
    ids.iter().filter_map(|id| e.find_symbol(id)).collect()
}

/// F71：文档漂移——仓里 `.md` 指向的目标文件/符号已失效（悬空链接）。core `drift()` 直出
/// （带缓存，按 index/doc-link 变更时刻快照；用户改了代码但没刷新时反映上次索引，与全景「陈旧
/// 靠手动刷新」模型一致——前端如实提示）。刷新按钮走 reindex → 重建 doc-links → drift 新鲜。
#[tauri::command]
pub async fn panorama_drift(repo: String) -> Result<Vec<model::DriftItem>, String> {
    with_engine(repo, |e| e.drift()).await
}

// === F72:批注 + 文档关联写层（只调 core 现成接口，SS-15；写落被分析仓、人手势触发） ===

/// F72:人写批注(直接 Active——人写永远赢)。落被分析仓 `<repo>/.codepicture/annotations/`(可提交、
/// 随仓走，F72 分家)。`symbol` = 符号段(如 `f`/`Type::method`)，None = 文件级批注。
#[tauri::command]
pub async fn panorama_add_annotation(
    repo: String,
    file: String,
    symbol: Option<String>,
    body: String,
    author: String,
) -> Result<String, String> {
    with_engine(repo, move |e| {
        e.add_annotation(&file, symbol.as_deref(), &body, &author)
            .map_err(|e| e.to_string())
    })
    .await?
}

/// F72:agent 提议批注(Proposed，需人 `approve` 才 Active——人审门禁；对消费者不可见直到批准)。
#[tauri::command]
pub async fn panorama_propose_annotation(
    repo: String,
    file: String,
    symbol: Option<String>,
    body: String,
    author: String,
) -> Result<String, String> {
    with_engine(repo, move |e| {
        e.propose_annotation(&file, symbol.as_deref(), &body, &author)
            .map_err(|e| e.to_string())
    })
    .await?
}

/// F72:批准一条 Proposed 批注 → Active(人审门禁)。
#[tauri::command]
pub async fn panorama_approve_annotation(repo: String, id: String) -> Result<bool, String> {
    with_engine(repo, move |e| {
        e.approve_annotation(&id).map_err(|e| e.to_string())
    })
    .await?
}

/// F72:删批注。
#[tauri::command]
pub async fn panorama_remove_annotation(repo: String, id: String) -> Result<bool, String> {
    with_engine(repo, move |e| {
        e.remove_annotation(&id).map_err(|e| e.to_string())
    })
    .await?
}

/// F72:列全部批注(含 Proposed，给审批队列)。
#[tauri::command]
pub async fn panorama_list_annotations(repo: String) -> Result<Vec<model::Annotation>, String> {
    with_engine(repo, |e| e.list_annotations()).await
}

/// F72:把某 `.md` 关联到某符号(写 doc 的 frontmatter `covers:`，进仓、可提交)。人手势触发——
/// 绝不接自动流程(融合手册)。
#[tauri::command]
pub async fn panorama_write_doc_link(
    repo: String,
    doc: String,
    target: String,
) -> Result<(), String> {
    with_engine(repo, move |e| {
        e.write_doc_link(&doc, &target).map_err(|e| e.to_string())
    })
    .await?
}

/// F72:删除某 `.md` 对某符号的关联。
#[tauri::command]
pub async fn panorama_remove_doc_link(
    repo: String,
    doc: String,
    target: String,
) -> Result<bool, String> {
    with_engine(repo, move |e| {
        e.remove_doc_link(&doc, &target).map_err(|e| e.to_string())
    })
    .await?
}

#[cfg(test)]
mod tests {
    /// ★★ **写 doc-link 那条路的安全性整个压在 vendor 的 `guard_rel` 上**
    /// 〔audit-0805 08-08，Phase G 第 87 件〕。
    ///
    /// # 先核的结果（三段，别只读结论）
    ///
    /// 08-08 横扫「收路径参数的 `#[tauri::command]`」得 22 条，逐条分档：
    /// 远端那批（`sftp_pool` / `sftp` / `acct_iso_deploy` / `tmux`）操作的是**用户自己的
    /// 远端机器**，收任意路径是功能本身；本机那侧除上一轮刚围栏的三条 `cc_integration_*`，
    /// 只剩 panorama 这五条。
    ///
    /// 1. `add_annotation` / `propose_annotation` 收的 `file` **不进路径** ——
    ///    vendor 把它当**数据**存进 `Annotation`，落盘文件名是内容哈希 ⇒ 无穿越面，
    ///    **刻意不给它们加守卫**（加了是安慰剂）。
    /// 2. `write_doc_link` / `remove_doc_link` 的 `doc` **真的进路径**（`repo.join(doc_rel)`），
    ///    而 vendor **自己有围栏**：`guard_rel` / `guard_doc_rel` 拒绝绝对路径与 `..`。
    /// 3. `repo` 本身**刻意不设围栏**：panorama 的功能就是「索引任意一个项目目录」，
    ///    home 围栏会砍掉 `/srv/work` 这类正当用法。⇒ 登记进 `ROADMAP §5` 当诚实边界，
    ///    而不是装一道假围栏。
    ///
    /// # 本条钉什么
    ///
    /// 我们这侧的安全性**整个压在别人家的两行守卫上**，而 vendor 是**冻结的副本**
    ///（红线：一字节不动）——它会被**整份换新**（`build.rs` 有 `.vendor_id` 新鲜度自检，
    /// 但那只说「副本旧了」，不说「那两行还在不在」）。
    /// ⇒ 前提触发器：那两个方法必须仍然调 `guard_rel`，且 `guard_rel` 必须仍然
    /// 同时拒**绝对路径**与 `..`。**只读 vendor，不改它一个字节。**
    #[test]
    fn the_doc_link_writes_still_go_through_the_vendor_guard() {
        let vendor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/code-picture-core/src/engine.rs");
        let src = std::fs::read_to_string(&vendor)
            .unwrap_or_else(|e| panic!("读不到 {vendor:?}：{e} —— vendor 布局变了就把本条一起改"));
        // 我们真正调到的**写路径**方法（`panorama.rs` 里各调一次）。
        for m in ["write_doc_link", "remove_doc_link"] {
            let at = src.find(&format!("pub fn {m}(")).unwrap_or_else(|| {
                panic!("vendor 里找不到 `{m}` —— 换版了，本条与 `panorama.rs` 一起复核")
            });
            // ⚠ 两件事一起做对，缺一个就白写：
            // ① **按 char 取，别按字节切** —— vendor 源码里全是中文注释，
            //    `&src[at..at+400]` 会落在汉字中间当场 panic（`digit_after` 头注记过）；
            // ② **切到方法真正的结尾**，不是「起点后 N 个字符」——
            //    ⚠ 08-08 变异实测：定长窗口把守卫删掉后**照样绿**，因为窗口
            //    一路吃进了下一个方法 `remove_doc_link`，那里还有一句 `guard_rel`。
            //    同一个缺陷两轮前刚在 `structural_scan` 修过，这次犯在自己新写的判据上。
            let body: String = {
                let rest: Vec<char> = src[at..].chars().take(4000).collect();
                let mut depth = 0i32;
                let mut end = rest.len();
                for (i, c) in rest.iter().enumerate() {
                    if *c == '{' {
                        depth += 1;
                    } else if *c == '}' {
                        depth -= 1;
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                }
                rest[..end].iter().collect()
            };
            let body = body.as_str();
            assert!(
                body.len() > 40 && body.lines().count() < 40,
                "从 `{m}` 切出 {} 行，不像一个方法体（配平切错了，本条会零命中地绿）",
                body.lines().count()
            );
            assert!(
                guard_core::contains_word(body, "guard_rel"),
                "vendor 的 `{m}` 不再调 `guard_rel` 了。\n\
                 ★ 那两行是**我们这侧唯一挡着路径穿越的东西**：`doc` 来自 webview，\n\
                 下游是 `repo.join(doc_rel)` 然后 `fs::write`。\n\
                 ⚠ vendor 是冻结副本、会被整份换新，而 `.vendor_id` 新鲜度自检只说\n\
                 「副本旧了」，不说「那两行还在不在」。\n\
                 换版后要么确认新版另有等价围栏，要么在 `panorama.rs` 这侧自己加一道。\n\
                 实得这一段：{body:?}"
            );
        }
        // 守卫本身还得真守：绝对路径与 `..` 两件都要拒。
        let g = src
            .find("fn guard_rel(")
            .expect("vendor 里找不到 `guard_rel` 定义 —— 上面那两条此刻在比一个不存在的东西");
        let gbody: String = src[g..].chars().take(300).collect();
        let gbody = gbody.as_str();
        // ⚠ 用 `contains_word` 而不是裸 `contains`：`needle_anchor` 棘轮当场拦过
        // （33→34），而它指出的不只是写法 —— 裸子串在事实被撑大时照样绿。
        for needle in ["starts_with", "\"..\""] {
            assert!(
                guard_core::contains_word(gbody, needle),
                "`guard_rel` 里找不到 `{needle}` —— 它可能只剩半道围栏了。\n\
                 两件缺一不可：绝对路径（`/etc/x.md` 直接跳出 repo）与 `..`（逐级爬出去）。\n\
                 实得：{gbody:?}"
            );
        }
    }

    use super::*;

    #[test]
    fn engine_pool_same_repo_same_arc_distinct_repo_distinct_arc() {
        // 池 get-or-create 语义:同 repo 返同一 Arc（命中,不重复 open）;异 repo 返不同 Arc。
        // 用显式临时 store（`engine_for_with_store`）——不走 panorama_store_dir 免污染真实数据目录。
        let base = std::env::temp_dir();
        let r1 = base.join("cc-monitor-b15-pool-a");
        let r2 = base.join("cc-monitor-b15-pool-b");
        let store = base.join("cc-monitor-b15-pool-store");
        std::fs::create_dir_all(&r1).ok();
        std::fs::create_dir_all(&r2).ok();
        let a1 = engine_for_with_store(r1.to_str().unwrap(), Some(store.clone())).expect("open r1");
        let a1b = engine_for_with_store(r1.to_str().unwrap(), Some(store.clone()))
            .expect("open r1 again");
        let a2 = engine_for_with_store(r2.to_str().unwrap(), Some(store.clone())).expect("open r2");
        assert!(
            Arc::ptr_eq(&a1, &a1b),
            "同 repo → 同一 Arc（池命中,不重复 open）"
        );
        assert!(!Arc::ptr_eq(&a1, &a2), "异 repo → 不同 Arc");
    }

    #[test]
    fn store_dir_some_writes_to_store_not_user_repo() {
        // F69/D20 回归防线:store_dir=Some → 索引落 store 目录,**用户仓不被建 .codepicture**
        // （消灭「点🗺就在你仓里凭空建目录」灰区）。防有人把 panorama.rs 的 store_dir 改回 None。
        let base = std::env::temp_dir();
        let repo = base.join("cc-monitor-f69-d20-repo");
        let store = base.join("cc-monitor-f69-d20-store");
        std::fs::remove_dir_all(repo.join(".codepicture")).ok(); // 干净起点
        std::fs::remove_dir_all(&store).ok();
        std::fs::create_dir_all(&repo).ok();
        let _e =
            engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open repo");
        assert!(
            !repo.join(".codepicture").exists(),
            "store_dir=Some 时用户仓不该凭空出现 .codepicture（D20）"
        );
        assert!(
            store.join(".codepicture").exists(),
            "索引应落到 store_dir 下（core 在其下建 .codepicture/<name>-<hash>/）"
        );
        std::fs::remove_dir_all(&store).ok();
    }

    #[test]
    fn symbols_in_file_lists_that_files_symbols() {
        // F71：索引一个含两个函数的临时仓 → collect_symbols_in_file 列出该文件的符号。
        // 显式临时 store（免污染真实数据目录）。走真 tree-sitter（rust grammar）索引。
        let base = std::env::temp_dir();
        let repo = base.join("cc-monitor-f71-symfile-repo");
        let store = base.join("cc-monitor-f71-symfile-store");
        std::fs::remove_dir_all(&repo).ok();
        std::fs::remove_dir_all(&store).ok();
        std::fs::create_dir_all(&repo).ok();
        std::fs::write(repo.join("lib.rs"), "pub fn alpha() {}\npub fn beta() {}\n").unwrap();
        let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
        {
            let mut g = arc.lock().unwrap();
            g.index().expect("index");
            let names: std::collections::HashSet<String> = collect_symbols_in_file(&g, "lib.rs")
                .into_iter()
                .map(|s| s.name)
                .collect();
            assert!(names.contains("alpha"), "应列出 alpha，实得 {names:?}");
            assert!(names.contains("beta"), "应列出 beta，实得 {names:?}");
            // 不存在的文件 → 空。
            assert!(collect_symbols_in_file(&g, "nope.rs").is_empty());
        }
        std::fs::remove_dir_all(&repo).ok();
        std::fs::remove_dir_all(&store).ok();
    }

    #[test]
    fn annotation_writes_to_repo_not_store_after_split() {
        // F72:re-vendor 后批注分家——store_dir=Some 时批注落 <repo>/.codepicture/annotations/、
        // 不落 store;写批注前 repo 内无 .codepicture(D20:批注 lazy)。验 re-vendor 的 core 行为。
        let base = std::env::temp_dir();
        let repo = base.join("cc-monitor-f72-ann-repo");
        let store = base.join("cc-monitor-f72-ann-store");
        std::fs::remove_dir_all(repo.join(".codepicture")).ok();
        std::fs::remove_dir_all(&store).ok();
        std::fs::create_dir_all(&repo).ok();
        std::fs::write(repo.join("lib.rs"), "pub fn f() {}\n").unwrap();
        let arc = engine_for_with_store(repo.to_str().unwrap(), Some(store.clone())).expect("open");
        {
            let mut g = arc.lock().unwrap();
            g.index().expect("index");
            assert!(
                !repo.join(".codepicture").exists(),
                "写批注前 repo 内不该有 .codepicture（D20 lazy）"
            );
            let id = g
                .add_annotation("lib.rs", Some("f"), "note", "me")
                .expect("add_annotation");
            assert!(
                repo.join(".codepicture")
                    .join("annotations")
                    .join(format!("{id}.json"))
                    .is_file(),
                "批注应落 <repo>/.codepicture/annotations/"
            );
            let store_has_ann = std::fs::read_dir(store.join(".codepicture"))
                .map(|rd| rd.flatten().any(|e| e.path().join("annotations").exists()))
                .unwrap_or(false);
            assert!(!store_has_ann, "批注不该落 store（已分家回仓）");
            assert_eq!(
                g.annotations_for(&"lib.rs#f".to_string()).len(),
                1,
                "应读回批注"
            );
        }
        std::fs::remove_dir_all(repo.join(".codepicture")).ok();
        std::fs::remove_dir_all(&store).ok();
    }

    /// `P8b-Y1`：`#79` 那半的边界必须留在本文件的头注上，且说的是
    /// **「上游还没有」**而不是**「我们还没做」**。
    ///
    /// 为什么值得立一条判据钉一段散文：它省的是**几天** —— 下一个被指派这件事的人
    /// 若不知道上游零实现，会先花时间去找「code-picture 的 test 那块 API 在哪」。
    ///
    /// ⚠ 读 `production_source` 而**不是** `production_code`：后者连 `//` 注释一起剥，
    /// 而本条钉的**恰恰就是一段注释** —— 用错那个的话判据当场瞎（首跑实测：红在
    /// 「头注里少了『上游』」，而那段字明明在）。
    /// ⚠ 仍必须剥测试段：否则本条自己那几个字面量会把自己喂绿 —— 那是本会话犯过
    /// **四次**的同一种自伤（`P3s-Y2` / `P4d-Y4` / `P4b-Y3`，`P8a` 时刻意绕开了一次）。
    #[test]
    fn the_upstream_gap_for_issue_79_is_written_down_here() {
        let prod = guard_core::production_source(include_str!("panorama.rs"));
        // ⚠ `别新造第二份` 是**首跑变异补进来的**：原表只钉 `check_vendor_freshness`
        // 这个名字，而把「别新造第二份检查」那句话删掉**照样绿** —— 名字在、
        // **可操作的那半没了**，下一个人照样会去造第二条绊线。
        for needle in [
            "上游",
            "#79",
            "check_vendor_freshness",
            "别新造第二份",
            "U10e",
        ] {
            assert!(
                prod.contains(needle),
                "头注里少了「{needle}」—— 那段边界是 `P8b` 唯一的交付物，删了它\
                 下一个人就会以为这半只是「还没排期」"
            );
        }
        // ★ 钉**说法本身**，不只是关键词：把「上游还没有」改写成「我们还没做」
        // 是本件最可能的腐坏形态，而那两句话的排期含义差一个量级。
        assert!(
            prod.contains("不是「我们还没做」，是「上游还没有」"),
            "那句区分被改掉了 —— 它正是本件的正题"
        );
    }
}
