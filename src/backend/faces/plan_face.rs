//! 计划读面（`plan-*`）的帧面宿主：把这台的事实拼成 [`crate::plan`] 要的几个口 ——
//! pb 在哪（这台偏好里设的路径；没设 ⇒ 各配置根底下的插件目录，问注册表）· 会话在哪些目录里（会话表）· 一个 id 是哪个会话（会话表 ＋ 记录树）。
//!
//! 命令：
//! - `plan-list {dirs?, fresh?}`：这台的工作区与片。目录 ＝ 此刻活着的会话的工作目录 ∪ `dirs`；每个目录让 pb 自己往上找工作区
//!   （认过的目录不再问，`fresh: true` 全部重问）。
//! - `plan-read {workspace}`：一个工作区的成品（[`crate::plan::product`]，顶上带 `rev` · `readAt` · `stale`）。
//! - `plan-cell-view {workspace, slice, id}`：一个节点的 agent 视角（点到才起 `pb dump --node`）。
//! - `plan-command {workspace, cmd: continue|pause|adopt|render|repo, slice?, path?, domain?}`：以人的身份代敲 pb 的命令（写盘的是 pb）；
//!   `adopt` 接过 `slice` 那个仓库；`repo` 在 `workspace` 那个目录登记 `path` 那一片（新计划框的［新建］·［切到这一片］）。
//! - `plan-new {dir, path, domain?}`：新计划框问的那一份（[`crate::plan::new_slice`]：能选的领域 · 落在哪 · 撞没撞上 · 要敲的那一行）。
//! - `plan-files {dir}`：文件窗口反查（[`crate::plan::owners`]）：这个目录落在哪一片的仓库里、每份文件归哪一格。
//! - `plan-pb-check {dir}`：这个路径能不能当 pb 用（[`locate::check`]），设置里存 pb 路径之前问。
//! - `plan-refs {sid, texts}`：会话消息里提到的格（[`crate::plan::refs`]：认片在这一侧，认编号交 pb 的 `dump --refs`）。
//!
//! 读过的工作区就开始盯（[`crate::plan::watch`]），变了推 `changed {plan}`。

use crate::plan::{book, locate, watch, wire, Live, Whose};
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Cx, Handle};
use copy_core::copy_text;
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
            crate::accounts::manage::scan::book_at(&home)
                .unwrap_or_default()
                .accounts
                .into_iter()
                .filter_map(|a| a.config_dir.map(PathBuf::from)),
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

