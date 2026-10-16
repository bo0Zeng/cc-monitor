//! **一组一条命令**在窗口里的那一层：复制到另一栏 · 复制到另一台 · 删除 · 改权限；下载 · 上传那两组的开单也经这里问同名
//! （开了单之后各项怎么传见 [`super::download`] · [`super::transfer`]）。
//!
//! 「先探 → 问一次 → 做 → 逐项结局」住后端（一组一条命令），窗口只剩：
//!
//! ```text
//! 发一条（带票）→ 回 `exists` ⇒ 照 `data.conflicts` 画一次冲突框 → 带 `decide`、同一张票再发
//!              → 通道那一跳断了 ⇒ 等那台连回来、同一张票再问（那台照常做完，回的是同一份结局，不重做）
//!              → 回包 ⇒ 结局那一行直接画后端拼好的 `said`，没成的那几项照 `outcomes` 列
//! 进度：订 `files-batch/<票>`（那一组在哪台做，哪台推；按字节的那一种画字节）
//! 撤：  `files-batch-stop {ticket}`（做完手上那一项就停，剩下的回 `cancelled`）
//! ```
//!
//! 票是窗口铸的（一组一张 UUID）：后端按票记的那几种（同台三条 · 两台之间复制）每一组都带，删一整棵树也撤得了；
//! 下载 · 上传开单不按票记（开了单的每一项各是传输台里的一单），不带。

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use copy_core::copy_text;

/// 一组那一问等多久（到点 ⇒ 当它断了：同一张票再问，那台还在做就等它做完）。
pub const BATCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 下载一组那一条（monitor 补拨号请求、转给本机后端的传输台）。
pub const CMD_DOWNLOAD: &str =
    <cmd_wire::transfer::TransferDownloadAsk as cmd_wire::files::Call>::NAME;

/// 撤一组那一条（后端 `files-batch-stop`）。
pub const CMD_STOP: &str = <cmd_wire::batch::FilesBatchStopReq as cmd_wire::files::Call>::NAME;

/// 后端「有同名、还没答」那个码（`data` 是同名清单）。
pub const EXISTS: &str = "exists";

/// 一组那一问没成的样子（[`super::source::ask_batch`] 交回）。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rebuff {
    pub failed: super::source::Failed,
    /// 对端拒时带的 `data`（`exists` ⇒ 同名清单）。
    pub data: Option<serde_json::Value>,
    /// 通道那一跳没走通（断了 · 到点）：那台可能已经在做 ⇒ 同一张票再问。
    pub cut: bool,
}

/// 哪一种动作（标题的动词 · 冲突框的标题）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Delete,
    Chmod,
    Download,
    Upload,
}

impl Verb {
    /// 后端按票记这一种吗（下载 · 上传开单不记：开了单的每一项各是传输台里的一单）。
    pub fn ticketed(self) -> bool {
        !matches!(self, Verb::Download | Verb::Upload)
    }

    /// 回包是开了哪几单（下载 · 上传），不是一组的结局。
    pub fn opens(self) -> bool {
        matches!(self, Verb::Download | Verb::Upload)
    }

    pub fn text(self) -> String {
        match self {
            Verb::Copy => copy_text("rsFilewinBatch.verb.copy", &[]),
            Verb::Delete => copy_text("rsFilewinBatch.verb.delete", &[]),
            Verb::Chmod => copy_text("rsFilewinBatch.verb.chmod", &[]),
            Verb::Download => copy_text("rsFilewinBatch.verb.download", &[]),
            Verb::Upload => copy_text("rsFilewinBatch.verb.upload", &[]),
        }
    }
}

/// 一组要发的那一条：命令 · 入参（不含票与 `decide`）· 各项给人看的名字（按序号，同后端的键）。
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub verb: Verb,
    pub cmd: &'static str,
    pub args: serde_json::Value,
    pub names: Vec<String>,
    /// 落到哪（复制那一种；结局与冲突框的标题写它）。
    pub into: Option<String>,
    /// 在哪台做（断开那一行写它）。
    pub machine: String,
}

/// 冲突框里摆着的那一问。
pub struct Asking {
    pub conflicts: Vec<Conflict>,
    /// 逐项选展开着吗。
    pub each: bool,
    /// 逐项选的答（缺 ⇒ 跳过）。
    pub picks: BTreeMap<String, Decide>,
    answer: Option<tokio::sync::oneshot::Sender<Option<BTreeMap<String, Decide>>>>,
}

