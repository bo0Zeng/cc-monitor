//! 语言支持层:**契约 + 注册表**。
//!
//! 递归驱动(`symbols` / `graph`)对每个节点只问几件事,答法按 `Lang` 分发 ——
//! 驱动本身语言无关,每门语言只实现 `LangSupport`。
//!
//! 共用的纯函数助手住 `helpers`(数名字、剥包裹、拼路径);
//! 每门语言一份实现住 `lang/<名>.rs`;Rust 的控制流建模单独住 `lang/rust_cfg.rs`
//! (控制流是**一门语言一套语义模型**,体量与那几个小问题不是一个量级)。

use crate::model::{Lang, SymKind};
use tree_sitter::{Language, Node};

mod c;
mod cpp;
mod csharp;
mod helpers;
mod java;
mod javascript;
mod kotlin;
mod python;
mod rust;
mod typescript;
pub use helpers::*;

impl Lang {
    /// 按文件扩展名判定语言。无法识别(非源码 / 未支持扩展名)→ None。
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = path.rsplit_once('.').map(|(_, e)| e)?;
        Some(match ext {
            "rs" => Lang::Rust,
            "py" | "pyi" => Lang::Python,
            "js" | "jsx" | "mjs" | "cjs" => Lang::JavaScript,
            "ts" | "tsx" | "mts" | "cts" => Lang::TypeScript,
            "java" => Lang::Java,
            "kt" | "kts" => Lang::Kotlin,
            "c" => Lang::C,
            // .h/.hpp/... 头文件 C/C++ 二义 → 统一归 C++(其 grammar 兼容 C 头,取舍见主计划 §8.1)
            "cc" | "cpp" | "cxx" | "c++" | "h" | "hpp" | "hh" | "hxx" => Lang::Cpp,
            "cs" => Lang::CSharp,
            _ => return None,
        })
    }

    /// 该语言的 tree-sitter grammar。F11 全部接线(8 门依赖全"被使用",无 unused-dep)。
    pub fn ts_language(self) -> Language {
        match self {
            Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
            Lang::Python => tree_sitter_python::LANGUAGE.into(),
            Lang::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Lang::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Lang::Java => tree_sitter_java::LANGUAGE.into(),
            Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            Lang::C => tree_sitter_c::LANGUAGE.into(),
            Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        }
    }
}

/// 一个符号定义节点的分类结果。`qualifier`/`kind` 让语言在**定义处自带**限定与种类,
/// 覆盖驱动"纯靠祖先容器 + 限定有无定 kind"的默认——支撑 thin 版兜不住的结构:
/// C++ 类外定义 `void A::method(){}`(无 class 祖先)、命名空间自由函数(作用域≠类型)、
/// class/类型容器成一等节点等(设计取自 MASTERPLAN §8.1 `SymbolDef`)。
pub struct SymbolDef {
    pub name: String,
    /// 定义**自带**的类型限定(如 `A::method` 的 `A`);Some 覆盖祖先容器限定,None 回落祖先。
    pub qualifier: Option<String>,
    /// 显式种类;None = 由驱动按"最终限定有无"定 Method/Function(Rust 及多数语言够用)。
    pub kind: Option<SymKind>,
}

/// 语言相关的 AST 分类:递归驱动(symbols/graph)对每个节点问这三件事。
/// 节点类型字符串是各 grammar 的 `node-types`,故实现细节封在各语言模块里。
pub trait LangSupport: Sync {
    /// 进入该节点时,若它为**子节点**设定类型限定(Rust `impl`、类 class、命名空间等),
    /// 返回限定名(例:`impl Foo` → `Foo`)。否则 None。
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String>;

    /// 该节点是否声明一个符号(可调用体 / 类容器)。是则返回其分类(名 + 自带限定? + 种类?)。
    /// 只经 `qualifier_of` 给子设限定、自身不产符号的节点(如 Rust `impl`)返回 None。
    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef>;

