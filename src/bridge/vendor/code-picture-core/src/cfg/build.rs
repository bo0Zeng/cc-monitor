//! **通用**控制流图构造器:算法在这里,节点名在语言层。
//!
//! 先前 Rust 的 CFG 把「怎么建图」和「哪个 kind 是 if」焊在一个文件里 ——
//! 扇出到第二门语言时就得整份抄一遍。拆开之后每门语言只答一个问题:
//! **这个节点在控制流上是什么角色**(`CtrlRole`),建图算法一份就够。
//!
//! 🔴 **过度近似的方向是安全的,少边才危险。**
//! 多连一条边 ⇒ 可达性变宽 ⇒ 判「互斥」的更少、到达定义集合更大 ——
//! 两个消费方都只会**少下结论**。漏边则相反:会让「这两个调用互斥」变成假事实。
//! ⇒ 拿不准的构造一律**整份作废**;拿得准但复杂的(如 try/except)就**多连边**。

use crate::lang::{CfgOutline, CtrlRole, JumpKind, LangSupport};
use crate::model::{CfgBlock, CfgDef};
use tree_sitter::Node;

pub(crate) struct Builder<'a> {
    spec: &'a dyn LangSupport,
    src: &'a [u8],
    blocks: Vec<CfgBlock>,
    edges: Vec<(usize, usize)>,
    /// 循环栈:(continue 去哪, break 去哪)
    loops: Vec<(usize, usize)>,
    unsupported: Option<String>,
    ctx: Option<&'a crate::lang::FlowCtx<'a>>,
}

