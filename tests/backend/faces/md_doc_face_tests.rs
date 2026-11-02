//! `md-doc` 的判据：两种读法各出哪棵树；入参的形（`as` 两个词 · 多余的键拒）。夹具是造的。

use super::*;

fn req(json: &str) -> Result<MdDocReq, serde_json::Error> {
    serde_json::from_str(json)
}

#[test]
fn prose_is_the_same_tree_as_assistant_text() {
    let src = "# 想\n\n\\(a+b\\) 与 \\[x\\]\n上面\n$$\ny\n$$";
    let r = req(&serde_json::json!({ "text": src, "as": "prose" }).to_string()).unwrap();
    assert_eq!(doc_of(&r), md_core::parse(src));
}

#[test]
fn tool_result_strips_line_numbers() {
    let src = "1\t# 标题\n2\t- 一\n3\t- 二";
    let r = req(&serde_json::json!({ "text": src, "as": "tool_result" }).to_string()).unwrap();
    assert_eq!(doc_of(&r), md_core::parse("# 标题\n- 一\n- 二"));
    assert_ne!(doc_of(&r), md_core::parse(src));
}

#[test]
fn request_shape_is_closed() {
    assert!(req(r#"{"text":"x","as":"html"}"#).is_err());
    assert!(req(r#"{"text":"x","as":"prose","more":1}"#).is_err());
    assert!(req(r#"{"text":"x"}"#).is_err());
}
