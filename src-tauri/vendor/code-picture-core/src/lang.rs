//! 语言支持层(F11):把"什么节点是函数/调用/类型限定"从 `symbols.rs`/`graph.rs` 的
//! 递归驱动里剥离出来,按 `Lang` 分发。递归驱动语言无关;每门语言只实现 `LangSupport`。
//!
//! F11 一次加全 8 门 grammar 依赖并在 `Lang::ts_language` 全部接线;但**分类**(`spec_for`)
//! 本轮只提供 Rust,其余语言的 `LangSupport` 实现留 F12–F14(接入即"点亮")。

use crate::model::{Lang, SymKind};
use tree_sitter::{Language, Node};

mod c;
mod cpp;
mod csharp;
mod java;
mod javascript;
mod kotlin;
mod python;
mod rust;
mod typescript;

/// 取节点某命名字段的文本(各语言 impl 共用)。字段缺失 / 非 UTF-8 → None。
pub(crate) fn field_text(node: Node, field: &str, src: &[u8]) -> Option<String> {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(src).ok())
        .map(String::from)
}

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

    #[test]
    fn all_nine_langs_wired() {
        // F14 收官:9 门全部接入分类(spec_for 皆 Some)。
        for lang in ALL {
            assert!(spec_for(lang).is_some(), "{lang:?} 应已接入 LangSupport");
        }
    }
}
