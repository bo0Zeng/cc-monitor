//! **一组一条命令**在手机这一侧的宿主：全过程是 `batch_core::run` 那一份（桌面文件窗口同一份），这里只接三根线 ——
//! 问那台（常驻流那一条，带票；断了 ⇒ 等流接上、同一张票再问）· 进度（`files_batch` 帧在核心里按票接，不交 Kotlin 的帧流）·
//! 往 Kotlin 交的事：`batch {id, phase, title, …}`，字全由核心写好（标题 · 断开那一行 · 结局句 · 没成的那几行 · 复制详情）。
//!
//! Kotlin 交的是「要做什么」（删 · 改权限 · 复制到 · 重试 · 撤销），入参由这里拼；同名那一问的答经 [`answer`]，「停」经 [`stop`]。

use crate::resident::{self, OnEvent};
use batch_core::{Board, Failed, Phase, Plan, Rebuff, Tick, Verb};
use cmd_wire::batch::{BatchReply, Conflict, Decide};
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
            on_event,
        }
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

    /// 同名那一问答了：`all` 整组一个答（`None` 且没有逐项 ⇒ 取消）；`each` 逐项那几格。回值 ＝ 真有人在等这一答。
    pub fn answer(&self, all: Option<Decide>, each: Option<BTreeMap<String, Decide>>) -> bool {
        let conflicts = std::mem::take(&mut *lock(&self.asking));
        let map = batch_core::answer_of(&conflicts, all, each.as_ref());
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
            .map(|i| i.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string())
            .collect()
    };
    let simple = |verb: Verb, cmd: &'static str, args: serde_json::Value, items: &[serde_json::Value]| Plan {
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
                    let mut w = serde_json::json!({ "rel": i.get("rel").cloned().unwrap_or_default() });
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
                .map(|n| serde_json::from_value::<WirePath>(n.clone()).ok().map(|p| p.0))
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
    let cut = matches!(e, CallError::Disconnected | CallError::Timeout { .. } | CallError::Cancelled);
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
            detail: line.get("detail").and_then(|d| d.as_str()).unwrap_or_default().to_string(),
        },
        data: None,
        cut,
    }
}

/// 起一组：回编号（进度 · 同名那一问 · 结局经 `on_event` 的 `batch` 那一种交）；这台不在 · 读不动 ⇒ 0。
pub fn start(h: i64, want: &str, on_event: OnEvent) -> i64 {
    let Some(f) = resident::feed(h) else { return 0 };
    let Some(plans) = serde_json::from_str::<serde_json::Value>(want)
        .ok()
        .and_then(|w| plan_of(&w, f.machine()))
    else {
        return 0;
    };
    let mut first = 0;
    for plan in plans {
        let id = resident::next_call();
        if first == 0 {
            first = id;
        }
        let mut random = [0u8; 16];
        let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random);
        let b = Arc::new(PhoneBoard::new(id, h, plan, chunk_core::key_of(random), on_event.clone()));
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
            batch_core::run(
                &*b,
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
    }
    first
}

/// 同名那一问的答：`{all: "skip"|"keep_both"|"overwrite"}` · `{each: {键: 答}}` · 别的（`null`）⇒ 取消。
pub fn answer(id: i64, text: Option<&str>) -> bool {
    let Some(b) = board(id) else { return false };
    let v: serde_json::Value = text
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or_default();
    let all = v
        .get("all")
        .and_then(|a| serde_json::from_value::<Decide>(a.clone()).ok());
    let each = v
        .get("each")
        .and_then(|e| serde_json::from_value::<BTreeMap<String, Decide>>(e.clone()).ok());
    b.answer(all, each)
}

