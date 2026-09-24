//! 各门语言 `LangSupport` 实现**共用的助手** —— 纯函数,不认识任何具体语言。
//!
//! 放这儿的判据:**它被两门以上语言用到,或它的规则值得只写一遍**。
//! 与 `super`(trait 契约 + 注册表)分家,是为了让「接口」与「实现细节」不互相挡视线。
//!
//! ⚠ 这些函数**都是语法层的**:数名字、剥包裹、拼路径。
//! 任何需要类型推导的东西都不该出现在这里 —— 那是编译器的活,见 `crate::graph::resolve` 的注释。

use super::LangSupport;

/// 「这个初始化表达式里,真能流到它的值的标识符是哪些」的问法。
/// `None`(不给缩窄器)或返回 `None`(问不出来)⇒ 退回「提到的都算」——
/// **宽的那一档,只会多记「可能」,不会漏**。
pub type Narrow<'a> = Option<&'a dyn Fn(Node) -> Option<Vec<String>>>;

/// 「这个调用点**调用之后**产生了哪些定义」的问法 —— 靠被调的**函数摘要**答。
/// `store(&mut s, x)` 且 store 的摘要是 `1~0:f` ⇒ 答 `[("s.f", ["x"])]`。
///
/// 这是**过程间**那一跳:值经「写进参数」离开函数,不靠摘要就在这里断了。
/// `None` / 空 ⇒ 什么都不记(= 现状,追不到 —— 那是**漏**,不是宽,所以九门都该接)。
pub type Effects<'a> = Option<&'a dyn Fn(Node) -> Vec<(String, Vec<String>)>>;
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

/// 取节点某命名字段的文本(各语言 impl 共用)。字段缺失 / 非 UTF-8 → None。
pub fn field_text(node: Node, field: &str, src: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(src).ok())
        .map(String::from)
}

// ── import 说明符解析:各语言 impl 共用的路径助手 ──

/// 取仓库相对路径的目录部分(不含末尾 `/`);顶层文件 → `""`。
pub fn dir_of(file: &str) -> &str {
    match file.rfind('/') {
        Some(i) => &file[..i],
        None => "",
    }
}

/// 把 `base`(目录,可空)与相对路径 `rel` 拼起来并归一 `.` / `..`。
/// 越过仓库根(`..` 太多)→ None。结果不含前导 `./`。
pub fn join_normalized(base: &str, rel: &str) -> Option<String> {
    // 空段必须滤掉:`base` 带尾斜杠(如 crate 根 `src/`)会切出一个空段,
    // 拼出 `src//model` 这种永远匹配不上的路径。
    let mut segs: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segs.pop()?;
            }
            s => segs.push(s),
        }
    }
    Some(segs.join("/"))
}

/// 给一个无扩展名的基路径扇出候选:`base.<ext>` 与 `base/<index>.<ext>`。
/// `indexes` 为空表示本门语言没有「目录默认文件」这一说。
pub fn ext_candidates(base: &str, exts: &[&str], indexes: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for e in exts {
        out.push(format!("{base}.{e}"));
    }
    for i in indexes {
        for e in exts {
            out.push(format!("{base}/{i}.{e}"));
        }
    }
    out
}

/// 点分名(`com.x.Foo`)→ **路径后缀**候选,逐段缩短(`com/x/Foo.java` → `com/x.java`)。
/// 给「包名映射目录」那类语言(Java · Kotlin)用:源根(`src/main/java/`)不在包名里,
/// 所以给的只能是后缀,由调用方按后缀匹配。逐段缩短是因为末段可能是类、也可能是
/// 静态成员(`import static com.x.Bar.baz`),静态分不开。
pub fn dotted_candidates(specifier: &str, ext: &str) -> Vec<String> {
    let segs: Vec<&str> = specifier.split('.').filter(|s| !s.is_empty()).collect();
    (1..=segs.len())
        .rev()
        .map(|take| format!("{}.{ext}", segs[..take].join("/")))
        .collect()
}

