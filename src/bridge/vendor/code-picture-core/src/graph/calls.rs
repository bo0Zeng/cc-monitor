//! `Calls` / `AmbiguousCall` 边:函数体里的调用点 → 仓内符号。
//!
//! 一个调用点**最多产一条边**(歧义时每候选一条,但一律标 `AmbiguousCall`)。
//! 解析规则住 `super::resolve`;参数流与局部类型住 `super::locals`。

use super::edges::upsert;
use super::locals::{arg_flow_of, collect_local_types};
use super::resolve::{qualifier_of_id, resolve, Resolved};
use super::table::SymbolTable;
use crate::lang::collect_local_sources;
use crate::lang::LangSupport;
use crate::model::{Confidence, Edge, EdgeKind, Slot, SymKind, Symbol};
use std::collections::HashMap;
use tree_sitter::Node;

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_calls(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file_syms: &[Symbol],
    current_fn: Option<&str>,
    table: &SymbolTable,
    edges: &mut Vec<Edge>,
    index_of: &mut HashMap<(String, String), usize>,
    unresolved: &mut usize,
    ambiguous: &mut usize,
    locals: &HashMap<String, String>,
    caller_params: &[Option<String>],
    caller_sources: &HashMap<String, Vec<String>>,
    caller_cfg: Option<&crate::model::Cfg>,
    caller_ctx: &crate::lang::FlowCtx,
    local_names: &std::collections::HashSet<String>,
) {
    // 沿 AST 祖先确定 enclosing 函数(比按行号归属更准:同行多函数/嵌套都对)
    let this_fn: Option<String> = if spec.symbol_at(node, src).is_some() {
        fn_id_of(spec, node, src, file_syms)
    } else {
        None
    };
    let enclosing: Option<&str> = this_fn.as_deref().or(current_fn);
    // 闭包:**不是**新函数(它的调用仍记在外层名下),但**是**新名字作用域。
    // 只在不是函数定义时问 —— 函数定义那条路已经全新建表了。
    let closure: Option<Vec<Option<String>>> = if this_fn.is_none() {
        spec.closure_params(node, src)
    } else {
        None
    };
    // 闭包形参盖住的名字(按**路径根**滤外层的表)
    let bound: std::collections::HashSet<&str> = closure
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .flatten()
        .map(String::as_str)
        .collect();
    // 🔴 局部变量类型表**必须先建** —— `effects` 要靠它定接收者类型(`s.set(..)` 的 `s` 是什么)。
    // 先前 `effects` 用的是不带接收者类型的那套解析,于是它和主路径对同一个调用点给出
    // 不同的解析结果:主路径解得出 `S::set`、`effects` 解不出 ⇒ 摘要一条都落不了地。
    // 两条路必须走同一套解析规则。
    // 局部绑定的名字(遮蔽模块级同名)。进函数/闭包时重建。
    let scope_names: Option<std::collections::HashSet<String>> =
        (this_fn.is_some() || closure.is_some()).then(|| {
            let mut n = if closure.is_some() {
                local_names.clone() // 闭包能看见外层的局部
            } else {
                std::collections::HashSet::new()
            };
            crate::lang::collect_local_names(spec, node, src, &mut n);
            n
        });
    let local_names: &std::collections::HashSet<String> =
        scope_names.as_ref().unwrap_or(local_names);

    let scope_locals: Option<HashMap<String, String>> = if this_fn.is_some() {
        let mut m = HashMap::new();
        collect_local_types(spec, node, src, table, &mut m);
        Some(m)
    } else if closure.is_some() {
        Some(layer(locals, &bound, |m| {
            collect_local_types(spec, node, src, table, m)
        }))
    } else {
        None
    };
    let locals: &HashMap<String, String> = scope_locals.as_ref().unwrap_or(locals);

    let own_params;
    let own_sources;
    // 缩窄器:`let y = h(x, z)` ⇒ 解析 h,看它的 `return_flow` 只留真能流到返回值的实参。
    // 解析不出 / h 没记 return_flow ⇒ 返回 None ⇒ 退回「提到的都算」(宽,方向安全)。
    let narrow = |init: Node| -> Option<Vec<String>> {
        let (name, qual, is_method) = spec.call_of(init, src)?;
        let Resolved::One(id) = resolve(&name, qual.as_deref(), is_method, table, false) else {
            return None;
        };
        let flow = table.return_flow_of(&id)?;
        let args = spec.call_args(init, src);
        Some(
            flow.split(',')
                .filter_map(|i| i.trim().parse::<usize>().ok())
                .filter_map(|i| args.get(i).cloned().flatten())
                .collect(),
        )
    };

    // 过程间副作用:被调若有**函数摘要**(`i~j:path`),在调用点记下 `实参j.path ← 实参i`。
    // 🔴 只认**唯一解析**的被调 —— 歧义时各候选摘要不同,挑哪个都是编。
    // ⚠ 被写的那个实参必须是**裸标识符**(`store(&mut s, ..)` 可,`store(&mut v[i], ..)` 不可):
    //   写进一个算出来的位置,得知道那位置是什么 —— 那是指向分析。
    let effects = |call: Node| -> Vec<(String, Vec<String>)> {
        let Some((name, qual, is_method)) = spec.call_of(call, src) else {
            return Vec::new();
        };
        // 与主路径同一套:接收者类型的两条语法可及来源(`self.m()` · 局部类型表)
        let self_qual = if spec.is_self_call(call, src) {
            enclosing.and_then(qualifier_of_id)
        } else {
            spec.receiver_of(call, src).and_then(|recv| {
                receiver_type(&recv, enclosing, spec.receiver_name(), locals, table)
            })
        };
        let (q, m) = match &self_qual {
            Some(q) => (Some(q.as_str()), false),
            None => (qual.as_deref(), is_method),
        };
        let Resolved::One(id) = resolve(&name, q, m, table, self_qual.is_some()) else {
            return Vec::new();
        };
        let Some(flow) = table.param_flow_of(&id) else {
            return Vec::new();
        };
        let args = spec.call_args(call, src);
        let mentions = spec.call_arg_mentions(call, src);
        // 接收者槽位落到**调用点的接收者**上:`obj.set(x)` 的接收者是 `obj`。
        // `self.set(x)` 的接收者是调用方自己的 `self` —— 那不是本函数的局部,
        // 记不成定义(要记得靠**调用方自己的**摘要往上传),故跳过。
        let recv = spec
            .receiver_of(call, src)
            .filter(|r| Some(r.as_str()) != spec.receiver_name());
        let slot_target = |sl: Slot| -> Option<String> {
            match sl {
                Slot::Recv => recv.clone(),
                Slot::Param(n) => args.get(n).cloned().flatten(),
            }
        };
        let slot_sources = |sl: Slot| -> Vec<String> {
            match sl {
                Slot::Recv => recv.clone().into_iter().collect(),
                Slot::Param(n) => mentions.get(n).cloned().unwrap_or_default(),
            }
        };
        let mut out = Vec::new();
        for (i, j, field) in crate::graph::summary::parse_param_flow(flow) {
            // 目标侧要裸标识符(写进一个算出来的位置得先知道那位置是什么 —— 那是指向分析);
            // 来源侧用「提到的路径」(宽,方向安全)
            let Some(target) = slot_target(j) else {
                continue;
            };
            let srcs = slot_sources(i);
            if srcs.is_empty() {
                continue;
            }
            out.push((format!("{target}.{field}"), srcs));
        }
        out
    };

    let own_cfg;
    let own_aliases;
    let own_ctx;
    #[allow(clippy::type_complexity)]
    let (locals, caller_params, caller_sources, caller_cfg, caller_ctx): (
        &HashMap<String, String>,
        &[Option<String>],
        &HashMap<String, Vec<String>>,
        Option<&crate::model::Cfg>,
        &crate::lang::FlowCtx,
    ) = if this_fn.is_some() {
        own_params = spec.param_names(node, src);
        // 别名表先建 —— 来源表与 CFG 的定义点都要靠它把路径的根归一
        let mut al = HashMap::new();
        crate::lang::collect_aliases(spec, node, src, &mut al);
        own_aliases = al;
        own_ctx = crate::lang::FlowCtx {
            narrow: Some(&narrow),
            aliases: &own_aliases,
            effects: Some(&effects),
        };
        let mut srcs = HashMap::new();
        collect_local_sources(spec, node, src, Some(&own_ctx), &mut srcs);
        own_sources = srcs;
        // 每个函数建一次 CFG:参数流的「可能」那一档靠它收紧成流敏感。
        // 建不出 / 作废 ⇒ None ⇒ 自动退回流不敏感(更宽,方向安全)。
        own_cfg = crate::cfg::outline(spec, node, src, Some(&own_ctx)).and_then(|o| {
            (o.unsupported.is_none()).then_some(crate::model::Cfg {
                function: String::new(), // 这里只用它的块与边,函数 id 不参与计算
                blocks: o.blocks,
                edges: o.edges,
                entry: o.entry,
                exit: o.exit,
                unsupported: None,
            })
        });
        (
            locals,
            &own_params,
            &own_sources,
            own_cfg.as_ref(),
            &own_ctx,
        )
    } else if closure.is_some() {
        // **分层作用域**:外层的表继续可见(那正是「捕获」),
        // 但闭包自己绑的名字盖住外层同名的(类型表已在上面分层建好)。
        // 形参遮蔽:被盖住的**位置**置 None ⇒ 在参数流里永不匹配。
        // 🔴 位置不能删,删了后面的形参就全错位了。
        own_params = caller_params
            .iter()
            .map(|p| match p {
                Some(n) if bound.contains(n.as_str()) => None,
                other => other.clone(),
            })
            .collect();
        own_aliases = layer(caller_ctx.aliases, &bound, |m| {
            crate::lang::collect_aliases(spec, node, src, m)
        });
        own_ctx = crate::lang::FlowCtx {
            narrow: Some(&narrow),
            aliases: &own_aliases,
            effects: Some(&effects),
        };
        own_sources = layer(caller_sources, &bound, |m| {
            collect_local_sources(spec, node, src, Some(&own_ctx), m)
        });
        // 🔴 CFG 必须置 `None`:外层的 CFG **不含**闭包体(闭包是不透明域),
        // 拿它去问「闭包这一行看得见哪几次赋值」会按行号落进外层的块,
        // 答出一个**偏窄**的答案 —— 那是漏,不是宽。退回流不敏感才是安全的那一侧。
        (locals, &own_params, &own_sources, None, &own_ctx)
    } else {
        (
            locals,
            caller_params,
            caller_sources,
            caller_cfg,
            caller_ctx,
        )
    };

    if let Some((name, qual, is_method)) = spec.call_of(node, src) {
        if let Some(from) = current_fn {
            let line = node.start_position().row + 1;
            // 🔴 `self.m()` / `this.m()` / `Self::m()`:接收者就是**当前所在类型**。
            // 这是**语言规则不是类型推断** ⇒ 拿当前类型当限定名精确解析,不算猜。
            // 不做这一步的代价:Rust/Python/TS 里绝大多数同类型内调用都会被算成歧义,
            // 于是 `paths` 在这些语言上几乎恒空(实测 overview→communities 两跳全是 self 调用)。
            // 接收者类型的两条**语法可及**来源,按可靠性排:
            // ① `self.m()` —— 接收者就是当前所在类型(语言规则)
            // ② `g.m()` 且 `g` 在局部类型表里 —— `let g = Graph::build(..)` 那类
            let self_qual = if spec.is_self_call(node, src) {
                qualifier_of_id(from)
            } else {
                spec.receiver_of(node, src).and_then(|recv| {
                    receiver_type(&recv, Some(from), spec.receiver_name(), locals, table)
                })
            };
            let (qual, is_method) = match &self_qual {
                Some(q) => (Some(q.as_str()), false),
                None => (qual.as_deref(), is_method),
            };
            // 参数流:实参里凡是**调用方自己的形参**,记下「第 i 参 → 第 j 参」。
            // 纯语法 —— 不看类型、不看值,只看「这个裸标识符正好是我的形参名」。
            let flow = arg_flow_of(
                spec,
                node,
                src,
                caller_params,
                caller_sources,
                caller_cfg,
                caller_ctx,
            );
            // 🔴 **局部绑定遮蔽模块级名字**(语言规则,不是猜):
            // `let empty = || ..; empty()` 调的是那个闭包,不是仓里碰巧同名的符号。
            // 不拦这一条会产生**假的 Exact 边**。
            // ⚠ 只拦**裸名调用** —— `Type::m()` / `x.m()` 有限定,不受局部名遮蔽。
            let shadowed_by_local =
                qual.is_none() && !is_method && self_qual.is_none() && local_names.contains(&name);
            let resolved = if shadowed_by_local {
                Resolved::None // 目标在函数内部,不是仓内的某个符号 ⇒ 看不见
            } else {
                resolve(&name, qual, is_method, table, self_qual.is_some())
            };
            match resolved {
                // 识别为调用、在函数内,但连不上任何仓内符号 → 未解析(外部/stdlib/漏抓)
                Resolved::None => *unresolved += 1,
                Resolved::One(to) => upsert(
                    edges,
                    index_of,
                    from,
                    &to,
                    line,
                    EdgeKind::Calls,
                    Confidence::Exact,
                    None,
                    flow.clone(),
                ),
                Resolved::Many { picks, confidence } => {
                    // 计**调用点**不计边数 —— 一个分不清的调用点就是一处缺口;
                    // 它有几个候选是另一回事(那个数记在边上)。
                    *ambiguous += 1;
                    let n = picks.len();
                    for to in &picks {
                        upsert(
                            edges,
                            index_of,
                            from,
                            to,
                            line,
                            EdgeKind::AmbiguousCall,
                            confidence,
                            Some(n),
                            flow.clone(),
                        );
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls(
            spec,
            child,
            src,
            file_syms,
            enclosing,
            table,
            edges,
            index_of,
            unresolved,
            ambiguous,
            locals,
            caller_params,
            caller_sources,
            caller_cfg,
            caller_ctx,
            local_names,
        );
    }
}

/// 把定义节点对回它的**可调用**符号 id(按名 + 起始行匹配;只认 Function/Method,
/// 使类/模块容器节点不会被当作调用的 enclosing 作用域——F13/F14 产出容器符号时仍稳)。
pub(super) fn fn_id_of(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    file_syms: &[Symbol],
) -> Option<String> {
    let name = spec.symbol_at(node, src)?.name;
    let line = node.start_position().row + 1;
    file_syms
        .iter()
        .find(|s| {
            s.name == name
                && s.start_line == line
                && matches!(s.kind, SymKind::Function | SymKind::Method)
        })
        .map(|s| s.id.clone())
}

/// 建**分层作用域**的表:外层的先滤掉被闭包形参盖住的,再让闭包自己的绑定覆盖同名项。
///
/// 滤外层看路径的**根**(`s.f` 的根是 `s`)—— 形参 `s` 被盖住,`s.f` 也就不是外层那个了。
fn layer<V: Clone>(
    outer: &HashMap<String, V>,
    bound: &std::collections::HashSet<&str>,
    fill_own: impl FnOnce(&mut HashMap<String, V>),
) -> HashMap<String, V> {
    let mut own = HashMap::new();
    fill_own(&mut own);
    let mut out: HashMap<String, V> = outer
        .iter()
        .filter(|(k, _)| !bound.contains(crate::lang::path_root(k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    out.extend(own); // 闭包自己的绑定**覆盖**(不是并)外层同名的 —— 那才是遮蔽
    out
}

/// 接收者**访问路径** → 类型名。
///
/// 根有两种解法,都**语法可及、非推断**:
/// * `self` / `this` —— 就是当前所在类型(语言规则);
/// * 别的标识符 —— 查局部类型表(形参标注 · 显式标注 · 被调返回类型)。
///
/// 之后逐段顺**字段类型表**走下去:`self.idx` ⇒ `Engine` 的 `idx` 字段 ⇒ `Index`。
/// 任何一段答不出就整条返回 `None` —— 半路猜一个比不答坏得多。
fn receiver_type(
    path: &str,
    enclosing_fn: Option<&str>,
    recv_name: Option<&str>,
    locals: &HashMap<String, String>,
    table: &SymbolTable,
) -> Option<String> {
    let mut segs = path.split('.');
    let root = segs.next()?;
    let own_type = || enclosing_fn.and_then(qualifier_of_id);
    let mut cur = if Some(root) == recv_name {
        own_type()?
    } else {
        // 局部(形参 / let 绑定)优先 —— 它遮蔽同名字段。
        // 查不到再当**隐式 `this`**:Java / C# / Kotlin 里 `idx.all()` 就是 `this.idx.all()`,
        // 而 Kotlin 的 `class E(val idx: Index)` 这一形连字段声明都写在形参表里。
        // ⚠ 只有 (当前类型, 这个名) 真的在字段表里才认 —— 认不出就是认不出,不猜。
        match locals.get(root) {
            Some(t) => t.clone(),
            None => table.field_type_of(&own_type()?, root)?.to_string(),
        }
    };
    // 有界:路径本就 k 限深,这里再兜一道防病态输入
    for field in segs.take(crate::lang::ACCESS_PATH_K) {
        cur = table.field_type_of(&cur, field)?.to_string();
    }
    Some(cur)
}