/// 这一组此刻在哪一步。
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Running,
    /// 冲突框摆着（进度表里那一行先不画）。
    Asking,
    /// 断开了，等那台连回来再带票问。
    Cut(String),
    /// 后端回了结局。
    Ended(BatchReply),
    /// 没成：那一句 ＋ 复制详情。
    Failed(super::source::Failed),
    /// 冲突框上答了「取消」：一项都没做。
    Dropped,
    /// 回包不是一组的结局（下载 · 上传一组回的是开了哪几单）：交给调用方接着看。
    Opened,
}

/// 一组的进度（`files_batch` 帧那一格）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub done: u64,
    pub total: u64,
    pub bytes: Option<(u64, u64)>,
}

impl Tick {
    /// 帧那一格的文本 ⇒ 一格（缺 `done` / `total` ⇒ 不认）。
    pub fn of_cell(v: &serde_json::Value) -> Option<Tick> {
        let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64);
        Some(Tick {
            done: n("done")?,
            total: n("total")?,
            bytes: n("bytes_done").zip(n("bytes_total")),
        })
    }

    /// 走到几成：报了字节就照字节，不然照项数。
    pub fn frac(&self) -> Option<f32> {
        match self.bytes {
            Some((d, t)) if t > 0 => Some(d as f32 / t as f32),
            _ if self.total > 0 => Some(self.done as f32 / self.total as f32),
            _ => None,
        }
    }
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
        self.lock().phase == Phase::Asking
    }

    /// 落一步（[`run`] 走它；截图台架也用它摆出那几态）。
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
            d.phase = Phase::Asking;
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
            let map = if each {
                Some(
                    a.conflicts
                        .iter()
                        .map(|c| {
                            let pick = a.picks.get(&c.key).copied().unwrap_or(Decide::Skip);
                            (c.key.clone(), pick)
                        })
                        .collect(),
                )
            } else {
                all.map(|d| a.conflicts.iter().map(|c| (c.key.clone(), d)).collect())
            };
            d.phase = Phase::Running;
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
        let p = self.plan();
        title_of(&p)
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
        failed_lines(&plan, &r)
    }
}

/// 一组的标题（冲突框 · 进度那一行）。
pub fn title_of(p: &Plan) -> String {
    let (action, n) = (p.verb.text(), p.names.len().to_string());
    match &p.into {
        Some(dir) => copy_text(
            "rsFilewinBatch.ask.title",
            &[("action", &action), ("n", &n), ("dir", dir)],
        ),
        None => copy_text(
            "rsFilewinBatch.row.title",
            &[("action", &action), ("n", &n)],
        ),
    }
}

/// 一项的键 ⇒ 给人看的名字（目录里那几格是 `序号/相对段`）。
pub fn name_of(p: &Plan, key: &str) -> String {
    let (head, rest) = match key.split_once('/') {
        Some((h, r)) => (h, Some(r)),
        None => (key, None),
    };
    let top = head
        .parse::<usize>()
        .ok()
        .and_then(|i| p.names.get(i))
        .cloned()
        .unwrap_or_else(|| head.to_string());
    match rest {
        Some(r) => format!("{top}/{r}"),
        None => top,
    }
}

/// 没成的那几项（失败 · 探完之后才冒出来的同名）⇒ 一项一行：`notes.md  失败 · 无权限`。
pub fn failed_lines(p: &Plan, r: &BatchReply) -> Vec<String> {
    r.outcomes
        .iter()
        .filter(|o| matches!(o.state, ItemState::Failed | ItemState::Exists))
        .map(|o| {
            copy_text(
                "rsFilewinProgress.detail.failedOne",
                &[("name", &name_of(p, &o.key)), ("why", &why_of(o))],
            )
        })
        .collect()
}

fn why_of(o: &BatchOutcome) -> String {
    match (&o.reason, o.state) {
        (Some(r), _) => r.clone(),
        (None, ItemState::Exists) => copy_text("rsFilewinBatch.why.exists", &[]),
        _ => o.code.clone().unwrap_or_default(),
    }
}

/// 复制详情：标题 ＋ 后端那一行 ＋ 没成的那几项各带它的详情（都成了 ⇒ `None`）。
pub fn copy_details(p: &Plan, r: &BatchReply) -> Option<String> {
    let segs: Vec<String> = r
        .outcomes
        .iter()
        .filter(|o| matches!(o.state, ItemState::Failed | ItemState::Exists))
        .map(|o| {
            let head = copy_text(
                "rsFilewinProgress.detail.failedOne",
                &[("name", &name_of(p, &o.key)), ("why", &why_of(o))],
            );
            match &o.detail {
                Some(d) if !d.trim().is_empty() => format!("{head}\n{d}"),
                _ => head,
            }
        })
        .collect();
    (!segs.is_empty()).then(|| format!("{}\n{}\n\n{}", title_of(p), r.said, segs.join("\n\n")))
}

