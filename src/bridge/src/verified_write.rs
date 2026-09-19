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
//! | `sftp.rs::install_remote_ccm_helper` 里 `CCM_CLI_SCRIPT` 那半（远端 ccm CLI） | 比内容 | 报错，不动 profile |
//! | `sftp.rs::install_remote_ccm_helper` 里 `merged` 那半（远端 profile） | 比内容 | 回滚 |
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

/// **写后校验器**：读回 → 比对 → 不符则回滚。
///
/// 两个动作以闭包注入，因为落点各不相同（本机 `std::fs` / 远端 SFTP），
/// **而"什么时候算失败、失败了要不要回滚"必须是同一套**——那才是这个抽象的内容。
///
/// 为什么要做成可注入而不是直接调 `std::fs`：变异测试实测发现，回滚那一步
/// **没有任何测试走得到**（它只在"真的写了文件且读回损坏"时才执行）。
/// 不可注入 = 不可测 = 那行代码没有门禁。现在 `rollback` 是否被调用可以直接断言。
///
/// ## 原先它还收一个 `write` 闭包，本轮**删掉了**（T01 审计 S7）
///
/// 两个真实调用点传的都是 `|| Ok(())` ——写入（含备份与写失败时的恢复）在调用方
/// 上方已经做完了，因为**那一段各落点不同**：本机侧要 `std::fs::copy` 备份、
/// 失败时要把备份路径拼进错误文本；远端侧要设权限位、要防传输损坏。
/// 留着那个参数的后果是：`write` 返回 `Err` 那条分支**生产上不可达**，
/// 而我为它写的测试看着是绿的——按本模块自己的 ≥2 判据，这个参数不合格。
/// 于是改名为 `verify_and_rollback`，让签名说的就是它真做的事。
///
/// 顺带说清一条**没被这个抽象覆盖**的：`sftp.rs` 那三处读回比对只共用了
/// [`verify_readback`]（判定），没走这里——它们的回滚是 `async` SFTP 操作，
/// 塞不进 `impl FnOnce()`。不谎称已统一。
pub fn verify_and_rollback(
    expected: &str,
    read_back: impl FnOnce() -> Result<String, String>,
    rollback: impl FnOnce(),
) -> Result<(), String> {
    let actual = match read_back() {
        Ok(a) => a,
        Err(e) => {
            // 读不回来 = 无法确认写对了 → 也要回滚。**不能当成功**：
            // "我写了但不知道写成什么样"和"我写对了"是两回事。
            rollback();
            return Err(format!("写后回读失败: {e}（已尝试回滚）"));
        }
    };
    if let WriteVerdict::Mismatch { detail } = verify_readback(expected, &actual) {
        rollback();
        return Err(format!("写后校验失败：{detail} 已尝试回滚。"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/bridge/verified_write_tests.rs"]
mod tests;
