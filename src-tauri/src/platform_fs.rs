//! 平台相关的文件系统原语。**存在的理由是 C10**：`backend/` 那一半必须平台无关，
//! 所以凡是带 `#[cfg(unix)]` / `std::os::*` 的文件操作都住在这里，由宿主注入给 backend。
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
