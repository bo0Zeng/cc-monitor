//! 计划读面（`plan-*`）的帧面宿主：把这台的事实拼成 [`crate::plan`] 要的几个口 ——
//! pb 在哪（这台偏好里设的路径；没设 ⇒ 各配置根底下的插件目录，问注册表）· 会话在哪些目录里（会话表）· 一个 id 是哪个会话（会话表 ＋ 记录树）。
//!
//! 命令：
//! - `plan-list {dirs?, fresh?}`：这台的工作区与片。目录 ＝ 此刻活着的会话的工作目录 ∪ `dirs`；每个目录让 pb 自己往上找工作区
//!   （认过的目录不再问，`fresh: true` 全部重问）。
//! - `plan-read {workspace}`：一个工作区的成品（[`crate::plan::product`]，顶上带 `rev` · `readAt` · `stale`）。
//! - `plan-cell-view {workspace, slice, id}`：一格的 `agent_view`（上一次读好的那一份里的）。
//! - `plan-command {workspace, cmd: continue|pause|take|view, slice?}`：以人的身份代敲 pb 的命令（写盘的是 pb）；`take` 接 `slice` 那一片的仓库。
//! - `plan-files {dir}`：文件窗口反查（[`crate::plan::owners`]）：这个目录落在哪一片的仓库里、每份文件归哪一格。
//! - `plan-pb-check {dir}`：这个路径能不能当 pb 用（[`locate::check`]），设置里存 pb 路径之前问。
//!
//! 读过的工作区就开始盯（[`crate::plan::watch`]），变了推 `changed {plan}`。

use crate::plan::{book, locate, watch, wire, Live, Whose};
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Cx, Handle};
use copy_core::copy_text;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn bad(what: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(what))
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 这台的配置根：默认那一个 ＋ 账号库里每个号的配置目录。
fn config_roots() -> Vec<PathBuf> {
    let mut roots = vec![crate::history::listing::agent_home()];
    if let Some(home) = crate::platform::paths::home_dir() {
        roots.extend(
            crate::faces::rotation_face::library_at(&home)
                .accounts
                .into_iter()
                .filter_map(|a| a.dir),
        );
    }
    roots.dedup();
    roots
}

/// pb 的入口脚本（这台设了路径只认它，没设认装着的插件）；找不到 ⇒ `no_pb`。
fn entry() -> Result<PathBuf, Fail> {
    let set = crate::control::machine_prefs::pb_dir_now();
    locate::pick(set.as_deref(), &crate::agents::plugins(&config_roots()))
        .map_err(|m| Fail::new("no_pb", m.said()))
}

/// 生产的「这个 id 是谁」：活会话表（判活与在干什么同主窗口标签页）· 有它的记录（哪一家都算）⇒ 会话；
/// 记录树里有它的子 agent 记录 ⇒ 挂在父会话底下（活不活看父会话）；都不中 ⇒ 认不出。
fn who_port() -> impl Fn(&str) -> Whose {
    let home = crate::history::listing::agent_home();
    let root = crate::agents::records_root(&home);
    let doing = crate::observe::accounts_query::live_doing(&home);
    let live = move |sid: &str| {
        doing.get(sid).map(|d| Live {
            activity: d.activity,
            needs: d.needs,
        })
    };
    let memo = std::sync::Mutex::new(BTreeMap::<String, Whose>::new());
    move |id: &str| {
        if let Some(w) = memo.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            return w.clone();
        }
        let w = if let Some(l) = live(id) {
            Whose::Session {
                sid: id.to_string(),
                live: Some(l),
            }
        } else if crate::agents::find_session(&home, id).is_ok() {
            Whose::Session {
                sid: id.to_string(),
                live: None,
            }
        } else if let Some(parent) = root
            .as_deref()
            .and_then(|r| crate::agents::child_parent_sid(r, id))
        {
            let l = live(&parent);
            Whose::Subagent { parent, live: l }
        } else {
            Whose::Unknown
        };
        memo.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), w.clone());
        w
    }
}

/// 此刻的认可与退回记录（读不懂 ⇒ 当什么都没记）。
fn review_now() -> crate::plan::review::Review {
    crate::plan::review::current(crate::plan::review::review_path().as_deref())
}

