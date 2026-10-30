//! **一组一条命令**在手机这一侧的宿主：全过程是 `batch_core::run` 那一份（桌面文件窗口同一份），这里只接三根线 ——
//! 问那台（常驻流那一条，带票；断了 ⇒ 等流接上、同一张票再问）· 进度（`files_batch` 帧在核心里按票接，不交 Kotlin 的帧流）·
//! 往 Kotlin 交的事：`batch {id, phase, title, …}`，字全由核心写好（标题 · 断开那一行 · 结局句 · 没成的那几行 · 复制详情）；
//! 落盘那一份 `batch_keep {ticket, plan}`（进程没了再开，Kotlin 原样交回、这里照同一张票再问）。
//!
//! Kotlin 交的是「要做什么」（删 · 改权限 · 复制到 · 下到本机 · 重试 · 撤销），入参由这里拼；同名那一问的答经 [`answer`]，「停」经 [`stop`]。
//!
//! 下到本机那一组（[`Verb::Pull`]）也是 `batch_core::run` 那一圈：「问那台」那一口在这一侧是本机那一圈（[`PhoneBoard::pull_round`]），
//! 每项要本机那一份时交 `batch_local {open}`、等 Kotlin 经 [`answer`] 回描述符；Kotlin 只开 · 删本机那一份。

use crate::busy::Busy;
use crate::resident::{self, OnEvent};
use crate::transfer::{self, Halt};
use batch_core::{Board, Failed, Phase, Plan, Rebuff, Tick, Verb};
use chunk_core::Ask;
use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Decide, ItemState};
use cmd_wire::files::WirePath;
use cmd_wire::phone::{
    BatchAnswer, BatchKeep, BatchLine, CallEnd, ClashRow, LocalAsk, LocalOpened,
};
use cmd_wire::Call;
use resident_client::client::CallError;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

/// 下到本机那一组在这一侧的那一圈：逐项结局（没轮到 ⇒ `None`）· 手上那一项（名字 · 几成）· 在等 Kotlin 开的那一份 · 撤了没有。
#[derive(Default)]
struct Pulling {
    outcomes: Mutex<Vec<Option<BatchOutcome>>>,
    part: Mutex<Option<(String, f32)>>,
    opening: Mutex<Option<(String, tokio::sync::oneshot::Sender<LocalOpened>)>>,
    halt: Arc<Halt>,
}

/// 一项拉完是什么样（[`transfer::pull_with`] 那一行）。
enum Pulled {
    Done(Option<u64>),
    Failed(String, String),
    Cut,
    Stopped,
}

/// 下到本机那一组的入参：`{items: [{path, name}], folder, here}`。
struct PullArgs {
    items: Vec<(WirePath, String)>,
    folder: String,
    here: Vec<String>,
}

