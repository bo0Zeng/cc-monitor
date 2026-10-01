//! 要求：设置「扩展」页 —— 各台目录合成「条目 × 机器」一张表、每格的态与唯一那个按钮都由后端判（界面只画，线上没有摘要）；
//! 装 / 卸跨两台走同一对枢纽命令，看过之后源变了 ⇒ `stale`、目标零写；卸：cc-monitor 装的按装记录撤，不是的先备份再删。
use super::*;
use crate::assets::asset_catalog::{Scanned, Snapshot, Visits};
use crate::assets::hub::{ext_apply, ext_preview, Here};
use crate::assets::mcp_sync::{Facts, There};
use crate::stream::inbound::LocalFiles;
use crate::stream::remote_ask::{register, Remote, Said, Table};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

struct NoFacts;
impl Facts for NoFacts {
    fn path(&self, _p: &str) -> There {
        There::Absent
    }
    fn command(&self, _name: &str) -> Option<bool> {
        None
    }
}

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ext-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ───────────────────────── 表的判定（纯） ─────────────────────────

fn asset(kind: &str, name: &str, digest: &str, project: Option<&str>) -> Asset {
    Asset {
        kind: kind.into(),
        name: name.into(),
        project: project.map(str::to_string),
        dir: Some(format!("/d/{name}")),
        digest: digest.into(),
        summary: json!({ "description": format!("about {name}") }),
    }
}

fn snap(label: &str, assets: Vec<Asset>, projects: &[&str]) -> Snapshot {
    Snapshot {
        label: label.into(),
        gen: 1,
        seen_at: 0,
        assets,
        project_dirs: projects.iter().map(|p| p.to_string()).collect(),
    }
}

/// 一格压成一句好比的话：态 ＋ 那个按钮（或没有按钮的那一句）。
fn brief(c: &ExtCell) -> String {
    let state = serde_json::to_value(c.state).unwrap();
    let act = match &c.action {
        None => format!("note:{}", c.note.clone().unwrap_or_default()),
        Some(ExtAction::Uninstall { at }) => {
            format!("uninstall@{}", at.project().unwrap_or("user"))
        }
        Some(ExtAction::Install { from, scope, .. }) => format!(
            "install<{}@{}>@{}",
            from.as_deref().unwrap_or("here"),
            scope.from.project().unwrap_or("user"),
            scope.to.project().unwrap_or("user")
        ),
        Some(ExtAction::Replace { from, scope, .. }) => format!(
            "replace<{}@{}>@{}",
            from.as_deref().unwrap_or("here"),
            scope.from.project().unwrap_or("user"),
            scope.to.project().unwrap_or("user")
        ),
    };
    format!("{}:{act}", state.as_str().unwrap())
}