/// 生产的「这条会话此刻在不在等人」：活会话表那一判（`LiveTable::waiting`，同 `sessions-needs`），这一次读取取一份表。
fn waiting_port() -> impl Fn(&str) -> bool {
    let table = crate::history::live_table::now(&crate::history::listing::agent_home());
    move |sid: &str| table.waiting(sid)
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
    // 子 agent 的负责人写成「{父会话} 的 agent」：父会话叫什么与标签页同一条规则（改过的名 ＞ 记录里的标题）。
    let annotations = match crate::history::history_annotations::load() {
        crate::history::history_annotations::Loaded::Read(t) => t,
        _ => Default::default(),
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
            let parent_title = crate::history::session_hidden::title_here(&parent, &annotations);
            Whose::Subagent {
                parent,
                parent_title,
                live: l,
            }
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
pub(crate) fn marked(mut doc: wire::PlanRead) -> wire::PlanRead {
    review_now().annotate(&mut doc);
    doc
}

/// [`marked`] ＋ 时刻写成给人看的字（按请求带来的看的那一台的时区）：要交出去的那一份走它。
pub(crate) fn annotated(doc: wire::PlanRead, tz: &crate::Tz) -> wire::PlanRead {
    let mut doc = marked(doc);
    crate::plan::product::with_time_texts(
        &mut doc,
        i64::try_from(now_ms()).unwrap_or(i64::MAX),
        tz,
    );
    doc
}

/// 读一个目录（并进本子）；读到了就开始盯它的工作区。回的是**没标**认可的那一份（[`annotated`] 再标）。
fn read_dir(entry: &Path, dir: &Path) -> Result<wire::PlanRead, book::Miss> {
    let who = who_port();
    let waiting = waiting_port();
    let review = review_now();
    let prior = |ws: &str, slice: &str| review.prior(ws, slice);
    let mut out = book::book().read(entry, dir, &who, &waiting, now_ms(), &prior)?;
    if let Some(ws) = Some(out.workspace.as_str())
        .filter(|w| !w.is_empty())
        .map(PathBuf::from)
    {
        // pb 的输出里印的路径可能随当前目录变 ⇒ 一律以工作区根为准再读一次，
        // 摘要才与盯盘那一路（当前目录 ＝ 工作区根）对得上，不会凭空推一帧。
        if ws.as_path() != dir {
            out = book::book().read(entry, &ws, &who, &waiting, now_ms(), &prior)?;
        }
        arm(entry, &ws);
    }
    Ok(out)
}

fn arm(entry: &Path, ws: &Path) {
    let e = entry.to_path_buf();
    let reread: watch::Reread = std::sync::Arc::new(move |p: &Path| {
        let who = who_port();
        let waiting = waiting_port();
        let review = review_now();
        let prior = |ws: &str, slice: &str| review.prior(ws, slice);
        let mut v = book::book()
            .read(&e, p, &who, &waiting, now_ms(), &prior)
            .ok()?;
        review.annotate(&mut v);
        Some((seen_of(&v), watch::spots(&v)))
    });
    let now = book::book().last(&ws.to_string_lossy()).map(marked);
    let spots = now.as_ref().map(watch::spots).unwrap_or_default();
    watch::arm(ws, spots, now.as_ref().map(seen_of), reread);
}

/// 一份标过的成品此刻的样子（`changed {plan}` 比的那两样）。
pub(crate) fn seen_of(v: &wire::PlanRead) -> watch::Seen {
    (v.rev.clone(), u64::from(v.need_count))
}

/// 现读一个工作区（起 pb）并标好（认可 · 时刻的字 · 每块的接手会话在哪几组）；计划审面送退回之前用它拿此刻的接手与状态。
pub(crate) fn read_fresh(ws: &str, tz: &crate::Tz) -> Result<wire::PlanRead, Fail> {
    let entry = entry()?;
    let mut doc = read_dir(&entry, Path::new(ws))
        .map(|d| annotated(d, tz))
        .map_err(miss)?;
    crate::plan::groups::annotate(&mut doc, &crate::plan::groups::here_ties);
    Ok(doc)
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

/// `plan-new {dir, path, domain?}`：开一片之前问的那一份（[`crate::plan::new_slice`]，只起 `pb dump --domains`）。
/// 那个工作区已有哪几片照本子里读好过的那一份（没读好过现读一次）。
fn new_form(req: &PlanNewReq) -> wire::PlanNewForm {
    let set = crate::control::machine_prefs::pb_dir_now();
    let entry = locate::pick(set.as_deref(), &crate::agents::plugins(&config_roots()));
    let found = entry.as_ref().ok().cloned();
    let slices = |ws: &str| {
        let doc = book::book().last(ws).or_else(|| {
            found
                .as_deref()
                .and_then(|e| read_dir(e, Path::new(ws)).ok())
        });
        doc.map(|d| d.slices.into_iter().map(|s| (s.name, s.domain)).collect())
            .unwrap_or_default()
    };
    crate::plan::new_slice::form(
        &crate::plan::new_slice::Ask {
            dir: &req.dir,
            path: &req.path,
            domain: req.domain.as_deref(),
        },
        entry,
        &slices,
    )
}

/// `plan-command {workspace, cmd, slice?, path?, domain?}`：以人的身份代敲 pb 的一条命令（[`crate::plan::command`]）。
/// 写盘的是 pb；改了工作区配置 ⇒ 盯盘那一路照常推 `changed {plan}`（`auto` · 只做计划 · 当前仓库都在成品里）。
/// `adopt` 先现读一次（起 pb），按那个仓库此刻是不是只做计划 · 是不是当前仓库定敲哪几条。
/// `repo` 先照 `plan-new` 同一份判（`workspace` 是新计划框里选的那个目录）：在哪敲、撞上已有的一片 ⇒ 只带名字，敲的就是框里写出的那一行。
fn command(req: &PlanCommandReq, tz: &crate::Tz) -> Result<wire::PlanCmdReply, Fail> {
    let cmd = Some(req.cmd.as_str())
        .filter(|c| crate::plan::command::VERBS.contains(c))
        .ok_or_else(|| bad("`cmd` must be one of continue · pause · adopt · render · repo"))?;
    if (cmd == "adopt") != req.slice.is_some() {
        return Err(bad("`slice` goes with `adopt` and only with it"));
    }
    if (cmd == "repo") != req.path.is_some() || (cmd != "repo" && req.domain.is_some()) {
        return Err(bad("`path` · `domain` go with `repo` and only with it"));
    }
    let entry = entry()?;
    if cmd == "repo" {
        let path = req.path.as_deref().unwrap_or_default();
        let f = new_form(&PlanNewReq {
            dir: req.workspace.clone(),
            path: path.to_string(),
            domain: req.domain.clone(),
        });
        if let Some(said) =
            f.pb.said
                .clone()
                .filter(|_| f.pb.state != "ok")
                .or(f.failed)
        {
            return Err(Fail::new("failed", said));
        }
        let args = crate::plan::new_slice::repo_args(
            path,
            req.domain.as_deref(),
            f.existing.as_ref().map(|e| e.slice.as_str()),
        );
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        return crate::plan::command::run(&entry, Path::new(&f.root), &argv)
            .map_err(|(code, said, raw)| Fail::new(code, said).with_raw(raw.as_deref()));
    }
    let ws = Path::new(&req.workspace);
    let run = |args: &[&str]| crate::plan::command::run(&entry, ws, args);
    let out = match req.slice.as_deref() {
        Some(name) => {
            let doc = read_fresh(&req.workspace, tz)?;
            let sl = doc.slices.iter().find(|s| s.name == name).ok_or_else(|| {
                Fail::new(
                    "no_such_slice",
                    copy_text("bePlan.command.noSlice", &[("slice", name)]),
                )
            })?;
            let t = crate::plan::command::Adopting {
                slice: name,
                path: sl.path.as_deref().unwrap_or_default(),
                docs_only: sl.docs_only,
                current: sl.current,
            };
            crate::plan::command::adopt_with(&t, &run)
        }
        None => run(&[cmd]),
    };
    out.map_err(|(code, said, raw)| Fail::new(code, said).with_raw(raw.as_deref()))
}

/// `plan-files {dir}`：文件窗口反查。只看读好过的工作区（主窗口与需手动的账起来就问过 `plan-list`）——
/// 不为文件窗口走到的每个目录起一次 pb；目录在最深的那个工作区里才算。不在任何一片的仓库里 ⇒ `slice: null`（文件窗口什么都不多）。
fn files(dir: &str, tz: &crate::Tz) -> wire::PlanFiles {
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
    match doc {
        Some(d) => crate::plan::owners::of_dir(&d, dir),
        None => crate::plan::owners::nowhere(None),
    }
}

fn list(req: &PlanListReq, tz: &crate::Tz) -> wire::PlanList {
    let (pb, docs) = reads(req, tz);
    list_of(pb, &docs)
}

/// `plan-list` 那一路读到的几份成品（标过认可的，照工作区排）· pb 的结局：[`list`] 与 [`need_slices`] 同一处取。
fn reads(req: &PlanListReq, tz: &crate::Tz) -> (wire::PlanPb, Vec<wire::PlanRead>) {
    let mut dirs: Vec<String> =
        crate::history::live_table::now(&crate::history::listing::agent_home()).cwds();
    dirs.extend(req.dirs.iter().flatten().cloned());
    let entry = match entry() {
        Ok(e) => e,
        Err(f) => return (pb_state("missing", Some(f.message)), Vec::new()),
    };
    reads_in(
        book::book(),
        dirs,
        req.fresh.unwrap_or(false),
        &|dir| read_dir(&entry, dir),
        tz,
    )
}

fn list_of(pb: wire::PlanPb, docs: &[wire::PlanRead]) -> wire::PlanList {
    wire::PlanList {
        pb,
        workspaces: docs.iter().map(crate::plan::product::list_row).collect(),
    }
}

/// [`list`] 的本体（判据换成替身数次数）：见 [`reads_in`]。
pub(crate) fn list_in(
    b: &book::Book,
    dirs: Vec<String>,
    fresh: bool,
    read: &dyn Fn(&Path) -> Result<wire::PlanRead, book::Miss>,
    tz: &crate::Tz,
) -> wire::PlanList {
    let (pb, docs) = reads_in(b, dirs, fresh, read, tz);
    list_of(pb, &docs)
}

/// [`reads`] 的本体：本子 · 要问的目录 · 认过的也重问不重问 · 起 pb 读一个目录的那一下。
/// 不重问时：认过的目录照本子里那一份；认不出的（不是工作区 · 读不成）也照本子里记的那一个结局，不再起 pb。
fn reads_in(
    b: &book::Book,
    mut dirs: Vec<String>,
    fresh: bool,
    read: &dyn Fn(&Path) -> Result<wire::PlanRead, book::Miss>,
    tz: &crate::Tz,
) -> (wire::PlanPb, Vec<wire::PlanRead>) {
    dirs.sort();
    dirs.dedup();
    let mut seen: BTreeMap<String, wire::PlanRead> = BTreeMap::new();
    let mut pb = pb_state("ok", None);
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
                seen.entry(v.workspace.clone()).or_insert(v);
            }
            Err(book::Miss::NotWorkspace(_)) => {}
            Err(book::Miss::Unsupported(said)) => pb = pb_state("unsupported", Some(said)),
            Err(book::Miss::Failed { .. }) => {}
        }
    }
    if let Some(v) = seen.values().next() {
        pb.version = v.pb.clone();
    }
    (pb, seen.into_values().collect())
}

