//! **手机聊天输入框上方那一排附件**在核心里的宿主：那一圈是 `chunk_core::attach` 那一份（桌面本机后端同一份），这里只接线 ——
//! 问收不收 · 提交 · 删一份经这台的常驻流（每一问一个期限）；送块照 `transfer` 的 [`Over`]（＋ 撤的旗 [`Halting`]）走 `chunk_core::stage`；
//! 本机那一份是 Kotlin 加那一下交出的描述符（归这里，重传从头读）；字由 `face_core::attach` 写。
//!
//! 对 Kotlin 一个口 [`attach`]：入 [`AttachDo`]、回 [`AttachDone`]；那一排每变一次交 `attach` 那一种事件（[`AttachShown`]）。

use crate::resident::{self, OnEvent};
use crate::transfer::{self, Halt, Halting};
use chunk_core::attach::{self as round, Card, Ended, Tray, Upload};
use chunk_core::{Ask, Watch};
use cmd_wire::attachment::{
    AttachItem, AttachVerdict, SessionAttachmentCheckReq, SessionAttachmentCommitReq,
    SessionAttachmentDropReq,
};
use cmd_wire::files::{Sha256Expect, Sha256Hex};
use cmd_wire::phone::{AttachDo, AttachDone, AttachKept, AttachShown, AttachShownCard, CallEnd};
use cmd_wire::transfer::Staged;
use cmd_wire::Call;
use resident_client::client::CallError;
use std::collections::HashMap;
use std::fs::File;
use std::io::Seek as _;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// 一问（问收不收 · 一块 · 提交 · 删）等多久（同文件页上传）。
const STEP: Duration = Duration::from_secs(20);

/// 一条会话那一排。
struct PhoneTray {
    id: i64,
    h: i64,
    sid: cmd_wire::Sid,
    machine: String,
    tray: Mutex<Tray>,
    /// 本机那几份（按卡的键）：Kotlin 交出的描述符，卡撤了 · 交出了 · 这一排关了就关。
    files: Mutex<HashMap<String, File>>,
    /// 在传的那一份的撤旗（按卡的键）。
    moving: Mutex<Option<(String, Arc<Halt>)>>,
    on_event: OnEvent,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn trays() -> &'static Mutex<HashMap<i64, Arc<PhoneTray>>> {
    static T: OnceLock<Mutex<HashMap<i64, Arc<PhoneTray>>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

fn tray_of(id: i64) -> Option<Arc<PhoneTray>> {
    lock(trays()).get(&id).cloned()
}

/// 常驻流那一问没成 ⇒ 结局表那一档：断开 · 超时 · 流撤了 ⇒ 断了；那台回的失败 ⇒ 那台的错（原句照带）；
/// 别的（事前就拒的那几种）⇒ 那台的错，那一句照一问一答那一份写（[`resident::reply_json`]）。
fn ended_of(machine: &str, cmd: &str, e: CallError) -> Ended {
    match e {
        CallError::Disconnected | CallError::Timeout { .. } | CallError::Cancelled => Ended::Cut,
        CallError::Remote(f) => Ended::Failed { said: f.said },
        other => Ended::Failed {
            said: said_of_line(&resident::reply_json(0, machine, cmd, Err(other))),
        },
    }
}

/// 交 Kotlin 的那一行（[`CallEnd`]）里的那一句。
fn said_of_line(line: &str) -> String {
    match serde_json::from_str::<CallEnd>(line) {
        Ok(
            CallEnd::Failed { said, .. }
            | CallEnd::Timeout { said, .. }
            | CallEnd::Cancelled { said, .. },
        ) => said,
        _ => copy_core::copy_text("reason.content.unparsable", &[]),
    }
}

/// 分块那一半没成的那一行（`transfer` 那一族交的 [`CallEnd`]）⇒ 结局表那一档；撤了的旗立着 ⇒ 用户撤了。
fn ended_of_end(line: &str, halted: bool) -> Ended {
    match serde_json::from_str::<CallEnd>(line) {
        Ok(CallEnd::Cancelled { .. }) if halted => Ended::Stopped,
        Ok(CallEnd::Timeout { .. } | CallEnd::Down { .. } | CallEnd::Cancelled { .. }) => {
            Ended::Cut
        }
        Ok(CallEnd::Failed { said, .. }) => Ended::Failed { said },
        _ => Ended::Failed {
            said: copy_core::copy_text("reason.content.unparsable", &[]),
        },
    }
}

/// 送块那一口：问那台照 `transfer` 的 [`Over`]（带撤旗），没成的那一行分进结局表。
struct Blocks<'a> {
    ask: Halting<transfer::Over>,
    halt: &'a Halt,
}

