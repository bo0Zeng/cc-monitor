//! 帧面 `prose-hl`（typed）：拉一段正文的代码高亮。树里的代码块不带高亮，两端建一段时把那一段的代码块（先序）交上来，
//! 这里照 `md_core::Highlighter` 算、按内容缓存（同样的语言 ＋ 原文不算第二遍）。
//!
//! 高亮器是进程里常驻的一份（语法表载一次几十毫秒；缓存跨连接、跨会话共用），md-core 自己不持状态。

use crate::wire::typed::{self, Command};
use cmd_wire::md::{Doc, HlCode, MdBlock, ProseHlReq, SegHl};
use relay_route_core::{Cap, Over};
use std::sync::{LazyLock, Mutex};

/// 一次上行的代码原文上限：一段至多两万字（`md_core::SEG_BYTES`），留足余量；超了 ⇒ 一个字节不算、回 `too_large`。
pub(crate) const PROSE_HL_REQ_CAP: Cap = Cap {
    bytes: 256 << 10,
    over: Over::Reject,
};

/// 高亮缓存的字节预算（每条按 键长 ＋ `hl` 长 × 4 计）：满了先淘汰最久没用的那条，下次再算一遍；不限任何上行或下行的量。
const HL_CACHE_BYTES: usize = 8 << 20;

/// 进程里那一份高亮器。
static HIGHLIGHTER: LazyLock<Mutex<md_core::Highlighter>> =
    LazyLock::new(|| Mutex::new(md_core::Highlighter::new(HL_CACHE_BYTES)));

typed::codes! {
    /// `prose-hl` 的码。
    pub enum ProseHlErr {
        /// 那一段的代码原文超过 [`PROSE_HL_REQ_CAP`]。
        TooLarge = "too_large",
    }
}

/// `prose-hl`：一段的代码块 ⇒ 各自的高亮。
pub struct ProseHlCmd;

impl Command for ProseHlCmd {
    const NAME: &'static str = "prose-hl";
    type Req = ProseHlReq;
    type Reply = SegHl;
    type Err = ProseHlErr;
}

impl typed::Handle for ProseHlCmd {
    type Needs = typed::needs::Nothing;
    async fn run(req: ProseHlReq, _cx: typed::Cx<Self::Needs>) -> Result<SegHl, ProseHlErr> {
        // 锁坏了（别处算到一半 panic）照样接着用：缓存里只有算完的条目。
        let mut hl = HIGHLIGHTER.lock().unwrap_or_else(|e| e.into_inner());
        seg_hl(&mut hl, req)
    }
}

/// [`ProseHlCmd`] 的本体，高亮器是参数（判据另建一份、看计数）。
pub(crate) fn seg_hl(hl: &mut md_core::Highlighter, req: ProseHlReq) -> Result<SegHl, ProseHlErr> {
    let bytes: u64 = req
        .codes
        .iter()
        .map(|c| (c.x.len() + c.lang.as_ref().map_or(0, String::len)) as u64)
        .sum();
    if bytes > PROSE_HL_REQ_CAP.bytes {
        return Err(ProseHlErr::TooLarge(format!(
            "prose-hl: {bytes} bytes of code over the {} byte cap",
            PROSE_HL_REQ_CAP.bytes
        )));
    }
    let doc = Doc {
        blocks: req
            .codes
            .into_iter()
            .map(|HlCode { lang, x }| MdBlock::Code {
                lang,
                x,
                cont: false,
            })
            .collect(),
        segs: Vec::new(),
    };
    Ok(SegHl {
        hl: hl.seg(&doc, 0),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/prose_hl_face_tests.rs"]
mod tests;
