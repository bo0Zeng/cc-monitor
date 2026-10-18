//! **一组一条命令**在手机这一侧的宿主：全过程是 `batch_core::run` 那一份（桌面文件窗口同一份），这里只接三根线 ——
//! 问那台（常驻流那一条，带票；断了 ⇒ 等流接上、同一张票再问）· 进度（`files_batch` 帧在核心里按票接，不交 Kotlin 的帧流）·
//! 往 Kotlin 交的事：`batch {id, phase, title, …}`，字全由核心写好（标题 · 断开那一行 · 结局句 · 没成的那几行 · 复制详情）；
//! 落盘那一份 `batch_keep {ticket, plan}`（进程没了再开，Kotlin 原样交回、这里照同一张票再问）。
//!
//! Kotlin 交的是「要做什么」（删 · 改权限 · 复制到 · 重试 · 撤销），入参由这里拼；同名那一问的答经 [`answer`]，「停」经 [`stop`]。

use crate::resident::{self, OnEvent};
use batch_core::{Board, Failed, Phase, Plan, Rebuff, Tick, Verb};
use cmd_wire::batch::{BatchReply, Conflict, Decide, ItemState};
use cmd_wire::files::{Call, WirePath};
use resident_client::client::CallError;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

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
    on_event: OnEvent,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PhoneBoard {
    pub fn new(id: i64, h: i64, plan: Plan, ticket: String, on_event: OnEvent) -> Self {
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
            serde_json::json!({
                "cmd": self.plan.cmd,
                "args": lock(&self.sent).clone(),
                "names": self.plan.names,
                "into": self.plan.into,
                "done": t.done,
                "total": t.total,
            })
            .to_string()
        });
        let body = serde_json::json!({ "ticket": self.ticket, "plan": plan });
        (self.on_event)(self.h, "batch_keep", body.to_string());
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

    /// 同名那一问答了（[`decided`] 那一形）。回值 ＝ 真有人在等这一答。
    pub fn answer(&self, text: Option<&str>) -> bool {
        let conflicts = std::mem::take(&mut *lock(&self.asking));
        let map = decided(conflicts.iter().map(|c| c.key.as_str()), text);
        lock(&self.answer)
            .take()
            .is_some_and(|tx| tx.send(map).is_ok())
    }

    fn emit(&self, phase: &Phase) {
        (self.on_event)(self.h, "batch", self.event(phase).to_string());
    }

    /// 交 Kotlin 的那一行（字全写好）。
    pub fn event(&self, phase: &Phase) -> serde_json::Value {
        let (t, current) = lock(&self.tick).clone();
        let mut v = serde_json::json!({
            "id": self.id,
            "title": batch_core::title_of(&self.plan),
        });
        let (name, extra) = match phase {
            Phase::Running => (
                "running",
                serde_json::json!({ "current": current, "frac": t.frac() }),
            ),
            Phase::Asking => (
                "asking",
                serde_json::json!({ "conflicts": clash_rows(&lock(&self.asking)), "go": self.plan.verb.text() }),
            ),
            Phase::Cut(line) => ("cut", serde_json::json!({ "line": line })),
            Phase::Ended(r) => {
                let failed = batch_core::failed_lines(&self.plan, r);
                (
                    "ended",
                    serde_json::json!({
                        "said": r.said,
                        "failed_title": copy_core::copy_text("beFiles.batch.segBad", &[("bad", &failed.len().to_string())]),
                        "failed": failed,
                        "details": batch_core::copy_details(&self.plan, r),
                        "retry": batch_core::retry_plan(&self.plan, r).is_some(),
                        "undo": batch_core::undo_plan(&self.plan, r).is_some(),
                    }),
                )
            }
            Phase::Failed(f) => (
                "failed",
                serde_json::json!({ "said": f.said, "details": Failed::copy_body(&f.said, &f.detail) }),
            ),
            Phase::Dropped | Phase::Opened => ("dropped", serde_json::json!({})),
        };
        v["phase"] = name.into();
        if self.kept {
            v["kept"] = true.into();
        }
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
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
        lock(&self.tick).0
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
pub fn clash_rows(conflicts: &[Conflict]) -> serde_json::Value {
    conflicts
        .iter()
        .map(|c| {
            serde_json::json!({
                "key": c.key,
                "name": String::from_utf8_lossy(c.name.bytes()),
                "meta": batch_core::conflict_meta(c),
                "answers": c.answers,
            })
        })
        .collect()
}

/// 下载一组那几格（逐项拉在 Kotlin 那一圈里走：每项交一个本机描述符，见 [`crate::transfer`]）：字照一组那一份写。
/// `want`：`{id, machine, names, phase, …}`，`phase` ＝
/// - `asking`（`here`：本机那个文件夹里已有的名字）⇒ 同名那一问（每行带 `there` ＝ 盘上撞的那一份）；一项都不撞 ⇒ `running`；
/// - `running`（`done` 下完几项 · `part` 手上那一项几成 · `current`）；
/// - `cut` · `ended`（`outcomes`：逐项结局，同后端一组的那一形）⇒ 断开那一行（给［继续］）· 结局句与没成的那几行。
///
/// 读不动 ⇒ `None`。
pub fn download_event(want: &serde_json::Value) -> Option<serde_json::Value> {
    let names: Vec<String> = want
        .get("names")?
        .as_array()?
        .iter()
        .map(|n| n.as_str().map(str::to_string))
        .collect::<Option<_>>()?;
    let plan = Plan {
        verb: Verb::Download,
        cmd: <cmd_wire::files::ReadChunkReq as Call>::NAME,
        args: serde_json::json!({ "items": names }),
        names,
        into: None,
        machine: want.get("machine")?.as_str()?.to_string(),
    };
    let id = want.get("id")?.as_i64()?;
    let head = serde_json::json!({ "id": id, "title": batch_core::title_of(&plan) });
    let with = |phase: &str, extra: serde_json::Value| {
        let mut v = head.clone();
        v["phase"] = phase.into();
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    };
    let outcomes = || {
        serde_json::from_value::<Vec<cmd_wire::batch::BatchOutcome>>(want.get("outcomes")?.clone())
            .ok()
    };
    let running = |done: f64, part: f64, current: Option<&str>| {
        let n = plan.names.len().max(1) as f64;
        with(
            "running",
            serde_json::json!({ "current": current, "frac": ((done + part) / n) as f32 }),
        )
    };
    Some(match want.get("phase")?.as_str()? {
        "asking" => {
            let here: Vec<&str> = want
                .get("here")?
                .as_array()?
                .iter()
                .filter_map(|h| h.as_str())
                .collect();
            let rows = local_clashes(&plan.names, &here);
            if rows.is_empty() {
                return Some(running(0.0, 0.0, None));
            }
            with(
                "asking",
                serde_json::json!({ "conflicts": rows, "go": plan.verb.text() }),
            )
        }
        "running" => {
            let n = |k: &str| want.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
            running(
                n("done"),
                n("part"),
                want.get("current").and_then(|c| c.as_str()),
            )
        }
        "cut" => {
            let outs = outcomes()?;
            let count = |s: ItemState| outs.iter().filter(|o| o.state == s).count();
            let line = batch_core::cut_left_line(
                &plan,
                count(ItemState::Done),
                count(ItemState::Cancelled),
            );
            with("cut", serde_json::json!({ "line": line, "resume": true }))
        }
        "ended" => {
            let outcomes = outcomes()?;
            let said = batch_core::said_of(&plan.verb.text(), &outcomes, None);
            let board = PhoneBoard::new(id, 0, plan, String::new(), Arc::new(|_, _, _| {}));
            board.event(&Phase::Ended(BatchReply { outcomes, said }))
        }
        _ => return None,
    })
}

/// 本机同名那一步：要下的名字里，文件夹里已有（不分大小写：手机的共享存储不分）的那几项 ⇒ 抽屉那几行（只有名字一格；目录不下，恒给覆盖）。
fn local_clashes(names: &[String], here: &[&str]) -> Vec<serde_json::Value> {
    let answers = [Decide::Overwrite, Decide::Skip, Decide::KeepBoth];
    names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let there = here.iter().find(|h| h.to_lowercase() == n.to_lowercase())?;
            Some(serde_json::json!({
                "key": i.to_string(),
                "name": n,
                "there": there,
                "meta": "",
                "answers": answers,
            }))
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
        .unwrap_or_else(|| copy_core::copy_text("front.body.offline", &[("machine", machine)]));
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
    tokio::spawn(async move {
        let cmd = b.plan.cmd;
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
            || async move {
                if let Some(f) = resident::feed(h) {
                    let far = tokio::time::Instant::now() + batch_core::BATCH_BUDGET;
                    let _ = f.client_by(far).await;
                }
            },
        )
        .await;
    });
    id
}

