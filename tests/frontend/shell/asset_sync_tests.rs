//! monitor 侧 `asset_sync.rs` 的判据：交出去的是什么、答回来的认不认得、判定没长第二个家。
//!
//! 守的要求：用户原话「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」·
//! （一切判定都在后端）· 观测方沿它本来就拥有的那条连接去拉（拉的是本机常驻后端，不是 monitor）。
//!
//! 买到：握手那一刻交的是「拨号请求（`capture`）＋ origin」；发的字段 ⊆ 后端登记的那一条（跨半边，读后端源码）；monitor 这一侧零合并规则。
//! 买不到：🔴 真远端 / 真本机后端（本机后端那一跳是替身）；连上那一刻的钩子只验「不认就不碰」这一半。

use super::*;
use std::sync::Mutex;

struct Recorder {
    sent: Mutex<Vec<Value>>,
    reply: Value,
}

impl LocalBackend for Recorder {
    async fn call(&self, args: Value) -> Result<Value, String> {
        self.sent.lock().unwrap().push(args);
        Ok(self.reply.clone())
    }
}

fn cfg() -> crate::stream_source::RemoteConfig {
    serde_json::from_value(json!({
        "host": "192.0.2.2",
        "label": "dev",
        "port": 2222,
        "user": "u",
        "keyPath": "/home/me/.ssh/id_ed25519",
    }))
    .or_else(|_| {
        serde_json::from_value(json!({
            "host": "192.0.2.2",
            "label": "dev",
            "port": 2222,
            "user": "u",
            "key_path": "/home/me/.ssh/id_ed25519",
        }))
    })
    .expect("夹具配置")
}

fn good_reply() -> Value {
    json!({
        "self": "9f",
        "synced": [{"origin": "dev", "peer": "4c", "changed": true, "pushed": 1, "error": null}],
        "reach": [{"origin": "dev", "machine": "4c"}],
    })
}

#[tokio::test]
async fn the_handshake_hands_over_how_to_reach_that_machine() {
    let rec = Recorder {
        sent: Mutex::new(vec![]),
        reply: good_reply(),
    };
    // 应答原样交回（monitor 不解释它，界面那一问经通道直问、`src/frontend/ui/assets-sync-reads.ts` 按形状收）。
    let got = sync_with(&cfg(), &rec).await.expect("握手那一刻");
    assert_eq!(got, good_reply());
    let sent = rec.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["origin"], json!("dev"));
    // 那台后端的路径不交（落点是固定常量）。
    assert_eq!(sent[0].get("backend"), None);
    assert_eq!(sent[0]["dial"]["use"], json!("capture"));
    assert_eq!(sent[0]["dial"]["machine"]["host"], json!("192.0.2.2"));
    assert_eq!(
        sent[0]["dial"]["machine"]["keyPath"],
        json!("/home/me/.ssh/id_ed25519"),
        "只交路径"
    );
}

// 「应答缺格就报错不猜」那一条随 `parse_reply`〔散文墓碑〕挪到界面：`tests/assets-sync-reads.vitest.ts`（金样 ＋ 逐格坏样）。

/// 跨半边：本侧发的命令名与字段 ⊆ 后端 `REGISTRY` 那一条声明的 `fields`，读的两格也在里面；远端要认的那条命令真在后端命令表里。
/// 一条登记里 `fields: &[…]` 声明的字段名：每个 `arg(` / `out(` / `both(` 的第一个实参（第二个是说明）。
fn declared_fields(block: &str) -> std::collections::BTreeSet<String> {
    let at = block.find("fields: &[").expect("fields 那一格");
    let list = &block[at..at + block[at..].find("],").expect("fields 没收尾")];
    let mut out = std::collections::BTreeSet::new();
    for ctor in ["arg(\"", "out(\"", "both(\""] {
        for (i, _) in list.match_indices(ctor) {
            let name = &list[i + ctor.len()..];
            out.insert(name[..name.find('"').expect("字段名没收尾")].to_string());
        }
    }
    assert!(
        !out.is_empty(),
        "从 fields 里一个字段名都没抠到 —— 登记的写法变了"
    );
    out
}