/// 给本子出的成品标上认可 · 要你看的数 · 退回的状态（每次答之前现读那份记录，本子里存的是没标的）。不写时刻的字（[`annotated`] 写）。
pub(crate) fn marked(mut doc: Value) -> Value {
    review_now().annotate(&mut doc);
    doc
}

/// [`marked`] ＋ 时刻写成给人看的字（按请求带来的看的那一台的时区）：要交出去的那一份走它。
pub(crate) fn annotated(doc: Value, tz: &crate::Tz) -> Value {
    let mut doc = marked(doc);
    crate::plan::product::with_time_texts(
        &mut doc,
        i64::try_from(now_ms()).unwrap_or(i64::MAX),
        tz,
    );
    doc
}

/// 读一个目录（并进本子）；读到了就开始盯它的工作区。回的是**没标**认可的那一份（[`annotated`] 再标）。
fn read_dir(entry: &Path, dir: &Path) -> Result<Value, book::Miss> {
    let who = who_port();
    let review = review_now();
    let prior = |ws: &str, slice: &str| review.prior(ws, slice);
    let mut out = book::book().read(entry, dir, &who, now_ms(), &prior)?;
    if let Some(ws) = out
        .get("workspace")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    {
        // pb 的输出随当前目录变（agent_view 里的路径是相对当前目录的）⇒ 一律以工作区根为准再读一次，
        // 摘要才与盯盘那一路（当前目录 ＝ 工作区根）对得上，不会凭空推一帧。
        if ws.as_path() != dir {
            out = book::book().read(entry, &ws, &who, now_ms(), &prior)?;
        }
        arm(entry, &ws);
    }
    Ok(out)
}

fn arm(entry: &Path, ws: &Path) {
    let e = entry.to_path_buf();
    let reread: watch::Reread = std::sync::Arc::new(move |p: &Path| {
        let who = who_port();
        let review = review_now();
        let prior = |ws: &str, slice: &str| review.prior(ws, slice);
        let mut v = book::book().read(&e, p, &who, now_ms(), &prior).ok()?;
        review.annotate(&mut v);
        Some((seen_of(&v)?, watch::spots(&v)))
    });
    let now = book::book().last(&ws.to_string_lossy()).map(marked);
    let spots = now.as_ref().map(watch::spots).unwrap_or_default();
    watch::arm(ws, spots, now.as_ref().and_then(seen_of), reread);
}

/// 一份标过的成品此刻的样子（`changed {plan}` 比的那两样）。
pub(crate) fn seen_of(v: &Value) -> Option<watch::Seen> {
    Some((
        v.get("rev")?.as_str()?.to_string(),
        v.get("needCount").and_then(Value::as_u64).unwrap_or(0),
    ))
}

/// 现读一个工作区（起 pb）并标好；计划审面送退回之前用它拿此刻的接手与状态。
pub(crate) fn read_fresh(ws: &str, tz: &crate::Tz) -> Result<Value, Fail> {
    let entry = entry()?;
    read_dir(&entry, Path::new(ws))
        .map(|d| annotated(d, tz))
        .map_err(miss)
}

/// 出口按线上类型收（拼的是 `Value`）；对不上是拼的那一侧的错 ⇒ `failed`。
pub(crate) fn wired<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, Fail> {
    serde_json::from_value(v).map_err(|e| {
        Fail::new("failed", copy_text("bePlan.face.shape", &[])).with_raw(Some(&e.to_string()))
    })
}

fn miss(m: book::Miss) -> Fail {
    match m {
        book::Miss::NotWorkspace(said) => {
            Fail::new("not_workspace", copy_text("bePlan.face.notWorkspace", &[]))
                .with_raw(Some(&said))
        }
        book::Miss::Unsupported(said) => Fail::new("pb_unsupported", said),
        book::Miss::Failed { said, raw } => Fail::new("failed", said).with_raw(raw.as_deref()),
    }
}

