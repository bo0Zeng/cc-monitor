//! 全部公共类型集中在此(账本共享面:字段一次到位,禁止回头补)。
//! F15:全部可视化类型派生 `Serialize`,供 cc-monitor 经 Tauri `invoke` 直传前端。

use serde::{Deserialize, Serialize};

/// 稳定符号 id:impl 方法 = `file#Type::method`,自由函数 = `file#name`(F02 已定死);
/// 残余同文件同名再追加 `@行号` 消歧(见 `symbols.rs`),避免落库 PRIMARY KEY 冲突丢符号。
pub type SymbolId = String;

/// 符号 id 的**结构化拆分** —— 拆法只住这里(`file#段[@行号]`;没有 `#` 的是文件级 id,如 `Imports` 边的端点)。
///
/// 该带文件的地方直接带上它(`Overview.entry_points` · `symbols_touching`),消费方读字段,
/// 不照着 id 的格式自己拆 —— 格式哪天变了,拆的人会一起静默坏。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SymbolRef {
    pub id: SymbolId,
    /// 仓库相对路径(`#` 之前)。
    pub file: String,
    /// `#` 之后、去掉 `@行号` 消歧的那一段(`Type::method` / `f`);文件级 id ⇒ `None`。
    /// 批注与文档关联按它挂(同名多符号共用一段 —— 与 `annotations_for` / `docs_for` 的比法一致)。
    pub symbol: Option<String>,
}

impl SymbolRef {
    pub fn of(id: &str) -> SymbolRef {
        let (file, symbol) = match id.split_once('#') {
            Some((file, rest)) => (
                file.to_string(),
                Some(rest.split('@').next().unwrap_or(rest).to_string()),
            ),
            None => (id.to_string(), None),
        };
        SymbolRef {
            id: id.to_string(),
            file,
            symbol,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SymKind {
    Function,
    Method,
    Class,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
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
    /// 返回类型的**类型名**(`-> Graph` → `Graph`;`-> Result<Graph, E>` → 取最外层名)。
    /// 从 AST 的 `return_type` 字段抽,不是从 `signature` 字符串里解 —— 后者九门形态各异,脆。
    ///
    /// 用途:`let g = Graph::build(..)` 之后,`g.communities()` 的接收者类型就知道了 ——
    /// **这仍是语法可及的,不需要类型推导引擎**(见 `graph::collect_local_types`)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    /// 哪几个**形参位**的值会流到返回值,逗号分隔(`"0,2"`)。`None` = 没有 / 算不出。
    ///
    /// `fn h(a, b) -> T { a }` ⇒ `"0"`。于是 `let y = h(x, z)` 之后 `y` 的来源只有 `x`;
    /// 没有这条的话 `z` 也会被算成「可能流到」—— 那是假阳。
    ///
    /// ⚠ 与 `arg_flow` 同一种口径:**只认语法可及的传递**,被变换过的照样算(宽),
    /// 存进结构体再取出的追不到(窄)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_flow: Option<String>,
    /// **函数摘要**:这个函数把哪个形参的值写进了哪个形参**所指对象**的哪条字段。
    /// 逗号分隔的 `i~j:path`(`store(dst, v){ dst.f = v }` ⇒ `"1~0:f"`)。
    ///
    /// 用途是过程间传播:调用点 `store(&mut s, x)` 见到这条摘要,就能在**那一行**
    /// 记下一个定义 `s.f ← x`,于是后面的 `sink(s.f)` 追得到 `x`。
    /// 没有它的话,值一旦经由「写进参数」离开函数就断了 —— 而那是最常见的一种传递。
    ///
    /// 🔴 一律是 `~`(**可能**):写可能在分支里,摘要不记条件。
    /// ⚠ 只认**字段写**(`dst.f = v`)。`dst = v`(重赋形参本身)不记 —— 值语义下传不出去;
    /// `dst.push(v)`(方法可能改)也不记 —— 那要知道 `push` 干了什么,是猜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param_flow: Option<String>,
    #[serde(skip)] // 内部内容指纹,非前端数据;u64 经 JSON number 会 >2^53 丢精度,不外传
    pub body_hash: u64,
}

/// **实现关系**:`impl Tr for A` / `class A implements Tr` ⇒ `("A", "Tr")`。
///
/// 用途只有一个:接收者类型是 `Tr` 时,这是**动态派发**,候选是各实现者的同名方法。
/// 🔴 它也是「`Tr` 是不是一个 trait」的唯一判据 —— 仓内一个实现都没有的 trait 认不出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplRel {
    pub type_name: String,
    pub trait_name: String,
}

/// 类型的**字段类型标注**:`struct Engine { idx: Index }` ⇒ `("Engine", "idx", "Index")`。
///
/// 🔴 **不做成 `Symbol`** —— 字段不可调用。塞进符号表会污染 `search`、符号计数,
/// 而且字段零入边会被「入口点」那条查询当成入口。它只有一个用途:定接收者类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldType {
    /// 所属类型名(`Engine`)。
    pub owner: String,
    pub field: String,
    /// 字段的类型名(取最外层,`Vec<T>` → `Vec`)。
    pub ty: String,
}

/// 参数流与函数摘要里的**槽位**:形参第 n 位,或**接收者**(`self` / `this`)。
///
/// 🔴 接收者必须算一个槽位,否则摘要在真实代码上几乎不响 ——
/// `fn set(&mut self, v) { self.f = v; }` 是最常见的那一形,而 `self` 不在形参表里。
/// (实测:本仓 870 个符号里,「写另一个形参的字段」**一次都没有**;
///  改成接收者也算槽位之后才有真实产出。)
///
/// 文本形:接收者写 `s`,形参写十进制位号。
/// ⚠ 这让 `arg_flow` 里出现 `s>s` 这类条目 —— 旧的 `parse_flow_pair` 解析失败即跳过,
/// 所以 `flow` 查询不受影响(它只认形参位,接收者那条它读不懂也用不上)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// 接收者(`self` / `this`)。
    Recv,
    /// 第 n 个形参(不含接收者)。
    Param(usize),
}

