//! Engine —— 唯一对外入口。索引 + 增量 + symbols_touching + 锚点解析 + 多语言 + 新鲜度。
//!
//! **线程模型(cc-monitor 融合必读)**:`Engine` 内含 `rusqlite::Connection`,故是 `Send`
//! 但**非 `Sync`**。多线程(如 Tauri command)共享时用 `State<Mutex<Engine>>`,并把
//! `index`/`update`/查询放进 `tokio::task::spawn_blocking`(SQLite 调用是阻塞的,勿在 async
//! 执行器线程上直接跑)。单线程使用无此约束。

use crate::annotations;
use crate::index::Index;
use crate::model::{Cfg, DriftItem, Overview, SymbolId};
use std::cell::RefCell;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Default)]
pub struct EngineOpts {
    /// F27:整个 `.codepicture`(索引 + 批注)的存放根。
    /// `None` → `<repo>/.codepicture`(旧行为,兼容);
    /// `Some(dir)` → `<dir>/.codepicture/<仓名-路径hash>/`(集中存,不污染被分析仓)。
    pub store_dir: Option<PathBuf>,
}

/// F27:解析某仓的 `.codepicture` 根。`store_dir`=None → 仓内(旧);Some → 集中到
/// `<store>/.codepicture/<仓名-路径hash>/`,同一仓(canonical 路径)总落同一目录。
fn codepicture_root(repo: &Path, opts: &EngineOpts) -> PathBuf {
    match &opts.store_dir {
        None => repo.join(".codepicture"),
        Some(store) => {
            let canon = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
            let name = repo
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "repo".to_string());
            let key = format!("{name}-{:016x}", fnv1a(canon.to_string_lossy().as_bytes()));
            store.join(".codepicture").join(key)
        }
    }
}

/// FNV-1a 64:跨 Rust 版本/平台稳定(不用 DefaultHasher——方案①下批注也存这、需稳定目录名)。
pub(super) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct Engine {
    repo: PathBuf,
    // F72:批注根,**恒 `<repo>/.codepicture/annotations/`**——与 index 落点(`store_dir`)解耦。批注是
    // 人写真相、可提交、随仓走(别人 clone 可见、抗仓移动),不该跟着索引缓存集中到仓外。**lazy 建**:
    // `open` 绝不建它(否则开面板即污染仓 = 消费方 D20 回归),只 `annotations::write` 首写时建。
    // 默认(`store_dir=None`)下与 index 根的 `annotations/` 子目录物理同址 → 行为逐字节不变。
    // (index.db 路径 open 时算好存进 `idx`,之后无需再持有 index 根,故不留 `dot` 字段。)
    annotations_dir: PathBuf,
    idx: Index,
    // F17 派生态缓存(内部可变,overview/drift 为 &self);按 index/update/doc-link 变更失效。
    // 缓存的 overview 是**未裁剪**的全景(budget 无关);drift 是全量结果。
    // RefCell 使 Engine 保持 !Sync(rusqlite 本已如此),仍 Send —— 线程模型不变。
    overview_cache: RefCell<Option<Overview>>,
    drift_cache: RefCell<Option<Vec<DriftItem>>>,
    /// **读库出错的账本**。查询接口返回 `Vec` 而非 `Result` 是刻意的(调用方要的就是一张表),
    /// 但「出错」与「确实没有」必须分得开 —— 出错时记一笔,由 `overview` 报出来。
    ///
    /// 🔴 真实事故:`all_edges` 少查一列 ⇒ `row_to_edge` 越界 ⇒ 被 `unwrap_or_default()`
    /// 静默吞掉 ⇒ **全景变空但不报错**,四个测试红了才发现。这个账本就是为它加的。
    db_errors: RefCell<Vec<String>>,
}

mod docs_anchors;
mod draw;
mod indexing;
mod overview;
mod queries;