impl PullArgs {
    fn of(args: &serde_json::Value) -> Option<Self> {
        let items = args
            .get("items")?
            .as_array()?
            .iter()
            .map(|i| {
                Some((
                    serde_json::from_value::<WirePath>(i.get("path")?.clone()).ok()?,
                    i.get("name")?.as_str()?.to_string(),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let here = args
            .get("here")?
            .as_array()?
            .iter()
            .map(|h| h.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            items,
            folder: args.get("folder")?.as_str()?.to_string(),
            here,
        })
    }

    fn names(&self) -> Vec<String> {
        self.items.iter().map(|(_, n)| n.clone()).collect()
    }
}

/// 一组在手机上的样子：计划 · 票 · 最后一格进度 · 手上那一项 · 同名那一问的答送哪儿 · 结局。
pub struct PhoneBoard {
    pub id: i64,
    h: i64,
    plan: Plan,
    ticket: String,
    tick: Mutex<(Tick, Option<String>)>,
    answer: Mutex<Option<tokio::sync::oneshot::Sender<Option<BTreeMap<String, Decide>>>>>,
    asking: Mutex<Vec<Conflict>>,
    ended: Mutex<Option<BatchReply>>,
    /// 最后发出去的那一份入参（落盘的就是它）。
    sent: Mutex<Option<serde_json::Value>>,
    /// 从落盘那一份接回来的（进程没了再开）：交的每一行带 `kept`，屏上没起过它也照画。
    kept: bool,
    /// 下到本机那一组才有（[`Verb::Pull`]）。
    pulling: Option<Pulling>,
    on_event: OnEvent,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PhoneBoard {
    pub fn new(id: i64, h: i64, plan: Plan, ticket: String, on_event: OnEvent) -> Self {
        let pulling = (plan.verb == Verb::Pull).then(|| Pulling {
            outcomes: Mutex::new(vec![None; plan.names.len()]),
            ..Pulling::default()
        });
        Self {
            id,
            h,
            plan,
            ticket,
            tick: Mutex::default(),
            answer: Mutex::default(),
            asking: Mutex::default(),
            ended: Mutex::default(),
            sent: Mutex::default(),
            kept: false,
            pulling,
            on_event,
        }
    }

    /// 落盘那一份（[`Self::keep`] 交的 `plan`）⇒ 接回来的那一组：同一张票、同一份入参、断开时的完成数。认不出 ⇒ `None`。
    pub fn resumed(id: i64, h: i64, kept: &str, machine: &str, on_event: OnEvent) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(kept).ok()?;
        let (cmd, verb) = [
            (
                <cmd_wire::files::FilesDeleteReq as Call>::NAME,
                Verb::Delete,
            ),
            (<cmd_wire::files::FilesChmodReq as Call>::NAME, Verb::Chmod),
            (<cmd_wire::files::FilesCopyReq as Call>::NAME, Verb::Copy),
            (<cmd_wire::files::ReadChunkReq as Call>::NAME, Verb::Pull),
        ]
        .into_iter()
        .find(|(c, _)| v.get("cmd").and_then(|c| c.as_str()) == Some(*c))?;
        let args = v.get("args")?.clone();
        let ticket = args.get("ticket")?.as_str()?.to_string();
        let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
        let plan = Plan {
            verb,
            cmd,
            args: args.clone(),
            names: serde_json::from_value(v.get("names")?.clone()).ok()?,
            into: v.get("into").and_then(|i| i.as_str()).map(str::to_string),
            machine: machine.to_string(),
        };
        let b = Self::new(id, h, plan, ticket, on_event);
        if let Some(p) = &b.pulling {
            // 做完的那几项照落盘的记着，接回来不重下。
            let kept: Vec<Option<BatchOutcome>> =
                serde_json::from_value(v.get("outcomes")?.clone()).ok()?;
            if kept.len() != b.plan.names.len() {
                return None;
            }
            *lock(&p.outcomes) = kept;
        }
        *lock(&b.sent) = Some(args);
        lock(&b.tick).0 = Tick {
            done: n("done"),
            total: n("total"),
            bytes: None,
        };
        Some(Self { kept: true, ..b })
    }

    /// 跑这一组（[`batch_core::run`]）：每次发之前入参变了 ⇒ 落盘那一份跟着换；收场 ⇒ 删。
    /// 一开工先摆「在做」那一格（那台可能很久才报第一格进度）；接回来的那一组先摆断开那一行。
    pub async fn go<C, CF, U, UF>(&self, call: C, wait_up: U)
    where
        C: Fn(serde_json::Value) -> CF,
        CF: std::future::Future<Output = Result<serde_json::Value, Rebuff>>,
        U: Fn() -> UF,
        UF: std::future::Future<Output = ()>,
    {
        self.emit(&if self.kept {
            Phase::Cut(batch_core::cut_line(&self.plan, &self.tick()))
        } else {
            Phase::Running
        });
        let sending = |args: serde_json::Value| {
            let changed = lock(&self.sent).as_ref() != Some(&args);
            if changed {
                *lock(&self.sent) = Some(args.clone());
                self.keep(true);
            }
            call(args)
        };
        batch_core::run(self, sending, wait_up).await;
        self.keep(false);
    }

    /// 交 Kotlin 落盘（`batch_keep {ticket, plan}`）：`plan` 是一串字，原样存、原样交回 [`start`]（`{kept: plan}`）；收场 ⇒ `plan: null`。
    fn keep(&self, on: bool) {
        let plan = on.then(|| {
            let t = self.tick();
            let mut blob = serde_json::json!({
                "cmd": self.plan.cmd,
                "args": lock(&self.sent).clone(),
                "names": self.plan.names,
                "into": self.plan.into,
                "done": t.done,
                "total": t.total,
            });
            if let Some(p) = &self.pulling {
                blob["outcomes"] = serde_json::to_value(&*lock(&p.outcomes)).unwrap_or_default();
            }
            blob.to_string()
        });
        let body = BatchKeep {
            ticket: self.ticket.clone(),
            plan,
        };
        (self.on_event)(
            self.h,
            "batch_keep",
            serde_json::to_string(&body).unwrap_or_default(),
        );
    }

    /// 一格进度（帧那一侧）：`{done, total, current?, bytes_done?, bytes_total?}`。
    pub fn progress(&self, cell: &serde_json::Value) {
        let Some(t) = Tick::of_cell(cell) else { return };
        let current = cell
            .get("current")
            .and_then(|c| c.as_str())
            .map(str::to_string);
        *lock(&self.tick) = (t, current);
        self.emit(&Phase::Running);
    }

    /// 同名那一问答了（[`decided`] 那一形）· 本机那一份开好了（[`LocalOpened`]，下到本机那一组在等它）。回值 ＝ 真有人在等这一答。
    pub fn answer(&self, text: Option<&str>) -> bool {
        let opened = text.and_then(|t| serde_json::from_str::<LocalOpened>(t).ok());
        if let (Some(p), Some(got)) = (&self.pulling, opened) {
            let for_key = match &got {
                LocalOpened::Fd { key, .. } | LocalOpened::Refused { key, .. } => key.clone(),
            };
            let tx = {
                let mut opening = lock(&p.opening);
                match opening.as_ref() {
                    Some((key, _)) if *key == for_key => opening.take().map(|(_, tx)| tx),
                    _ => None,
                }
            };
            let unsent = match tx {
                Some(tx) => match tx.send(got) {
                    Ok(()) => return true,
                    Err(got) => got,
                },
                None => got,
            };
            // 没人在等（撤了 · 不是这一项）：交来的描述符在这里关掉，不漏。
            if let LocalOpened::Fd { fd, .. } = unsent {
                drop(i32::try_from(fd).ok().and_then(crate::file_of));
            }
            return false;
        }
        let conflicts = std::mem::take(&mut *lock(&self.asking));
        let map = decided(conflicts.iter().map(|c| c.key.as_str()), text);
        lock(&self.answer)
            .take()
            .is_some_and(|tx| tx.send(map).is_ok())
    }

    fn emit(&self, phase: &Phase) {
        (self.on_event)(
            self.h,
            "batch",
            serde_json::to_string(&self.event(phase)).unwrap_or_default(),
        );
    }

    /// 下到本机那一组：做完了几项（有结局的）· 其中成了几项。
    fn settled(&self) -> Option<(usize, usize)> {
        let p = self.pulling.as_ref()?;
        let o = lock(&p.outcomes);
        let ok = o
            .iter()
            .flatten()
            .filter(|o| o.state == ItemState::Done)
            .count();
        Some((o.iter().flatten().count(), ok))
    }

    /// 交 Kotlin 的那一行（字全写好）。
    pub fn event(&self, phase: &Phase) -> BatchLine {
        let (t, current) = lock(&self.tick).clone();
        let (id, title, kept) = (self.id, batch_core::title_of(&self.plan), self.kept);
        let n = self.plan.names.len();
        match phase {
            Phase::Running => match (&self.pulling, self.settled()) {
                // 下到本机：做完的项数连同手上那一项下到几成。
                (Some(p), Some((done, _))) => {
                    let part = lock(&p.part).clone();
                    BatchLine::Running {
                        id,
                        title,
                        kept,
                        frac: Some(
                            ((done as f32 + part.as_ref().map_or(0.0, |(_, f)| *f))
                                / n.max(1) as f32)
                                .min(1.0),
                        ),
                        current: part.map(|(name, _)| name),
                    }
                }
                _ => BatchLine::Running {
                    id,
                    title,
                    kept,
                    current,
                    frac: t.frac(),
                },
            },
            Phase::Asking => BatchLine::Asking {
                id,
                title,
                kept,
                conflicts: match PullArgs::of(&self.plan.args) {
                    Some(a) if self.pulling.is_some() => local_clashes(&a.names(), &a.here),
                    _ => clash_rows(&lock(&self.asking)),
                },
                go: self.plan.verb.text(),
            },
            Phase::Cut(line) => BatchLine::Cut {
                id,
                title,
                kept,
                // 下到本机：完成几项 · 中断几项（没下完的连回来整份重下）。
                line: match self.settled() {
                    Some((done, ok)) => batch_core::cut_left_line(&self.plan, ok, n - done),
                    None => line.clone(),
                },
            },
            Phase::Ended(r) => {
                let failed = batch_core::failed_lines(&self.plan, r);
                BatchLine::Ended {
                    id,
                    title,
                    kept,
                    said: r.said.clone(),
                    failed_title: copy_core::copy_text(
                        "beFiles.batch.segBad",
                        &[("bad", &failed.len().to_string())],
                    ),
                    failed,
                    details: batch_core::copy_details(&self.plan, r),
                    retry: batch_core::retry_plan(&self.plan, r).is_some(),
                    undo: batch_core::undo_plan(&self.plan, r).is_some(),
                }
            }
            Phase::Failed(f) => BatchLine::Failed {
                id,
                title,
                kept,
                said: f.said.clone(),
                details: Failed::copy_body(&f.said, &f.detail),
            },
            Phase::Dropped | Phase::Opened => BatchLine::Dropped { id, title, kept },
        }
    }
}

impl PhoneBoard {
    /// 交 Kotlin 一件本机那一侧的事（`batch_local`）。
    fn local(&self, ask: &LocalAsk) {
        (self.on_event)(
            self.h,
            "batch_local",
            serde_json::to_string(ask).unwrap_or_default(),
        );
    }

    /// 撤了（［停］）：下到本机那一组的那一圈当场停。
    fn halt(&self) -> Option<Arc<Halt>> {
        self.pulling.as_ref().map(|p| p.halt.clone())
    }

    /// 一项有了结局 ⇒ 记下、落盘那一份跟着换（进程没了再开，做完的不重下）。
    fn settle(&self, i: usize, o: BatchOutcome) {
        if let Some(p) = &self.pulling {
            if let Some(slot) = lock(&p.outcomes).get_mut(i) {
                *slot = Some(o);
            }
            *lock(&p.part) = None;
        }
        self.keep(true);
    }

    /// 向 Kotlin 要本机那一份（`batch_local {open}`），等它经 [`answer`] 回；撤了 ⇒ `Err(None)`，开不了 ⇒ 那一句 · 系统原话。
    async fn open(
        &self,
        p: &Pulling,
        folder: &str,
        key: &str,
        name: &str,
        over: Option<String>,
    ) -> Result<std::fs::File, Option<(String, String)>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        *lock(&p.opening) = Some((key.to_string(), tx));
        self.local(&LocalAsk::Open {
            id: self.id,
            key: key.to_string(),
            folder: folder.to_string(),
            name: name.to_string(),
            over,
        });
        let got = tokio::select! {
            got = rx => got.ok(),
            () = p.halt.hit() => None,
        };
        lock(&p.opening).take();
        let said = || copy_core::copy_text("mobFiles.local.writeFailed", &[("path", name)]);
        match got {
            Some(LocalOpened::Fd { fd, .. }) => i32::try_from(fd)
                .ok()
                .and_then(crate::file_of)
                .ok_or_else(|| Some((said(), format!("fd {fd}")))),
            Some(LocalOpened::Refused { detail, .. }) => Err(Some((said(), detail))),
            None => {
                // 撤在开的路上：Kotlin 那一侧可能已经开了那一份 ⇒ 照删。
                self.local(&LocalAsk::Drop {
                    id: self.id,
                    key: key.to_string(),
                });
                Err(None)
            }
        }
    }

    /// 🔴 下到本机那一组「问一次」（[`batch_core::run`] 的 `ask`：入参带票，答过同名之后带 `decide`）——
    /// 本机有同名而还没答 ⇒ 照后端同名那一形拒（`exists` ＋ 同名清单，抽屉照画）；不然从第一项没结局的起逐项拉（`ask` 是逐块问那台的那一口）：
    /// 答了跳过 ⇒ 记跳过；开不了 · 拉不成 ⇒ 记失败（开了的那一份删）；断了 ⇒ 回「断了」（手上那一项删掉、连回来整份重下）；
    /// 撤了 ⇒ 手上那一项删掉、没轮到的记撤了。收场回一组的结局（结局句照 [`batch_core::said_of`]）。
    pub async fn pull_round<A: Ask<Fail = String>>(
        &self,
        args: &serde_json::Value,
        ask: &A,
    ) -> Result<serde_json::Value, Rebuff> {
        let unreadable = |detail: String| Rebuff {
            failed: Failed {
                code: None,
                said: copy_core::copy_text("reason.content.unparsable", &[]),
                detail,
            },
            data: None,
            cut: false,
        };
        let (Some(p), Some(a)) = (&self.pulling, PullArgs::of(args)) else {
            return Err(unreadable(args.to_string()));
        };
        let rows = local_clashes(&a.names(), &a.here);
        let decide: Option<BTreeMap<String, Decide>> = args
            .get("decide")
            .and_then(|d| serde_json::from_value(d.clone()).ok());
        let fresh = lock(&p.outcomes).iter().all(Option::is_none);
        if decide.is_none() && fresh && !rows.is_empty() {
            let conflicts: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    serde_json::json!({ "key": r.key, "name": WirePath(r.name.as_bytes().to_vec()),
                        "kind": "file", "there": "file", "answers": r.answers })
                })
                .collect();
            return Err(Rebuff {
                failed: Failed {
                    code: Some(batch_core::EXISTS.into()),
                    said: String::new(),
                    detail: String::new(),
                },
                data: Some(serde_json::json!({ "conflicts": conflicts })),
                cut: false,
            });
        }
        let one = |key: &str, state: ItemState| BatchOutcome::bare(key, state);
        for (i, (path, name)) in a.items.iter().enumerate() {
            if lock(&p.outcomes).get(i).is_some_and(Option::is_some) {
                continue;
            }
            if p.halt.is() {
                break;
            }
            let key = i.to_string();
            let row = rows.iter().find(|r| r.key == key);
            let d = row.and_then(|_| decide.as_ref()?.get(&key).copied());
            if d == Some(Decide::Skip) {
                self.settle(i, one(&key, ItemState::Skipped));
                continue;
            }
            let over = (d == Some(Decide::Overwrite))
                .then(|| row.and_then(|r| r.there.clone()))
                .flatten();
            *lock(&p.part) = Some((name.clone(), 0.0));
            self.emit(&Phase::Running);
            let file = match self.open(p, &a.folder, &key, name, over).await {
                Ok(f) => f,
                Err(Some((said, detail))) => {
                    self.settle(
                        i,
                        BatchOutcome {
                            reason: Some(said),
                            detail: Some(detail),
                            ..one(&key, ItemState::Failed)
                        },
                    );
                    continue;
                }
                Err(None) => break,
            };
            let mut moves = PartMoves { board: self, p, name };
            let line =
                transfer::pull_with(ask, self.id, path.clone(), file, name, &mut moves).await;
            match pulled(&line, p.halt.is()) {
                Pulled::Done(bytes) => self.settle(
                    i,
                    BatchOutcome {
                        bytes,
                        ..one(&key, ItemState::Done)
                    },
                ),
                Pulled::Failed(said, detail) => {
                    self.local(&LocalAsk::Drop { id: self.id, key: key.clone() });
                    self.settle(
                        i,
                        BatchOutcome {
                            reason: Some(said),
                            detail: Some(detail),
                            ..one(&key, ItemState::Failed)
                        },
                    );
                }
                Pulled::Cut => {
                    self.local(&LocalAsk::Drop { id: self.id, key });
                    *lock(&p.part) = None;
                    return Err(rebuff_of(
                        &self.plan.machine,
                        self.plan.cmd,
                        CallError::Disconnected,
                    ));
                }
                Pulled::Stopped => {
                    self.local(&LocalAsk::Drop { id: self.id, key });
                    break;
                }
            }
        }
        let outcomes: Vec<BatchOutcome> = {
            let mut o = lock(&p.outcomes);
            for (i, slot) in o.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(one(&i.to_string(), ItemState::Cancelled));
                }
            }
            o.iter().flatten().cloned().collect()
        };
        *lock(&p.part) = None;
        let said = batch_core::said_of(&self.plan.verb.text(), &outcomes, None);
        serde_json::to_value(BatchReply { outcomes, said }).map_err(|e| unreadable(e.to_string()))
    }
}

