//! **旧版放在 `~/.local/bin/ccm` 的那一份，认出是我们放的就删。**
//!
//! # 要求住址
//!
//! （用户 09-18）逐字：「⚠ 这是一次迁移：远端要同拍做三件 —— ① 改成同落点同名字 ·
//! ② 清掉旧的 `~/.local/bin/ccm` shim · ③ 清掉同目录下与后端重复的字节。按，三件都要在足迹里有入口。」
//! ① SR1b 已落（入口落 `~/.cc-monitor/bin/ccm`）；本模块做 ② ③：
//! **部署 / 升级时顺手删、足迹里可见**（不为旧状态留兼容）。
//!
//! # 删的是哪几形（现打：git 史里我们往远端 `~/.local/bin/ccm` 推过的只有两形）
//!
//! | 形 | 第二行（记号） | 来自 |
//! |---|---|---|
//! | 三行 shim（09-11 起，历代都是这一行） | `deploy_contract` 里那个 `SHIM_MARK`（生成器删了，记号留成字面量） | `ccm_entry_shim`〔散文墓碑〕历代 |
//! | bash 启动器（09-11 之前，`shared/ccm`，后端 ccm 的第二份实现 ＝ ③ 那份「与后端重复的」） | `# ccm — cc-monitor 统一启动器…` | `K-R48` 删掉的那份文件 |
//!
//! **认不认得出 · 删不删由本机常驻后端判**（帧命令 `deploy-retired`，`src/backend/control/deploy_plan.rs::retired_verdict`，
//! 与上传残件同一家；「判定只在后端」）：第一行必须是 `#!`、第二行认得出其中一形 ⇒ 是我们放的 ⇒ 后端答 `remove`
//! 并带上读到的全文。本模块只照答办：经**那台机器的后端**删（`user_files::Door::delete`，带 CAS：盘上逐字节等于那份全文才删）。
//! 别的一律**不动**（用户自己的脚本、读不成文本的、记号不在第二行的 —— 后端答 `keep` 并说为什么）。
//! ⚠「不为旧状态留兼容」：记号只回答「这是不是我们放的」这一问，不据它做任何别的事。
//!
//! # 为什么不走部署那条 SFTP
//!
//! 远端写只许 `~/.cc-monitor/{staging,bin}` 两处（`src/backend/dial/sftp.rs` 围栏），`~/.local/bin` 在两根之外；
//! 删用户家目录里的一个文件是文件管理那一面的事（`user_files` 头注：monitor 够用户文件的唯一开口）。
//!
//! # 买不到的
//!
//! - 本机那一份（monitor 跑着的这台上的 `~/.local/bin/ccm`）本模块不碰：那句说的是远端。
//! 本机 `~/.cc-monitor/bin/ccm` 今天就是后端本身（逐字节副本那一形删了）。
//! - 真远端一次都没跑过（判据用内存替身 `Door`）。

use crate::copy_table::copy_text;
use crate::user_files::{Door, Refused};

/// 那一份在家目录下的相对路径。**唯一住址**在契约 crate `deploy-contract`（后端判它、足迹那一行、本模块删它，同一个常量）。
pub(crate) use deploy_contract::LEGACY_ENTRY_REL as LEGACY_REL;

/// 本机常驻后端那条命令的名字（与 `src/backend/stream/inbound.rs::REGISTRY` 同名）。
pub(crate) const RETIRED_CMD: &str = "deploy-retired";

/// 问一次的上限：一次 stat ＋ 至多一次读回（池里那条 SSH，SFTP 第一问时才开）。
const RETIRED_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

/// 后端答的去向（线上形状住 `IPC-PROTOCOL.md` 的 `deploy-retired` 那一节）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Absent,
    /// 认出是我们放的；`expect` = 后端读到的全文（删时交给 `files-delete` 当期望值）。
    Remove {
        expect: String,
    },
    Keep {
        why: String,
    },
}

/// `deploy-retired` 的应答 → [`Verdict`]（**严格收**：恰 `{verdict, expect, why}`，三形之外都是错 —— 两侧漂了要当场说出来）。
pub(crate) fn decode_verdict(v: &serde_json::Value) -> Result<Verdict, String> {
    let bad = || copy_text("rsCcmLegacy.verdict.unreadable", &[]);
    let obj = v.as_object().ok_or_else(bad)?;
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["expect", "verdict", "why"] {
        return Err(bad());
    }
    let text = |k: &str| obj.get(k).and_then(serde_json::Value::as_str);
    let null = |k: &str| obj.get(k).is_some_and(serde_json::Value::is_null);
    match (text("verdict"), text("expect"), text("why")) {
        (Some("absent"), None, None) if null("expect") && null("why") => Ok(Verdict::Absent),
        (Some("remove"), Some(e), None) if null("why") => Ok(Verdict::Remove {
            expect: e.to_string(),
        }),
        (Some("keep"), None, Some(w)) if null("expect") => Ok(Verdict::Keep { why: w.to_string() }),
        _ => Err(bad()),
    }
}

