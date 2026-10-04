//! 帧面 `session-restart` 的宿主壳：把本体（`control/session_restart.rs`）要的几样拼成生产那一份。
//!
//! 每一步在自己的阻塞线程上拼一份与批量停 / 起同一份的事实（`session_batch_face`）；停旧 ＋ 起新那一步拿退出排空的票。
//! 两种等待住观测层（`observe/one_wait.rs`）：等压缩摘要 · 等会话由新进程报出。

use crate::control::session_batch::Deps;
use crate::control::session_restart::{self as restart, Host, Wait};
use crate::observe::one_wait::{self, Armed};
use serde_json::Value;
use std::future::Future;

struct Prod {
    client: Option<String>,
    agent: String,
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
        let (client, agent) = (self.client.clone(), self.agent.clone());
        joined(tokio::task::spawn_blocking(move || {
            crate::faces::session_batch_face::with_deps_as(client.as_deref(), &agent, f)
        }))
    }

    fn critical<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> impl Future<Output = Option<T>> + Send {
        let ticket = crate::stream::inbound::DRAIN.enter("session-restart".to_string());
        let (client, agent) = (self.client.clone(), self.agent.clone());
        async move {
            let ticket = ticket?;
            // 外层被撤只丢掉这一头的等待：阻塞线程照样做完、票跟着它落。
            Some(
                joined(tokio::task::spawn_blocking(move || {
                    let _ticket = ticket;
                    crate::faces::session_batch_face::with_deps_as(client.as_deref(), &agent, f)
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
            one_wait::compact_summary(&crate::observe::history_query::agent_home(), &sid)
        }))
    }

    fn watch_arrival(&self, sid: &str) -> impl Future<Output = Result<Armed, String>> + Send {
        let sid = sid.to_string();
        joined(tokio::task::spawn_blocking(move || {
            one_wait::session_arrival(&crate::observe::history_query::agent_home(), &sid)
        }))
    }
}

/// `session-restart`。
pub(crate) async fn answer(args: Value) -> Result<Value, restart::Fault> {
    let client =
        crate::control::gate::requester_of(&args).map_err(|(c, m)| (c.to_string(), m, None))?;
    let agent = args
        .get("agent")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    restart::run(args, Prod { client, agent }).await
}
