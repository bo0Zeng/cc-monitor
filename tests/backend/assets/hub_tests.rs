//! 设计/01 §3.5：「观测方沿它本来就拥有的那条连接去拉被观测方」·「不是经前端中继」（主会话 09-28 裁：两台之间那几件，
//! 界面只问本机一次，本机常驻后端当枢纽向来源那台取、向被写那台写；被写那台照旧自己判 CAS、`stale` 就停）。
use super::*;
use crate::remote_ask::{register, Remote, Table};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

/// 这台自己：按命令名答一份定好的成品，记下被问了什么。
struct FakeHere {
    answers: BTreeMap<&'static str, Value>,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Here for FakeHere {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)> {
        self.asked.lock().unwrap().push((cmd.to_string(), args));
        self.answers
            .get(cmd)
            .cloned()
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))
    }
}

/// 远端那台：记下命令行与 stdin，答一份定好的 stdout。
struct FakeRemote {
    stdout: Mutex<Vec<String>>,
    calls: Mutex<Vec<(String, Option<String>)>>,
}

impl Remote for FakeRemote {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        self.calls.lock().unwrap().push((command, stdin));
        let out = self.stdout.lock().unwrap().remove(0);
        Box::pin(async move { Ok(out) })
    }
}

fn here(answers: &[(&'static str, Value)]) -> (Arc<FakeHere>, Arc<dyn Here>) {
    let h = Arc::new(FakeHere {
        answers: answers.iter().cloned().collect(),
        asked: Mutex::new(Vec::new()),
    });
    let dyn_h: Arc<dyn Here> = h.clone();
    (h, dyn_h)
}

fn table_with(origin: &str) -> Table {
    let t: Table = Mutex::new(BTreeMap::new());
    register(
        &t,
        &json!({ "origin": origin, "dial": {"host": "h", "port": 22, "user": "u", "key_path": "/k"} }),
    )
    .unwrap();
    t
}

fn remote(outs: &[Value]) -> FakeRemote {
    FakeRemote {
        stdout: Mutex::new(outs.iter().map(|v| format!("{v}\n")).collect()),
        calls: Mutex::new(Vec::new()),
    }
}

/// ★ 推：来源是远端那台、被写的是这台 —— 来源那一跳经 capture 跑那台 CLI 面的 `--mcp-sync-source --stdin-line`（参数一行进 stdin），
/// 被写那一跳问这台自己；来源原文**由枢纽交给被写那台**（不经界面），被写那台的成品原样交回。
#[tokio::test]
async fn the_hub_takes_from_the_source_and_asks_the_target_itself() {
    let preview = json!({ "sourcePath": "/r/.mcp.json", "targetPath": "/l/.mcp.json", "sourceText": "S", "targetText": null, "rows": [] });
    let (h, dyn_h) = here(&[("mcp-sync-preview", preview.clone())]);
    let t = table_with("devbox");
    let r = remote(&[json!({ "path": "/r/.mcp.json", "text": "S" })]);
    let got = mcp_preview(
        &dyn_h,
        &json!({ "from": "devbox", "fromDir": "/r", "to": null, "toDir": "/l" }),
        &t,
        &r,
    )
    .await
    .expect("枢纽没答");
    assert_eq!(got, preview);
    let calls = r.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "来源那台只问一次：{calls:?}");
    assert!(
        calls[0]
            .0
            .ends_with(" -- '--mcp-sync-source' '--stdin-line'"),
        "{calls:?}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(calls[0].1.as_deref().unwrap().trim()).unwrap(),
        json!({ "projectDir": "/r" })
    );
    let asked = h.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, "mcp-sync-preview");
    assert_eq!(asked[0].1["source"], "S");
    assert_eq!(asked[0].1["sameMachine"], false);
}

