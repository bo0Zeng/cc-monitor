//! 〔AS2 · 第四波 4B · V113〕**skill「装到这台」** —— monitor 这一侧：**只编排 I/O，零判定**。
//!
//! # 用户裁决（逐字）
//!
//! V113「目录自动同步，**装要你点**」· 「这样远端后端也能在远端装skill或者mcp」；V112「内容，原样拷过去并标出可疑项」。
//!
//! # 形状（与 AS1 `mcp_sync.rs` 同一套：读 · 请被写那台判 · CAS 写，stale 不重来）
//!
//! ```text
//! 看差异 skill_install_preview(from, to, name)
//!   ① 来源那台的后端 skill-read {name}                    （每个文件的原文 ＋ 执行位）
//!   ② 要被写那台的后端 skill-install-plan {name, source}   （差异四态 ＋ 可疑项 ＋ 那台现有原文，事实是那台的）
//! 写    skill_install_apply(to, name, source, target, take, overwrite)
//!   ③ 要被写那台的后端 skill-install-plan {…, take, overwrite} （真要写哪几个；不同的没说盖 ⇒ 整趟拒）
//!   ④ 逐个 files-put（root = base，rel = <prefix>/<path>，expect = 看差异时那一份，parents）＋ 有执行位的 files-chmod 0755
//! ```
//!
//! - **判定一处，住后端**（`设计/01 §1.1`）：四态、可疑项、「写哪几个」都是 `skill-install-plan` 答的（它原样复用 AS1 的差异与闸）。
//! - **写一处**：经那台后端 `files-put`（`user_files::Door`），monitor 一个字节不落盘。
//! - 🔴 **不重读重算**（同 AS1）：用户确认的是他看到的那份差异 ⇒ `stale` 就停，说清前面已经写了哪几个。
//! - ⚠ **多个文件不是一次原子写**：写到一半停下 ⇒ 前面那几个已经在了，如实列出来（不回滚：回滚本身也是一次写）。
//!
//! # 买不到
//!
//! - 🔴 真远端 / 真 Windows：判据用替身门 ＋ 替身后端；Windows 那一台上 `files-chmod` 恒失败 ⇒ 执行位落进 `chmodFailed`（不算整趟失败）。

use crate::copy_table::copy_text;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client::{client_for, encode_request};
use crate::origin::Origin;
use crate::user_files::{BackendDoor, Door, Refused, REQUEST_LINE_CAP};

/// 后端那两条命令（与 `src/backend/inbound.rs::REGISTRY` 同名，判据现抠对拍）。
pub(crate) const READ: &str = "skill-read";
pub(crate) const PLAN: &str = "skill-install-plan";

/// 一趟读 / 判的上限（走目录 ＋ 读几百 KB 文本 ＋ 逐条 stat，秒级；给足余量同时防卡死）。
const BUDGET: Duration = Duration::from_secs(60);

/// 来源那台读到的一个文件。`text = null` ⇒ 这一个装不过去（`why` 说为什么）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SkillFile {
    pub path: String,
    pub text: Option<String>,
    pub exec: bool,
    pub why: Option<String>,
}

/// 要被写那台上现有的一份原文（写的时候当 CAS 期望）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SkillTargetText {
    pub path: String,
    pub text: String,
}

/// 一条可疑项（后端判的，原样转给界面）。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SkillInstallSuspect {
    /// `executable` · `binary` · `abs-path` · `command-missing`（闭集住后端 `skill_install::SUSPECT_KINDS`）。
    pub kind: String,
    pub value: String,
    /// `present` · `absent` · `foreign` · `unknown`；`executable` / `binary` 是 `null`。
    pub there: Option<String>,
}

/// 差异表的一行。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SkillInstallRow {
    pub path: String,
    /// `new` · `same` · `differs` · `only-there`（闭集住后端 AS1 `mcp_sync::STATES`）。
    pub state: String,
    pub suspects: Vec<SkillInstallSuspect>,
    /// 这台上那一份盖不了的原因（不是文本等）；盖得了 ⇒ `null`。
    pub blocked: Option<String>,
}

/// 看差异的结果。`source` / `target` 原样带回界面，写的时候原样送回来。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct SkillInstallPreview {
    /// 要写进去的那个目录（给人看）。
    pub dir: String,
    pub rows: Vec<SkillInstallRow>,
    pub source: Vec<SkillFile>,
    pub target: Vec<SkillTargetText>,
}

