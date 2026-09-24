//! **精确层的接缝**(2026-09-17)。
//!
//! core 是**自足**的:它不认识 LSP,也不依赖任何语言服务器(见 `Cargo.toml` 的说明)。
//! 但静态层有一类问题**语法上无解** —— 外部类型的方法与仓内符号重名:
//!
//! ```text
//! tx.commit()     // tx 是 rusqlite 的 Transaction ⇒ 仓内那两个 `commit` 候选**全是错的**
//! v.iter().ok()   // `ok` 在仓内也有同名的
//! ```
//!
//! 这一层只做两件事,**不发起任何查询**:
//! 1. [`Engine::precise_targets`] —— 把「静态层分不清的调用点」连同**精确坐标**吐出来;
//! 2. [`Engine::apply_precise`] —— 把外面解出来的答案写回。
//!
//! 谁去解由调用方决定(`code-picture-lsp` 提供了 LSP 实现)。依赖方向因此不变:
//! **lsp → core,core 不反依赖**。
//!
//! 🔴 **写回是有损的,所以定了两条保守规矩**:
//! * 边按 `(from, to)` 去重,一条边可能盖住多个调用点 ⇒ 以 `(调用方, 被调裸名)` 为单位处理;
//! * 同一单位下的多个调用点若**答案不一致**,整个单位跳过 —— 分不清就不动,不挑一个。

use crate::engine::Engine;
use crate::model::{Confidence, Edge, EdgeKind, Lang, SymbolId};
use crate::{lang, symbols};
use std::collections::{HashMap, HashSet};
use std::error::Error;

/// 一个待精确解析的调用点。坐标是**被调名字 token** 的位置 ——
/// 给调用点起始坐标会问到接收者头上,答出另一个东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreciseTarget {
    /// 仓库相对路径(正斜杠)。
    pub file: String,
    /// 调用方符号 id。
    pub from: SymbolId,
    /// 被调裸名。
    pub name: String,
    /// 1-based 行。
    pub line: usize,
    /// 0-based 列(LSP 的 `character`)。
    pub col: usize,
}

/// 精确层对一个调用点的答复。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreciseOutcome {
    /// 解到仓内某个符号 ⇒ 这一处是**确定**的。
    InRepo(SymbolId),
    /// 解到仓外(stdlib / 第三方)⇒ 静态层那些候选**全是编的**,该清掉。
    External,
    /// 答不出 ⇒ **保持静态层的答案不动**。
    Unknown,
}

/// 一条答复,连同它回答的是哪个调用点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreciseAnswer {
    pub target: PreciseTarget,
    pub outcome: PreciseOutcome,
}

/// 写回的结果(诚实读数:说清楚动了多少、为什么没动更多)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreciseStats {
    /// 升成 `Calls`/`Exact` 的调用单位数。
    pub resolved: usize,
    /// 判为仓外、候选被清掉的调用单位数。
    pub externalized: usize,
    /// 同一单位下答案不一致 ⇒ **整个单位跳过**(分不清就不动)。
    pub conflicted: usize,
    /// 精确层答不出,保持原样。
    pub unknown: usize,
    /// 清掉的假候选边条数。
    pub edges_removed: usize,
}

