//! # 要求住址：`INVARIANTS §4`（⑤ 回读逐字节比对）；用在部署物那一跳，见 `设计/15 §3.5`
//!
//! 核原文：`INVARIANTS §4` 逐字「⑤ 回读**逐字节**比对，不符回滚」—— 本族判 `verified_write.rs::verify_readback`
//! 对等长损坏（翻位 · 制表符换空格 · 大小写 · 换行 · 等长多字节）也判不符，不退回只比长度。
//! 今天唯一的调用方是 `fenced_block.rs::apply`（SFTP 部署物；`设计/15 §3.5` 逐字「读回逐字节比对」）。
//! ⚠ 用户文件那一份同名规则住后端 `files_write.rs::put_text` —— 同一个判定两个家（`D1` 角度记在 `JA1.md`）。〔JA1 点址 2026-09-24〕

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

// 〔RW1 · 第四波 09-24〕这里原来有三条回滚判据（内容不符回滚 · 写对了不回滚 · 读不回来也回滚），
//   守的是 `verify_and_rollback`〔散文墓碑〕。那个函数零调用方删了；同一组性质住到了后端
//   （`files_write_tests.rs` 的 `put_*` 那一族：CAS · 相同不写 · 回读 · 回滚那一支如实登记为「没量」）。

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
