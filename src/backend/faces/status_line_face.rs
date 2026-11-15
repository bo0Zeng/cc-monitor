//! 托管终端状态行那几格的值与偏好页的预览。模板 · 字段表 · 写成那一句住 `host::managed::status`。
//!
//! 值凭这台的盘现取（托管口是本机的 ⇒ 瘦客户端 · 托管终端 · 会话同在一台）：名 · 路径 · 号出自托管终端的事实；
//! 模型 · 上下文出自那条会话的会话事实（同 `history-facts` 那一份成品）；5h · 7d 出自额度账成品（同 `quota-read` 那一格的字）；
//! 排队出自会话输入队那一句；分支照工作目录往上找 `.git` 的 `HEAD`；机器名照这台的主机名。`holder` · `time` 由瘦客户端照它那一刻填。
//!
//! - [`ears`]：盯托管终端的家 · 队目录 · 偏好那份 · 额度账 · 那条会话的记录，变了重算、值或模板变了才说（入口装进 `attach::install_status`）。
//! - `status-line-preview`：照这台最近起的那个活着的托管终端现算；一个都没有 ⇒ 照字段表的示例值。

use crate::host::managed::{hold, home, status};
use crate::platform::fs::{Own, Ro};
use copy_core::copy_text;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 读 `HEAD` · 主机名那几份小文件的上限。
const SMALL: cap_core::Cap = cap_core::Cap {
    bytes: 4096,
    over: cap_core::Over::SkipAndSay,
};

/// 往上找 `.git` 至多几层。
const GIT_UP_MAX: usize = 64;

/// `cwd` 所在的 git 仓此刻的分支（游离 ⇒ 短提交号；不在仓里 ⇒ 空）。工作树那一形（`.git` 是一份写着 `gitdir:` 的文件）也认。
pub(crate) fn branch_of(cwd: &Path) -> String {
    let mut dir = Some(cwd);
    for _ in 0..GIT_UP_MAX {
        let Some(d) = dir else { break };
        let git = d.join(".git");
        let head = if git.is_dir() {
            Some(git.join("HEAD"))
        } else if git.is_file() {
            Ro::read_string_capped(&git, SMALL).ok().and_then(|t| {
                let g = t.trim().strip_prefix("gitdir:")?.trim().to_string();
                let g = PathBuf::from(g);
                Some(if g.is_absolute() { g } else { d.join(g) }.join("HEAD"))
            })
        } else {
            None
        };
        if let Some(h) = head {
            let Ok(t) = Ro::read_string_capped(&h, SMALL) else {
                return String::new();
            };
            let t = t.trim();
            return match t.strip_prefix("ref:") {
                Some(r) => r.trim().trim_start_matches("refs/heads/").to_string(),
                None => t.chars().take(7).collect(),
            };
        }
        dir = d.parent();
    }
    String::new()
}

