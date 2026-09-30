//! 建索引与增量更新 —— **唯一的写盘入口**(索引库那一侧)。
//!
//! 全量:扫文件 → 抽符号 → 建边 → 落库 → 重建文档链接 → 盖时间戳。
//! 增量:只重算指纹变了的文件;⚠ 两个 `unresolved_*` 与 `ambiguous_calls` **不在增量里重算**
//! (它们是全局计数),所以覆盖读数是「上次全量时」的 —— 这一条在 `mcp` 的输出里明说了。

use super::{fingerprint, now_nanos, to_rel, Engine};
use crate::model::{IndexDelta, IndexPhase, IndexProgress, IndexStats, Lang, Symbol};
use crate::{docs, graph, scan, symbols};
use std::error::Error;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

impl Engine {
    /// 全量索引:扫所有受支持语言的源文件 → 符号入库 + 记内容指纹 + 打索引时间戳。
    /// 先对账清理旧数据,保证幂等(已删文件不残留陈旧符号)。
    pub fn index(&mut self) -> Result<IndexStats, Box<dyn Error>> {
        self.index_with_progress(&mut |_| {})
    }

    /// 同 [`Engine::index`],边走边报进度(见 [`IndexProgress`]:每阶段先报 `0/总数`,之后每份一报)。
    /// 回调在建索引的线程上同步调 —— 慢了就拖慢建索引,节流是调用方的事。
    pub fn index_with_progress(
        &mut self,
        on: &mut dyn FnMut(IndexProgress),
    ) -> Result<IndexStats, Box<dyn Error>> {
        self.idx.clear_symbols_and_fingerprints()?;
        let files = scan::source_files(&self.repo);
        let total = files.len();
        let mut tick = |phase: IndexPhase, done: usize| on(IndexProgress { phase, done, total });
        let mut stats = IndexStats::default();
        // 缓存 (rel, src, symbols),供随后建边复用(避免二次读盘)
        let mut cache: Vec<(String, String, Vec<Symbol>)> = Vec::new();
        tick(IndexPhase::Parse, 0);
        for (rel, _lang) in &files {
            let src = std::fs::read_to_string(self.repo.join(rel)).unwrap_or_default();
            // 一次解析同时拿符号 · 字段类型 · 解析健康
            let got = symbols::index_source(&src, rel);
            let syms = got.symbols;
            // F18:仅"解析报错**且未产出任何符号**"才算硬失败。避免 grammar 底噪误报——
            // 如 kotlin-ng 对合法 `class C { fun m(){} }` 也报 has_error 但符号完好,不该计。
            if got.has_error && syms.is_empty() {
                stats.parse_errors += 1;
            }
            stats.symbols += syms.len();
            self.idx.replace_file_symbols(rel, &syms)?;
            self.idx.replace_file_field_types(rel, &got.fields)?;
            self.idx.replace_file_impls(rel, &got.impls)?;
            self.idx.set_file_fingerprint(rel, &fingerprint(&src))?;
            cache.push((rel.clone(), src, syms));
            tick(IndexPhase::Parse, cache.len());
        }
        stats.files = files.len();
        // 全量建边(需完整符号表);src 已在内存(cache),不再读盘、graph 不做 IO
        // 文件表传**全仓已索引文件**(不从符号推):没有符号的文件照样能当 import 目标
        let mut table =
            graph::SymbolTable::with_files(self.idx.all_symbols()?, self.idx.all_indexed_files()?)
                .with_field_types(self.idx.all_field_types()?)
                .with_impls(self.idx.all_impls()?);
        // 第一趟:建边。此刻**函数摘要只到一跳**(建符号阶段只看得见函数自己的函数体)。
        let mut pass_a: Vec<Vec<crate::model::Edge>> = Vec::with_capacity(cache.len());
        tick(IndexPhase::Link, 0);
        for (rel, src, syms) in &cache {
            let built = graph::build_edges(src, rel, syms, &table);
            stats.unresolved_calls += built.unresolved_calls; // F18:识别为调用但连不上仓内符号
            stats.ambiguous_calls += built.ambiguous_calls; // 看得见但分不清(≠ 看不见)
            stats.unresolved_imports += built.unresolved_imports; // 同上,但成因不同,分开计
            pass_a.push(built.edges);
            tick(IndexPhase::Link, pass_a.len());
        }
        // 不动点:顺第一趟的调用边(带 `arg_flow`)把摘要抬成传递闭包。
        // 🔴 解析结果与两个 unresolved 计数**不受摘要影响**(摘要只改参数流),
        //    所以 stats 用第一趟的就是对的,第二趟不再累加。
        let lifted = graph::propagate_summaries(&mut table, &pass_a.concat());
        if !lifted.is_empty() {
            self.idx.update_param_flows(&lifted)?;
        }
        // 第二趟:摘要厚了,调用点上能落地的定义也就多了。
        // 🔴 **只重建真的会变的文件** —— 一个文件只有在它调用了某个**摘要变了**的符号时,
        //    重来才可能得出不同的边。全量重来一趟是三倍索引时间换极少数文件的差异
        //    (实测本仓 2.7s → 8.2s,而真正要重建的是 0 个文件)。
        tick(IndexPhase::Relink, 0);
        for (k, (rel, src, syms)) in cache.iter().enumerate() {
            let needs_redo = pass_a[k].iter().any(|e| lifted.contains_key(&e.to));
            if needs_redo {
                let built = graph::build_edges(src, rel, syms, &table);
                self.idx.insert_edges(&built.edges)?;
            } else {
                self.idx.insert_edges(&pass_a[k])?;
            }
            tick(IndexPhase::Relink, k + 1);
        }
        // F18:覆盖信号入 meta,供 overview 诚实展示(update 增量不维护,故为"上次全量 index 时")
        self.idx
            .set_meta("unresolved_calls", &stats.unresolved_calls.to_string())?;
        self.idx
            .set_meta("ambiguous_calls", &stats.ambiguous_calls.to_string())?;
        self.idx
            .set_meta("unresolved_imports", &stats.unresolved_imports.to_string())?;
        self.idx
            .set_meta("parse_errors", &stats.parse_errors.to_string())?;
        // 文档链接:扫 .md → 解析 → 存(只读;drift/docs_for 查询时再据当前符号解析)
        self.rebuild_doc_links_with(on)?;
        self.stamp_index_time()?;
        self.invalidate_caches();
        Ok(stats)
    }