/// 问本机常驻后端「那台的旧入口怎么办」。入参只有事实：怎么够到那台（与 `files` 链路同一份拨号请求）。
async fn ask_verdict(cfg: &crate::ssh_source::RemoteConfig) -> Result<Verdict, String> {
    use crate::backend_route::{route_call_error, Routed};
    let dial = crate::dial_host::transfer_dial(cfg)?;
    let client = crate::dial_host::local_backend_accepting(RETIRED_CMD).await?;
    let data = client
        .call(
            RETIRED_CMD,
            serde_json::json!({ "dial": dial }),
            RETIRED_BUDGET,
        )
        .await
        .map_err(
            |e| match route_call_error(&e, |_code, message| message.to_string()) {
                Routed::NoChannel(s) | Routed::Refused(s) => s,
            },
        )?;
    decode_verdict(&data.unwrap_or_default())
}

/// 一次清理的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Swept {
    /// 那儿没有东西。
    Absent,
    /// 认出是我们放的、删了。
    Removed,
    /// 留着没动，`why` 说为什么（后端说的不是我们的 / 读不成文本；或读与删之间被改了）。
    Kept { why: String },
}

impl Swept {
    /// 部署按钮那句话后面要不要多说一句（没东西 ⇒ 不说）。
    pub(crate) fn say(&self) -> String {
        match self {
            Swept::Absent => String::new(),
            Swept::Removed => copy_text(
                "rsCcmLegacy.say.removed",
                &[("rel", &LEGACY_REL.to_string())],
            ),
            Swept::Kept { why } => copy_text(
                "rsCcmLegacy.say.kept",
                &[("rel", &LEGACY_REL.to_string()), ("why", &why.to_string())],
            ),
        }
    }
}

/// **照答办**：`remove` ⇒ 经那台机器的后端（`door`）带期望值删；别的原样变成结局。本函数不判「是不是我们放的」。
pub(crate) async fn apply<D: Door>(door: &D, verdict: Verdict) -> Result<Swept, String> {
    let expect = match verdict {
        Verdict::Absent => return Ok(Swept::Absent),
        Verdict::Keep { why } => return Ok(Swept::Kept { why }),
        Verdict::Remove { expect } => expect,
    };
    let home = door.home().await?;
    match door.delete(&home, LEGACY_REL, &expect).await {
        Ok(()) => Ok(Swept::Removed),
        Err(Refused::Stale(_)) => Ok(Swept::Kept {
            why: copy_text("rsCcmLegacy.kept.changed", &[]),
        }),
        Err(r) => Err(r.said()),
    }
}

/// 问一次、照答办一次（部署按钮 · 那台长连接握手完成，两个触发点同一个入口）。
pub(crate) async fn sweep(cfg: &crate::ssh_source::RemoteConfig) -> Result<Swept, String> {
    let verdict = ask_verdict(cfg).await?;
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(cfg.origin_label()));
    apply(&door, verdict).await
}

/// 升级那一格：那台的长连接握手完成那一刻，后台扫一次（`ssh_source` 在 `asset_sync::on_remote_ready` 旁边调）。
///
/// **每次连上都扫**，不只在「这一轮自动部署真写了字节」时扫：跳过预检那一格（`skip_preflight`）根本不跑部署，
/// 升级那一轮流若恰好没起来就永远删不掉；扫一次 ＝ 问一次本机常驻后端 `deploy-retired`（只读 SFTP stat，不在就停）。结局只进日志（足迹那一页看得到现状）。
pub(crate) fn on_remote_ready(cfg: &crate::ssh_source::RemoteConfig) {
    let cfg = cfg.clone();
    let origin = cfg.origin_label();
    tauri::async_runtime::spawn(async move {
        match sweep(&cfg).await {
            Ok(Swept::Absent) => {}
            Ok(Swept::Removed) => {
                tracing::info!(
                    "[{origin}] 旧版入口 ~/{LEGACY_REL} 本机后端认出是 cc-monitor 放的，已删"
                )
            }
            Ok(Swept::Kept { why }) => tracing::info!("[{origin}] ~/{LEGACY_REL} 没动：{why}"),
            Err(e) => tracing::warn!("[{origin}] 查旧版入口 ~/{LEGACY_REL} 没查成：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/ccm_legacy_tests.rs"]
mod tests;