impl Engine {
    /// 列出**静态层分不清**的调用点,带精确坐标,供精确层去解。
    ///
    /// 只列 `AmbiguousCall` 的。已经解出来的(`Calls`)不必再问,
    /// 未解析的(连候选都没有)也不问 —— 那类已经是诚实的「看不见」。
    ///
    /// ⚠ 要重新解析源文件才拿得到列号。文件读不到 / 本门没接入 ⇒ 那个文件整体跳过。
    pub fn precise_targets(&self) -> Result<Vec<PreciseTarget>, Box<dyn Error>> {
        // 🔴 `Dispatch` 的调用点**不问** —— 静态层在这一项上本来就更准。
        // LSP 的 `definition` 对 `dyn` 派发答的是**声明位置**(trait 那一份),不是被调;
        // 拿它覆盖会把「候选 = 各实现」压成「确定调 trait 的默认体」,
        // 而默认体被覆盖时根本不跑 —— 那是假事实,还顺手删掉了通往各实现的真边。
        // (实测:不拦这条,`Builder::hidden_ctrl → LangSupport::ctrl_role` 会变成 Calls/Exact。)
        let mut dispatch_sites: HashSet<(String, usize, String)> = HashSet::new();
        // 每个文件里「有歧义」的被调裸名。
        let mut wanted: HashMap<String, HashSet<String>> = HashMap::new();
        for e in self.all_ambiguous_edges()? {
            let file = e.from.split('#').next().unwrap_or(&e.from).to_string();
            let bare = bare_callee_name(&e.to);
            if e.confidence == Confidence::Dispatch {
                if let Some(line) = e.call_site_line {
                    dispatch_sites.insert((file, line, bare));
                }
                continue;
            }
            wanted.entry(file).or_default().insert(bare);
        }
        // 每文件的可调用符号(定"这个调用点在谁体内")。一次取全,不逐点查库。
        let mut by_file_syms: HashMap<String, Vec<(usize, usize, SymbolId)>> = HashMap::new();
        for s in self.all_symbols()? {
            if matches!(
                s.kind,
                crate::model::SymKind::Function | crate::model::SymKind::Method
            ) {
                by_file_syms.entry(s.file.clone()).or_default().push((
                    s.start_line,
                    s.end_line,
                    s.id,
                ));
            }
        }
        let mut out = Vec::new();
        for (file, names) in wanted {
            let Ok(src) = std::fs::read_to_string(self.repo_path().join(&file)) else {
                continue;
            };
            let (Some(lang), Some(syms)) = (Lang::from_path(&file), by_file_syms.get(&file)) else {
                continue;
            };
            let Some(spec) = lang::spec_for(lang) else {
                continue;
            };
            let Some(tree) = symbols::parse_with(lang, &src) else {
                continue;
            };
            // 🔴 **逐个调用点**枚举,不按边上记的行:一条边只记得住**一个**调用点行,
            // 同一函数里同名的多个调用点会被压成一个 —— 那样会拿一处的答案去删另一处的真边。
            let mut sites = Vec::new();
            collect_call_sites(spec, tree.root_node(), src.as_bytes(), &names, &mut sites);
            for (name, line, col) in sites {
                if dispatch_sites.contains(&(file.clone(), line, name.clone())) {
                    continue;
                }
                // 最内层的可调用符号 = 这个调用点所在的函数
                let Some(from) = syms
                    .iter()
                    .filter(|(a, b, _)| *a <= line && line <= *b)
                    .min_by_key(|(a, b, _)| b - a)
                    .map(|(_, _, id)| id.clone())
                else {
                    continue;
                };
                out.push(PreciseTarget {
                    file: file.clone(),
                    from,
                    name,
                    line,
                    col,
                });
            }
        }
        // 确定性:同一个仓两次跑出同一份清单
        out.sort_by(|a, b| {
            (&a.file, a.line, a.col, &a.name, &a.from)
                .cmp(&(&b.file, b.line, b.col, &b.name, &b.from))
        });
        out.dedup();
        Ok(out)
    }