impl Slot {
    pub fn parse(t: &str) -> Option<Slot> {
        let t = t.trim();
        if t == "s" {
            return Some(Slot::Recv);
        }
        t.parse().ok().map(Slot::Param)
    }

    pub fn token(&self) -> String {
        match self {
            Slot::Recv => "s".to_string(),
            Slot::Param(i) => i.to_string(),
        }
    }

    /// 槽位表 = `[接收者] ++ 形参`。下标 0 是接收者,之后依次是形参。
    /// 这样「顺来源表追回某个槽位」可以直接复用按形参位查的那套机制。
    pub fn from_index(i: usize) -> Slot {
        match i {
            0 => Slot::Recv,
            n => Slot::Param(n - 1),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

/// 人挂在代码上的锚点。Phase 1 只用文件/符号级;块级字段留占位(F09 才实现解析)。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Location {
    pub file: String,
    pub line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Resolution {
    pub state: AnchorState,
    pub location: Option<Location>,
    pub content_changed: Option<bool>,
    pub candidates: Vec<Location>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct IndexStats {
    pub files: usize,
    pub symbols: usize,
    // F18 覆盖完整度信号(诚实标注"漏了多少"):
    /// 识别为调用点但被调不在符号表(外部/stdlib/漏抓)→ 未建成边的调用点数。
    pub unresolved_calls: usize,
    /// **歧义**调用点数:仓内有同名候选但分不出是哪个(含全部方法调用)→ 未建成 `Calls` 边。
    /// 与 `unresolved_calls`(仓内一个同名都没有)是两回事:那是「看不见」,这是「看得见但分不清」。
    /// 🔴 这个数**大**是常态 —— 静态层对方法调用的接收者类型无从判定,那是编译器的活。
    pub ambiguous_calls: usize,
    /// 识别为 import 但目标不在仓内文件表(外部包/stdlib/本门语言解析不到文件)
    /// → 未建成边的 import 条数。与 `unresolved_calls` **分开计** —— 两者成因不同,
    /// 合成一个数会让「这张图漏了多少」说不清是漏在调用还是漏在依赖。
    pub unresolved_imports: usize,
    /// 解析报错**且未产出任何符号**(硬失败)的源文件数——只计真失败,滤掉 grammar 底噪。
    pub parse_errors: usize,
}

/// 建索引走到哪了(`Engine::index_with_progress` 的回调参数)。
///
/// 每一阶段先报一次 `done = 0`(说出这一阶段一共几份),之后每做完一份报一次;阶段按 [`IndexPhase`] 的顺序走。
/// 🔴 份数是**这一阶段**的,不是全程的 —— 各阶段每份的耗时差很多(解析远重于建边),合成一个百分比会骗人。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct IndexProgress {
    pub phase: IndexPhase,
    /// 这一阶段做完了几份(阶段内单调增)。
    pub done: usize,
    /// 这一阶段一共几份(阶段内不变)。
    pub total: usize,
}

/// 建索引的阶段(按这个顺序走)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum IndexPhase {
    /// 逐个源文件解析、抽符号(份 = 源文件)。
    Parse,
    /// 第一趟建边(份 = 源文件)。
    Link,
    /// 函数摘要抬高之后的第二趟(只重建会变的文件;份 = 源文件)。
    Relink,
    /// 扫 `.md` 建文档关联(份 = 文档文件)。
    Docs,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct IndexDelta {
    pub updated_files: usize,
    pub added: usize,
    pub removed: usize,
}

// ── 调用图(F02)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum EdgeKind {
    /// **一条事实**:这个调用点唯一地解析到了那个符号。
    /// 不变量:`Calls` ⟺ `Confidence::Exact` ⟺(非方法调用 ∧ 仓内候选唯一)。
    Calls,
    /// **端点形状与 `Calls` 不同**:`Imports` 的端点是**文件级 id**(裸仓库相对路径,无 `#`),不是符号 id ——
    /// 一条 import 的来源是整个文件,没有哪个符号当得起它。消费方按 `kind` 分流,别混着走。
    Imports,
    /// **一条「我不知道是哪个」**:这个调用点在仓内有多个同名候选,或它是方法调用
    /// (接收者类型未知,静态层无从判定)。`to` 指向候选中 **id 最小**的那个,`candidates` 记候选数。
    ///
    /// 🔴 **`to` 是妥协,不是真相** —— 取 id 最小只为确定性,**不代表它更可能对**。
    /// 读到这条边的人必须知道自己在看「N 选 1 里的一个」,别当调用事实用。
    ///
    /// 为什么不摊平成 N 条 `Calls`(这是 2026-09-16 之前的做法):
    /// 那会把**一条一对多的歧义**记成**多条一对一的断言**,其中至少 N-1 条是假的。
    /// 实测代价:`callers(annotations.rs#write)` 答 117 个而真相是 2 个;
    /// 且同名符号的度数被乘以候选数,凭空造出假 hub,把社区检测压塌。
    AmbiguousCall,
}

/// 边的可信度(F10):静态调用图对动态语言不可判定,一律标注而非假装 sound。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Confidence {
    /// 唯一名 / 限定名唯一匹配。
    Exact,
    /// **动态派发**:接收者类型已知是仓内的 trait / 接口,候选 = 它在仓内的全部实现。
    ///
    /// 🔴 与 `DynamicGuess` 是**两种认知状态**,不该混:
    /// 这一档的候选集是**完整且正确**的(只是运行时才知道是哪个);
    /// `DynamicGuess` 的候选集是按名字凑的,可能整个都不对。
    Dispatch,
    /// 多个同名候选(限定名匹配到不止一个)。
    Heuristic,
    /// 方法调用等,**接收者类型未知** —— 候选是按名字凑的。
    DynamicGuess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
    pub call_site_line: Option<usize>,
    pub confidence: Confidence,
    /// `AmbiguousCall` 专用:这个调用点在仓内有几个同名候选。
    /// `None` = 不适用(`Calls` / `Imports`)。
    /// **消费方要把它印出来** —— 「N 选 1」这件事不写在脸上,这条边就又变回一句假话。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<usize>,
    /// **参数流**:这条调用边上,调用方的第 i 个形参被原样传给了被调的第 j 个形参。
    /// 形如 `"0>1,2>0"`(调用方参数位 `>` 被调参数位),按 i 升序。`None` = 没有这种传递。
    ///
    /// 只记**实参是个裸标识符、且它正好是调用方的形参**这一种 —— 那是纯语法能确定的。
    /// `f(a.b)` / `f(g(x))` / 字面量一律不记。
    ///
    /// ⚠ 同一对 (调用方, 被调) 若有多个调用点,这里是**并集** —— 边只有一条,
    /// 分不出是哪个调用点传的。要精确到调用点得按调用点建边,那是另一种图。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg_flow: Option<String>,
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
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RankedFile {
    pub file: String,
    pub score: f64, // 该文件所有符号 PageRank 之和
    pub symbols: usize,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Subsystem {
    pub label: String,      // 代表(成员最多的文件)
    pub files: Vec<String>, // 涉及文件
    pub size: usize,        // 成员符号数
    /// 成员集合的指纹(成员 id 升序后 FNV-1a,十六进制)。
    ///
    /// 🔴 **这不是身份,是「变没变」** —— 任何一个成员进出都会让它变。
    /// 拿它当对账的配对键,会把「这团动了一下」读成「旧团消失 + 新团出现」。
    /// 要配对请用 `anchors`。
    pub member_hash: String,
    /// 按 PageRank 降序取的代表符号(最多 3 个)。两个用途:
    /// ① **给人看** —— 一眼知道这团在干嘛,好给它命名;
    /// ② **给对账当配对锚** —— 它比 `label` 稳(文件一改名 label 就换人),
    ///    也比 `member_hash` 稳(那个按定义就不稳)。
    ///
    /// ⚠ 诚实边界:社区真分裂时两半会各自认领不同的锚,配对会认成两个新团 ——
    /// 那不是配对失灵,那就是真的分裂了。
    pub anchors: Vec<SymbolId>,
    /// 团**内部**的调用边数(内聚的原始计数)。
    pub internal_edges: usize,
    /// 恰有一端在本团的调用边数(耦合的原始计数;同一条跨团边在两个团各记一次)。
    ///
    /// ⚠ 与 `internal_edges` 一样**只是计数,不是分数** —— 见 `rank::Graph::community_cut`。
    /// 另:两个数都只算 `Calls` 边(社区本来就是从调用图聚出来的),不含 `Imports`。
    pub external_edges: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Overview {
    pub spine_files: Vec<RankedFile>,
    pub subsystems: Vec<Subsystem>,
    /// 入口点(零入边、按 PageRank 排)。带 `file`:消费方按文件画的时候读字段,不拆 id。
    pub entry_points: Vec<SymbolRef>,
    pub total_symbols: usize,
    pub total_files: usize,
    // F18 覆盖信号(基于上次全量 index;见 IndexStats 同名字段)。
    pub unresolved_calls: usize,
    pub ambiguous_calls: usize,
    pub unresolved_imports: usize,
    pub parse_errors: usize,
    /// **读库出错的记录**(去重)。非空 = 这份全景**不完整**,不是「这仓就这样」。
    ///
    /// 🔴 为什么要有这条:查询接口返回 `Vec` 而不是 `Result`(调用方要的就是一张表),
    /// 出错时退成空表 —— 而「出错」与「确实没有」长得一模一样。
    /// 真实事故:`all_edges` 少查一列 ⇒ 全景**变空但不报错**。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub db_errors: Vec<String>,
}

// ── impact / blast-radius(F04)──

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AffectedSymbol {
    pub id: SymbolId,
    pub depth: usize, // 反向距离(1 = 直接调用者)
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImpactSet {
    pub root: SymbolId,
    pub affected: Vec<AffectedSymbol>,
}

impl ImpactSet {
    pub fn total(&self) -> usize {
        self.affected.len()
    }
}

// ── 控制流图(第三档地基,2026-09-17)──

/// 一个基本块:一段**一旦进入就会从头跑到尾**的语句序列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CfgBlock {
    pub id: usize,
    /// 块内第一/最后一条语句的行号。空块(纯汇合点)两者相等。
    pub start_line: usize,
    pub end_line: usize,
    /// 这一块里的调用点(被调裸名, 行号)。裸名 —— 解析到哪个符号是调用图那边的事。
    pub calls: Vec<(String, usize)>,
    /// 这一块里的**局部变量定义点**,按行号升序。给到达定义分析用。
    pub defs: Vec<CfgDef>,
}

/// 一个局部变量的定义点:`let y = h(x) + 1;` ⇒ `{ var: y, line, sources: [h, x] }`。
///
/// `sources` 是**这一次赋值**提到的标识符 —— 与流不敏感那张表的区别正在于此:
/// 那张表把一个变量的**所有**赋值混成一堆,这里一次赋值一条,由到达定义决定哪条算数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CfgDef {
    pub var: String,
    pub line: usize,
    /// 源文件里的**字节偏移** —— 定序用。
    ///
    /// 🔴 行号排不了同一行内的先后:`let s = mk(x); sink(s.f);` 写在一行时,
    /// 「定义在使用之前」这件事按行号判是 `4 < 4` = 假 ⇒ 整条链断掉。
    /// 字节偏移没这个问题。`line` 留着给人看。
    pub at: usize,
    pub sources: Vec<String>,
}

/// 一个函数体的控制流图。
///
/// 🔴 **`unsupported` 非空时整张图作废,不许用。** 这是本模块最要紧的一条:
/// 一张漏了某条边的 CFG,会把「这两个调用互斥」这种结论变成**假事实** ——
/// 而假事实比没有答案坏得多。遇到不建模的控制结构就整份弃掉,如实说是哪个。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Cfg {
    pub function: SymbolId,
    pub blocks: Vec<CfgBlock>,
    /// 有向边 (from_block, to_block)
    pub edges: Vec<(usize, usize)>,
    /// 入口块 id;出口块恒为最后一个
    pub entry: usize,
    pub exit: usize,
    /// 🔴 非空 = 这张图**不可用**,值是踩到的那个构造名。
    pub unsupported: Option<String>,
}

impl Cfg {
    /// 两个调用**会不会都发生** —— 互斥当且仅当两块互相都到不了。
    ///
    /// ⚠ 这是行号给不了的信息:两个调用写在前后两行,可能分属互斥的 if 分支。
    /// ⚠ `unsupported` 非空时恒返回 `false`(不知道 ≠ 不互斥,但不许瞎判)。
    pub fn mutually_exclusive(&self, a: usize, b: usize) -> bool {
        if self.unsupported.is_some() || a == b {
            return false;
        }
        !self.reaches(a, b) && !self.reaches(b, a)
    }

    /// 块 `from` 顺控制流到不到得了块 `to`。
    pub fn reaches(&self, from: usize, to: usize) -> bool {
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![from];
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            for (f, t) in &self.edges {
                if *f == n {
                    stack.push(*t);
                }
            }
        }
        false
    }
}

// ── 参数流(2026-09-16)──

/// 值流到的一个落点:某函数的某个形参位。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FlowStep {
    pub id: SymbolId,
    /// 形参位(0 基;**不含**接收者形参)
    pub param: usize,
    /// 从起点算起的跳数
    pub depth: usize,
    /// 🔴 `false` = **确定**原样传下去(实参就是那个形参,或它的借用);
    /// `true` = **可能**流到 —— 值被变换过或经中间变量,由**流不敏感**的过程内分析得出。
    ///
    /// 流不敏感意味着**过度近似**:`y = a; g(y); y = b; g(y)` 会算成 a 和 b 都流到两处。
    /// 链上只要有一跳是 `true`,整条结论就只能当「可能」读。
    pub derived: bool,
}

