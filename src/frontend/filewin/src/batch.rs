//! **一组一条命令**在窗口里的那一层：复制到另一栏 · 复制到另一台 · 删除 · 改权限；下载 · 上传那两组的开单也经这里问同名
//! （开了单之后各项怎么传见 [`super::download`] · [`super::transfer`]）。
//!
//! 一组的全过程（带票发 · 同名问一次 · 断了等连回同票再问 · 结局）与它的字住共用的 `batch_core`；窗口只剩：
//! 看板（[`BatchBoard`]，egui 那条线程画、tokio 那条写）· 冲突框（[`ask_ui`]）· 起跑（[`go`]：订 `files-batch/<票>` 的进度、
//! 装撤的那一手、问法走通道）。票是窗口铸的（一组一张 UUID）。

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use batch_core::{Phase, Plan, Tick};
use cmd_wire::batch::{Conflict, Decide};
use copy_core::copy_text;

/// 下载一组那一条（monitor 补拨号请求、转给本机后端的传输台）。
pub const CMD_DOWNLOAD: &str =
    <cmd_wire::transfer::TransferDownloadAsk as cmd_wire::files::Call>::NAME;

/// 冲突框里摆着的那一问。
pub struct Asking {
    pub conflicts: Vec<Conflict>,
    /// 逐项选展开着吗。
    pub each: bool,
    /// 逐项选的答（缺 ⇒ 跳过）。
    pub picks: BTreeMap<String, Decide>,
    answer: Option<tokio::sync::oneshot::Sender<Option<BTreeMap<String, Decide>>>>,
}

struct Desk {
    plan: Plan,
    ticket: String,
    phase: Phase,
    tick: Tick,
    asking: Option<Asking>,
    stopper: Option<Arc<dyn Fn() + Send + Sync>>,
    stopping: bool,
    /// 这一组寻址的那台 · 真正在哪台的后端里做（[`go`] 记下；「重试失败的」照它另起一组）。
    route: Option<(super::source::Origin, super::source::Origin)>,
}

/// 一组在窗口上的样子（UI 线程画、tokio 那条写）。
#[derive(Clone)]
pub struct BatchBoard {
    inner: Arc<Mutex<Desk>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl BatchBoard {
    /// 新的一组：铸一张票。
    pub fn new(plan: Plan) -> BatchBoard {
        BatchBoard {
            inner: Arc::new(Mutex::new(Desk {
                plan,
                ticket: uuid::Uuid::new_v4().to_string(),
                phase: Phase::Running,
                tick: Tick::default(),
                asking: None,
                stopper: None,
                stopping: false,
                route: None,
            })),
            ctx: Arc::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Desk> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn attach(&self, ctx: Option<egui::Context>) {
        *self.ctx.lock().unwrap_or_else(|p| p.into_inner()) = ctx;
    }

    fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            c.request_repaint();
        }
    }

    pub fn ticket(&self) -> String {
        self.lock().ticket.clone()
    }

    pub fn plan(&self) -> Plan {
        self.lock().plan.clone()
    }

    pub fn phase(&self) -> Phase {
        self.lock().phase.clone()
    }

    pub fn tick(&self) -> Tick {
        self.lock().tick
    }

    pub fn is_asking(&self) -> bool {
        self.lock().asking.is_some()
    }

    /// 落一步（`batch_core::run` 走它；截图台架也用它摆出那几态）。
    pub(crate) fn set(&self, phase: Phase) {
        self.lock().phase = phase;
        self.poke();
    }

    /// 进度来了一格（帧那一侧调）。
    pub fn progress(&self, t: Tick) {
        self.lock().tick = t;
        self.poke();
    }

    /// 摆出冲突框，交出「答复送哪儿」那一头（`None` ＝ 取消）。
    pub fn ask(
        &self,
        conflicts: Vec<Conflict>,
    ) -> tokio::sync::oneshot::Receiver<Option<BTreeMap<String, Decide>>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut d = self.lock();
            d.asking = Some(Asking {
                conflicts,
                each: false,
                picks: BTreeMap::new(),
                answer: Some(tx),
            });
        }
        self.poke();
        rx
    }

    /// 冲突框摆着的那几项（判据与画框用）。
    pub fn conflicts(&self) -> Vec<Conflict> {
        self.lock()
            .asking
            .as_ref()
            .map(|a| a.conflicts.clone())
            .unwrap_or_default()
    }

    /// 冲突框上答了：`all` ＝ 整组一个答（`None` ＝ 取消）；逐项选展开着 ⇒ 用逐项的答（没点的那几项跳过）。
    pub fn answer(&self, all: Option<Decide>, each: bool) -> bool {
        let (tx, map) = {
            let mut d = self.lock();
            let Some(mut a) = d.asking.take() else {
                return false;
            };
            let map = batch_core::answer_of(&a.conflicts, all, each.then_some(&a.picks));
            (a.answer.take(), map)
        };
        self.poke();
        tx.is_some_and(|t| t.send(map).is_ok())
    }

    /// 逐项选展开 / 收起。
    pub fn each(&self, on: bool) {
        if let Some(a) = self.lock().asking.as_mut() {
            a.each = on;
        }
        self.poke();
    }

    /// 逐项选那一格点了一下。
    pub fn pick(&self, key: &str, d: Decide) {
        if let Some(a) = self.lock().asking.as_mut() {
            a.picks.insert(key.to_string(), d);
        }
    }

    /// 装撤的那一手（起跑那一侧装：知道向哪台发 `files-batch-stop`）。
    pub fn set_stopper(&self, f: Arc<dyn Fn() + Send + Sync>) {
        self.lock().stopper = Some(f);
    }

    /// 撤这一组（「停」那颗）：回值 ＝ 真的发了。
    pub fn stop(&self) -> bool {
        let f = {
            let mut d = self.lock();
            if !matches!(d.phase, Phase::Running | Phase::Cut(_)) || d.stopping {
                return false;
            }
            d.stopping = true;
            d.stopper.clone()
        };
        match f {
            Some(f) => {
                f();
                true
            }
            None => false,
        }
    }

    pub fn stopping(&self) -> bool {
        self.lock().stopping
    }

    /// 记下这一组寻址的那台 · 在哪台的后端里做（[`go`] 起跑时记；截图台架摆结局那几态也用它）。
    pub(crate) fn set_route(&self, at: super::source::Origin, home: super::source::Origin) {
        self.lock().route = Some((at, home));
    }

    /// 这一组寻址的那台 · 在哪台的后端里做（还没起跑 ⇒ `None`）。
    pub fn route(&self) -> Option<(super::source::Origin, super::source::Origin)> {
        self.lock().route.clone()
    }

    /// 一行的标题：`复制 5 项`（落在一个目录 ⇒ `复制 5 项到 ~/proj/b`）。
    pub fn title(&self) -> String {
        batch_core::title_of(&self.plan())
    }

    /// 结局那一行底下逐项列的那几行（没成的那几项：名字 · 失败 · 原因）。
    pub fn failed_lines(&self) -> Vec<String> {
        let (plan, phase) = {
            let d = self.lock();
            (d.plan.clone(), d.phase.clone())
        };
        let Phase::Ended(r) = phase else {
            return Vec::new();
        };
        batch_core::failed_lines(&plan, &r)
    }
}