    /// 把精确层的答复写回索引。
    ///
    /// 以 `(调用方, 被调裸名)` 为单位 —— 边按 `(from, to)` 去重,一条边可能盖住
    /// 同一调用方对同一被调的**多个调用点**,细不到单个调用点。
    /// 🔴 同一单位下的答案**不一致**(一处解到仓内、另一处解到仓外)⇒ **整个单位跳过**。
    /// 分不清就不动,不挑一个。
    pub fn apply_precise(
        &mut self,
        answers: &[PreciseAnswer],
    ) -> Result<PreciseStats, Box<dyn Error>> {
        let mut by_unit: HashMap<(SymbolId, String), Vec<&PreciseOutcome>> = HashMap::new();
        for a in answers {
            by_unit
                .entry((a.target.from.clone(), a.target.name.clone()))
                .or_default()
                .push(&a.outcome);
        }
        let mut stats = PreciseStats::default();
        // 先算好要改什么,再一次性落库 —— 中途失败不留半套
        let mut drop_edges: HashSet<(SymbolId, SymbolId)> = HashSet::new();
        #[allow(clippy::type_complexity)]
        let mut upgrade: Vec<(SymbolId, SymbolId, usize, Option<String>)> = Vec::new();
        for ((from, name), outs) in by_unit {
            let first = outs[0];
            if outs.iter().any(|o| *o != first) {
                stats.conflicted += 1;
                continue;
            }
            let olds: Vec<Edge> = self
                .ambiguous_callees(&from)
                .into_iter()
                .filter(|e| bare_callee_name(&e.to) == name)
                .collect();
            match first {
                PreciseOutcome::Unknown => stats.unknown += 1,
                PreciseOutcome::External => {
                    stats.externalized += 1;
                    stats.edges_removed += olds.len();
                    for e in olds {
                        drop_edges.insert((from.clone(), e.to));
                    }
                }
                PreciseOutcome::InRepo(to) => {
                    // 目标必须真的在符号表里,否则不动(精确层给了个我们不认识的位置)
                    if self.find_symbol(to).is_none() {
                        stats.unknown += 1;
                        continue;
                    }
                    stats.resolved += 1;
                    let line = olds.first().and_then(|e| e.call_site_line).unwrap_or(0);
                    // 参数流是**静态层算的**,与解析无关 —— 升级时要带过去,不能丢
                    let flow = olds.iter().find_map(|e| e.arg_flow.clone());
                    stats.edges_removed += olds.iter().filter(|e| &e.to != to).count();
                    for e in olds {
                        drop_edges.insert((from.clone(), e.to));
                    }
                    upgrade.push((from.clone(), to.clone(), line, flow));
                }
            }
        }
        self.write_precise(&drop_edges, &upgrade)?;
        Ok(stats)
    }
}

/// 从符号 id 取被调裸名:`file#Type::m` / `file#m@12` → `m`。
fn bare_callee_name(id: &str) -> String {
    let seg = id.rsplit('#').next().unwrap_or(id);
    let seg = seg.split('@').next().unwrap_or(seg);
    seg.rsplit("::").next().unwrap_or(seg).to_string()
}

/// 扫全文件,收下**名字在 `names` 里**的每一个调用点 `(名, 行, 列)`。
///
/// 逐点收、不按行去重 —— 同一行同名调用多次时两处都要问(它们可能是不同的被调)。
fn collect_call_sites(
    spec: &dyn lang::LangSupport,
    node: tree_sitter::Node,
    src: &[u8],
    names: &HashSet<String>,
    out: &mut Vec<(String, usize, usize)>,
) {
    if let Some((name, _, _)) = spec.call_of(node, src) {
        if names.contains(&name) {
            if let Some((line, col)) = lang::call_name_pos(node, src, &name) {
                out.push((name, line, col));
            }
        }
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        collect_call_sites(spec, ch, src, names, out);
    }
}

/// 升级后的边用这个置信度 —— 与静态层解出来的 `Exact` 分开,
/// 读的人该知道这条边是**另一层**给的答案。
pub(crate) const PRECISE_CONFIDENCE: Confidence = Confidence::Exact;

/// 精确层写回时建的边。
pub(crate) fn precise_edge(from: &str, to: &str, line: usize, arg_flow: Option<String>) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        kind: EdgeKind::Calls,
        call_site_line: (line > 0).then_some(line),
        confidence: PRECISE_CONFIDENCE,
        candidates: None,
        arg_flow,
    }
}
