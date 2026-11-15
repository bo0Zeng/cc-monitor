//! **一组一条命令的全过程**：后端 · 桌面文件窗口 · 手机嵌的 Rust 核心同一份（两端各写一份 ⇒ 问法 · 断了怎么接 · 字各走各的）。
//!
//! ```text
//! 发一条（带票）→ 回 `exists` ⇒ 照 `data.conflicts` 问一次 → 带 `decide`、同一张票再发
//!              → 通道那一跳断了 ⇒ 等那台连回来、同一张票再问（那台照常做完，回的是同一份结局，不重做）
//!              → 回包 ⇒ 结局落看板（后端拼好的 `said` 与逐项结局）
//! ```
//!
//! 问那台的那一下、等连回来的那一下、看板（画在哪）由调用方给；这里还住一组的字（标题 · 结局句 · 没成的那几行 ·
//! 断开那一行 · 同名那一行的说明）、「重试失败的」那一组、改权限的撤销那几组、复制一组的公共根。

use std::collections::BTreeMap;
use std::future::Future;

use cmd_wire::batch::{
    BatchOutcome, BatchReply, ClashChoice, Conflict, Conflicts, Decide, ItemState,
};
use cmd_wire::failed::Failed;
use cmd_wire::files::{
    ChmodItem, CopyItem, DeleteItem, FilesChmodReq, FilesCopyReq, FilesDeleteReq, WirePath,
};
use cmd_wire::Call;
use copy_core::copy_text;

pub mod sessions;

/// 一个文件名拆成「名」与「扩展名」（扩展名带点、取最后一个点起；点打头的名字 · 没有点 ⇒ 扩展名空）。
/// 起新名字那几处（保留两份的 `名 (2).ext` · 「复制为」的缺省名）都照它拆，规矩只有这一份。
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// 一组那一问等多久（到点 ⇒ 当它断了：同一张票再问，那台还在做就等它做完）。
pub const BATCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 撤一组那一条（后端 `files-batch-stop`）。
pub const CMD_STOP: &str = <cmd_wire::batch::FilesBatchStopReq as Call>::NAME;

/// 后端「有同名、还没答」那个码（`data` 是同名清单）。
pub const EXISTS: &str = "exists";

/// 断了再问最多几次（每次都先等那台连回来）。
pub const CUTS_MAX: usize = 8;

/// 一行汇总底下几件各自的失败 ⇒ 复制出去的整段：首行是那一行，每件一段（「名字：那一句」＋ 它的详情），
/// 段间空一行；没有一件带详情 ⇒ `None`（不出按钮）。排法住 `copy_core::detail::many`。
pub fn copy_many(head: &str, items: &[(String, Failed)]) -> Option<String> {
    let segs: Vec<(String, String)> = items
        .iter()
        .map(|(name, f)| {
            (
                copy_text(
                    "rsFilewinProgress.detail.failedOne",
                    &[("name", name), ("why", &f.said)],
                ),
                f.detail.clone(),
            )
        })
        .collect();
    copy_core::detail::many(head, &segs)
}

/// 屏上那一句 `shown`（可能套了别的字）＋ 详情 ⇒ 复制出去的整段；没详情 ⇒ `None`（不出按钮）。
pub fn copy_body(shown: &str, detail: &str) -> Option<String> {
    (!detail.trim().is_empty()).then(|| format!("{shown}\n{detail}"))
}

/// 一组那一问没成的样子。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rebuff {
    /// 那一份失败（对端拒时带的 `data` 也在里面：`exists` ⇒ 同名清单）。
    pub failed: Failed,
    /// 通道那一跳没走通（断了 · 到点）：那台可能已经在做 ⇒ 同一张票再问。
    pub cut: bool,
}

/// 哪一种动作（标题的动词 · 冲突框的标题）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Copy,
    Delete,
    Chmod,
    /// 下载：桌面是落到本机的一组 `transfer-copy`；手机是逐项拉进手机本机那一棵（每项一个本机描述符）。
    Download,
    Upload,
}