impl Ask for Blocks<'_> {
    type Err = Ended;
    async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, Ended> {
        self.ask
            .ask(req)
            .await
            .map_err(|line| ended_of_end(&line, self.halt.is()))
    }
}

/// 撤 · 进度：撤旗立了 · 流收了 ⇒ 撤；每块报已送字节。
struct Moves<'a, 'b> {
    h: i64,
    halt: &'a Halt,
    moved: &'a mut (dyn FnMut(u64) + Send + 'b),
}

impl Watch for Moves<'_, '_> {
    fn stopped(&self) -> bool {
        self.halt.is() || resident::feed(self.h).is_none()
    }
    fn moved(&mut self, done: u64, _total: Option<u64>) {
        (self.moved)(done);
    }
}

impl PhoneTray {
    /// 经这台的常驻流问一条有类型的命令。
    async fn call<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, Ended> {
        let Some(f) = resident::feed(self.h) else {
            return Err(Ended::Cut);
        };
        let deadline = tokio::time::Instant::now() + STEP;
        let got = match f.client_by(deadline).await {
            Some(c) => c.ask(None, req, None, deadline).await,
            None => Err(CallError::Disconnected),
        };
        match got {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(_)) => Err(Ended::Failed {
                said: copy_core::copy_text("reason.content.unparsable", &[]),
            }),
            Err(e) => Err(ended_of(&self.machine, C::NAME, e)),
        }
    }

    /// 此刻那一排（交 Kotlin 的那一形）。
    fn shown(&self, t: &Tray) -> AttachShown {
        let offline = copy_core::offline(&self.machine);
        let want = t.want(Some(&offline));
        let face = face_core::attach::face(&want);
        AttachShown {
            tray: self.id,
            cards: t
                .cards()
                .iter()
                .zip(face.cards)
                .map(|(c, f)| AttachShownCard {
                    key: c.key.clone(),
                    phase: c.phase.clone(),
                    done: c.done,
                    id: c.id.clone(),
                    line: f.line,
                    remove: f.remove,
                })
                .collect(),
            note: face.note,
            offline: want.offline.is_some(),
            ready: t.ready(),
        }
    }

    fn emit(&self, t: &Tray) {
        (self.on_event)(
            self.h,
            "attach",
            serde_json::to_string(&self.shown(t)).unwrap_or_default(),
        );
    }
}

impl Upload for PhoneTray {
    async fn check(&self, items: &[AttachItem]) -> Result<Vec<AttachVerdict>, Ended> {
        let req = SessionAttachmentCheckReq {
            sid: self.sid.clone(),
            items: items.to_vec(),
        };
        let got = self.call(&req).await?.items;
        if got.len() != items.len() {
            return Err(Ended::Failed {
                said: copy_core::copy_text("reason.content.unparsable", &[]),
            });
        }
        Ok(got)
    }

