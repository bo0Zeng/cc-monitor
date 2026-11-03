//! 帧面 `session-restart` 的宿主壳：把本体（`control/session_restart.rs`）要的几样拼成生产那一份。
//!
//! 每一步在自己的阻塞线程上拼一份与批量停 / 起同一份的事实（`session_batch_face`）；停旧 ＋ 起新那一步拿退出排空的票。
//! 等压缩摘要住观测层（`observe/one_wait.rs`），等报出交报到那一本（`observe/arrivals.rs`，同 `session-arrive`）。

use crate::control::session_batch::Deps;
use crate::control::session_restart::{self as restart, Host, Wait};
use crate::observe::one_wait::{self, Armed};
use std::future::Future;

struct Prod {
    client: Option<String>,
    agent: crate::agents::AgentKind,
    files: crate::faces::launch_account_face::Files,
}

/// 等压缩摘要那一份（观测面的 `Armed`）交给本体：本体的 `Wait` 与观测面的类型各住一个包，接在这一层。
struct Ears(Armed);

impl Wait for Ears {
    fn within(self, ms: u64) -> impl Future<Output = bool> + Send {
        self.0.within(ms)
    }
}

/// 阻塞线程上那一步的结局（它 panic ⇒ 照样 panic，交给分派那一层回 `handler_panicked`）。
async fn joined<T>(h: tokio::task::JoinHandle<T>) -> T {
    match h.await {
        Ok(t) => t,
        Err(e) => std::panic::resume_unwind(e.into_panic()),
    }
}

impl Host for Prod {
    type Ears = Ears;

    fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> impl Future<Output = T> + Send {
        let (client, agent, files) = (self.client.clone(), self.agent.clone(), self.files);
        joined(tokio::task::spawn_blocking(move || {
            crate::faces::session_batch_face::with_deps_as(
                client.as_deref(),
                &agent,
                Some(files),
                f,
            )
        }))
    }

    fn critical<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> impl Future<Output = Option<T>> + Send {
        let ticket = crate::wire::drain::DRAIN.enter("session-restart".to_string());
        let (client, agent, files) = (self.client.clone(), self.agent.clone(), self.files);
        async move {
            let ticket = ticket?;
            // 外层被撤只丢掉这一头的等待：阻塞线程照样做完、票跟着它落。
            Some(
                joined(tokio::task::spawn_blocking(move || {
                    let _ticket = ticket;
                    crate::faces::session_batch_face::with_deps_as(
                        client.as_deref(),
                        &agent,
                        Some(files),
                        f,
                    )
                }))
                .await,
            )
        }
    }

    fn compact_request(&self, agent: &str) -> Option<&'static str> {
        crate::agents::compact_request_of(agent)
    }

    fn watch_compact(&self, sid: &str) -> impl Future<Output = Result<Ears, String>> + Send {
        let sid = sid.to_string();
        let armed = joined(tokio::task::spawn_blocking(move || {
            one_wait::compact_summary(&crate::history::listing::agent_home(), &sid)
        }));
        async move { armed.await.map(Ears) }
    }

    fn arrival(&self, sid: &str, since: u64, ms: u64) -> impl Future<Output = bool> + Send {
        // 等的是这一趟自己起的那条（sid 来自已解好的入参）；万一不是会话 id ⇒ 不等，照没等到。
        let heard = sid.parse().ok().map(|sid| {
            crate::observe::arrivals::expect(cmd_wire::session::ArriveBy::Sid { sid }, since)
        });
        async move {
            match heard {
                Some(heard) => one_wait::bounded(heard, ms).await.is_some(),
                None => false,
            }
        }
    }
}

/// `session-restart`：换号重启。可撤档（步与步之间撤单生效；停旧 ＋ 起新不可分，开跑之后撤也做完）。
pub(crate) struct SessionRestart;

impl crate::wire::typed::Command for SessionRestart {
    const NAME: &'static str = "session-restart";
    type Req = restart::RestartReq;
    type Reply = restart::Restarted;
    type Err = restart::RestartErr;
}

impl crate::wire::typed::Handle for SessionRestart {
    type Needs = crate::wire::typed::needs::Files;
    async fn run(
        req: restart::RestartReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<restart::Restarted, restart::RestartErr> {
        answer(req, cx.until, cx.needs)
            .await
            .map_err(restart::RestartErr)
    }
}

/// `until`：发起方的截止时刻（分派那一层给）。
pub(crate) async fn answer(
    req: restart::RestartReq,
    until: Option<crate::platform::child::Until>,
    files: crate::faces::launch_account_face::Files,
) -> Result<restart::Restarted, crate::common::said::Said> {
    let client = crate::control::gate::requester(req.client.as_deref())?;
    let agent = req.agent.clone();
    restart::run(
        req,
        until,
        Prod {
            client,
            agent,
            files,
        },
    )
    .await
}
