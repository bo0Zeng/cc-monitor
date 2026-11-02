//! 帧面 `md-doc`：一段 Markdown 原文 ⇒ 块树（[`md_core`] 解析，两端只照树排版）。
//! 给界面手上本来就有原文、点开才要排的那几处：思考展开 · 工具结果切到 Markdown。纯函数，不碰盘。

use crate::wire::typed::{codes, Command, Cx, Handle};
use cmd_wire::md::Doc;

/// 这段原文按哪一种读。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum MdAs {
    /// 照原文解析（思考）。
    Prose,
    /// 工具结果：Read / Grep 那种行号前缀先剥掉再解析（[`md_core::tool_result`]）。
    ToolResult,
}

/// `md-doc` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct MdDocReq {
    pub(crate) text: String,
    #[serde(rename = "as")]
    pub(crate) of: MdAs,
}

/// 本体：按 `of` 选解析法。
pub(crate) fn doc_of(req: &MdDocReq) -> Doc {
    match req.of {
        MdAs::Prose => md_core::parse(&req.text),
        MdAs::ToolResult => md_core::tool_result(&req.text),
    }
}

codes! {
    /// `md-doc` 自己的码（只有入参不对那一种）。
    pub(crate) enum MdDocErr {}
}

/// `md-doc`：一段原文解析成块树。
pub(crate) struct MdDocCmd;

impl Command for MdDocCmd {
    const NAME: &'static str = "md-doc";
    type Req = MdDocReq;
    type Reply = Doc;
    type Err = MdDocErr;
}

impl Handle for MdDocCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: MdDocReq,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Doc, MdDocErr>> + Send + 'static {
        async move { Ok(doc_of(&req)) }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/md_doc_face_tests.rs"]
mod tests;