/// 从类型文本里取**最外层类型名**。
/// `Graph` → `Graph` · `: Graph` → `Graph` · `-> Result<Graph, E>` → `Result`
/// · `&mut Foo` → `Foo` · `Vec<T>` → `Vec`。
///
/// ⚠ 取最外层是**刻意的保守**:`Result<Graph, E>` 的「真正有用的类型」是 `Graph`,
/// 但要判断得懂每门语言的包装约定 —— 那是推断。取最外层只会**少解出来**,不会解错。
pub fn outermost_type_name(text: &str) -> String {
    let mut t = text.trim().trim_start_matches(':').trim();
    // 逐层剥**修饰** —— 它们都不是类型名。顺序不定(`&'a dyn Tr` / `&mut dyn Tr`),所以循环。
    // 🔴 不剥 `dyn` / `impl` / 生命周期的代价是**静默的垃圾类型名**:
    //    `&dyn LangSupport` 会得到 `"dyn"`、`&'a dyn LangSupport` 得到 `"'a"`,
    //    拿去当接收者类型解析,仓内的真实调用会被判成「在仓外」而整条消失。
    loop {
        let before = t;
        t = t.trim_start_matches('&').trim();
        for kw in ["mut ", "dyn ", "impl ", "const ", "static "] {
            t = t.strip_prefix(kw).unwrap_or(t).trim();
        }
        // 生命周期 `'a`(后面还有东西才剥 —— `'a` 单独出现时不是修饰)
        if let Some(rest) = t.strip_prefix('\'') {
            if let Some((_, tail)) = rest.split_once(' ') {
                t = tail.trim();
            }
        }
        if t == before {
            break;
        }
    }
    let head = t.split(['<', '(', '[', ' ']).next().unwrap_or(t);
    // 带路径的取末段:`crate::rank::Graph` → `Graph`
    head.rsplit("::").next().unwrap_or(head).trim().to_string()
}

/// 从「形参容器」节点取形参名(按位置)。
/// 每个命名子节点:本身是 `identifier` 就取它,否则取它的**第一个命名子**(名字总在最前)。
/// `skip_first` 为真时跳过第 0 个 —— 用于去掉接收者形参。
pub fn param_names_from(
    container: Option<Node>,
    src: &[u8],
    skip_first: bool,
) -> Vec<Option<String>> {
    let Some(c) = container else {
        return Vec::new();
    };
    // 单参简写没有容器:JS 的 `x => ..` 里 `parameter` 字段**直接就是**那个标识符。
    // 不认这一形的话箭头函数最常见的写法整条取不到形参 —— 于是遮蔽也做不成。
    if c.kind() == "identifier" {
        return vec![c.utf8_text(src).ok().map(String::from)];
    }
    let mut out = Vec::new();
    for i in 0..c.named_child_count() {
        if skip_first && i == 0 {
            continue;
        }
        let p = c.named_child(i).expect("named_child_count 内");
        // 形参名优先走 `pattern` 字段:Rust 的 `mut v: u32` 里第一个命名子是
        // `mutable_specifier` 而不是名字,只看 named_child(0) 会整条漏掉
        // (而 `mut x` 在 Rust 里很常见)。`pattern` 字段直接给到 `identifier "v"`。
        // 解构(`(a,b)` → `tuple_pattern`)拿到的不是 identifier,下面的 filter 会正确跳过。
        // 名字字段按可靠性排,逐个试:
        //   `pattern` —— Rust `mut v: u32`(第一个命名子是 `mutable_specifier`,不是名字)
        //   `name`    —— Java `(int a) -> ..` · C# `(int a) => ..`(第一个命名子是**类型**)
        //   `declarator` —— C/C++ `(int a)`(同上)
        // 都没有才退回第一个命名子(名字总在最前的那些形)。
        // ⚠ 取不到 identifier 一律留 `None` —— 解构 / `int *p` 这类**不猜**,
        //   而 `None` 位在参数流里永不匹配,是安全的那一侧。
        let name_node = if p.kind() == "identifier" {
            Some(p)
        } else {
            p.child_by_field_name("pattern")
                .or_else(|| p.child_by_field_name("name"))
                .or_else(|| p.child_by_field_name("declarator"))
                .or_else(|| p.named_child(0))
        };
        out.push(
            name_node
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(src).ok())
                .map(String::from),
        );
    }
    out
}

