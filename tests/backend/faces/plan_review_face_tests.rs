//! 计划认可：只收此刻在的、不是问人那一种的键。

use super::*;
use crate::plan::fixture::{dump, SUB};

fn doc_with(who: &dyn Fn(&str) -> crate::plan::Whose) -> wire::PlanRead {
    let b = crate::plan::book::Book::default();
    let d = dump("/w");
    let raw = d.to_string().into_bytes();
    b.ingest(
        crate::plan::dump::Ran::Dump { doc: d, raw },
        std::path::Path::new("/w"),
        who,
        1,
    )
    .unwrap()
}

#[test]
fn only_a_present_key_that_is_not_an_ask_can_be_acked() {
    let doc = doc_with(&crate::plan::fixture::who);
    let sl = &doc.slices[0];
    let red = "red:边指着图上的节点@project";
    assert!(check_ack(sl, red).is_ok());
    assert_eq!(
        check_ack(sl, "red:别的@project")
            .unwrap_err()
            .code
            .as_deref(),
        Some("no_such_need")
    );
    let ask = format!("ask:A2@{SUB}");
    assert_eq!(
        check_ack(sl, &ask).unwrap_err().code.as_deref(),
        Some("not_ackable")
    );
}

/// 撤回认可走 `plan-unack` 那一支：认可过的撤回 ⇒ 那一条回到没认可、要你看的数加回去；撤回后再认可照常。
/// 没认可过的撤回今天照「没有也不算错」答（`PlanUnackErr` 没有那一个码），只答成、不多记一条。
#[test]
fn unack_puts_the_need_back_and_a_later_ack_still_takes() {
    // 进程里那一本与这个测试进程的家（沙箱）：工作区取一个只这条用的名字，别的判据撞不到。
    let ws = "/w-unack";
    let d = dump(ws);
    let raw = d.to_string().into_bytes();
    crate::plan::book::book()
        .ingest(
            crate::plan::dump::Ran::Dump { doc: d, raw },
            std::path::Path::new(ws),
            &crate::plan::fixture::who,
            1,
        )
        .unwrap();
    // 认可记录落这条判据自己的家（每跑一次都是空的）：写哪份就读哪份，不读进程的家（上一趟留下的认可不算进来）。
    let home = test_home::scratch("plan-unack");
    let own = crate::platform::fs::Own::issue_at(&home);
    let file = home.join(relay_route_core::PLAN_REVIEW_REL);
    let key = "red:边指着图上的节点@project";
    let req = |k: &str| PlanAckReq {
        workspace: ws.into(),
        slice: "alpha".into(),
        key: k.into(),
    };
    // 记下了没有：照那份文件看（认可那一格是 `acks.<工作区>.<片>[]`）。
    let acked = || {
        std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| v["acks"][ws]["alpha"].as_array().cloned())
            .is_some_and(|ks| ks.iter().any(|k| k == key))
    };

    let base = after_ack(ws, key, &own).unwrap().need_count;
    // 没认可过就撤 ⇒ 答成，什么都没记上。
    let r = unack(&req(key), &own).unwrap();
    assert_eq!((r.key.as_str(), r.need_count), (key, base));
    assert!(!acked());

    let r = ack(&req(key), &own).unwrap();
    assert!(acked());
    assert_eq!(r.need_count, base - 1, "认可过 ⇒ 少一条要你看");

    let r = unack(&req(key), &own).unwrap();
    assert!(!acked(), "撤回 ⇒ 那一条回到没认可");
    assert_eq!(r.need_count, base, "撤回 ⇒ 要你看的数加回去");

    let r = ack(&req(key), &own).unwrap();
    assert!(acked(), "撤回之后再认可照常");
    assert_eq!(r.need_count, base - 1);
    let _ = std::fs::remove_dir_all(&home);
}