/// `plan-command {workspace, cmd, slice?}`：以人的身份代敲 pb 的一条命令（[`crate::plan::command`]）。
/// 写盘的是 pb；改了工作区配置 ⇒ 盯盘那一路照常推 `changed {plan}`（`auto` · 只写文档 · 此刻那一片都在成品里）。
/// `take` 先现读一次（起 pb），按那一片此刻只写不只写文档 · 是不是此刻那一片定敲哪几条。
fn command(req: &PlanCommandReq, tz: &crate::Tz) -> Result<wire::PlanCmdReply, Fail> {
    let cmd = Some(req.cmd.as_str())
        .filter(|c| crate::plan::command::VERBS.contains(c))
        .ok_or_else(|| bad("`cmd` must be one of continue · pause · take · view"))?;
    if (cmd == "take") != req.slice.is_some() {
        return Err(bad("`slice` goes with `take` and only with it"));
    }
    let entry = entry()?;
    let ws = Path::new(&req.workspace);
    let run = |args: &[&str]| crate::plan::command::run(&entry, ws, args);
    let out = match req.slice.as_deref() {
        Some(name) => {
            let doc = read_fresh(&req.workspace, tz)?;
            let sl = doc
                .get("slices")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
                .ok_or_else(|| {
                    Fail::new(
                        "no_such_slice",
                        copy_text("bePlan.command.noSlice", &[("slice", name)]),
                    )
                })?;
            let flag = |k: &str| sl.get(k).and_then(Value::as_bool) == Some(true);
            let t = crate::plan::command::Taking {
                slice: name,
                path: sl.get("path").and_then(Value::as_str).unwrap_or(name),
                docs_only: flag("docsOnly"),
                current: flag("current"),
            };
            crate::plan::command::take_with(&t, &run)
        }
        None => run(&[cmd]),
    };
    out.map_err(|(code, said, raw)| Fail::new(code, said).with_raw(raw.as_deref()))
        .and_then(wired::<wire::PlanCmdReply>)
}

/// `plan-files {dir}`：文件窗口反查。只看读好过的工作区（主窗口与需手动的账起来就问过 `plan-list`）——
/// 不为文件窗口走到的每个目录起一次 pb；目录在最深的那个工作区里才算。不在任何一片的仓库里 ⇒ `slice: null`（文件窗口什么都不多）。
fn files(dir: &str, tz: &crate::Tz) -> Result<wire::PlanFiles, Fail> {
    let b = book::book();
    let ws = b
        .workspaces()
        .into_iter()
        .filter(|w| {
            let w = w.trim_end_matches('/');
            dir.strip_prefix(w)
                .is_some_and(|r| r.is_empty() || r.starts_with('/'))
        })
        .max_by_key(String::len);
    let doc = ws
        .and_then(|w| b.last(w.as_str()))
        .map(|d| annotated(d, tz));
    let out = match doc {
        Some(d) => crate::plan::owners::of_dir(&d, dir),
        None => {
            json!({"workspace": null, "slice": null, "unreadable": null, "entries": [], "unowned": null})
        }
    };
    wired::<wire::PlanFiles>(out)
}

fn list(req: &PlanListReq, tz: &crate::Tz) -> Result<wire::PlanList, Fail> {
    let mut dirs: Vec<String> =
        crate::observe::accounts_query::live_cwds(&crate::history::listing::agent_home());
    dirs.extend(req.dirs.iter().flatten().cloned());
    let entry = match entry() {
        Ok(e) => e,
        Err(f) => {
            return wired::<wire::PlanList>(
                json!({ "pb": { "state": "missing", "said": f.message }, "workspaces": [] }),
            )
        }
    };
    list_in(
        book::book(),
        dirs,
        req.fresh.unwrap_or(false),
        &|dir| read_dir(&entry, dir),
        tz,
    )
}

