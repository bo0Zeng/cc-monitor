//! Java 的 `LangSupport`(F13)。Java 无自由函数——方法/构造器恒在类型内,由祖先限定
//! 成 Method(`file#Type::name`)。`class/interface/enum/record` 设限定;
//! 构造器 name=类名(`new B()` 可连到 `B::B`);`method_invocation`/`object_creation_expression` 为调用。

use super::{field_text, LangSupport, SymbolDef};
use tree_sitter::Node;

pub(crate) struct Java;
pub(crate) static JAVA: Java = Java;

/// 该节点是否直接含 `class_body` 子节点(用于识别匿名类 `new T(){…}`)。
fn has_class_body(node: Node) -> bool {
    (0..node.child_count()).any(|i| node.child(i).map(|c| c.kind()) == Some("class_body"))
}

impl LangSupport for Java {
    fn qualifier_of(&self, node: Node, src: &[u8]) -> Option<String> {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => field_text(node, "name", src),
            // 匿名类 `new Runnable(){ … }`(object_creation_expression 带 class_body):
            // 其方法归到被实现的类型,否则会泄漏成假的顶层 Function 污染符号表
            "object_creation_expression" if has_class_body(node) => field_text(node, "type", src),
            _ => None,
        }
    }

    fn symbol_at(&self, node: Node, src: &[u8]) -> Option<SymbolDef> {
        // 方法 + 构造器都是可调用定义;恒在类型内 → 驱动按祖先限定成 Method
        if matches!(
            node.kind(),
            "method_declaration" | "constructor_declaration"
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
            "method_invocation" => {
                let name = field_text(node, "name", src)?;
                // 有接收者(obj.m() / this.n())→ 类型未知走 DynamicGuess;
                // 无接收者(隐式 this 的 m())→ 按名解析(单候选可 Exact)
                let is_method = node.child_by_field_name("object").is_some();
                Some((name, None, is_method))
            }
            // new B() → 连到构造器 B::B(type 字段取类名;泛型 new B<T>() 的 T 不剥离,记债)
            "object_creation_expression" => field_text(node, "type", src).map(|t| (t, None, false)),
            _ => None,
        }
    }
}
