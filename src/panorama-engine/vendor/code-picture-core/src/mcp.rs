//! MCP 侧文本序列化(F06):把查询结果转成 token 预算化的 agent 可读文本。
//! 统一走 `TokenBudget::est_tokens` 估算(账本 TokenBudget 行「core 提供 mcp 序列化」)。
//! 列表类输出一律 `append_within_budget` 裁剪;overview 已在 engine 内按 budget 裁剪。

use crate::model::{
    Cfg, DocLink, DriftItem, Edge, FlowSet, ImpactSet, NodeView, Overview, PathSet, Symbol,
    TokenBudget,
};

pub fn overview_text(ov: &Overview) -> String {
    let mut s = format!(
        "项目全景:{} 符号 / {} 文件\n",
        ov.total_symbols, ov.total_files
    );
    // F18 覆盖信号:诚实标注"漏了多少",避免全景看似完整(有缺口才显示)
    if ov.unresolved_calls > 0
        || ov.ambiguous_calls > 0
        || ov.unresolved_imports > 0
        || ov.parse_errors > 0
    {
        s.push_str(&format!(
            "覆盖缺口(基于上次 index):{} 处调用**看不见**(外部/stdlib/漏抓)· {} 处调用**看得见但分不清**(方法调用或同名多候选 → 归 ambiguous,不进 callers/impact)· {} 条 import 未解析成边 · {} 文件解析失败未产出符号\n",
            ov.unresolved_calls, ov.ambiguous_calls, ov.unresolved_imports, ov.parse_errors
        ));
    }
    // 🔴 读库出错 ⇒ 这份全景**不完整**。放在覆盖缺口之前 —— 它比缺口严重:
    // 缺口是「我们知道自己看不见什么」,这个是「答案可能整块是空的而我们差点没发现」。
    if !ov.db_errors.is_empty() {
        s.push_str(&format!(
            "🔴 **本次读索引出错 {} 处,下面的全景不完整**(不是「这仓就这样」):\n",
            ov.db_errors.len()
        ));
        for e in &ov.db_errors {
            s.push_str(&format!("   · {e}\n"));
        }
        s.push_str("   多半是索引 schema 与当前版本不符 —— 跑一次 reindex。\n");
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
        // 指纹要印出来:MCP 消费方只拿得到文本,藏在结构里等于对账的人用不上。
        // 「内/外」是**原始计数不是分数** —— 内 3 外 30 是混了还是它本来就是公共设施,
        // 得看它是什么,不是看比值(见 rank::Graph::community_cut)。
        s.push_str(&format!(
            "  [{}] {} 符号 · 边 内{}/外{} · 指纹 {}",
            sub.label, sub.size, sub.internal_edges, sub.external_edges, sub.member_hash
        ));
        if !sub.anchors.is_empty() {
            s.push_str(&format!(" · 代表 {}", sub.anchors.join(", ")));
        }
        s.push('\n');
    }
    s.push_str(&format!("入口点({}):\n", ov.entry_points.len()));
    for e in &ov.entry_points {
        s.push_str(&format!("  {}\n", e.id));
    }
    s
}

/// 控制流图的文本形。
///
/// 🔴 **作废时只印那一句,不印图** —— 印了就会有人拿去用。
pub fn cfg_text(c: &Cfg, budget: TokenBudget) -> String {
    if let Some(why) = &c.unsupported {
        return format!(
            "控制流图 {}:**不可用**\n  原因:函数里有本实现不建模的构造 —— {why}\n             \x20 这是刻意的:一张漏边的图会让「这两个调用互斥」变成假事实,\n             \x20 比没有答案坏得多 ⇒ 整份作废,不给半张。\n",
            c.function
        );
    }
    let mut s = format!(
        "控制流图 {}:{} 块 / {} 边(入口 b{} · 出口 b{})\n",
        c.function,
        c.blocks.len(),
        c.edges.len(),
        c.entry,
        c.exit
    );
    s.push_str(
        "  · 基本块 = 一旦进入就从头跑到尾的一段\n         \x20 · 它能答**行号答不了**的问题:两个调用会不会都发生(分属互斥分支就不会)\n         \x20 ⚠ 闭包体不在本图里 —— 它何时执行是另一回事\n",
    );
    append_within_budget(&mut s, &c.blocks, budget, |b| {
        let succ: Vec<String> = c
            .edges
            .iter()
            .filter(|(f, _)| *f == b.id)
            .map(|(_, t)| format!("b{t}"))
            .collect();
        let calls: Vec<String> = b.calls.iter().map(|(n, l)| format!("{n}@{l}")).collect();
        format!(
            "  b{} 行{}–{} → [{}]{}\n",
            b.id,
            b.start_line,
            b.end_line,
            succ.join(" "),
            if calls.is_empty() {
                String::new()
            } else {
                format!("  调用: {}", calls.join(" "))
            }
        )
    });
    s
}