/// 类型节点 → 类型名文本。
///
/// 类型节点**自己带 `name` 字段**时优先用它:C 的 `struct Index *idx` 里类型节点是
/// `struct_specifier`,整段文本是 `struct Index`,按「最外层词」取会得到 `struct`。
fn type_name_node<'a>(ty: Node<'a>) -> Node<'a> {
    ty.child_by_field_name("name").unwrap_or(ty)
}

/// 调用点上**被调名字 token** 的位置 `(1-based 行, 0-based 列)`。
///
/// 判据语言无关:call 节点里**第一个**文本等于被调名的标识符类节点 ——
/// 被调名总在实参之前(`a.b.m(x)` 的 `m` 在 `(x)` 前),所以"第一个"就是它。
/// 连 `m(m)` 这种实参与被调同名的也对。
///
/// 🔴 这个位置是给**精确层**(LSP)用的:`textDocument/definition` 要的是
/// 名字 token 的坐标,给调用点起始坐标会问到接收者头上、答出另一个东西。
/// ⚠ 按文本找过(`line.find(name)`)的粗办法实测 60 例里失手 14 例 —— 不能那么干。
pub fn call_name_pos(call: Node, src: &[u8], name: &str) -> Option<(usize, usize)> {
    fn walk<'a>(n: Node<'a>, src: &[u8], name: &str, best: &mut Option<Node<'a>>) {
        if matches!(
            n.kind(),
            "identifier" | "field_identifier" | "property_identifier" | "type_identifier"
        ) && n.utf8_text(src).is_ok_and(|t| t == name)
        {
            let better = best.is_none_or(|b| n.start_byte() < b.start_byte());
            if better {
                *best = Some(n);
            }
            return;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, name, best);
        }
    }
    let mut best = None;
    walk(call, src, name, &mut best);
    let n = best?;
    Some((n.start_position().row + 1, n.start_position().column))
}

/// 从「基类 / 接口列表」子树里收类型名(`implements A, B` · `: A, B` · `: public A`)。
///
/// 各门那层包装不同(`type_list` · `class_heritage` · `base_list` · `base_class_clause`),
/// 但**里面都是一串类型标识符** —— 递归收标识符类节点即可,不必每门认一遍包装。
/// 访问修饰符(`public`)不是标识符节点,自然被滤掉。
pub fn collect_type_names(node: Node, src: &[u8], out: &mut Vec<String>) {
    if matches!(node.kind(), "type_identifier" | "identifier") {
        if let Ok(t) = node.utf8_text(src) {
            let n = outermost_type_name(t);
            if !n.is_empty() {
                out.push(n);
            }
        }
        return;
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        collect_type_names(ch, src, out);
    }
}

/// 在「字段 / 形参声明」节点上找**名字标识符**。
///
/// 依次试 `name` → `declarator`,拿不到标识符就再往里走一层(最多 3 跳):
/// Java 的 `declarator: variable_declarator(name:)`、C 的 `pointer_declarator(declarator:)`
/// 都要多走这一步。走不到就是 `None` —— **不猜**。
fn name_ident<'a>(n: Node<'a>) -> Option<Node<'a>> {
    let mut cur = n;
    for _ in 0..3 {
        if matches!(
            cur.kind(),
            "identifier" | "field_identifier" | "property_identifier"
        ) {
            return Some(cur);
        }
        cur = cur
            .child_by_field_name("name")
            .or_else(|| cur.child_by_field_name("declarator"))?;
    }
    None
}

