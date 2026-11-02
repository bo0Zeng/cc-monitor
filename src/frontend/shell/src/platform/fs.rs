//! 平台相关的文件系统原语。**存在的理由是 `backend-split` 的 C10**〔用 08-01〕：
//! 「`platform/` 是**唯一**允许平台原语与平台 cfg 的地方」——`backend/` 那一半必须平台无关，
//! 所以凡是带 `#[cfg(unix)]` / `std::os::*` 的文件操作都住在这里，由宿主注入给 backend。
//!
//! # 诚实边界 10g：**没有判据钉「平台原语只许住这里」**
//!
//! `platform_home_guard.rs::platform_forms_live_only_at_home` 只扫 `backend/`，
//! 它保证的是「那半没有平台原语」，**不保证「平台原语都在本文件」**。
//! 有人在 `backend/` 之外的别处再写一个 `#[cfg(unix)]`，**没有任何东西会红**。
//! ⇒ 今天靠约定。解锁条件：backend 侧出现第二处平台原语时建 `backend/platform/`，
//! 届时把这条一并收进那层的判据。
//!
//! ⚠ **不是「工具函数堆」** —— 只放「同一件事在两个平台上做法不同」的那种原语。
//! 纯逻辑（路径拼接、命名规则）不许进来：那些在 backend 里就能测，搬进来反而丢了可测性。

use copy_core::copy_text;
use copy_core::said::Said;
use std::path::Path;

/// 置可执行位。
///
/// Unix：`0o700`（**只给本人**——释放出来的是后端二进制，没有理由让同机别的用户能跑它）。
/// Windows：**无操作** —— 可执行性由扩展名决定，没有对应的位可置。
///
/// 这是 `local_backend.rs::extract_embedded_to` 的注入参数：
/// backend 那边只知道「写完要让它可执行」，不知道**这个平台上那句话怎么落**。
// 唯一的调用方是 monitor 那侧的凭据写口，那个写口随「本机那一份也交本机常驻后端写」删了 ⇒ 它零调用、删掉。
// 收窄那条原语照旧只住 `creds_core::perm`，今天只有后端账号域那一份写口在用。

pub fn make_executable(p: &Path) -> Result<(), Said> {
    #[cfg(unix)]
    #[expect(
        clippy::disallowed_methods,
        reason = "`own_fs` 底下那一层的平台原语：放部署物时置可执行位，落点由调用方（`own_fs::OwnBin` · 释放的注入参数）定"
    )]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
            Said::with_raw(
                copy_text(
                    "rsPlatformFs.chmod.failed",
                    &[("why", &copy_core::io_reason(e.kind()))],
                ),
                &e,
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = p; // Windows 上没有可执行位；参数照收，签名两边一致。
    }
    Ok(())
}

/// **monitor 建后端自家目录（`~/.cc-monitor` 与它底下几层）的那一个函数**：
/// 建的那一下就是只给本人（unix：`DirBuilder` 的 mode 在创建时生效，没有「先按 umask 建出来、再收窄」的那一段），
/// 缺的中间几层一并这样建；**已在的不动**（那可能是用户自己设的）。别的平台照常建（那边不是 unix 权限位这一问）。
///
/// 建法只有一处（`platform_core::dir::private_dir_all`，后端同一份）；这里只造那一句。
/// 这是 `local_backend.rs` 那几处释放的注入参数（`C10`：那一半不认识平台），宿主自己也直接调。
pub fn ensure_private_dir(dir: &Path) -> Result<(), Said> {
    platform_core::dir::private_dir_all(dir).map_err(|e| {
        Said::with_raw(
            copy_text(
                "reason.fs.writeFailed",
                &[
                    ("path", &(dir.display()).to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })
}

/// 这一趟 `open` 若是**新建**，新文件只给本人（unix：`mode(0o600)`，创建那一刻生效）；别的平台照常建。
/// 调用方是 `local_backend_host.rs` 写监听口钥匙 / pid 记录的那两处（钥匙是那条回环口上唯一的门）。
pub fn only_me_on_create(opts: &mut std::fs::OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    #[cfg(not(unix))]
    {
        let _ = opts; // 不是 unix 权限位这一问；参数照收，签名两边一致。
    }
}

/// **monitor 自有状态文件的跨进程锁**：锁那份文件所在的**目录**（不是文件：文件每写一次就被原子挪换成新 inode）。
///
/// 守的要求：「CFG1 把 `config.json` 收成单一写口 …… 两个 monitor 进程同写没有跨进程锁 —— 用你那一族同一套 `flock`
/// （Windows 对应）把它也包上」。锁本身只有一处（`platform_core::dir::hold`，后端第四层那把同一份 ⇒ 两边进内核同一张表、互斥）；
/// 这里只造拿不到时的那一句。阻塞等到拿到为止（等的是别人放锁，不是节拍）。`dir` 必须已经在（这里不建：建目录是调用方的事）。
pub fn hold_dir_lock(dir: &Path) -> Result<platform_core::dir::DirLock, Said> {
    use platform_core::dir::{LockFail, LockStage};
    platform_core::dir::hold(dir).map_err(|LockFail { stage, err }| {
        let shown = dir.display().to_string();
        let said = match stage {
            LockStage::Open => copy_text(
                "rsPlatformFs.lock.openFailed",
                &[("dir", &shown), ("why", &copy_core::io_reason(err.kind()))],
            ),
            LockStage::Lock | LockStage::Wait => {
                copy_text("rsPlatformFs.lock.failed", &[("dir", &shown)])
            }
        };
        Said::with_raw(said, &err)
    })
}