impl Verb {
    pub fn text(self) -> String {
        match self {
            Verb::Copy => copy_text("render.codeBlock.copy", &[]),
            Verb::Delete => copy_text("rsFilewinWriteops.label.delete", &[]),
            Verb::Chmod => copy_text("beFiles.batch.verbChmod", &[]),
            Verb::Download => copy_text("rsFilewinDownload.label.download", &[]),
            Verb::Upload => copy_text("rsFilewinUpload.label.upload", &[]),
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

/// 这一组此刻在哪一步。
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Running,
    /// 同名那一问摆着。
    Asking,
    /// 断开了，等那台连回来再带票问（那一行的字）。
    Cut(String),
    /// 后端回了结局。
    Ended(BatchReply),
    Failed(Failed),
    /// 同名那一问答了「取消」：一项都没做。
    Dropped,
}

/// 一组的进度（`files_batch` 帧那一格）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tick {
    pub done: u64,
    pub total: u64,
    pub bytes: Option<(u64, u64)>,
}

impl Tick {
    /// 帧那一格 ⇒ 一格（缺 `done` / `total` ⇒ 不认）。
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

/// 一组画在哪（桌面的窗口 · 手机的浮卡）：[`run`] 照它取计划与票、落每一步、摆同名那一问。
pub trait Board {
    fn plan(&self) -> Plan;
    fn ticket(&self) -> String;
    /// 最后一格进度（断开那一行写完成数）。
    fn tick(&self) -> Tick;
    fn set(&self, phase: Phase);
    /// 摆出同名那一问；答了 ⇒ 每个键一格（`None` ＝ 取消）。
    fn ask(
        &self,
        conflicts: Vec<Conflict>,
    ) -> impl Future<Output = Option<BTreeMap<String, Decide>>>;
}

/// 同名那一问的答 ⇒ 每个键一格：`all` ＝ 整组一个答（`None` ＝ 取消）；逐项选（`picks`）⇒ 用逐项的答，没点的那几项跳过。
pub fn answer_of<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    all: Option<Decide>,
    picks: Option<&BTreeMap<String, Decide>>,
) -> Option<BTreeMap<String, Decide>> {
    let keys = keys.into_iter().map(str::to_string);
    match picks {
        Some(p) => Some(
            keys.map(|k| {
                let d = p.get(&k).copied().unwrap_or(Decide::Skip);
                (k, d)
            })
            .collect(),
        ),
        None => all.map(|d| keys.map(|k| (k, d)).collect()),
    }
}

/// 同名那一问的几种答 ⇒ 带字的几颗（`answers` 里有的才出），先后恒为跳过 · 保留两份 · 覆盖：
/// 不毁东西的在前、覆盖在最后，不照那台给的先后。手机抽屉与文件窗口冲突框都照它摆，字只住这里。
pub fn choices(answers: &[Decide]) -> Vec<ClashChoice> {
    [Decide::Skip, Decide::KeepBoth, Decide::Overwrite]
        .into_iter()
        .filter(|d| answers.contains(d))
        .map(|decide| {
            let (label, pick) = match decide {
                Decide::Skip => (
                    copy_text("rsFilewinBatch.btn.skip", &[]),
                    copy_text("rsFilewinBatch.pick.skip", &[]),
                ),
                Decide::KeepBoth => (
                    copy_text("rsFilewinBatch.btn.keepBoth", &[]),
                    copy_text("rsFilewinBatch.btn.keepBoth", &[]),
                ),
                Decide::Overwrite => (
                    copy_text("rsFilewinBatch.btn.overwrite", &[]),
                    copy_text("rsFilewinBatch.btn.overwrite", &[]),
                ),
            };
            ClashChoice {
                decide,
                label,
                pick,
            }
        })
        .collect()
}

/// 整组那几颗：清单里有一项给了的答并起来（[`choices`] 的先后）。
pub fn whole(conflicts: &[Conflict]) -> Vec<ClashChoice> {
    let given: Vec<Decide> = conflicts
        .iter()
        .flat_map(|c| c.answers.iter().copied())
        .collect();
    choices(&given)
}

/// 同名的那一项能怎么答（同名清单里那一格，两端照画）：两头有一头是目录 ⇒ 目录不合并、不给覆盖；
/// `skip` ⇒ 这条命令收不收「跳过」（只一项的上传提交不收：跳过就是什么都不交）。后端探同名 · 手机本机同名都照它。
pub fn answers_of(dir_into: bool, skip: bool) -> Vec<Decide> {
    let mut a = Vec::with_capacity(3);
    if !dir_into {
        a.push(Decide::Overwrite);
    }
    if skip {
        a.push(Decide::Skip);
    }
    a.push(Decide::KeepBoth);
    a
}

/// 保留两份另起名时最多往后试到第几个。
pub const MINT_TRIES: u32 = 999;

/// 保留两份另起的名：`名 (k).ext`（拆法照 [`split_ext`]）；`k` 从 2 起由调用方往后试。
pub fn minted(name: &str, k: u32) -> String {
    let (stem, ext) = split_ext(name);
    format!("{stem} ({k}){ext}")
}

fn same_name(a: &str, b: &str, fold_case: bool) -> bool {
    if fold_case {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

/// `name` 在已有的名字里撞的那一个（`fold_case` ⇒ 不分大小写：手机的共享存储不分）。
pub fn taken_as<'a>(name: &str, here: &'a [String], fold_case: bool) -> Option<&'a str> {
    here.iter()
        .find(|h| same_name(h, name, fold_case))
        .map(String::as_str)
}

/// 本机那一侧的同名清单：要放的名字（按序号）里撞了已有的那几项 ⇒ 后端同名那一形（键是序号，两头都是文件，答法照 [`answers_of`]）。
/// 只知道名字、不知道大小与时刻的那一侧（手机下到本机）照它问。
pub fn local_clashes(names: &[String], here: &[String], fold_case: bool) -> Vec<Conflict> {
    names
        .iter()
        .enumerate()
        .filter(|(_, n)| taken_as(n, here, fold_case).is_some())
        .map(|(i, n)| Conflict {
            key: i.to_string(),
            name: WirePath(n.as_bytes().to_vec()),
            kind: cmd_wire::files::FileKind::File,
            size: None,
            size_text: None,
            mtime_text: None,
            there: cmd_wire::files::FileKind::File,
            there_size_text: None,
            there_mtime_text: None,
            answers: answers_of(false, true),
        })
        .collect()
}

/// 保留两份那一项落成的名：从「名 (2).ext」起往后试，头一个不撞已有的；试满 [`MINT_TRIES`] ⇒ `None`。
pub fn keep_both_name(name: &str, here: &[String], fold_case: bool) -> Option<String> {
    (2..=MINT_TRIES)
        .map(|k| minted(name, k))
        .find(|n| taken_as(n, here, fold_case).is_none())
}

/// 🔴 **一组的全过程**：发（带票）→ 同名就问一次、带 `decide` 再发 → 断了就等连回来、同一张票再问 → 结局。
///
/// - `ask`：发一次（入参里已有票，答过同名之后多 `decide`）；
/// - `wait_up`：等那台连回来（断开那一支）。
///
/// 结局落在看板上（[`Phase::Ended`] · [`Phase::Failed`] · [`Phase::Dropped`]）。
pub async fn run<B, C, CF, U, UF>(board: &B, ask: C, wait_up: U)
where
    B: Board,
    C: Fn(serde_json::Value) -> CF,
    CF: Future<Output = Result<serde_json::Value, Rebuff>>,
    U: Fn() -> UF,
    UF: Future<Output = ()>,
{
    let plan = board.plan();
    let mut args = plan.args.clone();
    args["ticket"] = serde_json::Value::String(board.ticket());
    let mut cuts = 0usize;
    loop {
        match ask(args.clone()).await {
            Ok(v) => {
                let phase = match serde_json::from_value::<BatchReply>(v) {
                    Ok(r) => Phase::Ended(r),
                    Err(e) => Phase::Failed(Failed {
                        detail: e.to_string(),
                        ..Failed::local(copy_text("reason.content.unparsable", &[]))
                    }),
                };
                board.set(phase);
                return;
            }
            Err(r) if r.failed.code == EXISTS && r.failed.data.is_some() => {
                let conflicts = r
                    .failed
                    .data
                    .clone()
                    .and_then(|d| serde_json::from_value::<Conflicts>(d).ok())
                    .map(|c| c.conflicts)
                    .unwrap_or_default();
                if conflicts.is_empty() {
                    board.set(Phase::Failed(r.failed));
                    return;
                }
                board.set(Phase::Asking);
                match board.ask(conflicts).await {
                    Some(map) => {
                        let mut d = args
                            .get("decide")
                            .and_then(|v| v.as_object().cloned())
                            .unwrap_or_default();
                        for (k, v) in map {
                            d.insert(k, serde_json::to_value(v).unwrap_or_default());
                        }
                        args["decide"] = serde_json::Value::Object(d);
                        board.set(Phase::Running);
                    }
                    None => {
                        board.set(Phase::Dropped);
                        return;
                    }
                }
            }
            // 连着断（连回来又断）到 [`CUTS_MAX`] 次 ⇒ 照实说没走通（不在这里空转）。
            Err(r) if r.cut && cuts < CUTS_MAX => {
                cuts += 1;
                board.set(Phase::Cut(cut_line(&plan, &board.tick())));
                wait_up().await;
                board.set(Phase::Running);
            }
            Err(r) => {
                board.set(Phase::Failed(r.failed));
                return;
            }
        }
    }
}

/// 一组的标题：`复制 5 项`（落在一个目录 ⇒ `复制 5 项到 ~/proj/b`）。
pub fn title_of(p: &Plan) -> String {
    let (action, n) = (p.verb.text(), p.names.len().to_string());
    match &p.into {
        Some(dir) => copy_text(
            "rsFilewinBatch.ask.title",
            &[("action", &action), ("n", &n), ("dir", dir)],
        ),
        None => copy_text("beFiles.batch.head", &[("action", &action), ("n", &n)]),
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

fn not_done(o: &BatchOutcome) -> bool {
    matches!(o.state, ItemState::Failed | ItemState::Exists)
}

/// 没成的那几项（失败 · 探完之后才冒出来的同名）⇒ 一项一行：`notes.md  失败 · 无权限`。
pub fn failed_lines(p: &Plan, r: &BatchReply) -> Vec<String> {
    r.outcomes
        .iter()
        .filter(|o| not_done(o))
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
        .filter(|o| not_done(o))
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

/// 键的顶上那一项的序号（目录里那一格 `序号/相对段` 算它顶上那一项）。
fn top_of(key: &str) -> Option<usize> {
    key.split_once('/')
        .map_or(key, |(head, _)| head)
        .parse::<usize>()
        .ok()
}

/// 只留 `keep` 那几项（按序号）的同一组：入参照原样（根 · 落点 · 权限位），票在另起那一组时新铸。
fn subset(p: &Plan, keep: &[usize]) -> Option<Plan> {
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

/// 「重试失败的」那一组：只含没成的那几项。一项都没失败 ⇒ `None`。
pub fn retry_plan(p: &Plan, r: &BatchReply) -> Option<Plan> {
    let mut keep: Vec<usize> = r
        .outcomes
        .iter()
        .filter(|o| not_done(o))
        .filter_map(|o| top_of(&o.key))
        .collect();
    keep.sort_unstable();
    keep.dedup();
    subset(p, &keep)
}

/// 改权限的撤销：做成的那几项（序号 · 改之前的权限位）按改之前那一值并组 ⇒ `{八进制串: [序号…]}`。
/// 有一项做成了却读不到改之前那一值（或不是权限位）⇒ `None`：撤不全就不给撤销，不猜；一项都没做成 ⇒ `None`。
pub fn undo_groups(done: &[(usize, Option<u32>)]) -> Option<BTreeMap<String, Vec<usize>>> {
    let mut out: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &(i, before) in done {
        let m = before.filter(|m| *m <= 0o7777)?;
        out.entry(format!("{m:o}")).or_default().push(i);
    }
    (!out.is_empty()).then_some(out)
}

/// 改权限那一组的撤销 ⇒ 几组（每个改之前的值一组，同一个根）；撤不全 ⇒ `None`（见 [`undo_groups`]）。
pub fn undo_plan(p: &Plan, r: &BatchReply) -> Option<Vec<Plan>> {
    if p.verb != Verb::Chmod {
        return None;
    }
    let done: Vec<(usize, Option<u32>)> = r
        .outcomes
        .iter()
        .filter(|o| o.state == ItemState::Done)
        .map(|o| top_of(&o.key).map(|i| (i, o.before)))
        .collect::<Option<_>>()?;
    undo_groups(&done)?
        .into_iter()
        .map(|(mode, keep)| {
            let mut g = subset(p, &keep)?;
            g.args["mode"] = serde_json::Value::String(mode);
            Some(g)
        })
        .collect()
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

/// 下载那一圈（手机逐项拉）断开那一行：`devbox 连接已断开 · 下载 5 项 · 完成 2 · 中断 3`（没下完的连回来整份重下）。
pub fn cut_left_line(p: &Plan, ok: usize, left: usize) -> String {
    copy_text(
        "beFiles.batch.cut",
        &[
            ("machine", &p.machine),
            ("action", &p.verb.text()),
            ("n", &p.names.len().to_string()),
            ("ok", &ok.to_string()),
            ("left", &left.to_string()),
        ],
    )
}

/// 同名那一行的说明：`2.1 MB · 10-12 14:02 → 已有 1.8 MB · 10-01 09:10`；不给覆盖（目录不合并）⇒ `目录不合并 · 跳过或保留两份`。
/// 大小 · 时刻照抄后端写好的字（按看的那一台的钟），不换算。
pub fn conflict_meta(c: &Conflict) -> String {
    if !c.answers.contains(&Decide::Overwrite) {
        return copy_text("rsFilewinBatch.ask.dirNoMerge", &[]);
    }
    let side = |size: &Option<String>, mtime: &Option<String>| {
        [size, mtime]
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(&copy_text("kit.text.sep", &[]))
    };
    copy_text(
        "rsFilewinBatch.meta.vs",
        &[
            ("mine", &side(&c.size_text, &c.mtime_text)),
            ("there", &side(&c.there_size_text, &c.there_mtime_text)),
        ],
    )
}

/// 一组的结局那一句：全成 ⇒ `已复制 5 项`（落在同一个目录 ⇒ `→ 目录`）；别的 ⇒ `删除 5 项 · 完成 3 · 失败 ×2`，是 0 的那一段不写。
pub fn said_of(verb: &str, outcomes: &[BatchOutcome], into: Option<&str>) -> String {
    let count = |s: &[ItemState]| outcomes.iter().filter(|o| s.contains(&o.state)).count();
    let n = outcomes.len().to_string();
    let ok = count(&[ItemState::Done]);
    if ok == outcomes.len() {
        return match into {
            Some(dir) => copy_text(
                "beFiles.batch.doneInto",
                &[("action", verb), ("n", &n), ("dir", dir)],
            ),
            None => copy_text("beFiles.batch.done", &[("action", verb), ("n", &n)]),
        };
    }
    let skip = count(&[ItemState::Skipped]);
    let bad = count(&[ItemState::Failed, ItemState::Exists]);
    let left = count(&[ItemState::Cancelled]);
    let mut parts = vec![copy_text(
        "beFiles.batch.head",
        &[("action", verb), ("n", &n)],
    )];
    let segs = [
        (
            ok,
            copy_text("beFiles.batch.segOk", &[("ok", &ok.to_string())]),
        ),
        (
            skip,
            copy_text("beFiles.batch.segSkip", &[("skip", &skip.to_string())]),
        ),
        (
            bad,
            copy_text("beFiles.batch.segBad", &[("bad", &bad.to_string())]),
        ),
        (
            left,
            copy_text("beFiles.batch.segLeft", &[("left", &left.to_string())]),
        ),
    ];
    parts.extend(segs.into_iter().filter(|(k, _)| *k > 0).map(|(_, t)| t));
    parts.join(&copy_text("kit.text.sep", &[]))
}

/// 复制一组 → 一条 `files-copy`：根 ＝ 所有源与目标的公共上级（[`WirePath::parent`] · [`WirePath::common_dir`]），
/// 各项 `from` / `to` 是根底下的相对段。
/// 同名缺省问（后端先探整组，有同名回 `exists` 带同名清单）。某一条不在根底下 ⇒ 那一句（不发）。
pub fn copy_req(pairs: &[(Vec<u8>, Vec<u8>)]) -> Result<FilesCopyReq, String> {
    let mut root: Option<Vec<u8>> = None;
    for (f, t) in pairs {
        for p in [f, t] {
            let up = WirePath::parent(p).0;
            root = Some(match root {
                None => up,
                Some(r) => WirePath::common_dir(&r, &up).0,
            });
        }
    }
    let root = root.unwrap_or_else(|| b"/".to_vec());
    let rel = |b: &[u8]| -> Result<WirePath, String> {
        let cut = if root == b"/" { 1 } else { root.len() + 1 };
        (b.starts_with(&root)
            && b.len() > cut
            && (root == b"/" || b.get(root.len()) == Some(&b'/')))
        .then(|| WirePath(b[cut..].to_vec()))
        .ok_or_else(|| {
            copy_text(
                "rsFilewinWorkspace.acrossArgs.notUnder",
                &[
                    ("path", &String::from_utf8_lossy(b)),
                    ("root", &String::from_utf8_lossy(&root)),
                ],
            )
        })
    };
    let mut items = Vec::with_capacity(pairs.len());
    for (f, t) in pairs {
        items.push(CopyItem {
            from: rel(f)?,
            to: rel(t)?,
        });
    }
    Ok(FilesCopyReq {
        root: WirePath(root.clone()),
        items,
        on_exists: None,
        decide: None,
        ticket: None,
        single: None,
    })
}

/// 删当前目录 `root` 底下的这几项（相对段 · 给人看的名字 · 是不是目录：目录 ⇒ 连同里面全部，`recursive`）⇒ 一条 `files-delete`。
pub fn delete_plan(root: WirePath, items: &[(WirePath, String, bool)], machine: &str) -> Plan {
    let req = FilesDeleteReq {
        root,
        items: items
            .iter()
            .map(|(rel, _, dir)| DeleteItem {
                rel: rel.clone(),
                recursive: dir.then_some(true),
                expect: None,
            })
            .collect(),
        ticket: None,
    };
    Plan {
        verb: Verb::Delete,
        cmd: <FilesDeleteReq as Call>::NAME,
        args: serde_json::to_value(&req).unwrap_or_default(),
        names: items.iter().map(|(_, n, _)| n.clone()).collect(),
        into: None,
        machine: machine.to_string(),
    }
}

/// 把 `root` 底下这几项（相对段 · 名字）改成 `mode`（八进制串原样，由那台解）⇒ 一条 `files-chmod`。
pub fn chmod_plan(root: WirePath, items: &[(WirePath, String)], mode: &str, machine: &str) -> Plan {
    let req = FilesChmodReq {
        root,
        items: items
            .iter()
            .map(|(rel, _)| ChmodItem { rel: rel.clone() })
            .collect(),
        mode: mode.to_string(),
        ticket: None,
    };
    Plan {
        verb: Verb::Chmod,
        cmd: <FilesChmodReq as Call>::NAME,
        args: serde_json::to_value(&req).unwrap_or_default(),
        names: items.iter().map(|(_, n)| n.clone()).collect(),
        into: None,
        machine: machine.to_string(),
    }
}

/// 改权限那一框的标题：一项 ⇒ 带它的名字；几项 ⇒ 按件数（不拼名字）。
pub fn chmod_title(names: &[String]) -> String {
    match names {
        [one] => copy_text("rsFilewinWriteops.heading.chmodOne", &[("name", one)]),
        _ => copy_text(
            "rsFilewinSelect.label.chmodMany",
            &[("n", &names.len().to_string())],
        ),
    }
}

/// 改权限那一框底下那一行与要不要预填。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModeLine {
    pub line: String,
    /// 预填进那一栏的八进制串；`None` ＝ 不预填。
    pub prefill: Option<String>,
}

/// 逐项读回来的权限位（`None` ＝ 那一项没读到）⇒ 那一行：全读到且都一样 ⇒ 「现在是 644」、预填；
/// 不一样 ⇒ 「这几项现在的权限不一样」、不预填（填哪一个都是替用户挑）；有一项没读到 · 空 ⇒ 「读不到」、不预填（不猜）。
pub fn mode_line(modes: &[Option<u32>]) -> ModeLine {
    let unreadable = ModeLine {
        line: copy_text("rsFilewinWriteops.mode.unreadable", &[]),
        prefill: None,
    };
    let Some(all) = modes.iter().copied().collect::<Option<Vec<u32>>>() else {
        return unreadable;
    };
    let Some(&m) = all.first() else {
        return unreadable;
    };
    if all.iter().any(|x| *x != m) {
        return ModeLine {
            line: copy_text("rsFilewinWriteops.mode.mixed", &[]),
            prefill: None,
        };
    }
    ModeLine {
        line: copy_text("rsFilewinWriteops.mode.now", &[("mode", &format!("{m:o}"))]),
        prefill: Some(format!("{m:o}")),
    }
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/batch-core/batch_core_tests.rs"]
mod tests;
