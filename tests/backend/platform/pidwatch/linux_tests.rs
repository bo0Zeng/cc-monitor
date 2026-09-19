/// **「`poll` 真错误不判死」这条语义的结构性钉子。**
///
/// # 为什么是源码扫描而不是普通测试
///
/// 那条路径要求 `poll(2)` 返回负值**且不是 `EINTR`` —— 在测试里可靠地制造它需要
/// 伪造一个坏 fd 或注入 syscall 失败，成本远超收益。而它恰恰是**切分时最容易丢的一条**：
/// 另外三条都「发死亡事件」，只有它不发；重写的人很自然会把四条统一成「都发」，
/// 而后果是**一次系统调用失败就把活着的会话误归档**（与 `is_same_live_process` 头注
/// 那条「瞬时读失败绝不误归档」是同一条纪律）。
///
/// ⇒ 退而求其次：钉住**代码形状** —— EINTR 之后那段里不许出现 `on_dead`。
/// 这挡不住逻辑改写，但挡得住「顺手统一成都发」这个真实的失败模式。
/// Phase D 审计确认过：这条路径今天**零测试覆盖**，是 U2 之前就有的缺口。
#[test]
fn poll_hard_error_must_not_report_dead() {
    let src = include_str!("../../../../src/backend/platform/pidwatch/linux.rs");
    let prod = crate::guard_support::production_code(src);
    let i = prod
        .find("ErrorKind::Interrupted")
        .expect("找不到 EINTR 分支 —— 本钉子的锚点没了，先确认 poll 循环还在");
    // 从 EINTR 判断到函数收尾这一段 = 「真错误」的处理段。
    let tail = &prod[i..];
    let end = tail.find("\n    });").unwrap_or(tail.len());
    let hard_error_arm = &tail[..end];
    assert!(
        !hard_error_arm.contains("on_dead"),
        "`poll` 真错误的处理段里出现了 `on_dead` —— 那条路径**必须不判死**。\n             宁可让会话留在 live、等 pidfile 删除或断连来收，也不因一次系统调用失败就误归档。\n             实际那一段：\n{hard_error_arm}"
    );
    assert!(
        hard_error_arm.contains("放弃看守"),
        "「放弃看守（不报死）」那句 warn 不见了 —— 它是这条路径唯一的可观测痕迹"
    );
}
