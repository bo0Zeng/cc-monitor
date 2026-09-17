//! MCP 侧文本序列化(F06):把查询结果转成 token 预算化的 agent 可读文本。
//! 统一走 `TokenBudget::est_tokens` 估算(账本 TokenBudget 行「core 提供 mcp 序列化」)。
//! 列表类输出一律 `append_within_budget` 裁剪;overview 已在 engine 内按 budget 裁剪。

use crate::model::{DocLink, DriftItem, Edge, ImpactSet, NodeView, Overview, Symbol, TokenBudget};

pub fn overview_text(ov: &Overview) -> String {
    let mut s = format!(
        "项目全景:{} 符号 / {} 文件\n",
        ov.total_symbols, ov.total_files
    );
    // F18 覆盖信号:诚实标注"漏了多少",避免全景看似完整(有缺口才显示)
    if ov.unresolved_calls > 0 || ov.parse_errors > 0 {
        s.push_str(&format!(
            "覆盖缺口:{} 处调用未解析成边(外部/stdlib/漏抓)· {} 文件解析失败未产出符号 —— 静态分析已知缺口,基于上次 index\n",
            ov.unresolved_calls, ov.parse_errors
        ));
    }
    s.push_str(&format!("脊柱文件({}):\n", ov.spine_files.len()));
    for f in &ov.spine_files {
        s.push_str(&format!(
            "  {} ({} 符号, 分 {:.4})\n",
            f.file, f.symbols, f.score
        ));
    }
    s.push_str(&format!("子系统({}):\n", ov.subsystems.len()));
    for sub in &ov.subsystems {
        s.push_str(&format!("  [{}] {} 符号\n", sub.label, sub.size));
    }
    s.push_str(&format!("入口点({}):\n", ov.entry_points.len()));
    for e in &ov.entry_points {
        s.push_str(&format!("  {}\n", e));
    }
    s
}

pub fn edges_text(edges: &[Edge], label: &str, budget: TokenBudget) -> String {
    let mut s = format!("{}({}):\n", label, edges.len());
    append_within_budget(&mut s, edges, budget, |e| {
        format!("  {} → {} [{:?}{}]\n", e.from, e.to, e.confidence, at(e))
    });
    s
}

pub fn impact_text(imp: &ImpactSet, budget: TokenBudget) -> String {
    let mut s = format!("影响面 impact({}):{} 个传递调用者\n", imp.root, imp.total());
    append_within_budget(&mut s, &imp.affected, budget, |a| {
        format!("  d{} {}\n", a.depth, a.id)
    });
    s
}

pub fn docs_text(links: &[DocLink], budget: TokenBudget) -> String {
    if links.is_empty() {
        return "(无关联文档)\n".to_string();
    }
    let mut s = format!("关联文档({}):\n", links.len());
    append_within_budget(&mut s, links, budget, |l| {
        format!(
            "  {} → {}{} [{:?}]\n",
            l.doc_path,
            l.target_file,
            sym_suffix(&l.target_symbol),
            l.source
        )
    });
    s
}

pub fn drift_text(items: &[DriftItem], budget: TokenBudget) -> String {
    if items.is_empty() {
        return "(无漂移)\n".to_string();
    }
    let mut s = format!("漂移链接({}):\n", items.len());
    append_within_budget(&mut s, items, budget, |d| {
        format!(
            "  {} → {}{}:{}\n",
            d.doc_path,
            d.target_file,
            sym_suffix(&d.target_symbol),
            d.reason
        )
    });
    s
}

pub fn search_text(syms: &[Symbol], budget: TokenBudget) -> String {
    if syms.is_empty() {
        return "(无匹配符号)\n".to_string();
    }
    let mut s = format!("匹配符号({}):\n", syms.len());
    append_within_budget(&mut s, syms, budget, |sym| {
        format!(
            "  {} ({:?}) {}:{}\n",
            sym.id, sym.kind, sym.file, sym.start_line
        )
    });
    s
}

/// F15:接受 `Engine::node` 组装好的 `NodeView`(不再由 MCP 层现拼 callers/callees/docs/批注)。
pub fn node_text(view: &NodeView, budget: TokenBudget) -> String {
    let sym = &view.symbol;
    let mut s = format!(
        "符号 {}\n  {:?} {}:{}-{}\n",
        sym.id, sym.kind, sym.file, sym.start_line, sym.end_line
    );
    // 人写批注(Active)—— 可信 ground truth,放在前面;body 单行化 + 按预算裁剪
    if !view.annotations.is_empty() {
        s.push_str(&format!("  批注({}):\n", view.annotations.len()));
        append_within_budget(&mut s, &view.annotations, budget, |a| {
            format!("    · {}(by {})\n", one_line(&a.body), a.author)
        });
    }
    s.push_str(&format!("  被调用(callers):{}\n", view.callers.len()));
    append_within_budget(&mut s, &view.callers, budget, |e| {
        format!("    ← {} [{:?}{}]\n", e.from, e.confidence, at(e))
    });
    s.push_str(&format!("  调用(callees):{}\n", view.callees.len()));
    append_within_budget(&mut s, &view.callees, budget, |e| {
        format!("    → {} [{:?}{}]\n", e.to, e.confidence, at(e))
    });
    if !view.docs.is_empty() {
        s.push_str(&format!("  关联文档:{}\n", view.docs.len()));
        append_within_budget(&mut s, &view.docs, budget, |d| {
            format!("    {}\n", d.doc_path)
        });
    }
    s
}

/// 把多行文本压成单行(批注 body 可能多行,避免破坏缩进)。
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// 逐行累加,累计估算 token 超预算即停并标记(header 已计入 s)。
fn append_within_budget<T>(
    s: &mut String,
    items: &[T],
    budget: TokenBudget,
    line_of: impl Fn(&T) -> String,
) {
    let mut used = TokenBudget::est_tokens(s);
    for it in items {
        let line = line_of(it);
        used += TokenBudget::est_tokens(&line);
        if used > budget.0 {
            s.push_str("  …(已按预算截断)\n");
            break;
        }
        s.push_str(&line);
    }
}

fn sym_suffix(opt: &Option<String>) -> String {
    opt.as_deref()
        .map(|x| format!("#{}", x))
        .unwrap_or_default()
}

fn at(e: &Edge) -> String {
    e.call_site_line
        .map(|l| format!(" @{}", l))
        .unwrap_or_default()
}
