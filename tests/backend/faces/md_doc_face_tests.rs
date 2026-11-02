//! `md-doc` 的判据：两种读法各出哪棵树；计划格正文里提到的格解成引用节点；入参的形（`as` 两个词 · 多余的键拒）。夹具是造的。

use super::*;
use serde_json::json;

fn req(v: serde_json::Value) -> Result<MdDocReq, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn prose_is_the_same_tree_as_assistant_text() {
    let src = "# 想\n\n\\(a+b\\) 与 \\[x\\]\n上面\n$$\ny\n$$";
    let r = req(json!({ "text": src, "as": "prose", "refs": [] })).unwrap();
    assert_eq!(doc_of(&r), md_core::parse(src));
}

#[test]
fn tool_result_strips_line_numbers() {
    let src = "1\t# 标题\n2\t- 一\n3\t- 二";
    let r = req(json!({ "text": src, "as": "tool_result", "refs": [] })).unwrap();
    assert_eq!(doc_of(&r), md_core::parse("# 标题\n- 一\n- 二"));
    assert_ne!(doc_of(&r), md_core::parse(src));
}

#[test]
fn plan_body_refs_become_ref_nodes_with_the_label_left_to_the_view() {
    let src = "见 A1-2 的写法";
    let start = "见 ".encode_utf16().count() as u32;
    let r = req(json!({ "text": src, "as": "prose", "refs": [{"start": start, "end": start + 4, "ids": ["A1-2"]}] })).unwrap();
    assert_eq!(
        doc_of(&r).blocks,
        vec![md_core::MdBlock::P {
            k: vec![
                md_core::Inl::S("见 ".into()),
                md_core::Inl::N(md_core::InlNode::Ref {
                    x: "A1-2".into(),
                    ids: vec!["A1-2".into()],
                    label: String::new(),
                }),
                md_core::Inl::S(" 的写法".into()),
            ]
        }]
    );
}

#[test]
fn request_shape_is_closed() {
    assert!(req(json!({"text": "x", "as": "html", "refs": []})).is_err());
    assert!(req(json!({"text": "x", "as": "prose", "refs": [], "more": 1})).is_err());
    assert!(req(json!({"text": "x", "refs": []})).is_err());
    assert!(req(json!({"text": "x", "as": "prose"})).is_err());
}