/// pb 装没装、认不认得它的输出（版本由 [`reads_in`] 照读到的那一份填）。
fn pb_state(state: &str, said: Option<String>) -> wire::PlanPb {
    wire::PlanPb {
        state: state.to_string(),
        said,
        version: None,
    }
}

/// 这台计划里要人看的每一条（工作区 · 片名 · 落在哪一格，照 `plan-list` 的先后；[`crate::plan::needs::need_items`] 摊开）：
/// 同 `plan-list {}` 那一路取（认过的目录照本子里那一份，不重问 pb）。读不出的工作区不在里头（计划那一截空着，不挡会话那几项）。
pub(crate) fn need_slices(tz: &crate::Tz) -> Vec<crate::observe::recognize::facts::NeedAt> {
    crate::plan::needs::need_items(&reads(&PlanListReq::default(), tz).1)
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
    /// `continue` · `pause` · `adopt` · `render` · `repo`（`repo` 的 `workspace` 是新计划框里选的目录）。
    #[cfg_attr(
        test,
        ts(type = "\"continue\" | \"pause\" | \"adopt\" | \"render\" | \"repo\"")
    )]
    pub(crate) cmd: String,
    /// `adopt` 接过哪一个仓库（仓库名）；别的动词不带。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) slice: Option<String>,
    /// `repo` 登记的仓库相对工作区根在哪；别的动词不带。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) path: Option<String>,
    /// `repo` 选的那一档领域（缺 ⇒ 不带）；别的动词不带。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) domain: Option<String>,
}

