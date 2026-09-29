//! **图种注册表**(2026-09-24):「有哪些图、每张图要什么输入、画出来是什么形状」的**唯一真相源**。
//!
//! 🔴 为什么要有它:此前图种写了两遍 —— MCP `schema.rs` 的 `"enum":["arch","calls"]` 与
//! `tools.rs` 的 `match kind` —— 而且**已经漂了**:schema 缺 `module`/`uml`、说 `arch` 是默认
//! (其实是 `module`),`tools.rs` 对认不出的 kind 用 `_ =>` **静默画成模块图**。
//! 认错一个字母的请求拿回一张「看起来对」的别的图,没有人会发现。
//!
//! ⇒ 这里是唯一一处写图种的地方。MCP 的 schema 枚举、分派、参数表**全从这里派生**;
//! 第三方(cc-monitor)经 [`kinds`] 原样拿到这张表,按 [`DiagramShape`] 渲染 ——
//! 上游加一种**已有形状**的新图,消费方零改动就能画。
//!
//! 认不出的 kind 是**错误**([`DiagramError::UnknownKind`]),不回落。

use crate::diagram::DrawOpts;
use crate::model::SymbolId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// 图种。**加一种图 = 在这里加一个变体 + `ALL` + `info()`**,别处不用写它的名字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagramKind {
    /// 模块依赖图(目录级)—— 你声明的结构。
    Module,
    /// 社区架构图(Louvain 子系统级)—— 实际存在的耦合。
    Arch,
    /// 某符号周围的调用子图。
    Calls,
    /// UML 类图(类型 · 实现 · 组合)。
    Uml,
}

impl DiagramKind {
    /// 全部图种,**按展示顺序**。
    pub const ALL: [DiagramKind; 4] = [
        DiagramKind::Module,
        DiagramKind::Arch,
        DiagramKind::Calls,
        DiagramKind::Uml,
    ];

    /// 不给 kind 时画哪张。目录级最像人期待的架构图;社区图信息量更大但要解释。
    pub const DEFAULT: DiagramKind = DiagramKind::Module;

    /// 稳定 id —— 写进协议(MCP 的 `kind`、cc-monitor 的命令参数)。**改它就是改协议。**
    pub fn id(self) -> &'static str {
        match self {
            DiagramKind::Module => "module",
            DiagramKind::Arch => "arch",
            DiagramKind::Calls => "calls",
            DiagramKind::Uml => "uml",
        }
    }

    /// id → 图种。🔴 认不出就是错误,**不许回落成默认图**。
    pub fn from_id(id: &str) -> Result<DiagramKind, DiagramError> {
        DiagramKind::ALL
            .into_iter()
            .find(|k| k.id() == id)
            .ok_or_else(|| DiagramError::UnknownKind {
                given: id.to_string(),
                known: DiagramKind::ALL
                    .iter()
                    .map(|k| k.id().to_string())
                    .collect(),
            })
    }

    /// 这张图的全部元数据。
    pub fn info(self) -> DiagramKindInfo {
        use DiagramParam::*;
        match self {
            DiagramKind::Module => DiagramKindInfo {
                kind: self,
                title: "模块依赖图",
                summary: "目录当节点、跨目录的调用当连接 —— 你声明的结构",
                params: &[MaxNodes, CertainOnly, ExcludeTests],
                shape: DiagramShape::Clusters,
            },
            DiagramKind::Arch => DiagramKindInfo {
                kind: self,
                title: "架构图(社区)",
                summary: "按调用密度切出的子系统当节点 —— 实际存在的耦合;与模块图不一致处最值得看",
                params: &[MaxNodes, CertainOnly, ExcludeTests],
                shape: DiagramShape::Clusters,
            },
            DiagramKind::Calls => DiagramKindInfo {
                kind: self,
                title: "调用子图",
                summary: "某个符号周围的调用关系(双向,按跳数)",
                params: &[Symbol, Depth, CertainOnly],
                shape: DiagramShape::CallGraph,
            },
            DiagramKind::Uml => DiagramKindInfo {
                kind: self,
                title: "类图(UML)",
                summary: "类型 · 实现 · 组合 —— 关系来自声明,不是推断",
                params: &[MaxNodes],
                shape: DiagramShape::TypeGraph,
            },
        }
    }

    /// 这张图要不要先给一个符号。
    pub fn needs_symbol(self) -> bool {
        self.info().params.contains(&DiagramParam::Symbol)
    }
}

impl fmt::Display for DiagramKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// 线上就是它的 id(`"module"`),不是变体名。
impl Serialize for DiagramKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id())
    }
}

impl<'de> Deserialize<'de> for DiagramKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let id = String::deserialize(d)?;
        DiagramKind::from_id(&id).map_err(serde::de::Error::custom)
    }
}

/// 一张图画出来的**形状**。消费方按它选渲染器 —— 不按图种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagramShape {
    /// 节点 + **带成分**的聚合连接(团 / 模块)。
    Clusters,
    /// 符号级调用子图(一条边就是一条边,不聚合)。
    CallGraph,
    /// 类型 + 实现/组合关系。
    TypeGraph,
}

