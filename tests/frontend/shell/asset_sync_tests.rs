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

/// 一条 typed 帧命令入参 · 回包的顶层格名（从后端类型生成的 TS 声明里读：`generated/TypedCommands.ts` 那一行点名两个类型，
/// 各自的 `generated/<类型>.ts` 是 `{ 格: 类型, … }`）。命令表里没有这一条 ⇒ `None`。
fn typed_fields(
    cmd: &str,
) -> Option<(
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
)> {
    let gen = crate::guard_support::repo_root().join("src/frontend/ui/generated");
    let map = std::fs::read_to_string(gen.join("TypedCommands.ts")).expect("生成的命令表");
    let line = map
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("\"{cmd}\":")))?;
    let ty = |k: &str| -> String {
        let at = line.find(&format!("{k}: ")).expect("那一行的类型") + k.len() + 2;
        line[at..].split(';').next().unwrap().trim().to_string()
    };
    let keys = |name: &str| -> std::collections::BTreeSet<String> {
        let decl = std::fs::read_to_string(gen.join(format!("{name}.ts"))).expect("生成的类型声明");
        let mut text = String::new();
        let mut rest = decl.as_str();
        while let Some(i) = rest.find("/*") {
            text.push_str(&rest[..i]);
            rest = &rest[i + rest[i..].find("*/").expect("注释没收尾") + 2..];
        }
        text.push_str(rest);
        let body = &text[text.find("= {").expect("对象类型") + 3..text.rfind('}').unwrap()];
        body.split(',')
            .filter_map(|f| f.split(':').next())
            .map(|k| k.trim().trim_end_matches('?').to_string())
            .filter(|k| !k.is_empty())
            .collect()
    };
    Some((keys(&ty("req")), keys(&ty("reply"))))
}

/// 跨半边：可达表登记那一条 —— 本侧发的格 == 后端 `remote-reach` 入参类型的格，读回的那一格（`reach`）在它回包类型里；
/// 发的入参就是 `assets-sync` 那一份（同一个 `args_for`，逐键相等）。
///
/// 守的要求：「可达表 origin → {dial, backend_path, 对面 id} 由 monitor 在（`backend_path` 那一格随 `backendPath` 删了，落点是固定常量）
/// 远端流握手成功那一刻交给本机后端」—— C4d 让它对每台远端都成立（不只认资产目录的那几台）。
#[test]
fn the_reach_registration_sends_what_the_backend_registers() {
    let (req, reply) =
        typed_fields(REACH_CMD).unwrap_or_else(|| panic!("后端命令表里没有 `{REACH_CMD}`"));
    let args = args_for(&cfg()).unwrap();
    let sent: std::collections::BTreeSet<String> =
        args.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        sent, req,
        "本侧发的格与后端 `remote-reach` 入参类型的格不相等（两向）"
    );
    assert!(
        reply.contains("reach"),
        "后端 `remote-reach` 回包里没有本侧读的 `reach`：{reply:?}"
    );
    // 拨号那一格的每一格也都在后端那一份类型里（多一格后端就回 bad_args）。
    let dial: std::collections::BTreeSet<String> =
        args["dial"].as_object().unwrap().keys().cloned().collect();
    let gen = crate::guard_support::repo_root().join("src/frontend/ui/generated/ReachDial.ts");
    let decl = std::fs::read_to_string(gen).expect("ReachDial 的生成声明");
    for k in &dial {
        assert!(
            decl.contains(&format!(" {k}:"))
                || decl.contains(&format!(" {k}?:"))
                || decl.contains(&format!("\n{k}:"))
                || decl.contains(&format!("\n{k}?:")),
            "本侧 `dial` 里的 `{k}` 不在后端 `ReachDial` 里"
        );
    }
}
