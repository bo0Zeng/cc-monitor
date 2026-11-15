//! **桌面聊天输入框上方那一排附件**，本机后端替界面走：那一圈是 `chunk_core::attach` 那一份（手机嵌的核心同一份），这里只接线 ——
//! 问收不收 · 送块 · 提交 · 删一份经 [`Host::ask`] 问会话那台（本机 ⇒ 进程内那几条命令；别台 ⇒ 它的常驻流，[`ask_there`]）；
//! 送块读壳放在本机暂存区里的那一份（`~/.cc-monitor/staging/<32 位十六进制>.part`，别处的路径不读），落好 · 撤掉之后删它；
//! 字由 `face_core::attach` 写；那一排每变一次经 [`Host::push`] 推 `changed/attach`（key 草稿号）。
//!
//! ```text
//! 界面 attach-draft.ts ──attach_stage（原样字节）──▶ 壳 ──写进本机暂存区──▶ 回路径
//!        ──attach-draft-put {draft, machine, sid, cards:[{key, name, path, bytes}]}──▶ 本文件 ──chunk_core::attach::pump──▶ 那台
//!        ◀── changed/attach {key: draft, body: AttachDraftShown} ──
//! ```
//!
//! 那台连上与否由这里自己看（[`Desk::follow`]：机器状态里 `linked` 为真 ⇒ 断着的那几排再跑一圈），界面不另说。

use crate::common::said::Said;
use crate::platform::fs::{Own, Ro};
use chunk_core::attach::{self as round, Card, Ended, Tray, Upload};
use chunk_core::{Ask, Watch};
use cmd_wire::attachment::{
    AttachDraftDropReq, AttachDraftPutReq, AttachDraftRetryReq, AttachDraftShown,
    AttachDraftShownCard, AttachDraftTakeReq, AttachDraftTaken, AttachDraftViewReq, AttachItem,
    AttachVerdict, SessionAttachmentCheckReq, SessionAttachmentCommitReq, SessionAttachmentDropReq,
};
use cmd_wire::files::{Sha256Expect, Sha256Hex};
use cmd_wire::transfer::Staged;
use cmd_wire::{Call, Machine, Sid};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// 宿主那几口的未来（装箱：本机 · 别台两种实现）。
pub type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 这一圈够外面的两口（生产那一份住命令表那一层 `stream/inbound/registry/link.rs`；判据给替身）。
pub trait Host: Send + Sync + 'static {
    /// 问会话那台一条命令（名字 · 入参）⇒ 回包，或那一份失败（码 ＋ 那一句）。
    fn ask(
        &self,
        machine: &Machine,
        cmd: &'static str,
        args: serde_json::Value,
    ) -> Fut<'static, Result<serde_json::Value, Said>>;
    /// 那一排变了：推给看着的界面。
    fn push(&self, shown: AttachDraftShown);
}

/// 别台那一问经它的常驻流（同 `feeds::call_passing`，名字取自类型表）。
pub fn ask_there(
    machine: &Machine,
    cmd: &'static str,
    args: serde_json::Value,
) -> Fut<'static, Result<serde_json::Value, Said>> {
    let name = machine.host_name().unwrap_or_default().to_string();
    Box::pin(async move {
        let line = serde_json::json!({ "id": "", "cmd": cmd, "args": args });
        let req: crate::wire::Request = serde_json::from_value(line)
            .map_err(|_| Said::new("failed", copy_core::reply_unreadable(&name)))?;
        let got = crate::control::feeds::forward(name, req).await?;
        Ok(got.unwrap_or(serde_json::Value::Null))
    })
}

/// 会话在本机时问这台自己那几条（`session-attachment-*` · `files-stage-chunk`，都在阻塞档）的那一扇门：
/// 生产那一份住入方向那一层（`stream/inbound/doors.rs::AttachHere`，只放那几条），进程起来时装一次（[`install_here`]）。
pub trait Here: Send + Sync + 'static {
    fn ask(&self, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, Said>;
}

static HERE: std::sync::OnceLock<Arc<dyn Here>> = std::sync::OnceLock::new();

/// 装上问这台自己那一扇门（进程里第一次装的那一个算数）。
pub fn install_here(here: Arc<dyn Here>) {
    if HERE.set(here).is_err() {
        tracing::warn!("[attach_upload] install_here 被调了第二次：照第一次那一份");
    }
}

/// 那一排每变一次的通道（流连接的 tap 订它推 `changed/attach`，可丢：丢了界面重问 `attach-draft-view`）。
pub fn changes() -> &'static tokio::sync::broadcast::Sender<AttachDraftShown> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<AttachDraftShown>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<AttachDraftShown>(64).0)
}