impl DiagramShape {
    pub const ALL: [DiagramShape; 3] = [
        DiagramShape::Clusters,
        DiagramShape::CallGraph,
        DiagramShape::TypeGraph,
    ];
}

/// 一张图认的**输入**。id 与 [`DiagramRequest`] 的字段名**逐字相同**(有测试钉两向相等)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagramParam {
    /// 以哪个符号为中心(全限定 id)。
    Symbol,
    /// 跳数。
    Depth,
    /// 最多画几个节点。
    MaxNodes,
    /// 只画有确定成分的连接。
    CertainOnly,
    /// 把测试排除在外。
    ExcludeTests,
}

impl DiagramParam {
    pub const ALL: [DiagramParam; 5] = [
        DiagramParam::Symbol,
        DiagramParam::Depth,
        DiagramParam::MaxNodes,
        DiagramParam::CertainOnly,
        DiagramParam::ExcludeTests,
    ];

    pub fn id(self) -> &'static str {
        match self {
            DiagramParam::Symbol => "symbol",
            DiagramParam::Depth => "depth",
            DiagramParam::MaxNodes => "max_nodes",
            DiagramParam::CertainOnly => "certain_only",
            DiagramParam::ExcludeTests => "exclude_tests",
        }
    }

    /// JSON Schema 里的类型名(MCP schema 由它生成)。
    pub fn json_type(self) -> &'static str {
        match self {
            DiagramParam::Symbol => "string",
            DiagramParam::Depth | DiagramParam::MaxNodes => "integer",
            DiagramParam::CertainOnly | DiagramParam::ExcludeTests => "boolean",
        }
    }

    /// 给人/agent 看的一句话(含默认值)。
    pub fn describe(self) -> &'static str {
        match self {
            DiagramParam::Symbol => "中心符号的全限定 id",
            DiagramParam::Depth => "跳数,默认 2",
            DiagramParam::MaxNodes => "最多画几个节点,默认 12;没画的会记进图注",
            DiagramParam::CertainOnly => "只画有确定成分的连接,默认 true;滤掉多少写进图注",
            DiagramParam::ExcludeTests => "把测试排除在外,默认 true;排掉多少写进图注",
        }
    }
}

/// 一种图的元数据 —— [`kinds`] 原样吐给第三方。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagramKindInfo {
    /// 线上是 id(`"module"`)。
    #[serde(rename = "id")]
    pub kind: DiagramKind,
    /// 人读名。
    pub title: &'static str,
    pub summary: &'static str,
    /// 这张图**认**哪些输入。不在表里的输入会被忽略(不报错),所以消费方只该给人拧这几个。
    pub params: &'static [DiagramParam],
    pub shape: DiagramShape,
}

/// 注册表全表,按 [`DiagramKind::ALL`] 的顺序。
pub fn kinds() -> Vec<DiagramKindInfo> {
    DiagramKind::ALL
        .into_iter()
        .map(DiagramKind::info)
        .collect()
}

/// 一次画图请求。**一个类型喂所有图**;全部字段可缺,缺了取默认。
/// 字段名与 [`DiagramParam::id`] 逐字相同。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DiagramRequest {
    pub symbol: Option<SymbolId>,
    pub depth: Option<u32>,
    pub max_nodes: Option<usize>,
    pub certain_only: Option<bool>,
    pub exclude_tests: Option<bool>,
}

impl DiagramRequest {
    pub const DEFAULT_DEPTH: u32 = 2;

    /// 折成 [`DrawOpts`](缺的取默认)。`max_nodes` 至少 1。
    pub fn draw_opts(&self) -> DrawOpts {
        let d = DrawOpts::default();
        DrawOpts {
            max_nodes: self.max_nodes.unwrap_or(d.max_nodes).max(1),
            certain_only: self.certain_only.unwrap_or(d.certain_only),
            exclude_tests: self.exclude_tests.unwrap_or(d.exclude_tests),
        }
    }

    pub fn depth(&self) -> u32 {
        self.depth.unwrap_or(Self::DEFAULT_DEPTH)
    }
}

/// 画图失败。🔴 每一种都是**明说**的失败,没有一种会回落成「画一张别的图」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum DiagramError {
    /// 认不出的图种。带上认得的全表,调用方不用再查一次。
    UnknownKind { given: String, known: Vec<String> },
    /// 这张图要一个中心符号,没给。
    NeedsSymbol { kind: DiagramKind },
    /// 给的符号不在索引里。
    SymbolNotFound { symbol: SymbolId },
}

impl fmt::Display for DiagramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagramError::UnknownKind { given, known } => {
                write!(f, "认不出的图种 `{given}`;认得的:{}", known.join(" / "))
            }
            DiagramError::NeedsSymbol { kind } => {
                write!(f, "`{kind}` 图需要 symbol 参数(中心符号的全限定 id)")
            }
            DiagramError::SymbolNotFound { symbol } => write!(f, "找不到符号 {symbol}"),
        }
    }
}

impl std::error::Error for DiagramError {}
