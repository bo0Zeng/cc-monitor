//! 〔AS2 · 第四波 4B〕monitor 侧 `asset_sync.rs` 的判据：交出去的是什么、答回来的认不认得、判定没长第二个家。
//!
//! 守的要求（住址）：用户裁决 **V113** 逐字「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」·
//! `设计/01 §1.1`（一切判定都在后端）· `§3.5`（观测方沿它本来就拥有的那条连接去拉 —— 拉的是本机常驻后端，不是 monitor）。
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

fn cfg() -> crate::ssh_source::RemoteConfig {
    serde_json::from_value(json!({
        "host": "10.0.0.2",
        "label": "dev",
        "port": 2222,
        "user": "u",
        "keyPath": "/home/me/.ssh/id_ed25519",
    }))
    .or_else(|_| {
        serde_json::from_value(json!({
            "host": "10.0.0.2",
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
    // 〔MIG-3a〕应答原样交回（monitor 不解释它，界面那一问经通道直问、`src/assets-sync-reads.ts` 按形状收）。
    let got = sync_with(&cfg(), &rec).await.expect("握手那一刻");
    assert_eq!(got, good_reply());
    let sent = rec.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["origin"], json!("dev"));
    // 〔E2〕那台后端的路径不交（落点是固定常量）。
    assert_eq!(sent[0].get("backend"), None);
    assert_eq!(sent[0]["dial"]["use"], json!("capture"));
    assert_eq!(sent[0]["dial"]["machine"]["host"], json!("10.0.0.2"));
    assert_eq!(
        sent[0]["dial"]["machine"]["keyPath"],
        json!("/home/me/.ssh/id_ed25519"),
        "只交路径"
    );
}

// 〔MIG-3a〕「应答缺格就报错不猜」那一条随 `parse_reply`〔散文墓碑〕挪到界面：`tests/assets-sync-reads.vitest.ts`（金样 ＋ 逐格坏样）。

/// 跨半边：本侧发的命令名与字段 ⊆ 后端 `REGISTRY` 那一条声明的 `fields`，读的两格也在里面；远端要认的那条命令真在后端命令表里。
#[test]
fn what_monitor_sends_and_reads_is_what_the_backend_registers() {
    let inbound = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/stream/inbound.rs"),
    )
    .expect("读后端 inbound.rs");
    let at = inbound
        .find(&format!("name: \"{CMD}\""))
        .unwrap_or_else(|| panic!("后端 REGISTRY 里没有 `{CMD}`"));
    let block = &inbound[at..at + inbound[at..].find("run:").expect("那一条的 run")];
    let fields_line = block
        .lines()
        .find(|l| l.trim_start().starts_with("fields:"))
        .expect("fields 那一行");
    let declared: std::collections::BTreeSet<String> = fields_line
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    let used: std::collections::BTreeSet<String> = ["origin", "dial", "self", "synced", "reach"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        declared, used,
        "本侧发 / 读的字段与后端声明的不相等（两向）"
    );
    // 远端要认的那条在后端命令镜子里
    let at = guard_core::find_pinned(&inbound, "pub const COMMANDS").expect("后端命令镜子");
    let commands_block: String = inbound[at..]
        .lines()
        .take_while(|l| l.trim() != "];")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(commands_block.contains(&format!("\"{REMOTE_NEEDS}\"")));
    assert!(commands_block.contains(&format!("\"{CMD}\"")));
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

/// 〔C4d · 第四波 4B〕跨半边：可达表登记那一条 —— 本侧发的三格 ＋ 读回的那一格 == 后端 `REGISTRY` 里 `remote-reach` 声明的 `fields`（两向），
/// 且那条命令真在后端命令镜子里；发的入参就是 `assets-sync` 那一份（同一个 `args_for`，逐键相等）。
///
/// 守的要求：主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 1 条）「可达表 origin → {dial, backend_path, 对面 id} 由 monitor 在（〔E2〕`backend_path` 那一格随 `backendPath` 删了，落点是固定常量）
/// 远端流握手成功那一刻交给本机后端」—— C4d 让它对每台远端都成立（不只认资产目录的那几台）。
#[test]
fn the_reach_registration_sends_what_the_backend_registers() {
    let inbound = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/stream/inbound.rs"),
    )
    .expect("读后端 inbound.rs");
    let at = inbound
        .find(&format!("name: \"{REACH_CMD}\""))
        .unwrap_or_else(|| panic!("后端 REGISTRY 里没有 `{REACH_CMD}`"));
    let block = &inbound[at..at + inbound[at..].find("run:").expect("那一条的 run")];
    let fields_line = block
        .lines()
        .find(|l| l.trim_start().starts_with("fields:"))
        .expect("fields 那一行");
    let declared: std::collections::BTreeSet<String> = fields_line
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    let args = args_for(&cfg()).unwrap();
    let mut used: std::collections::BTreeSet<String> =
        args.as_object().unwrap().keys().cloned().collect();
    used.insert("reach".to_string());
    assert_eq!(
        declared, used,
        "本侧发 / 读的字段与后端 `remote-reach` 声明的不相等（两向）"
    );
    let at = guard_core::find_pinned(&inbound, "pub const COMMANDS").expect("后端命令镜子");
    let commands_block: String = inbound[at..]
        .lines()
        .take_while(|l| l.trim() != "];")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(commands_block.contains(&format!("\"{REACH_CMD}\"")));
}
