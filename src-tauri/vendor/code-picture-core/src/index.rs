//! SQLite 索引(唯一 IO 边界之一)。F01 一次性建全 5 表,后续功能只填不改结构。

use crate::model::{Confidence, DocLink, Edge, EdgeKind, Lang, LinkSource, SymKind, Symbol};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// F68：schema 版本。加 `symbols.signature` 列 = v2。旧库（v1，无 signature 列）打开时
/// 版本不符 → drop 派生表重建（`index.db` 是派生 + gitignore，reindex 便宜；不迁移则
/// 旧库 `SELECT signature` 爆 "no such column"）。以后改 symbols/edges/doc_links 结构就 bump。
const SCHEMA_VERSION: &str = "2";

pub struct Index {
    conn: Connection,
}

impl Index {
    pub fn open(db_path: &Path) -> rusqlite::Result<Index> {
        let conn = Connection::open(db_path)?;
        let idx = Index { conn };
        idx.init_schema()?;
        Ok(idx)
    }

    /// schema:symbols/edges/doc_links/meta 四表。
    /// (F07 批注改走 JSON 侧车 `.codepicture/annotations/*.json`——人写可版本化的唯一真相,
    /// 早先预建的 annotations SQLite 表从未读写,已于 Phase G 删除。)
    fn init_schema(&self) -> rusqlite::Result<()> {
        // F68：schema 版本迁移。先建 meta（判定要读它），再比对版本——不符则 drop 派生表
        // 重建（旧 v1 库的 symbols 无 signature 列，直接 CREATE IF NOT EXISTS 不会补列 →
        // SELECT signature 会爆）。index.db 派生 + gitignore，reindex 便宜，drop 重建安全。
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )?;
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if current.as_deref() != Some(SCHEMA_VERSION) {
            self.conn.execute_batch(
                "DROP TABLE IF EXISTS symbols;
                 DROP TABLE IF EXISTS edges;
                 DROP TABLE IF EXISTS doc_links;
                 DELETE FROM meta WHERE key LIKE 'fp:%';
                 DELETE FROM meta WHERE key = 'last_index_time';",
            )?;
        }
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS symbols (
                id          TEXT PRIMARY KEY,
                file        TEXT NOT NULL,
                name        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                lang        TEXT NOT NULL,
                start_line  INTEGER NOT NULL,
                end_line    INTEGER NOT NULL,
                body_hash   TEXT NOT NULL,
                signature   TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);

            CREATE TABLE IF NOT EXISTS edges (
                from_id        TEXT NOT NULL,
                to_id          TEXT NOT NULL,
                kind           TEXT NOT NULL,
                call_site_line INTEGER,
                confidence     TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
            CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);