/// 「重试失败的」那一组：只含没成的那几项（失败 · 探完之后才冒出来的同名；目录里的那一格算它顶上那一项），
/// 别的入参照原样（根 · 落点 · 权限位）；票在另起那一组时新铸。一项都没失败 ⇒ `None`。
pub fn retry_plan(p: &Plan, r: &BatchReply) -> Option<Plan> {
    let mut keep: Vec<usize> = r
        .outcomes
        .iter()
        .filter(|o| matches!(o.state, ItemState::Failed | ItemState::Exists))
        .filter_map(|o| {
            o.key
                .split_once('/')
                .map_or(o.key.as_str(), |(head, _)| head)
                .parse::<usize>()
                .ok()
        })
        .collect();
    keep.sort_unstable();
    keep.dedup();
    let items = p.args.get("items")?.as_array()?;
    let picked: Vec<serde_json::Value> =
        keep.iter().filter_map(|&i| items.get(i).cloned()).collect();
    if picked.is_empty() {
        return None;
    }
    let mut args = p.args.clone();
    args["items"] = serde_json::Value::Array(picked);
    Some(Plan {
        args,
        names: keep
            .iter()
            .filter_map(|&i| p.names.get(i).cloned())
            .collect(),
        ..p.clone()
    })
}

/// 断开那一行：`devbox 断开 · 复制 5 项 · 完成 2 · 其余待核`。
pub fn cut_line(p: &Plan, t: &Tick) -> String {
    copy_text(
        "rsFilewinBatch.row.cut",
        &[
            ("machine", &p.machine),
            ("action", &p.verb.text()),
            ("n", &p.names.len().to_string()),
            ("ok", &t.done.to_string()),
        ],
    )
}

/// 🔴 **一组的全过程**：发（带票）→ 同名就问一次、带 `decide` 再发 → 断了就等连回来、同一张票再问 → 结局。
///
/// - `ask`：发一次（入参里已有票，答过冲突框之后多 `decide`）；
/// - `wait_up`：等那台连回来（断开那一支）。
///
/// 结局落在看板上（[`Phase::Ended`] · [`Phase::Failed`] · [`Phase::Dropped`] · [`Phase::Opened`]）；
/// 回包不是一组的结局（下载开单）⇒ 原样交回给调用方。
pub async fn run<C, CF, U, UF>(board: &BatchBoard, ask: C, wait_up: U) -> Option<serde_json::Value>
where
    C: Fn(serde_json::Value) -> CF,
    CF: Future<Output = Result<serde_json::Value, Rebuff>>,
    U: Fn() -> UF,
    UF: Future<Output = ()>,
{
    let mut args = board.plan().args;
    if board.plan().verb.ticketed() {
        args["ticket"] = serde_json::Value::String(board.ticket());
    }
    let mut cuts = 0usize;
    loop {
        match ask(args.clone()).await {
            Ok(v) => {
                if board.plan().verb.opens() {
                    board.set(Phase::Opened);
                    return Some(v);
                }
                let phase = match serde_json::from_value::<BatchReply>(v) {
                    Ok(r) => Phase::Ended(r),
                    Err(e) => Phase::Failed(super::source::Failed {
                        code: None,
                        said: copy_text("rsFilewinSource.said.badReply", &[]),
                        detail: e.to_string(),
                    }),
                };
                board.set(phase);
                return None;
            }
            Err(r) if r.failed.code.as_deref() == Some(EXISTS) && r.data.is_some() => {
                let conflicts = r
                    .data
                    .and_then(|d| serde_json::from_value::<Conflicts>(d).ok())
                    .map(|c| c.conflicts)
                    .unwrap_or_default();
                if conflicts.is_empty() {
                    board.set(Phase::Failed(r.failed));
                    return None;
                }
                match board.ask(conflicts).await.ok().flatten() {
                    Some(map) => {
                        let mut d = args
                            .get("decide")
                            .and_then(|v| v.as_object().cloned())
                            .unwrap_or_default();
                        for (k, v) in map {
                            d.insert(k, serde_json::to_value(v).unwrap_or_default());
                        }
                        args["decide"] = serde_json::Value::Object(d);
                    }
                    None => {
                        board.set(Phase::Dropped);
                        return None;
                    }
                }
            }
            // 连着断（连回来又断）到 [`CUTS_MAX`] 次 ⇒ 照实说没走通（不在这里空转）。
            Err(r) if r.cut && cuts < CUTS_MAX => {
                cuts += 1;
                let t = board.tick();
                board.set(Phase::Cut(cut_line(&board.plan(), &t)));
                wait_up().await;
                board.set(Phase::Running);
            }
            Err(r) => {
                board.set(Phase::Failed(r.failed));
                return None;
            }
        }
    }
}

