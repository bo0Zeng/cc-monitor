//! 要求：两台之间「装」那一件不经前端中继 —— 界面只问本机一次，本机常驻后端当枢纽向来源那台取、交被写那台判与写；
//! skill 与 MCP 同一对命令（`ext-hub-preview` / `ext-hub-apply`），看过之后任一头变了 ⇒ `stale`、一个字节不写。
use super::*;
use crate::stream::remote_ask::{register, Remote, Table};
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
        &json!({ "origin": origin, "dial": {"machine": {"host": "h", "port": 22, "user": "u", "keyPath": "/k"}} }),
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

fn mcp_args(from: Value, to: Value) -> Value {
    json!({ "kind": "mcp", "name": "x", "from": from, "to": to,
            "scope": { "from": { "level": "project", "dir": "/r" }, "to": { "level": "project", "dir": "/l" } } })
}

/// ★ MCP：来源是远端那台 —— 来源那一跳经 capture 跑那台 CLI 面的 `--mcp-sync-source --stdin-line`（参数一行进 stdin），
/// 被写那一跳问这台自己，交过去的是来源那台给的空位定义；卡上带两头的记号。
#[tokio::test]
async fn the_hub_takes_from_the_source_and_asks_the_target_itself() {
    let preview = json!({ "path": "/l/.mcp.json", "state": "new", "def": { "command": "c", "env": { "K": null } },
                          "slots": [{ "field": "env", "key": "K", "kept": false }], "suspects": [], "target": null });
    let (h, dyn_h) = here(&[("mcp-sync-preview", preview)]);
    let t = table_with("devbox");
    let r = remote(&[
        json!({ "path": "/r/.mcp.json", "def": { "command": "c", "env": { "K": null } },
                             "slots": [{ "field": "env", "key": "K" }], "token": "t-src" }),
    ]);
    let card = ext_preview(&dyn_h, &mcp_args(json!("devbox"), Value::Null), &t, &r)
        .await
        .expect("枢纽没答");
    assert_eq!(card["tokens"], json!({ "source": "t-src", "target": null }));
    assert_eq!(card["writes"], json!(["/l/.mcp.json"]));
    assert_eq!(
        card["slots"],
        json!([{ "field": "env", "key": "K", "kept": false }])
    );
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
        json!({ "name": "x", "at": { "level": "project", "dir": "/r" } })
    );
    let asked = h.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, "mcp-sync-preview");
    assert_eq!(
        asked[0].1["def"],
        json!({ "command": "c", "env": { "K": null } })
    );
}

/// ★ 写：两头再看一次 —— 来源的记号与卡上不同 ⇒ `stale`、被写那台一次都没被叫去写；相同 ⇒ 交被写那台写（带用户填的值）。
#[tokio::test]
async fn the_hub_refuses_to_write_when_the_source_moved_since_the_preview() {
    let preview = json!({ "path": "/l/.mcp.json", "state": "new", "def": { "command": "c" }, "slots": [], "suspects": [], "target": "t-dst" });
    let (h, dyn_h) = here(&[
        (
            "mcp-sync-source",
            json!({ "path": "/a/.mcp.json", "def": { "command": "c" }, "slots": [], "token": "NEW" }),
        ),
        ("mcp-sync-preview", preview),
        (
            "mcp-sync-apply",
            json!({ "path": "/l/.mcp.json", "written": true, "recordFailed": null }),
        ),
    ]);
    let t = table_with("devbox");
    let r = remote(&[]);
    let mut args = mcp_args(Value::Null, Value::Null);
    args["scope"]["from"]["dir"] = json!("/a");
    args["tokens"] = json!({ "source": "OLD", "target": "t-dst" });
    let (code, _) = ext_apply(&dyn_h, &args, &t, &r)
        .await
        .expect_err("来源变了还写了");
    assert_eq!(code, "stale");
    assert!(
        h.asked
            .lock()
            .unwrap()
            .iter()
            .all(|(c, _)| c != "mcp-sync-apply"),
        "来源变了，被写那台却被叫去写了"
    );
    args["tokens"]["source"] = json!("NEW");
    args["fill"] = json!({ "env": { "K": "typed" } });
    let done = ext_apply(&dyn_h, &args, &t, &r).await.expect("写");
    assert_eq!(
        done,
        json!({ "path": "/l/.mcp.json", "changed": ["x"], "note": null })
    );
    let asked = h.asked.lock().unwrap().clone();
    let apply = asked.iter().find(|(c, _)| c == "mcp-sync-apply").unwrap();
    assert_eq!(apply.1["fill"], json!({ "env": { "K": "typed" } }));
    assert_eq!(apply.1["target"], "t-dst");
}