/// [`list`] 的本体：本子 · 要问的目录 · 认过的也重问不重问 · 起 pb 读一个目录的那一下（判据换成替身数次数）。
/// 不重问时：认过的目录照本子里那一份；认不出的（不是工作区 · 读不成）也照本子里记的那一个结局，不再起 pb。
pub(crate) fn list_in(
    b: &book::Book,
    mut dirs: Vec<String>,
    fresh: bool,
    read: &dyn Fn(&Path) -> Result<Value, book::Miss>,
    tz: &crate::Tz,
) -> Result<wire::PlanList, Fail> {
    dirs.sort();
    dirs.dedup();
    let mut seen: BTreeMap<String, Value> = BTreeMap::new();
    let mut pb = json!({ "state": "ok", "said": null });
    for d in &dirs {
        let dir = Path::new(d);
        let unread = (!fresh).then(|| b.unread(dir)).flatten();
        let cached = match b.known_dir(dir) {
            _ if unread.is_some() => None,
            Some(None) if !fresh => continue,
            Some(Some(ws)) if !fresh => {
                if seen.contains_key(&ws) {
                    continue;
                }
                b.last(&ws)
            }
            _ => None,
        };
        let got = match (cached, unread) {
            (Some(v), _) => Ok(v),
            (None, Some(m)) => Err(m),
            (None, None) => read(dir),
        };
        match got.map(|d| annotated(d, tz)) {
            Ok(v) => {
                let ws = v
                    .get("workspace")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                seen.entry(ws).or_insert(v);
            }
            Err(book::Miss::NotWorkspace(_)) => {}
            Err(book::Miss::Unsupported(said)) => {
                pb = json!({ "state": "unsupported", "said": said })
            }
            Err(book::Miss::Failed { .. }) => {}
        }
    }
    let workspaces: Vec<Value> = seen.values().map(crate::plan::product::list_row).collect();
    if let Some(v) = seen.values().next() {
        pb["version"] = v["pb"].clone();
    }
    wired::<wire::PlanList>(json!({ "pb": pb, "workspaces": workspaces }))
}

/// 这台计划里要人看的那几片（工作区 · 片名 · 几格，照 `plan-list` 的先后）：同 `plan-list {}` 那一路取（认过的目录照本子里那一份，不重问 pb）。
/// 读不出 ⇒ 没有（计划那一截空着，不挡会话那几项）。
pub(crate) fn need_slices(tz: &crate::Tz) -> Vec<(String, String, u32)> {
    let Ok(list) = list(&PlanListReq::default(), tz) else {
        return Vec::new();
    };
    list.workspaces
        .iter()
        .flat_map(|ws| {
            ws.slices
                .iter()
                .filter(|sl| sl.need_count > 0)
                .map(|sl| (ws.workspace.clone(), sl.name.clone(), sl.need_count))
        })
        .collect()
}

/// `plan-list` 的入参。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanListReq {
    /// 另要问的目录（这台的路径）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) dirs: Option<Vec<String>>,
    /// `true` ⇒ 认过的目录也重问 pb。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) fresh: Option<bool>,
}

/// `plan-read` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanReadReq {
    /// 工作区根（`plan-list` 给的那个）。
    pub(crate) workspace: String,
}

/// `plan-cell-view` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanCellViewReq {
    /// 工作区根。
    pub(crate) workspace: String,
    /// 片名。
    pub(crate) slice: String,
    /// 格的编号。
    pub(crate) id: String,
}

/// `plan-cell-view` 的回包：pb 印给站在这一格的 agent 的那一段，原样。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct PlanCellView {
    pub(crate) view: String,
}

/// `plan-command` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanCommandReq {
    /// 工作区根。
    pub(crate) workspace: String,
    /// `continue` · `pause` · `take` · `view`。
    #[cfg_attr(test, ts(type = "\"continue\" | \"pause\" | \"take\" | \"view\""))]
    pub(crate) cmd: String,
    /// `take` 接哪一片的仓库（片名）；别的动词不带。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) slice: Option<String>,
}

/// `plan-pb-check` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanPbCheckReq {
    /// 要校验的插件根目录（那台机器上的绝对路径）。
    pub(crate) dir: String,
}

/// `plan-files` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanFilesReq {
    /// 那台机器上的绝对路径。
    pub(crate) dir: String,
}

typed::codes! {
    /// `plan-list` 的码：拼出来的成品对不上线上类型（拼的那一侧的错）。
    pub(crate) enum PlanListErr {
        Failed = "failed",
    }
}

typed::codes! {
    /// `plan-read` · `plan-command` 的码（起 pb 的那几种）。
    pub(crate) enum PlanPbErr {
        Failed = "failed",
        NoPb = "no_pb",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
    }
}

typed::codes! {
    /// `plan-command` 的码：同 [`PlanPbErr`]，另有 pb 拒了（或这一侧带着 agent 身份）· 没有那一片。
    pub(crate) enum PlanCommandErr {
        Failed = "failed",
        NoPb = "no_pb",
        NoSuchSlice = "no_such_slice",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
        Refused = "refused",
    }
}

typed::codes! {
    /// `plan-cell-view` 的码：上一次读好的那一份里没有这一格的视角。
    pub(crate) enum PlanCellViewErr {
        NoView = "no_view",
    }
}

