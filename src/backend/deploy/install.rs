//! **手机那一趟装**：①–④ 在一条池连接上做完（计划 · 取字节 ＋ 自检 · 放进暂存区 ＋ 读回 · 那一份自己换进落点 · 收拾残件）。⑤ 起常驻 / 换旧的由宿主跑。
//!
//! 字节是宿主交来的一份文件（手机：App 从包里解到私有目录的那一格；每版一次）：宿主经 [`Load`] 从文件读，不经 JNI 的数组口。
//! 桌面不走这里：字节在 monitor 手里、放字节经后端那条 `files` 链路（同一个 `put_verified`），判定同一份 [`crate::plan`]。
//!
//! 中途断了 / 失败：暂存件 · 它的临时件都是唯一名，断了就留成残件，下次连上照计划（[`crate::stale_leftovers`]）删；
//! SFTP 只写暂存区，落点只由那一份自己 `rename(2)` 盖掉（原子）⇒ 换进去之前断，落点上旧的那一份原样不动；读回不对当场删掉。

use std::path::PathBuf;

use copy_core::copy_text;
use deploy_contract::{DeployAction, Key, RemoteIdentity};

use serde_json::Value;

use crate::{Facing, Fail, Fut, Plan};

/// 宿主带着的一格：哪种机器 · 那一份自报的身份（构建时扫出来的）· 字节在哪个文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub key: Key,
    pub id: String,
    pub path: PathBuf,
}

/// 读一格的字节（宿主给：本 crate 不碰本机文件系统，读文件是宿主的事）。
pub type Load = dyn Fn(&std::path::Path) -> std::io::Result<Vec<u8>> + Send + Sync;

/// 走到哪一步了（宿主照它显示进度；那一句 [`Step::said`] 两端照画）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// 问那台：要哪一格 · 落点那一份是谁。
    Plan,
    /// 放字节（`updating` = 落点上原有一份旧的）。
    Place { updating: bool },
    /// ⑤ 换常驻（宿主跑 `--resident-ensure --replace`）。
    Restart,
}

impl Step {
    /// 这一步对人说的那一句（`machine` = 那台的名字）。
    pub fn said(self, machine: &str) -> String {
        let connecting = |stage: String| {
            copy_text(
                "machineState.busy.connecting",
                &[("machine", machine), ("stage", &stage)],
            )
        };
        match self {
            Step::Plan => connecting(copy_text("machineState.stage.deploy", &[])),
            Step::Place { updating: true } => {
                copy_text("machineState.busy.updating", &[("machine", machine)])
            }
            Step::Place { updating: false } => {
                copy_text("machineState.busy.installing", &[("machine", machine)])
            }
            Step::Restart => connecting(copy_text("machineState.stage.attach", &[])),
        }
    }
}

impl Plan {
    /// 这一趟做了什么那一句：已是这一版 · 装上了这一版（带原因）· 那台不比这一版旧、没动它（计划里那一句）。
    pub fn said(&self) -> String {
        let path = format!("~/{}", relay_route_core::BACKEND_LANDING_REL);
        let machine = self.key.label();
        match &self.action {
            DeployAction::Skip => copy_text(
                "rsSftp.deploy.upToDate",
                &[
                    ("buildId", &self.expected),
                    ("machine", &machine),
                    ("path", &path),
                ],
            ),
            DeployAction::Deploy(reason) => copy_text(
                "rsSftp.deploy.done",
                &[
                    ("buildId", &self.expected),
                    ("machine", &machine),
                    ("path", &path),
                    ("reason", reason),
                ],
            ),
            DeployAction::Keep { why, .. } => why.clone(),
        }
    }
}

/// 放得了字节的对面：计划那几问之外，还给得出这条连接上的 SFTP 会话。
pub trait Place: Facing {
    fn sftp(&self) -> Fut<'_, Result<&ssh_dial::sftp::Session, Fail>>;
}

impl Place for crate::DialFacing {
    fn sftp(&self) -> Fut<'_, Result<&ssh_dial::sftp::Session, Fail>> {
        Box::pin(async move {
            self.session()
                .await
                .map_err(|s| Fail::new("unreachable", s.said.clone()).with_raw(s.raw.as_deref()))
        })
    }
}

/// 写那一步没成 ⇒ 码 ＋ 那一句 ＋ 原话。
fn refusal(r: ssh_dial::sftp::Refusal) -> Fail {
    let code = match r {
        ssh_dial::sftp::Refusal::Fenced { .. } => "fenced",
        ssh_dial::sftp::Refusal::Io { .. } => "io_failed",
    };
    Fail::new(code, r.message().to_string()).with_raw(r.raw())
}

