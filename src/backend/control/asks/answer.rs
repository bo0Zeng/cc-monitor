//! `needs-answer` 的处理器与生产那一份本体：挡着终端的那一条经读屏驱动按（[`crate::control::screen_drive::press_box`]）。
//! 只有这一份点名读屏驱动：登记表那一份（`control::asks`）同住着帧面硬臂 `needs-ask`，那一条不碰终端。

use super::{answer_needs_through, AnswerErr, AnswerReq, Answered, NeedsAnswerCmd};
use crate::common::said::Said;
use copy_core::copy_text;

impl crate::wire::typed::Handle for NeedsAnswerCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: AnswerReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<Answered, AnswerErr>> + Send + 'static {
        async move {
            tokio::task::spawn_blocking(move || answer_needs(req))
                .await
                .unwrap_or_else(|_| Err(Said::new("unsure", copy_text("beAsk.screen.unsure", &[]))))
                .map_err(crate::wire::typed::of_fail_with(AnswerErr::NotFound))
        }
    }
}

/// `needs-answer` 的本体（生产：挡着终端的那一条经读屏驱动按）。
pub fn answer_needs(req: AnswerReq) -> Result<Answered, Said> {
    answer_needs_through(req, crate::control::screen_drive::press_box)
}
