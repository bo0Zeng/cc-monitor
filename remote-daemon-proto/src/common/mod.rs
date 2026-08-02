//! U2（2026-08-01）：**`common/` —— observe 与 control 两边都要、又不含平台原语的纯工具。**
//!
//! # 为什么 `platform`/`observe`/`control` 三分不够
//!
//! 计划自审 §0.5-6 打掉过我一处错：v1/v2 只写三分。实际 `projects_root` 有**四份逐字相同**的
//! 副本，其中 `fork_write.rs` 那份属 control、另三份属 observe —— 归哪边都不对，它谁也不属于。
//! 强行塞进 observe 会让 control 反向依赖 observe（回边），塞进 platform 更错（它不碰平台）。
//! ⇒ 加第四层。
//!
//! # 门槛（别让这里变成杂物间）
//!
//! 进 `common/` 要同时满足三条：
//! ① **≥2 个上层用**（按**层**数，不是按文件数）
//! ② **平台无关**（不含平台 cfg、不依赖某个 OS 的文件布局或 ABI —— 那是 `platform/` 的事）
//! ③ **无域知识**（不认识 `WatchEvent` / `ResumeSpec` 这类东西）
//!
//! 不满足就留在它自己的模块里 —— 「反正大家都可能用」不是理由。
//!
//! > **② 的措辞是 Phase D 审计订正的。** 我第一版写的是「**纯**（不 I/O 决策…）」，
//! > 而同一次提交放进来的 `mtime_ms` 第一行就是 `std::fs::metadata` —— **门槛被自己放进去的
//! > 函数当场破了**。门槛是防「杂物间」的唯一机制，第一条就自相矛盾会让它彻底失去约束力。
//! > 真正要挡的从来不是 I/O，是**平台知识**与**域知识**：`mtime_ms` 做 I/O 但对 OS 一无所知
//! > （`std::fs` 在哪都一样），它该在这里；`proc_starttime` 也做 I/O，但它认识 `/proc` 的字段
//! > 布局，它该在 `platform/`。
//!
//! > **一条已知的不达标项，登记而不粉饰**：`mtime_ms` 的两个调用点
//! > （`history_query.rs` / `search_query.rs`）**同属 observe**，按「层」口径①不成立。
//! > U3 划出 `observe/` 之后要重新判：要么它下沉成 observe 内部共享，要么真有 control 侧用上它。
//! > 现在不动是因为 `observe/` 还不存在，硬划等于凭空造一个层。**U3 的 DoD 里必须复查这一条。**

pub(crate) mod fs;
pub(crate) mod paths;