/// `plan-new` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanNewReq {
    /// 在哪个目录开（那台机器上的绝对路径；还不在也行）。
    pub(crate) dir: String,
    /// 仓库相对工作区根在哪（最后一段是片名）；空 ⇒ 只问领域。
    pub(crate) path: String,
    /// 选的那一档领域。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) domain: Option<String>,
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

typed::codes! {
    /// `plan-list` · `plan-files` · `plan-pb-check` · `plan-refs` · `plan-new` 的码：只有入参不对那一种（读不成的工作区 · pb 不在都写在回包里）。
    pub(crate) enum PlanListErr {}
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
    /// `plan-cell-view` 的码：起 pb 的那几种 ＋ pb 说那一片 · 那一格不在。
    pub(crate) enum PlanCellViewErr {
        Failed = "failed",
        NoPb = "no_pb",
        NoView = "no_view",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
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
        async move { Ok(list(&req, &cx.tz)) }
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
            read_fresh(&req.workspace, &cx.tz).map_err(|f| typed::of_fail(f, PlanPbErr::Failed))
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
            cell_view(&req)
                .map(|view| PlanCellView { view })
                .map_err(|f| typed::of_fail(f, PlanCellViewErr::Failed))
        }
    }
}

/// `plan-cell-view` 的本体：点到这一格才起 `pb dump --node`（整份 dump 里不带 agent 视角）。
/// pb 说那个仓库 · 那个节点不在 ⇒ `no_view`（pb 那一句）。
fn cell_view(req: &PlanCellViewReq) -> Result<String, Fail> {
    use crate::plan::dump::{self, NodeMiss, Ran};
    let entry = entry()?;
    dump::run_node(&entry, Path::new(&req.workspace), &req.slice, &req.id).map_err(|m| match m {
        NodeMiss::Gone(said) if said.trim().is_empty() => {
            Fail::new("no_view", copy_text("bePlan.face.noView", &[]))
        }
        NodeMiss::Gone(said) => Fail::new("no_view", said),
        NodeMiss::Ran(Ran::NotWorkspace(said)) => miss(book::Miss::NotWorkspace(said)),
        NodeMiss::Ran(Ran::Unsupported(said)) => miss(book::Miss::Unsupported(said)),
        NodeMiss::Ran(Ran::Failed { said, raw }) => miss(book::Miss::Failed { said, raw }),
        NodeMiss::Ran(Ran::Dump { .. }) => Fail::new("failed", copy_text("bePlan.face.shape", &[])),
    })
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

/// `plan-new`：开一片之前问的那一份（起一次 `pb dump --domains` ⇒ 阻塞档；不写盘）。pb 不在 · 读不成都写在回包里。
pub(crate) struct PlanNewCmd;

impl Command for PlanNewCmd {
    const NAME: &'static str = "plan-new";
    type Req = PlanNewReq;
    type Reply = wire::PlanNewForm;
    type Err = PlanListErr;
}

impl Handle for PlanNewCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanNewReq,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanNewForm, PlanListErr>> + Send + 'static
    {
        async move {
            if !Path::new(req.dir.trim()).is_absolute() {
                return Err(PlanListErr::BadArgs(crate::common::contract::malformed(
                    "`dir` must be an absolute path",
                )));
            }
            Ok(new_form(&req))
        }
    }
}

