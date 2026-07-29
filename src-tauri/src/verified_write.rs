//! T01 第一块：**统一的「备份 → 写 → 读回比对 → 回滚」写入器**。
//!
//! ## 为什么要有它（不是为了去重）
//!
//! 去重本身证成不了一个抽象。真正的理由是实测发现：本仓这套范式有 **4 处独立实现，
//! 而它们的校验强度并不一致**——
//!
//! | 处 | 比什么 | 失败时 |
//! |---|---|---|
//! | `profile_installer.rs:242`（写入） | **只比长度** | 从备份恢复 |
//! | `profile_installer.rs:305`（剥离） | **只比长度** | 从备份恢复 |
//! | `sftp.rs:729`（远端 ccm CLI） | 比内容 | 报错，不动 profile |
//! | `sftp.rs:767`（远端 profile） | 比内容 | 回滚 |
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
mod tests {
    use super::*;

    /// **本模块存在意义的直接证明。** 旧实现（`written.len() != updated.len()`）对这些
    /// 输入一律放行；新实现必须全部拦下。这条测试若被删/改弱，升级就白做了。
    #[test]
    fn content_differs_at_same_length_is_caught() {
        let cases: &[(&str, &str, &str)] = &[
            // 单字节翻转
            (
                "export PATH=/usr/bin\n",
                "export PATH=/usr/bon\n",
                "字节翻转",
            ),
            // CRLF ↔ LF 之外的等长替换：制表符 ↔ 空格
            ("a\tb\n", "a b\n", "制表符换空格"),
            // 大小写变形（某些同步工具会干这事）
            ("export FOO=1\n", "export foo=1\n", "大小写变形"),
            // 行尾 LF → CR（某些传输/编辑器会干这事，两者都是 1 字节）
            ("alias cc='ccm'\n", "alias cc='ccm'\r", "行尾 LF 变 CR"),
        ];
        for (expected, actual, what) in cases {
            assert_eq!(
                expected.len(),
                actual.len(),
                "构造错误：{what} 两侧长度应相同"
            );
            // 旧判据（只比长度）会放行 —— 把它写出来，证明差别是真的
            assert!(
                expected.len() == actual.len(),
                "旧的长度比对对 {what} 判为通过"
            );
            // 新判据必须拦下
            let v = verify_readback(expected, actual);
            assert_ne!(v, WriteVerdict::Ok, "{what} 必须被拦下");
            match v {
                WriteVerdict::Mismatch { detail } => {
                    assert!(
                        detail.contains("长度相同"),
                        "{what} 的差异描述要说清是等长损坏"
                    );
                    assert!(detail.contains("首个差异在第"), "{what} 要指出差异位置");
                }
                WriteVerdict::Ok => unreachable!(),
            }
        }
    }

    // ===== 统一写入器：回滚这一步此前**没有任何测试走得到** =====
    use std::cell::Cell;

    #[test]
    fn rollback_is_called_on_content_mismatch() {
        let rolled = Cell::new(false);
        let r = verify_and_rollback(
            "expected\n",
            || Ok("expectee\n".to_string()), // 同长度、内容不同
            || rolled.set(true),
        );
        assert!(r.is_err(), "等长损坏必须判失败");
        assert!(rolled.get(), "校验不过必须回滚");
        assert!(
            r.unwrap_err().contains("长度相同"),
            "错误里要说清是等长损坏"
        );
    }

    #[test]
    fn rollback_is_not_called_on_success() {
        let rolled = Cell::new(false);
        let r = verify_and_rollback("same\n", || Ok("same\n".to_string()), || rolled.set(true));
        assert!(r.is_ok());
        assert!(!rolled.get(), "写对了不该回滚（回滚会把刚写好的覆盖掉）");
    }

    #[test]
    fn readback_failure_also_rolls_back() {
        // "我写了但读不回来" ≠ "我写对了"。不能当成功放过。
        let rolled = Cell::new(false);
        let r = verify_and_rollback(
            "x\n",
            || Err("permission denied".into()),
            || rolled.set(true),
        );
        assert!(r.is_err());
        assert!(rolled.get(), "读不回来也要回滚");
        assert!(r.unwrap_err().contains("回读失败"));
    }

    // 原先这里还有一条 `write_failure_short_circuits_without_rollback`。
    // 它守的是 `write` 闭包返回 `Err` 那条路，而两个真实调用点传的都是 `|| Ok(())`
    // ——**生产不可达**。删参数的同时删掉它：留着就是一条恒绿的装饰（T01 审计 S7）。

    #[test]
    fn identical_content_passes() {
        assert_eq!(verify_readback("", ""), WriteVerdict::Ok);
        assert_eq!(verify_readback("a\nb\n", "a\nb\n"), WriteVerdict::Ok);
        // 含中文与制表符的真实 profile 片段
        let s = "# ccm 别名块\nalias cct='ccm --tmux'\t# 注释\n";
        assert_eq!(verify_readback(s, s), WriteVerdict::Ok);
    }

    #[test]
    fn different_length_still_caught_with_useful_detail() {
        let v = verify_readback("abc\n", "ab\n");
        assert_ne!(v, WriteVerdict::Ok);
        match v {
            WriteVerdict::Mismatch { detail } => {
                assert!(detail.contains("长度不匹配"));
                assert!(detail.contains("期望 4 字节"));
                assert!(detail.contains("实际 3 字节"));
            }
            WriteVerdict::Ok => unreachable!(),
        }
    }

    #[test]
    fn truncation_and_empty_readback_are_caught() {
        // 写了但文件被清空（磁盘满 / 中断）
        assert_ne!(verify_readback("something\n", ""), WriteVerdict::Ok);
        // 期望空但读回有内容
        assert_ne!(verify_readback("", "leftover\n"), WriteVerdict::Ok);
    }

    #[test]
    fn multibyte_boundary_is_safe() {
        // 中文等长替换：两个不同汉字都是 3 字节
        let a = "路径：中\n";
        let b = "路径：文\n";
        assert_eq!(a.len(), b.len());
        let v = verify_readback(a, b);
        assert_ne!(v, WriteVerdict::Ok, "等长的多字节差异同样要拦下");
        // 不得 panic（按字节找差异位置时可能落在字符中间，只用于展示）
        match v {
            WriteVerdict::Mismatch { detail } => assert!(detail.contains("长度相同")),
            WriteVerdict::Ok => unreachable!(),
        }
    }
}