/// 反向：可达表里没有那一台 ⇒ `unreachable`、说出是哪台，一次都不拨。
#[tokio::test]
async fn an_unregistered_machine_is_unreachable_and_never_dialed() {
    let (_h, dyn_h) = here(&[]);
    let t: Table = Mutex::new(BTreeMap::new());
    let r = remote(&[]);
    let (code, said) = ext_preview(&dyn_h, &mcp_args(json!("ghost"), Value::Null), &t, &r)
        .await
        .expect_err("没登记的那台也答了");
    assert_eq!(code, "unreachable");
    assert!(said.contains("ghost"), "{said}");
    assert!(r.calls.lock().unwrap().is_empty());
}

/// ★ skill：同一对命令按种类分派到 `skill-read` → `skill-install-plan`；卡上写哪几个 = 新的 ＋ 不同的；来源变了 ⇒ `stale`；
/// 同一台同一处 ⇒ 拒。
#[tokio::test]
async fn skill_preview_and_apply_go_through_the_same_pair() {
    let files =
        json!([{ "path": "SKILL.md", "text": "a", "exec": false, "why": null, "bytes": 1 }]);
    let plan = json!({ "dir": "/s/demo", "target": [], "base": "/s", "prefix": "demo",
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }] });
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[("skill-install-plan", plan)]);
    let r = remote(&[
        json!({ "files": files }),
        json!({ "files": [{ "path": "SKILL.md", "text": "CHANGED", "exec": false }] }),
    ]);
    let args = json!({ "kind": "skill", "name": "demo", "from": "devbox", "to": null,
                       "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    let card = ext_preview(&dyn_h, &args, &t, &r).await.expect("看卡");
    assert_eq!(card["path"], "/s/demo");
    assert_eq!(card["writes"], json!(["SKILL.md"]));
    assert_eq!(card["stop"], Value::Null);
    let plan_ask = h.asked.lock().unwrap()[0].clone();
    assert_eq!(plan_ask.0, "skill-install-plan");
    assert_eq!(
        plan_ask.1["source"],
        json!([{ "path": "SKILL.md", "text": "a", "exec": false }])
    );
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let (code, _) = ext_apply(&dyn_h, &apply, &t, &r)
        .await
        .expect_err("来源变了还装了");
    assert_eq!(code, "stale");
    assert!(h
        .asked
        .lock()
        .unwrap()
        .iter()
        .all(|(c, _)| c != "skill-install-apply"));
    let mut same = args.clone();
    same["from"] = Value::Null;
    let (code, _) = ext_preview(&dyn_h, &same, &t, &r)
        .await
        .expect_err("同一台同一处也看卡了");
    assert_eq!(code, "refused");
}

/// 远端答「按码说」：被写那台 CLI 信封答 `stale` ⇒ 枢纽回的也是 `stale`（别压成 `refused`）。
struct CodedRemote;
impl Remote for CodedRemote {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async { Err("不该走到这里".to_string()) })
    }
    fn run_coded<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, crate::stream::remote_ask::Said>> + Send + 'a>>
    {
        Box::pin(async {
            Err(crate::stream::remote_ask::Said {
                code: Some("stale".to_string()),
                message: "盘上那份在看过之后被改过".to_string(),
            })
        })
    }
}

#[tokio::test]
async fn the_hub_passes_the_remote_code_through() {
    let (_h, dyn_h) = here(&[(
        "mcp-sync-source",
        json!({ "path": "/a/.mcp.json", "def": {}, "slots": [], "token": "S" }),
    )]);
    let t = table_with("devbox");
    let (code, said) = ext_preview(
        &dyn_h,
        &mcp_args(Value::Null, json!("devbox")),
        &t,
        &CodedRemote,
    )
    .await
    .expect_err("被写那台答了 stale");
    assert_eq!(code, "stale", "{said}");
    assert!(said.contains("被改过"));
}

