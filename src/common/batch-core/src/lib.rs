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

use cmd_wire::batch::{BatchOutcome, BatchReply, Conflict, Conflicts, Decide, ItemState};
use cmd_wire::files::{Call, CopyItem, FilesCopyReq, WirePath};
use copy_core::copy_text;

/// 一组那一问等多久（到点 ⇒ 当它断了：同一张票再问，那台还在做就等它做完）。
pub const BATCH_BUDGET: std::time::Duration = std::time::Duration::from_secs(600);

/// 撤一组那一条（后端 `files-batch-stop`）。
pub const CMD_STOP: &str = <cmd_wire::batch::FilesBatchStopReq as Call>::NAME;

/// 后端「有同名、还没答」那个码（`data` 是同名清单）。
pub const EXISTS: &str = "exists";

/// 断了再问最多几次（每次都先等那台连回来）。
pub const CUTS_MAX: usize = 8;

/// 没成的一下：码（对端拒的才有）· 给人看的那一句 · 「复制详情」那几行（空 ⇒ 不出按钮）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Failed {
    /// 对端拒绝时它给的那个码（`None` = 不是对端拒的：没走通 / 这一侧拼错 / 对端不认这条命令）。
    pub code: Option<String>,
    pub said: String,
    pub detail: String,
}

impl Failed {
    /// 只有一句话（这一侧说的，没有详情）。
    pub fn local(said: String) -> Self {
        Self {
            code: None,
            said,
            detail: String::new(),
        }
    }

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
}

impl From<String> for Failed {
    fn from(said: String) -> Self {
        Failed::local(said)
    }
}

impl From<&str> for Failed {
    fn from(said: &str) -> Self {
        Failed::local(said.to_string())
    }
}

/// 写出来就是给人看的那一句（详情另取）。
impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.said)
    }
}

/// 一组那一问没成的样子。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rebuff {
    pub failed: Failed,
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
            Verb::Copy => copy_text("beFiles.batch.verbCopy", &[]),
            Verb::Delete => copy_text("beFiles.batch.verbDelete", &[]),
            Verb::Chmod => copy_text("beFiles.batch.verbChmod", &[]),
            Verb::Download => copy_text("beFiles.batch.verbDownload", &[]),
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

/// 🔴 **一组的全过程**：发（带票）→ 同名就问一次、带 `decide` 再发 → 断了就等连回来、同一张票再问 → 结局。
///
/// - `ask`：发一次（入参里已有票，答过同名之后多 `decide`）；
/// - `wait_up`：等那台连回来（断开那一支）。
///
/// 结局落在看板上（[`Phase::Ended`] · [`Phase::Failed`] · [`Phase::Dropped`] · [`Phase::Opened`]）；
/// 回包不是一组的结局（下载开单）⇒ 原样交回给调用方。
pub async fn run<B, C, CF, U, UF>(board: &B, ask: C, wait_up: U) -> Option<serde_json::Value>
where
    B: Board,
    C: Fn(serde_json::Value) -> CF,
    CF: Future<Output = Result<serde_json::Value, Rebuff>>,
    U: Fn() -> UF,
    UF: Future<Output = ()>,
{
    let plan = board.plan();
    let mut args = plan.args.clone();
    if plan.verb.ticketed() {
        args["ticket"] = serde_json::Value::String(board.ticket());
    }
    let mut cuts = 0usize;
    loop {
        match ask(args.clone()).await {
            Ok(v) => {
                if plan.verb.opens() {
                    board.set(Phase::Opened);
                    return Some(v);
                }
                let phase = match serde_json::from_value::<BatchReply>(v) {
                    Ok(r) => Phase::Ended(r),
                    Err(e) => Phase::Failed(Failed {
                        code: None,
                        said: copy_text("reason.content.unparsable", &[]),
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
                        return None;
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
                return None;
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
            .join(&copy_text("rsFilewinCopy.batch.listSep", &[]))
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

/// 两条路径（字节）按段比的公共上级目录（没有 ⇒ `/`）。
pub fn common_dir_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    let segs = |x: &[u8]| -> Vec<Vec<u8>> {
        x.split(|c| *c == b'/')
            .filter(|s| !s.is_empty())
            .map(<[u8]>::to_vec)
            .collect()
    };
    let (sa, sb) = (segs(a), segs(b));
    let mut out = Vec::new();
    for (x, y) in sa.iter().zip(sb.iter()) {
        if x != y {
            break;
        }
        out.push(b'/');
        out.extend_from_slice(x);
    }
    if out.is_empty() {
        out.push(b'/');
    }
    out
}

fn parent_bytes(p: &[u8]) -> Vec<u8> {
    let t = p.strip_suffix(b"/").unwrap_or(p);
    match t.iter().rposition(|c| *c == b'/') {
        Some(0) | None => b"/".to_vec(),
        Some(i) => t[..i].to_vec(),
    }
}

/// 复制一组 → 一条 `files-copy`：根 ＝ 所有源与目标的公共上级（按**字节**按段比），各项 `from` / `to` 是根底下的相对段。
/// 同名缺省问（后端先探整组，有同名回 `exists` 带同名清单）。某一条不在根底下 ⇒ 那一句（不发）。
pub fn copy_req(pairs: &[(Vec<u8>, Vec<u8>)]) -> Result<FilesCopyReq, String> {
    let mut root: Option<Vec<u8>> = None;
    for (f, t) in pairs {
        for p in [f, t] {
            let up = parent_bytes(p);
            root = Some(match root {
                None => up,
                Some(r) => common_dir_bytes(&r, &up),
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

/// 把 `from_dir` 底下的这几项复制进 `to_dir`（同名）：手机「复制到…」那一组（[`copy_req`] 照算公共根）。
pub fn copy_plan(
    from_dir: &[u8],
    names: &[Vec<u8>],
    to_dir: &[u8],
    machine: &str,
) -> Result<Plan, String> {
    let join = |d: &[u8], n: &[u8]| {
        let mut p = d.strip_suffix(b"/").unwrap_or(d).to_vec();
        p.push(b'/');
        p.extend_from_slice(n);
        p
    };
    let pairs: Vec<(Vec<u8>, Vec<u8>)> = names
        .iter()
        .map(|n| (join(from_dir, n), join(to_dir, n)))
        .collect();
    let req = copy_req(&pairs)?;
    Ok(Plan {
        verb: Verb::Copy,
        cmd: <FilesCopyReq as Call>::NAME,
        args: serde_json::to_value(&req).map_err(|e| e.to_string())?,
        names: names
            .iter()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect(),
        into: Some(String::from_utf8_lossy(to_dir).into_owned()),
        machine: machine.to_string(),
    })
}

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

#[cfg(test)]
#[path = "../../../../tests/common/batch-core/batch_core_tests.rs"]
mod tests;