/// 这台的主机名（问不出 ⇒ 空）。
pub(crate) fn host_name() -> String {
    #[cfg(unix)]
    {
        if let Ok(t) = Ro::read_string_capped("/proc/sys/kernel/hostname", SMALL) {
            return t.trim().to_string();
        }
        if let Ok(t) = Ro::read_string_capped("/etc/hostname", SMALL) {
            return t.trim().to_string();
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default()
}

/// 额度那一格：`5h 41%`（位名 ＋ 额度账成品里那一位的字）。
fn quota_cell(slot: &str, text: &str) -> String {
    copy_text(
        "resumeMenu.account.quota",
        &[
            ("slot", &crate::accounts::quota::name_words::slot_text(slot)),
            ("value", text),
        ],
    )
}

/// 号 · 5h · 7d 那三格：号没点名 ⇒ 起会话那一判给的号（这一家的默认号）；额度照 `quota-read` 那一份成品按（家, 号）对。
fn account_cells(kind: &str, named: &str, tz: &crate::common::time::Tz) -> [(String, String); 3] {
    let account = if named.is_empty() {
        // 没点名 ⇒ 起会话那一判（`launch_account::pick` 跟随、没有上次的号 ⇒ 这一家的默认号），不另推一份。
        crate::agents::pick_route_among(crate::agents::REGISTRY, Some(kind))
            .ok()
            .and_then(|r| {
                let list = crate::faces::launch_account_face::list_of(r.id());
                match crate::control::launch_account::pick(
                    &cmd_wire::open::AccountAsk::Follow,
                    &list,
                    None,
                    &cmd_wire::AgentKind::new(kind),
                ) {
                    Ok(crate::control::launch_account::Settled::Account(a)) => Some(a.name),
                    _ => None,
                }
            })
            .unwrap_or_default()
    } else {
        named.to_string()
    };
    let mut out = [
        ("account".to_string(), String::new()),
        (cmd_wire::quota::SLOT_5H.to_string(), String::new()),
        (cmd_wire::quota::SLOT_7D.to_string(), String::new()),
    ];
    if account.is_empty() {
        return out;
    }
    out[0].1 = crate::accounts::quota::name_words::account_text_of(kind, &account);
    let q = crate::faces::rotation_face::quota_read_with(
        &crate::faces::rotation_face::Ctx::here(),
        crate::common::time::now_secs(),
        tz,
    );
    if let Some(a) = q
        .accounts
        .iter()
        .find(|a| a.head.agent.as_str() == kind && a.head.account == account)
    {
        for (i, slot) in [(1, cmd_wire::quota::SLOT_5H), (2, cmd_wire::quota::SLOT_7D)] {
            if let Some(s) = a.head.show.slots.iter().find(|s| s.slot == slot) {
                out[i].1 = quota_cell(slot, &s.text.0);
            }
        }
    }
    out
}

/// 模型 · 上下文那两格与那条会话的记录（找不到 ⇒ 空）。上下文那一格是会话事实里写好的那一格字（`35%` · 判不出上限只写用量）。
fn session_cells(sid: &str, tz: &crate::common::time::Tz) -> (String, String, Option<PathBuf>) {
    if sid.is_empty() {
        return (String::new(), String::new(), None);
    }
    let limits = crate::common::machine_prefs::read_now().prefs().limits();
    let req =
        crate::faces::history_face::HistoryFactsReq::Sid(crate::faces::history_face::FactsBySid {
            sid: sid.to_string(),
        });
    let Ok(r) =
        crate::faces::history_face::facts_at(&crate::agents::local_homes(), req, &limits, tz, None)
    else {
        return (String::new(), String::new(), None);
    };
    let usage = r.facts.usage.as_ref();
    let model = usage.and_then(|u| u.model.clone()).unwrap_or_default();
    let ctx = usage
        .and_then(|u| serde_json::to_value(u).ok())
        .and_then(|v| v.get("contextText")?.as_str().map(str::to_string))
        .unwrap_or_default();
    (model, ctx, r.path.map(PathBuf::from))
}

/// 一个托管终端此刻那几格（`holder` · `time` 不填）与那条会话的记录在哪。读不出那个终端的事实 ⇒ 空的一份。
pub(crate) fn values_of(own: &Own, id: &str) -> (status::Values, Option<PathBuf>) {
    let mut v = status::Values::new();
    let Some(f) = home::dir_of(own, id)
        .ok()
        .and_then(|d| home::read_facts(d.as_path()))
    else {
        return (v, None);
    };
    let tz = crate::common::time::Tz::here();
    let user_home = crate::platform::paths::home_dir();
    let home_s = user_home.as_ref().map(|h| h.display().to_string());
    v.insert("name".into(), f.name.clone());
    v.insert("cwd".into(), status::path_shown(&f.cwd, home_s.as_deref()));
    v.insert("branch".into(), branch_of(Path::new(&f.cwd)));
    v.insert("host".into(), host_name());
    v.insert(
        "queue".into(),
        crate::history::session_queue::line_of_host(own, id).unwrap_or_default(),
    );
    for (k, val) in account_cells(&f.agent, &f.account, &tz) {
        v.insert(k, val);
    }
    let (model, ctx, record) = session_cells(&f.sid, &tz);
    v.insert("model".into(), model);
    v.insert("ctx".into(), ctx);
    (v, record)
}

/// 此刻那一份（模板 ＋ 值）。
fn snap_of(own: &Own, id: &str) -> (status::Snap, Option<PathBuf>) {
    let (values, record) = values_of(own, id);
    (
        status::Snap {
            template: crate::host::managed::attach::template_at(own),
            values,
        },
        record,
    )
}

/// 耳朵那一份状态：上次说的 · 那条会话的记录在哪 · 盯记录的那一个（记录换了跟着换）。
struct Ears {
    own: Own,
    id: String,
    tell: Box<dyn Fn(status::Snap) + Send + Sync>,
    last: status::Snap,
    record: Option<PathBuf>,
    on_record: Option<crate::platform::watch_file::Watching>,
}

/// 盯那条会话的记录所在的目录（只认那一份）；那一下重算交 `again`（耳朵丢了 ⇒ 不做）。
fn watch_record(
    record: &Path,
    ears: std::sync::Weak<Mutex<Ears>>,
) -> Option<crate::platform::watch_file::Watching> {
    let dir = record.parent()?.to_path_buf();
    let file = record.to_path_buf();
    let watching = crate::platform::watch_file::watch(
        &[(dir, false)],
        move |p| p == file,
        "status-record",
        move |_| {
            if let Some(e) = ears.upgrade() {
                again(&e);
            }
        },
    );
    match watching {
        Ok(w) => Some(w),
        Err(e) => {
            tracing::warn!("状态行：会话记录没盯上，模型 · 上下文那两格不跟着变：{e}");
            None
        }
    }
}

/// 重算一次：值或模板变了才说；记录换了 ⇒ 换盯的那一个。
fn again(ears: &Arc<Mutex<Ears>>) {
    let mut g = ears.lock().unwrap_or_else(|e| e.into_inner());
    let (snap, record) = snap_of(&g.own, &g.id);
    if record != g.record {
        g.on_record = record
            .as_deref()
            .and_then(|r| watch_record(r, Arc::downgrade(ears)));
        g.record = record;
    }
    if snap != g.last {
        g.last = snap.clone();
        (g.tell)(snap);
    }
}

/// 这台的额度账那一份的落点。
fn ledger_path() -> Option<PathBuf> {
    crate::faces::rotation_face::Ctx::here()
        .hop
        .quota
        .path()
        .map(Path::to_path_buf)
}

/// 瘦客户端那一行的耳朵（`attach::StatusEars`）：起来那一下先说一次，之后盯着的那几份一变就重算、值或模板变了才说。
/// 盯的（只认这几份，同目录里别的文件动了不算）：托管终端的事实（打标）· 队目录里的队 · 偏好那份 · 额度账 ·
/// 那条会话的记录（打了标、找得到才盯，记录换了跟着换）。交回的那一份丢了 ⇒ 不再盯。
pub fn ears(
    own: &Own,
    id: &str,
    tell: Box<dyn Fn(status::Snap) + Send + Sync>,
) -> Result<crate::platform::watch_file::Watching, String> {
    let host = home::dir_of(own, id).map_err(|s| s.logged())?;
    let queue = crate::history::session_queue::made_dir(own).map_err(|s| s.logged())?;
    let (host, queue) = (host.as_path().to_path_buf(), queue.as_path().to_path_buf());
    let files: Vec<PathBuf> = [crate::host::managed::attach::prefs_at(own), ledger_path()]
        .into_iter()
        .flatten()
        .collect();
    let mut dirs: Vec<PathBuf> = vec![host.clone(), queue.clone()];
    dirs.extend(
        files
            .iter()
            .filter_map(|f| f.parent().map(Path::to_path_buf)),
    );
    dirs.retain(|d| d.is_dir());
    dirs.sort();
    dirs.dedup();
    let concerns = move |p: &Path| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        (p.parent() == Some(host.as_path()) && name == home::FACTS)
            || (p.parent() == Some(queue.as_path()) && name.ends_with(".json"))
            || files.iter().any(|f| f == p)
    };
    let (first, record) = snap_of(own, id);
    tell(first.clone());
    let ears = Arc::new(Mutex::new(Ears {
        own: own.clone(),
        id: id.to_string(),
        tell,
        last: first,
        record: record.clone(),
        on_record: None,
    }));
    let on_record = record
        .as_deref()
        .and_then(|r| watch_record(r, Arc::downgrade(&ears)));
    ears.lock().unwrap_or_else(|e| e.into_inner()).on_record = on_record;
    let dirs: Vec<(PathBuf, bool)> = dirs.into_iter().map(|d| (d, false)).collect();
    crate::platform::watch_file::watch(&dirs, concerns, "status-line", move |_| again(&ears))
}

// ── status-line-preview ─────────────────────────────────────────────────────────────

/// `status-line-preview` 的入参：要预览的那一条模板（还没存的也行；`null` ⇒ 随包的缺省，同偏好里没设那一格）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusLinePreviewReq {
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub(crate) template: Option<String>,
}