/// 生产那一份宿主：本机 ⇒ 装上的那扇门（阻塞线程上问）；别台 ⇒ 它的常驻流；推 ⇒ [`changes`]。
struct Live;

impl Host for Live {
    fn ask(
        &self,
        machine: &Machine,
        cmd: &'static str,
        args: serde_json::Value,
    ) -> Fut<'static, Result<serde_json::Value, Said>> {
        if !machine.is_local() {
            return ask_there(machine, cmd, args);
        }
        let here = HERE.get().cloned();
        Box::pin(async move {
            let Some(here) = here else {
                return Err(Said::new(cmd_wire::UNKNOWN_COMMAND, cmd.to_string()));
            };
            tokio::task::spawn_blocking(move || here.ask(cmd, args))
                .await
                .unwrap_or_else(|e| {
                    Err(crate::wire::spec::fail(
                        "io_failed",
                        Said::with_raw(copy_core::copy_text("reason.code.bug", &[]), e),
                    ))
                })
        })
    }

    fn push(&self, shown: AttachDraftShown) {
        // 没人订（没有连着的界面）⇒ 交不出去，不是错。
        if changes().send(shown).is_err() {
            tracing::trace!("[attach_upload] 那一排变了，此刻没人订");
        }
    }
}

/// 这台的那几排（头一次加的那一下造，起跟着机器状态的那一路）。
static DESK: std::sync::OnceLock<Arc<Desk>> = std::sync::OnceLock::new();

fn desk_with(own: &Own) -> Arc<Desk> {
    DESK.get_or_init(|| {
        let d = Arc::new(Desk::new(Arc::new(Live), own.clone()));
        d.follow();
        d
    })
    .clone()
}

/// 一问没成的码 ⇒ 结局表那一档：够不到 · 断开 · 超时 · 撤了 ⇒ 断了；别的 ⇒ 那台的错（那一句照带）。
fn ended_of(f: Said) -> Ended {
    match f.code.as_deref() {
        Some(
            crate::control::feeds::UNREACHABLE
            | cmd_wire::DISCONNECTED
            | cmd_wire::TIMEOUT
            | cmd_wire::CANCELLED,
        ) => Ended::Cut,
        _ => Ended::Failed { said: f.said },
    }
}