/// 暂存那一份的名字打头那一段：后端认自己的名字（被叫成别的名字 ⇒ 当一段配置，`--place-self` 进不了后端）。
pub const STAGED_STEM: &str = "cc-monitor-backend";

/// 在那台上跑暂存那一份的 `--place-self`：回一行 `{"placed":true,…}` 才算换进了落点；不然 ⇒ 那台说的那一句（失败信封的 `message`）。
async fn place_self(host: &dyn Place, staged: &str, machine: &str) -> Result<(), Fail> {
    let failed = |why: &str| {
        Fail::new(
            "place_failed",
            copy_text("deploy.place.failed", &[("machine", machine), ("why", why)]),
        )
    };
    let (got, _) = host
        .exec(format!(
            "{} -- --place-self",
            relay_route_core::home_shell(staged)
        ))
        .await
        .map_err(|e| failed(&e))?;
    let line = |t: &str| {
        t.lines()
            .next()
            .and_then(|l| serde_json::from_str::<Value>(l).ok())
    };
    if got.exit_status == Some(0)
        && line(&got.stdout).is_some_and(|v| v["placed"] == Value::Bool(true))
    {
        return Ok(());
    }
    let said = line(&got.stderr)
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| got.stderr.trim().to_string());
    Err(failed(&said).with_raw(Some(&format!("{}\n{}", got.stdout, got.stderr))))
}

/// 装一趟：回那份计划（`action` 说这一趟做了什么：`Skip` 已是这一版 · `Deploy` 放上了这一版 · `Keep` 那台不比这一版旧、一个字节没写）。
/// `machine` = 那台的名字（只用来说话）；`now_secs` = 此刻（判残件新旧）。
pub async fn install(
    host: &dyn Place,
    slots: &[Slot],
    load: &Load,
    machine: &str,
    now_secs: u64,
    on: &(dyn Fn(Step) + Send + Sync),
) -> Result<Plan, Fail> {
    on(Step::Plan);
    let carried: Vec<(Key, String)> = slots.iter().map(|s| (s.key, s.id.clone())).collect();
    let (plan, updating) = crate::plan_and_landing(host, &carried, machine, now_secs).await?;
    if let DeployAction::Deploy(_) = &plan.action {
        // ② 照计划取那一格的字节；计划说的身份与字节自报的对不上 ⇒ 不推。
        let not_this = || {
            Fail::new(
                "not_carried",
                copy_text(
                    "rsSftp.plan.notThisSlot",
                    &[("machine", &plan.key.label()), ("expected", &plan.expected)],
                ),
            )
        };
        let slot = slots
            .iter()
            .find(|s| s.key == plan.key && s.id == plan.expected)
            .ok_or_else(not_this)?;
        let bytes = load(&slot.path).map_err(|e| {
            Fail::new(
                "not_carried",
                copy_text(
                    "deploy.bytes.unreadable",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
            )
            .with_raw(Some(&format!("{}: {e}", slot.path.display())))
        })?;
        if deploy_contract::identity_of_bytes(&bytes, crate::MARKS)
            != RemoteIdentity::Stamp(plan.expected.clone())
        {
            return Err(Fail::new(
                "not_carried",
                copy_text("rsSftp.deploy.noBuildId", &[]),
            ));
        }
        // ③ 放进暂存区 ＋ 读回（读回不对当场删）→ 那一份自己核字节、`rename(2)` 换进落点。落点上原来有一份 ⇒ 这一趟是更新。
        on(Step::Place { updating });
        let s = host.sftp().await?;
        ssh_dial::sftp::make_dirs(s, ssh_dial::sftp::STAGING_ROOT)
            .await
            .map_err(refusal)?;
        let name = ssh_dial::sftp::staged_name(STAGED_STEM);
        let staged = format!("{}/{name}", ssh_dial::sftp::STAGING_ROOT);
        ssh_dial::sftp::put_verified(s, &staged, &bytes, 0o700)
            .await
            .map_err(refusal)?;
        place_self(host, &staged, machine).await?;
    }
    // ④ 照计划删上一趟没收拾掉的残件（删不掉只进日志、不挡这一趟，下次连上再来）。
    if !plan.leftovers.is_empty() {
        match host.sftp().await {
            Ok(s) => {
                for rel in &plan.leftovers {
                    match ssh_dial::sftp::remove(s, rel).await {
                        Ok(_) => tracing::info!("[{machine}] 删了上一趟留下的上传残件 ~/{rel}"),
                        Err(e) => tracing::warn!("[{machine}] 上传残件 ~/{rel} 没删掉：{e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("[{machine}] 残件这一趟没删（{}）", e.said),
        }
    }
    Ok(plan)
}
