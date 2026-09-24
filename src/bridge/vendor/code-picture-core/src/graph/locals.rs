//! 函数体内的**语法可及**信息:局部变量类型 · 局部变量来源 · 参数流。
//!
//! 三件都不需要类型推导引擎:
//! * **类型** —— 显式标注,或初始化调用的返回类型(写在被调自己签名里);
//! * **来源** —— 初始化表达式提到了哪些标识符(**流不敏感**);
//! * **参数流** —— 实参第 j 位是不是调用方第 i 个形参(确定),或顺来源表追得回去(可能)。
//!
//! 🔴 「可能」那一档是**过度近似**(流不敏感 ⇒ 重新赋值不区分先后)。
//! 要收紧成流敏感需要控制流图 —— 见 `crate::cfg`。

use super::resolve::{resolve, Resolved};
use super::table::SymbolTable;
use crate::lang::{param_origins, LangSupport};
use crate::model::Slot;
use std::collections::HashMap;
use tree_sitter::Node;

/// 解析调用 → (目标 id, 置信度) 列表。语言无关(输入已由 `LangSupport::call_of` 抽好)。
/// 算一个调用点的参数流串。实参第 j 位若是个裸标识符、且它正好是调用方的第 i 个形参
/// ⇒ 记 `i>j`。**纯语法**:不看类型、不看值,只看名字对不对得上。
///
/// ⚠ 形参重名(遮蔽)时取**第一个**匹配位 —— 遮蔽在正常代码里罕见,
/// 而这里取错的后果是记下一条假的传递,所以宁可只认第一个、不做更聪明的猜。
pub(super) fn arg_flow_of(
    spec: &dyn LangSupport,
    call: Node,
    src: &[u8],
    caller_params: &[Option<String>],
    sources: &HashMap<String, Vec<String>>,
    cfg: Option<&crate::model::Cfg>,
    ctx: &crate::lang::FlowCtx,
) -> Option<String> {
    // 槽位表 = `[接收者] ++ 形参`。接收者算一个槽位,`self.m(v)` / `dst.m(v)` 才记得下来 ——
    // 函数摘要的传播要靠它(见 `crate::model::Slot`)。
    let mut slots: Vec<Option<String>> = vec![spec.receiver_name().map(String::from)];
    slots.extend(caller_params.iter().cloned());
    if slots.iter().all(Option::is_none) {
        return None;
    }
    let slot_of = |name: &str| slots.iter().position(|p| p.as_deref() == Some(name));
    // 被调侧的槽位:实参按位置,接收者单列
    let recv = spec.receiver_of(call, src);
    let args = spec.call_args(call, src);

    // `>` = **确定**原样传(实参就是那个槽位,或它的借用)
    let mut direct: Vec<(Slot, Slot)> = Vec::new();
    for (j, arg) in args.iter().enumerate() {
        let Some(a) = arg else { continue };
        if let Some(i) = slot_of(a) {
            direct.push((Slot::from_index(i), Slot::Param(j)));
        }
    }
    if let Some(r) = &recv {
        if let Some(i) = slot_of(r) {
            direct.push((Slot::from_index(i), Slot::Recv));
        }
    }

    // `~` = **可能**流到(表达式提到的标识符,顺来源表能追回某个槽位)
    let line = call.start_position().row + 1;
    let at = call.start_byte();
    let origins_of = |id: &str| -> Vec<usize> {
        // 查之前先归一:`g(t.f)` 在 `t → s` 时查的是 `s.f`
        let id = ctx.canon(id);
        // CFG 可用 ⇒ 走**到达定义**(流敏感,假阳少);
        // 不可用(语言未实现 / 图作废)⇒ 退回流不敏感那张表。
        // 🔴 退回的是**更宽**的那一档,只会多记「可能」,不会漏 —— 方向安全。
        match cfg.and_then(|c| crate::pdg::param_origins_at(c, &id, at, line, &slots)) {
            Some(v) => v,
            None => {
                let mut v = Vec::new();
                param_origins(&id, sources, &slots, &mut v);
                v
            }
        }
    };
    let mut maybe: Vec<(Slot, Slot)> = Vec::new();
    let add_maybe = |ids: &[String], to: Slot, maybe: &mut Vec<(Slot, Slot)>| {
        for id in ids {
            for i in origins_of(id) {
                let pair = (Slot::from_index(i), to);
                if !direct.contains(&pair) {
                    maybe.push(pair);
                }
            }
        }
    };
    for (j, ids) in spec.call_arg_mentions(call, src).iter().enumerate() {
        add_maybe(ids, Slot::Param(j), &mut maybe);
    }
    if let Some(r) = &recv {
        add_maybe(std::slice::from_ref(r), Slot::Recv, &mut maybe);
    }

    direct.sort_unstable();
    direct.dedup();
    maybe.sort_unstable();
    maybe.dedup();
    if direct.is_empty() && maybe.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = direct
        .iter()
        .map(|(i, j)| format!("{}>{}", i.token(), j.token()))
        .collect();
    parts.extend(
        maybe
            .iter()
            .map(|(i, j)| format!("{}~{}", i.token(), j.token())),
    );
    Some(parts.join(","))
}

/// 扫一个函数体,建**局部变量 → 类型名**表。
///
/// 两条来源,都**语法可及、不需要类型推导引擎**:
/// 1. **显式标注** —— `let g: Graph = ..`,直接读;
/// 2. **初始化调用的返回类型** —— `let g = Graph::build(..)`,而 `build` 的
///    返回类型写在它自己的签名里(已存进 `Symbol.return_type`)。
///
/// ⚠ 单趟、按书写顺序累积:后面的绑定看得见前面的,反过来不行。
/// 直线代码够用;要处理「先用后定义」得做真的定义-使用链,那是第三档。
/// ⚠ 只认最简单的绑定形(单标识符),解构一律跳过 —— 认了就是猜。
pub(super) fn collect_local_types(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    table: &SymbolTable,
    out: &mut HashMap<String, String>,
) {
    // **形参的类型标注**:`fn take(s: &mut S)` 里 `S` 就写在签名里 —— 读它不是推断。
    // 🔴 不做这一步,`s.set(x)` 的接收者类型未知 ⇒ 一律算歧义,
    //    于是函数摘要在真实代码上全落不了地(实测端到端一条都不响)。
    // 只在函数定义节点上做一次;递归到子节点时 `params_container` 返回 None,不会重复。
    for (name, ty) in spec
        .param_names(node, src)
        .into_iter()
        .zip(spec.param_types(node, src))
    {
        if let (Some(n), Some(t)) = (name, ty) {
            out.insert(n, t);
        }
    }
    if let Some((name, explicit, init)) = spec.local_binding(node, src) {
        let ty = explicit.or_else(|| {
            let init = init?;
            let (cname, cqual, cmethod) = spec.call_of(init, src)?;
            match resolve(&cname, cqual.as_deref(), cmethod, table, false) {
                // 只在**唯一解析**时才认返回类型 —— 歧义的被调,返回类型也是猜的
                Resolved::One(id) => table.return_type_of(&id).map(String::from),
                _ => None,
            }
        });
        if let Some(t) = ty {
            out.insert(name, t);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // 🔴 闭包/嵌套函数是**另一个名字作用域** —— 它的绑定不该污染这里。
        // 不拦的话 `let f = |a| { let y = a; }` 会给外层也记一个 `y ← a`,
        // 而外层的 `y` 明明是别的东西。进闭包时另建分层作用域(见 `graph::calls`)。
        if spec.is_opaque_scope(child) {
            continue;
        }
        collect_local_types(spec, child, src, table, out);
    }
}