/// 回包读不懂那一档。
fn unreadable() -> Ended {
    Ended::Failed {
        said: copy_core::copy_text("reason.content.unparsable", &[]),
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 这台的那几排（按草稿号）。
pub struct Desk {
    host: Arc<dyn Host>,
    own: Own,
    /// 本机暂存区（壳放字节的地方）。
    staging: PathBuf,
    rows: Mutex<HashMap<String, Arc<Row>>>,
}

/// 一条草稿那一排。
struct Row {
    draft: String,
    machine: Machine,
    sid: Sid,
    host: Arc<dyn Host>,
    own: Own,
    tray: Mutex<Tray>,
    /// 本机那几份（按卡的键）：壳放进暂存区的那一份；落好 · 撤掉 · 交出去就删。
    files: Mutex<HashMap<String, PathBuf>>,
    /// 在传的那一份的撤旗（按卡的键）。
    moving: Mutex<Option<(String, Arc<AtomicBool>)>>,
}

impl Desk {
    pub fn new(host: Arc<dyn Host>, own: Own) -> Desk {
        let staging = own
            .at_home(relay_route_core::STAGING_DIR_REL)
            .map(|p| p.as_path().to_path_buf())
            .unwrap_or_default();
        Desk {
            host,
            own,
            staging,
            rows: Mutex::default(),
        }
    }

    /// 本机暂存区。
    #[cfg(test)]
    pub(crate) fn staging(&self) -> &Path {
        &self.staging
    }

    fn row(&self, draft: &str) -> Option<Arc<Row>> {
        lock(&self.rows).get(draft).cloned()
    }

    /// 壳放的那一份是不是在暂存区里、名字是不是那一形（`<32 位十六进制>.part`）。
    fn staged_copy(&self, path: &str) -> Option<PathBuf> {
        let p = Path::new(path);
        let name = p.file_name()?.to_str()?;
        let key = name.strip_suffix(crate::common::staging::PART_SUFFIX)?;
        let hex = key.len() == crate::common::staging::KEY_LEN
            && key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        (hex && !self.staging.as_os_str().is_empty() && p.parent()? == self.staging)
            .then(|| p.to_path_buf())
    }

    /// `attach-draft-put`：加几份（那一排没有 ⇒ 开一排），起一圈。
    pub fn put(&self, req: AttachDraftPutReq) -> AttachDraftShown {
        let row = lock(&self.rows)
            .entry(req.draft.clone())
            .or_insert_with(|| {
                Arc::new(Row {
                    draft: req.draft.clone(),
                    machine: req.machine.clone(),
                    sid: req.sid.clone(),
                    host: self.host.clone(),
                    own: self.own.clone(),
                    tray: Mutex::default(),
                    files: Mutex::default(),
                    moving: Mutex::default(),
                })
            })
            .clone();
        {
            let mut t = lock(&row.tray);
            for c in req.cards {
                if t.cards().iter().any(|x| x.key == c.key) {
                    continue;
                }
                if let Some(at) = self.staged_copy(&c.path) {
                    lock(&row.files).insert(c.key.clone(), at);
                }
                t.add(c.key, c.name, c.bytes, c.dir);
            }
        }
        let shown = row.shown(&lock(&row.tray));
        row.host.push(shown.clone());
        kick(row);
        shown
    }

    /// `attach-draft-drop`：✕ 一份。
    pub fn drop_card(&self, req: AttachDraftDropReq) -> AttachDraftShown {
        let Some(row) = self.row(&req.draft) else {
            return Desk::nothing(&req.draft);
        };
        let gone = lock(&row.tray).remove(&req.key);
        row.forget(&req.key);
        if let Some((k, halt)) = lock(&row.moving).as_ref() {
            if *k == req.key {
                halt.store(true, Ordering::SeqCst);
            }
        }
        if let Some(id) = gone.and_then(|c| c.id) {
            let r = row.clone();
            tokio::spawn(async move { r.drop_landed(&id).await });
        }
        let shown = row.shown(&lock(&row.tray));
        row.host.push(shown.clone());
        kick(row);
        shown
    }

    /// `attach-draft-retry`：［重试］。
    pub fn retry(&self, req: AttachDraftRetryReq) -> AttachDraftShown {
        let Some(row) = self.row(&req.draft) else {
            return Desk::nothing(&req.draft);
        };
        lock(&row.tray).retry();
        let shown = row.shown(&lock(&row.tray));
        row.host.push(shown.clone());
        kick(row);
        shown
    }

    /// `attach-draft-take`：全落好了 ⇒ 编号（那一排清空）；还有没落好的 ⇒ `None`。
    pub fn take(&self, req: AttachDraftTakeReq) -> AttachDraftTaken {
        let Some(row) = self.row(&req.draft) else {
            return AttachDraftTaken {
                ids: Some(Vec::new()),
            };
        };
        let ids = lock(&row.tray).take();
        if ids.is_some() {
            let keys: Vec<String> = lock(&row.files).keys().cloned().collect();
            for k in keys {
                row.forget(&k);
            }
            row.host.push(row.shown(&lock(&row.tray)));
        }
        AttachDraftTaken { ids }
    }

    /// `attach-draft-view`：此刻那一排。
    pub fn view(&self, req: AttachDraftViewReq) -> AttachDraftShown {
        match self.row(&req.draft) {
            Some(row) => row.shown(&lock(&row.tray)),
            None => Desk::nothing(&req.draft),
        }
    }

    /// 那台连上了：断着的那几排接着传。
    pub fn up(&self, machine: &str) {
        let at = Machine::named(machine);
        let rows: Vec<Arc<Row>> = lock(&self.rows)
            .values()
            .filter(|r| r.machine == at)
            .cloned()
            .collect();
        for row in rows {
            if !lock(&row.tray).cut() {
                continue;
            }
            lock(&row.tray).up();
            kick(row);
        }
    }

    /// 跟着机器状态：哪台连着（`linked`）⇒ [`Desk::up`]。落后丢了几件 ⇒ 每台都再试一次。在异步运行时里调一次。
    pub fn follow(self: &Arc<Self>) {
        let me = self.clone();
        let mut rx = crate::control::machine_state::changes().subscribe();
        tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match rx.recv().await {
                    Ok(m) if m.state.linked => me.up(&m.machine),
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => {
                        let names: Vec<Machine> =
                            lock(&me.rows).values().map(|r| r.machine.clone()).collect();
                        for m in names {
                            me.up(m.host_name().unwrap_or(cmd_wire::LOCAL));
                        }
                    }
                    Err(RecvError::Closed) => return,
                }
            }
        });
    }

    /// 没有这一排：没有卡、全落好。
    pub fn nothing(draft: &str) -> AttachDraftShown {
        AttachDraftShown {
            draft: draft.to_string(),
            cards: Vec::new(),
            note: None,
            offline: false,
            ready: true,
        }
    }
}