/// ★ 写：来源那份再取一次 —— 与看差异时那份不同 ⇒ `stale`、被写那台一次都没被问；相同 ⇒ 写的是枢纽自己取来的那份。
#[tokio::test]
async fn the_hub_refuses_to_write_when_the_source_moved_since_the_preview() {
    let applied = json!({ "path": "/l/.mcp.json", "written": true, "names": ["x"] });
    let (h, dyn_h) = here(&[
        (
            "mcp-sync-source",
            json!({ "path": "/a/.mcp.json", "text": "NEW" }),
        ),
        ("mcp-sync-apply", applied.clone()),
    ]);
    let t = table_with("devbox");
    let r = remote(&[]);
    let args = |expect: &str| {
        json!({ "from": null, "fromDir": "/a", "to": null, "toDir": "/b",
                "expectSource": expect, "target": null, "take": ["x"], "overwrite": [] })
    };
    let (code, _) = mcp_apply(&dyn_h, &args("OLD"), &t, &r)
        .await
        .expect_err("来源变了还写了");
    assert_eq!(code, "stale");
    assert!(
        h.asked
            .lock()
            .unwrap()
            .iter()
            .all(|(c, _)| c != "mcp-sync-apply"),
        "来源变了，被写那台却被问了"
    );
    let got = mcp_apply(&dyn_h, &args("NEW"), &t, &r).await.expect("写");
    assert_eq!(got, applied);
    let asked = h.asked.lock().unwrap().clone();
    let apply = asked.iter().find(|(c, _)| c == "mcp-sync-apply").unwrap();
    assert_eq!(apply.1["source"], "NEW");
}

/// 反向：可达表里没有那一台 ⇒ `unreachable`、说出是哪台，一次都不拨。
#[tokio::test]
async fn an_unregistered_machine_is_unreachable_and_never_dialed() {
    let (_h, dyn_h) = here(&[]);
    let t: Table = Mutex::new(BTreeMap::new());
    let r = remote(&[]);
    let (code, said) = mcp_preview(
        &dyn_h,
        &json!({ "from": "ghost", "fromDir": "/r", "to": null, "toDir": "/l" }),
        &t,
        &r,
    )
    .await
    .expect_err("没登记的那台也答了");
    assert_eq!(code, "unreachable");
    assert!(said.contains("ghost"), "{said}");
    assert!(r.calls.lock().unwrap().is_empty());
}

/// ★ skill：看差异 = 来源那台读 ＋ 被写那台判，一趟交回 `{dir, rows, target, source}`；写之前来源再读一次，变了 ⇒ `stale`。
#[tokio::test]
async fn skill_preview_and_apply_go_through_the_hub() {
    let files =
        json!([{ "path": "SKILL.md", "text": "a", "exec": false, "why": null, "bytes": 1 }]);
    let plan =
        json!({ "dir": "/s/demo", "rows": [], "target": [], "base": "/s", "prefix": "demo" });
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[
        ("skill-install-plan", plan),
        (
            "skill-install-apply",
            json!({ "dir": "/s/demo", "written": ["SKILL.md"], "chmodFailed": [], "recordFailed": null }),
        ),
    ]);
    let r = remote(&[json!({ "files": files }), json!({ "files": files })]);
    let p = skill_preview(
        &dyn_h,
        &json!({ "from": "devbox", "to": null, "name": "demo" }),
        &t,
        &r,
    )
    .await
    .expect("看差异");
    assert_eq!(p["dir"], "/s/demo");
    assert_eq!(p["source"], files);
    let plan_ask = h.asked.lock().unwrap()[0].clone();
    assert_eq!(plan_ask.0, "skill-install-plan");
    assert_eq!(
        plan_ask.1["source"],
        json!([{ "path": "SKILL.md", "text": "a", "exec": false }])
    );
    let (code, _) = skill_apply(
        &dyn_h,
        &json!({ "from": "devbox", "to": null, "name": "demo",
                 "expectSource": [{ "path": "SKILL.md", "text": "OLD", "exec": false, "why": null }],
                 "target": [], "take": ["SKILL.md"], "overwrite": [] }),
        &t,
        &r,
    )
    .await
    .expect_err("来源变了还装了");
    assert_eq!(code, "stale");
    let (same, _) = skill_preview(
        &dyn_h,
        &json!({ "from": null, "to": null, "name": "demo" }),
        &t,
        &r,
    )
    .await
    .expect_err("同一台也看差异了");
    assert_eq!(same, "refused");
}