/// 一项拉完那一行（[`CallEnd`]）⇒ 成了 · 没成（那一句 · 详情）· 断了 · 撤了。
fn pulled(line: &str, halted: bool) -> Pulled {
    match serde_json::from_str::<CallEnd>(line) {
        Ok(CallEnd::Ok { data, .. }) => Pulled::Done(data.get("bytes").and_then(|b| b.as_u64())),
        Ok(CallEnd::Failed {
            message, detail, ..
        }) => Pulled::Failed(message, detail),
        Ok(CallEnd::Cancelled { .. }) if halted => Pulled::Stopped,
        Ok(CallEnd::Timeout { .. } | CallEnd::Down { .. } | CallEnd::Cancelled { .. }) => {
            Pulled::Cut
        }
        Ok(CallEnd::Unreadable { detail, .. }) => Pulled::Failed(
            copy_core::copy_text("reason.content.unparsable", &[]),
            detail,
        ),
        Err(e) => Pulled::Failed(
            copy_core::copy_text("reason.content.unparsable", &[]),
            format!("{e}: {line}"),
        ),
    }
}

/// 手上那一项下到几成（每一块报一次 ⇒ 那一格跟着换）；撤了 ⇒ 下一块不问。
struct PartMoves<'a> {
    board: &'a PhoneBoard,
    p: &'a Pulling,
    name: &'a str,
}

impl chunk_core::Watch for PartMoves<'_> {
    fn stopped(&self) -> bool {
        self.p.halt.is()
    }
    fn moved(&mut self, done: u64, total: Option<u64>) {
        if let Some(t) = total.filter(|t| *t > 0) {
            *lock(&self.p.part) = Some((self.name.to_string(), (done as f32 / t as f32).min(1.0)));
            self.board.emit(&Phase::Running);
        }
    }
}

impl Board for PhoneBoard {
    fn plan(&self) -> Plan {
        self.plan.clone()
    }

    fn ticket(&self) -> String {
        self.ticket.clone()
    }

    fn tick(&self) -> Tick {
        match self.settled() {
            Some((done, _)) => Tick {
                done: done as u64,
                total: self.plan.names.len() as u64,
                bytes: None,
            },
            None => lock(&self.tick).0,
        }
    }

    fn set(&self, phase: Phase) {
        match &phase {
            // 同名那一问由 [`Board::ask`] 带着那几项交（这一步先落、那几项还没摆上）。
            Phase::Asking => return,
            Phase::Ended(r) => *lock(&self.ended) = Some(r.clone()),
            // 断开时的完成数跟着落盘（进程没了再开，那一行照它写）。
            Phase::Cut(_) => self.keep(true),
            _ => {}
        }
        self.emit(&phase);
    }

    fn ask(
        &self,
        conflicts: Vec<Conflict>,
    ) -> impl std::future::Future<Output = Option<BTreeMap<String, Decide>>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        *lock(&self.asking) = conflicts;
        *lock(&self.answer) = Some(tx);
        self.emit(&Phase::Asking);
        async move { rx.await.ok().flatten() }
    }
}

/// 同名那一问的几行（抽屉照画）：键 · 名字 · 那一行的说明（核心写好）· 这一项能怎么答（那台给的）。
pub fn clash_rows(conflicts: &[Conflict]) -> Vec<ClashRow> {
    conflicts
        .iter()
        .map(|c| ClashRow {
            key: c.key.clone(),
            name: String::from_utf8_lossy(c.name.bytes()).into_owned(),
            meta: batch_core::conflict_meta(c),
            answers: c.answers.clone(),
            there: None,
        })
        .collect()
}

/// 本机同名那一步：要下的名字里，文件夹里已有（不分大小写：手机的共享存储不分）的那几项 ⇒ 抽屉那几行（只有名字一格；目录不下，恒给覆盖）。
fn local_clashes(names: &[String], here: &[String]) -> Vec<ClashRow> {
    let answers = [Decide::Overwrite, Decide::Skip, Decide::KeepBoth];
    names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let there = here.iter().find(|h| h.to_lowercase() == n.to_lowercase())?;
            Some(ClashRow {
                key: i.to_string(),
                name: n.clone(),
                there: Some(there.clone()),
                meta: String::new(),
                answers: answers.to_vec(),
            })
        })
        .collect()
}

/// 在做的 · 做完还留着（重试 · 撤销照它另起）的那几组，按编号。
fn boards() -> &'static Mutex<HashMap<i64, Arc<PhoneBoard>>> {
    static B: OnceLock<Mutex<HashMap<i64, Arc<PhoneBoard>>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

/// 做完还留着几组（更早的那几组不再给重试 · 撤销）。
const KEPT: usize = 16;

fn board(id: i64) -> Option<Arc<PhoneBoard>> {
    lock(boards()).get(&id).cloned()
}