/// `plan-files`：文件窗口反查（只读本子里那一份，不起 pb；读认可那份小文件 ⇒ 阻塞档）。
pub(crate) struct PlanFilesCmd;

impl Command for PlanFilesCmd {
    typed::by_call!(cmd_wire::plan::PlanFilesReq);
    type Err = PlanListErr;
}

impl Handle for PlanFilesCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: cmd_wire::plan::PlanFilesReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanFiles, PlanListErr>> + Send + 'static
    {
        async move { Ok(files(&req.dir, &cx.tz)) }
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
            Ok(locate::check(dir))
        }
    }
}

/// `plan-refs {sid, texts}` 的本体（[`crate::plan::refs::answer`]）：照读好过的各工作区成品认片（不为认片起 pb）；
/// 会话的工作目录照这台活会话表（停了的会话只认它接手的那一块）；认出了才找 pb、起一次 `dump --refs`。
fn refs(req: &PlanRefsReq) -> wire::PlanMsgRefs {
    let b = book::book();
    let docs: Vec<wire::PlanRead> = b.workspaces().iter().filter_map(|w| b.last(w)).collect();
    let home = crate::history::listing::agent_home();
    let cwd = crate::history::live_table::now(&home)
        .rows()
        .iter()
        .find(|r| r.sid() == Some(req.sid.as_str()))
        .and_then(|r| r.fact.cwd.clone())
        .filter(|c| !c.is_empty());
    crate::plan::refs::answer(
        &docs,
        &req.sid,
        cwd.as_deref(),
        &req.texts,
        crate::plan::refs::memo(),
        &|at, one| {
            let entry = entry().ok()?;
            crate::plan::dump::run_refs(&entry, Path::new(&at.workspace), &at.slice, one)
        },
    )
}

/// `plan-refs` 的入参：哪个会话的几段正文（照先后，回包一段对一段）。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanRefsReq {
    pub(crate) sid: String,
    pub(crate) texts: Vec<String>,
}

/// `plan-refs`：会话消息里提到的格（可能起一次 pb ⇒ 阻塞档；pb 不在 · 读不成都回空，不报错）。
pub(crate) struct PlanRefsCmd;

impl Command for PlanRefsCmd {
    const NAME: &'static str = "plan-refs";
    type Req = PlanRefsReq;
    type Reply = wire::PlanMsgRefs;
    type Err = PlanListErr;
}

impl Handle for PlanRefsCmd {
    type Needs = typed::needs::Nothing;
    fn run(
        req: PlanRefsReq,
        _cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanMsgRefs, PlanListErr>> + Send + 'static
    {
        async move { Ok(refs(&req)) }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_face_tests.rs"]
mod tests;