impl Engine {
    /// 打开一个仓库。**索引**落 `.codepicture`(位置见 [`EngineOpts::store_dir`]:默认
    /// `<repo>/.codepicture`,或集中到 `<store_dir>/.codepicture/<仓hash>/`);**批注**恒落
    /// `<repo>/.codepicture/annotations/`(F72,与 store_dir 解耦、可提交)。`open` 只 eager 建
    /// **index 侧** `dot`——批注目录首写时 lazy 建,`open` 绝不碰它(保消费方 D20:开面板不污染仓)。
    pub fn open(repo: &Path, opts: EngineOpts) -> Result<Engine, Box<dyn Error>> {
        let dot = codepicture_root(repo, &opts);
        std::fs::create_dir_all(&dot)?;
        // 总是写 index 侧 `.codepicture/.gitignore`:忽略派生 index.db(默认模式下 annotations/ 同
        // 目录、仍可提交;store_dir 模式下 dot 在仓外、此 gitignore 无关紧要)。
        annotations::ensure_gitignore(&dot)?;
        let idx = Index::open(&dot.join("index.db"))?;
        // F72:批注恒仓内、与 index 落点解耦。**不** create_dir_all(保 lazy = 消费方 D20 生命线)。
        let annotations_dir = repo.join(".codepicture").join("annotations");
        Ok(Engine {
            repo: repo.to_path_buf(),
            annotations_dir,
            idx,
            overview_cache: RefCell::new(None),
            drift_cache: RefCell::new(None),
            db_errors: RefCell::new(Vec::new()),
        })
    }

    /// F17:清空两个派生缓存(index/update 改了符号+边+doc_links,两者都可能陈旧)。
    fn invalidate_caches(&self) {
        *self.overview_cache.borrow_mut() = None;
        *self.drift_cache.borrow_mut() = None;
    }

    /// 只清 drift 缓存:doc_links 变(write/remove_doc_link)不影响 overview(仅依赖符号+边),
    /// 故不白算 overview 的 PageRank/社区。
    fn invalidate_drift(&self) {
        *self.drift_cache.borrow_mut() = None;
    }

    /// 某函数的**控制流图**(第三档地基)。实现住 `crate::cfg` —— 这一层只转发,
    /// 因为定位 AST 节点是**解析层**的活,查询层不该碰 `tree_sitter`。
    ///
    /// 🔴 **`unsupported` 非空时这张图不可用**(踩到了不建模的控制结构)。
    /// ⚠ 目前只有 Rust 有实现;别的语言返回 `None`(= 没做,不是「这函数没有控制流」)。
    /// 把「查库出错」变成空结果 —— 但**留下痕迹**。
    ///
    /// 出错与「确实没有」长得一模一样,是这套系统撞过的最贵的一类 bug。
    /// 这里不 panic(MCP server 不该因为一次读库失败就倒),也不改签名(调用方要的就是一张表),
    /// 而是记一笔;`overview` 会把它报出来,`db_errors()` 可以直接读。
    pub(super) fn or_empty<T>(&self, what: &str, r: rusqlite::Result<Vec<T>>) -> Vec<T> {
        match r {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("{what}: {e}");
                // stderr 也吼一声 —— 跑在 MCP server 里时这是唯一看得见的地方
                eprintln!("⚠ code-picture 读库出错 —— {msg}");
                let mut log = self.db_errors.borrow_mut();
                if !log.contains(&msg) {
                    log.push(msg);
                }
                Vec::new()
            }
        }
    }

    /// 读库出错的记录(去重)。**非空 = 本次答案不完整**,不是「这仓就这样」。
    pub fn db_errors(&self) -> Vec<String> {
        self.db_errors.borrow().clone()
    }

    /// 仓库根(精确层要按仓库相对路径读源文件)。
    pub fn repo_path(&self) -> &Path {
        &self.repo
    }

    /// 全仓符号(精确层拿它定"这个调用点在谁体内")。
    pub fn all_symbols(&self) -> Result<Vec<crate::model::Symbol>, Box<dyn Error>> {
        Ok(self.idx.all_symbols()?)
    }

    /// 全仓 `AmbiguousCall` 边(精确层拿它找"该问谁")。
    pub fn all_ambiguous_edges(&self) -> Result<Vec<crate::model::Edge>, Box<dyn Error>> {
        Ok(self.idx.all_ambiguous_edges()?)
    }

    /// 精确层写回:删掉指定的边,再插入升级后的边。**一个事务,不留半套**。
    pub(crate) fn write_precise(
        &mut self,
        drop_edges: &std::collections::HashSet<(SymbolId, SymbolId)>,
        upgrade: &[(SymbolId, SymbolId, usize, Option<String>)],
    ) -> Result<(), Box<dyn Error>> {
        let drops: Vec<(String, String)> = drop_edges.iter().cloned().collect();
        let adds: Vec<crate::model::Edge> = upgrade
            .iter()
            .map(|(f, t, l, flow)| crate::precise::precise_edge(f, t, *l, flow.clone()))
            .collect();
        self.idx.apply_precise_edges(&drops, &adds)?;
        self.invalidate_caches();
        Ok(())
    }

    pub fn cfg(&self, sym: &SymbolId) -> Option<Cfg> {
        let s = self.find_symbol(sym)?;
        crate::cfg::build(&self.repo, sym, &s)
    }
}