/// 写的结果。
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallApplied {
    pub dir: String,
    /// 真写了的那几个（相对路径，按写的顺序）。
    pub written: Vec<String>,
    /// 执行位没设上的那几个（Windows 那一台上恒如此；不算整趟失败）。
    pub chmod_failed: Vec<String>,
}

/// 问一台后端一条命令（生产 = [`BackendAsk`]；判据用替身）。
pub(crate) trait Ask {
    async fn ask(&self, origin: &Origin, cmd: &str, args: Value) -> Result<Value, String>;
}

pub(crate) struct BackendAsk;

impl Ask for BackendAsk {
    async fn ask(&self, origin: &Origin, cmd: &str, args: Value) -> Result<Value, String> {
        let wire = origin.as_wire_str();
        let who = crate::backend::control::cc_bus::machine_label(wire);
        let Some(client) = client_for(wire) else {
            let why = match no_channel(wire) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => String::new(),
            };
            return Err(copy_text(
                "rsSkillInstall.ask.backendDown",
                &[("who", &who.to_string()), ("why", &why.to_string())],
            ));
        };
        if !client.accepts(cmd) {
            return Err(
                crate::backend::control::cc_bus::describe_backend_too_old_for(
                    wire,
                    cmd,
                    &copy_text("rsSkillInstall.ask.failed", &[]),
                ),
            );
        }
        let line = encode_request("0", cmd, &args);
        if line.len() > REQUEST_LINE_CAP {
            return Err(copy_text(
                "rsSkillInstall.ask.tooBig",
                &[
                    ("bytes", &(line.len()).to_string()),
                    ("cap", &REQUEST_LINE_CAP.to_string()),
                ],
            ));
        }
        let data = client.call(cmd, args, BUDGET).await.map_err(|e| {
            match route_call_error(&e, |_code, message| format!("{who}：{message}")) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
                Routed::Done => {
                    copy_text("rsSkillInstall.ask.internal", &[("who", &who.to_string())])
                }
            }
        })?;
        data.ok_or_else(|| {
            copy_text(
                "rsSkillInstall.ask.emptyReply",
                &[("who", &who.to_string())],
            )
        })
    }
}

/// 后端应答认不出来时的那句话（多半是两边版本不一样）。**不猜默认值**。
static UNREADABLE_REPLY: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsSkillInstall.reply.unreadable", &[]));

fn broken() -> String {
    UNREADABLE_REPLY.to_string()
}

fn str_of(v: &Value, k: &str) -> Result<String, String> {
    v.get(k)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(broken)
}

fn opt_str_of(v: &Value, k: &str) -> Result<Option<String>, String> {
    match v.get(k) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(broken()),
    }
}

fn arr_of<'a>(v: &'a Value, k: &str) -> Result<&'a Vec<Value>, String> {
    v.get(k).and_then(Value::as_array).ok_or_else(broken)
}