/// 同名那一问的答 ⇒ 每个键一格（`batch_core::answer_of`）：`{all: "skip"|"keep_both"|"overwrite"}` · `{each: {键: 答}}` · 别的（`null`）⇒ 取消。
pub fn decided<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    text: Option<&str>,
) -> Option<BTreeMap<String, Decide>> {
    let v: serde_json::Value = text
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    let all = v
        .get("all")
        .and_then(|a| serde_json::from_value::<Decide>(a.clone()).ok());
    let each = v
        .get("each")
        .and_then(|e| serde_json::from_value::<BTreeMap<String, Decide>>(e.clone()).ok());
    batch_core::answer_of(keys, all, each.as_ref())
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
pub fn stop(id: i64) -> bool {
    let Some(b) = board(id) else { return false };
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

    fn pull(phase: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut w = serde_json::json!({"id": -3, "machine": "devbox", "names": ["a.txt", "B.md", "c"], "phase": phase});
        if let (Some(o), Some(e)) = (w.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        download_event(&w).unwrap()
    }

    /// 🔴 下载一组本机同名那一步（纯函数）：文件夹里已有的名字 ⇒ 同名那几项（不分大小写，记下盘上那一份叫什么）；一项都不撞 ⇒ 不问。
    #[test]
    fn the_names_already_in_the_folder_become_the_clash_rows() {
        let ask = pull(
            "asking",
            serde_json::json!({"here": ["A.TXT", "b.md", "z"]}),
        );
        assert_eq!(ask["phase"], "asking");
        assert_eq!(ask["title"], "下载 3 项");
        assert_eq!(ask["go"], "下载");
        assert_eq!(
            ask["conflicts"],
            serde_json::json!([
                {"key": "0", "name": "a.txt", "there": "A.TXT", "meta": "", "answers": ["overwrite", "skip", "keep_both"]},
                {"key": "1", "name": "B.md", "there": "b.md", "meta": "", "answers": ["overwrite", "skip", "keep_both"]},
            ])
        );
        assert_eq!(
            pull("asking", serde_json::json!({"here": ["z"]}))["phase"],
            "running",
            "不撞就直接下"
        );
    }

    /// 🔴 下载一组的几格字都在核心：走到哪（手上那一项连同它下到几成）· 断开那一行（中断 ＝ 没下完的项数，给［继续］）·
    /// 结局句（同后端那一句）与没成的那几行、给重试。
    #[test]
    fn a_download_group_is_spelt_by_the_core() {
        let run = pull(
            "running",
            serde_json::json!({"done": 1, "part": 0.5, "current": "B.md"}),
        );
        assert_eq!(
            (run["phase"].as_str(), run["current"].as_str()),
            (Some("running"), Some("B.md"))
        );
        assert_eq!(run["frac"], 0.5);
        let cut = pull(
            "cut",
            serde_json::json!({"outcomes": [{"key": "0", "state": "done"}, {"key": "1", "state": "cancelled"}, {"key": "2", "state": "cancelled"}]}),
        );
        assert_eq!(
            cut["line"],
            "devbox 连接已断开 · 下载 3 项 · 完成 1 · 中断 2"
        );
        assert_eq!(cut["resume"], true);
        let end = pull(
            "ended",
            serde_json::json!({"outcomes": [{"key": "0", "state": "done"}, {"key": "1", "state": "failed", "reason": "写不进本机", "detail": "EROFS"}, {"key": "2", "state": "done"}]}),
        );
        assert_eq!(end["said"], "下载 3 项 · 完成 2 · 失败 ×1");
        assert_eq!(end["failed"], serde_json::json!(["B.md：写不进本机"]));
        assert_eq!(
            (end["retry"].as_bool(), end["undo"].as_bool()),
            (Some(true), Some(false))
        );
        assert!(end["details"].as_str().unwrap().contains("EROFS"));
        let all = pull(
            "ended",
            serde_json::json!({"outcomes": [{"key": "0", "state": "done"}, {"key": "1", "state": "done"}, {"key": "2", "state": "skipped"}]}),
        );
        assert_eq!(all["failed"], serde_json::json!([]));
        assert!(
            download_event(&serde_json::json!({"phase": "ended"})).is_none(),
            "读不动 ⇒ 不给"
        );
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

    /// 🔴 同名那一问的答只在核心里拆：手机 Kotlin 生产源里拆开一个答（`is` / `as` 到 `All` · `Each`）的只有交核心那一形（`BatchAnswer.wire`）。
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
                    if marks.iter().any(|m| text.contains(m)) {
                        hits.insert(p.file_name().unwrap().to_string_lossy().into_owned());
                    }
                }
            }
        }
        assert_eq!(
            hits,
            std::collections::BTreeSet::from(["BatchEvent.kt".to_string()]),
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
        assert!(first["frac"].is_null(), "还没有一格进度 ⇒ 不写几成：{first}");
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