/// 「这个参数流到哪去了」的结果。
///
/// 🔴 **它只追「原样传下去」这一种流动** —— 实参必须是个裸标识符且正好是当前形参。
/// 被变换过(`g(x + 1)` / `g(x.field)` / `g(h(x))`)、被存进结构体再取出来、
/// 经闭包捕获的,**一律追不到**。那些要函数体内的定义-使用链(第三档)。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FlowSet {
    pub root: SymbolId,
    pub param: usize,
    pub steps: Vec<FlowStep>,
    /// 触到深度上限。**截断了必须说**,否则「没再往下」会被读成「到头了」。
    pub truncated: bool,
}

// ── 调用路径(2026-09-16)──

/// 一条调用路径:逐跳的边。`steps[0].from` = 起点,`steps.last().to` = 终点。
///
/// 🔴 **这是「调用关系上存在一条链」,不是「执行轨迹」。** 图里没有分支、循环、
/// 提前返回、条件,也没有先后。同一函数体内多个调用点按行号排只是**书写顺序**,
/// 不是运行顺序 —— 别把它读成 trace。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CallPath {
    pub steps: Vec<Edge>,
}

impl CallPath {
    /// 跳数(= 边数)。
    pub fn hops(&self) -> usize {
        self.steps.len()
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PathSet {
    pub from: SymbolId,
    pub to: SymbolId,
    pub paths: Vec<CallPath>,
    /// 因条数或深度上限被截断。**截断了必须说** ——
    /// 不说,读者会把「我只找了这么多」读成「一共就这么多」。
    pub truncated: bool,
    /// 搜索过程中遇到并切断的环数(递归 / 互递归)。0 = 没遇到。
    pub cycles_cut: usize,
}

// ── doc-links(F05)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum LinkSource {
    Colocation,  // 目录里的 README.md
    Frontmatter, // md 头部 covers:
    Inline,      // 正文 [..](path)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DriftItem {
    pub doc_path: String,
    pub target_file: String,
    pub target_symbol: Option<String>,
    pub reason: String, // 为什么算漂移
}

// ── 批注(F07)—— 侧车 JSON 文件为唯一真相(人写、可版本化)──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum AnnotationStatus {
    Active,   // 人写 / 已批准 —— agent 可消费
    Proposed, // agent 提议 —— 待人审批准
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Annotation {
    pub id: String,
    pub file: String,           // 锚点文件
    pub symbol: Option<String>, // 锚点符号(None = 文件级)
    pub body: String,           // 人写的批注正文
    pub author: String,
    pub status: AnnotationStatus,
    /// **谁写的**(2026-09-24)—— 与 `status`(审没审)是两件事,批准只改 `status` 不改它。
    ///
    /// 🔴 为什么要有:批准之后「人写的」与「agent 提议、人点了批准的」都是 `Active`,
    /// 此前只剩 `author` 一个自由文本能分 —— 那是约定,不是数据。而人那一侧的价值是
    /// 「人指出哪里不对」:agent 读到**自己**提的话、却当成人的指示,是这一族最贵的错。
    /// 旧侧车文件没有这个字段 ⇒ `Unrecorded`(不猜)。
    #[serde(default)]
    pub origin: AnnotationOrigin,
}

/// 批注的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum AnnotationOrigin {
    /// 人写的(`add_annotation`)。
    Human,
    /// agent 提议的(`propose_annotation`)—— 批准之后仍是它。
    Agent,
    /// 旧文件没记来源。**不猜**:`Active` 的旧批注可能是人写的,也可能是批准过的提议。
    #[default]
    Unrecorded,
}