/// 枢纽这台按 Windows 那一形判它自己的路径（内层命令收到的 `project` 先过一遍 Windows 形的判定，再答定好的成品）。
struct WindowsHere {
    answers: BTreeMap<&'static str, Value>,
}

impl Here for WindowsHere {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)> {
        if let Some(p) = args["project"].as_str() {
            crate::assets::mcp_edit::project_root_as(p, crate::assets::mcp_edit::PathForm::Windows)
                .map_err(|(c, m)| (c.to_string(), m))?;
        }
        self.answers
            .get(cmd)
            .cloned()
            .ok_or_else(|| ("unknown_command".to_string(), cmd.to_string()))
    }
}

/// 被写那台是 POSIX 那一形：自己判收到的 `project`，判不过 ⇒ 按码答错；记下每次被问的参数。
struct PosixRemote {
    plan: Value,
    asked: Mutex<Vec<Value>>,
}

impl Remote for PosixRemote {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async { Err("不该走到这里".to_string()) })
    }
    fn run_coded<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, crate::stream::remote_ask::Said>> + Send + 'a>>
    {
        let args: Value = serde_json::from_str(stdin.unwrap_or_default().trim()).unwrap();
        self.asked.lock().unwrap().push(args.clone());
        let judged = crate::assets::mcp_edit::project_root_as(
            args["project"].as_str().unwrap_or_default(),
            crate::assets::mcp_edit::PathForm::Posix,
        );
        let out = match judged {
            Ok(_) => Ok(format!("{}\n", self.plan)),
            Err((code, message)) => Err(crate::stream::remote_ask::Said {
                code: Some(code.to_string()),
                message,
            }),
        };
        Box::pin(async move { out })
    }
}

/// ★ 项目目录由它所属的那台判：枢纽这台是 Windows 那一形，被写那台的 `/home/user` 原样交过去（那台判过、给出卡）；
/// 被写那台收到相对路径 ⇒ 那台自己拒、码与原话交回；正控：同一个 `/home/user` 落在枢纽自己这台 ⇒ 这台按 Windows 那一形拒。
#[tokio::test]
async fn a_project_dir_is_judged_by_the_machine_it_belongs_to() {
    use crate::assets::mcp_edit::{project_root_as, PathForm};
    let files =
        json!([{ "path": "SKILL.md", "text": "a", "exec": false, "why": null, "bytes": 1 }]);
    let plan = json!({ "dir": "/home/user/.claude/skills/demo", "target": [], "base": "/home/user/.claude/skills", "prefix": "demo",
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }] });
    let here: Arc<dyn Here> = Arc::new(WindowsHere {
        answers: [("skill-read", json!({ "files": files }))]
            .into_iter()
            .collect(),
    });
    let t = table_with("laptop");
    let laptop = PosixRemote {
        plan,
        asked: Mutex::new(Vec::new()),
    };
    let args = |to: Value, dir: &str| {
        json!({ "kind": "skill", "name": "demo", "from": null, "to": to,
                "scope": { "from": { "level": "project", "dir": "C:\\w" }, "to": { "level": "project", "dir": dir } } })
    };

    let card = ext_preview(&here, &args(json!("laptop"), "/home/user"), &t, &laptop)
        .await
        .expect("枢纽把被写那台的项目目录拒了");
    assert_eq!(card["path"], "/home/user/.claude/skills/demo");
    assert_eq!(laptop.asked.lock().unwrap()[0]["project"], "/home/user");

    let (code, said) = ext_preview(&here, &args(json!("laptop"), "w/x"), &t, &laptop)
        .await
        .expect_err("被写那台收下了相对路径");
    let want = project_root_as("w/x", PathForm::Posix).unwrap_err();
    assert_eq!((code.as_str(), said), (want.0, want.1));
    assert_eq!(
        laptop.asked.lock().unwrap().len(),
        2,
        "相对路径该由被写那台判（它要被问到）"
    );

    let (code, _) = ext_preview(&here, &args(Value::Null, "/home/user"), &t, &laptop)
        .await
        .expect_err("正控：Windows 那一形的这台收下了 /home/user");
    assert_eq!(code, "bad_path");
}