/// 断了再问最多几次（每次都先等那台连回来）。
pub const CUTS_MAX: usize = 8;

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
    run(
        &board,
        |args| {
            let (line, at) = (line.clone(), at.clone());
            async move { super::source::ask_batch(&line, &at, cmd, &args, BATCH_BUDGET).await }
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
    let title = title_of(&plan);
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
                        let opts: Vec<(Decide, String)> =
                            c.answers.iter().map(|d| (*d, pick_text(*d))).collect();
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
    // 整组答：清单里有一项给了的答才画那一颗（覆盖只对给了它的那几项；别的照「目录不合并」跳过）。
    let given = |d: Decide| conflicts.iter().any(|c| c.answers.contains(&d));
    let mut answers: Vec<(Option<Decide>, String, Btn)> = vec![(None, cancel, Btn::Plain)];
    for (d, text, btn) in [
        (
            Decide::Skip,
            copy_text("rsFilewinBatch.btn.skip", &[]),
            Btn::Primary,
        ),
        (
            Decide::KeepBoth,
            copy_text("rsFilewinBatch.btn.keepBoth", &[]),
            Btn::Plain,
        ),
        (
            Decide::Overwrite,
            copy_text("rsFilewinBatch.btn.overwrite", &[]),
            Btn::Danger,
        ),
    ] {
        if given(d) {
            answers.push((Some(d), text, btn));
        }
    }
    let buttons: Vec<(String, Btn)> = answers.iter().map(|(_, t, b)| (t.clone(), *b)).collect();
    let hit = super::kit::dialog(
        ui.ctx(),
        "filewin-batch-ask",
        &title,
        body,
        &buttons,
        DEFAULT_ANSWER.min(buttons.len() - 1),
        0,
    );
    if toggle {
        board.each(true);
        return;
    }
    if let Some(k) = hit {
        board.answer(answers.get(k).and_then(|(d, ..)| *d), false);
    }
}

/// 逐项答那一格的字。
fn pick_text(d: Decide) -> String {
    match d {
        Decide::Skip => copy_text("rsFilewinBatch.pick.skip", &[]),
        Decide::KeepBoth => copy_text("rsFilewinBatch.pick.keepBoth", &[]),
        Decide::Overwrite => copy_text("rsFilewinBatch.pick.overwrite", &[]),
    }
}

/// 冲突框打开时焦点落在哪一颗：「跳过同名」（不毁东西的那一个）。
pub const DEFAULT_ANSWER: usize = 1;

/// 冲突框清单的一行：名字 ｜ `2.1 MB · 10-12 14:02 → 已有 1.8 MB · 10-01 09:10`；目录同名 ⇒ `目录不合并 · 跳过或保留两份`。
pub fn conflict_row(c: &Conflict) -> super::kit::ListRow {
    use cmd_wire::files::FileKind;
    // 时刻照抄后端写好的字（按看的那一台的钟），不换算。
    let side = |size: Option<u64>, mtime: &Option<String>| {
        let mut parts = Vec::new();
        if let Some(s) = size {
            parts.push(copy_core::size_text(s));
        }
        if let Some(m) = mtime {
            parts.push(m.clone());
        }
        parts.join(&copy_text("rsFilewinCopy.batch.listSep", &[]))
    };
    let meta = if !c.answers.contains(&Decide::Overwrite) {
        copy_text("rsFilewinBatch.ask.dirNoMerge", &[])
    } else {
        copy_text(
            "rsFilewinBatch.meta.vs",
            &[
                ("mine", &side(c.size, &c.mtime_text)),
                ("there", &side(c.there_size, &c.there_mtime_text)),
            ],
        )
    };
    super::kit::ListRow {
        icon: if c.kind == FileKind::Dir {
            egui_phosphor::regular::FOLDER
        } else {
            egui_phosphor::regular::FILE
        },
        name: String::from_utf8_lossy(c.name.bytes()).into_owned(),
        meta,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/batch_tests.rs"]
mod tests;