    /// 该节点是否为**调用点**。是则返回 (被调裸名, 类型限定?, 是否方法调用 `recv.m()`)。
    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)>;

    /// F68：符号的**签名文本**（`fn foo(a:u32)->String` / `def f(a):`）。默认实现取函数
    /// 定义节点从起始到 `body` 起点之间的文本、折单行（覆盖多数语言的函数定义节点）；
    /// body 拿不到时返回 None（不硬凑）。**箭头赋值（JS/TS `const f = () => {}`）的 body
    /// 藏在 `value` 下的 arrow 里，默认 helper 取不到 → 由 `javascript.rs` override**。
    fn signature_of(&self, node: Node, src: &[u8]) -> Option<String> {
        signature_before_body(node, src)
    }

    /// 这个节点在控制流上是什么角色。默认 `Plain` —— **未实现的语言等于「全是顺序语句」**,
    /// 那会漏掉所有分支边 ⇒ 所以 `control_flow` 默认返回 `None`,不让默认实现产出图。
    fn ctrl_role<'a>(&self, _node: Node<'a>, _src: &[u8]) -> CtrlRole<'a> {
        CtrlRole::Plain
    }

    /// 这个节点是不是「语句容器」(Rust/Python 的 `block`、JS 的 `statement_block`)。
    fn is_block(&self, _node: Node) -> bool {
        false
    }

    /// 这个节点是不是**不该下钻**的作用域(闭包 / 嵌套函数)——
    /// 它何时执行是另一回事,放进本函数的 CFG 就是假的顺序。
    fn is_opaque_scope(&self, _node: Node) -> bool {
        false
    }

    /// 这个**构造函数**节点有哪些「参数属性」——写在形参表里、同时就是字段的那种。
    /// 返回 `(形参位, 字段名)`。
    ///
    /// TS 的 `constructor(private theme: Theme)` 等价于 `this.theme = theme`,
    /// 但函数体里**没有这条赋值** —— 不认它,这一形的函数摘要整类抓不到。
    /// (Kotlin 的 `class C(val idx: Index)` 同理,但主构造器不是可调用符号,
    ///  那门只在 `type_fields` 里认它。)
    fn param_properties(&self, _node: Node, _src: &[u8]) -> Vec<(usize, String)> {
        Vec::new()
    }

    /// 这个节点声明了**实现关系**吗?是则给出 (类型名, [它实现/继承的 trait / 接口名])。
    ///
    /// 用途:接收者类型是 trait 时,调用是**动态派发**。
    /// 🔴 那时候两种答案都是错的:
    /// * 解成「trait 的默认实现」⇒ **假的 Exact**(实现者会覆盖它);
    /// * 解成「看不见」⇒ 也错 —— 那些实现明明在仓里。
    ///
    /// 正确答案是「候选 = 仓内各实现者的同名方法」,标 `AmbiguousCall` + `Confidence::Dispatch`。
    ///
    /// ⚠ trait 在仓内**一个实现都没有**时认不出它是 trait(这张表是从实现关系反推的)。
    fn impl_of(&self, _node: Node, _src: &[u8]) -> Option<(String, Vec<String>)> {
        None
    }

    /// 这个节点是不是一个**类型声明**(struct / class)?是则给出
    /// (类型名, [(字段名, 字段类型名)])。
    ///
    /// 🔴 字段的类型标注**写在声明里** —— 读它与读形参标注是同一件事,不是推断。
    /// 不读它的话 `self.idx.m()` 这类调用连接收者类型都定不下来(本仓 139 处这种写法)。
    /// ⚠ 动态语言(Python / JS)没有可靠的字段声明形 ⇒ `None`(不是空表:那是「没这一说」)。
    /// ⚠ 取不到类型的字段**跳过**,不占位 —— 这张表是按名查的,不像形参表要对齐位置。
    fn type_fields(&self, _node: Node, _src: &[u8]) -> Option<(String, Vec<(String, String)>)> {
        None
    }

    /// 本门里**接收者**的名字(`self` / `this`)。没有接收者这一说的语言 → `None`。
    ///
    /// 用在函数摘要上:`fn set(&mut self, v) { self.f = v; }` 的 `self` 不在形参表里,
    /// 不把它算一个槽位的话,这最常见的一形就完全记不下来(实测本仓因此产出为 0)。
    fn receiver_name(&self) -> Option<&'static str> {
        None
    }

    /// 这个节点是不是**闭包 / lambda**?是则给出它自己的形参名(按位置)。
    ///
    /// 与 `is_opaque_scope` 分工清楚:
    /// * `is_opaque_scope` 管 **CFG 定序** —— 闭包体何时执行是另一回事,不进当前流;
    /// * 这个管**名字遮蔽** —— 闭包形参挡住外层同名的形参与局部。
    ///
    /// 🔴 不做遮蔽的后果是**假事实,不是过度近似**:
    /// ```text
    /// fn take(x: u32) { let f = |x: u32| sink(x); }
    /// ```
    /// 那个 `x` 是闭包自己的,可不遮蔽就会记成「take 的第 0 参**确定**(`>`)流到 sink」。
    /// 过度近似只是多说「可能」,这个是直接说错。
    ///
    /// 默认 `None` = 本门没有闭包这一形(C),或尚未接入。
    fn closure_params(&self, _node: Node, _src: &[u8]) -> Option<Vec<Option<String>>> {
        None
    }

    /// 这条语句里有没有**隐式早退**(Rust 的 `?`)。
    fn has_implicit_exit(&self, _node: Node) -> bool {
        false
    }

    /// 该**函数定义**节点的**函数体**。
    ///
    /// 🔴 **实现了它就等于承诺也实现了 `ctrl_role`** —— 否则默认的 `ctrl_role`
    /// 把一切当顺序语句,会建出一张**漏掉全部分支边**的图,而那正是最危险的结果
    /// (漏边让「这两个调用互斥」变成假事实)。默认 `None` ⇒ `control_flow` 也返回 `None`。
    fn cfg_function_body<'a>(&self, _node: Node<'a>) -> Option<Node<'a>> {
        None
    }

    /// 该**函数定义**节点的**形参容器** + 要不要跳过第 0 个(接收者形参)。
    ///
    /// 🔴 `param_names` 与 `param_types` 都从这里派生 —— 两处各找一遍容器的话,
    /// 迟早有一处跟另一处对不上位,而那种错**静默**(名字与类型错位 = 记假事实)。
    /// 接这个方法就同时点亮了名字与类型两件。
    fn params_container<'a>(&self, _node: Node<'a>, _src: &[u8]) -> Option<(Node<'a>, bool)> {
        None
    }

    /// 该**函数定义**节点的形参名,按位置。**不含接收者形参**(Rust 的 `&self`、
    /// Python 的 `self`)—— 去掉它,索引才与调用点的实参表对齐。
    /// 取不到名的位置(解构等)给 `None`,**位置要占住**,否则后面的全错位。
    fn param_names(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        match self.params_container(node, src) {
            Some((c, skip)) => param_names_from(Some(c), src, skip),
            None => Vec::new(),
        }
    }

    /// 该**函数定义**节点的形参**类型名**,按位置(与 `param_names` 一一对齐)。
    ///
    /// 🔴 这是**读签名**,不是类型推断 —— `fn take(s: &mut S)` 里 `S` 三个字就写在那儿。
    /// 不读它的话,`s.set(x)` 这类最常见的方法调用连被调都定不下来(接收者类型未知
    /// ⇒ 一律算歧义),于是函数摘要在真实代码上全落不了地。
    /// 没写类型的位置给 `None`(动态语言的常态),**位置要占住**。
    fn param_types(&self, node: Node, src: &[u8]) -> Vec<Option<String>> {
        match self.params_container(node, src) {
            Some((c, skip)) => param_types_from(Some(c), src, skip),
            None => Vec::new(),
        }
    }

    /// 该**调用点**每个实参**提到的标识符**(按位置)。
    ///
    /// 给「流不敏感的可能流到」用:`g(x + 1)` 的第 0 个实参提到了 `x`。
    /// 与 `call_args` 的区别 —— 那个只认**裸标识符**(确定原样传),这个认**提到**(可能流到)。
    /// 两者在边上用不同标记记,输出里也分开说。
    fn call_arg_mentions(&self, _node: Node, _src: &[u8]) -> Vec<Vec<String>> {
        Vec::new()
    }

    /// 该**调用点**的实参,按位置;**仅当实参是个简单标识符**时给 `Some(名字)`。
    /// 复杂表达式(`f(a.b)` / `f(g(x))` / 字面量)给 `None` —— 那要真的值流分析。
    /// 同样**位置要占住**。
    fn call_args(&self, _node: Node, _src: &[u8]) -> Vec<Option<String>> {
        Vec::new()
    }

    /// 该**函数定义**节点里,**值会成为返回值**的那些表达式。
    ///
    /// 用途:算「哪几个形参的值会流到返回值」。`fn h(a, b) -> T { a }` 的答案是 `[0]`,
    /// 于是 `let y = h(x, z)` 之后,`y` 的来源只有 `x` —— 没有它的话 `z` 也会被算进去。
    /// 默认空 = 本门未实现(⇒ 不缩窄,只会多记「可能」,方向安全)。
    fn return_exprs<'a>(&self, _node: Node<'a>, _src: &[u8]) -> Vec<Node<'a>> {
        Vec::new()
    }

    /// 该**函数定义**节点的返回类型名。`-> Graph` → `Graph`;
    /// 泛型包装(`-> Result<Graph, E>` / `-> Option<Graph>`)取**最外层**名 —— 够用且不猜。
    /// 默认 None(本门未实现 / 无返回类型)。
    fn return_type_of(&self, _node: Node, _src: &[u8]) -> Option<String> {
        None
    }

    /// 该节点是不是一条**局部绑定**(`let x = ..` / `const x = ..`)。
    /// 返回 `(变量名, 显式类型?, 初始化表达式?)`。
    ///
    /// 显式类型有就直接用;没有则由驱动看初始化表达式是不是调用 ——
    /// 是的话拿被调函数的**返回类型**当这个变量的类型。
    fn local_binding<'a>(
        &self,
        _node: Node<'a>,
        _src: &[u8],
    ) -> Option<(String, Option<String>, Option<Node<'a>>)> {
        None
    }

    /// 方法调用的**接收者访问路径**:`g.run()` → `g`,`self.idx.m()` → `self.idx`。
    ///
    /// 判据与别处一致 —— 是不是一条**点分标识符链**。于是 `a.b().c()`(有括号)、
    /// `v[i].m()`(有方括号)自动排除:那些要真的类型推导,不在本层射程内。
    /// 🔴 认到 `self.idx` 这一层是必需的:本仓有 139 处 `self.<字段>.<方法>()`,
    /// 只认裸标识符的话这些调用连接收者类型都定不下来(见 `graph::calls::receiver_type`)。
    fn receiver_of(&self, _node: Node, _src: &[u8]) -> Option<String> {
        None
    }

    /// 这个调用点的接收者是不是**本对象**(`self.m()` / `Self::m()` / `this.m()`)?
    ///
    /// 🔴 **是的话就不是猜** —— 被调一定在当前 `impl` / `class` 里,
    /// 这是**语言规则**,不需要类型推导。驱动会拿当前所在类型当限定名去精确解析。
    ///
    /// 默认 `false`:本门未实现 ⇒ 照旧算歧义。**只会少报,不会错报。**
    fn is_self_call(&self, _node: Node, _src: &[u8]) -> bool {
        false
    }

    /// 该节点是否是一条 import / use / include / require。是则返回**模块说明符原文**
    /// (`./auth` · `crate::model::Lang` · `a.b.c` · `com.x.Foo` · `a/b.hpp`)。
    ///
    /// 返回 `Vec` 是因为**一条语句可以带多个模块**(Python `import a, b`);
    /// 空 = 这个节点不是 import,或本门未实现抽取(不报错、不产边)。
    fn import_of(&self, _node: Node, _src: &[u8]) -> Vec<String> {
        Vec::new()
    }

    /// 把说明符解析成**候选仓内路径**(按优先级排;调用方取第一个真在库里的)。
    /// 候选可以是完整仓库相对路径,也可以是**路径后缀**(Java/Kotlin 的包名→目录约定
    /// 不含 `src/main/java/` 这种前缀) —— 调用方先试精确、再试后缀。
    ///
    /// `from_file` = 导入方的仓库相对路径(解相对导入用)。
    /// 空 = 外部依赖 / **本门语言解析不到文件**(如 C# 的 `using` 是命名空间,
    /// 语言本身没有「一个命名空间一个文件」的规矩) —— 两者在此不可分,一律不产边。
    fn resolve_import(&self, _from_file: &str, _specifier: &str) -> Vec<String> {
        Vec::new()
    }
}