    /// 全量重建索引(F16:`index()` 的语义明确别名,供 cc-monitor 的 reindex 按钮调用)。
    pub fn reindex(&mut self) -> Result<IndexStats, Box<dyn Error>> {
        self.index()
    }

    /// 同 [`Engine::reindex`],边走边报进度(同 [`Engine::index_with_progress`])。
    pub fn reindex_with_progress(
        &mut self,
        on: &mut dyn FnMut(IndexProgress),
    ) -> Result<IndexStats, Box<dyn Error>> {
        self.index_with_progress(on)
    }

    /// 全量重建文档链接(doc-links 由 .md 派生)。
    pub(super) fn rebuild_doc_links(&mut self) -> Result<(), Box<dyn Error>> {
        self.rebuild_doc_links_with(&mut |_| {})
    }

    /// 同 [`Engine::rebuild_doc_links`],边走边报 [`IndexPhase::Docs`] 那一段。
    fn rebuild_doc_links_with(
        &mut self,
        on: &mut dyn FnMut(IndexProgress),
    ) -> Result<(), Box<dyn Error>> {
        let mds = scan::markdown_files(&self.repo);
        let total = mds.len();
        let mut links = Vec::new();
        on(IndexProgress {
            phase: IndexPhase::Docs,
            done: 0,
            total,
        });
        for (k, md) in mds.iter().enumerate() {
            let content = std::fs::read_to_string(self.repo.join(md)).unwrap_or_default();
            links.extend(docs::parse_md(md, &content));
            on(IndexProgress {
                phase: IndexPhase::Docs,
                done: k + 1,
                total,
            });
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
                let got = symbols::index_source(&src, &rel);
                let syms = got.symbols;
                self.idx.replace_file_field_types(&rel, &got.fields)?;
                self.idx.replace_file_impls(&rel, &got.impls)?;
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
            let table = graph::SymbolTable::with_files(
                self.idx.all_symbols()?,
                self.idx.all_indexed_files()?,
            )
            .with_field_types(self.idx.all_field_types()?)
            .with_impls(self.idx.all_impls()?);
            for (rel, src, syms) in &rebuild {
                // 增量不重算全局覆盖计数(两个 unresolved 都忽略);coverage 保持上次 full index 值
                let built = graph::build_edges(src, rel, syms, &table);
                self.idx.insert_edges(&built.edges)?;
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
}
