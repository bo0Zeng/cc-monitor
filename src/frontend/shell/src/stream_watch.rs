//! **桌面的在看名单**：每台一张名单台（[`Desk`]：报名单 · 新连接重报 · `from[]` 都在那里，手机同一份），这里只管名单从哪来、交给谁。
//!
//! 名单按机器合一份：主窗口在看的 tab ∪ 有 `session-lines/<sid>` 订阅的会话（[`EventReplay::watched`]，从空开始）；一变就逐台整份交名单台。
//! 应答 `from[]`（刚进名单的会话从第几行起上流）交会话流（[`EventReplay::on_watch_from`]），界面据它补上没上流的那段。
//! 主线外清单：名单里每个会话都要，各交回会话账手上那份的条数与摘要（[`crate::session_book::Book::branch_held`]），那台只补缺的。
//! 手上留着的：重放留存里那台有行的会话（[`EventReplay::kept`]）随名单一起交，那台不在说的它答已结束；留存多 / 少一条也重报。

use crate::event_replay::EventReplay;
use crate::origin::Origin;
use resident_client::watch::Desk;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// 会话流那一份（起步时 [`spawn`] 交）。
static REPLAY: OnceLock<Arc<EventReplay>> = OnceLock::new();

/// 每台一张（键是那台的线上串；本机 · 远端同一张表，进程里一直留着）。
fn desks() -> &'static Mutex<HashMap<String, Arc<Desk>>> {
    static D: OnceLock<Mutex<HashMap<String, Arc<Desk>>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

/// 那台的名单台（没有就起一张，名单照此刻的）。连接那一侧拿它：写 attach 行时问名单、接上时交连接。
pub(crate) fn desk(origin: &Origin) -> Arc<Desk> {
    let mut t = desks().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = t.get(origin.as_wire_str()) {
        return Arc::clone(d);
    }
    let (held_at, from_at) = (origin.clone(), origin.clone());
    // 发送任务住 monitor 那一份运行时（调的人可能在同步线程上）。
    let _rt = tauri::async_runtime::handle().inner().enter();
    let d = Desk::new(
        move |sids| {
            crate::session_book::book()
                .read()
                .branch_held(&held_at, sids)
        },
        move |rows| {
            if let Some(r) = REPLAY.get() {
                r.on_watch_from(&from_at, rows.into_iter().map(payload).collect());
            }
        },
    );
    if let Some(r) = REPLAY.get() {
        d.set(r.watched(origin), r.kept(origin));
    }
    t.insert(origin.as_wire_str().to_string(), Arc::clone(&d));
    d
}

/// 起那条循环（`lib.rs` 起步时一次）：名单一变，逐台交名单台。
pub(crate) fn spawn(replay: Arc<EventReplay>) {
    let replay = Arc::clone(REPLAY.get_or_init(|| replay));
    tauri::async_runtime::spawn(async move {
        let mut ticks = replay.watch_changes();
        loop {
            let all: Vec<(String, Arc<Desk>)> = desks()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|(o, d)| (o.clone(), Arc::clone(d)))
                .collect();
            for (wire, d) in all {
                let origin = Origin(wire);
                let want = replay.watched(&origin);
                // 不在看的会话：续点作废（它们的行不来，续点停住了；重连时别从旧续点补读一段）。
                crate::snapshot_resume::keep_only(&origin, &want);
                d.set(want, replay.kept(&origin));
            }
            if ticks.changed().await.is_err() {
                return;
            }
        }
    });
}

/// 应答里的一格 ⇒ 会话流那一格的体。
fn payload(w: cmd_wire::watch::WatchFrom) -> crate::ui_contract::SessionWatchPayload {
    crate::ui_contract::SessionWatchPayload {
        session_id: w.sid,
        path: w.path,
        seq: w.seq,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/stream_watch_tests.rs"]
mod tests;
