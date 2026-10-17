//! 帧面 `session-restart` 的宿主壳：把本体（`control/session_restart.rs`）要的几样拼成生产那一份。
//!
//! 每一步在自己的阻塞线程上拼一份与批量停 / 起同一份的事实（`session_batch_face`）；停旧 ＋ 起新那一步拿退出排空的票。
//! 两种等待住观测层（`observe/one_wait.rs`）：等压缩摘要 · 等会话由新进程报出。

use crate::control::session_batch::Deps;
use crate::control::session_restart::{self as restart, Host, Wait};
use crate::observe::one_wait::{self, Armed};
use std::future::Future;

struct Prod {
    client: Option<String>,
    agent: String,
    files: crate::faces::launch_account_face::Files,
}

impl Wait for Armed {
    fn within(self, ms: u64) -> impl Future<Output = bool> + Send {
        Armed::within(self, ms)
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
    type Ears = Armed;

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

    fn watch_compact(&self, sid: &str) -> impl Future<Output = Result<Armed, String>> + Send {
        let sid = sid.to_string();
        joined(tokio::task::spawn_blocking(move || {
            one_wait::compact_summary(&crate::history::listing::agent_home(), &sid)
        }))
    }

    fn watch_arrival(&self, sid: &str) -> impl Future<Output = Result<Armed, String>> + Send {
        let sid = sid.to_string();
        joined(tokio::task::spawn_blocking(move || {
            one_wait::session_arrival(&crate::history::listing::agent_home(), &sid)
        }))
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
) -> Result<restart::Restarted, restart::Fault> {
    let client = crate::control::gate::requester(req.client.as_deref())
        .map_err(|(c, m)| (c.to_string(), m, None))?;
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