/// 字段表的一行（给偏好页列说明）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct StatusField {
    /// 模板里写的样子（`{name}`）。
    pub(crate) key: String,
    /// 说明那一句。
    pub(crate) says: String,
}

/// 预览那一行照多宽写（偏好页照摆，不另截）。
pub(crate) const PREVIEW_COLS: usize = 80;

/// `status-line-preview` 的回包：写好的那一行 · 认不出的那一句 · 字段表 · 缺省模板 · 照哪一个算的那一句。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub struct StatusLinePreview {
    /// 照 [`PREVIEW_COLS`] 列写好的那一行（左段靠左、右段靠右）；模板空 ⇒ 不留那一行，`null`。
    pub(crate) line: Option<String>,
    /// 有认不出的键 ⇒ 写好的那一句；没有 ⇒ `null`。
    pub(crate) unknown_said: Option<String>,
    pub(crate) fields: Vec<StatusField>,
    pub(crate) default_template: String,
    /// 照哪一个算的（照哪个托管终端 · 没有活着的 ⇒ 照示例值 · 模板空 ⇒ 不留那一行）。
    pub(crate) from: String,
}

/// 这台最近起的那个活着的托管终端（`alive` 照起来的先后排）。
pub(crate) fn newest_host(own: &Own) -> Option<home::Facts> {
    home::alive(own).pop()
}