/// ★ 表格判据：持有人最多的那一版算「这一版」、打平时本机优先 · 只在项目里有 ⇒ `project` · 用户级 MCP 只读 ·
/// 没连上的那台没有按钮 · MCP 只装进项目、那台没项目就说出来 · 「新见到」只标上次来看之后第一次见到的。
#[test]
fn the_table_judges_every_cell_and_names_its_one_button() {
    let n = |k: &str| copy_text(k, &[]);
    let mut cat = asset_catalog::fresh("h".into());
    cat.machines.insert(
        "h".into(),
        snap(
            "me@h",
            vec![
                asset(KIND_SKILL, "s-major", "d1", None),
                asset(KIND_SKILL, "s-tie", "dA", None),
                asset(KIND_MCP, "m-user", "m1", None),
                asset(KIND_MCP, "m-proj", "m2", Some("/h/p")),
            ],
            &["/h/p", "/shared"],
        ),
    );
    cat.machines.insert(
        "r1".into(),
        snap(
            "u@r1",
            vec![
                asset(KIND_SKILL, "s-major", "d1", None),
                asset(KIND_SKILL, "s-tie", "dB", None),
                asset(KIND_SKILL, "s-proj", "d9", Some("/shared")),
            ],
            &[],
        ),
    );
    cat.machines.insert(
        "r2".into(),
        snap(
            "u@r2",
            vec![asset(KIND_SKILL, "s-major", "d2", None)],
            &["/x"],
        ),
    );
    cat.known = [
        ("skill/s-major", 5u64),
        ("skill/s-tie", 5),
        ("skill/s-proj", 50),
        ("mcp/m-user", 5),
        ("mcp/m-proj", 5),
    ]
    .iter()
    .map(|(k, t)| (k.to_string(), *t))
    .collect();
    cat.visits = Visits { prev: 10, last: 60 };
    let reach = vec![("aya".to_string(), Some("r1".to_string()))];
    let list = table(&cat, &reach);
    let names: Vec<(Option<String>, bool, bool, String)> = list
        .machines
        .iter()
        .map(|m| (m.key.clone(), m.here, m.reachable, m.name.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            (None, true, true, "me@h".into()),
            (Some("aya".into()), false, true, "aya".into()),
            (None, false, false, "u@r2".into()),
        ],
        "本机在前；认得出的用可达表里的名字；没连上的用它自报的称呼"
    );
    let got: Vec<(String, bool, Vec<String>)> = list
        .rows
        .iter()
        .map(|r| (r.name.clone(), r.new, r.cells.iter().map(brief).collect()))
        .collect();
    let off = format!("note:{}", n("beExt.note.offline"));
    let ro = format!("note:{}", n("beExt.note.userMcpReadOnly"));
    let no_proj = format!("note:{}", n("beExt.note.noProject"));
    let want: Vec<(String, bool, Vec<String>)> = vec![
        (
            "m-proj".into(),
            false,
            vec![
                "project:uninstall@/h/p".into(),
                format!("missing:{no_proj}"),
                format!("missing:{off}"),
            ],
        ),
        (
            "m-user".into(),
            false,
            vec![
                format!("same:{ro}"),
                format!("missing:{no_proj}"),
                format!("missing:{off}"),
            ],
        ),
        (
            "s-major".into(),
            false,
            vec![
                "same:uninstall@user".into(),
                "same:uninstall@user".into(),
                format!("differs:{off}"),
            ],
        ),
        (
            "s-proj".into(),
            true,
            vec![
                "missing:install<aya@/shared>@/shared".into(),
                "project:uninstall@/shared".into(),
                format!("missing:{off}"),
            ],
        ),
        (
            "s-tie".into(),
            false,
            vec![
                "same:uninstall@user".into(),
                "differs:replace<here@user>@user".into(),
                format!("missing:{off}"),
            ],
        ),
    ];
    assert_eq!(got, want);
    let wire = serde_json::to_string(&list).unwrap();
    assert!(
        !wire.contains("digest") && !wire.contains("\"d1\""),
        "线上带了摘要：界面就有东西可比了"
    );
    // 正控：摘要确实在目录里（不是因为本来就没有才看不到）。
    assert!(serde_json::to_string(&cat).unwrap().contains("\"d1\""));
    // 从没来看过（prev = 0）⇒ 一条都不标「新」。
    cat.visits = Visits { prev: 0, last: 60 };
    assert!(table(&cat, &reach).rows.iter().all(|r| !r.new));
}