    async fn stage(
        &self,
        card: &Card,
        moved: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Staged, Ended> {
        let file = {
            let files = lock(&self.files);
            files.get(&card.key).map(File::try_clone)
        };
        let Some(Ok(mut file)) = file else {
            return Err(Ended::Unreadable);
        };
        if file.rewind().is_err() {
            return Err(Ended::Unreadable);
        }
        let halt = Arc::new(Halt::default());
        *lock(&self.moving) = Some((card.key.clone(), halt.clone()));
        let blocks = Blocks {
            ask: Halting {
                ask: transfer::over(self.h, 0, STEP),
                halt: halt.clone(),
                id: 0,
            },
            halt: &halt,
        };
        let mut random = [0u8; 16];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random);
        let total = transfer::size_of(&file);
        let mut watch = Moves {
            h: self.h,
            halt: &halt,
            moved,
        };
        let got = chunk_core::stage(
            &blocks,
            chunk_core::key_of(random),
            file,
            total,
            &mut watch,
            || {
                if halt.is() {
                    Ended::Stopped
                } else {
                    Ended::Cut
                }
            },
            |_| Ended::Unreadable,
        )
        .await;
        *lock(&self.moving) = None;
        got
    }

    async fn commit(&self, card: &Card, staged: &Staged) -> Result<String, Ended> {
        let req = SessionAttachmentCommitReq {
            sid: self.sid.clone(),
            key: staged.key.clone(),
            name: card.name.clone(),
            bytes: staged.bytes,
            chunks: Some(staged.chunks),
            expect: Sha256Expect {
                sha256: Sha256Hex(staged.sha256.clone()),
            },
        };
        let id = self.call(&req).await?.id;
        // 落好了：本机那一份不再要（再开照编号摆回，不重传）。
        lock(&self.files).remove(&card.key);
        Ok(id)
    }

    async fn drop_landed(&self, id: &str) {
        let req = SessionAttachmentDropReq {
            sid: self.sid.clone(),
            id: id.to_string(),
        };
        // 删不掉：那台的附件目录过期自清。
        let _ = self.call(&req).await;
    }

    fn changed(&self, tray: &Tray) {
        self.emit(tray);
    }
}

/// 起一圈（已经在跑 ⇒ 那一圈自己接着走）。
fn kick(t: Arc<PhoneTray>) {
    tokio::spawn(async move {
        round::pump(&t.tray, &*t).await;
    });
}

/// 收下一份本机副本的描述符（没给 · 读不出 ⇒ 不记，传到它时落「本机副本开不了」）。
fn keep(t: &PhoneTray, card: &AttachKept) {
    if let Some(file) = card.fd.and_then(crate::file_of) {
        lock(&t.files).insert(card.key.clone(), file);
    }
}

fn next_tray() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Kotlin 那一口：入 [`AttachDo`] 那一行、回 [`AttachDone`] 那一行（读不动 ⇒ 空串）。
pub fn attach(h: i64, op: &str, on_event: OnEvent) -> String {
    let Ok(op) = serde_json::from_str::<AttachDo>(op) else {
        return String::new();
    };
    let done = run(h, op, on_event);
    serde_json::to_string(&done).unwrap_or_default()
}