/// Kotlin 交的「要做什么」⇒ 一组的计划（入参在这里拼）。`machine` 写进断开那一行。读不动 · 那一组已经不在 ⇒ `None`。
///
/// - `{delete: {root, items: [{rel, name, dir}]}}`：删（目录整棵）；
/// - `{chmod: {root, items: [{rel, name}], mode}}`：改权限（八进制串）；
/// - `{copy: {from, names: [..], to}}`：复制到 `to`（公共根同桌面）；
/// - `{download: {folder, items: [{path, name}], here: [..]}}`：下到本机 `folder` 那一棵（`here` ＝ 那里已有的名字，同名照它问）；
/// - `{retry: id}` · `{undo: id}`：那一组没成的那几项再做一次 · 撤销那一组改的权限（撤销可能是几组：每个改之前的值一组，交第一组）。
pub fn plan_of(want: &serde_json::Value, machine: &str) -> Option<Vec<Plan>> {
    let o = want.as_object()?;
    let (k, v) = o.iter().next()?;
    let names = |items: &[serde_json::Value]| -> Vec<String> {
        items
            .iter()
            .map(|i| {
                i.get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    };
    let simple = |verb: Verb,
                  cmd: &'static str,
                  args: serde_json::Value,
                  items: &[serde_json::Value]| Plan {
        verb,
        cmd,
        args,
        names: names(items),
        into: None,
        machine: machine.to_string(),
    };
    Some(match k.as_str() {
        "delete" => {
            let items = v.get("items")?.as_array()?;
            let wire: Vec<serde_json::Value> = items
                .iter()
                .map(|i| {
                    let mut w =
                        serde_json::json!({ "rel": i.get("rel").cloned().unwrap_or_default() });
                    if i.get("dir").and_then(|d| d.as_bool()) == Some(true) {
                        w["recursive"] = true.into();
                    }
                    w
                })
                .collect();
            vec![simple(
                Verb::Delete,
                <cmd_wire::files::FilesDeleteReq as Call>::NAME,
                serde_json::json!({ "root": v.get("root")?, "items": wire }),
                items,
            )]
        }
        "chmod" => {
            let items = v.get("items")?.as_array()?;
            let wire: Vec<serde_json::Value> = items
                .iter()
                .map(|i| serde_json::json!({ "rel": i.get("rel").cloned().unwrap_or_default() }))
                .collect();
            vec![simple(
                Verb::Chmod,
                <cmd_wire::files::FilesChmodReq as Call>::NAME,
                serde_json::json!({ "root": v.get("root")?, "items": wire, "mode": v.get("mode")?.as_str()? }),
                items,
            )]
        }
        "copy" => {
            let path = |k: &str| serde_json::from_value::<WirePath>(v.get(k)?.clone()).ok();
            let names: Vec<Vec<u8>> = v
                .get("names")?
                .as_array()?
                .iter()
                .map(|n| {
                    serde_json::from_value::<WirePath>(n.clone())
                        .ok()
                        .map(|p| p.0)
                })
                .collect::<Option<_>>()?;
            vec![batch_core::copy_plan(&path("from")?.0, &names, &path("to")?.0, machine).ok()?]
        }
        "download" => {
            let a = PullArgs::of(v)?;
            vec![Plan {
                verb: Verb::Pull,
                cmd: <cmd_wire::files::ReadChunkReq as Call>::NAME,
                args: serde_json::json!({
                    "items": a.items.iter().map(|(path, name)| serde_json::json!({ "path": path, "name": name })).collect::<Vec<_>>(),
                    "folder": a.folder,
                    "here": a.here,
                }),
                names: a.names(),
                into: None,
                machine: machine.to_string(),
            }]
        }
        "retry" | "undo" => {
            let b = board(v.as_i64()?)?;
            let ended = lock(&b.ended).clone()?;
            if k == "retry" {
                vec![batch_core::retry_plan(&b.plan, &ended)?]
            } else {
                batch_core::undo_plan(&b.plan, &ended)?
            }
        }
        _ => return None,
    })
}

/// 问那台一次（带一个期限）：对端拒 ⇒ 码 · 那一句 · 详情 · `data`；流没通 · 到点 ⇒ 断了（同一张票再问）。
fn rebuff_of(machine: &str, cmd: &str, e: CallError) -> Rebuff {
    let cut = matches!(
        e,
        CallError::Disconnected | CallError::Timeout { .. } | CallError::Cancelled
    );
    if let CallError::Remote {
        code,
        message,
        detail,
        data,
    } = e
    {
        return Rebuff {
            failed: Failed {
                code: Some(code),
                said: message,
                detail,
            },
            data: data.as_deref().and_then(|d| serde_json::from_str(d).ok()),
            cut: false,
        };
    }
    // 别的没走通那几种：那一行的字照一问一答那一份（`reply_json`）。
    let line: serde_json::Value =
        serde_json::from_str(&resident::reply_json(0, machine, cmd, Err(e))).unwrap_or_default();
    let said = line
        .get("message")
        .and_then(|m| m.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| comms_inward::chan::said::offline(machine));
    Rebuff {
        failed: Failed {
            code: None,
            said,
            detail: line
                .get("detail")
                .and_then(|d| d.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        data: None,
        cut,
    }
}

/// 起一组：回编号（进度 · 同名那一问 · 结局经 `on_event` 的 `batch` 那一种交，落盘那一份经 `batch_keep`）；
/// `{kept: plan}` ⇒ 接回落盘的那一组（同一张票再问）。这台不在 · 读不动 ⇒ 0。
pub fn start(h: i64, want: &str, on_event: OnEvent) -> i64 {
    let Some(f) = resident::feed(h) else { return 0 };
    let Ok(want) = serde_json::from_str::<serde_json::Value>(want) else {
        return 0;
    };
    if let Some(kept) = want.get("kept").and_then(|k| k.as_str()) {
        let id = resident::next_call();
        return match PhoneBoard::resumed(id, h, kept, f.machine(), on_event) {
            Some(b) => launch(Arc::new(b)),
            None => 0,
        };
    }
    let Some(plans) = plan_of(&want, f.machine()) else {
        return 0;
    };
    let mut first = 0;
    for plan in plans {
        let mut random = [0u8; 16];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random);
        let b = PhoneBoard::new(
            resident::next_call(),
            h,
            plan,
            chunk_core::key_of(random),
            on_event.clone(),
        );
        let id = launch(Arc::new(b));
        if first == 0 {
            first = id;
        }
    }
    first
}

/// 记下这一组（重试 · 撤销照它另起）、在运行时上跑起来（问那台走常驻流那一条）；回编号。
fn launch(b: Arc<PhoneBoard>) -> i64 {
    let (id, h) = (b.id, b.h);
    {
        let mut all = lock(boards());
        if all.len() >= KEPT {
            if let Some(old) = all.keys().min().copied() {
                all.remove(&old);
            }
        }
        all.insert(id, b.clone());
    }
    let busy = Busy::hold(h, b.on_event.clone());
    tokio::spawn(async move {
        let _busy = busy;
        let cmd = b.plan.cmd;
        // 等那台连回来（断开那一支）；下到本机那一组撤了也不再等。
        let wait_up = || {
            let halt = b.halt();
            async move {
                let up = async {
                    if let Some(f) = resident::feed(h) {
                        let far = tokio::time::Instant::now() + batch_core::BATCH_BUDGET;
                        let _ = f.client_by(far).await;
                    }
                };
                match halt {
                    Some(halt) => tokio::select! {
                        () = up => {},
                        () = halt.hit() => {},
                    },
                    None => up.await,
                }
            }
        };
        if let Some(halt) = b.halt() {
            // 下到本机：「问那台」那一口是本机那一圈，逐块问那台走常驻流那一条（每块一个期限）。
            let ask = transfer::Halting {
                ask: transfer::over(h, b.id, PULL_CHUNK),
                halt,
                id: b.id,
            };
            let ask = &ask;
            let me = &b;
            b.go(
                |args| async move { me.pull_round(&args, ask).await },
                wait_up,
            )
            .await;
            return;
        }
        b.go(
            |args| async move {
                let Some(f) = resident::feed(h) else {
                    return Err(rebuff_of("", cmd, CallError::Cancelled));
                };
                let deadline = tokio::time::Instant::now() + batch_core::BATCH_BUDGET;
                let got = match f.client_by(deadline).await {
                    Some(c) => c.call_until(cmd, args, deadline).await,
                    None => Err(CallError::Disconnected),
                };
                got.map(|v| v.unwrap_or_default())
                    .map_err(|e| rebuff_of(f.machine(), cmd, e))
            },
            wait_up,
        )
        .await;
    });
    id
}

/// 下到本机那一组逐块问那台，每一块等多久。
const PULL_CHUNK: std::time::Duration = std::time::Duration::from_secs(20);

/// 同名那一问的答（[`BatchAnswer`]：`{all: 答}` · `{each: {键: 答}}`；`null` · 读不动 ⇒ 取消）⇒ 每个键一格（`batch_core::answer_of`）。
pub fn decided<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    text: Option<&str>,
) -> Option<BTreeMap<String, Decide>> {
    let answer = text.and_then(|t| serde_json::from_str::<BatchAnswer>(t).ok());
    let (all, each) = match &answer {
        Some(BatchAnswer::All(d)) => (Some(*d), None),
        Some(BatchAnswer::Each(m)) => (None, Some(m)),
        None => (None, None),
    };
    batch_core::answer_of(keys, all, each)
}

/// 核心跑的那几组的答（[`decided`] 那一形）；那一组不在 ⇒ `false`。
pub fn answer(id: i64, text: Option<&str>) -> bool {
    board(id).is_some_and(|b| b.answer(text))
}

/// 手机那一侧问的同名（下载 · 上传）：`keys` 是那几行的键（JSON 数组）⇒ 每个键一格（JSON）；取消 · 读不动 ⇒ `null`。
pub fn decided_json(keys: &str, text: Option<&str>) -> String {
    let keys: Vec<String> = serde_json::from_str(keys).unwrap_or_default();
    serde_json::to_string(&decided(keys.iter().map(String::as_str), text)).unwrap_or_default()
}

/// 撤这一组（「停」）：同一张票发 `files-batch-stop`；那台做完手上那一项就停，剩下的回 `cancelled`。
/// 下到本机那一组：本机那一圈当场停（手上那一项删掉、没轮到的记撤了）；同名那一问摆着 ⇒ 当取消。
pub fn stop(id: i64) -> bool {
    let Some(b) = board(id) else { return false };
    if let Some(halt) = b.halt() {
        halt.set();
        if let Some(tx) = lock(&b.answer).take() {
            let _ = tx.send(None);
        }
        return true;
    }
    let Some(f) = resident::feed(b.h) else {
        return false;
    };
    let ticket = b.ticket.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        if let Some(c) = f.client_by(deadline).await {
            let _ = c
                .call_until(
                    batch_core::CMD_STOP,
                    serde_json::json!({ "ticket": ticket }),
                    deadline,
                )
                .await;
        }
    });
    true
}

