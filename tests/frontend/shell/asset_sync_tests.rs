//! monitor 侧 `asset_sync.rs` 的判据：交出去的是什么、答回来的认不认得、判定没长第二个家。
//!
//! 守的要求：「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」·
//! （一切判定都在后端）· 观测方沿它本来就拥有的那条连接去拉（拉的是本机常驻后端，不是 monitor）。
//!
//! 买到：握手那一刻只交那台的名字（`origin`；怎么够到它照本机后端的机器表）；入参 · 回包与后端那一条是同一份类型（`cmd_wire::assets`）；monitor 这一侧零合并规则。
//! 买不到：🔴 真远端 / 真本机后端（本机后端那一跳是替身）；连上那一刻的钩子只验「不认就不碰」这一半。

use super::*;
use cmd_wire::assets::{AssetsSyncRow, ReachRow};
use std::sync::Mutex;

struct Recorder {
    sent: Mutex<Vec<AssetsSyncReq>>,
    reply: AssetsSynced,
}

impl LocalBackend for Recorder {
    async fn call(&self, req: &AssetsSyncReq) -> Result<AssetsSynced, String> {
        self.sent.lock().unwrap().push(req.clone());
        Ok(self.reply.clone())
    }
}

fn good_reply() -> AssetsSynced {
    let dev = cmd_wire::Machine::named("dev");
    AssetsSynced {
        self_id: "9f".into(),
        synced: vec![AssetsSyncRow {
            origin: dev.clone(),
            peer: Some("4c".into()),
            changed: true,
            pushed: 1,
            error: None,
        }],
        reach: vec![ReachRow {
            origin: dev,
            machine: Some("4c".into()),
        }],
    }
}

/// 握手那一刻只交那台的名字：线上恰是 `{"origin": "dev"}`（入参 · 回包与后端那一条是同一份类型，`cmd_wire::assets`）。
#[tokio::test]
async fn the_handshake_hands_over_only_the_machine_name() {
    let rec = Recorder {
        sent: Mutex::new(vec![]),
        reply: good_reply(),
    };
    // 回包交回（monitor 不解释它，界面那一问经通道直问、`src/frontend/ui/assets-sync-reads.ts` 按形状收）。
    let got = sync_with("dev", &rec).await.expect("握手那一刻");
    assert_eq!(got, good_reply());
    let sent = rec.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    // 只交名字：拨号那一格不交（怎么够到那台由本机后端照它的机器表组）。
    assert_eq!(
        serde_json::to_value(&sent[0]).unwrap(),
        serde_json::json!({"origin": "dev"})
    );
}

/// 远端要认的那条命令真在后端命令表里（编进来的形状指纹表有它）。
#[test]
fn the_command_the_remote_must_know_is_registered() {
    assert!(
        cmd_wire::shapes::shape_of(REMOTE_NEEDS).is_some(),
        "后端命令表里没有 `{REMOTE_NEEDS}`"
    );
}

/// 判定没长第二个家：monitor 这一侧零合并 / 推送规则的字样（人群从后端两份源码现抠：它们的公开函数名）。
#[test]
fn this_module_holds_no_sync_rule() {
    let root = guard_core::addr::repo_root();
    let mine = guard_core::production_code(
        &std::fs::read_to_string(root.join("src/frontend/shell/src/asset_sync.rs")).unwrap(),
    );
    let mut needles = Vec::new();
    for f in [
        "src/backend/assets/asset_catalog.rs",
        "src/backend/assets/asset_sync.rs",
    ] {
        let src = std::fs::read_to_string(root.join(f)).unwrap();
        // 取「`pub` / `pub(crate)` [`async`] `fn` <名字>」里的名字：按词走，不在语料上做带字面量的前缀匹配（needle 棘轮）。
        // 包内可见的也算（资产域成了独立的包之后，包内才用的那几样是 `pub(crate)`，判定照样是后端的）。
        let words: Vec<&str> = src.split_whitespace().collect();
        for w in words.windows(3) {
            let (lead, kw, tail) = (w[0], w[1], w[2]);
            if kw == "fn" && (lead == "pub" || lead == "pub(crate)" || lead == "async") {
                let name: String = tail
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                needles.push(name);
            }
        }
    }
    assert!(
        needles.iter().any(|n| n == "merge") && needles.iter().any(|n| n == "push_plan"),
        "人群没抠到：{needles:?}"
    );
    let hits: Vec<&String> = needles
        .iter()
        .filter(|n| mine.contains(&format!("{n}(")))
        .collect();
    assert!(
        hits.is_empty(),
        "monitor 这一侧出现了后端判定函数的名字：{hits:?}"
    );
    // 正控：同一把尺子在后端那一份上数得出东西
    let backend = guard_core::production_code(
        &std::fs::read_to_string(root.join("src/backend/assets/asset_sync.rs")).unwrap(),
    );
    assert!(backend.contains("push_plan("));
}
