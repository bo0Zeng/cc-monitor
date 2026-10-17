//! 帧面 `session-arrive` 的宿主壳：认报到交观测层（`observe::arrivals`），抓那一屏交 `terminals`（阻塞线程上）。

use crate::control::session_arrive::{self as arrive, Host, Screen};
use cmd_wire::session::{ArriveAsk, ArriveBy, ArriveEnd};
use std::future::Future;

struct Prod;

impl Host for Prod {
    fn arrival(
        &self,
        by: &ArriveBy,
        since: u64,
        ms: u64,
    ) -> impl Future<Output = Option<String>> + Send {
        let heard = crate::observe::arrivals::expect(by.clone(), since);
        crate::observe::one_wait::bounded(heard, ms)
    }

    fn screen(&self, session: &str) -> impl Future<Output = Screen> + Send {
        let session = session.to_string();
        async move {
            tokio::task::spawn_blocking(move || crate::control::terminals::screen_of(&session))
                .await
                .unwrap_or_else(|e| Screen::Unread(e.to_string()))
        }
    }
}

crate::wire::typed::codes! {
    /// `session-arrive` 的码：只有形状那一种（认的那一格不像样）。
    pub(crate) enum ArriveErr {}
}

/// `session-arrive`：等起好的会话报到，期限 ＝ 请求信封的期限。
pub(crate) struct SessionArrive;

impl crate::wire::typed::Command for SessionArrive {
    const NAME: &'static str = "session-arrive";
    type Req = ArriveAsk;
    type Reply = ArriveEnd;
    type Err = ArriveErr;
}

impl crate::wire::typed::Handle for SessionArrive {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: ArriveAsk,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<ArriveEnd, ArriveErr> {
        let ms = cx.until.map_or(0, crate::platform::child::Until::left_ms);
        arrive::run(req, ms, &Prod)
            .await
            .map_err(ArriveErr::BadArgs)
    }
}
