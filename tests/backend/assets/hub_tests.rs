//! 要求：两台之间「装」那一件不经前端中继 —— 界面只问本机一次，本机常驻后端当枢纽向来源那台取、交被写那台判与写；
//! skill 与 MCP 同一对命令（`ext-hub-preview` / `ext-hub-apply`），看过之后任一头变了 ⇒ `stale`、一个字节不写。
use super::*;
use crate::assets::ext::ExtSlot;
use crate::dial::remote_ask::{Remote, Table};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

/// 判据仍用 JSON 写一台的问法：看卡 ⇒ 卡的线上形。
pub(crate) async fn pv(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Result<Value, crate::common::said::Said> {
    ext_preview(crate::agents::REGISTRY, here, args, table, remote)
        .await
        .map(|c| serde_json::to_value(c).unwrap())
}

/// 同 [`pv`]：装一台，记号与填的值从问法里的 `tokens` · `fill` 取（界面那一侧交回的就是这两格）。
pub(crate) async fn ap(
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Result<Value, crate::common::said::Said> {
    let tokens: ExtTokens =
        serde_json::from_value(args["tokens"].clone()).expect("判据给的 tokens");
    let fill: Option<Fill> =
        serde_json::from_value(args.get("fill").cloned().unwrap_or(Value::Null))
            .expect("判据给的 fill");
    ext_apply(
        crate::agents::REGISTRY,
        here,
        args,
        &tokens,
        fill.as_ref(),
        table,
        remote,
    )
    .await
    .map(|d| serde_json::to_value(d).unwrap())
}

/// 这台自己：按命令名答一份定好的成品，记下被问了什么。
struct FakeHere {
    answers: BTreeMap<&'static str, Value>,
    asked: Mutex<Vec<(String, Value)>>,
    tz: crate::common::time::Tz,
}

impl Here for FakeHere {
    fn viewer_tz(&self) -> crate::common::time::Tz {
        self.tz.clone()
    }

    fn ask(&self, cmd: &str, args: Value) -> Result<Value, crate::common::said::Said> {
        self.asked.lock().unwrap().push((cmd.to_string(), args));
        self.answers
            .get(cmd)
            .cloned()
            .ok_or_else(|| crate::common::said::Said::new("unknown_command", cmd.to_string()))
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
        tz: Default::default(),
    });
    let dyn_h: Arc<dyn Here> = h.clone();
    (h, dyn_h)
}

fn table_with(origin: &str) -> Table {
    Table::of(vec![
        <crate::dial::Host as serde::Deserialize>::deserialize(
            &json!({"label": origin, "host": "h", "port": 22, "user": "u", "keyPath": "/k"}),
        )
        .unwrap(),
    ])
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
    let card = pv(&dyn_h, &mcp_args(json!("devbox"), Value::Null), &t, &r)
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
            .ends_with(" -- '--mcp-sync-source' '--stdin-line' '--tz' 'UTC'"),
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

/// 枢纽代问别台时把看的人那一台的时区（请求信封的 `tz`）转过去：那台回的「几点」（失败详情里的时刻也是）按它写，不落成 UTC。
/// 没带时区 ⇒ 照信封的缺省（UTC）明说。
#[tokio::test]
async fn the_hub_hands_the_viewers_time_zone_to_the_machine_it_asks() {
    let src =
        json!({ "path": "/r/.mcp.json", "def": { "command": "c" }, "slots": [], "token": "t" });
    let preview = json!({ "path": "/l/.mcp.json", "state": "new", "def": { "command": "c" }, "slots": [], "suspects": [], "target": null });
    for (tz, tail) in [
        (
            crate::common::time::Tz::iana("Asia/Shanghai").unwrap(),
            " -- '--mcp-sync-source' '--stdin-line' '--tz' 'Asia/Shanghai'",
        ),
        (
            crate::common::time::Tz::default(),
            " -- '--mcp-sync-source' '--stdin-line' '--tz' 'UTC'",
        ),
    ] {
        let h: Arc<dyn Here> = Arc::new(FakeHere {
            answers: [("mcp-sync-preview", preview.clone())]
                .into_iter()
                .collect(),
            asked: Mutex::new(Vec::new()),
            tz,
        });
        let r = remote(std::slice::from_ref(&src));
        pv(
            &h,
            &mcp_args(json!("devbox"), Value::Null),
            &table_with("devbox"),
            &r,
        )
        .await
        .expect("枢纽没答");
        let calls = r.calls.lock().unwrap().clone();
        assert!(calls[0].0.ends_with(tail), "{calls:?}");
    }
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
    let e = ap(&dyn_h, &args, &t, &r).await.expect_err("来源变了还写了");
    assert_eq!(e.code.as_deref(), Some("stale"));
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
    let done = ap(&dyn_h, &args, &t, &r).await.expect("写");
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
    let t = crate::dial::remote_ask::test_peers::empty();
    let r = remote(&[]);
    let e = pv(&dyn_h, &mcp_args(json!("ghost"), Value::Null), &t, &r)
        .await
        .expect_err("没登记的那台也答了");
    assert_eq!(e.code.as_deref(), Some("unreachable"));
    assert!(e.said.contains("ghost"), "{}", e.said);
    assert!(r.calls.lock().unwrap().is_empty());
}

/// ★ skill：同一对命令按种类分派到 `skill-read` → `skill-install-plan`；卡上写哪几个 = 新的 ＋ 不同的；来源变了 ⇒ `stale`；
/// 同一台同一处 ⇒ 拒。
#[tokio::test]
async fn skill_preview_and_apply_go_through_the_same_pair() {
    let files =
        json!([{ "path": "SKILL.md", "text": "a", "exec": false, "why": null, "bytes": 1 }]);
    let plan = json!({ "root": "/s", "dir": "/s/demo", "target": [], "base": "/s", "prefix": "demo",
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }],
                       "write": null, "ledger": null });
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[("skill-install-plan", plan)]);
    let r = remote(&[
        json!({ "root": "/r", "dir": "/r/demo", "files": files, "skipped": [] }),
        json!({ "root": "/r", "dir": "/r/demo", "skipped": [],
                "files": [{ "path": "SKILL.md", "text": "CHANGED", "exec": false, "why": null, "bytes": 7 }] }),
    ]);
    let args = json!({ "kind": "skill", "name": "demo", "from": "devbox", "to": null,
                       "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    let card = pv(&dyn_h, &args, &t, &r).await.expect("看卡");
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
    let e = ap(&dyn_h, &apply, &t, &r)
        .await
        .expect_err("来源变了还装了");
    assert_eq!(e.code.as_deref(), Some("stale"));
    assert!(h
        .asked
        .lock()
        .unwrap()
        .iter()
        .all(|(c, _)| c != "skill-install-apply"));
    let mut same = args.clone();
    same["from"] = Value::Null;
    let e = pv(&dyn_h, &same, &t, &r)
        .await
        .expect_err("同一台同一处也看卡了");
    assert_eq!(e.code.as_deref(), Some("refused"));
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
    ) -> Pin<Box<dyn Future<Output = Result<String, crate::common::said::Said>> + Send + 'a>> {
        Box::pin(async {
            Err(crate::common::said::Said::new(
                "stale",
                "盘上那份在看过之后被改过".to_string(),
            ))
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
    let e = pv(
        &dyn_h,
        &mcp_args(Value::Null, json!("devbox")),
        &t,
        &CodedRemote,
    )
    .await
    .expect_err("被写那台答了 stale");
    assert_eq!(e.code.as_deref(), Some("stale"), "{}", e.said);
    assert!(e.said.contains("被改过"));
}

/// 枢纽这台按 Windows 那一形判它自己的路径（内层命令收到的 `project` 先过一遍 Windows 形的判定，再答定好的成品）。
struct WindowsHere {
    answers: BTreeMap<&'static str, Value>,
}

impl Here for WindowsHere {
    fn viewer_tz(&self) -> crate::common::time::Tz {
        Default::default()
    }
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, crate::common::said::Said> {
        if let Some(p) = args["project"].as_str() {
            crate::assets::mcp_edit::project_root_as(
                p,
                crate::assets::mcp_edit::PathForm::Windows,
            )?;
        }
        self.answers
            .get(cmd)
            .cloned()
            .ok_or_else(|| crate::common::said::Said::new("unknown_command", cmd.to_string()))
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
    ) -> Pin<Box<dyn Future<Output = Result<String, crate::common::said::Said>> + Send + 'a>> {
        let args: Value = serde_json::from_str(stdin.unwrap_or_default().trim()).unwrap();
        self.asked.lock().unwrap().push(args.clone());
        let judged = crate::assets::mcp_edit::project_root_as(
            args["project"].as_str().unwrap_or_default(),
            crate::assets::mcp_edit::PathForm::Posix,
        );
        let out = match judged {
            Ok(_) => Ok(format!("{}\n", self.plan)),
            Err(e) => Err(e),
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
    let plan = json!({ "root": "/home/user/.claude/skills", "dir": "/home/user/.claude/skills/demo", "target": [],
                       "base": "/home/user/.claude/skills", "prefix": "demo", "write": null, "ledger": null,
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }] });
    let here: Arc<dyn Here> = Arc::new(WindowsHere {
        answers: [(
            "skill-read",
            json!({ "root": "C:\\w\\.claude\\skills", "dir": "C:\\w\\.claude\\skills\\demo", "files": files, "skipped": [] }),
        )]
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

    let card = pv(&here, &args(json!("laptop"), "/home/user"), &t, &laptop)
        .await
        .expect("枢纽把被写那台的项目目录拒了");
    assert_eq!(card["path"], "/home/user/.claude/skills/demo");
    assert_eq!(laptop.asked.lock().unwrap()[0]["project"], "/home/user");

    let e = pv(&here, &args(json!("laptop"), "w/x"), &t, &laptop)
        .await
        .expect_err("被写那台收下了相对路径");
    let want = project_root_as("w/x", PathForm::Posix).unwrap_err();
    assert_eq!((e.code, e.said), (want.code, want.said));
    assert_eq!(
        laptop.asked.lock().unwrap().len(),
        2,
        "相对路径该由被写那台判（它要被问到）"
    );

    let e = pv(&here, &args(Value::Null, "/home/user"), &t, &laptop)
        .await
        .expect_err("正控：Windows 那一形的这台收下了 /home/user");
    assert_eq!(e.code.as_deref(), Some("bad_path"));
}

/// ★ 自己发版的插件（planned-build）不拒：照 skill 那条路，来源那台读整个目录、被写那台判、看卡；
/// 项目里不装（插件只有用户级那一处）。别的插件照旧拒（上一条）。
#[tokio::test]
async fn the_hub_installs_planned_build_like_a_skill_directory() {
    let files = json!([{ "path": ".claude-plugin/plugin.json", "text": "{}", "exec": false, "why": null, "bytes": 2 }]);
    let plan = json!({ "root": "/s", "dir": "/s/planned-build", "target": [], "base": "/s", "prefix": "planned-build",
                       "rows": [{ "path": ".claude-plugin/plugin.json", "state": "new", "suspects": [], "blocked": null }],
                       "write": null, "ledger": null });
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[("skill-install-plan", plan)]);
    let r = remote(&[
        json!({ "root": "/r", "dir": "/r/planned-build", "files": files, "skipped": [] }),
    ]);
    let args = json!({ "kind": "plugin", "name": "planned-build", "from": "devbox", "to": null,
                       "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    let card = pv(&dyn_h, &args, &t, &r)
        .await
        .expect("planned-build 给拒了");
    assert_eq!(card["kind"], "plugin");
    assert_eq!(card["path"], "/s/planned-build");
    assert_eq!(card["writes"], json!([".claude-plugin/plugin.json"]));
    assert_eq!(h.asked.lock().unwrap()[0].0, "skill-install-plan");
    let mut proj = args.clone();
    proj["scope"]["to"] = json!({ "level": "project", "dir": "/w" });
    let e = pv(&dyn_h, &proj, &t, &r).await.expect_err("插件装进了项目");
    assert_eq!(e.code.as_deref(), Some("refused"));
}

// ═══════════════════════════════════════════════════════════════════════════
// 一次装到几台：落点取交集 · 要填格并成一份 · 会写的文件逐台一行（判定都在枢纽）
// ═══════════════════════════════════════════════════════════════════════════

fn golden_list() -> ExtList {
    let g: Value =
        serde_json::from_str(include_str!("../../__fixtures__/ext-flow.golden.json")).unwrap();
    serde_json::from_value(g["list"].clone()).unwrap()
}

#[test]
fn many_places_are_the_intersection_and_the_place_is_ok_for_every_picked_machine() {
    let list = golden_list();
    // demo：本机那一格只能装到本机的项目（全局是来源同一处）；laptop 那一格全局与 laptop 的项目都行。
    let p = plan_many(
        crate::agents::REGISTRY,
        &list,
        None,
        ExtKind::Skill,
        "demo",
        None,
        &[Some("laptop".into())],
        None,
    )
    .unwrap();
    assert_eq!(
        p.places
            .iter()
            .map(|t| (t.at.clone(), t.ok))
            .collect::<Vec<_>>()[0],
        (ExtLoc::User, true)
    );
    assert_eq!(p.place, Some(ExtLoc::User), "建议的那一处：第一台的建议");
    let both = plan_many(
        crate::agents::REGISTRY,
        &list,
        None,
        ExtKind::Skill,
        "demo",
        None,
        &[None, Some("laptop".into())],
        None,
    )
    .unwrap();
    let user = both.places.iter().find(|t| t.at == ExtLoc::User).unwrap();
    assert!(!user.ok, "本机那台装不了全局 ⇒ 交集里全局不可选");
    assert!(user.note.is_some());
    assert!(both.places.iter().all(|t| !t.ok), "两台没有共同能装的一处");
    assert_eq!(both.place, None);
    // 要的那一处对哪台都不行 ⇒ 不照它，退回建议。
    let asked = plan_many(
        crate::agents::REGISTRY,
        &list,
        None,
        ExtKind::Skill,
        "demo",
        None,
        &[Some("laptop".into())],
        Some(ExtLoc::Project {
            dir: "/nowhere".into(),
        }),
    )
    .unwrap();
    assert_eq!(asked.place, Some(ExtLoc::User));
}

#[test]
fn a_machine_without_a_bring_or_not_in_the_table_is_said_per_machine() {
    let list = golden_list();
    let p = plan_many(
        crate::agents::REGISTRY,
        &list,
        None,
        ExtKind::Skill,
        "demo",
        None,
        &[Some("ghost".into())],
        None,
    )
    .unwrap();
    assert!(p.picks[0].bring.is_none());
    assert_eq!(
        plan_many(
            crate::agents::REGISTRY,
            &list,
            None,
            ExtKind::Skill,
            "nope",
            None,
            &[None],
            None
        )
        .unwrap_err()
        .code
        .as_deref(),
        Some("missing")
    );
    assert_eq!(
        plan_many(
            crate::agents::REGISTRY,
            &list,
            None,
            ExtKind::Skill,
            "demo",
            None,
            &[],
            None
        )
        .unwrap_err()
        .code
        .as_deref(),
        Some("bad_args")
    );
}

#[test]
fn slots_merge_into_one_list_and_say_which_machines_already_have_each() {
    let card = |kept: bool| ExtCard {
        kind: ExtKind::Mcp,
        name: "srv".into(),
        path: "/h/.claude.json".into(),
        writes: vec!["/h/.claude.json".into()],
        unchanged: false,
        suspects: vec![],
        stop: None,
        config: None,
        slots: vec![ExtSlot {
            field: "env".into(),
            key: "API_KEY".into(),
            kept,
        }],
        tokens: ExtTokens {
            source: "s".into(),
            target: None,
        },
    };
    let (a, b) = (card(false), card(true));
    assert_eq!(
        json!(merge_slots(&[
            ("laptop".to_string(), &a),
            ("nano".to_string(), &b)
        ])),
        json!([{"field": "env", "key": "API_KEY", "kept": ["nano"]}])
    );
    assert_eq!(
        files_of(&a),
        vec!["/h/.claude.json · mcpServers".to_string()],
        "MCP：路径 ＋ 键，不重复文件名"
    );
    let skill = ExtCard {
        kind: ExtKind::Skill,
        path: "/h/.claude/skills/demo".into(),
        writes: vec!["SKILL.md".into(), "a.sh".into()],
        slots: vec![],
        ..card(false)
    };
    assert_eq!(
        files_of(&skill),
        vec!["/h/.claude/skills/demo · SKILL.md, a.sh".to_string()]
    );
}

/// ★ 那台回的包解不出（后端版本不同：少了一格 · 一格换了型）⇒ `backend_differs`、说是哪台（本机说「本机」），解码器那一句进原话；
/// 不拿半截回包拼卡。
#[tokio::test]
async fn a_reply_that_does_not_parse_says_that_backend_differs() {
    let t = table_with("devbox");
    // 远端那台（来源）：回包缺 `token`。
    let r = remote(&[json!({ "path": "/r/.mcp.json", "def": { "command": "c" }, "slots": [] })]);
    let (_h, dyn_h) = here(&[]);
    let e = pv(&dyn_h, &mcp_args(json!("devbox"), Value::Null), &t, &r)
        .await
        .expect_err("半截回包也拼了卡");
    assert_eq!(e.code.as_deref(), Some("backend_differs"), "{}", e.said);
    assert!(
        e.said.starts_with(&copy_core::backend_differs("devbox")),
        "{}",
        e.said
    );
    assert!(e.said.contains("token"), "解码器那一句没进原话：{}", e.said);
    // 本机那一跳（被写那台）：`state` 换了型。
    let (_h, dyn_h) = here(&[
        (
            "mcp-sync-source",
            json!({ "path": "/a/.mcp.json", "def": { "command": "c" }, "slots": [], "token": "S" }),
        ),
        (
            "mcp-sync-preview",
            json!({ "path": "/l/.mcp.json", "state": 3, "def": { "command": "c" }, "slots": [], "suspects": [], "target": null }),
        ),
    ]);
    let e = pv(
        &dyn_h,
        &mcp_args(Value::Null, Value::Null),
        &t,
        &remote(&[]),
    )
    .await
    .expect_err("本机回的半截包也拼了卡");
    assert_eq!(e.code.as_deref(), Some("backend_differs"), "{}", e.said);
    assert!(
        e.said
            .starts_with(&copy_core::backend_differs(&copy_core::copy_text(
                "detail.label.local",
                &[]
            ))),
        "{}",
        e.said
    );
}

/// 回包多出来的键照收（对面可能更新）。
#[tokio::test]
async fn a_reply_with_an_extra_key_still_parses() {
    let t = table_with("devbox");
    let r = remote(&[
        json!({ "path": "/r/.mcp.json", "def": { "command": "c" }, "slots": [], "token": "t", "newer": 1 }),
    ]);
    let preview = json!({ "path": "/l/.mcp.json", "state": "new", "def": { "command": "c" }, "slots": [], "suspects": [], "target": null, "newer": true });
    let (_h, dyn_h) = here(&[("mcp-sync-preview", preview)]);
    let card = pv(&dyn_h, &mcp_args(json!("devbox"), Value::Null), &t, &r)
        .await
        .expect("多一格就拒了");
    assert_eq!(card["tokens"]["source"], "t");
}

/// 没声明插件那一格的那一家（第二家替身）那一句只读。
fn plugin_read_only_fake2() -> crate::common::said::Said {
    let said = crate::agents::family_said_among(fams(), "fake2");
    crate::common::said::Said::new(
        "refused",
        copy_text("beExt.note.pluginReadOnly", &[("family", &said)]),
    )
}

/// ★ 没声明插件那一格的家：看卡 · 装两条都拒（`refused`，说为什么），一跳也不往外问。
#[tokio::test]
async fn the_hub_refuses_a_plugin_without_asking_anyone() {
    let (h, dyn_h) = here(&[]);
    let t = table_with("devbox");
    let r = remote(&[]);
    let args = json!({ "kind": "plugin", "name": "github", "agent": "fake2", "from": "devbox", "to": null,
                       "tokens": { "source": "t", "target": null },
                       "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    let want = plugin_read_only_fake2();
    assert_eq!(
        pv_in(fams(), &dyn_h, &args, &t, &r)
            .await
            .expect_err("插件给看卡了"),
        want
    );
    let tokens = ExtTokens {
        source: "t".into(),
        target: None,
    };
    assert_eq!(
        ext_apply(fams(), &dyn_h, &args, &tokens, None, &t, &r)
            .await
            .expect_err("插件给装了"),
        want
    );
    assert!(r.calls.lock().unwrap().is_empty(), "问了来源那台");
    assert!(h.asked.lock().unwrap().is_empty(), "问了这台");
}

/// ★ 声明了插件那一格的家：卡上写被写那台要跑的那一行（来源那台目录里那一家的写法恰在里面，不是光名字），不问来源那台；
/// 装 ⇒ 只问被写那台一跳 `plugin-install`（带那一家的写法，默认那一家不写家），它的结局原样交回；
/// 来源那一条没有那一家的写法（旧对端）⇒ 看卡 · 装都拒、说列不出；
/// 落点是项目那一级 ⇒ 看卡 · 装都拒（插件只到用户级，同表上那一处判），一跳不发。
#[tokio::test]
async fn the_hub_installs_a_declared_familys_plugin_by_asking_the_target_once() {
    let done = json!({ "path": "fakeagent gadget fit shelf/github", "changed": ["shelf/github"], "note": null });
    let (h, dyn_h) = here(&[("plugin-install", done.clone())]);
    let t = table_with("devbox");
    let r = remote(&[]);
    let args = json!({ "kind": "plugin", "name": "github", "key": "shelf/github", "from": "devbox", "to": null,
                       "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    let mut keyless = args.clone();
    keyless.as_object_mut().unwrap().remove("key");
    let unlisted =
        crate::common::said::Said::new("refused", copy_text("beExt.plugin.unlisted", &[]));
    assert_eq!(
        pv_in(fams(), &dyn_h, &keyless, &t, &r)
            .await
            .expect_err("没有写法给看卡了"),
        unlisted
    );
    let card = pv_in(fams(), &dyn_h, &args, &t, &r).await.expect("出卡");
    assert_eq!(card["path"], "fakeagent gadget fit shelf/github");
    assert_eq!(card["writes"], json!(["github"]));
    assert!(h.asked.lock().unwrap().is_empty() && r.calls.lock().unwrap().is_empty());
    let tokens: ExtTokens = serde_json::from_value(card["tokens"].clone()).unwrap();
    let mut project = args.clone();
    project["scope"]["to"] = json!({ "level": "project", "dir": "/w" });
    let want =
        crate::common::said::Said::new("refused", copy_text("beExt.target.pluginUserOnly", &[]));
    assert_eq!(
        pv_in(fams(), &dyn_h, &project, &t, &r)
            .await
            .expect_err("项目那一级给看卡了"),
        want
    );
    assert_eq!(
        ext_apply(fams(), &dyn_h, &project, &tokens, None, &t, &r)
            .await
            .expect_err("项目那一级给装了"),
        want
    );
    assert_eq!(
        ext_apply(fams(), &dyn_h, &keyless, &tokens, None, &t, &r)
            .await
            .expect_err("没有写法给装了"),
        unlisted
    );
    assert!(h.asked.lock().unwrap().is_empty());
    let got = ext_apply(fams(), &dyn_h, &args, &tokens, None, &t, &r)
        .await
        .expect("装上");
    assert_eq!(serde_json::to_value(got).unwrap(), done);
    assert_eq!(
        *h.asked.lock().unwrap(),
        vec![(
            "plugin-install".to_string(),
            json!({ "name": "shelf/github" })
        )]
    );
    assert!(r.calls.lock().unwrap().is_empty(), "问了来源那台");
}

// ═══════════════════════════════════════════════════════════════════════════
// 照请求里的家：找哪一行 · 向两头问的那几条都带上家
// ═══════════════════════════════════════════════════════════════════════════

/// 两个替身家（`fake` 默认 · `fake2`）。
fn fams() -> &'static [crate::agents::Adapter] {
    crate::agents::fake::two_families()
}

/// 照给定的注册表看一台的卡（线上形）。
async fn pv_in(
    reg: &[crate::agents::Adapter],
    here: &Arc<dyn Here>,
    args: &Value,
    table: &Table,
    remote: &dyn Remote,
) -> Result<Value, crate::common::said::Said> {
    ext_preview(reg, here, args, table, remote)
        .await
        .map(|c| serde_json::to_value(c).unwrap())
}

/// ★ 两家同名 ⇒ 表里两行：指名第二家 ⇒ 照第二家那一行的「装到…」；不指名 · 指名默认那一家 ⇒ 默认那一家那一行；
/// 指名了认不出的家 ⇒ `bad_args`（家先规整，不当成「表里没有这一行」）。
#[test]
fn many_finds_the_row_of_the_requested_family() {
    let mut list = golden_list();
    let demo = list
        .rows
        .iter()
        .find(|r| r.kind == ExtKind::Skill && r.name == "demo" && r.agent.is_none())
        .cloned()
        .expect("金样里有 demo");
    let mut second = demo.clone();
    second.agent = Some(cmd_wire::AgentKind::from("fake2"));
    for c in &mut second.cells {
        c.bring = None;
    }
    list.rows.push(second);
    let to = [Some("laptop".to_string())];
    let fam2 = cmd_wire::AgentKind::from("fake2");
    let p = plan_many(
        fams(),
        &list,
        Some(&fam2),
        ExtKind::Skill,
        "demo",
        None,
        &to,
        None,
    )
    .unwrap();
    assert!(
        p.picks[0].bring.is_none(),
        "指名第二家却照了默认那一家那一行"
    );
    assert_eq!(p.agent, Some(fam2.clone()), "规整后的家没交给那几跳");
    let p = plan_many(fams(), &list, None, ExtKind::Skill, "demo", None, &to, None).unwrap();
    assert!(p.picks[0].bring.is_some(), "不指名没照默认那一家那一行");
    let named = cmd_wire::AgentKind::from("fake");
    let p = plan_many(
        fams(),
        &list,
        Some(&named),
        ExtKind::Skill,
        "demo",
        None,
        &to,
        None,
    )
    .expect("指名默认那一家找不到行");
    assert!(
        p.picks[0].bring.is_some(),
        "指名默认那一家没照默认那一家那一行"
    );
    assert_eq!(p.agent, None, "指名默认那一家，交给那几跳的家不是缺");
    let ghost = cmd_wire::AgentKind::from("ghost");
    assert_eq!(
        plan_many(
            fams(),
            &list,
            Some(&ghost),
            ExtKind::Skill,
            "demo",
            None,
            &to,
            None
        )
        .unwrap_err()
        .code
        .as_deref(),
        Some("bad_args")
    );
}

/// 一趟 skill 的看卡：来源（远端 devbox）读一次、被写那台（本机）判一次。
fn skill_ends() -> (Value, Value) {
    let files =
        json!([{ "path": "SKILL.md", "text": "x\n", "exec": false, "why": null, "bytes": 2 }]);
    let plan = json!({ "root": "/s", "dir": "/s/demo", "target": [], "base": "/s", "prefix": "demo",
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }],
                       "write": null, "ledger": null });
    (
        json!({ "root": "/r", "dir": "/r/demo", "files": files, "skipped": [] }),
        plan,
    )
}

fn skill_args(name: &str, agent: Option<&str>) -> Value {
    let mut args = json!({ "kind": "skill", "name": name, "from": "devbox", "to": null,
                           "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
    if let Some(k) = agent {
        args["agent"] = json!(k);
    }
    args
}

/// ★ 指名默认那一家 ⇒ 与不指名同一结果：向两头问的那几条都不写 `agent`、卡一样。
#[tokio::test]
async fn naming_the_default_family_is_the_same_as_naming_none() {
    let (read, plan) = skill_ends();
    let t = table_with("devbox");
    let mut got = Vec::new();
    for agent in [Some("fake"), None] {
        let (h, dyn_h) = here(&[("skill-install-plan", plan.clone())]);
        let r = remote(std::slice::from_ref(&read));
        let card = pv_in(fams(), &dyn_h, &skill_args("demo", agent), &t, &r)
            .await
            .expect("看不了卡");
        let (_, stdin) = r.calls.lock().unwrap()[0].clone();
        let sent: Value = serde_json::from_str(stdin.as_deref().unwrap_or("null").trim()).unwrap();
        let asked = h.asked.lock().unwrap()[0].1.clone();
        got.push((
            card,
            sent.get("agent").cloned(),
            asked.get("agent").cloned(),
        ));
    }
    assert_eq!(got[0], got[1], "指名默认那一家与不指名结果不同");
    assert_eq!((got[0].1.clone(), got[0].2.clone()), (None, None));
}

/// ★ 枢纽里写了认不出的家 ⇒ `bad_args`，一跳也不往外问。
#[tokio::test]
async fn an_unknown_family_at_the_hub_is_bad_args() {
    let (h, dyn_h) = here(&[]);
    let t = table_with("devbox");
    let r = remote(&[]);
    let e = pv_in(fams(), &dyn_h, &skill_args("demo", Some("ghost")), &t, &r)
        .await
        .expect_err("认不出的家也看卡了");
    assert_eq!(e.code.as_deref(), Some("bad_args"), "{}", e.said);
    assert!(r.calls.lock().unwrap().is_empty(), "问了来源那台");
    assert!(h.asked.lock().unwrap().is_empty(), "问了这台");
}

/// ★ 第二家项目级 MCP 经枢纽装：来源那一跳（远端，参数一行进 stdin）· 看 · 写那几跳都带上行上的家；不指名 ⇒ 那一格不写。
#[tokio::test]
async fn the_hub_hands_the_rows_family_to_every_mcp_hop() {
    let src = json!({ "path": "/r/fake2-mcp.conf", "def": { "command": "c" }, "slots": [], "token": "t-src" });
    let preview = json!({ "path": "/l/fake2-mcp.conf", "state": "new", "def": { "command": "c" }, "slots": [], "suspects": [], "target": null });
    let (h, dyn_h) = here(&[
        ("mcp-sync-preview", preview),
        (
            "mcp-sync-apply",
            json!({ "path": "/l/fake2-mcp.conf", "written": true, "recordFailed": null }),
        ),
    ]);
    let t = table_with("devbox");
    let r = remote(&[src.clone(), src]);
    let mut args = mcp_args(json!("devbox"), Value::Null);
    args["agent"] = json!("fake2");
    pv_in(fams(), &dyn_h, &args, &t, &r)
        .await
        .expect("第二家的项目级 MCP 没给看卡");
    let tokens: ExtTokens =
        serde_json::from_value(json!({ "source": "t-src", "target": null })).unwrap();
    let done = ext_apply(fams(), &dyn_h, &args, &tokens, None, &t, &r)
        .await
        .expect("第二家的项目级 MCP 没装上");
    assert_eq!(
        serde_json::to_value(done).unwrap()["path"],
        "/l/fake2-mcp.conf"
    );
    let calls = r.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "{calls:?}");
    for (_, stdin) in &calls {
        assert_eq!(
            serde_json::from_str::<Value>(stdin.as_deref().unwrap().trim()).unwrap(),
            json!({ "name": "x", "at": { "level": "project", "dir": "/r" }, "agent": "fake2" }),
            "来源那一跳没带家"
        );
    }
    let asked = h.asked.lock().unwrap().clone();
    assert_eq!(
        asked.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>(),
        vec!["mcp-sync-preview", "mcp-sync-preview", "mcp-sync-apply"]
    );
    for (c, a) in &asked {
        assert_eq!(a["agent"], "fake2", "{c} 没带家");
    }
    // 不指名 ⇒ 那一格不写（旧对端照收）。
    let (h, dyn_h) = here(&[(
        "mcp-sync-preview",
        json!({ "path": "/l/.fake-mcp.json", "state": "new", "def": { "command": "c" }, "slots": [], "suspects": [], "target": null }),
    )]);
    let r = remote(&[
        json!({ "path": "/r/.fake-mcp.json", "def": { "command": "c" }, "slots": [], "token": "t" }),
    ]);
    pv_in(
        fams(),
        &dyn_h,
        &mcp_args(json!("devbox"), Value::Null),
        &t,
        &r,
    )
    .await
    .expect("默认那一家没给看卡");
    let stdin = r.calls.lock().unwrap()[0].1.clone().unwrap();
    assert!(!stdin.contains("agent"), "不指名也写了家：{stdin}");
    assert!(h.asked.lock().unwrap()[0].1.get("agent").is_none());
}

/// ★ 第二家的 MCP 来源或落点在全局那一级 ⇒ 枢纽先拒（全局那一份只认默认那一家），看卡 · 装都一跳也不往外问。
#[tokio::test]
async fn the_hub_refuses_a_second_familys_user_level_mcp_without_asking_anyone() {
    let want = crate::common::said::Said::new(
        "refused",
        crate::assets::ext::family_refused(ExtKind::Mcp, Some(&"fake2".into()), &ExtLoc::User)
            .unwrap(),
    );
    for side in ["from", "to"] {
        let (h, dyn_h) = here(&[]);
        let t = table_with("devbox");
        let r = remote(&[]);
        let mut args = mcp_args(json!("devbox"), Value::Null);
        args["scope"][side] = json!({ "level": "user" });
        args["agent"] = json!("fake2");
        assert_eq!(
            pv_in(fams(), &dyn_h, &args, &t, &r)
                .await
                .expect_err("第二家的全局 MCP 给看卡了"),
            want,
            "{side}"
        );
        let tokens: ExtTokens =
            serde_json::from_value(json!({ "source": "t", "target": null })).unwrap();
        assert_eq!(
            ext_apply(fams(), &dyn_h, &args, &tokens, None, &t, &r)
                .await
                .expect_err("第二家的全局 MCP 给装了"),
            want,
            "{side}"
        );
        assert!(r.calls.lock().unwrap().is_empty(), "{side}：问了来源那台");
        assert!(h.asked.lock().unwrap().is_empty(), "{side}：问了这台");
    }
}

/// ★ 自带的 · 自己发版的照带家的行认：第二家一个叫 cc-bus 的 skill ⇒ 照 skill 那条路问两头（不装自带的那一份）；
/// 第二家一个叫 planned-build 的插件 ⇒ 插件只读、拒（自己发版的例外只属默认那一家）。
#[tokio::test]
async fn builtin_and_release_are_judged_on_the_familys_row() {
    let (read, plan) = skill_ends();
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[("skill-install-plan", plan)]);
    let r = remote(&[read]);
    pv_in(
        fams(),
        &dyn_h,
        &skill_args(crate::assets::cc_bus_install::NAME, Some("fake2")),
        &t,
        &r,
    )
    .await
    .expect("第二家的同名 skill 看不了卡");
    let asked: Vec<String> = h
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    assert_eq!(
        asked,
        vec!["skill-install-plan".to_string()],
        "第二家的同名 skill 走了自带的那条路"
    );
    let (h, dyn_h) = here(&[]);
    let r = remote(&[]);
    let mut plugin = skill_args("planned-build", Some("fake2"));
    plugin["kind"] = json!("plugin");
    assert_eq!(
        pv_in(fams(), &dyn_h, &plugin, &t, &r)
            .await
            .expect_err("第二家的同名插件照自己发版的那条路装了"),
        plugin_read_only_fake2()
    );
    assert!(h.asked.lock().unwrap().is_empty() && r.calls.lock().unwrap().is_empty());
}

/// ★ 装那一跳带家：指名第二家装 ⇒ 交被写那台的 `skill-install-apply` 带上那一家。
#[tokio::test]
async fn the_apply_hop_carries_the_family() {
    let (read, plan) = skill_ends();
    let applied = json!({ "dir": "/s/demo", "written": ["SKILL.md"], "chmodFailed": [], "recordFailed": null });
    let t = table_with("devbox");
    let (h, dyn_h) = here(&[
        ("skill-install-plan", plan),
        ("skill-install-apply", applied),
    ]);
    let r = remote(&[read.clone(), read]);
    let args = skill_args("demo", Some("fake2"));
    let card = pv_in(fams(), &dyn_h, &args, &t, &r)
        .await
        .expect("看不了卡");
    let tokens: ExtTokens = serde_json::from_value(card["tokens"].clone()).unwrap();
    ext_apply(fams(), &dyn_h, &args, &tokens, None, &t, &r)
        .await
        .expect("装不了");
    let asked = h.asked.lock().unwrap().clone();
    let apply = asked
        .iter()
        .find(|(c, _)| c == "skill-install-apply")
        .expect("没交被写那台装");
    assert_eq!(
        apply.1.get("agent"),
        Some(&json!("fake2")),
        "装那一跳没带家"
    );
}

/// ★ 一台那一跳的参数带上规整后的家（缺 ⇒ 不写）；插件另带那一行的来源键（行上没有 ⇒ `null`）。
#[test]
fn the_per_machine_ask_carries_the_family() {
    let b = ExtBring {
        from: Some("devbox".into()),
        scope: super::super::ext::ExtScope {
            from: ExtLoc::User,
            to: ExtLoc::User,
        },
        targets: Vec::new(),
    };
    let p = Pick {
        to: None,
        name: "here".into(),
        bring: Some(b.clone()),
    };
    let fam2 = cmd_wire::AgentKind::from("fake2");
    let v = ask_for(
        None,
        ExtKind::Skill,
        "demo",
        Some(&fam2),
        &p,
        &b,
        &ExtLoc::User,
    );
    assert_eq!(v["agent"], json!("fake2"), "一台那一跳没带家");
    assert_eq!(v["key"], Value::Null, "skill 带了插件的写法");
    let v = ask_for(
        Some("shelf/demo"),
        ExtKind::Plugin,
        "demo",
        Some(&fam2),
        &p,
        &b,
        &ExtLoc::User,
    );
    assert_eq!(v["key"], json!("shelf/demo"), "没照那一行的来源带写法");
}

/// ★ 指名第二家装一个 skill ⇒ 来源那台的 `skill-read` 与被写那台的 `skill-install-plan` 都带上那一家；
/// 不指名 ⇒ 两条都不写 `agent`（旧对端拒多出来的键，默认那一家那一形与今天逐字相同）。
#[tokio::test]
async fn the_hub_asks_both_ends_for_the_requested_family() {
    let files =
        json!([{ "path": "SKILL.md", "text": "x\n", "exec": false, "why": null, "bytes": 2 }]);
    let plan = json!({ "root": "/s", "dir": "/s/demo", "target": [], "base": "/s", "prefix": "demo",
                       "rows": [{ "path": "SKILL.md", "state": "new", "suspects": [], "blocked": null }],
                       "write": null, "ledger": null });
    let t = table_with("devbox");
    for agent in [Some("fake2"), None] {
        let (h, dyn_h) = here(&[("skill-install-plan", plan.clone())]);
        let r = remote(&[json!({ "root": "/r", "dir": "/r/demo", "files": files, "skipped": [] })]);
        let mut args = json!({ "kind": "skill", "name": "demo", "from": "devbox", "to": null,
                               "scope": { "from": { "level": "user" }, "to": { "level": "user" } } });
        if let Some(k) = agent {
            args["agent"] = json!(k);
        }
        pv_in(fams(), &dyn_h, &args, &t, &r)
            .await
            .expect("看不了卡");
        let (_, stdin) = r.calls.lock().unwrap()[0].clone();
        let read: Value = serde_json::from_str(stdin.as_deref().unwrap_or("null").trim()).unwrap();
        let asked = h.asked.lock().unwrap()[0].1.clone();
        let want = agent.map(|k| json!(k));
        assert_eq!(
            (read.get("agent").cloned(), asked.get("agent").cloned()),
            (want.clone(), want),
            "向两头问的那几条没照请求里的家（{agent:?}）"
        );
    }
}

/// ★ 来源那台目录里同一个光名字的插件两条（写法不同）⇒ 表上两行；装照请求带的那一行的来源走：
/// 卡上与被写那台那一跳交的都是那一行的写法（不取第一条）；不带来源 ⇒ 认不出是哪一行（`missing`），一跳不发。
#[tokio::test]
async fn the_hub_installs_the_plugin_of_the_rows_source_when_a_name_has_two() {
    let plugin = |key: &str| crate::assets::asset_catalog::Asset {
        agent: None,
        kind: ExtKind::Plugin,
        name: "demo".into(),
        project: None,
        dir: None,
        digest: format!("d-{key}"),
        summary: json!({ (crate::assets::plugin_exec::KEY): key,
                         (crate::assets::plugin_exec::BY_COMMAND): true }),
        off: false,
    };
    let snap = |assets| crate::assets::asset_catalog::Snapshot {
        label: "peer".into(),
        gen: 1,
        seen_at: 0,
        project_dirs: Vec::new(),
        notes: Default::default(),
        shared_mcp: false,
        assets,
    };
    let mut cat = crate::assets::asset_catalog::fresh("me".into());
    cat.machines.insert("me".into(), snap(Vec::new()));
    cat.machines.insert(
        "peer".into(),
        snap(vec![plugin("shelf/demo"), plugin("rack/demo")]),
    );
    let done = json!({ "path": "claude plugin install rack/demo", "changed": ["rack/demo"], "note": null });
    let (h, dyn_h) = here(&[("plugin-install", done)]);
    let t = table_with("devbox");
    t.set_peer("devbox", "peer");
    let r = remote(&[]);
    let current = move |_visit: bool| Ok((cat.clone(), Vec::new()));
    use crate::assets::hub_wire::{HubApplyReq, HubReq};
    use crate::wire::typed::take;
    let bare = ext_preview_many(
        &dyn_h,
        &take::<HubReq>(&json!({ "kind": "plugin", "name": "demo", "to": [null] })).unwrap(),
        &t,
        &r,
        &current,
    )
    .await
    .expect_err("不带来源却认出了一行");
    assert_eq!(bare.code.as_deref(), Some("missing"));
    let pre = ext_preview_many(
        &dyn_h,
        &take::<HubReq>(
            &json!({ "kind": "plugin", "name": "demo", "source": "rack/demo", "to": [null] }),
        )
        .unwrap(),
        &t,
        &r,
        &current,
    )
    .await
    .expect("拼不出表");
    let card = pre.machines[0]
        .card
        .clone()
        .unwrap_or_else(|| panic!("带来源却没出卡：{:?}", pre.machines[0]));
    assert!(
        card.path.ends_with(" rack/demo"),
        "卡上不是那一行的写法：{}",
        card.path
    );
    let apply = take::<HubApplyReq>(
        &json!({ "kind": "plugin", "name": "demo", "source": "rack/demo",
        "to": [null], "place": pre.place.clone().unwrap_or(ExtLoc::User),
        "tokens": { "": card.tokens }, "fill": {} }),
    )
    .unwrap();
    let got = ext_apply_many(&dyn_h, &apply, &t, &r, &current)
        .await
        .expect("拼不出表");
    assert!(got.machines[0].done.is_some(), "{:?}", got.machines[0]);
    assert_eq!(
        *h.asked.lock().unwrap(),
        vec![("plugin-install".to_string(), json!({ "name": "rack/demo" }))],
        "装的不是那一行的那一条"
    );
    assert!(r.calls.lock().unwrap().is_empty(), "问了来源那台");
}