/// 一个扩展常常是「一个 skill ＋ 一个同名 MCP」两样东西（用法说明 ＋ 干活的服务器，code-picture 就是这一形）：
/// 表按（类，名）分行 ⇒ 两条都列出来、各判各的格，不因为同名并成一行或互相盖掉。
#[test]
fn a_skill_and_an_mcp_with_the_same_name_are_listed_as_two_rows() {
    let mut cat = asset_catalog::fresh("h".into());
    cat.machines.insert(
        "h".into(),
        snap(
            "me@h",
            vec![
                asset(KIND_SKILL, "code-picture", "d1", None),
                asset(KIND_MCP, "code-picture", "m1", Some("/h/p")),
            ],
            &["/h/p"],
        ),
    );
    let list = table(&cat, &[]);
    let got: Vec<(Value, String, Vec<String>)> = list
        .rows
        .iter()
        .map(|r| {
            (
                serde_json::to_value(r.kind).unwrap(),
                r.name.clone(),
                r.cells.iter().map(brief).collect(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                json!("mcp"),
                "code-picture".to_string(),
                vec!["project:uninstall@/h/p".to_string()]
            ),
            (
                json!("skill"),
                "code-picture".to_string(),
                vec!["same:uninstall@user".to_string()]
            ),
        ]
    );
}

// ───────────────────────── 两台：本机 ＋ 一台假远端 ─────────────────────────

/// 一台机器 = 一个临时家目录（skill 根 · 用户级 MCP · 装记录 · 目录文件 · 一个开过会话的项目）。
struct M {
    env: Env,
    cat: PathBuf,
    label: &'static str,
    proj: PathBuf,
}

fn machine(base: &Path, tag: &'static str) -> M {
    let home = base.join(tag);
    for d in [".cc-monitor", ".claude/skills", "proj"] {
        std::fs::create_dir_all(home.join(d)).unwrap();
    }
    M {
        env: Env {
            skills: Some(home.join(".claude/skills")),
            user_mcp: Some(home.join(".claude.json")),
            ledger: Some(home.join(".cc-monitor/skill-installs.json")),
            home: Some(home.clone()),
        },
        cat: home.join(".cc-monitor/assets-catalog.json"),
        label: tag,
        proj: home.join("proj"),
    }
}

impl M {
    fn projects(&self) -> Vec<String> {
        vec![self.proj.display().to_string()]
    }
    fn scan(&self) -> Scanned {
        let s = crate::agents::claudecode::assets::scan_at(
            self.env.skills.as_deref(),
            self.env.user_mcp.as_deref(),
            &self.projects(),
        );
        let (assets, problems) = asset_catalog::assets_from(&[s]);
        Scanned {
            assets,
            projects: self.projects(),
            problems,
        }
    }
    fn refresh(&self, incoming: Option<BTreeMap<String, Snapshot>>) -> Catalog {
        asset_catalog::update_with(&self.cat, self.scan(), self.label, incoming, false, 100)
            .expect("目录记不下")
            .0
    }
    /// 这台跑一条内层命令（与生产那几条同一个本体，只是根换成这台的临时家目录）。
    fn inner(&self, cmd: &str, args: &Value) -> Result<Value, (String, String)> {
        let skills = self.env.skills.clone().unwrap();
        let ledger = self.env.ledger.clone().unwrap();
        let root = match args.get("project").and_then(Value::as_str) {
            Some(p) => crate::agents::skill_root_at(Some(Path::new(p))).unwrap(),
            None => skills.clone(),
        };
        let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
        let r = match cmd {
            "skill-read" => super::super::skill_install::answer_read_at(Some(&root), args),
            "skill-install-plan" => {
                super::super::skill_install::answer_plan_with(&NoFacts, Some(&root), args)
            }
            "skill-install-apply" => super::super::skill_flow::answer_install(
                &LocalFiles,
                &NoFacts,
                Some(&root),
                &record,
                args,
            ),
            "mcp-sync-source" => super::super::mcp_sync_flow::answer_source(
                &LocalFiles,
                self.env.user_mcp.as_deref(),
                args,
            ),
            "mcp-sync-preview" => {
                super::super::mcp_sync_flow::answer_preview(&LocalFiles, &NoFacts, args)
            }
            "mcp-sync-apply" => {
                super::super::mcp_sync_flow::answer_apply(&LocalFiles, &record, args)
            }
            other => Err(("unknown_command", other.to_string())),
        };
        r.map_err(|(c, m)| (c.to_string(), m))
    }
}

/// 本机那一跳：这台自己。
struct HereM(Arc<M>);
impl Here for HereM {
    fn ask(&self, cmd: &str, args: Value) -> Result<Value, (String, String)> {
        self.0.inner(cmd, &args)
    }
}

/// 远端那一跳：从命令行里认出子命令、参数从 stdin 那一行读，交那台自己跑；记下被叫了哪几条。
struct RemoteM(Arc<M>, Mutex<Vec<String>>);
impl Remote for RemoteM {
    fn run<'a>(
        &'a self,
        _dial: &'a Value,
        _command: String,
        _stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async { Err("走 run_coded".to_string()) })
    }
    fn run_coded<'a>(
        &'a self,
        _dial: &'a Value,
        command: String,
        stdin: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, Said>> + Send + 'a>> {
        let flag = command
            .split(" -- ")
            .nth(1)
            .and_then(|t| t.split('\'').nth(1))
            .unwrap_or_default()
            .trim_start_matches('-')
            .to_string();
        self.1.lock().unwrap().push(flag.clone());
        let args: Value = serde_json::from_str(stdin.as_deref().unwrap_or("null").trim()).unwrap();
        let out = self.0.inner(&flag, &args);
        Box::pin(async move {
            out.map(|v| v.to_string()).map_err(|(c, m)| Said {
                code: Some(c),
                message: m,
            })
        })
    }
}

