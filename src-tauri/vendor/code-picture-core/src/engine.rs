//! Engine —— 唯一对外入口。索引 + 增量 + symbols_touching + 锚点解析 + 多语言 + 新鲜度。
//!
//! **线程模型(cc-monitor 融合必读)**:`Engine` 内含 `rusqlite::Connection`,故是 `Send`
//! 但**非 `Sync`**。多线程(如 Tauri command)共享时用 `State<Mutex<Engine>>`,并把
//! `index`/`update`/查询放进 `tokio::task::spawn_blocking`(SQLite 调用是阻塞的,勿在 async
//! 执行器线程上直接跑)。单线程使用无此约束。

use crate::anchor;
use crate::index::Index;
use crate::model::{
    AffectedSymbol, Anchor, AnchorState, Annotation, AnnotationStatus, DocLink, DriftItem, Edge,
    ImpactSet, IndexDelta, IndexStats, Lang, LineRange, NodeView, Overview, RankedFile, Resolution,
    SubGraph, Subsystem, Symbol, SymbolId, TokenBudget,
};
use crate::{annotations, docs, graph, rank, scan, symbols};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
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
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub struct Engine {
    repo: PathBuf,
    // F27:该仓的 `.codepicture` 根(index.db + annotations/)。默认 `<repo>/.codepicture`,
    // 或经 `EngineOpts.store_dir` 集中到 `<store>/.codepicture/<仓hash>/`。
    dot: PathBuf,
    idx: Index,
    // F17 派生态缓存(内部可变,overview/drift 为 &self);按 index/update/doc-link 变更失效。
    // 缓存的 overview 是**未裁剪**的全景(budget 无关);drift 是全量结果。
    // RefCell 使 Engine 保持 !Sync(rusqlite 本已如此),仍 Send —— 线程模型不变。
    overview_cache: RefCell<Option<Overview>>,
    drift_cache: RefCell<Option<Vec<DriftItem>>>,
}