fn run(h: i64, op: AttachDo, on_event: OnEvent) -> AttachDone {
    let none = |tray| AttachDone { tray, ids: None };
    if let AttachDo::Open { sid, cards } = op {
        let Some(f) = resident::feed(h) else {
            // 描述符交来了就归这里：这台不在也当场关。
            for c in &cards {
                drop(c.fd.and_then(crate::file_of));
            }
            return none(0);
        };
        let mut tray = Tray::default();
        for c in &cards {
            tray.restore(c.key.clone(), c.name.clone(), c.bytes, c.id.clone());
        }
        let t = Arc::new(PhoneTray {
            id: next_tray(),
            h,
            sid,
            machine: f.machine().to_string(),
            tray: Mutex::new(tray),
            files: Mutex::default(),
            moving: Mutex::default(),
            on_event,
        });
        for c in cards.iter().filter(|c| c.id.is_none()) {
            keep(&t, c);
        }
        lock(trays()).insert(t.id, t.clone());
        t.emit(&lock(&t.tray));
        let id = t.id;
        kick(t);
        return none(id);
    }
    let id = match &op {
        AttachDo::Add { tray, .. }
        | AttachDo::Remove { tray, .. }
        | AttachDo::Retry { tray, .. }
        | AttachDo::Up { tray }
        | AttachDo::Local { tray, .. }
        | AttachDo::Take { tray }
        | AttachDo::Close { tray } => *tray,
        AttachDo::Open { .. } => 0,
    };
    let Some(t) = tray_of(id) else {
        match &op {
            AttachDo::Add { card, .. } => drop(card.fd.and_then(crate::file_of)),
            AttachDo::Retry { cards, .. } => cards
                .iter()
                .for_each(|c| drop(c.fd.and_then(crate::file_of))),
            _ => {}
        }
        return none(id);
    };
    match op {
        AttachDo::Open { .. } => none(id),
        AttachDo::Add { card, .. } => {
            keep(&t, &card);
            {
                let mut g = lock(&t.tray);
                g.add(card.key, card.name, card.bytes, false);
                t.emit(&g);
            }
            kick(t);
            none(id)
        }
        AttachDo::Remove { key, .. } => {
            let gone = {
                let mut g = lock(&t.tray);
                let gone = g.remove(&key);
                t.emit(&g);
                gone
            };
            lock(&t.files).remove(&key);
            if let Some((k, halt)) = lock(&t.moving).as_ref() {
                if *k == key {
                    halt.set();
                }
            }
            if let Some(landed) = gone.and_then(|c| c.id) {
                let t = t.clone();
                tokio::spawn(async move { t.drop_landed(&landed).await });
            }
            kick(t);
            none(id)
        }
        AttachDo::Retry { cards, .. } => {
            // 本机副本重开过的那几份：换上新的描述符（重传从头读）。
            cards.iter().for_each(|c| keep(&t, c));
            {
                let mut g = lock(&t.tray);
                g.retry();
                t.emit(&g);
            }
            kick(t);
            none(id)
        }
        AttachDo::Up { .. } => {
            {
                let mut g = lock(&t.tray);
                g.up();
                t.emit(&g);
            }
            kick(t);
            none(id)
        }
        AttachDo::Local { why, .. } => {
            let mut g = lock(&t.tray);
            g.local(why);
            t.emit(&g);
            none(id)
        }
        AttachDo::Take { .. } => {
            let mut g = lock(&t.tray);
            let ids = g.take();
            if ids.is_some() {
                lock(&t.files).clear();
                t.emit(&g);
            }
            AttachDone { tray: id, ids }
        }
        AttachDo::Close { .. } => {
            lock(trays()).remove(&id);
            // 卡全撤：在跑的那一圈下一步收场，在传那一份落好了照「卡已撤」让那台删掉。
            lock(&t.tray).close();
            if let Some((_, halt)) = lock(&t.moving).as_ref() {
                halt.set();
            }
            lock(&t.files).clear();
            none(id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmd_wire::attachment::AttachCardPhase;
    use cmd_wire::failed::Failed;

    #[test]
    fn link_endings_fall_into_the_outcome_table() {
        assert_eq!(ended_of("box", "x", CallError::Disconnected), Ended::Cut);
        assert_eq!(ended_of("box", "x", CallError::Cancelled), Ended::Cut);
        let timeout = CallError::Timeout {
            after: Duration::from_secs(1),
            withdraw: comms_inward::chan::wire::Withdraw::Unsent,
        };
        assert_eq!(ended_of("box", "x", timeout), Ended::Cut);
        let remote = CallError::Remote(Failed {
            code: "dir_full".into(),
            said: "那台装不下了".into(),
            detail: "d".into(),
            data: None,
        });
        assert_eq!(
            ended_of("box", "x", remote),
            Ended::Failed {
                said: "那台装不下了".into()
            }
        );
    }

    #[test]
    fn chunk_endings_fall_into_the_outcome_table() {
        let line = |e: &CallEnd| serde_json::to_string(e).unwrap();
        let cancelled = line(&CallEnd::Cancelled {
            call: 0,
            said: "s".into(),
            detail: "d".into(),
        });
        assert_eq!(ended_of_end(&cancelled, true), Ended::Stopped);
        assert_eq!(ended_of_end(&cancelled, false), Ended::Cut);
        assert_eq!(
            ended_of_end(&line(&CallEnd::Down { call: 0 }), false),
            Ended::Cut
        );
        let failed = line(&CallEnd::Failed {
            call: 0,
            code: "io_failed".into(),
            said: "磁盘满".into(),
            detail: "d".into(),
            data: None,
        });
        assert_eq!(
            ended_of_end(&failed, false),
            Ended::Failed {
                said: "磁盘满".into()
            }
        );
    }

    #[test]
    fn the_op_lines_are_the_contract_shapes() {
        let op: AttachDo = serde_json::from_str(
            r#"{"op":"add","tray":3,"card":{"key":"k","name":"a.png","bytes":5,"id":null,"fd":7}}"#,
        )
        .expect("契约那一形");
        assert!(matches!(op, AttachDo::Add { tray: 3, .. }));
        for (line, want) in [
            (
                r#"{"op":"open","sid":"00000000-0000-0000-0000-000000000000","cards":[]}"#,
                "open",
            ),
            (r#"{"op":"remove","tray":3,"key":"k"}"#, "remove"),
            (r#"{"op":"retry","tray":3,"cards":[]}"#, "retry"),
            (r#"{"op":"up","tray":3}"#, "up"),
            (r#"{"op":"local","tray":3,"why":"no_room"}"#, "local"),
            (r#"{"op":"take","tray":3}"#, "take"),
            (r#"{"op":"close","tray":3}"#, "close"),
        ] {
            let op: AttachDo = serde_json::from_str(line).unwrap_or_else(|e| panic!("{want}: {e}"));
            assert_eq!(serde_json::to_value(&op).unwrap()["op"], want);
        }
        assert!(
            serde_json::from_str::<AttachDo>(r#"{"op":"retry","tray":3}"#).is_err(),
            "重试要带那几份的描述符"
        );
        assert_eq!(attach(0, "nope", Arc::new(|_, _, _| {})), "");
        // 没开过的那一排：照号回、不做。
        assert_eq!(
            attach(0, r#"{"op":"take","tray":999}"#, Arc::new(|_, _, _| {})),
            r#"{"tray":999,"ids":null}"#
        );
    }

    #[test]
    fn the_shown_row_follows_the_tray_card_by_card() {
        let mut tray = Tray::default();
        tray.restore("k-a".into(), "a.png".into(), 5, Some("id-a".into()));
        tray.add("k-b".into(), "b.png".into(), 5, false);
        let t = PhoneTray {
            id: 4,
            h: 0,
            sid: serde_json::from_str("\"00000000-0000-0000-0000-000000000000\"").unwrap(),
            machine: "box".into(),
            tray: Mutex::new(Tray::default()),
            files: Mutex::default(),
            moving: Mutex::default(),
            on_event: Arc::new(|_, _, _| {}),
        };
        let s = t.shown(&tray);
        assert_eq!(s.tray, 4);
        let keys: Vec<&str> = s.cards.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, vec!["k-a", "k-b"]);
        assert_eq!(s.cards[0].phase, AttachCardPhase::Landed);
        assert_eq!(s.cards[0].id.as_deref(), Some("id-a"));
        assert_eq!(s.cards[0].line, None);
        assert_eq!(
            s.cards[1].line.as_deref(),
            Some(copy_core::copy_text("mobAttach.card.queued", &[]).as_str())
        );
        assert!(!s.ready);
        assert!(!s.offline, "没断着不画虚线");
    }
}