            CREATE TABLE IF NOT EXISTS doc_links (
                doc_path      TEXT NOT NULL,
                anchor_file   TEXT NOT NULL,
                anchor_symbol TEXT,
                source        TEXT NOT NULL,
                state         TEXT
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        // F68：记录当前 schema 版本（迁移完成标记，幂等——已是 v2 则 no-op 覆盖）。
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION],
        )?;
        Ok(())
    }

    /// 用一个文件的最新符号覆盖该文件在库里的旧符号(增量的基本操作)。
    pub fn replace_file_symbols(&mut self, file: &str, syms: &[Symbol]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM symbols WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO symbols
                 (id, file, name, kind, lang, start_line, end_line, body_hash, signature)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for s in syms {
                stmt.execute(params![
                    s.id,
                    s.file,
                    s.name,
                    kind_str(&s.kind),
                    lang_str(&s.lang),
                    s.start_line as i64,
                    s.end_line as i64,
                    s.body_hash.to_string(),
                    s.signature, // F68：Option<String> → NULL/TEXT
                ])?;
            }
        }
        tx.commit()
    }

    /// 全量重建前的对账清理:清空符号 + 边 + 所有文件指纹(避免已删文件的陈旧数据残留)。
    pub fn clear_symbols_and_fingerprints(&self) -> rusqlite::Result<()> {
        // 也清 last_index_time:index() 开头调用本方法,若随后中途失败,库里符号为空、
        // 时间戳也无 → is_stale=true(正确地报"需重建"),而非留旧戳误报新鲜。
        self.conn.execute_batch(
            "DELETE FROM symbols; DELETE FROM edges; DELETE FROM doc_links;
             DELETE FROM meta WHERE key LIKE 'fp:%';
             DELETE FROM meta WHERE key = 'last_index_time';",
        )
    }

    /// `ORDER BY id` 钉死节点顺序 → 依赖遍历序的社区检测(LPA)跨环境可复现。
    pub fn all_symbols(&self) -> rusqlite::Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file, name, kind, lang, start_line, end_line, body_hash, signature
             FROM symbols ORDER BY id",
        )?;
        let rows = stmt.query_map([], row_to_symbol)?;
        rows.collect()
    }

    pub fn all_edges(&self) -> rusqlite::Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, kind, call_site_line, confidence
             FROM edges ORDER BY from_id, to_id",
        )?;
        let rows = stmt.query_map([], row_to_edge)?;
        rows.collect()
    }

    /// 文档链接全量重建(F05:doc-links 由 .md 派生,index/md 变更时整体重写)。
    pub fn replace_all_doc_links(&mut self, links: &[DocLink]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM doc_links", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO doc_links (doc_path, anchor_file, anchor_symbol, source, state)
                 VALUES (?1, ?2, ?3, ?4, NULL)",
            )?;
            for l in links {
                stmt.execute(params![
                    l.doc_path,
                    l.target_file,
                    l.target_symbol,
                    source_str(&l.source),
                ])?;
            }
        }
        tx.commit()
    }

    pub fn all_doc_links(&self) -> rusqlite::Result<Vec<DocLink>> {
        let mut stmt = self.conn.prepare(
            "SELECT doc_path, anchor_file, anchor_symbol, source
             FROM doc_links ORDER BY doc_path, anchor_file",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DocLink {
                doc_path: r.get(0)?,
                target_file: r.get(1)?,
                target_symbol: r.get(2)?,
                source: source_from(&r.get::<_, String>(3)?),
            })
        })?;
        rows.collect()
    }

    /// 删除某文件所有函数的出边(靠仍在库的旧符号命中;须在替换该文件符号之前调用)。
    pub fn delete_edges_from_file(&self, file: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?1)",
            params![file],
        )?;
        Ok(())
    }

    pub fn insert_edges(&mut self, edges: &[Edge]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO edges (from_id, to_id, kind, call_site_line, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for e in edges {
                stmt.execute(params![
                    e.from,
                    e.to,
                    edgekind_str(&e.kind),
                    e.call_site_line.map(|l| l as i64),
                    conf_str(&e.confidence),
                ])?;
            }
        }
        tx.commit()
    }

    /// 出边(callees 方向)。过滤掉目标已不存在的悬空边(增量滞后期的兜底)。
    pub fn edges_from(&self, id: &str) -> rusqlite::Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, kind, call_site_line, confidence FROM edges
             WHERE from_id = ?1 AND to_id IN (SELECT id FROM symbols)",
        )?;
        let rows = stmt.query_map(params![id], row_to_edge)?;
        rows.collect()
    }

    /// 入边(callers 方向)。过滤掉来源已不存在的悬空边。
    pub fn edges_to(&self, id: &str) -> rusqlite::Result<Vec<Edge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, kind, call_site_line, confidence FROM edges
             WHERE to_id = ?1 AND from_id IN (SELECT id FROM symbols)",
        )?;
        let rows = stmt.query_map(params![id], row_to_edge)?;
        rows.collect()
    }

    pub fn symbols_in_file(&self, file: &str) -> rusqlite::Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file, name, kind, lang, start_line, end_line, body_hash, signature
             FROM symbols WHERE file = ?1",
        )?;
        let rows = stmt.query_map(params![file], row_to_symbol)?;
        rows.collect()
    }

    pub fn count_symbols(&self) -> rusqlite::Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
    }

    pub fn symbol_by_id(&self, id: &str) -> Option<Symbol> {
        self.conn
            .query_row(
                "SELECT id, file, name, kind, lang, start_line, end_line, body_hash, signature
                 FROM symbols WHERE id = ?1",
                params![id],
                row_to_symbol,
            )
            .ok()
    }

    /// 按符号名子串搜索(F06)。转义 LIKE 通配符,让 query 里的 `%`/`_` 按字面匹配。
    pub fn search_symbols(&self, pattern: &str, limit: usize) -> rusqlite::Result<Vec<Symbol>> {
        let esc = pattern
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let like = format!("%{}%", esc);
        let mut stmt = self.conn.prepare(
            "SELECT id, file, name, kind, lang, start_line, end_line, body_hash, signature
             FROM symbols WHERE name LIKE ?1 ESCAPE '\\' ORDER BY name, id LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![like, limit as i64], row_to_symbol)?;
        rows.collect()
    }

    pub fn set_file_fingerprint(&self, file: &str, fp: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![format!("fp:{}", file), fp],
        )?;
        Ok(())
    }

    pub fn get_file_fingerprint(&self, file: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![format!("fp:{}", file)],
                |r| r.get(0),
            )
            .ok()
    }

    pub fn clear_file_fingerprint(&self, file: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM meta WHERE key = ?1",
            params![format!("fp:{}", file)],
        )?;
        Ok(())
    }

    /// 通用 meta 键值(F16:`last_index_time` 等;与 `fp:` 指纹同表不同键)。
    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .ok()
    }
}