struct Two {
    a: Arc<M>,
    b: Arc<M>,
    here: Arc<dyn Here>,
    remote: RemoteM,
    reach: Table,
}

fn two(tag: &str) -> (PathBuf, Two) {
    let base = temp(tag);
    let a = Arc::new(machine(&base, "a"));
    let b = Arc::new(machine(&base, "b"));
    let reach: Table = Mutex::new(BTreeMap::new());
    register(
        &reach,
        &json!({ "origin": "gpd", "dial": {"machine": {"host": "h", "port": 22, "user": "u", "keyPath": "/k"}} }),
    )
    .unwrap();
    let here: Arc<dyn Here> = Arc::new(HereM(a.clone()));
    let remote = RemoteM(b.clone(), Mutex::new(Vec::new()));
    (
        base,
        Two {
            a,
            b,
            here,
            remote,
            reach,
        },
    )
}

impl Two {
    /// 各台现扫一次、B 的那份并进 A（同步那一跳由 `asset_sync` 的判据管），A 出表。
    fn list(&self) -> ExtList {
        let cb = self.b.refresh(None);
        let ca = self.a.refresh(Some(cb.machines.clone()));
        crate::stream::remote_ask::lock(&self.reach)
            .get_mut("gpd")
            .unwrap()
            .peer = Some(cb.self_id.clone());
        table(&ca, &reach_of(&self.reach))
    }
    fn cell(&self, list: &ExtList, name: &str, at: usize) -> ExtCell {
        list.rows
            .iter()
            .find(|r| r.name == name)
            .expect("表里没有这一行")
            .cells[at]
            .clone()
    }
}

fn scope_of(a: &ExtAction) -> (Option<String>, ExtScope) {
    match a {
        ExtAction::Install { from, scope, .. } | ExtAction::Replace { from, scope, .. } => {
            (from.clone(), scope.clone())
        }
        ExtAction::Uninstall { .. } => panic!("这一格是卸"),
    }
}

