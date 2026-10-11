//! **每条长连接报「在看哪几个会话」**（那台后端的 `stream-watch {sids}`，整份换）。
//!
//! 名单按机器合一份：主窗口在看的 tab ∪ 有 `session-lines/<sid>` 订阅的会话（[`EventReplay::watched`]）。
//! 名单一变，或那台换了一条新连接（新连接 ＝ 没报过 ＝ 全看），就在那台的长连接上发一次；主窗口还没报过 ⇒ 一台都不报。
//! 应答 `from[]`（刚进名单的会话从第几行起上流）原样交会话流（[`EventReplay::on_watch_from`]），界面据它补上没上流的那段。
//!
//! 每台一个发送任务，手里只有「最新那一份」（`watch` 通道）：整份换的语义下中间那几份不必发；同一台按序发，不会后发先到。

use crate::event_replay::EventReplay;
use crate::inbound_client::InboundClient;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Weak};
use std::time::Duration;

/// 那台后端的命令名。
const WATCH_CMD: &str = "stream-watch";

/// 一次报名单的期限：后端在连接内就地做完（只是换一张表、数几个游标），几秒足够；超了就等下一次变化 / 下一条连接。
const WATCH_BUDGET: Duration = Duration::from_secs(10);

/// 起那条总循环（`lib.rs` 起步时一次）。
pub(crate) fn spawn(replay: Arc<EventReplay>) {
    tauri::async_runtime::spawn(run(replay));
}

/// 某台的发送任务手里那一份：发给哪条连接（弱引用：连接走了就不发）、报哪几个。
type Report = Option<(Weak<InboundClient>, BTreeSet<String>)>;

async fn run(replay: Arc<EventReplay>) {
    let mut ticks = replay.watch_changes();
    let mut links = crate::inbound_client::link_changes();
    // 每台：上一次交给发送任务的那一份（连接只记弱引用，不替它续命）与发送任务的入口。
    // 键是那台的线上串（`Origin` 不进散列表）。
    let mut sent: HashMap<String, (Weak<InboundClient>, BTreeSet<String>)> = HashMap::new();
    let mut desks: HashMap<String, tokio::sync::watch::Sender<Report>> = HashMap::new();
    loop {
        for (wire, client) in crate::inbound_client::connected() {
            let origin = crate::origin::Origin(wire.clone());
            let Some(want) = replay.watched(&origin) else {
                continue;
            };
            // 不在看的会话：续点作废（它们的行不来，续点停住了；重连时别从旧续点补读一段）。
            crate::snapshot_resume::keep_only(&origin, &want);
            let same = sent.get(&wire).is_some_and(|(c, s)| {
                c.upgrade().is_some_and(|c| Arc::ptr_eq(&c, &client)) && *s == want
            });
            if same {
                continue;
            }
            sent.insert(wire.clone(), (Arc::downgrade(&client), want.clone()));
            let desk = desks.entry(wire).or_insert_with(|| {
                let (tx, rx) = tokio::sync::watch::channel::<Report>(None);
                tauri::async_runtime::spawn(send_loop(origin.clone(), rx, replay.clone()));
                tx
            });
            desk.send_replace(Some((Arc::downgrade(&client), want)));
        }
        tokio::select! {
            r = ticks.changed() => if r.is_err() { return },
            r = links.changed() => if r.is_err() { return },
        }
    }
}

/// 一台的发送任务：拿到最新那一份就发，应答里的 `from[]` 交会话流。
async fn send_loop(
    origin: crate::origin::Origin,
    mut rx: tokio::sync::watch::Receiver<Report>,
    replay: Arc<EventReplay>,
) {
    while rx.changed().await.is_ok() {
        let Some((client, sids)) = rx.borrow_and_update().clone() else {
            continue;
        };
        let Some(client) = client.upgrade() else {
            continue; // 那条连接已经走了（新连接来时总循环会再交一份）
        };
        let args = serde_json::json!({ "sids": sids });
        match client.call(WATCH_CMD, args, WATCH_BUDGET).await {
            Ok(data) => {
                let from = from_of(data.as_ref());
                if !from.is_empty() {
                    replay.on_watch_from(&origin, from);
                }
            }
            Err(e) => tracing::warn!(
                "stream_watch [{}] 报名单没报上（这条连接照旧全看）：{e:?}",
                origin.as_wire_str()
            ),
        }
    }
}

/// 应答 `{from: [{sid, path, seq}]}` ⇒ 会话流那一格的体。形状不对的那一格不收（记一行）。
pub(crate) fn from_of(
    data: Option<&serde_json::Value>,
) -> Vec<crate::ui_contract::SessionWatchPayload> {
    let Some(rows) = data.and_then(|d| d.get("from")).and_then(|f| f.as_array()) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|r| {
            let got = (|| {
                Some(crate::ui_contract::SessionWatchPayload {
                    session_id: r.get("sid")?.as_str()?.to_string(),
                    path: r.get("path")?.as_str()?.to_string(),
                    seq: r.get("seq")?.as_u64()?,
                })
            })();
            if got.is_none() {
                tracing::warn!("stream_watch 应答里一格形状不对，不收：{r}");
            }
            got
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/stream_watch_tests.rs"]
mod tests;