/// 一个节点在**控制流**上的角色。语言层只回答这一个问题,建图的算法由
/// `crate::cfg::build` 一份通用实现负责 —— 这样扇出一门语言只要几十行分类,
/// 不用把建图逻辑整份抄一遍。
///
/// 🔴 **拿不准就给 `Unsupported`**:整份作废好过漏一条边
/// (漏边会让「这两个调用互斥」变成假事实)。
pub enum CtrlRole<'a> {
    /// 普通语句(可能含调用点与局部定义)。
    ///
    /// 🔴 **它承诺「这个节点里没有嵌套的控制流」** —— 通用构造器会**核这条承诺**:
    /// 子树里若冒出任何非 `Plain` 的控制节点,整份作废。
    /// 不核的话,`with x:` / 块表达式里的 if 会被静默吞掉 ⇒ 漏边 ⇒ 假事实。
    Plain,
    /// 纯粹的语句容器(Python 的 `with`、裸块):**照语句串下钻**,不产分支。
    Sequence { body: Node<'a> },
    /// 分支:若干**互斥**的分支体。`exhaustive` = 有兜底分支(else / default / `_`)——
    /// 没有的话要补一条「条件全不中」的边到汇合点。
    Branch {
        cond: Option<Node<'a>>,
        arms: Vec<Node<'a>>,
        exhaustive: bool,
    },
    /// 循环
    Loop {
        cond: Option<Node<'a>>,
        body: Node<'a>,
    },
    /// 受保护块(try/except):异常可能在 body 的**任意一点**抛出。
    Guarded {
        body: Node<'a>,
        handlers: Vec<Node<'a>>,
    },
    /// 提前返回
    Return,
    /// break / continue
    Jump { kind: JumpKind, labeled: bool },
    /// 本实现不建模 ⇒ 整份作废,值是构造名
    Unsupported(&'static str),
}

pub enum JumpKind {
    Break,
    Continue,
}

/// 语言层吐出来的 CFG 轮廓(不带函数 id —— 那是调用方的事)。
pub struct CfgOutline {
    pub blocks: Vec<crate::model::CfgBlock>,
    pub edges: Vec<(usize, usize)>,
    pub entry: usize,
    pub exit: usize,
    pub unsupported: Option<String>,
}

/// 分类实现登记表。**语言在此"点亮"**:F11 只有 Rust,F12–F14 增加对应 arm。
/// 未登记的语言 → None:该语言文件会被扫描到但产不出符号/边(不报错、不影响其它语言)。
pub fn spec_for(lang: Lang) -> Option<&'static dyn LangSupport> {
    match lang {
        Lang::Rust => Some(&rust::RUST),
        Lang::Python => Some(&python::PYTHON),
        Lang::JavaScript => Some(&javascript::JAVASCRIPT),
        Lang::TypeScript => Some(&typescript::TYPESCRIPT),
        Lang::Java => Some(&java::JAVA),
        Lang::Kotlin => Some(&kotlin::KOTLIN),
        Lang::C => Some(&c::CLANG),
        Lang::Cpp => Some(&cpp::CPP),
        Lang::CSharp => Some(&csharp::CSHARP),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    const ALL: [Lang; 9] = [
        Lang::Rust,
        Lang::Python,
        Lang::JavaScript,
        Lang::TypeScript,
        Lang::Java,
        Lang::Kotlin,
        Lang::C,
        Lang::Cpp,
        Lang::CSharp,
    ];

    #[test]
    fn all_grammars_load_at_runtime() {
        // 8 门 grammar 全部与 tree-sitter 0.25 ABI 兼容、可 set_language(F12–F14 前置)
        for lang in ALL {
            let mut p = Parser::new();
            assert!(
                p.set_language(&lang.ts_language()).is_ok(),
                "{lang:?} grammar 加载失败(ABI 不兼容?)"
            );
        }
    }

    #[test]
    fn from_path_classifies_extensions() {
        assert_eq!(Lang::from_path("src/a.rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_path("x.py"), Some(Lang::Python));
        assert_eq!(Lang::from_path("x.mjs"), Some(Lang::JavaScript));
        assert_eq!(Lang::from_path("x.ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_path("x.tsx"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_path("x.java"), Some(Lang::Java));
        assert_eq!(Lang::from_path("x.kt"), Some(Lang::Kotlin));
        assert_eq!(Lang::from_path("x.c"), Some(Lang::C));
        assert_eq!(Lang::from_path("x.cpp"), Some(Lang::Cpp));
        assert_eq!(Lang::from_path("x.h"), Some(Lang::Cpp)); // 头文件归 C++
        assert_eq!(Lang::from_path("x.cs"), Some(Lang::CSharp));
        assert_eq!(Lang::from_path("README.md"), None);
        assert_eq!(Lang::from_path("Makefile"), None);
    }

    /// 树里第一个「是闭包」的节点的形参表。没有闭包 → None。
    fn first_closure_params(
        spec: &dyn LangSupport,
        n: tree_sitter::Node,
        src: &[u8],
    ) -> Option<Vec<Option<String>>> {
        if let Some(ps) = spec.closure_params(n, src) {
            return Some(ps);
        }
        let mut c = n.walk();
        let kids: Vec<_> = n.children(&mut c).collect();
        kids.into_iter()
            .find_map(|ch| first_closure_params(spec, ch, src))
    }

    /// 扇出守卫:**每门**闭包的形参都要取得出来。
    ///
    /// 🔴 没有逐门点名的话,某门悄悄返回空形参表不会有人发现 —— 而空形参表
    /// 意味着「遮蔽做不成」,于是外层同名形参会被记成**确定**流下去(假事实)。
    /// Kotlin 的 `lambda_literal` 没有字段名、Java/C#/C++ 的形参第一个命名子是**类型**,
    /// 这三种形都只会静悄悄取到空 —— 正是要点名的原因。
    #[test]
    fn all_nine_langs_read_closure_params() {
        let cases: [(&str, &str, &[&str]); 9] = [
            ("x.rs", "fn t(){ let f = |a: u32, b| g(a); }", &["a", "b"]),
            ("x.py", "def t():\n  f = lambda a, b: g(a)\n", &["a", "b"]),
            // ⚠ JS/TS 写成**顶层**闭包:这两门的 `function_declaration` 本身也遮蔽、
            //   也会被 `closure_params` 认下 —— 包一层函数的话先撞上的是外层那个。
            ("x.js", "const f = (a,b) => g(a);", &["a", "b"]),
            (
                "x.ts",
                "const f = (a: number, b: number) => g(a);",
                &["a", "b"],
            ),
            (
                "X.java",
                "class X { void t(){ R f = (int a, int b) -> g(a); } }",
                &["a", "b"],
            ),
            (
                "X.kt",
                "fun t(){ val f = { a: Int, b: Int -> g(a) } }",
                &["a", "b"],
            ),
            (
                "x.cpp",
                "void t(){ auto f = [&](int a, int b){ return g(a); }; }",
                &["a", "b"],
            ),
            (
                "X.cs",
                "class X { void T(){ var f = (int a, int b) => G(a); } }",
                &["a", "b"],
            ),
            // C 没有闭包这一形 —— 该**答不出**,不是答空
            ("x.c", "void t(void){ int a = 1; g(a); }", &[]),
        ];
        for (path, code, want) in cases {
            let lang = Lang::from_path(path).unwrap();
            let spec = spec_for(lang).unwrap();
            let mut p = Parser::new();
            p.set_language(&lang.ts_language()).unwrap();
            let tree = p.parse(code, None).unwrap();
            let got = first_closure_params(spec, tree.root_node(), code.as_bytes());
            if want.is_empty() {
                assert!(
                    got.is_none(),
                    "{path}: 这门没有闭包,不该认出一个来({got:?})"
                );
                continue;
            }
            let got = got.unwrap_or_else(|| panic!("{path}: 找不到闭包节点(closure_params 没接?)"));
            let got: Vec<&str> = got.iter().map(|o| o.as_deref().unwrap_or("<?>")).collect();
            assert_eq!(got, want, "{path}: 闭包形参取错了");
        }
    }

    /// JS 的单参简写 `a => ..` 没有形参容器 —— 形参**直接**挂在 `parameter` 字段上。
    /// 这是箭头函数最常见的写法,取不到就等于这一整类都不遮蔽。
    #[test]
    fn a_one_param_arrow_still_yields_its_param() {
        let lang = Lang::from_path("x.js").unwrap();
        let spec = spec_for(lang).unwrap();
        let code = "const h = a => g(a);";
        let mut p = Parser::new();
        p.set_language(&lang.ts_language()).unwrap();
        let tree = p.parse(code, None).unwrap();
        assert_eq!(
            first_closure_params(spec, tree.root_node(), code.as_bytes()),
            Some(vec![Some("a".to_string())])
        );
    }

    /// 扇出守卫:**每门**形参的「名字」与「类型」都要取得出来,而且**按位对齐**。
    ///
    /// 🔴 名字与类型错位是**静默的假事实**(第 0 参的名字配上第 1 参的类型 ⇒ 接收者类型解错
    /// ⇒ 调用边连错)。两者都从 `params_container` 派生就是为了不可能错位,
    /// 这条测试守的是「每门真的接了那个方法」。
    /// ⚠ 动态语言的类型位允许是 `None`(没写就是没写),名字位不许。
    #[test]
    fn all_nine_langs_read_param_names_and_types() {
        // (路径, 源码, 期望名字, 期望类型;`None` = 这门这里读不到类型)
        type Case = (
            &'static str,
            &'static str,
            &'static [&'static str],
            &'static [Option<&'static str>],
        );
        let cases: [Case; 9] = [
            (
                "x.rs",
                "fn t(a: u32, b: String) {}",
                &["a", "b"],
                &[Some("u32"), Some("String")],
            ),
            // Python:标注可选 —— 这里故意一个有一个没有
            (
                "x.py",
                "def t(a: int, b):\n  pass\n",
                &["a", "b"],
                &[Some("int"), None],
            ),
            ("x.js", "function t(a, b) {}", &["a", "b"], &[None, None]),
            (
                "x.ts",
                "function t(a: number, b: string) {}",
                &["a", "b"],
                &[Some("number"), Some("string")],
            ),
            (
                "X.java",
                "class X { void t(int a, String b){} }",
                &["a", "b"],
                &[Some("int"), Some("String")],
            ),
            (
                "X.kt",
                "fun t(a: Int, b: String){}",
                &["a", "b"],
                &[Some("Int"), Some("String")],
            ),
            (
                "x.c",
                "void t(int a, char b){}",
                &["a", "b"],
                &[Some("int"), Some("char")],
            ),
            (
                "x.cpp",
                "void t(int a, char b){}",
                &["a", "b"],
                &[Some("int"), Some("char")],
            ),
            (
                "X.cs",
                "class X { void T(int a, string b){} }",
                &["a", "b"],
                &[Some("int"), Some("string")],
            ),
        ];
        for (path, code, want_names, want_types) in cases {
            let lang = Lang::from_path(path).unwrap();
            let spec = spec_for(lang).unwrap();
            let mut p = Parser::new();
            p.set_language(&lang.ts_language()).unwrap();
            let tree = p.parse(code, None).unwrap();
            let (names, types) = first_params(spec, tree.root_node(), code.as_bytes())
                .unwrap_or_else(|| panic!("{path}: 找不到形参容器(params_container 没接?)"));
            let got_names: Vec<&str> = names
                .iter()
                .map(|o| o.as_deref().unwrap_or("<?>"))
                .collect();
            assert_eq!(got_names, want_names, "{path}: 形参名取错了");
            let got_types: Vec<Option<&str>> = types.iter().map(|o| o.as_deref()).collect();
            assert_eq!(
                got_types, want_types,
                "{path}: 形参类型取错了(与名字必须按位对齐)"
            );
            assert_eq!(
                names.len(),
                types.len(),
                "{path}: 名字与类型的位数必须一致 —— 不一致就是错位"
            );
        }
    }

    /// 树里第一个有形参容器的函数的 (名字表, 类型表)。
    #[allow(clippy::type_complexity)]
    fn first_params(
        spec: &dyn LangSupport,
        n: tree_sitter::Node,
        src: &[u8],
    ) -> Option<(Vec<Option<String>>, Vec<Option<String>>)> {
        if spec.params_container(n, src).is_some() {
            return Some((spec.param_names(n, src), spec.param_types(n, src)));
        }
        let mut c = n.walk();
        let kids: Vec<_> = n.children(&mut c).collect();
        kids.into_iter().find_map(|ch| first_params(spec, ch, src))
    }

    /// 扇出守卫:有字段声明的**七门**都要读得出 (类型名, 字段名, 字段类型)。
    ///
    /// 🔴 读不出等于 `self.idx.m()` 这类调用定不下接收者类型 —— 而那是本仓
    /// 歧义调用点里最大的一块。C# 的类型挂在祖父上、Kotlin 全靠位置、
    /// Java/C/C++ 的名字藏在 declarator 里:三种形都只会**静悄悄读出空表**。
    /// ⚠ Python / JS 没有可靠的字段声明形 ⇒ 该答 `None`(「没这一说」),不是空表。
    #[test]
    fn seven_langs_read_field_types_and_two_decline() {
        type Case = (
            &'static str,
            &'static str,
            Option<(&'static str, &'static [(&'static str, &'static str)])>,
        );
        let cases: [Case; 9] = [
            (
                "x.rs",
                "pub struct E { idx: Index, pub n: u32 }",
                Some(("E", &[("idx", "Index"), ("n", "u32")])),
            ),
            (
                "x.ts",
                "class E { private idx: Index; n: number = 1; }",
                Some(("E", &[("idx", "Index"), ("n", "number")])),
            ),
            (
                "X.java",
                "class E { private Index idx; int n; }",
                Some(("E", &[("idx", "Index"), ("n", "int")])),
            ),
            (
                "X.kt",
                "class E { private val idx: Index = mk(); var n: Int = 1 }",
                Some(("E", &[("idx", "Index"), ("n", "Int")])),
            ),
            (
                "x.c",
                "struct E { struct Index *idx; int n; };",
                Some(("E", &[("idx", "Index"), ("n", "int")])),
            ),
            (
                "x.cpp",
                "class E { Index idx; int n; };",
                Some(("E", &[("idx", "Index"), ("n", "int")])),
            ),
            (
                "X.cs",
                "class E { private Index idx; public int N { get; set; } }",
                Some(("E", &[("N", "int"), ("idx", "Index")])),
            ),
            // 动态语言:没有字段声明这一说 —— 该**答不出**,不是答空
            ("x.py", "class E:\n  def m(self): pass\n", None),
            ("x.js", "class E { m(){} }", None),
        ];
        for (path, code, want) in cases {
            let lang = Lang::from_path(path).unwrap();
            let spec = spec_for(lang).unwrap();
            let mut p = Parser::new();
            p.set_language(&lang.ts_language()).unwrap();
            let tree = p.parse(code, None).unwrap();
            let got = first_type_fields(spec, tree.root_node(), code.as_bytes());
            let Some((wname, wfields)) = want else {
                assert!(
                    got.is_none(),
                    "{path}: 这门没有字段声明,不该读出一张表({got:?})"
                );
                continue;
            };
            let (name, mut fields) =
                got.unwrap_or_else(|| panic!("{path}: 读不出字段表(type_fields 没接?)"));
            assert_eq!(name, wname, "{path}: 类型名取错了");
            fields.sort();
            let got_f: Vec<(&str, &str)> = fields
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let mut want_f = wfields.to_vec();
            want_f.sort();
            assert_eq!(got_f, want_f, "{path}: 字段表取错了");
        }
    }

    /// 树里第一个「是类型声明」的节点的字段表。
    #[allow(clippy::type_complexity)]
    fn first_type_fields(
        spec: &dyn LangSupport,
        n: tree_sitter::Node,
        src: &[u8],
    ) -> Option<(String, Vec<(String, String)>)> {
        if let Some(t) = spec.type_fields(n, src) {
            return Some(t);
        }
        let mut c = n.walk();
        let kids: Vec<_> = n.children(&mut c).collect();
        kids.into_iter()
            .find_map(|ch| first_type_fields(spec, ch, src))
    }

    #[test]
    fn all_nine_langs_wired() {
        // F14 收官:9 门全部接入分类(spec_for 皆 Some)。
        for lang in ALL {
            assert!(spec_for(lang).is_some(), "{lang:?} 应已接入 LangSupport");
        }
    }
}
