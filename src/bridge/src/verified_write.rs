//! T01 第一块：**统一的「备份 → 写 → 读回比对 → 回滚」写入器**。
//!
//! ## 为什么要有它（不是为了去重）
//!
//! 去重本身证成不了一个抽象。真正的理由是实测发现：本仓这套范式有 **4 处独立实现，
//! 而它们的校验强度并不一致**——
//!
//! （下表记的是 **T01 之前**那四处各自的强度；四处今天都已改走本模块。）
//!
//! | 处 | 比什么 | 失败时 |
//! |---|---|---|
//! | `profile_installer.rs::install_to_profile`（写入） | **只比长度** | 从备份恢复 |
//! | `profile_installer.rs::uninstall_from_profile`（剥离） | **只比长度** | 从备份恢复 |
//! | `install_remote_ccm_helper`〔散文墓碑〕（`sftp.rs`）里 `CCM_CLI_SCRIPT` 那半（远端 ccm CLI） | 比内容 | 报错，不动 profile |
//! | `install_remote_ccm_helper`〔散文墓碑〕（`sftp.rs`）里 `merged` 那半（远端 profile） | 比内容 | 回滚 |
//!
//! 〔AL1 · 2026-09-24〕那两处今天都走 `fenced_block::apply`（入口那半并进了 `sftp.rs::deploy_remote_backend`，
//! rc 那半是 `install_remote_alias_block`，〔W5-ALIAS〕今天住 `profile_installer.rs`）。
//!
//! **本机侧只比长度 = 同长度的损坏被静默放过**：字节翻转、编码变形、CRLF↔LF 等长替换
//! 都能穿过去。而 `~/.bashrc` / `$PROFILE` 写坏的后果是用户下次开终端就炸。
//!
//! 所以本模块的统一语义取四者中**最强**的那一档：**内容级比对 + 回滚**。
//! 这条升级必须有一个会红的测试来证明（见 `content_differs_at_same_length_is_caught`）
//! ——重构完跑一遍原有测试不算数，**原有测试恰恰挡不住这个**，否则它早就红了。
//!
//! ## 边界
//!
//! 落点（本机 fs / 远端 SFTP）是**实现**，校验与回滚语义才是**共享**的。
//! 差异（远端要防传输损坏、要设权限位；本机不用）留在落点里，不上提到这里。

/// 一次写入尝试的结果判定。纯逻辑，不碰 I/O ——这样它才好测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteVerdict {
    /// 读回内容与期望逐字节相同。
    Ok,
    /// 读回内容与期望不符 → 必须回滚。`detail` 是给用户看的差异描述。
    Mismatch { detail: String },
}

/// **核心判据**：读回的内容是否与期望**逐字节**一致。
///
/// 刻意**不**提供"只比长度"的选项——那正是被本模块取代的弱实现。
/// 差异描述里同时给出长度与首个不同位置：长度相同的损坏若只报长度，用户会一头雾水。
pub fn verify_readback(expected: &str, actual: &str) -> WriteVerdict {
    if expected == actual {
        return WriteVerdict::Ok;
    }
    let detail = if expected.len() == actual.len() {
        // **这一支是本模块存在的理由**：长度相同但内容不同，旧的长度比对会放过。
        let at = expected
            .bytes()
            .zip(actual.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        format!(
            "长度相同（{} 字节）但内容不同，首个差异在第 {} 字节。\
             这类损坏（字节翻转 / 编码变形 / CRLF↔LF 等长替换）只比长度是查不出来的。",
            expected.len(),
            at
        )
    } else {
        format!(
            "长度不匹配：期望 {} 字节，实际 {} 字节。",
            expected.len(),
            actual.len()
        )
    };
    WriteVerdict::Mismatch { detail }
}

// 〔RW1 · 第四波 · 2026-09-24〕这里原来是 `verify_and_rollback`〔散文墓碑〕（写后回读比对、不符就调回滚闭包）。
// 它最后一个调用方（`skill_host::write_skill_file` 的本机直写）改经后端写之后零调用方 ⇒ 删了。
// 用户文件「备份 → 写 → 读回比对 → 回滚」那一份规则今天住后端 `control/files_write.rs::put_text`；
// 本模块只剩判定那一半 [`verify_readback`]（F08 部署物那一条 `fenced_block::apply` 还在用）。

#[cfg(test)]
#[path = "../../../tests/bridge/verified_write_tests.rs"]
mod tests;