/// 从「类型声明」节点抽 (类型名, [(字段名, 字段类型名)])。
///
/// 通用形:类型名在 `name` 字段;每个字段节点的类型在 `type` 字段、名字走
/// [`name_ident`]。不合这一形的(C# 的类型挂在祖父上、Kotlin 全靠位置)自己实现。
pub fn type_fields_from(
    node: Node,
    src: &[u8],
    body: Option<Node>,
    field_kinds: &[&str],
) -> Option<(String, Vec<(String, String)>)> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(src)
        .ok()?
        .to_string();
    let body = body?;
    let mut out = Vec::new();
    let mut c = body.walk();
    for f in body.children(&mut c) {
        if !field_kinds.contains(&f.kind()) {
            continue;
        }
        let (Some(id), Some(ty)) = (name_ident(f), f.child_by_field_name("type")) else {
            continue; // 名字或类型有一个拿不到就跳过 —— 不猜
        };
        let (Ok(fname), Ok(tytext)) = (id.utf8_text(src), type_name_node(ty).utf8_text(src)) else {
            continue;
        };
        let tyname = outermost_type_name(tytext);
        if !tyname.is_empty() {
            out.push((fname.to_string(), tyname));
        }
    }
    Some((name, out))
}

/// 函数体里所有 `return <expr>` 的表达式(按本门的 return 语句 kind 找)。
///
/// **不下钻不透明域** —— 嵌套函数 / 闭包里的 `return` 是它自己的,
/// 算进来就会把别人的返回值当成本函数的(那是假事实)。
pub fn return_exprs_in<'a>(spec: &dyn LangSupport, body: Node<'a>, kind: &str) -> Vec<Node<'a>> {
    fn walk<'a>(spec: &dyn LangSupport, n: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
        if n.kind() == kind {
            if let Some(v) = n.named_child(0) {
                out.push(v);
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            if spec.is_opaque_scope(ch) {
                continue;
            }
            walk(spec, ch, kind, out);
        }
    }
    let mut out = Vec::new();
    walk(spec, body, kind, &mut out);
    out
}

/// 从「形参容器」节点取形参的**类型名**(按位置,与 `param_names_from` 对齐)。
///
/// 每个形参读 `type` 字段,再用 `outermost_type_name` 取最外层名
/// (`&mut S` → `S`,`Vec<T>` → `Vec`)。没有 `type` 字段的位置给 `None`。
///
/// 🔴 这是**读标注**不是推断 —— 类型名就是源码里那几个字。
pub fn param_types_from(
    container: Option<Node>,
    src: &[u8],
    skip_first: bool,
) -> Vec<Option<String>> {
    let Some(c) = container else {
        return Vec::new();
    };
    // 单参简写(JS 的 `x => ..`)没有类型可言,但**位置要占住**
    if c.kind() == "identifier" {
        return vec![None];
    }
    let mut out = Vec::new();
    for i in 0..c.named_child_count() {
        if skip_first && i == 0 {
            continue;
        }
        let p = c.named_child(i).expect("named_child_count 内");
        out.push(
            p.child_by_field_name("type")
                .and_then(|t| type_name_node(t).utf8_text(src).ok())
                .map(outermost_type_name)
                .filter(|t| !t.is_empty()),
        );
    }
    out
}

/// 从「实参容器」节点取实参(按位置)。只有简单标识符给名,其余占位 `None`。
///
/// `peel` 列出**不改变值身份的包裹节点** kind,会剥掉再看里面 ——
/// Rust 的 `f(&x)` / `f(&mut x)` 是借用,传的还是 `x` 那个值。
/// 🔴 这一条在 Rust 上不做的话,参数流少一大半(`f(&x)` 是最常见的写法之一)。
/// ⚠ 只许剥**真的不改变身份**的:`&x` 可以,`x.clone()` / `*x` / `x as u64` **不许** ——
/// 那些是新值或换了语义,剥了就是记假事实。
pub fn call_args_from(container: Option<Node>, src: &[u8], peel: &[&str]) -> Vec<Option<String>> {
    let Some(c) = container else {
        return Vec::new();
    };
    (0..c.named_child_count())
        .map(|i| {
            let mut a = c.named_child(i).expect("named_child_count 内");
            // 逐层剥包裹(`&&x` 这种嵌套也剥得掉);剥到不认识的就停
            while peel.contains(&a.kind()) {
                match a.named_child(a.named_child_count().saturating_sub(1)) {
                    Some(inner) => a = inner,
                    None => break,
                }
            }
            if a.kind() == "identifier" {
                a.utf8_text(src).ok().map(String::from)
            } else {
                None
            }
        })
        .collect()
}

