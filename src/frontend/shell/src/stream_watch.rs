//! **每条长连接报「在看哪几个会话」**（那台后端的 `stream-watch {sids}`，整份换）。
//!
//! 名单按机器合一份：主窗口在看的 tab ∪ 有 `session-lines/<sid>` 订阅的会话（[`EventReplay::watched`]）。
//! 名单一变，或那台换了一条新连接（新连接 ＝ 没报过 ＝ 全看），就在那台的长连接上发一次；主窗口还没报过 ⇒ 一台都不报。
//! 应答 `from[]`（刚进名单的会话从第几行起上流）原样交会话流（[`EventReplay::on_watch_from`]），界面据它补上没上流的那段。
//! 主线外清单：名单里每个会话都要（`branch`），各交回会话账手上那份的条数与摘要（[`crate::session_book::Book::branch_held`]），那台只补缺的。
//!
//! 每台一个发送任务，手里只有「最新那一份」（`watch` 通道）：整份换的语义下中间那几份不必发；同一台按序发，不会后发先到。

use crate::event_replay::EventReplay;
use crate::inbound_client::InboundClient;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Weak};

// 命令名 · 期限 · 入参与应答的读法住 `resident-client`（手机同一份）；这里只管名单从哪来（窗口 ∪ 订阅）、交给谁（会话流）。
use resident_client::watch::{WATCH_BUDGET, WATCH_CMD};

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
        // 交回手上各份主线外清单有什么（读在发的这一刻：断线期间那几帧的拼法已落进会话账），那台只补缺的那一截。
        let held = crate::session_book::book().read().branch_held(&origin, &sids);
        let args = resident_client::watch::args(&sids, &held);
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

/// 应答 `{from: [{sid, path, seq}]}` ⇒ 会话流那一格的体（读法住 `resident_client::watch::from_of`）。形状不对的那一格不收。
pub(crate) fn from_of(
    data: Option<&serde_json::Value>,
) -> Vec<crate::ui_contract::SessionWatchPayload> {
    resident_client::watch::from_of(data)
        .into_iter()
        .map(|w| crate::ui_contract::SessionWatchPayload {
            session_id: w.sid,
            path: w.path,
            seq: w.seq,
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/stream_watch_tests.rs"]
mod tests;