impl<'a> Builder<'a> {
    pub(crate) fn new(
        spec: &'a dyn LangSupport,
        src: &'a [u8],
        ctx: Option<&'a crate::lang::FlowCtx<'a>>,
    ) -> Builder<'a> {
        Builder {
            spec,
            src,
            blocks: Vec::new(),
            edges: Vec::new(),
            loops: Vec::new(),
            unsupported: None,
            ctx,
        }
    }

    fn new_block(&mut self, line: usize) -> usize {
        let id = self.blocks.len();
        self.blocks.push(CfgBlock {
            id,
            start_line: line,
            end_line: line,
            calls: Vec::new(),
            defs: Vec::new(),
        });
        id
    }

    fn edge(&mut self, from: usize, to: usize) {
        if !self.edges.contains(&(from, to)) {
            self.edges.push((from, to));
        }
    }

    fn bail(&mut self, what: &str) {
        if self.unsupported.is_none() {
            self.unsupported = Some(what.to_string());
        }
    }

    /// 记一个节点里的调用点与局部定义。**不下钻闭包/嵌套函数** ——
    /// 它们何时执行是另一回事,放进来就是假的顺序。
    fn record(&mut self, n: Node, blk: usize) {
        if self.spec.is_opaque_scope(n) {
            return;
        }
        if let Some((name, _, _)) = self.spec.call_of(n, self.src) {
            self.blocks[blk]
                .calls
                .push((name, n.start_position().row + 1));
        }
        if let Some((var, _, Some(init))) = self.spec.local_binding(n, self.src) {
            let mut sources = self
                .ctx
                .and_then(|c| c.narrow)
                .and_then(|f| f(init))
                .unwrap_or_else(|| crate::lang::access_paths_in(init, self.src));
            // 路径归一(经别名的写要认得出) —— 与 `collect_local_sources` 同一套口径
            let var = self.ctx.map(|c| c.canon(&var)).unwrap_or(var);
            if let Some(c) = self.ctx {
                for s in sources.iter_mut() {
                    *s = c.canon(s);
                }
            }
            sources.retain(|i| i != &var); // 自指不记,否则传递闭包算不完
            self.blocks[blk].defs.push(CfgDef {
                var,
                line: n.start_position().row + 1,
                at: n.start_byte(),
                sources,
            });
        }
        // 过程间:被调的**函数摘要**说它写了哪个实参的哪条字段 ⇒ 在**这一行**记个定义。
        // 🔴 偏移取 `end_byte`:写是在调用**完成**时发生的,不是在调用开始时 ——
        //   用 start 的话同一行里 `store(&mut s, x); sink(s.f)` 的先后就排错了。
        if let Some(c) = self.ctx {
            let line = n.start_position().row + 1;
            let at = n.end_byte();
            for (target, srcs) in c.call_effects(n) {
                let var = c.canon(&target);
                let mut sources: Vec<String> = srcs.iter().map(|s| c.canon(s)).collect();
                sources.retain(|s| s != &var);
                if !sources.is_empty() {
                    self.blocks[blk].defs.push(CfgDef {
                        var,
                        line,
                        at,
                        sources,
                    });
                }
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            self.record(ch, blk);
        }
    }

    /// 子树里有没有藏着非 `Plain` 的控制节点(不下钻不透明作用域)。
    /// 返回它的名字 —— 作废信息要说得出是什么,不然没人知道该怎么办。
    fn hidden_ctrl(&self, n: Node) -> Option<&'static str> {
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if self.spec.is_opaque_scope(ch) {
                continue;
            }
            let name = match self.spec.ctrl_role(ch, self.src) {
                CtrlRole::Plain => None,
                CtrlRole::Sequence { .. } => Some("嵌套语句容器"),
                CtrlRole::Branch { .. } => Some("嵌套分支"),
                CtrlRole::Loop { .. } => Some("嵌套循环"),
                CtrlRole::Guarded { .. } => Some("嵌套受保护块"),
                CtrlRole::Return => Some("嵌套 return"),
                CtrlRole::Jump { .. } => Some("嵌套 break/continue"),
                CtrlRole::Unsupported(w) => Some(w),
            };
            if name.is_some() {
                return name;
            }
            if let Some(w) = self.hidden_ctrl(ch) {
                return Some(w);
            }
        }
        None
    }

    fn touch(&mut self, blk: usize, line: usize) {
        let b = &mut self.blocks[blk];
        if b.calls.is_empty() && b.defs.is_empty() && b.start_line > line {
            b.start_line = line;
        }
        if line > b.end_line {
            b.end_line = line;
        }
    }

    /// 走一个「语句容器」里的语句串,或单条语句。
    /// 返回「继续往下」的块;`None` = 这条路已终止。
    fn body(&mut self, n: Node, cur: usize, exit: usize) -> Option<usize> {
        if self.spec.is_block(n) {
            let mut c = n.walk();
            let kids: Vec<Node> = n.children(&mut c).filter(|k| k.is_named()).collect();
            let mut at = cur;
            for k in kids {
                at = self.stmt(k, at, exit)?;
            }
            Some(at)
        } else {
            self.stmt(n, cur, exit)
        }
    }

    fn stmt(&mut self, n: Node, cur: usize, exit: usize) -> Option<usize> {
        let line = n.start_position().row + 1;
        self.touch(cur, line);
        match self.spec.ctrl_role(n, self.src) {
            CtrlRole::Plain => {
                // 🔴 核 `Plain` 那条承诺:子树里不许藏着控制流。
                // 不核的话,`with x:` / 块表达式里的 if 会被静默吞掉 —— 漏边即假事实。
                if let Some(what) = self.hidden_ctrl(n) {
                    self.bail(&format!("{what}(藏在被当作顺序语句的节点里)"));
                    return None;
                }
                self.record(n, cur);
                if self.spec.has_implicit_exit(n) {
                    self.edge(cur, exit); // 隐式早退(Rust 的 `?`)
                }
                Some(cur)
            }
            CtrlRole::Sequence { body } => self.body(body, cur, exit),
            CtrlRole::Branch {
                cond,
                arms,
                exhaustive,
            } => {
                if let Some(c) = cond {
                    self.record(c, cur);
                }
                let merge = self.new_block(line);
                let mut any = false;
                for arm in arms {
                    let b = self.new_block(arm.start_position().row + 1);
                    self.edge(cur, b);
                    if let Some(e) = self.body(arm, b, exit) {
                        self.edge(e, merge);
                        any = true;
                    }
                }
                if !exhaustive {
                    self.edge(cur, merge); // 条件全不中
                    any = true;
                }
                if any {
                    Some(merge)
                } else {
                    None
                }
            }
            CtrlRole::Loop { cond, body } => {
                let head = self.new_block(line);
                let after = self.new_block(line);
                self.edge(cur, head);
                self.edge(head, after); // 条件为假 / 迭代结束
                self.record(cond.unwrap_or(n), head);
                let b = self.new_block(body.start_position().row + 1);
                self.edge(head, b);
                self.loops.push((head, after));
                let end = self.body(body, b, exit);
                self.loops.pop();
                if let Some(e) = end {
                    self.edge(e, head); // 回边
                }
                Some(after)
            }
            CtrlRole::Guarded { body, handlers } => {
                // 异常可能在 body 的**任意一点**抛出 ⇒ body 期间开的每个块都连到处理块。
                // 刻意的过度近似 —— 少连才危险(见本模块头注释)。
                let b = self.new_block(body.start_position().row + 1);
                self.edge(cur, b);
                let first = self.blocks.len() - 1;
                let end = self.body(body, b, exit);
                let last = self.blocks.len() - 1;
                let merge = self.new_block(line);
                if let Some(e) = end {
                    self.edge(e, merge);
                }
                for h in handlers {
                    let hb = self.new_block(h.start_position().row + 1);
                    for bb in first..=last {
                        self.edge(bb, hb);
                    }
                    if let Some(he) = self.body(h, hb, exit) {
                        self.edge(he, merge);
                    }
                }
                Some(merge)
            }
            CtrlRole::Return => {
                self.record(n, cur);
                self.edge(cur, exit);
                None
            }
            CtrlRole::Jump { kind, labeled } => {
                if labeled {
                    self.bail("带标签的 break/continue");
                }
                let Some(&(cont, brk)) = self.loops.last() else {
                    self.bail("循环外的 break/continue");
                    return None;
                };
                let target = match kind {
                    JumpKind::Break => brk,
                    JumpKind::Continue => cont,
                };
                self.edge(cur, target);
                None
            }
            CtrlRole::Unsupported(what) => {
                self.bail(what);
                None
            }
        }
    }

    /// 建图收尾:入口/出口块 + 把最后一条路连到出口。
    pub(crate) fn finish(mut self, body: Node, fn_end_line: usize) -> CfgOutline {
        let entry = self.new_block(body.start_position().row + 1);
        let exit = self.new_block(fn_end_line);
        if let Some(end) = self.body(body, entry, exit) {
            self.edge(end, exit);
        }
        for blk in self.blocks.iter_mut() {
            blk.defs.sort_by_key(|d| d.at); // 按字节偏移定序(行号排不了同一行内的先后)
        }
        CfgOutline {
            blocks: self.blocks,
            edges: self.edges,
            entry,
            exit,
            unsupported: self.unsupported,
        }
    }
}