fn row_to_symbol(r: &rusqlite::Row) -> rusqlite::Result<Symbol> {
    Ok(Symbol {
        id: r.get(0)?,
        file: r.get(1)?,
        name: r.get(2)?,
        kind: kind_from(&r.get::<_, String>(3)?),
        lang: lang_from(&r.get::<_, String>(4)?),
        start_line: r.get::<_, i64>(5)? as usize,
        end_line: r.get::<_, i64>(6)? as usize,
        body_hash: r.get::<_, String>(7)?.parse().unwrap_or(0),
        signature: r.get(8)?, // F68：TEXT/NULL → Option<String>
    })
}

fn kind_str(k: &SymKind) -> &'static str {
    match k {
        SymKind::Function => "function",
        SymKind::Method => "method",
        SymKind::Class => "class",
        SymKind::Module => "module",
    }
}

fn kind_from(s: &str) -> SymKind {
    match s {
        "method" => SymKind::Method,
        "class" => SymKind::Class,
        "module" => SymKind::Module,
        _ => SymKind::Function,
    }
}

fn lang_str(l: &Lang) -> &'static str {
    match l {
        Lang::Rust => "rust",
        Lang::Python => "python",
        Lang::JavaScript => "javascript",
        Lang::TypeScript => "typescript",
        Lang::Java => "java",
        Lang::Kotlin => "kotlin",
        Lang::C => "c",
        Lang::Cpp => "cpp",
        Lang::CSharp => "csharp",
    }
}

/// 落库字符串 → Lang;未知(或旧 "rust")兜底 Rust,保证读回不 panic。
fn lang_from(s: &str) -> Lang {
    match s {
        "python" => Lang::Python,
        "javascript" => Lang::JavaScript,
        "typescript" => Lang::TypeScript,
        "java" => Lang::Java,
        "kotlin" => Lang::Kotlin,
        "c" => Lang::C,
        "cpp" => Lang::Cpp,
        "csharp" => Lang::CSharp,
        _ => Lang::Rust,
    }
}

fn row_to_edge(r: &rusqlite::Row) -> rusqlite::Result<Edge> {
    Ok(Edge {
        from: r.get(0)?,
        to: r.get(1)?,
        kind: edgekind_from(&r.get::<_, String>(2)?),
        call_site_line: r.get::<_, Option<i64>>(3)?.map(|l| l as usize),
        confidence: conf_from(&r.get::<_, String>(4)?),
    })
}

fn edgekind_str(k: &EdgeKind) -> &'static str {
    match k {
        EdgeKind::Calls => "calls",
        EdgeKind::Imports => "imports",
    }
}

fn edgekind_from(s: &str) -> EdgeKind {
    match s {
        "imports" => EdgeKind::Imports,
        _ => EdgeKind::Calls,
    }
}

fn conf_str(c: &Confidence) -> &'static str {
    match c {
        Confidence::Exact => "exact",
        Confidence::Heuristic => "heuristic",
        Confidence::DynamicGuess => "dynamic_guess",
    }
}

fn conf_from(s: &str) -> Confidence {
    match s {
        "heuristic" => Confidence::Heuristic,
        "dynamic_guess" => Confidence::DynamicGuess,
        _ => Confidence::Exact,
    }
}

fn source_str(s: &LinkSource) -> &'static str {
    match s {
        LinkSource::Colocation => "colocation",
        LinkSource::Frontmatter => "frontmatter",
        LinkSource::Inline => "inline",
    }
}

fn source_from(s: &str) -> LinkSource {
    match s {
        "colocation" => LinkSource::Colocation,
        "inline" => LinkSource::Inline,
        _ => LinkSource::Frontmatter,
    }
}