/// 从「实参容器」节点取**每个实参提到的标识符**(按位置)。
pub fn arg_mentions_from(container: Option<Node>, src: &[u8]) -> Vec<Vec<String>> {
    let Some(c) = container else {
        return Vec::new();
    };
    (0..c.named_child_count())
        .map(|i| access_paths_in(c.named_child(i).expect("named_child_count 内"), src))
        .collect()
}

/// 过程内的**流上下文**:缩窄器 + 别名表。两件都是「建边时才知道、语言层不知道」的东西,
/// 打包传比加两个参数清楚。
pub struct FlowCtx<'a> {
    /// 「这个初始化表达式里,真能流到它的值的标识符是哪些」——
    /// `None` 或返回 `None` ⇒ 退回「提到的都算」(宽,方向安全)。
    pub narrow: Narrow<'a>,
    /// **语法别名**:`t` → `s`(`let t = &s` / `t = s`)。
    /// 路径在记录与查询前都先把**根**换成规范名,这样经别名的**写**也认得出:
    /// `let t = &mut s; t.f = x; g(s.f)` —— 不做归一的话 `t.f` 与 `s.f` 是两条不相关的路径。
    ///
    /// ⚠ 只认**语法上看得见**的别名(右边是个裸路径 / 对它的借用)。
    /// 经函数参数传进来的别名、经集合下标的别名,**都不在射程内** —— 那要过程间指向分析。
    pub aliases: &'a HashMap<String, String>,
    /// 调用点的**过程间副作用**(见 [`Effects`])。
    pub effects: Effects<'a>,
}

impl FlowCtx<'_> {
    /// 把一条访问路径的**根**换成规范名。`t.f` 在 `t → s` 时变成 `s.f`。
    /// 有界跟随(8 跳),防 `a = b; b = a` 这类环。
    /// 这个调用点产生哪些定义(没接 / 不是调用 ⇒ 空)。
    pub fn call_effects(&self, node: Node) -> Vec<(String, Vec<String>)> {
        self.effects.map(|f| f(node)).unwrap_or_default()
    }

    pub fn canon(&self, path: &str) -> String {
        let (root, rest) = match path.split_once('.') {
            Some((r, t)) => (r, Some(t)),
            None => (path, None),
        };
        let mut cur = root;
        for _ in 0..8 {
            match self.aliases.get(cur) {
                Some(next) if next != cur => cur = next,
                _ => break,
            }
        }
        match rest {
            Some(t) => format!("{cur}.{t}"),
            None => cur.to_string(),
        }
    }
}

/// 扫一个函数体,收**局部绑定的名字**(含形参)。
///
/// 🔴 用途:**局部绑定遮蔽模块级名字**,这是语言规则不是猜。
/// `let empty = || ...; empty()` 调的是那个闭包,不是仓里碰巧同名的 `LspEngine::empty`。
/// 不认这一条会产生**假的 Exact 边** —— 实测 code-picture 自己的架构图上
/// 因此出现了一条 `core/src → lsp/src`,而那个依赖方向是 Cargo.toml 里写死禁止的。
///
/// ⚠ 不下钻不透明域:闭包里的绑定是闭包自己的作用域(见 `graph::calls` 的分层作用域)。
pub fn collect_local_names(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    out: &mut HashSet<String>,
) {
    if let Some((name, _, _)) = spec.local_binding(node, src) {
        // 只收裸名:`s.f = ..` 那种路径写入不是在绑定一个可调用的名字
        if !name.contains('.') {
            out.insert(name);
        }
    }
    for p in spec.param_names(node, src).into_iter().flatten() {
        out.insert(p);
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if spec.is_opaque_scope(ch) {
            continue;
        }
        collect_local_names(spec, ch, src, out);
    }
}

