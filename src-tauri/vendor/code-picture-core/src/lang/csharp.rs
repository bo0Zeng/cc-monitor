//! C# 的 `LangSupport`(F14)。方法恒在 class/struct/interface/record 内 → 全 Method
//! (`file#Type::name`);namespace **不**设限定(无自由方法,类已给限定)。构造器 name=类名
//! (`new D()`→`D::D`)。调用:`invocation_expression`(identifier / `member_access_expression`.name)
//! + `object_creation_expression`(`new`)。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct CSharp;
pub(crate) static CSHARP: CSharp = CSharp;

impl LangSupport for CSharp {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        if matches!(
            node.kind(),
            "class_declaration"
                | "struct_declaration"
                | "interface_declaration"
                | "record_declaration"
        ) {
            field_text(node, "name", src)
        } else {
            None
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        if matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration" | "local_function_statement"
        ) {
            let name = field_text(node, "name", src)?;
            Some(SymbolDef {
                name,
                qualifier: None,
                kind: None,
            })
        } else {
            None
        }
    }

    fn call_of(&self, node: Node, src: &[u8]) -> Option<(String, Option<String>, bool)> {
        match node.kind() {
            "invocation_expression" => {
                let func = node.child_by_field_name("function")?;
                match func.kind() {
                    "identifier" => func
                        .utf8_text(src)
                        .ok()
                        .map(|s| (s.to_string(), None, false)),
                    // M<int>() 泛型调用:剥 type-args 取裸名
                    "generic_name" => generic_bare(func, src).map(|n| (n, None, false)),
                    // obj.M() / this.n() / obj.M<int>() → 方法调用,接收者类型未知
                    "member_access_expression" => {
                        let name_node = func.child_by_field_name("name")?;
                        bare_name(name_node, src).map(|s| (s, None, true))
                    }
                    _ => None,
                }
            }
            "object_creation_expression" => field_text(node, "type", src).map(|t| (t, None, false)),
            _ => None,
        }
    }
}

/// `generic_name`(`M<int>`)的裸名 = 其首个 `identifier` 子节点。
fn generic_bare(node: Node, src: &[u8]) -> Option<String> {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .find(|c| c.kind() == "identifier")
        .and_then(|c| c.utf8_text(src).ok())
        .map(String::from)
}

/// 成员名节点:`identifier` 直接取文本;`generic_name`(`M<int>`)剥 type-args。
fn bare_name(node: Node, src: &[u8]) -> Option<String> {
    if node.kind() == "generic_name" {
        generic_bare(node, src)
    } else {
        node.utf8_text(src).ok().map(String::from)
    }
}
