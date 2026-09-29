//! 文档关联 · 漂移对账 · 锚点解析 · 人工批注。
//!
//! 这一族的共同点:**它们锚定的是工作树的当前内容,不是索引快照** ——
//! 索引可能旧,工作树不会。所以 `drift` / `resolve_anchor` 都现场读盘解析。
//!
//! 批注侧车是**唯一真相**(人写、可版本化);agent 只能 `propose`(落 `Proposed`),
//! 要人 `approve` 才对 agent 可见。

use super::{bare_name, guard_rel, split_sym_id, symbol_matches, to_rel, Engine};
use crate::edits::{self, FileEdit};
use crate::model::{
    Anchor, AnchorState, Annotation, AnnotationStatus, DocLink, DriftItem, LineRange, Resolution,
    SymbolId,
};
use crate::scan;
use crate::{anchor, annotations, docs, symbols};
use std::collections::HashSet;
use std::error::Error;
use std::path::PathBuf;

impl Engine {
    /// 覆盖某符号的文档链接(符号级 / 文件级 / 目录前缀)。
    /// 符号级:限定名目标(`Type::name`)按完整段精确比,裸名目标才退化为裸名比。
    pub fn docs_for(&self, sym: &SymbolId) -> Vec<DocLink> {
        let (file, seg) = split_sym_id(sym);
        let seg_bare = seg.as_deref().map(bare_name);
        let links = self.or_empty("all_doc_links", self.idx.all_doc_links());
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
        let links = self.or_empty("all_doc_links", self.idx.all_doc_links());
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

    // ── 批注(F07,写 `.codepicture/annotations/` 侧车)──

    /// 人写批注,直接 Active(人写永远赢:可把同内容的 Proposed 提为 Active)。
    ///
    /// = [`edits::plan_add_annotation`](读 + 算)+ [`annotations::apply`](写盘)。
    /// 别的写者(cc-monitor)只用前一半,落盘走自己的写口。
    pub fn add_annotation(
        &self,
        file: &str,
        symbol: Option<&str>,
        body: &str,
        author: &str,
    ) -> Result<String, Box<dyn Error>> {
        let p = edits::plan_add_annotation(&self.repo, file, symbol, body, author)?;
        self.apply_annotation(p.edit.as_ref())?;
        Ok(p.value)
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
        let p = edits::plan_propose_annotation(&self.repo, file, symbol, body, author)?;
        self.apply_annotation(p.edit.as_ref())?;
        Ok(p.value)
    }

    /// 写盘那一层(批注):计划说有改动才落。
    fn apply_annotation(&self, edit: Option<&FileEdit>) -> Result<(), Box<dyn Error>> {
        if let Some(e) = edit {
            annotations::apply(&self.repo, e)?;
        }
        Ok(())
    }

    /// 人审批准:Proposed → Active。返回该 id 是否存在。**只改 `status`,不改 `origin`** ——
    /// 批准过的 agent 提议仍是 agent 说的。
    pub fn approve_annotation(&self, id: &str) -> Result<bool, Box<dyn Error>> {
        let p = edits::plan_approve_annotation(&self.repo, id)?;
        self.apply_annotation(p.edit.as_ref())?;
        Ok(p.value)
    }

    pub fn remove_annotation(&self, id: &str) -> Result<bool, Box<dyn Error>> {
        let p = edits::plan_remove_annotation(&self.repo, id)?;
        self.apply_annotation(p.edit.as_ref())?;
        Ok(p.value)
    }

    /// 全部批注(含 Proposed;给人看 / 审批队列)。
    pub fn list_annotations(&self) -> Vec<Annotation> {
        annotations::list(&self.annotations_dir)
    }

    /// 覆盖某符号的 **Active** 批注(给 agent 消费;Proposed 不可见 = 人审门禁)。
    /// 符号匹配复用 `symbol_matches`:限定名批注精确比、裸名批注按裸名比(与 docs_for 一致)。
    pub fn annotations_for(&self, sym: &SymbolId) -> Vec<Annotation> {
        let (file, seg) = split_sym_id(sym);
        let seg_bare = seg.as_deref().map(bare_name);
        annotations::list(&self.annotations_dir)
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
    /// = [`edits::plan_write_doc_link`] + [`docs::apply`] + [`Engine::refresh_doc_links`]。
    pub fn write_doc_link(&mut self, doc: &str, target: &str) -> Result<(), Box<dyn Error>> {
        guard_rel(doc)?;
        let p = edits::plan_write_doc_link(&self.repo, doc, target)?;
        if let Some(e) = &p.edit {
            docs::apply(&self.repo, e)?;
        }
        self.rebuild_doc_links()?;
        Ok(())
    }

    /// 从指定 `.md` 删一条 `covers:` 关联(返回原本是否存在);存在才刷新 doc-links。
    pub fn remove_doc_link(&mut self, doc: &str, target: &str) -> Result<bool, Box<dyn Error>> {
        guard_rel(doc)?;
        let p = edits::plan_remove_doc_link(&self.repo, doc, target)?;
        if let Some(e) = &p.edit {
            docs::apply(&self.repo, e)?;
        }
        if p.value {
            self.rebuild_doc_links()?;
        }
        Ok(p.value)
    }

    /// 让索引里的文档关联跟上盘上的 `.md`(别的写者落了 `covers:` 之后调;只写索引,不碰用户文件)。
    pub fn refresh_doc_links(&mut self) -> Result<(), Box<dyn Error>> {
        self.rebuild_doc_links()
    }
}