impl batch_core::Board for BatchBoard {
    fn plan(&self) -> Plan {
        BatchBoard::plan(self)
    }

    fn ticket(&self) -> String {
        BatchBoard::ticket(self)
    }

    fn tick(&self) -> Tick {
        BatchBoard::tick(self)
    }

    fn set(&self, phase: Phase) {
        BatchBoard::set(self, phase)
    }

    fn ask(
        &self,
        conflicts: Vec<Conflict>,
    ) -> impl Future<Output = Option<BTreeMap<String, Decide>>> {
        let rx = BatchBoard::ask(self, conflicts);
        async move { rx.await.ok().flatten() }
    }
}

/// 起一组（UI 线程调，[`go`] 扔给 tokio）。
pub fn spawn(
    h: &tokio::runtime::Handle,
    line: super::source::Line,
    at: super::source::Origin,
    home: super::source::Origin,
    board: BatchBoard,
) {
    h.spawn(go(line, at, home, board));
}

/// 做一组到落地：订进度、装撤的那一手、发（[`run`]）。`at` ＝ 寻址的那台，`home` ＝ 那一组真正在哪台的后端里做
/// （同台那几条 ＝ `at`；两台之间的复制在本机后端里做 ⇒ 本机）—— 撤发给它、断开时等它连回来。要在 tokio 运行时里调。
pub async fn go(
    line: super::source::Line,
    at: super::source::Origin,
    home: super::source::Origin,
    board: BatchBoard,
) {
    let ticket = board.ticket();
    board.set_route(at.clone(), home.clone());
    let rt = tokio::runtime::Handle::current();
    {
        let (line, home, ticket) = (line.clone(), home.clone(), ticket.clone());
        let rt = rt.clone();
        board.set_stopper(Arc::new(move || {
            let (line, home, ticket) = (line.clone(), home.clone(), ticket.clone());
            rt.spawn(async move {
                let req = cmd_wire::batch::FilesBatchStopReq { ticket };
                if let Err(f) =
                    super::source::ask_typed(&line, &home, &req, super::writeops::WRITE_BUDGET)
                        .await
                {
                    tracing::warn!("files-batch-stop not delivered: {}", f.said);
                }
            });
        }));
    }
    // 进度：订 `files-batch/<票>` 到这一组收场。
    let watcher = {
        let (line, at, board) = (line.clone(), at.clone(), board.clone());
        let kind = format!("{}{ticket}", filewin_contract::FILES_BATCH_KIND_PREFIX);
        rt.spawn(async move {
            super::source::watch_cells(&line, &at, &kind, |v| {
                if let Some(t) = Tick::of_cell(&v) {
                    board.progress(t);
                }
            })
            .await;
        })
    };
    let cmd = board.plan().cmd;
    batch_core::run(
        &board,
        |args| {
            let (line, at) = (line.clone(), at.clone());
            async move { super::source::ask_batch(&line, &at, cmd, &args, batch_core::BATCH_BUDGET).await }
        },
        || {
            let (line, home) = (line.clone(), home.clone());
            async move { super::source::link_up(&line, &home).await }
        },
    )
    .await;
    watcher.abort();
}

