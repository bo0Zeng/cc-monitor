//! 全部公共类型集中在此(账本共享面:字段一次到位,禁止回头补)。
//! F15:全部可视化类型派生 `Serialize`,供 cc-monitor 经 Tauri `invoke` 直传前端。

use serde::{Deserialize, Serialize};

/// 稳定符号 id:impl 方法 = `file#Type::method`,自由函数 = `file#name`(F02 已定死);
/// 残余同文件同名再追加 `@行号` 消歧(见 `symbols.rs`),避免落库 PRIMARY KEY 冲突丢符号。
pub type SymbolId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SymKind {
    Function,
    Method,
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Java,
    Kotlin,
    C,
    Cpp,
    CSharp,
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub file: String, // 仓库相对,正斜杠
    pub kind: SymKind,
    pub lang: Lang,
    pub start_line: usize, // 1-based
    pub end_line: usize,
    /// F68：符号签名文本（如 `fn foo(a:u32)->String`）——函数定义节点从起始到 body 前的
    /// 文本，折单行。拿不到（body 字段非标准 / 非可调用符号）= None。传前端做详情面板展示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip)] // 内部内容指纹,非前端数据;u64 经 JSON number 会 >2^53 丢精度,不外传
    pub body_hash: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

/// 人挂在代码上的锚点。Phase 1 只用文件/符号级;块级字段留占位(F09 才实现解析)。
#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    pub file: String,
    pub symbol: String,
    #[serde(skip)] // 内部指纹(见 Symbol.body_hash),不外传
    pub orig_body_hash: Option<u64>,
    // ── 块级/边级占位(F09)──
    pub node_path: Option<String>,
    pub quote: Option<String>,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    #[serde(skip)] // 内部指纹,不外传
    pub content_hash: Option<u64>,
}

impl Anchor {
    /// 造一个文件/符号级锚点(块级字段留空)。
    pub fn symbol_level(
        file: impl Into<String>,
        symbol: impl Into<String>,
        orig_body_hash: Option<u64>,
    ) -> Anchor {
        Anchor {
            file: file.into(),
            symbol: symbol.into(),
            orig_body_hash,
            node_path: None,
            quote: None,
            prefix: None,
            suffix: None,
            content_hash: None,
        }
    }

    /// 造一个块级锚点(F09:内容引用 + 上下文 + 完整性哈希;symbol 留空)。
    pub fn block_level(
        file: impl Into<String>,
        quote: impl Into<String>,
        prefix: Option<String>,
        suffix: Option<String>,
        content_hash: Option<u64>,
    ) -> Anchor {
        Anchor {
            file: file.into(),
            symbol: String::new(),
            orig_body_hash: None,
            node_path: None,
            quote: Some(quote.into()),
            prefix,
            suffix,
            content_hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum AnchorState {
    /// 原文件在(或经 git 改名跟到),符号也在
    Resolved,
    /// 文件被 git 改名,符号在新文件里
    MovedFileRenamed,
    /// 符号按名在别处找到(无 git 血缘)
    MovedToOtherFile,
    /// 多个同名候选
    Ambiguous,
    /// 哪都找不到
    Orphaned,
}

#[derive(Debug, Clone, Serialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub state: AnchorState,
    pub location: Option<Location>,
    pub content_changed: Option<bool>,
    pub candidates: Vec<Location>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    // F18 覆盖完整度信号(诚实标注"漏了多少"):
    /// 识别为调用点但被调不在符号表(外部/stdlib/漏抓)→ 未建成边的调用点数。
    pub unresolved_calls: usize,
    /// 解析报错**且未产出任何符号**(硬失败)的源文件数——只计真失败,滤掉 grammar 底噪。
    pub parse_errors: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct IndexDelta {
    pub updated_files: usize,
    pub added: usize,
    pub removed: usize,
}

// ── 调用图(F02)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EdgeKind {
    Calls,
    Imports, // 预留;F02 只填 Calls
}

/// 边的可信度(F10):静态调用图对动态语言不可判定,一律标注而非假装 sound。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Confidence {
    Exact,        // 唯一名 / 限定名唯一匹配
    Heuristic,    // 多个同名候选
    DynamicGuess, // 方法调用等,接收者类型未知
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub call_site_line: Option<usize>,
    pub confidence: Confidence,
}

// ── overview / 预算(F03)──

/// token 预算。agent 侧输出统一用 `est_tokens` 估算并裁剪到此上限(F06 复用)。
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget(pub usize);

impl TokenBudget {
    /// 粗略 token 估算(≈ 字符数/4)。**全项目统一口径**,F06 序列化复用。
    pub fn est_tokens(text: &str) -> usize {
        text.len().div_ceil(4)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RankedFile {
    pub file: String,
    pub score: f64, // 该文件所有符号 PageRank 之和
    pub symbols: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Subsystem {
    pub label: String,      // 代表(成员最多的文件)
    pub files: Vec<String>, // 涉及文件
    pub size: usize,        // 成员符号数
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Overview {
    pub spine_files: Vec<RankedFile>,
    pub subsystems: Vec<Subsystem>,
    pub entry_points: Vec<SymbolId>,
    pub total_symbols: usize,
    pub total_files: usize,
    // F18 覆盖信号(基于上次全量 index;见 IndexStats 同名字段)。
    pub unresolved_calls: usize,
    pub parse_errors: usize,
}

// ── impact / blast-radius(F04)──

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AffectedSymbol {
    pub id: SymbolId,
    pub depth: usize, // 反向距离(1 = 直接调用者)
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpactSet {
    pub root: SymbolId,
    pub affected: Vec<AffectedSymbol>,
}

impl ImpactSet {
    pub fn total(&self) -> usize {
        self.affected.len()
    }
}

// ── doc-links(F05)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LinkSource {
    Colocation,  // 目录里的 README.md
    Frontmatter, // md 头部 covers:
    Inline,      // 正文 [..](path)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocLink {
    pub doc_path: String,              // .md 文件(仓库相对)
    pub target_file: String,           // 目标文件或目录(目录以 '/' 结尾)
    pub target_symbol: Option<String>, // 符号级目标的符号名
    pub source: LinkSource,
}

impl DocLink {
    pub fn is_dir(&self) -> bool {
        self.target_file.ends_with('/')
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DriftItem {
    pub doc_path: String,
    pub target_file: String,
    pub target_symbol: Option<String>,
    pub reason: String, // 为什么算漂移
}

// ── 批注(F07)—— 侧车 JSON 文件为唯一真相(人写、可版本化)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationStatus {
    Active,   // 人写 / 已批准 —— agent 可消费
    Proposed, // agent 提议 —— 待人审批准
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub file: String,           // 锚点文件
    pub symbol: Option<String>, // 锚点符号(None = 文件级)
    pub body: String,           // 人写的批注正文
    pub author: String,
    pub status: AnnotationStatus,
}

// ── cc-monitor 视图聚合(F15)—— Engine 一等返回,MCP 与 cc-monitor 共用 ──

/// 单个符号的完整视图:符号本体 + 直接调用者/被调 + 关联文档 + 批注。
/// 由 `Engine::node` 组装(此前散在 MCP 层现拼,cc-monitor 得自己组合;F15 收口)。
#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub symbol: Symbol,
    pub callers: Vec<Edge>,
    pub callees: Vec<Edge>,
    pub docs: Vec<DocLink>,
    pub annotations: Vec<Annotation>,
}

/// 以某符号为心的邻域调用子图(双向、可控深度):节点集 + 边集。
#[derive(Debug, Clone, Default, Serialize)]
pub struct SubGraph {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
}