#[test]
fn what_monitor_sends_and_reads_is_what_the_backend_registers() {
    let families = crate::guard_support::backend_registry_sources();
    let (family, at) = families
        .iter()
        .find_map(|(_, prod)| prod.find(&format!("name: \"{CMD}\"")).map(|at| (prod, at)))
        .unwrap_or_else(|| panic!("后端 REGISTRY 里没有 `{CMD}`"));
    let block = &family[at..at + family[at..].find("run:").expect("那一条的 run")];
    let declared = declared_fields(block);
    let used: std::collections::BTreeSet<String> = ["origin", "dial", "self", "synced", "reach"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        declared, used,
        "本侧发 / 读的字段与后端声明的不相等（两向）"
    );
    // 远端要认的那条在后端命令表里
    let registered: std::collections::BTreeSet<&str> = families
        .iter()
        .flat_map(|(_, prod)| {
            prod.split("name: \"")
                .skip(1)
                .filter_map(|t| t.split('"').next())
        })
        .collect();
    assert!(
        registered.contains(REMOTE_NEEDS),
        "后端命令表里没有 `{REMOTE_NEEDS}`"
    );
    assert!(registered.contains(CMD), "后端命令表里没有 `{CMD}`");
}

/// 判定没长第二个家：monitor 这一侧零合并 / 推送规则的字样（人群从后端两份源码现抠：它们的公开函数名）。
#[test]
fn this_module_holds_no_sync_rule() {
    let root = crate::guard_support::repo_root();
    let mine = guard_core::production_code(
        &std::fs::read_to_string(root.join("src/frontend/shell/src/asset_sync.rs")).unwrap(),
    );
    let mut needles = Vec::new();
    for f in [
        "src/backend/assets/asset_catalog.rs",
        "src/backend/assets/asset_sync.rs",
    ] {
        let src = std::fs::read_to_string(root.join(f)).unwrap();
        // 取「`pub` [`async`] `fn` <名字>」里的名字：按词走，不在语料上做带字面量的前缀匹配（needle 棘轮）。
        let words: Vec<&str> = src.split_whitespace().collect();
        for w in words.windows(3) {
            let (lead, kw, tail) = (w[0], w[1], w[2]);
            if kw == "fn" && (lead == "pub" || lead == "async") {
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

/// 跨半边：可达表登记那一条 —— 本侧发的三格 ＋ 读回的那一格 == 后端 `REGISTRY` 里 `remote-reach` 声明的 `fields`（两向），
/// 且那条命令真在后端命令表里；发的入参就是 `assets-sync` 那一份（同一个 `args_for`，逐键相等）。
///
/// 守的要求：「可达表 origin → {dial, backend_path, 对面 id} 由 monitor 在（`backend_path` 那一格随 `backendPath` 删了，落点是固定常量）
/// 远端流握手成功那一刻交给本机后端」—— C4d 让它对每台远端都成立（不只认资产目录的那几台）。
#[test]
fn the_reach_registration_sends_what_the_backend_registers() {
    let families = crate::guard_support::backend_registry_sources();
    let (family, at) = families
        .iter()
        .find_map(|(_, prod)| {
            prod.find(&format!("name: \"{REACH_CMD}\""))
                .map(|at| (prod, at))
        })
        .unwrap_or_else(|| panic!("后端 REGISTRY 里没有 `{REACH_CMD}`"));
    let block = &family[at..at + family[at..].find("run:").expect("那一条的 run")];
    let declared = declared_fields(block);
    let args = args_for(&cfg()).unwrap();
    let mut used: std::collections::BTreeSet<String> =
        args.as_object().unwrap().keys().cloned().collect();
    used.insert("reach".to_string());
    assert_eq!(
        declared, used,
        "本侧发 / 读的字段与后端 `remote-reach` 声明的不相等（两向）"
    );
    let registered: std::collections::BTreeSet<&str> = families
        .iter()
        .flat_map(|(_, prod)| {
            prod.split("name: \"")
                .skip(1)
                .filter_map(|t| t.split('"').next())
        })
        .collect();
    assert!(
        registered.contains(REACH_CMD),
        "后端命令表里没有 `{REACH_CMD}`"
    );
}