// ── cc-monitor 视图聚合(F15)—— Engine 一等返回,MCP 与 cc-monitor 共用 ──

/// 单个符号的完整视图:符号本体 + 直接调用者/被调 + 关联文档 + 批注。
/// 由 `Engine::node` 组装(此前散在 MCP 层现拼,cc-monitor 得自己组合;F15 收口)。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct NodeView {
    pub symbol: Symbol,
    pub callers: Vec<Edge>,
    pub callees: Vec<Edge>,
    pub docs: Vec<DocLink>,
    pub annotations: Vec<Annotation>,
}

/// 以某符号为心的邻域调用子图(双向、可控深度):节点集 + 边集。
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SubGraph {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
}

/// 以某符号为心的邻域,**每个够得着的符号带距根几跳**(`Engine::neighborhood`)。
///
/// 与 [`SubGraph`] 同一个邻域(同一个 depth 口径),只是把「第几跳」这个量直接给出来 ——
/// 消费方要按跳数分层画图时不必自己再走一遍图(那等于在消费方那一侧算图)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Neighborhood {
    pub root: SymbolId,
    /// 根不在里面。depth 升序、同层 id 升序。
    pub reached: Vec<Reached>,
}

/// 邻域里的一个符号与它距根几跳(1 = 直接调它 / 它直接调)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Reached {
    pub id: SymbolId,
    pub depth: usize,
}