fn row_from_wire(r: &Value) -> Result<SkillInstallRow, String> {
    Ok(SkillInstallRow {
        path: str_of(r, "path")?,
        state: str_of(r, "state")?,
        suspects: arr_of(r, "suspects")?
            .iter()
            .map(|s| {
                Ok(SkillInstallSuspect {
                    kind: str_of(s, "kind")?,
                    value: str_of(s, "value")?,
                    there: opt_str_of(s, "there")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        blocked: opt_str_of(r, "blocked")?,
    })
}

fn source_wire(source: &[SkillFile]) -> Value {
    json!(source
        .iter()
        .map(|f| json!({ "path": f.path, "text": f.text, "exec": f.exec }))
        .collect::<Vec<_>>())
}

/// 看差异（可测的那一半：问后端那一口是参数）。
pub(crate) async fn preview_with(
    ask: &impl Ask,
    from: &Origin,
    to: &Origin,
    name: &str,
) -> Result<SkillInstallPreview, String> {
    let read = ask.ask(from, READ, json!({ "name": name })).await?;
    let source: Vec<SkillFile> = arr_of(&read, "files")?
        .iter()
        .map(|f| {
            Ok(SkillFile {
                path: str_of(f, "path")?,
                text: opt_str_of(f, "text")?,
                exec: f.get("exec").and_then(Value::as_bool).ok_or_else(broken)?,
                why: opt_str_of(f, "why")?,
            })
        })
        .collect::<Result<_, String>>()?;
    let plan = ask
        .ask(
            to,
            PLAN,
            json!({ "name": name, "source": source_wire(&source) }),
        )
        .await?;
    let rows = arr_of(&plan, "rows")?
        .iter()
        .map(row_from_wire)
        .collect::<Result<Vec<_>, String>>()?;
    let target = arr_of(&plan, "target")?
        .iter()
        .map(|t| {
            Ok(SkillTargetText {
                path: str_of(t, "path")?,
                text: str_of(t, "text")?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SkillInstallPreview {
        dir: str_of(&plan, "dir")?,
        rows,
        source,
        target,
    })
}

/// 写到一半停下时的那句话：说清停在哪、前面写了哪几个。
fn stopped_said(machine: &str, at: &str, why: &str, written: &[String]) -> String {
    let done = if written.is_empty() {
        copy_text("rsSkillInstall.stopped.none", &[])
    } else {
        copy_text(
            "rsSkillInstall.stopped.some",
            &[(
                "list",
                &(written.join(&copy_text("rsSkillInstall.stopped.listSep", &[]))).to_string(),
            )],
        )
    };
    copy_text(
        "rsSkillInstall.stopped.at",
        &[
            ("machine", &machine.to_string()),
            ("at", &at.to_string()),
            ("why", &why.to_string()),
            ("done", &done.to_string()),
        ],
    )
}

/// 写（可测的那一半）。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn apply_with(
    ask: &impl Ask,
    door: &impl Door,
    to: &Origin,
    name: &str,
    source: &[SkillFile],
    target: &[SkillTargetText],
    take: &[String],
    overwrite: &[String],
) -> Result<SkillInstallApplied, String> {
    let plan = ask
        .ask(
            to,
            PLAN,
            json!({
                "name": name,
                "source": source_wire(source),
                "take": take,
                "overwrite": overwrite,
            }),
        )
        .await?;
    let write: Vec<String> = arr_of(&plan, "write")?
        .iter()
        .map(|n| n.as_str().map(str::to_string))
        .collect::<Option<_>>()
        .ok_or_else(broken)?;
    let (dir, base, prefix) = (
        str_of(&plan, "dir")?,
        str_of(&plan, "base")?,
        str_of(&plan, "prefix")?,
    );
    let mut written = Vec::new();
    let mut chmod_failed = Vec::new();
    for path in &write {
        let file = source.iter().find(|f| &f.path == path).ok_or_else(|| {
            copy_text(
                "rsSkillInstall.apply.notInCopy",
                &[("path", &path.to_string())],
            )
        })?;
        let text = file.text.as_deref().ok_or_else(|| {
            copy_text(
                "rsSkillInstall.apply.noOriginal",
                &[("path", &path.to_string())],
            )
        })?;
        let expect = target
            .iter()
            .find(|t| &t.path == path)
            .map(|t| t.text.as_str());
        let rel = format!("{prefix}/{path}");
        match door.put(&base, &rel, text, expect, false, true).await {
            Ok(_) => written.push(path.clone()),
            Err(Refused::Stale(_)) => {
                return Err(stopped_said(
                    &door.machine(),
                    path,
                    &copy_text("rsSkillInstall.apply.stale", &[]),
                    &written,
                ))
            }
            Err(e) => return Err(stopped_said(&door.machine(), path, &e.said(), &written)),
        }
        if file.exec && door.chmod(&base, &rel, 0o755).await.is_err() {
            chmod_failed.push(path.clone());
        }
    }
    Ok(SkillInstallApplied {
        dir,
        written,
        chmod_failed,
    })
}

/// 看差异：`from` 那台的 skill「name」装到 `to` 那台会发生什么（判定由 `to` 那台的后端做）。
#[tauri::command]
pub async fn skill_install_preview(
    from: Origin,
    to: Origin,
    name: String,
) -> Result<SkillInstallPreview, String> {
    if from == to {
        return Err(copy_text("rsSkillInstall.preview.sameMachine", &[]));
    }
    preview_with(&BackendAsk, &from, &to, &name).await
}

/// 写：把勾的那几个原样写进 `to` 那台（`differs` 的必须也在 `overwrite` 里）。
/// `source` / `target` 是看差异时拿到的那两份，原样送回来（后者当 CAS 期望）。
#[tauri::command]
pub async fn skill_install_apply(
    to: Origin,
    name: String,
    source: Vec<SkillFile>,
    target: Vec<SkillTargetText>,
    take: Vec<String>,
    overwrite: Vec<String>,
) -> Result<SkillInstallApplied, String> {
    apply_with(
        &BackendAsk,
        &BackendDoor::new(to.clone()),
        &to,
        &name,
        &source,
        &target,
        &take,
        &overwrite,
    )
    .await
}

#[cfg(test)]
#[path = "../../../tests/bridge/skill_install_tests.rs"]
mod tests;