/// 扫一个函数体,建**语法别名表**(`t` → `s`)。
///
/// 认这两形(右边必须是**裸访问路径**,或对它的借用):
/// * `let t = s;` / `t = s;`
/// * `let t = &s;` / `let t = &mut s;`
///
/// 🔴 **不认**的:`let t = f(s)`(经函数)· `let t = v[i]`(经下标)
/// · 经参数传进来的别名。那些要过程间指向分析,不是语法能定的。
/// ⚠ 认错的后果是把两条本不相关的路径当成一条 ⇒ **多记「可能」**,不会漏。方向安全。
pub fn collect_aliases(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    out: &mut HashMap<String, String>,
) {
    if let Some((name, _, Some(init))) = spec.local_binding(node, src) {
        // 🔴 剥壳必须**纯文本**,不能结构式下钻。
        // 结构式下钻会钻进 `pick(x, z)` 的实参,把 `y` 记成 `z` 的别名 —— 那是编的。
        // 文本剥前缀只认「借用/解引用外壳」这一形,剥完还得整体是一条裸路径。
        let Ok(text) = init.utf8_text(src) else {
            return;
        };
        let stripped = text
            .trim()
            .trim_start_matches('&')
            .trim_start()
            .trim_start_matches("mut ")
            .trim_start()
            .trim_start_matches('*');
        if let Some(target) = path_of_text(stripped) {
            // 只给**根**建别名(`let t = s.f` 不算 t≡s,那是取值不是取别名)
            if !target.contains('.') && target != name && !name.contains('.') {
                out.insert(name, target);
            }
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
        collect_aliases(spec, child, src, out);
    }
}

/// 访问路径的最大段数。`a.b.c.d` 截成 `a.b.c` —— k 限深是这类分析的标准做法:
/// 不限深的话递归数据结构会让路径集合无界。截断只会让两条路径**更容易**被判成相关
/// (前缀关系),而那是**宽**的方向,安全。
pub const ACCESS_PATH_K: usize = 3;

/// 这个节点是不是一条**访问路径**(`a` / `a.b` / `a.b.c`)?是则返回它的文本(k 限深)。
///
/// 🔴 判据是**源文本形状**,不是节点 kind —— 「点分标识符链」在 Rust / Python / JS /
/// Java / C# 里长得一模一样,不必每门认一遍节点名。
/// 自然排除:`a.b()`(有括号)· `a[i]`(有方括号)· `std::fs`(是 `::`)· `a + b`。
///
/// ⚠ 不认 C 的 `s->f`(那是另一种写法,本版不收)。
pub fn access_path(node: Node, src: &[u8]) -> Option<String> {
    path_of_text(node.utf8_text(src).ok()?)
}

/// 路径判据的**文本级**本体:`access_path` 与别名剥壳共用这一份,
/// 免得两处各判一遍、判歪了还不自知。
fn path_of_text(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.len() > 200 {
        return None;
    }
    let segs: Vec<&str> = t.split('.').collect();
    if segs.iter().any(|sg| {
        sg.is_empty()
            || !sg
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            || !sg.chars().all(|c| c.is_alphanumeric() || c == '_')
    }) {
        return None;
    }
    Some(segs[..segs.len().min(ACCESS_PATH_K)].join("."))
}

/// 收集子树里出现的**极大访问路径**(`s.f.g` 只给一次,不再给 `s.f` 与 `s`)。
/// 极大 = 它自己是路径,而它的父节点不是。
pub fn access_paths_in(node: Node, src: &[u8]) -> Vec<String> {
    fn walk(n: Node, src: &[u8], out: &mut Vec<String>) {
        if let Some(p) = access_path(n, src) {
            let parent_is_path = n.parent().is_some_and(|pa| access_path(pa, src).is_some());
            if !parent_is_path {
                out.push(p);
                return; // 极大路径内部不必再看
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, out);
        }
    }
    let mut out = Vec::new();
    walk(node, src, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

/// 两条访问路径**是否相关**:一条是另一条的前缀(按段比)。
///
/// `s` 与 `s.f` 相关(写 `s` 影响读 `s.f`,反之写 `s.f` 也让 `s` 变了);
/// `s.f` 与 `s.g` **不相关** —— 这正是字段敏感买到的东西。
///
/// ⚠ **不考虑别名**:`let t = &mut s; t.f = x` 之后 `s.f` 与 `t.f` 在本函数里被当成两条
/// 不相关的路径。别名是下一阶段的事(见 `crate::graph::locals` 的别名表)。
pub fn paths_related(a: &str, b: &str) -> bool {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (Some(p), Some(q)) if p == q => continue,
            (Some(_), Some(_)) => return false,
            _ => return true, // 一条走完了 ⇒ 是另一条的前缀
        }
    }
}

/// 一条访问路径的**根**(`s.f.g` → `s`)。形参匹配看的是根。
pub fn path_root(p: &str) -> &str {
    p.split('.').next().unwrap_or(p)
}

/// 收集一棵子树里出现的全部标识符(去重、升序)。语言无关 —— `identifier` 是各 grammar 通用的 kind。
///
/// ⚠ 它**不区分**这个标识符是变量还是函数名(`h(x)` 会同时收到 `h` 和 `x`)。
/// 调用方只拿它跟**形参名**比对,所以多收的那些通常匹配不上;
/// 但形参恰好与某个函数同名时会多记一条「可能流到」—— 这一档本就是**过度近似**,
/// 如实标成「可能」即可,不当确定事实用。
pub fn identifiers_in(node: Node, src: &[u8]) -> Vec<String> {
    fn walk(n: Node, src: &[u8], out: &mut Vec<String>) {
        if n.kind() == "identifier" {
            if let Ok(t) = n.utf8_text(src) {
                out.push(t.to_string());
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, out);
        }
    }
    let mut out = Vec::new();
    walk(node, src, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

/// F68 共享 helper：取函数定义节点从起始到 body 起点之间的文本，折单行去尾。
/// body 定位两条路：① `body` 命名字段（多数语言）；② 无字段名时按**子节点 kind**
/// `function_body` 扫（Kotlin 的 `tree-sitter-kotlin-ng` 函数体是节点 kind 无字段名，
/// 早先误当字段名 `child_by_field_name("function_body")` 查 → 恒 None，整门签名丢失）。
pub fn signature_before_body(node: Node, src: &[u8]) -> Option<String> {
    let body = node
        .child_by_field_name("body")
        .or_else(|| child_by_kind(node, "function_body"))?;
    signature_before(node, body, src)
}

/// 取 `node.start` 到 `body.start` 之间的文本，折单行 + 去尾 `{`/`=`/空白。
/// `javascript.rs` 的箭头 override 也复用它（传 arrow 的 body）。
pub fn signature_before(node: Node, body: Node, src: &[u8]) -> Option<String> {
    let sig = src.get(node.start_byte()..body.start_byte())?;
    let text = std::str::from_utf8(sig).ok()?;
    // split_whitespace().join(" ") 已无首尾空白;trim_end_matches 去尾 `{`/`=`/空格即够。
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_end_matches(['{', '=', ' ']);
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// 按 kind 找第一个直接子节点（tree-sitter 有些语法把结构放在 kind 而非命名字段上）。
///
/// ⚠ `clippy::manual_find` 会建议改成 `node.children(&mut cursor).find(...)` —— **那个改法编不过**：
/// 迭代器借着 `cursor`，作为尾表达式时临时值活得比 `cursor` 久（E0597）。改 `let` 绑定撞
/// `let_and_return`、加 `return` 撞 `needless_return`，没有干净的重写 ⇒ 就地 allow。
#[allow(clippy::manual_find)]
pub fn child_by_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

// ── 过程内的语法可及信息(spec 驱动,不认识具体语言;`symbols` 与 `graph` 都要用)──

/// 扫一个函数体,建**局部变量 → 它的来源标识符**表(流不敏感的过程内定义-使用)。
///
/// `let y = h(x) + 1;` ⇒ `y` 的来源里有 `x`(和 `h`,见 `identifiers_in` 的诚实边界)。
///
/// 🔴 **流不敏感 = 过度近似**:重新赋值不区分先后 ——
/// `y = a; g(y); y = b; g(y)` 会算成 a 和 b 都流到了两个 `g`。
/// 所以它产出的是「**可能**流到」,边上用 `~` 标记,与确定的 `>` 分开。
/// 要区分先后得有控制流图,那是第三档。
pub fn collect_local_sources(
    spec: &dyn LangSupport,
    node: Node,
    src: &[u8],
    ctx: Option<&FlowCtx>,
    out: &mut HashMap<String, Vec<String>>,
) {
    if let Some((name, _, Some(init))) = spec.local_binding(node, src) {
        // 缩窄:`let y = h(x, z)` 若 h 的 return_flow 只含第 0 参 ⇒ y 的来源只有 x。
        // 拿不到缩窄器(没有符号表 / 解析不出被调)⇒ 退回「提到的都算」,**只会多记不会漏**。
        // 用**访问路径**而非裸标识符:`let y = s.f + 1` 的来源是 `s.f`,不是 `s` 和 `f`
        let mut ids = ctx
            .and_then(|c| c.narrow)
            .and_then(|f| f(init))
            .unwrap_or_else(|| access_paths_in(init, src));
        // 路径的根换成规范名 ⇒ 经别名的写也认得出(`t.f = x` 记成 `s.f ← x`)
        let name = ctx.map(|c| c.canon(&name)).unwrap_or(name);
        if let Some(c) = ctx {
            for id in ids.iter_mut() {
                *id = c.canon(id);
            }
        }
        ids.retain(|i| i != &name); // `let x = x + 1` 自指不记,否则闭包算不完
        if !ids.is_empty() {
            out.entry(name).or_default().extend(ids);
        }
    }
    // 过程间:调用点按被调的**函数摘要**记下定义(`store(&mut s, x)` ⇒ `s.f ← x`)
    if let Some(c) = ctx {
        for (target, srcs) in c.call_effects(node) {
            let target = c.canon(&target);
            let mut srcs: Vec<String> = srcs.iter().map(|s| c.canon(s)).collect();
            srcs.retain(|s| s != &target); // 自指不记,否则传递闭包算不完
            if !srcs.is_empty() {
                out.entry(target).or_default().extend(srcs);
            }
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
        collect_local_sources(spec, child, src, ctx, out);
    }
}

/// 一个标识符**可能**来自哪些形参(顺来源表做传递闭包)。
/// 有界:最多 8 层,防病态输入打转(正常代码远到不了)。
pub fn param_origins(
    ident: &str,
    sources: &HashMap<String, Vec<String>>,
    params: &[Option<String>],
    out: &mut Vec<usize>,
) {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut frontier = vec![ident];
    for _ in 0..8 {
        let mut next: Vec<&str> = Vec::new();
        for cur in frontier {
            if !seen.insert(cur) {
                continue;
            }
            // 形参匹配看路径的**根**:`s.f` 的根是 `s`,`s` 是形参就算命中
            if let Some(i) = params
                .iter()
                .position(|p| p.as_deref() == Some(path_root(cur)))
            {
                out.push(i);
            }
            // 来源匹配按**前缀相关**:写 `s` 影响读 `s.f`,但 `s.f` 与 `s.g` 互不相干
            for (k, v) in sources.iter() {
                if paths_related(k, cur) {
                    next.extend(v.iter().map(|s| s.as_str()));
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    out.sort_unstable();
    out.dedup();
}