/// `plan-list`：这台的 pb 工作区与片（起 pb ⇒ 阻塞档）。
pub(crate) struct PlanListCmd;

impl Command for PlanListCmd {
    const NAME: &'static str = "plan-list";
    type Req = PlanListReq;
    type Reply = wire::PlanList;
    type Err = PlanListErr;
}

impl Handle for PlanListCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanListReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanList, PlanListErr>> + Send + 'static
    {
        async move { list(&req, &cx.tz).map_err(|f| typed::of_fail(f, PlanListErr::Failed)) }
    }
}

/// `plan-read`：一个工作区的成品（起 pb ⇒ 阻塞档）。
pub(crate) struct PlanReadCmd;

impl Command for PlanReadCmd {
    const NAME: &'static str = "plan-read";
    type Req = PlanReadReq;
    type Reply = wire::PlanRead;
    type Err = PlanPbErr;
}

impl Handle for PlanReadCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanReadReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanRead, PlanPbErr>> + Send + 'static {
        async move {
            read_fresh(&req.workspace, &cx.tz)
                .and_then(wired::<wire::PlanRead>)
                .map_err(|f| typed::of_fail(f, PlanPbErr::Failed))
        }
    }
}

/// `plan-cell-view`：一格的 agent 视角（读本子里那一份 ⇒ 阻塞档，同族）。
pub(crate) struct PlanCellViewCmd;

impl Command for PlanCellViewCmd {
    const NAME: &'static str = "plan-cell-view";
    type Req = PlanCellViewReq;
    type Reply = PlanCellView;
    type Err = PlanCellViewErr;
}

impl Handle for PlanCellViewCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanCellViewReq,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PlanCellView, PlanCellViewErr>> + Send + 'static
    {
        async move {
            book::book()
                .view(&req.workspace, &req.slice, &req.id)
                .map(|view| PlanCellView { view })
                .ok_or_else(|| PlanCellViewErr::NoView(copy_text("bePlan.face.noView", &[])))
        }
    }
}

/// `plan-command`：以人的身份代敲 pb 的用户命令（起 pb ⇒ 阻塞档）。
pub(crate) struct PlanCommandCmd;

impl Command for PlanCommandCmd {
    const NAME: &'static str = "plan-command";
    type Req = PlanCommandReq;
    type Reply = wire::PlanCmdReply;
    type Err = PlanCommandErr;
}

impl Handle for PlanCommandCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanCommandReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanCmdReply, PlanCommandErr>> + Send + 'static
    {
        async move { command(&req, &cx.tz).map_err(|f| typed::of_fail(f, PlanCommandErr::Failed)) }
    }
}

/// `plan-files`：文件窗口反查（只读本子里那一份，不起 pb；读认可那份小文件 ⇒ 阻塞档）。
pub(crate) struct PlanFilesCmd;

impl Command for PlanFilesCmd {
    const NAME: &'static str = "plan-files";
    type Req = PlanFilesReq;
    type Reply = wire::PlanFiles;
    type Err = PlanListErr;
}

impl Handle for PlanFilesCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanFilesReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanFiles, PlanListErr>> + Send + 'static
    {
        async move { files(&req.dir, &cx.tz).map_err(|f| typed::of_fail(f, PlanListErr::Failed)) }
    }
}

/// `plan-pb-check`：一个路径能不能当 pb 用（起一次 pb ⇒ 阻塞档；不写盘）。
pub(crate) struct PlanPbCheckCmd;

impl Command for PlanPbCheckCmd {
    const NAME: &'static str = "plan-pb-check";
    type Req = PlanPbCheckReq;
    type Reply = wire::PlanPbCheck;
    type Err = PlanListErr;
}

impl Handle for PlanPbCheckCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanPbCheckReq,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanPbCheck, PlanListErr>> + Send + 'static
    {
        async move {
            let dir = Path::new(req.dir.trim());
            if !dir.is_absolute() {
                return Err(PlanListErr::BadArgs(crate::common::contract::malformed(
                    "`dir` must be an absolute path",
                )));
            }
            wired::<wire::PlanPbCheck>(locate::check(dir))
                .map_err(|f| typed::of_fail(f, PlanListErr::Failed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_face_tests.rs"]
mod tests;
