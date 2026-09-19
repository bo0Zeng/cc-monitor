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