/// 参数流的文本形。**必须把「只追原样传递」这条边界印出来** ——
/// 否则读者会把它当成完整的数据流,而它只覆盖其中很窄的一种。
pub fn flow_text(fs: &FlowSet, budget: TokenBudget) -> String {
    let maybe = fs.steps.iter().filter(|s| s.derived).count();
    let mut s = format!(
        "参数流 {}(第 {} 个形参)→ 流到 {} 处(确定 {} · 可能 {})\n",
        fs.root,
        fs.param,
        fs.steps.len(),
        fs.steps.len() - maybe,
        maybe
    );
    s.push_str(
        "  · **确定** = 原样传下去(实参就是那个形参,或它的借用)\n         \x20 · **可能** = 值被变换过或经中间变量,由**流不敏感**的过程内分析得出\n         \x20   ⚠ 流不敏感 = 过度近似:重新赋值不区分先后\n         \x20     (`y=a; g(y); y=b; g(y)` 会算成 a 和 b 都流到两处)\n         \x20   ⚠ 链上有一跳是「可能」,后面全段就只能当「可能」读\n         \x20 · 仍**追不到**:存进结构体再取出 · 经闭包捕获 · 经返回值\n         \x20 · 只走唯一解析的调用;同一对调用方-被调的参数流是多个调用点的并集\n",
    );
    if fs.truncated {
        s.push_str("  ⚠ **触到深度上限**:是「我没再往下找」,不是「到头了」。\n");
    }
    append_within_budget(&mut s, &fs.steps, budget, |st| {
        format!(
            "  d{} {} 第{}参 [{}]\n",
            st.depth,
            st.id,
            st.param,
            if st.derived { "可能" } else { "确定" }
        )
    });
    s
}

/// 调用路径的文本形。**每一条都要带上「这不是执行轨迹」那句** ——
/// 一串带箭头的函数名太像 trace 了,不说清楚读者一定会当运行顺序读。
pub fn paths_text(ps: &PathSet, budget: TokenBudget) -> String {
    let mut s = format!("调用路径 {} → {}:{} 条\n", ps.from, ps.to, ps.paths.len());
    s.push_str(
        "  ⚠ 这是**调用关系上存在的链**,不是执行轨迹 —— 图里没有分支/循环/提前返回,\n         \x20   同一函数体内的先后也不代表运行顺序。只走唯一解析的调用(Calls/Exact)。\n",
    );
    if ps.truncated {
        s.push_str("  ⚠ **已截断**(触到条数或深度上限):还可能有别的路,不是「一共就这些」。\n");
    }
    if ps.cycles_cut > 0 {
        s.push_str(&format!(
            "  · 切断了 {} 处环(递归 / 互递归),未展开\n",
            ps.cycles_cut
        ));
    }
    append_within_budget(&mut s, &ps.paths, budget, |p| {
        let mut line = format!("  [{} 跳] ", p.hops());
        if let Some(first) = p.steps.first() {
            line.push_str(&first.from);
        }
        for st in &p.steps {
            line.push_str(&format!(" →{} {}", at(st), st.to));
        }
        line.push('\n');
        line
    });
    s
}

pub fn edges_text(edges: &[Edge], label: &str, budget: TokenBudget) -> String {
    let mut s = format!("{}({}):\n", label, edges.len());
    append_within_budget(&mut s, edges, budget, |e| {
        format!(
            "  {} → {} [{:?}{}{}]\n",
            e.from,
            e.to,
            e.confidence,
            at(e),
            cands(e)
        )
    });
    s
}

pub fn impact_text(imp: &ImpactSet, budget: TokenBudget) -> String {
    let mut s = format!("影响面 impact({}):{} 个传递调用者\n", imp.root, imp.total());
    // 🔴 这一行是本输出的**唯一诚实信道**:ImpactSet 本身不带可信度字段,
    // 不写这句,读者无从知道它只覆盖「唯一解析」的那一部分调用。
    s.push_str(
        "  ⚠ 只含**唯一解析**的调用(Calls/Exact)。方法调用与同名多候选属歧义,\n         \x20   不进这张表 —— 查它们用 ambiguous_callers;要完整调用关系用 lsp_callers。\n",
    );
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
            // 来源写在脸上:批准过的 agent 提议**不是人的指示**,别把自己的话读成人说的
            let from = match a.origin {
                crate::model::AnnotationOrigin::Human => "人写",
                crate::model::AnnotationOrigin::Agent => "agent 提议·人已批准",
                crate::model::AnnotationOrigin::Unrecorded => "来源未记录",
            };
            format!("    · {}(by {} · {from})\n", one_line(&a.body), a.author)
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

/// 歧义边把「N 选 1」印在脸上 —— 不印,这条边在读者眼里就又变回一句调用事实。
fn cands(e: &Edge) -> String {
    match e.candidates {
        Some(n) if n > 1 => format!(" {n}选1·未定"),
        Some(_) => " 接收者类型未知".to_string(),
        None => String::new(),
    }
}

fn at(e: &Edge) -> String {
    e.call_site_line
        .map(|l| format!(" @{}", l))
        .unwrap_or_default()
}
