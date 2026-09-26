//! 平台相关的文件系统原语。**存在的理由是 `backend-split` 的 C10**〔用 08-01〕：
//! 「`platform/` 是**唯一**允许平台原语与平台 cfg 的地方」——`backend/` 那一半必须平台无关，
//! 所以凡是带 `#[cfg(unix)]` / `std::os::*` 的文件操作都住在这里，由宿主注入给 backend。
//!
//! # 诚实边界 10g：**没有判据钉「平台原语只许住这里」**
//!
//! `backend/mod.rs` 的 `the_backend_half_stays_platform_agnostic` 只扫 `backend/`，
//! 它保证的是「那半没有平台原语」，**不保证「平台原语都在本文件」**。
//! 有人在 `backend/` 之外的别处再写一个 `#[cfg(unix)]`，**没有任何东西会红**。
//! ⇒ 今天靠约定。解锁条件：backend 侧出现第二处平台原语时建 `backend/platform/`，
//! 届时把这条一并收进那层的判据。
//!
//! ⚠ **不是「工具函数堆」** —— 只放「同一件事在两个平台上做法不同」的那种原语。
//! 纯逻辑（路径拼接、命名规则）不许进来：那些在 backend 里就能测，搬进来反而丢了可测性。

use crate::copy_table::copy_text;
use std::path::Path;

/// 置可执行位。
///
/// Unix：`0o700`（**只给本人**——释放出来的是后端二进制，没有理由让同机别的用户能跑它）。
/// Windows：**无操作** —— 可执行性由扩展名决定，没有对应的位可置。
///
/// 这是 `backend/control/local_backend.rs::extract_embedded_to` 的注入参数：
/// backend 那边只知道「写完要让它可执行」，不知道**这个平台上那句话怎么落**。
// 〔GP1 · 第四波〕这里原来有 `make_private`〔散文墓碑〕（转发 `creds_core::perm::make_private`，把凭据文件收成只给本人）。
// 唯一的调用方是 monitor 那侧的凭据写口，那个写口随「本机那一份也交本机常驻后端写」删了 ⇒ 它零调用、删掉。
// 收窄那条原语照旧只住 `creds_core::perm`，今天只有后端账号域那一份写口在用。

pub fn make_executable(p: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| copy_text("rsPlatformFs.chmod.failed", &[("e", &e.to_string())]))?;
    }
    #[cfg(not(unix))]
    {
        let _ = p; // Windows 上没有可执行位；参数照收，签名两边一致。
    }
    Ok(())
}

/// 〔HX1 · 4D · 主会话裁 HX1 拍板项 4〕**monitor 建后端自家目录（`~/.cc-monitor` 与它底下几层）的那一个函数**：
/// 建的那一下就是只给本人（unix：`DirBuilder` 的 mode 在创建时生效，没有「先按 umask 建出来、再收窄」的那一段），
/// 缺的中间几层一并这样建；**已在的不动**（那可能是用户自己设的）。别的平台照常建（那边不是 unix 权限位这一问）。
///
/// 与后端那一份（`src/backend/own_dir.rs::ensure_private_dir`）是两个 crate 各一份：两个 crate 没有能放平台原语的共享落点
/// （`creds-core` 的平台那半是 `harden` feature，monitor 不开）—— 权限位同一个值（0700），各自的判据各钉一半。
/// 这是 `backend/control/local_backend.rs` 那几处释放的注入参数（`C10`：那一半不认识平台），宿主自己也直接调。
pub fn ensure_private_dir(dir: &Path) -> Result<(), String> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        b.mode(0o700);
    }
    b.create(dir).map_err(|e| {
        copy_text(
            "rsPlatformFs.mkdir.failed",
            &[("dir", &(dir.display()).to_string()), ("e", &e.to_string())],
        )
    })
}
