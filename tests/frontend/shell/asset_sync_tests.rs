//! monitor 侧 `asset_sync.rs` 的判据：交出去的是什么、答回来的认不认得、判定没长第二个家。
//!
//! 守的要求：「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」·
//! （一切判定都在后端）· 观测方沿它本来就拥有的那条连接去拉（拉的是本机常驻后端，不是 monitor）。
//!
//! 买到：握手那一刻只交那台的名字（`origin`；怎么够到它照本机后端的机器表）；发的字段 == 后端那一条的入参类型（跨半边，读生成的类型声明）；monitor 这一侧零合并规则。
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

fn good_reply() -> Value {
    json!({
        "self": "9f",
        "synced": [{"origin": "dev", "peer": "4c", "changed": true, "pushed": 1, "error": null}],
        "reach": [{"origin": "dev", "machine": "4c"}],
    })
}

#[tokio::test]
async fn the_handshake_hands_over_only_the_machine_name() {
    let rec = Recorder {
        sent: Mutex::new(vec![]),
        reply: good_reply(),
    };
    // 应答原样交回（monitor 不解释它，界面那一问经通道直问、`src/frontend/ui/assets-sync-reads.ts` 按形状收）。
    let got = sync_with("dev", &rec).await.expect("握手那一刻");
    assert_eq!(got, good_reply());
    let sent = rec.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    // 只交名字：拨号那一格不交（怎么够到那台由本机后端照它的机器表组）。
    assert_eq!(sent[0], json!({"origin": "dev"}));
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
    let sent: std::collections::BTreeSet<String> = args_for("dev")
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