/// 预览的本体：照 `newest`（最近起的那个活着的托管终端；没有 ⇒ 示例值）现算。`now` 是此刻（unix 秒）。
pub(crate) fn preview_of(
    template: &str,
    newest: Option<(&Own, &home::Facts)>,
    now: i64,
    tz: &crate::common::time::Tz,
) -> StatusLinePreview {
    let t = status::Template::parse(template);
    let (values, from) = match newest {
        Some((own, f)) => {
            let (mut v, _) = values_of(own, &f.id);
            let who = if f.attached {
                hold::HolderWho::TerminalWindow
            } else {
                hold::HolderWho::None
            };
            v.insert(status::HOLDER.into(), hold::said_of(who, false));
            v.insert(status::TIME.into(), crate::common::time::hm(tz.local(now)));
            (
                v,
                copy_text("beStatusLine.preview.from", &[("name", &f.name)]),
            )
        }
        None => (
            status::samples(),
            copy_text("beStatusLine.preview.sample", &[]),
        ),
    };
    let line = t.shows().then(|| t.render(&values, PREVIEW_COLS));
    let from = if t.shows() {
        from
    } else {
        copy_text("beStatusLine.preview.off", &[])
    };
    let unknown = t.unknown();
    let unknown_said = (!unknown.is_empty()).then(|| {
        let keys: Vec<String> = unknown.iter().map(|k| format!("{{{k}}}")).collect();
        copy_text(
            "beStatusLine.preview.unknown",
            &[("keys", &keys.join(&copy_text("kit.text.sep", &[])))],
        )
    });
    StatusLinePreview {
        line,
        unknown_said,
        fields: status::field_rows()
            .into_iter()
            .map(|(k, says)| StatusField {
                key: format!("{{{k}}}"),
                says,
            })
            .collect(),
        default_template: status::DEFAULT_TEMPLATE.to_string(),
        from,
    }
}

crate::wire::typed::codes! {
    /// `status-line-preview` 没有别的码：模板写成什么样都照它算（认不出的键照实列出）。
    pub enum StatusLinePreviewErr {}
}

/// `status-line-preview`：偏好页那一格的预览（读盘 ⇒ 阻塞档）。
pub struct StatusLinePreviewCmd;

impl crate::wire::typed::Command for StatusLinePreviewCmd {
    const NAME: &'static str = "status-line-preview";
    type Req = StatusLinePreviewReq;
    type Reply = StatusLinePreview;
    type Err = StatusLinePreviewErr;
}

impl crate::wire::typed::Handle for StatusLinePreviewCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: StatusLinePreviewReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<StatusLinePreview, StatusLinePreviewErr> {
        let now = i64::try_from(crate::common::time::now_secs()).unwrap_or(i64::MAX);
        let own = crate::host::managed::installed();
        let newest = own.and_then(|o| newest_host(o).map(|f| (o, f)));
        Ok(preview_of(
            status::template_of(req.template.as_deref()),
            newest.as_ref().map(|(o, f)| (*o, f)),
            now,
            &cx.tz,
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/status_line_face_tests.rs"]
mod tests;