/// 内容指纹,用作增量缓存 key。`DefaultHasher` 用固定 keys(0,0),同输入跨进程稳定;
/// 但 std 不保证跨 Rust 版本稳定 —— 升级工具链后指纹可能全失配,触发一次全量重扫
/// (方向安全:只多做功,不产生错误结果)。将来要持久稳定可换固定算法(FNV/BLAKE3)。
fn fingerprint(src: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    format!("{:x}", h.finish())
}

/// 当前 unix 纳秒(F16 索引时间戳;纳秒精度让"索引后立刻改文件"也能被 is_stale 检出)。
fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

// ── 跨子模块共用的小工具 ──

/// 拆分符号 id → (文件, 符号段)。符号段 = `#` 之后去 `@行号`(如 "Foo::run" 或 "login")。
pub(super) fn split_sym_id(id: &str) -> (String, Option<String>) {
    match id.split_once('#') {
        Some((file, rest)) => (
            file.to_string(),
            Some(rest.split('@').next().unwrap_or(rest).to_string()),
        ),
        None => (id.to_string(), None),
    }
}

/// 符号段的裸名(去掉 `Type::` 限定)。
pub(super) fn bare_name(seg: &str) -> &str {
    seg.rsplit("::").next().unwrap_or(seg)
}

/// 查询符号(段/裸名)是否匹配文档目标:限定名目标按完整段比,裸名目标按裸名比。
pub(super) fn symbol_matches(
    query_seg: Option<&str>,
    query_bare: Option<&str>,
    target: &str,
) -> bool {
    if target.contains("::") {
        query_seg == Some(target)
    } else {
        query_bare == Some(target)
    }
}

pub(super) fn to_rel(repo: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(repo).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

/// 防越界:拒绝含 `..` 或绝对路径的仓库相对路径(F08 写用户 .md 前的防御)。
pub(super) fn guard_rel(rel: &str) -> Result<(), Box<dyn Error>> {
    if rel.starts_with('/') || rel.split(['/', '\\']).any(|s| s == "..") {
        return Err(format!("路径越界或非法:{}", rel).into());
    }
    Ok(())
}

/// 批注 id:内容哈希(file|symbol|body),确定性;status 不入哈希 → approve 不改 id。
/// ⚠ `DefaultHasher` 跨 Rust 版本不保证稳定;id 是提交进 git 的侧车文件名,换工具链后同内容
/// 再 `add` 会算出新 id(生成重复文件而非覆盖,旧文件成孤儿)。要跨版本稳定可换固定算法。
pub(super) fn annotation_id(file: &str, symbol: Option<&str>, body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    file.hash(&mut h);
    symbol.hash(&mut h);
    body.hash(&mut h);
    format!("{:x}", h.finish())
}