/// ★ 端到端：本机有个 skill、假远端没有 —— 表上远端那一格 ○ → 装（中途改源 ⇒ `stale`、远端零写）→ ● → 卸 → ○。
#[tokio::test]
async fn a_skill_goes_to_the_other_machine_and_comes_back_off() {
    let (base, t) = two("e2e-skill");
    let src = t.a.env.skills.clone().unwrap().join("demo");
    std::fs::create_dir_all(src.join("lib")).unwrap();
    std::fs::write(src.join("SKILL.md"), "---\ndescription: d\n---\n").unwrap();
    std::fs::write(src.join("lib/x.txt"), "one\n").unwrap();

    let list = t.list();
    assert_eq!(list.machines[1].key.as_deref(), Some("gpd"));
    assert_eq!(t.cell(&list, "demo", 0).state, ExtState::Same);
    let there = t.cell(&list, "demo", 1);
    assert_eq!(there.state, ExtState::Missing);
    let (from, scope) = scope_of(there.action.as_ref().expect("缺的那一格没按钮"));
    assert_eq!(from, None, "来源是本机");
    let args =
        json!({ "kind": "skill", "name": "demo", "from": from, "to": "gpd", "scope": scope });

    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("看卡");
    assert_eq!(card["writes"], json!(["SKILL.md", "lib/x.txt"]));
    // 看过之后源变了 ⇒ `stale`，远端一个字节不写。
    std::fs::write(src.join("lib/x.txt"), "two\n").unwrap();
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let (code, _) = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect_err("源变了还装了");
    assert_eq!(code, "stale");
    let dst = t.b.env.skills.clone().unwrap().join("demo");
    assert!(!dst.exists(), "stale 了远端还多出了目录");
    assert!(
        !t.b.env.ledger.clone().unwrap().exists(),
        "stale 了远端还记了账"
    );
    assert!(
        !t.remote
            .1
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "skill-install-apply"),
        "stale 了还叫远端去写"
    );

    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("重看");
    apply["tokens"] = card["tokens"].clone();
    let done = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect("装不上");
    assert_eq!(done["changed"], json!(["SKILL.md", "lib/x.txt"]));
    assert_eq!(
        std::fs::read_to_string(dst.join("lib/x.txt")).unwrap(),
        "two\n"
    );

    let list = t.list();
    let there = t.cell(&list, "demo", 1);
    assert_eq!(there.state, ExtState::Same, "装完那一格没变 ●");
    let Some(ExtAction::Uninstall { at }) = there.action else {
        panic!("● 那一格的按钮该是卸：{there:?}")
    };

    // 卸在被卸那台上判、写（界面直问那台）：装记录里有 ⇒ 只撤装时写的。
    let u = json!({ "kind": "skill", "name": "demo", "at": at });
    let card = answer_uninstall_preview(&LocalFiles, &t.b.env, &u).expect("卸之前那张卡");
    assert_eq!(card["recorded"], true);
    assert_eq!(card["files"], json!(["SKILL.md", "lib/x.txt"]));
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    answer_uninstall_apply(&LocalFiles, &t.b.env, &record, &ua).expect("卸不掉");
    assert!(!dst.exists(), "装时建的目录没收掉");

    let list = t.list();
    assert_eq!(
        t.cell(&list, "demo", 1).state,
        ExtState::Missing,
        "卸完那一格没变 ○"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ 端到端（MCP）：本机项目里的一条带密钥 → 装进远端的项目：卡上是待填、远端写下的是用户填的值、来源的值一个字都没过去；
/// 表上那一格 ◎（只在项目里）；卸 ⇒ 按装记录撤、○。
#[tokio::test]
async fn an_mcp_entry_goes_over_without_its_secret_and_comes_back_off() {
    let (base, t) = two("e2e-mcp");
    std::fs::write(
        t.a.proj.join(".mcp.json"),
        json!({ "mcpServers": { "m": { "command": "srv", "env": { "KEY": "secret-A" } } } })
            .to_string(),
    )
    .unwrap();
    let list = t.list();
    let there = t.cell(&list, "m", 1);
    assert_eq!(there.state, ExtState::Missing);
    let (from, scope) = scope_of(there.action.as_ref().unwrap());
    assert_eq!(
        scope.to,
        ExtLoc::Project {
            dir: t.b.proj.display().to_string()
        },
        "MCP 只装进项目：替那台先挑了它开过会话的那一个"
    );
    let args = json!({ "kind": "mcp", "name": "m", "from": from, "to": "gpd", "scope": scope });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("看卡");
    assert_eq!(
        card["slots"],
        json!([{ "field": "env", "key": "KEY", "kept": false }])
    );
    assert!(
        !card.to_string().contains("secret-A"),
        "卡上有来源的密钥：{card}"
    );
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let (code, _) = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect_err("没填值也装了");
    assert_eq!(code, "needs_input");
    apply["fill"] = json!({ "env": { "KEY": "typed-B" } });
    ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect("装不上");
    let written = std::fs::read_to_string(t.b.proj.join(".mcp.json")).unwrap();
    assert!(
        written.contains("typed-B") && !written.contains("secret-A"),
        "{written}"
    );

    let list = t.list();
    let there = t.cell(&list, "m", 1);
    assert_eq!(there.state, ExtState::Project, "装进项目之后那一格是 ◎");
    let Some(ExtAction::Uninstall { at }) = there.action else {
        panic!("◎ 那一格的按钮该是卸：{there:?}")
    };
    let u = json!({ "kind": "mcp", "name": "m", "at": at });
    let card = answer_uninstall_preview(&LocalFiles, &t.b.env, &u).unwrap();
    assert_eq!(
        (card["recorded"].clone(), card["backup"].clone()),
        (json!(true), Value::Null)
    );
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    answer_uninstall_apply(&LocalFiles, &t.b.env, &record, &ua).expect("卸不掉");
    let list = t.list();
    assert_eq!(t.cell(&list, "m", 1).state, ExtState::Missing);
    assert!(
        super::super::skill_ledger::load_at(&ledger)
            .unwrap()
            .mcp
            .is_empty(),
        "卸完没从装记录里摘掉"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// 那一句话里的路径（按文案表那一句的前后两截切出来，不另写一份拼法）。
fn said_path(note: &Value, key: &str) -> PathBuf {
    let tpl = copy_text(key, &[("path", "\u{0}")]);
    let (pre, post) = tpl.split_once('\u{0}').expect("那一句没有 {path}");
    let note = note.as_str().expect("没说放到了哪");
    PathBuf::from(
        note.strip_prefix(pre)
            .and_then(|r| r.strip_suffix(post))
            .unwrap_or_else(|| panic!("认不出那一句：{note}")),
    )
}

/// 不是 cc-monitor 装的：卡上明说、先挪 / 抄进备份再删；看过之后变了 ⇒ `stale`、一个字节不动。
#[test]
fn a_foreign_skill_or_mcp_entry_is_backed_up_before_it_goes() {
    let base = temp("foreign");
    let m = machine(&base, "m");
    let dir = m.env.skills.clone().unwrap().join("mine");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), "mine\n").unwrap();
    let record = |_: &Value| Ok(json!({}));
    let u = json!({ "kind": "skill", "name": "mine", "at": { "level": "user" } });
    let card = answer_uninstall_preview(&LocalFiles, &m.env, &u).unwrap();
    assert_eq!(card["recorded"], false);
    assert!(
        card["backup"].as_str().unwrap().ends_with("backups"),
        "{card}"
    );
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    std::fs::write(dir.join("SKILL.md"), "edited\n").unwrap();
    let (code, _) =
        answer_uninstall_apply(&LocalFiles, &m.env, &record, &ua).expect_err("变了还删了");
    assert_eq!(code, "stale");
    assert!(dir.join("SKILL.md").exists());
    let card = answer_uninstall_preview(&LocalFiles, &m.env, &u).unwrap();
    ua["token"] = card["token"].clone();
    let done = answer_uninstall_apply(&LocalFiles, &m.env, &record, &ua).expect("卸不掉");
    assert!(!dir.exists(), "没删");
    let backups = m
        .env
        .home
        .clone()
        .unwrap()
        .join(relay_route_core::EXT_BACKUPS_DIR_REL);
    let kept = said_path(&done["note"], "beExt.uninstall.movedTo");
    assert!(kept.starts_with(&backups), "{kept:?} 不在备份目录里");
    assert_eq!(
        std::fs::read_to_string(kept.join("SKILL.md")).unwrap(),
        "edited\n",
        "挪进备份的是卸之前那一份"
    );

    std::fs::write(
        m.proj.join(".mcp.json"),
        json!({ "mcpServers": { "x": { "command": "c" }, "y": { "command": "d" } } }).to_string(),
    )
    .unwrap();
    let u = json!({ "kind": "mcp", "name": "x", "at": { "level": "project", "dir": m.proj.display().to_string() } });
    let card = answer_uninstall_preview(&LocalFiles, &m.env, &u).unwrap();
    assert_eq!(card["recorded"], false);
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let done = answer_uninstall_apply(&LocalFiles, &m.env, &record, &ua).expect("卸不掉");
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(m.proj.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        after["mcpServers"],
        json!({ "y": { "command": "d" } }),
        "别的条目动了"
    );
    let copy = said_path(&done["note"], "beExt.uninstall.copiedTo");
    assert!(copy.starts_with(&backups), "{copy:?} 不在备份目录里");
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(copy.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        kept["mcpServers"]["x"],
        json!({ "command": "c" }),
        "MCP 那份原文没抄进备份"
    );
    // 用户级 MCP：只读，卸也拒。
    let (code, _) = answer_uninstall_preview(
        &LocalFiles,
        &m.env,
        &json!({ "kind": "mcp", "name": "x", "at": { "level": "user" } }),
    )
    .expect_err("用户级也给卸");
    assert_eq!(code, "refused");
    let _ = std::fs::remove_dir_all(&base);
}

/// 枢纽命令只剩一套：旧的两套外层命令名在仓里零出现（BUILD_ID 的子命令历史那张表是冻结的旧记录，照原样留）。
/// 正控：同一把尺子数得出新那一套。
#[test]
fn only_one_pair_of_hub_commands_is_left_in_the_repo() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let old: Vec<String> = ["skill-install", "mcp-sync"]
        .iter()
        .flat_map(|p| [format!("{p}-hub-"), format!("{}_hub_", p.replace('-', "_"))])
        .collect();
    let files = guard_core::scan_tree_excluding(
        &repo,
        &["rs", "ts", "md", "json", "py", "sh", "toml", "tsv"],
        &["tests/backend/build_id_guard.rs"],
    );
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, text)| old.iter().any(|n| text.contains(n.as_str())))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert_eq!(hits, Vec::<String>::new(), "旧的枢纽命令名还在");
    let new_pair = files
        .iter()
        .filter(|(_, text)| text.contains(concat!("ext-hub", "-preview")))
        .count();
    assert!(new_pair > 0, "正控没过：尺子数不出新那一套");
}