/// 起一圈（已经在跑 ⇒ 那一圈自己接着走）。
fn kick(row: Arc<Row>) {
    tokio::spawn(async move {
        round::pump(&row.tray, &*row).await;
    });
}

impl Row {
    /// 删掉壳放的那一份（删不掉交暂存区的孤儿扫）。
    fn forget(&self, key: &str) {
        let Some(at) = lock(&self.files).remove(key) else {
            return;
        };
        let name = at.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let rel = format!("{}/{name}", relay_route_core::STAGING_DIR_REL);
        if let Ok(p) = self.own.at_home(&rel) {
            if let Err(e) = self.own.delete_file(&p) {
                tracing::debug!("附件暂存那一份删不掉（交孤儿扫）：{e}");
            }
        }
    }

    async fn call<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, Ended> {
        let args = serde_json::to_value(req).map_err(|_| unreadable())?;
        let got = self
            .host
            .ask(&self.machine, C::NAME, args)
            .await
            .map_err(ended_of)?;
        serde_json::from_value(got).map_err(|_| unreadable())
    }

    /// 此刻那一排（推给界面的那一形）。
    fn shown(&self, t: &Tray) -> AttachDraftShown {
        let offline = copy_core::offline(self.machine.host_name().unwrap_or(cmd_wire::LOCAL));
        let want = t.want(Some(&offline));
        let face = face_core::attach::face(&want);
        AttachDraftShown {
            draft: self.draft.clone(),
            cards: t
                .cards()
                .iter()
                .zip(face.cards)
                .map(|(c, f)| {
                    let class = crate::common::file_class::class_of_name(c.name.as_bytes(), c.dir);
                    AttachDraftShownCard {
                        key: c.key.clone(),
                        name: c.name.clone(),
                        phase: c.phase.clone(),
                        done: c.done,
                        bytes: c.bytes,
                        id: c.id.clone(),
                        line: f.line,
                        remove: f.remove,
                        class,
                        class_text: crate::common::file_class::label(class),
                    }
                })
                .collect(),
            note: face.note,
            offline: want.offline.is_some(),
            ready: t.ready(),
        }
    }
}

/// 送块那一口：问那台照 [`Row::call`]；撤旗立着 ⇒ 用户撤了。
struct Blocks<'a> {
    row: &'a Row,
    halt: &'a AtomicBool,
}

impl Ask for Blocks<'_> {
    type Err = Ended;
    async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, Ended> {
        let got = self.row.call(req).await;
        if got.is_err() && self.halt.load(Ordering::SeqCst) {
            return Err(Ended::Stopped);
        }
        got
    }
}

/// 撤 · 进度。
struct Moves<'a, 'b> {
    halt: &'a AtomicBool,
    moved: &'a mut (dyn FnMut(u64) + Send + 'b),
}

impl Watch for Moves<'_, '_> {
    fn stopped(&self) -> bool {
        self.halt.load(Ordering::SeqCst)
    }
    fn moved(&mut self, done: u64, _total: Option<u64>) {
        (self.moved)(done);
    }
}

/// 一份暂存件的键：16 个随机字节（进程随机种子的散列 ＋ 此刻 ＋ 卡的键）。
fn fresh_key(card: &str) -> String {
    use std::hash::{BuildHasher as _, Hasher as _};
    let mut random = [0u8; 16];
    for (i, half) in random.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write(card.as_bytes());
        h.write_usize(i);
        h.write_u64(crate::common::time::now_ms());
        half.copy_from_slice(&h.finish().to_le_bytes());
    }
    chunk_core::key_of(random)
}

impl Upload for Row {
    async fn check(&self, items: &[AttachItem]) -> Result<Vec<AttachVerdict>, Ended> {
        let req = SessionAttachmentCheckReq {
            sid: self.sid.clone(),
            items: items.to_vec(),
        };
        let got = self.call(&req).await?.items;
        if got.len() != items.len() {
            return Err(unreadable());
        }
        Ok(got)
    }

