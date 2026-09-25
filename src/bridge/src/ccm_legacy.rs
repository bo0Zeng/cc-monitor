//! 〔GP1 · 第四波〕**旧版放在 `~/.local/bin/ccm` 的那一份，认出是我们放的就删。**
//!
//! # 要求住址
//!
//! `设计/01 §6.7b`（用户 09-18 · V28）逐字：「⚠ 这是一次迁移：远端要同拍做三件 —— ① 改成同落点同名字 ·
//! ② 清掉旧的 `~/.local/bin/ccm` shim · ③ 清掉同目录下与后端重复的字节。按 `96 §4`，三件都要在足迹里有入口。」
//! ① SR1b 已落（入口落 `~/.cc-monitor/bin/ccm`）；本模块做 ② ③。主会话 09-25 裁：按 V28 / V41 清掉，
//! **部署 / 升级时顺手删、足迹里可见**，不再「不删也不更新」（`调研/第四波记录/GP1.md §2`）。
//!
//! # 删的是哪几形（现打：git 史里我们往远端 `~/.local/bin/ccm` 推过的只有两形）
//!
//! | 形 | 第二行（记号） | 来自 |
//! |---|---|---|
//! | 三行 shim（09-11 起，历代都是这一行） | 与今天 `local_backend::ccm_entry_shim` 的第二行**逐字相同** | `ccm_entry_shim` 历代 |
//! | bash 启动器（09-11 之前，`shared/ccm`，后端 ccm 的第二份实现 ＝ ③ 那份「与后端重复的」） | `# ccm — cc-monitor 统一启动器…` | `K-R48` 删掉的那份文件 |
//!
//! 第一行必须是 `#!`、第二行认得出其中一形 ⇒ 是我们放的 ⇒ 经**那台机器的后端**删（`user_files::Door::delete`，带 CAS：
//! 盘上逐字节等于刚读到的那一份才删）。别的一律**不动**（用户自己的脚本、读不成文本的、记号不在第二行的）。
//! ⚠ V41「不为旧状态留兼容」：记号只回答「这是不是我们放的」这一问，不据它做任何别的事。
//!
//! # 为什么不走部署那条 SFTP
//!
//! 远端写只许 `~/.cc-monitor/{staging,bin}` 两处（V89 · `src/backend/dial/sftp.rs` 围栏），`~/.local/bin` 在两根之外；
//! 删用户家目录里的一个文件是文件管理那一面的事（`user_files` 头注：monitor 够用户文件的唯一开口）。
//!
//! # 买不到的
//!
//! - 本机那一份（monitor 跑着的这台上的 `~/.local/bin/ccm`）本模块不碰：`01 §6.7b` 那句说的是远端；
//!   本机 `~/.cc-monitor/bin/ccm` 是后端的逐字节副本（V28「第二份拷贝」），消掉它要 `ccm` 就是后端本身 ——
//!   卡在 `tests/evidence/MC1-AL1-摸底.md` 第三节那道 argv 设计题，报告里交主会话。
//! - 真远端一次都没跑过（判据用内存替身 `Door`）。

use crate::user_files::{Door, Refused};

/// 那一份在家目录下的相对路径。**唯一住址**（足迹那一行由判据对拍它）。
pub(crate) const LEGACY_REL: &str = ".local/bin/ccm";

/// 09-11 之前那份 bash 启动器第二行的开头（那份文件已删，记号只能是字面量；出处：`git show e8f9e08e^:shared/ccm`）。
const LAUNCHER_MARK: &str = "# ccm — cc-monitor 统一启动器";

/// 这份文本是不是我们放的（两形之一）。**纯函数**。
pub(crate) fn is_ours(text: &str) -> bool {
    let mut lines = text.lines();
    let (Some(first), Some(second)) = (lines.next(), lines.next()) else {
        return false;
    };
    if !first.starts_with("#!") {
        return false;
    }
    // shim 那一形的记号取自**今天的生成器**（第二行历代未变）—— 不在这里抄第二份字面量。
    let shim = crate::backend::control::local_backend::ccm_entry_shim("/x");
    let shim_mark = shim.lines().nth(1).unwrap_or("\u{0}");
    second == shim_mark || second.starts_with(LAUNCHER_MARK)
}

/// 一次清理的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Swept {
    /// 那儿没有东西。
    Absent,
    /// 认出是我们放的、删了（`path` = 后端解过链接的那一份）。
    Removed { path: String },
    /// 留着没动，`why` 说为什么（不是我们的 / 读不成文本 / 读与删之间被改了）。
    Kept { why: String },
}

impl Swept {
    /// 部署按钮那句话后面要不要多说一句（没东西 ⇒ 不说）。
    pub(crate) fn say(&self) -> String {
        match self {
            Swept::Absent => String::new(),
            Swept::Removed { .. } => format!("旧版放在 ~/{LEGACY_REL} 的入口已删掉。"),
            Swept::Kept { why } => format!("~/{LEGACY_REL} 没动：{why}。"),
        }
    }
}

/// 看一眼、认得出就删。**只经那台机器的后端**（`door`）。
pub(crate) async fn sweep<D: Door>(door: &D) -> Result<Swept, String> {
    let home = door.home().await?;
    let got = match door.peek(&home, LEGACY_REL).await {
        Ok(g) => g,
        // 读不成（不是文本 / 读不动）⇒ 不认、不动。说出来，不当成「不在」。
        Err(e) => {
            return Ok(Swept::Kept {
                why: format!("读不成文本（{e}），认不出是不是 cc-monitor 放的"),
            })
        }
    };
    let Some(text) = got.text else {
        return Ok(Swept::Absent);
    };
    if !is_ours(&text) {
        return Ok(Swept::Kept {
            why: "不是 cc-monitor 放的".to_string(),
        });
    }
    match door.delete(&home, LEGACY_REL, &text).await {
        Ok(()) => Ok(Swept::Removed { path: got.path }),
        Err(Refused::Stale(_)) => Ok(Swept::Kept {
            why: "刚读完它就被改了，这一次不删".to_string(),
        }),
        Err(r) => Err(r.said()),
    }
}

/// 升级那一格：那台的长连接握手完成那一刻，后台扫一次（`ssh_source` 在 `asset_sync::on_remote_ready` 旁边调）。
///
/// **每次连上都扫**，不只在「这一轮自动部署真写了字节」时扫：跳过预检那一格（`skip_preflight`）根本不跑部署，
/// 升级那一轮流若恰好没起来就永远删不掉；扫一次 ＝ 一次 `files-peek`（不在就停）。结局只进日志（足迹那一页看得到现状）。
pub(crate) fn on_remote_ready(cfg: &crate::ssh_source::RemoteConfig) {
    let origin = cfg.origin_label();
    tauri::async_runtime::spawn(async move {
        let door = crate::user_files::BackendDoor::new(crate::origin::Origin(origin.clone()));
        match sweep(&door).await {
            Ok(Swept::Absent) => {}
            Ok(Swept::Removed { path }) => {
                tracing::info!("[{origin}] 旧版入口 {path} 认出是 cc-monitor 放的，已删")
            }
            Ok(Swept::Kept { why }) => tracing::info!("[{origin}] ~/{LEGACY_REL} 没动：{why}"),
            Err(e) => tracing::warn!("[{origin}] 查旧版入口 ~/{LEGACY_REL} 没查成：{e}"),
        }
    });
}

#[cfg(test)]
#[path = "../../../tests/bridge/ccm_legacy_tests.rs"]
mod tests;