/// ★ 跨语言金样：一趟「装上 → 卸掉」的五份线上成品（表 · 确认卡 · 装完 · 卸之前那张卡 · 卸完）== `ext-flow.golden.json`；
/// 界面的解码器（`ext-reads.ts`）读同一份。临时目录那一截换成 `<BASE>`；只采结构（占位名、占位正文）。
#[tokio::test]
async fn the_wire_matches_the_cross_language_golden() {
    let (base, t) = two("golden");
    let src = t.a.env.skills.clone().unwrap().join("demo");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("SKILL.md"), "---\ndescription: d\n---\n").unwrap();
    let list = t.list();
    let (from, scope) = scope_of(t.cell(&list, "demo", 1).action.as_ref().unwrap());
    let args =
        json!({ "kind": "skill", "name": "demo", "from": from, "to": "gpd", "scope": scope });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .unwrap();
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let done = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .unwrap();
    let u = json!({ "kind": "skill", "name": "demo", "at": { "level": "user" } });
    let ucard = answer_uninstall_preview(&LocalFiles, &t.b.env, &u).unwrap();
    let mut ua = u.clone();
    ua["token"] = ucard["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    let udone = answer_uninstall_apply(&LocalFiles, &t.b.env, &record, &ua).unwrap();
    let got = json!({
        "list": serde_json::to_value(&list).unwrap(),
        "card": card,
        "done": done,
        "uninstallCard": ucard,
        "uninstallDone": udone,
    });
    let got: Value = serde_json::from_str(
        &got.to_string()
            .replace(&*base.display().to_string(), "<BASE>"),
    )
    .unwrap();
    let path = crate::guard_support::repo_root().join("tests/__fixtures__/ext-flow.golden.json");
    let want: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到金样 {path:?}：{e}")),
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(
        got["list"], want["list"],
        "表的线上形状变了 —— 真改了就重打金样（界面解码器读同一份）"
    );
    for k in ["card", "done", "uninstallCard", "uninstallDone"] {
        assert_eq!(got[k], want[k], "`{k}` 的线上形状变了");
    }
}