/// 撤这一组（「停」）：同一张票发 `files-batch-stop`；那台做完手上那一项就停，剩下的回 `cancelled`。
pub fn stop(id: i64) -> bool {
    let Some(b) = board(id) else { return false };
    let Some(f) = resident::feed(b.h) else { return false };
    let ticket = b.ticket.clone();
    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        if let Some(c) = f.client_by(deadline).await {
            let _ = c
                .call_until(batch_core::CMD_STOP, serde_json::json!({ "ticket": ticket }), deadline)
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
            assert_eq!(kind, "batch");
            lock(&s2).push(serde_json::from_str(&body).unwrap());
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
        assert_eq!(c.args, serde_json::json!({"root": "/w", "items": [{"rel": "a"}], "mode": "644"}));
        assert_eq!(c.cmd, "files-chmod");
        let p = plan_of(&serde_json::json!({"copy": {"from": "/w/a", "names": ["x"], "to": "/w/b"}}), "devbox")
            .unwrap()
            .remove(0);
        assert_eq!(p.args, serde_json::json!({"root": "/w", "items": [{"from": "a/x", "to": "b/x"}]}));
        assert!(plan_of(&serde_json::json!({"nope": {}}), "devbox").is_none());
        // 带上票（发出去的那一形）照后端的入参类型解得开。
        let ticketed = |p: &Plan| {
            let mut a = p.args.clone();
            a["ticket"] = "t-1".into();
            a
        };
        serde_json::from_value::<cmd_wire::files::FilesDeleteReq>(ticketed(&del("devbox"))).unwrap();
        serde_json::from_value::<cmd_wire::files::FilesChmodReq>(ticketed(&c)).unwrap();
        let mut answered = ticketed(&p);
        answered["decide"] = serde_json::json!({"0": "keep_both"});
        serde_json::from_value::<cmd_wire::files::FilesCopyReq>(answered).unwrap();
        serde_json::from_value::<cmd_wire::batch::FilesBatchStopReq>(serde_json::json!({"ticket": "t-1"})).unwrap();
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
                    assert!(b2.answer(Some(Decide::KeepBoth), None));
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
        assert!(sent.iter().all(|a| a["ticket"] == "t-1"), "同一张票：{sent:?}");
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
        assert_eq!(end["said"], "删除 2 项 · 完成 1 · 失败 ×1", "结局句照后端那一句");
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
                    if !matches!(p.file_name().and_then(|n| n.to_str()), Some("build" | ".gradle")) {
                        stack.push(p);
                    }
                } else if p.extension().is_some_and(|x| x == "kt") && p.to_string_lossy().contains("/src/main/") {
                    seen += 1;
                    if std::fs::read_to_string(&p).unwrap_or_default().contains("beFiles.batch.") {
                        hits.push(p.display().to_string());
                    }
                }
            }
        }
        assert!(seen > 50, "Kotlin 生产源码一份都没扫到 —— 本条在空转");
        assert!(hits.is_empty(), "Kotlin 里拼了一组的字：{hits:?}");
        let core = std::fs::read_to_string(src.join("../common/batch-core/src/lib.rs")).unwrap();
        assert!(core.contains("\"beFiles.batch.segBad\""), "正控：共用那一份取这几个键");
    }

    /// 🔴 `files_batch` 帧按票进那一组的进度（`running` 带手上那一项 · 几成），不交 Kotlin 的帧流；别的帧照交。
    #[test]
    fn a_batch_frame_goes_to_its_ticket_not_to_kotlin() {
        let (on, seen) = events();
        let b = Arc::new(PhoneBoard::new(9001, 1, del("devbox"), "t-frame".into(), on));
        lock(boards()).insert(9001, b);
        assert!(took(r#"{"type":"files_batch","ticket":"t-frame","done":1,"total":4,"current":"a.txt"}"#));
        assert!(took(r#"{"type":"files_batch","ticket":"t-nobody","done":1,"total":4}"#), "没人认的票也不交 Kotlin");
        assert!(!took(r#"{"type":"line","sid":"s","files_batch":1}"#));
        lock(boards()).remove(&9001);
        let seen = lock(&seen).clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["current"], "a.txt");
        assert_eq!(seen[0]["frac"], 0.25);
    }
}
