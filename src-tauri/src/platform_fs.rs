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

use std::path::Path;

/// 置可执行位。
///
/// Unix：`0o700`（**只给本人**——释放出来的是 daemon 二进制，没有理由让同机别的用户能跑它）。
/// Windows：**无操作** —— 可执行性由扩展名决定，没有对应的位可置。
///
/// 这是 `backend/control/local_backend.rs::extract_embedded_to` 的注入参数：
/// backend 那边只知道「写完要让它可执行」，不知道**这个平台上那句话怎么落**。
/// 把一份文件收窄到**只给本人**。
///
/// # ★ 它为什么只是一层转发，而不是在这里写 `#[cfg(windows)]`
///
/// 本文件的存在理由是 `backend-split` `C10`〔用 08-01〕：「`platform/` 是**唯一**允许
/// 平台原语与平台 cfg 的地方」。而 `K-H2a` 裁三（PM 08-27）把这条原语的**住址**定在了
/// `crates/creds-core`，理由是**中转住 daemon crate、它不依赖 `src-tauri`** ——
/// 两边各写一份 `#[cfg(windows)]` 设 DACL，就是「**一个安全性质两个实现**」，
/// 什么时候漂开没有任何东西会说。
///
/// ⇒ 真正的实现（Unix `0o600` / Windows `SetNamedSecurityInfoW` + 断继承 /
/// 其余平台**诚实报错**）住 `creds_core::perm::make_private`，本函数是**调用点之一**。
/// 签名与 `make_executable` 逐字同形：收一个路径、还一个 `Result<(), String>`，
/// 调用方只知道「写完要让它只给本人」，不知道这个平台上那句话怎么落。
///
/// ⚠ **诚实边界**（与本文件头注 `10g` 同一条）：没有判据钉「平台原语只许住这一层」。
/// `backend/mod.rs` 那条只扫 `backend/`，daemon 的 `fallback_guard` 只扫 `platform/`,
/// **两条都扫不到 `src-tauri/crates/`** ⇒ 有人在别处再写一个平台 cfg **不会红**。今天靠约定。
pub fn make_private(p: &Path) -> Result<(), String> {
    creds_core::perm::make_private(p)
}

pub fn make_executable(p: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("置可执行位失败: {e}"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = p; // Windows 上没有可执行位；参数照收，签名两边一致。
    }
    Ok(())
}