impl Engine {
    /// 打开一个仓库。索引 + 批注落 `.codepicture`(位置见 [`EngineOpts::store_dir`]:默认
    /// `<repo>/.codepicture`,或集中到 `<store_dir>/.codepicture/<仓hash>/`);除此之外不写仓库任何文件。
    pub fn open(repo: &Path, opts: EngineOpts) -> Result<Engine, Box<dyn Error>> {
        let dot = codepicture_root(repo, &opts);
        std::fs::create_dir_all(&dot)?;
        // 总是写 `.codepicture/.gitignore`:忽略派生 index.db、保留人写 annotations/
        annotations::ensure_gitignore(&dot)?;
        let idx = Index::open(&dot.join("index.db"))?;
        Ok(Engine {
            repo: repo.to_path_buf(),
            dot,
            idx,
            overview_cache: RefCell::new(None),
            drift_cache: RefCell::new(None),
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

    /// 全量索引:扫所有受支持语言的源文件 → 符号入库 + 记内容指纹 + 打索引时间戳。
    /// 先对账清理旧数据,保证幂等(已删文件不残留陈旧符号)。
    pub fn index(&mut self) -> Result<IndexStats, Box<dyn Error>> {
        self.idx.clear_symbols_and_fingerprints()?;
        let files = scan::source_files(&self.repo);
        let mut stats = IndexStats::default();
        // 缓存 (rel, src, symbols),供随后建边复用(避免二次读盘)
        let mut cache: Vec<(String, String, Vec<Symbol>)> = Vec::new();
        for (rel, _lang) in &files {
            let src = std::fs::read_to_string(self.repo.join(rel)).unwrap_or_default();
            let (syms, has_err) = symbols::index_source(&src, rel); // 一次解析拿符号 + 解析健康
                                                                    // F18:仅"解析报错**且未产出任何符号**"才算硬失败。避免 grammar 底噪误报——
                                                                    // 如 kotlin-ng 对合法 `class C { fun m(){} }` 也报 has_error 但符号完好,不该计。
            if has_err && syms.is_empty() {
                stats.parse_errors += 1;
            }
            stats.symbols += syms.len();
            self.idx.replace_file_symbols(rel, &syms)?;
            self.idx.set_file_fingerprint(rel, &fingerprint(&src))?;
            cache.push((rel.clone(), src, syms));
        }
        stats.files = files.len();
        // 全量建边(需完整符号表);src 已在内存(cache),不再读盘、graph 不做 IO
        let table = graph::SymbolTable::from_symbols(self.idx.all_symbols()?);
        for (rel, src, syms) in &cache {
            let (edges, unresolved) = graph::build_edges(src, rel, syms, &table);
            stats.unresolved_calls += unresolved; // F18:识别为调用但连不上仓内符号
            self.idx.insert_edges(&edges)?;
        }
        // F18:覆盖信号入 meta,供 overview 诚实展示(update 增量不维护,故为"上次全量 index 时")
        self.idx
            .set_meta("unresolved_calls", &stats.unresolved_calls.to_string())?;
        self.idx
            .set_meta("parse_errors", &stats.parse_errors.to_string())?;
        // 文档链接:扫 .md → 解析 → 存(只读;drift/docs_for 查询时再据当前符号解析)
        self.rebuild_doc_links()?;
        self.stamp_index_time()?;
        self.invalidate_caches();
        Ok(stats)
    }

    /// 全量重建索引(F16:`index()` 的语义明确别名,供 cc-monitor 的 reindex 按钮调用)。
    pub fn reindex(&mut self) -> Result<IndexStats, Box<dyn Error>> {
        self.index()
    }

    /// 全量重建文档链接(doc-links 由 .md 派生)。
    fn rebuild_doc_links(&mut self) -> Result<(), Box<dyn Error>> {
        let mut links = Vec::new();
        for md in scan::markdown_files(&self.repo) {
            let content = std::fs::read_to_string(self.repo.join(&md)).unwrap_or_default();
            links.extend(docs::parse_md(&md, &content));
        }
        self.idx.replace_all_doc_links(&links)?;
        self.invalidate_drift(); // doc_links 变仅 drift 失效(overview 不依赖 doc_links);index/update 另清 overview
        Ok(())
    }

    /// 增量:只处理传入的变更文件;指纹未变则跳过;文件已删则清空其符号。
    /// 出边随之重建(删旧边须在替换符号前,以命中旧符号)。
    /// 已知限制(Phase 1):只重建变更文件的**出边**;指向变更文件的跨文件**入边**
    /// 可能滞后,直到那些来源文件被再次 update / index。
    pub fn update(&mut self, changed: &[PathBuf]) -> Result<IndexDelta, Box<dyn Error>> {
        let mut delta = IndexDelta::default();
        // 缓存待重建出边的文件 (rel, src, symbols)
        let mut rebuild: Vec<(String, String, Vec<Symbol>)> = Vec::new();
        for path in changed {
            let rel = to_rel(&self.repo, path);
            let abs = self.repo.join(&rel);
            if abs.is_file() && Lang::from_path(&rel).is_some() {
                let src = std::fs::read_to_string(&abs).unwrap_or_default();
                let fp = fingerprint(&src);
                if self.idx.get_file_fingerprint(&rel).as_deref() == Some(fp.as_str()) {
                    continue; // 未变,跳过(边也不动)
                }
                self.idx.delete_edges_from_file(&rel)?; // 旧符号仍在库,能命中
                let syms = symbols::symbols_in_source(&src, &rel);
                delta.added += syms.len();
                self.idx.replace_file_symbols(&rel, &syms)?;
                self.idx.set_file_fingerprint(&rel, &fp)?;
                delta.updated_files += 1;
                rebuild.push((rel, src, syms));
            } else {
                // 文件被删 / 非 .rs → 删其出边 + 清空其符号 + 清指纹
                self.idx.delete_edges_from_file(&rel)?;
                self.idx.replace_file_symbols(&rel, &[])?;
                self.idx.clear_file_fingerprint(&rel)?;
                delta.removed += 1;
                delta.updated_files += 1;
            }
        }
        if !rebuild.is_empty() {
            let table = graph::SymbolTable::from_symbols(self.idx.all_symbols()?);
            for (rel, src, syms) in &rebuild {
                // 增量不重算全局覆盖计数(unresolved 忽略);coverage 保持上次 full index 值
                let (edges, _unresolved) = graph::build_edges(src, rel, syms, &table);
                self.idx.insert_edges(&edges)?;
            }
        }
        // .md 变更 → 重建全部文档链接(廉价、全量)
        if changed
            .iter()
            .any(|p| to_rel(&self.repo, p).ends_with(".md"))
        {
            self.rebuild_doc_links()?;
        }
        self.stamp_index_time()?;
        self.invalidate_caches();
        Ok(delta)
    }

    // ── 新鲜度(F16,cc-monitor #3)──

    /// 打上"本次索引时间"(纳秒 unix,防同秒抖动)。index/update 成功后调用。
    fn stamp_index_time(&self) -> Result<(), Box<dyn Error>> {
        self.idx
            .set_meta("last_index_time", &now_nanos().to_string())?;
        Ok(())
    }

    /// 上次 index/update 的 unix 秒(cc-monitor 显示"N 秒前索引")。从未索引 → None。
    pub fn indexed_at(&self) -> Option<u64> {
        self.idx
            .get_meta("last_index_time")
            .and_then(|s| s.parse::<u128>().ok())
            .map(|nanos| (nanos / 1_000_000_000) as u64)
    }

    /// 索引是否已陈旧:任一源文件 mtime 晚于上次索引时间(或从未索引)。
    /// 这是给 cc-monitor 的**粗查**信号,精确新鲜度靠 cc-monitor 侧的文件系统 watcher。
    /// **已知盲区**(均为安全方向——偏"漏报新鲜",不产错误结果):
    /// - 文件**删除**不被捕获(重扫时缺席的文件不参与比较)。
    /// - `last_index_time` 是**全局水位**,`update(changed)` 结尾也会推进它——**假定
    ///   `update` 收到了全部变更**;若 watcher 漏报某文件、又因别的文件触发了 update,
    ///   该文件的陈旧会被水位抹平,直到它再次被改。
    /// - 亚秒检出取决于 FS 的 mtime 分辨率(ext4/xfs/btrfs/tmpfs 为纳秒;FAT/老 ext3 为
    ///   秒级,同秒改动可能漏检)。
    /// - `cp -p`/`git checkout` 等保留旧 mtime 的操作会让"内容变但 mtime 不变"漏检。
    /// - 大仓上每次调用全量 `readdir` + 每源文件一次 `stat`(O(files));勿高频轮询。
    pub fn is_stale(&self) -> bool {
        let indexed_nanos = match self
            .idx
            .get_meta("last_index_time")
            .and_then(|s| s.parse::<u128>().ok())
        {
            Some(n) => n,
            None => return true, // 从未索引
        };
        scan::source_files(&self.repo).iter().any(|(rel, _)| {
            std::fs::metadata(self.repo.join(rel))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() > indexed_nanos)
                .unwrap_or(false)
        })
    }

    /// 它调谁(出边),BFS 到 depth 层(depth=1 = 直接)。
    pub fn callees(&self, sym: &SymbolId, depth: u32) -> Vec<Edge> {
        self.traverse(sym, depth, Direction::Out)
    }

    /// 谁调它(入边)。
    pub fn callers(&self, sym: &SymbolId, depth: u32) -> Vec<Edge> {
        self.traverse(sym, depth, Direction::In)
    }

    fn traverse(&self, start: &SymbolId, depth: u32, dir: Direction) -> Vec<Edge> {
        let mut result = Vec::new();
        let mut seen_edges: HashSet<(String, String)> = HashSet::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(start.clone());
        let mut frontier = vec![start.clone()];
        let mut d = 0;
        while d < depth && !frontier.is_empty() {
            let mut next = Vec::new();
            for node in &frontier {
                let edges = match dir {
                    Direction::Out => self.idx.edges_from(node).unwrap_or_default(),
                    Direction::In => self.idx.edges_to(node).unwrap_or_default(),
                };
                for e in edges {
                    let other = match dir {
                        Direction::Out => e.to.clone(),
                        Direction::In => e.from.clone(),
                    };
                    if seen_edges.insert((e.from.clone(), e.to.clone())) {
                        result.push(e);
                    }
                    if visited.insert(other.clone()) {
                        next.push(other);
                    }
                }
            }
            frontier = next;
            d += 1;
        }
        result
    }

    /// 改动 `sym` 的 blast-radius:反向可达的全部传递调用者,各带最短反向距离(depth)。
    pub fn impact(&self, sym: &SymbolId) -> ImpactSet {
        let mut affected: Vec<AffectedSymbol> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(sym.clone());
        let mut frontier = vec![sym.clone()];
        let mut depth = 0usize;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for node in &frontier {
                for e in self.idx.edges_to(node).unwrap_or_default() {
                    if visited.insert(e.from.clone()) {
                        affected.push(AffectedSymbol {
                            id: e.from.clone(),
                            depth,
                        });
                        next.push(e.from);
                    }
                }
            }
            frontier = next;
        }
        // 确定性排序:depth 升序、并列 id 升序(不依赖 edges_to 顺序)
        affected.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.id.cmp(&b.id)));
        ImpactSet {
            root: sym.clone(),
            affected,
        }
    }

    /// 单符号完整视图(F15):符号 + 直接 callers/callees + 关联文档 + 批注。
    /// 一等方法,MCP 与 cc-monitor 共用(此前 node 在 MCP 层现拼)。符号不存在 → None。
    pub fn node(&self, sym: &SymbolId) -> Option<NodeView> {
        let symbol = self.find_symbol(sym)?;
        Some(NodeView {
            symbol,
            callers: self.callers(sym, 1),
            callees: self.callees(sym, 1),
            docs: self.docs_for(sym),
            annotations: self.annotations_for(sym),
        })
    }

    /// 以 `sym` 为心的双向邻域子图(F15):depth 层内的边(callers ∪ callees,去重)+
    /// 涉及到的全部符号(含中心;悬空 id 查不到则略过)。确定性排序,便于前端/测试稳定。
    pub fn subgraph(&self, sym: &SymbolId, depth: u32) -> SubGraph {
        let mut edges = self.callers(sym, depth);
        edges.extend(self.callees(sym, depth));
        let mut seen_edge = HashSet::new();
        edges.retain(|e| seen_edge.insert((e.from.clone(), e.to.clone())));
        edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));

        let mut ids: HashSet<SymbolId> = HashSet::new();
        ids.insert(sym.clone());
        for e in &edges {
            ids.insert(e.from.clone());
            ids.insert(e.to.clone());
        }
        let mut symbols: Vec<Symbol> = ids.iter().filter_map(|id| self.find_symbol(id)).collect();
        symbols.sort_by(|a, b| a.id.cmp(&b.id));
        SubGraph { symbols, edges }
    }

    /// 覆盖某符号的文档链接(符号级 / 文件级 / 目录前缀)。
    /// 符号级:限定名目标(`Type::name`)按完整段精确比,裸名目标才退化为裸名比。
    pub fn docs_for(&self, sym: &SymbolId) -> Vec<DocLink> {
        let (file, seg) = split_sym_id(sym);
        let seg_bare = seg.as_deref().map(bare_name);
        let links = self.idx.all_doc_links().unwrap_or_default();
        links
            .into_iter()
            .filter(|l| {
                if l.is_dir() {
                    file.starts_with(&l.target_file)
                } else if let Some(tsym) = &l.target_symbol {
                    l.target_file == file && symbol_matches(seg.as_deref(), seg_bare, tsym)
                } else {
                    l.target_file == file
                }
            })
            .collect()
    }

    /// 只读漂移:目标已失效的文档链接(不改任何文件)。按 (doc,file,symbol) 去重。
    /// 只读漂移(F05);F17:缓存全量结果(无 budget),按 index/update/doc-link 变更失效。
    /// **快照语义(cc-monitor 注意)**:drift 现场读工作树判断目标是否还在,但结果被缓存;
    /// 故返回的是**上次 index/update/doc-link 变更时的快照**,**不**是每次现场重扫。绕过
    /// `update()` 直接改磁盘上被 doc-link 指向的源文件,drift 不会立刻反映(与索引本身的
    /// 陈旧一致——请经 `update()` 上报变更,或先 `is_stale()` 判断)。
    pub fn drift(&self) -> Vec<DriftItem> {
        let cached = self.drift_cache.borrow().clone();
        if let Some(d) = cached {
            return d;
        }
        let result = self.compute_drift();
        *self.drift_cache.borrow_mut() = Some(result.clone());
        result
    }

    fn compute_drift(&self) -> Vec<DriftItem> {
        let links = self.idx.all_doc_links().unwrap_or_default();
        let mut out: Vec<DriftItem> = Vec::new();
        let mut seen: HashSet<(String, String, Option<String>)> = HashSet::new();
        for l in links {
            let reason: Option<String> = if l.is_dir() {
                if self.repo.join(l.target_file.trim_end_matches('/')).is_dir() {
                    None
                } else {
                    Some("目录不存在".to_string())
                }
            } else if let Some(sym) = &l.target_symbol {
                self.symbol_link_reason(&l.target_file, sym)
            } else if self.repo.join(&l.target_file).is_file() {
                None
            } else {
                Some("文件不存在".to_string())
            };
            if let Some(reason) = reason {
                let key = (
                    l.doc_path.clone(),
                    l.target_file.clone(),
                    l.target_symbol.clone(),
                );
                if seen.insert(key) {
                    out.push(DriftItem {
                        doc_path: l.doc_path,
                        target_file: l.target_file,
                        target_symbol: l.target_symbol,
                        reason,
                    });
                }
            }
        }
        out
    }

    /// 符号级链接是否漂移。限定名目标按完整段精确查(区分同文件同名兄弟);裸名复用锚点。
    fn symbol_link_reason(&self, file: &str, target_symbol: &str) -> Option<String> {
        if !target_symbol.contains("::") {
            // 裸名目标:复用 resolve_anchor
            return match self
                .resolve_anchor(&Anchor::symbol_level(
                    file.to_string(),
                    target_symbol.to_string(),
                    None,
                ))
                .state
            {
                AnchorState::Orphaned => Some("符号已不存在".to_string()),
                AnchorState::Ambiguous => Some("符号有多个同名候选,无法定位".to_string()),
                _ => None,
            };
        }
        // 限定名目标:按完整段(Type::name)精确匹配,文件改名跟随
        let count_in = |rel: &str| -> usize {
            let src = std::fs::read_to_string(self.repo.join(rel)).unwrap_or_default();
            symbols::symbols_in_source(&src, rel)
                .into_iter()
                .filter(|s| split_sym_id(&s.id).1.as_deref() == Some(target_symbol))
                .count()
        };
        let cur = if crate::git::file_exists(&self.repo, file) {
            Some(file.to_string())
        } else {
            crate::git::follow_rename(&self.repo, file)
        };
        if let Some(f) = &cur {
            if count_in(f) >= 1 {
                return None; // 原(或改名后)文件里仍在
            }
        }
        let total: usize = scan::source_files(&self.repo)
            .iter()
            .map(|(r, _)| count_in(r))
            .sum();
        match total {
            0 => Some("符号已不存在".to_string()),
            1 => None, // 移到别处 → 仍可解析,不算漂移
            _ => Some("符号有多个同名候选,无法定位".to_string()),
        }
    }

    /// F12 seam:一组变更文件/行 → 受影响的符号 id。ranges 为空表示整文件。
    /// 给 cc-monitor 以后把「Claude 刚 Edit 的东西」高亮到全景图上。
    pub fn symbols_touching(&self, files: &[PathBuf], ranges: &[LineRange]) -> Vec<SymbolId> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for f in files {
            let rel = to_rel(&self.repo, f);
            if let Ok(syms) = self.idx.symbols_in_file(&rel) {
                for s in syms {
                    let overlaps = ranges.is_empty()
                        || ranges
                            .iter()
                            .any(|r| r.start <= s.end_line && s.start_line <= r.end);
                    // 去重:同一文件被传入多次时不重复输出
                    if overlaps && seen.insert(s.id.clone()) {
                        out.push(s.id);
                    }
                }
            }
        }
        out
    }

    /// 解析一个锚点(现场对工作树,反映当前代码;有 quote 则走块级)。
    pub fn resolve_anchor(&self, a: &Anchor) -> Resolution {
        anchor::resolve(&self.repo, a)
    }

    /// 从当前文件抓一个块(1-based 行范围)做块级锚点(quote + 上下文 + content_hash)。只读。
    pub fn capture_block_anchor(
        &self,
        file: &str,
        start_line: usize,
        end_line: usize,
    ) -> Option<Anchor> {
        let content = std::fs::read_to_string(self.repo.join(file)).ok()?;
        let (block, prefix, suffix) = anchor::extract_block(&content, start_line, end_line)?;
        let hash = anchor::hash_str(&block);
        Some(Anchor::block_level(
            file.to_string(),
            block,
            Some(prefix),
            Some(suffix),
            Some(hash),
        ))
    }

    /// 库里符号总数(测试/诊断用)。
    pub fn symbol_count(&self) -> usize {
        self.idx.count_symbols().unwrap_or(0)
    }

    /// 按 id 取单个符号(F06)。
    pub fn find_symbol(&self, id: &SymbolId) -> Option<Symbol> {
        self.idx.symbol_by_id(id)
    }

    /// 按名子串搜索符号(F06)。
    pub fn search(&self, query: &str, limit: usize) -> Vec<Symbol> {
        self.idx.search_symbols(query, limit).unwrap_or_default()
    }

    // ── 批注(F07,写 `.codepicture/annotations/` 侧车)──

    /// 人写批注,直接 Active(人写永远赢:可把同内容的 Proposed 提为 Active)。
    pub fn add_annotation(
        &self,
        file: &str,
        symbol: Option<&str>,
        body: &str,
        author: &str,
    ) -> Result<String, Box<dyn Error>> {
        if body.trim().is_empty() {
            return Err("批注正文不能为空".into());
        }
        self.write_annotation(file, symbol, body, author, AnnotationStatus::Active)
    }

    /// agent 提议批注,Proposed;需人 `approve_annotation` 才 Active(人审门禁)。
    /// **门禁保护**:绝不把已 Active 的同内容批注降级回 Proposed(幂等返回既有 id)。
    pub fn propose_annotation(
        &self,
        file: &str,
        symbol: Option<&str>,
        body: &str,
        author: &str,
    ) -> Result<String, Box<dyn Error>> {
        if body.trim().is_empty() {
            return Err("批注正文不能为空".into());
        }
        let id = annotation_id(file, symbol, body);
        if let Some(existing) = annotations::get(&self.dot, &id) {
            if existing.status == AnnotationStatus::Active {
                return Ok(id); // 已被人批准的同内容 → 不降级
            }
        }
        self.write_annotation(file, symbol, body, author, AnnotationStatus::Proposed)
    }

    fn write_annotation(
        &self,
        file: &str,
        symbol: Option<&str>,
        body: &str,
        author: &str,
        status: AnnotationStatus,
    ) -> Result<String, Box<dyn Error>> {
        let id = annotation_id(file, symbol, body);
        let ann = Annotation {
            id: id.clone(),
            file: file.to_string(),
            symbol: symbol.map(String::from),
            body: body.to_string(),
            author: author.to_string(),
            status,
        };
        annotations::write(&self.dot, &ann)?;
        Ok(id)
    }

    /// 人审批准:Proposed → Active。返回该 id 是否存在。
    pub fn approve_annotation(&self, id: &str) -> Result<bool, Box<dyn Error>> {
        match annotations::get(&self.dot, id) {
            Some(mut a) => {
                a.status = AnnotationStatus::Active;
                annotations::write(&self.dot, &a)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn remove_annotation(&self, id: &str) -> Result<bool, Box<dyn Error>> {
        Ok(annotations::remove(&self.dot, id)?)
    }

    /// 全部批注(含 Proposed;给人看 / 审批队列)。
    pub fn list_annotations(&self) -> Vec<Annotation> {
        annotations::list(&self.dot)
    }

    /// 覆盖某符号的 **Active** 批注(给 agent 消费;Proposed 不可见 = 人审门禁)。
    /// 符号匹配复用 `symbol_matches`:限定名批注精确比、裸名批注按裸名比(与 docs_for 一致)。
    pub fn annotations_for(&self, sym: &SymbolId) -> Vec<Annotation> {
        let (file, seg) = split_sym_id(sym);
        let seg_bare = seg.as_deref().map(bare_name);
        annotations::list(&self.dot)
            .into_iter()
            .filter(|a| {
                a.status == AnnotationStatus::Active
                    && a.file == file
                    && match &a.symbol {
                        None => true, // 文件级批注覆盖该文件所有符号
                        Some(s) => symbol_matches(seg.as_deref(), seg_bare, s),
                    }
            })
            .collect()
    }

    // ── 文档关联写(F08,编辑用户 `.md` 的 frontmatter `covers:`)──

    /// 在指定 `.md` 的 `covers:` 加一条关联(只动 covers、保留其余);写后刷新 doc-links。
    pub fn write_doc_link(&mut self, doc: &str, target: &str) -> Result<(), Box<dyn Error>> {
        guard_rel(doc)?;
        docs::write_doc_link(&self.repo, doc, target)?;
        self.rebuild_doc_links()?;
        Ok(())
    }

    /// 从指定 `.md` 删一条 `covers:` 关联(返回原本是否存在);存在才刷新 doc-links。
    pub fn remove_doc_link(&mut self, doc: &str, target: &str) -> Result<bool, Box<dyn Error>> {
        guard_rel(doc)?;
        let removed = docs::remove_doc_link(&self.repo, doc, target)?;
        if removed {
            self.rebuild_doc_links()?;
        }
        Ok(removed)
    }

    /// 项目全景概览:PageRank 脊柱文件 + 社区子系统 + 入口点,按 token 预算裁剪。
    /// 项目全景(F03);F17:缓存**未裁剪**的全景(budget 无关),裁剪很廉价、每次对 clone 施加。
    pub fn overview(&self, budget: TokenBudget) -> Overview {
        let cached = self.overview_cache.borrow().clone();
        let mut ov = match cached {
            Some(raw) => raw,
            None => {
                let raw = self.compute_overview_raw();
                *self.overview_cache.borrow_mut() = Some(raw.clone());
                raw
            }
        };
        trim_to_budget(&mut ov, budget);
        ov
    }

    /// 全量计算未裁剪的全景(PageRank + 社区 + 脊柱/入口);贵,故缓存。
    fn compute_overview_raw(&self) -> Overview {
        let symbols = self.idx.all_symbols().unwrap_or_default();
        let edges = self.idx.all_edges().unwrap_or_default();
        let g = rank::Graph::build(&symbols, &edges);

        let total_symbols = symbols.len();
        let total_files: usize = {
            let mut set = HashSet::new();
            for s in &symbols {
                set.insert(s.file.as_str());
            }
            set.len()
        };
        // F18 覆盖信号(上次全量 index 时统计;未索引 → 0)
        let meta_usize = |k: &str| -> usize {
            self.idx
                .get_meta(k)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        };
        let unresolved_calls = meta_usize("unresolved_calls");
        let parse_errors = meta_usize("parse_errors");
        if g.is_empty() {
            return Overview {
                total_symbols,
                total_files,
                unresolved_calls,
                parse_errors,
                ..Default::default()
            };
        }

        let pr = g.pagerank(30, 0.85);
        let comm = g.communities(10);
        let entries = g.entry_points();
        let id_file: HashMap<&str, &str> = symbols
            .iter()
            .map(|s| (s.id.as_str(), s.file.as_str()))
            .collect();

        let spine_files = compute_spine(&g.ids, &pr, &id_file);
        let subsystems = compute_subsystems(&g.ids, &comm, &id_file);
        let entry_points = compute_entries(&g.ids, &pr, &entries);

        Overview {
            spine_files,
            subsystems,
            entry_points,
            total_symbols,
            total_files,
            unresolved_calls,
            parse_errors,
        }
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

fn to_rel(repo: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(repo).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

/// 防越界:拒绝含 `..` 或绝对路径的仓库相对路径(F08 写用户 .md 前的防御)。
fn guard_rel(rel: &str) -> Result<(), Box<dyn Error>> {
    if rel.starts_with('/') || rel.split(['/', '\\']).any(|s| s == "..") {
        return Err(format!("路径越界或非法:{}", rel).into());
    }
    Ok(())
}

/// 批注 id:内容哈希(file|symbol|body),确定性;status 不入哈希 → approve 不改 id。
/// ⚠ `DefaultHasher` 跨 Rust 版本不保证稳定;id 是提交进 git 的侧车文件名,换工具链后同内容
/// 再 `add` 会算出新 id(生成重复文件而非覆盖,旧文件成孤儿)。要跨版本稳定可换固定算法。
fn annotation_id(file: &str, symbol: Option<&str>, body: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    file.hash(&mut h);
    symbol.hash(&mut h);
    body.hash(&mut h);
    format!("{:x}", h.finish())
}

/// 拆分符号 id → (文件, 符号段)。符号段 = `#` 之后去 `@行号`(如 "Foo::run" 或 "login")。
fn split_sym_id(id: &str) -> (String, Option<String>) {
    match id.split_once('#') {
        Some((file, rest)) => (
            file.to_string(),
            Some(rest.split('@').next().unwrap_or(rest).to_string()),
        ),
        None => (id.to_string(), None),
    }
}

/// 符号段的裸名(去掉 `Type::` 限定)。
fn bare_name(seg: &str) -> &str {
    seg.rsplit("::").next().unwrap_or(seg)
}

/// 查询符号(段/裸名)是否匹配文档目标:限定名目标按完整段比,裸名目标按裸名比。
fn symbol_matches(query_seg: Option<&str>, query_bare: Option<&str>, target: &str) -> bool {
    if target.contains("::") {
        query_seg == Some(target)
    } else {
        query_bare == Some(target)
    }
}

/// 脊柱文件:按文件聚合 PageRank,分数降序(并列文件名升序)。
fn compute_spine(ids: &[SymbolId], pr: &[f64], id_file: &HashMap<&str, &str>) -> Vec<RankedFile> {
    let mut file_score: HashMap<&str, (f64, usize)> = HashMap::new();
    for (i, id) in ids.iter().enumerate() {
        if let Some(&file) = id_file.get(id.as_str()) {
            let e = file_score.entry(file).or_insert((0.0, 0));
            e.0 += pr[i];
            e.1 += 1;
        }
    }
    let mut spine: Vec<RankedFile> = file_score
        .into_iter()
        .map(|(file, (score, symbols))| RankedFile {
            file: file.to_string(),
            score,
            symbols,
        })
        .collect();
    spine.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.file.cmp(&b.file))
    });
    spine
}

/// 子系统:按社区标签聚合;label = 成员最多的文件;按 size 降序(并列 label 升序)。
fn compute_subsystems(
    ids: &[SymbolId],
    comm: &[usize],
    id_file: &HashMap<&str, &str>,
) -> Vec<Subsystem> {
    let mut by_label: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &l) in comm.iter().enumerate() {
        by_label.entry(l).or_default().push(i);
    }
    let mut subsystems: Vec<Subsystem> = by_label
        .into_values()
        .map(|members| {
            let mut fcount: HashMap<&str, usize> = HashMap::new();
            for &i in &members {
                if let Some(&file) = id_file.get(ids[i].as_str()) {
                    *fcount.entry(file).or_insert(0) += 1;
                }
            }
            // 频次最高的文件作 label;`b.0.cmp(a.0)` 反转次键 → 并列取最小文件名
            let label = fcount
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(f, _)| f.to_string())
                .unwrap_or_default();
            let mut files: Vec<String> = fcount.keys().map(|s| s.to_string()).collect();
            files.sort();
            Subsystem {
                label,
                files,
                size: members.len(),
            }
        })
        .collect();
    subsystems.sort_by(|a, b| b.size.cmp(&a.size).then(a.label.cmp(&b.label)));
    subsystems
}

