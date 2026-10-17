use super::*;

/// 自上一次看过之后响没响（看过就算取走）。
fn rang(rx: &mut tokio::sync::watch::Receiver<u64>) -> bool {
    let r = rx.has_changed().expect("通道还在");
    rx.borrow_and_update();
    r
}

/// ★ 同一个会话连着变（记录一行行长）⇒ 只响一下；答过清单之后再变 ⇒ 再响一下。
#[test]
fn one_session_rings_once_until_the_list_is_answered() {
    let n = ListNode::default();
    let mut rx = n.subscribe();
    n.moved(Some("a"));
    assert!(rang(&mut rx));
    n.moved(Some("a"));
    n.moved(Some("a"));
    assert!(!rang(&mut rx), "还没答过清单：同一个会话不再响");
    n.moved(Some("b"));
    assert!(rang(&mut rx), "别的会话照响");
    n.asked();
    n.moved(Some("a"));
    assert!(rang(&mut rx), "答过之后再变 ⇒ 再响");
}

/// ★ 不分会话的那一种（注解 · 隐藏规则）每次都响。
#[test]
fn a_change_for_every_row_always_rings() {
    let n = ListNode::default();
    let mut rx = n.subscribe();
    n.moved(None);
    assert!(rang(&mut rx));
    n.moved(None);
    assert!(rang(&mut rx));
}