    async fn stage(
        &self,
        card: &Card,
        moved: &mut (dyn FnMut(u64) + Send),
    ) -> Result<Staged, Ended> {
        let Some(at) = lock(&self.files).get(&card.key).cloned() else {
            return Err(Ended::Unreadable);
        };
        let Ok(file) = Ro::open_nofollow(&at) else {
            return Err(Ended::Unreadable);
        };
        let halt = Arc::new(AtomicBool::new(false));
        *lock(&self.moving) = Some((card.key.clone(), halt.clone()));
        let blocks = Blocks {
            row: self,
            halt: &halt,
        };
        let mut watch = Moves { halt: &halt, moved };
        let got = chunk_core::stage(
            &blocks,
            fresh_key(&card.key),
            file,
            Some(card.bytes),
            &mut watch,
            || Ended::Stopped,
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
        // 落好了：壳放的那一份不再要。
        self.forget(&card.key);
        Ok(id)
    }

    async fn drop_landed(&self, id: &str) {
        let req = SessionAttachmentDropReq {
            sid: self.sid.clone(),
            id: id.to_string(),
        };
        // 删不掉：那台的附件目录过期自清。
        if let Err(e) = self.call(&req).await {
            tracing::debug!("撤掉的附件那台没删成（交它过期自清）：{e:?}");
        }
    }

    fn changed(&self, tray: &Tray) {
        self.host.push(self.shown(tray));
    }
}

crate::wire::typed::codes! {
    /// attach-draft-* 那五条的码（那一排里没成的逐张卡落在体里，不是错）。
    pub enum AttachDraftErr {}
}

// ── 五条 typed 命令：只有界面问、没有 Rust 客户端 ⇒ 名字与类型写在这一处（不进 `cmd_wire::call` 那张表）──

use crate::wire::typed::{needs, Command, Cx, Handle};

/// `attach-draft-put`。
pub struct AttachDraftPut;
impl Command for AttachDraftPut {
    const NAME: &'static str = "attach-draft-put";
    type Req = AttachDraftPutReq;
    type Reply = AttachDraftShown;
    type Err = AttachDraftErr;
}
impl Handle for AttachDraftPut {
    type Needs = needs::Own;
    async fn run(
        req: AttachDraftPutReq,
        cx: Cx<Self::Needs>,
    ) -> Result<AttachDraftShown, AttachDraftErr> {
        Ok(desk_with(&cx.needs).put(req))
    }
}

/// `attach-draft-drop`。
pub struct AttachDraftDrop;
impl Command for AttachDraftDrop {
    const NAME: &'static str = "attach-draft-drop";
    type Req = AttachDraftDropReq;
    type Reply = AttachDraftShown;
    type Err = AttachDraftErr;
}
impl Handle for AttachDraftDrop {
    type Needs = needs::Nothing;
    async fn run(
        req: AttachDraftDropReq,
        _cx: Cx<Self::Needs>,
    ) -> Result<AttachDraftShown, AttachDraftErr> {
        Ok(match DESK.get() {
            Some(d) => d.drop_card(req),
            None => Desk::nothing(&req.draft),
        })
    }
}

/// `attach-draft-retry`。
pub struct AttachDraftRetry;
impl Command for AttachDraftRetry {
    const NAME: &'static str = "attach-draft-retry";
    type Req = AttachDraftRetryReq;
    type Reply = AttachDraftShown;
    type Err = AttachDraftErr;
}
impl Handle for AttachDraftRetry {
    type Needs = needs::Nothing;
    async fn run(
        req: AttachDraftRetryReq,
        _cx: Cx<Self::Needs>,
    ) -> Result<AttachDraftShown, AttachDraftErr> {
        Ok(match DESK.get() {
            Some(d) => d.retry(req),
            None => Desk::nothing(&req.draft),
        })
    }
}

/// `attach-draft-take`。
pub struct AttachDraftTake;
impl Command for AttachDraftTake {
    const NAME: &'static str = "attach-draft-take";
    type Req = AttachDraftTakeReq;
    type Reply = AttachDraftTaken;
    type Err = AttachDraftErr;
}
impl Handle for AttachDraftTake {
    type Needs = needs::Nothing;
    async fn run(
        req: AttachDraftTakeReq,
        _cx: Cx<Self::Needs>,
    ) -> Result<AttachDraftTaken, AttachDraftErr> {
        Ok(match DESK.get() {
            Some(d) => d.take(req),
            None => AttachDraftTaken {
                ids: Some(Vec::new()),
            },
        })
    }
}

/// `attach-draft-view`。
pub struct AttachDraftView;
impl Command for AttachDraftView {
    const NAME: &'static str = "attach-draft-view";
    type Req = AttachDraftViewReq;
    type Reply = AttachDraftShown;
    type Err = AttachDraftErr;
}
impl Handle for AttachDraftView {
    type Needs = needs::Nothing;
    async fn run(
        req: AttachDraftViewReq,
        _cx: Cx<Self::Needs>,
    ) -> Result<AttachDraftShown, AttachDraftErr> {
        Ok(match DESK.get() {
            Some(d) => d.view(req),
            None => Desk::nothing(&req.draft),
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/control/attach_upload_tests.rs"]
mod tests;