/// 画冲突框（有一组在问时）：标题 `复制 5 项到 ~/proj/b` · `同名 3 项` · 清单（名字 ｜ 要放过去的 → 已有的；目录写不合并）·
/// ［逐项选］· ［取消］［跳过同名］（默认）［保留两份］［覆盖］（危险）。逐项选展开 ⇒ 每行一个三段（目录只给跳过｜保留两份）·［取消］［动作本身（复制 · 上传 …）］。
pub fn ask_ui(ui: &mut egui::Ui, board: &BatchBoard) {
    if !board.is_asking() {
        return;
    }
    let (plan, conflicts, each, picks) = {
        let d = board.lock();
        let Some(a) = d.asking.as_ref() else { return };
        (d.plan.clone(), a.conflicts.clone(), a.each, a.picks.clone())
    };
    let title = batch_core::title_of(&plan);
    let count = copy_text(
        "rsFilewinBatch.ask.count",
        &[("n", &conflicts.len().to_string())],
    );
    let mut toggle = false;
    let body = |ui: &mut egui::Ui| {
        ui.label(egui::RichText::new(&count).color(super::theme::palette(ui.ctx()).text2));
        ui.add_space(6.0);
        if each {
            for c in &conflicts {
                ui.horizontal(|ui| {
                    ui.label(String::from_utf8_lossy(c.name.bytes()).into_owned());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let now = picks.get(&c.key).copied().unwrap_or(Decide::Skip);
                        let mut opts = vec![
                            (Decide::Skip, copy_text("rsFilewinBatch.pick.skip", &[])),
                            (
                                Decide::KeepBoth,
                                copy_text("rsFilewinBatch.pick.keepBoth", &[]),
                            ),
                        ];
                        if !c.dir_into {
                            opts.push((
                                Decide::Overwrite,
                                copy_text("rsFilewinBatch.pick.overwrite", &[]),
                            ));
                        }
                        for (d, text) in opts.into_iter().rev() {
                            if ui.selectable_label(now == d, text).clicked() {
                                board.pick(&c.key, d);
                            }
                        }
                    });
                });
            }
        } else {
            let rows: Vec<super::kit::ListRow> = conflicts.iter().map(conflict_row).collect();
            super::kit::list_box(ui, &rows, None);
        }
        ui.add_space(6.0);
        if ui
            .selectable_label(each, copy_text("rsFilewinBatch.btn.each", &[]))
            .clicked()
        {
            toggle = true;
        }
    };
    use super::kit::Btn;
    let cancel = copy_text("rsFilewinBatch.btn.cancel", &[]);
    if each {
        let hit = super::kit::dialog(
            ui.ctx(),
            "filewin-batch-ask",
            &title,
            body,
            &[
                (cancel, Btn::Plain),
                // 照所选做：按钮写那个动作（复制 · 下载 …）。
                (plan.verb.text(), Btn::Primary),
            ],
            1,
            0,
        );
        if toggle {
            board.each(false);
        } else if let Some(k) = hit {
            board.answer(None, k == 1);
        }
        return;
    }
    let hit = super::kit::dialog(
        ui.ctx(),
        "filewin-batch-ask",
        &title,
        body,
        &[
            (cancel, Btn::Plain),
            (copy_text("rsFilewinBatch.btn.skip", &[]), Btn::Primary),
            (copy_text("rsFilewinBatch.btn.keepBoth", &[]), Btn::Plain),
            (copy_text("rsFilewinBatch.btn.overwrite", &[]), Btn::Danger),
        ],
        DEFAULT_ANSWER,
        0,
    );
    if toggle {
        board.each(true);
        return;
    }
    match hit {
        Some(1) => board.answer(Some(Decide::Skip), false),
        Some(2) => board.answer(Some(Decide::KeepBoth), false),
        Some(3) => board.answer(Some(Decide::Overwrite), false),
        Some(_) => board.answer(None, false),
        None => false,
    };
}

/// 冲突框打开时焦点落在哪一颗：「跳过同名」（不毁东西的那一个）。
pub const DEFAULT_ANSWER: usize = 1;

/// 冲突框清单的一行：名字 ｜ `2.1 MB · 10-12 14:02 → 已有 1.8 MB · 10-01 09:10`；目录同名 ⇒ `目录不合并 · 跳过或保留两份`。
pub fn conflict_row(c: &Conflict) -> super::kit::ListRow {
    use cmd_wire::files::FileKind;
    super::kit::ListRow {
        icon: if c.kind == FileKind::Dir {
            egui_phosphor::regular::FOLDER
        } else {
            egui_phosphor::regular::FILE
        },
        name: String::from_utf8_lossy(c.name.bytes()).into_owned(),
        meta: batch_core::conflict_meta(c),
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/batch_tests.rs"]
mod tests;
