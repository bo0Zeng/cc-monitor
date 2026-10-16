//! monitor 侧 `asset_sync.rs` 的判据：交出去的是什么、答回来的认不认得、判定没长第二个家。
//!
//! 守的要求：用户原话「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」·
//! （一切判定都在后端）· 观测方沿它本来就拥有的那条连接去拉（拉的是本机常驻后端，不是 monitor）。
//!
//! 买到：握手那一刻交的是「拨号请求（`capture`）＋ origin」；发的字段 == 后端那一条的入参类型（跨半边，读生成的类型声明）；monitor 这一侧零合并规则。
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

/// 跨半边：本侧发的格 == 后端 `assets-sync` 入参类型的格，回包类型的格 == 本侧认的那三格（两向）；远端要认的那条命令真在后端命令表里。
/// 两条都是 typed（命令表那一行点类型名、不写字面名）⇒ 读生成的类型声明与编进来的形状指纹表。
#[test]
fn what_monitor_sends_and_reads_is_what_the_backend_registers() {
    let gen = |f: &str| {
        std::fs::read_to_string(guard_core::addr::repo_root().join(f)).expect("生成的类型声明")
    };
    assert!(
        gen("src/frontend/ui/generated/TypedCommands.ts")
            .lines()
            .any(|l| l.trim().starts_with(&format!(
                "\"{CMD}\": {{ req: AssetsSyncReq; reply: AssetsSynced;"
            ))),
        "后端命令表里没有 `{CMD}`（或它的入参 · 回包类型换了名）"
    );
    let req = decl_keys(&gen("src/frontend/ui/generated/AssetsSyncReq.ts"));
    let reply = decl_keys(&gen("src/frontend/ui/generated/AssetsSynced.ts"));
    let sent: std::collections::BTreeSet<String> = args_for(&cfg())
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        sent, req,
        "本侧发的格与后端 `{CMD}` 入参类型的格不相等（两向）"
    );
    let read: std::collections::BTreeSet<String> = ["self", "synced", "reach"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(reply, read, "后端 `{CMD}` 回包的格与本侧认的不相等（两向）");
    assert!(
        cmd_wire::shapes::shape_of(CMD).is_some(),
        "形状指纹表里没有 `{CMD}`"
    );
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

/// 一份生成的 TS 声明（`{ 格: 类型, … }`）的顶层格名：逐字扫，跳过 `/* … */` 注释，只认对象那一层里「名字 ＋ 可缺的 `?` ＋ `:`」。
fn decl_keys(decl: &str) -> std::collections::BTreeSet<String> {
    let cs: Vec<char> = decl.chars().collect();
    let (mut i, mut depth, mut word, mut out) =
        (0, 0usize, String::new(), std::collections::BTreeSet::new());
    while i < cs.len() {
        let c = cs[i];
        if c == '/' && cs.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < cs.len() && !(cs[i] == '*' && cs[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '/' && cs.get(i + 1) == Some(&'/') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '{' | '<' => depth += 1,
            '}' | '>' => depth = depth.saturating_sub(1),
            ':' if depth == 1 && !word.is_empty() => {
                out.insert(word.trim_end_matches('?').to_string());
            }
            _ => {}
        }
        if c.is_alphanumeric() || c == '_' || c == '?' {
            word.push(c);
        } else {
            word.clear();
        }
        i += 1;
    }
    out
}

/// 跨半边：可达表登记那一条 —— 本侧发的格 == 后端 `remote-reach` 入参类型的格，读回的那一格（`reach`）在它回包类型里；
/// 发的入参就是 `assets-sync` 那一份（同一个 `args_for`，逐键相等）。
///
/// 守的要求：「可达表 origin → {dial, backend_path, 对面 id} 由 monitor 在（`backend_path` 那一格随 `backendPath` 删了，落点是固定常量）
/// 远端流握手成功那一刻交给本机后端」—— C4d 让它对每台远端都成立（不只认资产目录的那几台）。
#[test]
fn the_reach_registration_sends_what_the_backend_registers() {
    let gen = |f: &str| {
        std::fs::read_to_string(guard_core::addr::repo_root().join(f)).expect("生成的类型声明")
    };
    // 命令表那一行点的是这两个类型（`generated/TypedCommands.ts`；改了名这里跟着改，下面那条断言会先红）。
    assert!(
        gen("src/frontend/ui/generated/TypedCommands.ts")
            .lines()
            .any(|l| l.trim()
                == "\"remote-reach\": { req: ReachReq; reply: Reached; code: \"bad_args\" };"),
        "后端命令表里没有 `{REACH_CMD}`（或它的入参 · 回包类型换了名）"
    );
    let req = decl_keys(&gen("src/frontend/ui/generated/ReachReq.ts"));
    let reply = decl_keys(&gen("src/frontend/ui/generated/Reached.ts"));
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
    let declared = decl_keys(&gen("src/frontend/ui/generated/MachineDial.ts"));
    assert!(
        dial.is_subset(&declared),
        "本侧 `dial` 的格 {dial:?} 不全在后端 `MachineDial` 里 {declared:?}"
    );
}