/// 流上来的一帧：是在做的某一组的进度（`files_batch`，按票）⇒ 这里接了、回 `true`（不交 Kotlin 的帧流）。
pub fn took(line: &str) -> bool {
    if !line.contains("\"files_batch\"") {
        return false;
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return false;
    };
    if v.get("type").and_then(|t| t.as_str()) != Some("files_batch") {
        return false;
    }
    let Some(t) = v.get("ticket").and_then(|t| t.as_str()) else {
        return true;
    };
    let hit = lock(boards()).values().find(|b| b.ticket == t).cloned();
    if let Some(b) = hit {
        b.progress(&v);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events() -> (OnEvent, Arc<Mutex<Vec<serde_json::Value>>>) {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let s2 = seen.clone();
        let on: OnEvent = Arc::new(move |_, kind, body| {
            if kind == "batch" {
                lock(&s2).push(serde_json::from_str(&body).unwrap());
            }
        });
        (on, seen)
    }

    fn del(machine: &str) -> Plan {
        plan_of(
            &serde_json::json!({"delete": {"root": "/w", "items": [
                {"rel": "a.txt", "name": "a.txt"}, {"rel": {"b16": "2fff"}, "name": "d", "dir": true}]}}),
            machine,
        )
        .unwrap()
        .remove(0)
    }

    /// 删 · 改权限 · 复制：入参在核心里拼（Kotlin 只交要做什么）；目录那一项整棵删。
    #[test]
    fn the_wants_become_the_wire_args() {
        assert_eq!(
            del("devbox").args,
            serde_json::json!({"root": "/w", "items": [{"rel": "a.txt"}, {"rel": {"b16": "2fff"}, "recursive": true}]})
        );
        let c = plan_of(
            &serde_json::json!({"chmod": {"root": "/w", "items": [{"rel": "a", "name": "a"}], "mode": "644"}}),
            "devbox",
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            c.args,
            serde_json::json!({"root": "/w", "items": [{"rel": "a"}], "mode": "644"})
        );
        assert_eq!(c.cmd, "files-chmod");
        let p = plan_of(
            &serde_json::json!({"copy": {"from": "/w/a", "names": ["x"], "to": "/w/b"}}),
            "devbox",
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            p.args,
            serde_json::json!({"root": "/w", "items": [{"from": "a/x", "to": "b/x"}]})
        );
        assert!(plan_of(&serde_json::json!({"nope": {}}), "devbox").is_none());
        // 带上票（发出去的那一形）照后端的入参类型解得开。
        let ticketed = |p: &Plan| {
            let mut a = p.args.clone();
            a["ticket"] = "t-1".into();
            a
        };
        serde_json::from_value::<cmd_wire::files::FilesDeleteReq>(ticketed(&del("devbox")))
            .unwrap();
        serde_json::from_value::<cmd_wire::files::FilesChmodReq>(ticketed(&c)).unwrap();
        let mut answered = ticketed(&p);
        answered["decide"] = serde_json::json!({"0": "keep_both"});
        serde_json::from_value::<cmd_wire::files::FilesCopyReq>(answered).unwrap();
        serde_json::from_value::<cmd_wire::batch::FilesBatchStopReq>(
            serde_json::json!({"ticket": "t-1"}),
        )
        .unwrap();
    }

    fn reply(said: &str) -> serde_json::Value {
        serde_json::json!({"outcomes": [{"key": "0", "state": "done"}, {"key": "1", "state": "failed", "reason": "无权限"}], "said": said})
    }

    /// 🔴 带票发；回 `exists` ⇒ 交出 `asking`（每项那一行的字核心写好）、答了带 `decide` 同票再发；断了 ⇒ `cut` 那一行、连回来同票再问；
    /// 结局 ⇒ `ended` 带后端那一句与没成的那几行、给重试。
    #[tokio::test]
    async fn a_group_asks_once_waits_out_a_cut_and_keeps_its_ticket() {
        let (on, seen) = events();
        let b = Arc::new(PhoneBoard::new(7, 1, del("devbox"), "t-1".into(), on));
        let sent: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let (s2, b2) = (sent.clone(), b.clone());
        let answering = tokio::spawn(async move {
            for _ in 0..2000 {
                if lock(&b2.answer).is_some() {
                    assert!(b2.answer(Some(r#"{"all":"keep_both"}"#)));
                    return;
                }
                tokio::task::yield_now().await;
            }
            panic!("等不到那一问");
        });
        batch_core::run(
            &*b,
            |args| {
                let n = {
                    let mut s = lock(&s2);
                    s.push(args);
                    s.len()
                };
                async move {
                    match n {
                        1 => Err(Rebuff {
                            failed: Failed { code: Some("exists".into()), said: "同名 1 项".into(), detail: String::new() },
                            data: Some(serde_json::json!({"conflicts": [{"key": "0", "name": "a.txt", "kind": "file",
                                "size_text": "3 字节", "there": "file", "there_size_text": "1 字节", "answers": ["overwrite", "skip", "keep_both"]}]})),
                            cut: false,
                        }),
                        2 => Err(rebuff_of("devbox", "files-delete", CallError::Disconnected)),
                        _ => Ok(reply("删除 2 项 · 完成 1 · 失败 ×1")),
                    }
                }
            },
            || async {},
        )
        .await;
        answering.await.unwrap();
        let sent = lock(&sent).clone();
        assert_eq!(sent.len(), 3);
        assert!(
            sent.iter().all(|a| a["ticket"] == "t-1"),
            "同一张票：{sent:?}"
        );
        assert_eq!(sent[0].get("decide"), None);
        assert_eq!(sent[1]["decide"], serde_json::json!({"0": "keep_both"}));
        assert_eq!(sent[2]["decide"], sent[1]["decide"], "断了再问照带那一答");
        let seen = lock(&seen).clone();
        let phases: Vec<&str> = seen.iter().map(|e| e["phase"].as_str().unwrap()).collect();
        assert_eq!(phases, ["asking", "running", "cut", "running", "ended"]);
        assert_eq!(seen[0]["conflicts"][0]["meta"], batch_core::conflict_meta(&serde_json::from_value(
            serde_json::json!({"key": "0", "name": "a.txt", "kind": "file", "size_text": "3 字节", "there": "file", "there_size_text": "1 字节", "answers": ["overwrite", "skip", "keep_both"]})).unwrap()));
        assert!(seen[2]["line"].as_str().unwrap().starts_with("devbox"));
        let end = &seen[4];
        assert_eq!(
            end["said"], "删除 2 项 · 完成 1 · 失败 ×1",
            "结局句照后端那一句"
        );
        assert_eq!(end["failed"].as_array().unwrap().len(), 1);
        assert_eq!(end["retry"], true);
        assert_eq!(end["undo"], false);
    }

    /// 🔴 一组的字只在核心里拼：手机 Kotlin 那几棵一个 `beFiles.batch.` 键都不取（正控：共用那一份里有）。
    #[test]
    fn kotlin_spells_no_batch_sentence() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut hits = Vec::new();
        let mut seen = 0usize;
        let mut stack = vec![src.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    if !matches!(
                        p.file_name().and_then(|n| n.to_str()),
                        Some("build" | ".gradle")
                    ) {
                        stack.push(p);
                    }
                } else if p.extension().is_some_and(|x| x == "kt")
                    && p.to_string_lossy().contains("/src/main/")
                {
                    seen += 1;
                    if std::fs::read_to_string(&p)
                        .unwrap_or_default()
                        .contains("beFiles.batch.")
                    {
                        hits.push(p.display().to_string());
                    }
                }
            }
        }
        assert!(seen > 50, "Kotlin 生产源码一份都没扫到 —— 本条在空转");
        assert!(hits.is_empty(), "Kotlin 里拼了一组的字：{hits:?}");
        let core = std::fs::read_to_string(src.join("../common/batch-core/src/lib.rs")).unwrap();
        assert!(
            core.contains("\"beFiles.batch.segBad\""),
            "正控：共用那一份取这几个键"
        );
    }

    /// 替身那台（下到本机那一组逐块问它）：每个路径一份内容；`cut_once` 里的路径头一回问就断；`hang` 里的一直不回。
    #[derive(Default)]
    struct There {
        files: HashMap<String, Vec<u8>>,
        cut_once: Mutex<Vec<String>>,
        hang: Vec<String>,
    }

    impl There {
        fn of(files: &[(&str, &[u8])]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(p, b)| (p.to_string(), b.to_vec()))
                    .collect(),
                ..Self::default()
            }
        }
    }

    impl Ask for There {
        type Fail = String;
        async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
            let r: cmd_wire::files::ReadChunkReq =
                serde_json::from_value(serde_json::to_value(req).unwrap()).unwrap();
            let path = String::from_utf8(r.path.0.clone()).unwrap();
            let cut = {
                let mut c = lock(&self.cut_once);
                let hit = c.iter().position(|p| *p == path);
                hit.map(|i| c.remove(i)).is_some()
            };
            if cut {
                return crate::transfer::typed::<C>(
                    7,
                    "devbox",
                    Err(CallError::Disconnected),
                );
            }
            if self.hang.contains(&path) {
                std::future::pending::<()>().await;
            }
            let body = self.files.get(&path).cloned().unwrap_or_default();
            let a = (r.offset as usize).min(body.len());
            let b = (a + r.len as usize).min(body.len());
            let reply = cmd_wire::files::ReadChunkReply {
                path: r.path,
                offset: r.offset,
                size: body.len() as u64,
                eof: b == body.len(),
                content: cmd_wire::files::HexBytes(body[a..b].to_vec()),
            };
            crate::transfer::typed::<C>(
                7,
                "devbox",
                Ok(serde_json::from_value(serde_json::to_value(reply).unwrap())),
            )
        }
    }

    /// Kotlin 那一侧的替身：收 `batch_local`，开 ⇒ 在 `dir` 里建那一份（盖 `over` 那一份 · 新建）、把描述符交回；删 ⇒ 删那一份。
    /// `refuse` 里的名字开不了。记下每一次开（键 · 名字 · 盖哪一份）与每一次删（键）。
    #[derive(Default)]
    struct Here {
        opens: Mutex<Vec<(String, String, Option<String>)>>,
        drops: Mutex<Vec<String>>,
        made: Mutex<HashMap<String, std::path::PathBuf>>,
    }

    fn here_side(
        dir: std::path::PathBuf,
        refuse: &'static [&'static str],
    ) -> (
        OnEvent,
        Arc<Mutex<Vec<(String, serde_json::Value)>>>,
        Arc<Here>,
        Arc<Mutex<Option<Arc<PhoneBoard>>>>,
    ) {
        let seen: Arc<Mutex<Vec<(String, serde_json::Value)>>> = Arc::default();
        let here = Arc::new(Here::default());
        let board: Arc<Mutex<Option<Arc<PhoneBoard>>>> = Arc::default();
        let (s2, h2, b2) = (seen.clone(), here.clone(), board.clone());
        let on: OnEvent = Arc::new(move |_, kind, body| {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            lock(&s2).push((kind.to_string(), v));
            if kind != "batch_local" {
                return;
            }
            match serde_json::from_str::<LocalAsk>(&body).unwrap() {
                LocalAsk::Open { key, name, over, .. } => {
                    lock(&h2.opens).push((key.clone(), name.clone(), over.clone()));
                    let answer = if refuse.contains(&name.as_str()) {
                        LocalOpened::Refused {
                            key,
                            detail: "EACCES".into(),
                        }
                    } else {
                        let path = dir.join(over.unwrap_or(name));
                        let f = std::fs::File::create(&path).unwrap();
                        lock(&h2.made).insert(key.clone(), path);
                        use std::os::fd::IntoRawFd as _;
                        LocalOpened::Fd {
                            key,
                            fd: i64::from(f.into_raw_fd()),
                        }
                    };
                    let b = lock(&b2).clone().unwrap();
                    // 回话在另一条线上到（Kotlin 那一侧在它自己的线程上开）。
                    std::thread::spawn(move || {
                        assert!(b.answer(Some(&serde_json::to_string(&answer).unwrap())))
                    });
                }
                LocalAsk::Drop { key, .. } => {
                    if let Some(p) = lock(&h2.made).remove(&key) {
                        let _ = std::fs::remove_file(p);
                    }
                    lock(&h2.drops).push(key);
                }
            }
        });
        (on, seen, here, board)
    }

    fn folder(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pull-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn pull_plan(dir: &std::path::Path, here: &[&str]) -> Plan {
        plan_of(
            &serde_json::json!({"download": {"folder": dir.to_str().unwrap(), "here": here,
                "items": [{"path": "/w/a.txt", "name": "a.txt"}, {"path": "/w/b.md", "name": "b.md"}]}}),
            "devbox",
        )
        .expect("下到本机那一组读得懂")
        .remove(0)
    }

    /// 跑下到本机那一组（`batch_core::run` 那一圈，逐块问 `there`）；`wait_up` 不回 ⇒ 断了就停在那里（进程在这里被杀）。
    async fn pull_go(b: Arc<PhoneBoard>, there: Arc<There>, wait_up_never: bool) {
        let ask = Arc::new(crate::transfer::Halting {
            ask: ThereRef(there),
            halt: b.halt().unwrap(),
            id: b.id,
        });
        let b2 = b.clone();
        let call = move |args: serde_json::Value| {
            let (b3, ask) = (b2.clone(), ask.clone());
            async move { b3.pull_round(&args, &*ask).await }
        };
        if wait_up_never {
            b.go(call, std::future::pending::<()>).await;
        } else {
            b.go(call, || async {}).await;
        }
    }

    struct ThereRef(Arc<There>);

    impl Ask for ThereRef {
        type Fail = String;
        async fn ask<C: Call + Sync>(&self, req: &C) -> Result<C::Reply, String> {
            self.0.ask(req).await
        }
    }

    fn of_phase(seen: &[(String, serde_json::Value)]) -> Vec<String> {
        seen.iter()
            .filter(|(k, _)| k == "batch")
            .map(|(_, v)| v["phase"].as_str().unwrap().to_string())
            .collect()
    }

    /// 🔴 下到本机那一组落盘、被杀了照落盘那一份接回：头一项拉完、第二项拉到一半断了（手上那一份删掉）、进程在等连回来时没了 ⇒
    /// 再开照那一串接回（同一张票），先摆断开那一行（完成 1 · 中断 1），只重拉第二项，收场两项都在本机。
    #[tokio::test]
    async fn a_download_group_picks_up_from_what_it_kept() {
        let dir = folder("resume");
        let there = There::of(&[("/w/a.txt", b"AAA"), ("/w/b.md", b"BBBB")]);
        lock(&there.cut_once).push("/w/b.md".into());
        let (on, seen, here, slot) = here_side(dir.clone(), &[]);
        let b = Arc::new(PhoneBoard::new(7, 1, pull_plan(&dir, &[]), "t-1".into(), on));
        *lock(&slot) = Some(b.clone());
        let run = tokio::spawn(pull_go(b.clone(), Arc::new(there), true));
        for _ in 0..20_000 {
            if of_phase(&lock(&seen)).iter().any(|p| p == "cut") {
                break;
            }
            tokio::task::yield_now().await;
        }
        run.abort();
        assert_eq!(*lock(&here.drops), ["1"], "断了 ⇒ 手上那一份删掉");
        let kept = lock(&seen)
            .iter()
            .rev()
            .find(|(k, _)| k == "batch_keep")
            .and_then(|(_, v)| v["plan"].as_str().map(str::to_string))
            .expect("落了盘");

        let (on, seen, here, slot) = here_side(dir.clone(), &[]);
        let r = Arc::new(PhoneBoard::resumed(8, 2, &kept, "devbox", on).expect("落盘那一份读得回"));
        *lock(&slot) = Some(r.clone());
        let there = There::of(&[("/w/a.txt", b"XXX"), ("/w/b.md", b"BBBB")]);
        pull_go(r.clone(), Arc::new(there), false).await;
        assert_eq!(
            *lock(&here.opens),
            [("1".to_string(), "b.md".to_string(), None)],
            "做完的那一项不重下"
        );
        let phases = of_phase(&lock(&seen));
        assert_eq!(phases.first().map(String::as_str), Some("cut"));
        assert_eq!(phases.last().map(String::as_str), Some("ended"));
        let shown = lock(&seen).clone();
        let first = &shown.iter().find(|(k, _)| k == "batch").unwrap().1;
        assert_eq!(
            first["line"],
            batch_core::cut_left_line(&r.plan, 1, 1),
            "完成 1 · 中断 1"
        );
        let end = lock(&r.ended).clone().unwrap();
        assert!(end.outcomes.iter().all(|o| o.state == ItemState::Done), "{end:?}");
        assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"AAA");
        assert_eq!(std::fs::read(dir.join("b.md")).unwrap(), b"BBBB");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 本机同名问一次（抽屉那几行带盘上撞的那一份）：答了覆盖 ⇒ 开的时候盖那一份；没点的那一项照跳过；结局照后端那一形写。
    #[tokio::test]
    async fn a_local_clash_is_asked_once_and_its_answer_picks_what_to_open() {
        let dir = folder("clash");
        let (on, seen, here, slot) = here_side(dir.clone(), &[]);
        let b = Arc::new(PhoneBoard::new(7, 1, pull_plan(&dir, &["A.TXT", "b.md"]), "t".into(), on));
        *lock(&slot) = Some(b.clone());
        let b2 = b.clone();
        let answering = tokio::spawn(async move {
            for _ in 0..20_000 {
                if lock(&b2.answer).is_some() {
                    assert!(b2.answer(Some(r#"{"each":{"0":"overwrite"}}"#)));
                    return;
                }
                tokio::task::yield_now().await;
            }
            panic!("等不到那一问");
        });
        let there = There::of(&[("/w/a.txt", b"AAA"), ("/w/b.md", b"BBBB")]);
        pull_go(b.clone(), Arc::new(there), false).await;
        answering.await.unwrap();
        let shown = lock(&seen).clone();
        let ask = &shown.iter().find(|(k, v)| k == "batch" && v["phase"] == "asking").unwrap().1;
        assert_eq!(ask["conflicts"][0]["there"], "A.TXT");
        assert_eq!(ask["conflicts"][1]["there"], "b.md");
        assert_eq!(ask["title"], "下载 2 项");
        assert_eq!(
            *lock(&here.opens),
            [("0".to_string(), "a.txt".to_string(), Some("A.TXT".to_string()))]
        );
        let end = lock(&b.ended).clone().unwrap();
        assert_eq!(
            end.outcomes.iter().map(|o| o.state).collect::<Vec<_>>(),
            [ItemState::Done, ItemState::Skipped]
        );
        assert_eq!(
            end.said,
            batch_core::said_of(&Verb::Pull.text(), &end.outcomes, None)
        );
        assert_eq!(std::fs::read(dir.join("A.TXT")).unwrap(), b"AAA");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 开不了的那一项记失败（那一句 · 系统原话进详情、给重试）；［停］⇒ 手上那一项删掉、没轮到的记撤了。
    #[tokio::test]
    async fn a_refused_open_fails_one_and_a_stop_cancels_the_rest() {
        let dir = folder("refuse");
        let (on, seen, here, slot) = here_side(dir.clone(), &["a.txt"]);
        let b = Arc::new(PhoneBoard::new(9300, 1, pull_plan(&dir, &[]), "t".into(), on));
        *lock(&slot) = Some(b.clone());
        lock(boards()).insert(9300, b.clone());
        let there = There {
            hang: vec!["/w/b.md".into()],
            ..There::of(&[("/w/b.md", b"BBBB")])
        };
        let run = tokio::spawn(pull_go(b.clone(), Arc::new(there), false));
        for _ in 0..20_000 {
            if lock(&here.opens).len() == 2 && lock(&here.made).contains_key("1") {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(stop(9300), "那一组在");
        run.await.unwrap();
        lock(boards()).remove(&9300);
        assert_eq!(*lock(&here.drops), ["1"], "停了 ⇒ 手上那一份删掉");
        let end = lock(&b.ended).clone().unwrap();
        assert_eq!(
            end.outcomes.iter().map(|o| o.state).collect::<Vec<_>>(),
            [ItemState::Failed, ItemState::Cancelled]
        );
        let shown = lock(&seen).clone();
        let last = &shown.iter().rev().find(|(k, _)| k == "batch").unwrap().1;
        assert_eq!(last["phase"], "ended");
        assert_eq!(
            last["failed"],
            serde_json::json!([copy_core::copy_text(
                "rsFilewinProgress.detail.failedOne",
                &[
                    ("name", "a.txt"),
                    (
                        "why",
                        &copy_core::copy_text("mobFiles.local.writeFailed", &[("path", "a.txt")])
                    )
                ]
            )])
        );
        assert!(last["details"].as_str().unwrap().contains("EACCES"));
        assert_eq!(last["retry"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 同名那一问的答只在核心里认（整组一个答 · 逐项选、没点的照跳过 · 别的当取消）：下载 · 上传问的那一口（[`decided_json`]）
    /// 与核心跑的那几组（[`answer`]，复制到… 走它）同一组答出同一个结果。
    #[tokio::test]
    async fn one_clash_answer_decides_the_same_on_every_path() {
        let conflict = |k: &str| -> Conflict {
            serde_json::from_value(serde_json::json!({"key": k, "name": "a", "kind": "file", "there": "file", "answers": ["overwrite", "skip", "keep_both"]}))
                .unwrap()
        };
        let d = |v: &[(&str, Decide)]| {
            v.iter()
                .map(|(k, d)| (k.to_string(), *d))
                .collect::<BTreeMap<_, _>>()
        };
        let cases = [
            (
                Some(r#"{"all":"keep_both"}"#),
                Some(d(&[("0", Decide::KeepBoth), ("1", Decide::KeepBoth)])),
            ),
            (
                Some(r#"{"each":{"1":"overwrite"}}"#),
                Some(d(&[("0", Decide::Skip), ("1", Decide::Overwrite)])),
            ),
            (
                Some(r#"{"each":{}}"#),
                Some(d(&[("0", Decide::Skip), ("1", Decide::Skip)])),
            ),
            (None, None),
            (Some("{}"), None),
        ];
        for (n, (text, want)) in cases.into_iter().enumerate() {
            let id = 9100 + n as i64;
            let (on, _) = events();
            let b = Arc::new(PhoneBoard::new(id, 1, del("devbox"), "t".into(), on));
            lock(boards()).insert(id, b.clone());
            let asked = b.ask(vec![conflict("0"), conflict("1")]);
            assert!(answer(id, text), "{text:?}：有人在等这一答");
            lock(boards()).remove(&id);
            let port: Option<BTreeMap<String, Decide>> =
                serde_json::from_str(&decided_json(r#"["0","1"]"#, text)).unwrap();
            assert_eq!(asked.await, want, "{text:?}：一组那一份");
            assert_eq!(port, want, "{text:?}：下载 · 上传那一口");
        }
    }

    /// 🔴 同名那一问的答只在核心里拆：手机 Kotlin 生产源里拆开一个答（`is` / `as` 到 `All` · `Each`）的只有交核心那一形（生成的编解码 `Generated.kt`）。
    #[test]
    fn kotlin_takes_a_clash_answer_apart_only_to_hand_it_over() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let marks = [
            "is All",
            "is Each",
            "is BatchAnswer.",
            "as BatchAnswer.",
            "as? BatchAnswer.",
        ];
        let mut hits = std::collections::BTreeSet::new();
        let mut stack = vec![src];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    if !matches!(
                        p.file_name().and_then(|n| n.to_str()),
                        Some("build" | ".gradle")
                    ) {
                        stack.push(p);
                    }
                } else if p.extension().is_some_and(|x| x == "kt")
                    && p.to_string_lossy().contains("/src/main/")
                {
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    // 记号后面得是个断点（`is All` 不算 `is Allow.No`）。
                    let whole = |m: &str| {
                        text.match_indices(m).any(|(i, _)| {
                            m.ends_with('.')
                                || !text[i + m.len()..]
                                    .chars()
                                    .next()
                                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                        })
                    };
                    if marks.iter().any(|m| whole(m)) {
                        hits.insert(p.file_name().unwrap().to_string_lossy().into_owned());
                    }
                }
            }
        }
        assert_eq!(
            hits,
            std::collections::BTreeSet::from(["Generated.kt".to_string()]),
            "只许交核心那一形拆（它也得在：正控）"
        );
    }

    /// 🔴 `files_batch` 帧按票进那一组的进度（`running` 带手上那一项 · 几成），不交 Kotlin 的帧流；别的帧照交。
    #[test]
    fn a_batch_frame_goes_to_its_ticket_not_to_kotlin() {
        let (on, seen) = events();
        let b = Arc::new(PhoneBoard::new(
            9001,
            1,
            del("devbox"),
            "t-frame".into(),
            on,
        ));
        lock(boards()).insert(9001, b);
        assert!(took(
            r#"{"type":"files_batch","ticket":"t-frame","done":1,"total":4,"current":"a.txt"}"#
        ));
        assert!(
            took(r#"{"type":"files_batch","ticket":"t-nobody","done":1,"total":4}"#),
            "没人认的票也不交 Kotlin"
        );
        assert!(!took(r#"{"type":"line","sid":"s","files_batch":1}"#));
        lock(boards()).remove(&9001);
        let seen = lock(&seen).clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["current"], "a.txt");
        assert_eq!(seen[0]["frac"], 0.25);
    }

    /// 两种事都收（`batch` · `batch_keep`），照到达先后。
    fn kinds() -> (OnEvent, Arc<Mutex<Vec<(String, serde_json::Value)>>>) {
        let seen: Arc<Mutex<Vec<(String, serde_json::Value)>>> = Arc::default();
        let s2 = seen.clone();
        let on: OnEvent = Arc::new(move |_, kind, body| {
            lock(&s2).push((kind.to_string(), serde_json::from_str(&body).unwrap()));
        });
        (on, seen)
    }

    fn of_kind(seen: &[(String, serde_json::Value)], kind: &str) -> Vec<serde_json::Value> {
        seen.iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// 🔴 一开工卡就在：那台还没回（删一棵大目录那一问可能几分钟不报一格进度）⇒ 屏上已经是「在做」那一格，不等第一格进度。
    #[tokio::test]
    async fn a_group_shows_running_before_the_first_reply() {
        let (on, seen) = events();
        let b = Arc::new(PhoneBoard::new(7, 1, del("devbox"), "t-1".into(), on));
        let b2 = b.clone();
        let run = tokio::spawn(async move {
            b2.go(
                |_args| std::future::pending::<Result<serde_json::Value, Rebuff>>(),
                || async {},
            )
            .await;
        });
        for _ in 0..2000 {
            if !lock(&seen).is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        run.abort();
        let seen = lock(&seen).clone();
        let first = seen.first().expect("那台没回之前一格都没交：屏上没有卡");
        assert_eq!(first["phase"], "running", "{seen:?}");
        assert_eq!(first["id"], 7);
        assert!(
            first["frac"].is_null(),
            "还没有一格进度 ⇒ 不写几成：{first}"
        );
    }

    /// 🔴 一组落盘的那一份 ＝ 最后发出去的入参（带票 · 带答过的 `decide`）＋ 断开时的完成数：开始时写、答了重写、断开重写、收场删；
    /// 进程没了再开 ⇒ 照那一份接回来：先摆「其余待核」那一行（完成数照落盘的），再问的入参逐字节等于落盘那一份，收场删掉。
    #[tokio::test]
    async fn a_kept_group_asks_again_with_the_bytes_it_kept() {
        let (on, seen) = kinds();
        let b = Arc::new(PhoneBoard::new(7, 1, del("devbox"), "t-1".into(), on));
        let b2 = b.clone();
        let run = tokio::spawn(async move {
            let n = Arc::new(Mutex::new(0usize));
            let b3 = b2.clone();
            b2.go(
                move |_args| {
                    let k = {
                        let mut n = lock(&n);
                        *n += 1;
                        *n
                    };
                    let b4 = b3.clone();
                    async move {
                        match k {
                            1 => Err(Rebuff {
                                failed: Failed { code: Some("exists".into()), said: "同名 1 项".into(), detail: String::new() },
                                data: Some(serde_json::json!({"conflicts": [{"key": "0", "name": "a.txt", "kind": "file", "there": "file",
                                    "answers": ["overwrite", "skip", "keep_both"]}]})),
                                cut: false,
                            }),
                            _ => {
                                b4.progress(&serde_json::json!({"done": 1, "total": 2}));
                                Err(rebuff_of("devbox", "files-delete", CallError::Disconnected))
                            }
                        }
                    }
                },
                // 那台一直不回来：进程在这里被杀。
                std::future::pending::<()>,
            )
            .await;
        });
        for _ in 0..2000 {
            if lock(&b.answer).is_some() {
                assert!(b.answer(Some(r#"{"all":"keep_both"}"#)));
            }
            if of_kind(&lock(&seen), "batch")
                .iter()
                .any(|e| e["phase"] == "cut")
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        run.abort();
        let keeps = of_kind(&lock(&seen), "batch_keep");
        assert_eq!(keeps.len(), 3, "开始 · 答了 · 断开各写一次：{keeps:?}");
        assert!(keeps.iter().all(|k| k["ticket"] == "t-1"));
        let blob = |k: &serde_json::Value| -> serde_json::Value {
            serde_json::from_str(k["plan"].as_str().expect("落盘那一份是一串字")).unwrap()
        };
        assert_eq!(blob(&keeps[0])["args"].get("decide"), None);
        assert_eq!(
            blob(&keeps[1])["args"]["decide"],
            serde_json::json!({"0": "keep_both"})
        );
        let kept = keeps[2]["plan"].as_str().unwrap().to_string();

        // 再开：只有落盘那一份。
        let (on, seen) = kinds();
        let r = Arc::new(PhoneBoard::resumed(8, 2, &kept, "devbox", on).expect("落盘那一份读得回"));
        let sent: Arc<Mutex<Vec<String>>> = Arc::default();
        let s2 = sent.clone();
        r.go(
            move |args| {
                lock(&s2).push(serde_json::to_string(&args).unwrap());
                async { Ok(reply("删除 2 项 · 完成 1 · 失败 ×1")) }
            },
            || async {},
        )
        .await;
        let sent = lock(&sent).clone();
        let want =
            serde_json::to_string(&blob(&serde_json::json!({ "plan": kept }))["args"]).unwrap();
        assert_eq!(sent, [want], "再问那一问的入参逐字节等于落盘那一份");
        assert!(kept.contains(&sent[0]), "落盘那一串里原样有这一份：{kept}");
        let seen = lock(&seen).clone();
        let shown = of_kind(&seen, "batch");
        let phases: Vec<&str> = shown.iter().map(|e| e["phase"].as_str().unwrap()).collect();
        assert_eq!(phases, ["cut", "ended"]);
        assert_eq!(
            shown[0]["line"],
            "devbox 连接已断开 · 删除 2 项 · 完成 1 · 其余待核"
        );
        assert!(
            shown.iter().all(|e| e["kept"] == true && e["id"] == 8),
            "接回来的那一组带记号：{shown:?}"
        );
        assert_eq!(
            of_kind(&seen, "batch_keep"),
            [serde_json::json!({"ticket": "t-1", "plan": null})],
            "入参没变不重写；收场删"
        );
        assert!(
            PhoneBoard::resumed(
                9,
                2,
                "{\"cmd\":\"files-nope\"}",
                "devbox",
                Arc::new(|_, _, _| {})
            )
            .is_none(),
            "认不出的那一份不接"
        );
    }
}