/// 入口点(无入边),按 PageRank 降序(并列 id 升序)。
fn compute_entries(ids: &[SymbolId], pr: &[f64], entries: &[usize]) -> Vec<SymbolId> {
    let mut scored: Vec<(SymbolId, f64)> =
        entries.iter().map(|&i| (ids[i].clone(), pr[i])).collect();
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    scored.into_iter().map(|(id, _)| id).collect()
}

// 各类条目的每项序列化开销估算(分隔符/字段名等),供 budget 裁剪用
const SPINE_ITEM_TOKENS: usize = 4;
const SUBSYS_ITEM_TOKENS: usize = 6;
const ENTRY_ITEM_TOKENS: usize = 2;

/// 按 token 预算裁剪 overview:脊柱 50% / 子系统 30% / 入口 20%,各自贪心保留高排名项。
fn trim_to_budget(ov: &mut Overview, budget: TokenBudget) {
    let b = budget.0;
    let spine_b = b / 2;
    let sub_b = b / 10 * 3; // 先除后乘,避免 b*3 溢出
    let entry_b = b.saturating_sub(spine_b + sub_b);
    truncate_by_tokens(&mut ov.spine_files, spine_b, |rf| {
        TokenBudget::est_tokens(&rf.file) + SPINE_ITEM_TOKENS
    });
    truncate_by_tokens(&mut ov.subsystems, sub_b, |s| {
        TokenBudget::est_tokens(&s.label) + SUBSYS_ITEM_TOKENS
    });
    truncate_by_tokens(&mut ov.entry_points, entry_b, |id| {
        TokenBudget::est_tokens(id) + ENTRY_ITEM_TOKENS
    });
}

fn truncate_by_tokens<T>(v: &mut Vec<T>, budget: usize, cost: impl Fn(&T) -> usize) {
    let mut used = 0usize;
    let mut keep = 0usize;
    for item in v.iter() {
        used += cost(item);
        if used > budget {
            break;
        }
        keep += 1;
    }
    v.truncate(keep);
}
