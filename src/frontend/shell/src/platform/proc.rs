//! 进程这一族的平台差异〔P4b · 阶段 H：`设计/90 §4`〕：壳里平台原语的唯一住址（同 [`super::fs`]）。
//! ⚠ P4 同名同住址另收了一份（`CAN_DETACH` · `exit_signal` · `os_opener` 与这同一个 `EXE_SUFFIX`）：合并时取并集、`EXE_SUFFIX` 只留一行。

/// 这个平台上可执行文件名的后缀（Windows `.exe`，别处空串）。〔P4b〕从 `local_backend.rs::resolve_beside_this_exe` 收来：平台原语只住这一层。
pub const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;
