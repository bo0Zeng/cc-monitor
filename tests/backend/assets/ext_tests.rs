//! 要求：设置「扩展」页 —— 各台目录合成「条目 × 机器」一张表、每格的点 · 那台上的各处 · 「装到…」能装到哪几处都由后端判（界面只画，线上没有摘要）；
//! 装 / 卸跨两台走同一对枢纽命令，装到哪由用户选（skill：全局 / 项目；MCP：只项目，全局由后端拒），看过之后源变了 ⇒ `stale`、目标零写；
//! 卸：cc-monitor 装的按装记录撤，不是的先备份再删；备注随目录同步到别的后端；cc-bus 是自带的一行，装它用被写那台二进制里那一份。
use super::*;
use crate::assets::aliases::tests::HomeDoor;
use crate::assets::asset_catalog::{Scanned, Snapshot, Visits};
use crate::assets::hub::{ext_apply, ext_preview, Here};
use crate::assets::mcp_sync::{Facts, There};
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
        notes: BTreeMap::new(),
        shared_mcp: false,
    }
}

fn at_s(l: &ExtLoc) -> &str {
    l.project().unwrap_or("user")
}

/// 一格压成一句好比的话：点 · 各处（态，`+卸` = 有卸载，`!` = 不能卸、说了为什么）· 「装到…」（来源 · 建议的那一处 · 各选项，`✗` = 不可选）或没有它的那一句。
fn brief(c: &ExtCell) -> String {
    let st = |s: ExtState| {
        serde_json::to_value(s)
            .unwrap()
            .as_str()
            .unwrap()
            .to_string()
    };
    let places: Vec<String> = c
        .places
        .iter()
        .map(|p| {
            let mark = if p.uninstall {
                "+卸"
            } else if p.note.is_some() {
                "!"
            } else {
                ""
            };
            format!("{}={}{mark}", at_s(&p.at), st(p.state))
        })
        .collect();
    let tail = match &c.bring {
        None => format!("note:{}", c.note.clone().unwrap_or_default()),
        Some(b) => format!(
            "bring<{}@{}>@{} [{}]",
            b.from.as_deref().unwrap_or("here"),
            at_s(&b.scope.from),
            at_s(&b.scope.to),
            b.targets
                .iter()
                .map(|t| format!("{}{}", at_s(&t.at), if t.ok { "" } else { "✗" }))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    };
    format!("{}:{} {tail}", st(c.state), places.join(","))
}

/// ★ 表格判据：持有人最多的那一版算「这一版」、打平时本机优先 · 只在项目里有 ⇒ `project` · 每台展开成各处（全局一行 ＋ 每个装着它的项目）·
/// 用户级 MCP 只读（那一处没有卸载、「装到…」里全局不可选）· 来源那一处不可选 · 建议的落点与来源同级、不行就挑第一个能选的 ·
/// 没连上的那台没有「装到…」也没有卸载 · MCP 那台没项目就说出来 · 自带的 cc-bus 永远有一行、只装全局 · 「新见到」只标上次来看之后第一次见到的。
#[test]
fn the_table_judges_every_cell_place_and_target() {
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
    let reach = vec![("devbox".to_string(), Some("r1".to_string()))];
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
            (Some("devbox".into()), false, true, "devbox".into()),
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
    let no_proj = format!("note:{}", n("beExt.note.noProject"));
    let want: Vec<(String, bool, Vec<String>)> = vec![
        (
            "m-proj".into(),
            false,
            vec![
                "project:user=missing,/h/p=same+卸 bring<here@/h/p>@/shared [user✗ /h/p✗ /shared]"
                    .into(),
                format!("missing:user=missing {no_proj}"),
                format!("missing:user=missing {off}"),
            ],
        ),
        (
            "m-user".into(),
            false,
            vec![
                "same:user=same! bring<here@user>@/h/p [user✗ /h/p /shared]".into(),
                format!("missing:user=missing {no_proj}"),
                format!("missing:user=missing {off}"),
            ],
        ),
        (
            "cc-bus".into(),
            false,
            vec![
                "missing:user=missing bring<here@user>@user [user /h/p✗ /shared✗]".into(),
                "missing:user=missing bring<here@user>@user [user]".into(),
                format!("missing:user=missing {off}"),
            ],
        ),
        (
            "s-major".into(),
            false,
            vec![
                "same:user=same+卸 bring<here@user>@/h/p [user✗ /h/p /shared]".into(),
                "same:user=same+卸 bring<here@user>@user [user]".into(),
                format!("differs:user=differs {off}"),
            ],
        ),
        (
            "s-proj".into(),
            true,
            vec![
                "missing:user=missing bring<devbox@/shared>@/shared [user /h/p /shared]".into(),
                "project:user=missing,/shared=same+卸 bring<devbox@/shared>@user [user]".into(),
                format!("missing:user=missing {off}"),
            ],
        ),
        (
            "s-tie".into(),
            false,
            vec![
                "same:user=same+卸 bring<here@user>@/h/p [user✗ /h/p /shared]".into(),
                "differs:user=differs+卸 bring<here@user>@user [user]".into(),
                format!("missing:user=missing {off}"),
            ],
        ),
    ];
    assert_eq!(got, want);
    // 不可选的那几项各说为什么（同一句话只住后端一处）。
    let m_user = &list.rows[1].cells[0];
    assert_eq!(m_user.places[0].note, Some(n("beExt.note.userMcpReadOnly")));
    let targets = &m_user.bring.as_ref().unwrap().targets;
    assert_eq!(targets[0].note, Some(n("beExt.note.userMcpReadOnly")));
    let cc = &list.rows[2];
    assert_eq!(
        cc.builtin,
        Some(ExtBuiltin {
            note: n("beExt.builtin.ccBus"),
            hooks: true
        })
    );
    assert!(list
        .rows
        .iter()
        .filter(|r| r.name != "cc-bus")
        .all(|r| r.builtin.is_none()));
    let cc_bring = cc.cells[0].bring.as_ref().unwrap();
    assert_eq!(cc_bring.from_name, n("beExt.from.builtin"));
    assert_eq!(
        cc_bring.targets[1].note,
        Some(n("beExt.target.builtinUserOnly"))
    );
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
    let same = format!("note:{}", copy_text("beExt.card.sameMachine", &[]));
    let got: Vec<(Value, String, Vec<String>)> = list
        .rows
        .iter()
        .filter(|r| r.builtin.is_none())
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
                vec![format!("project:user=missing,/h/p=same+卸 {same}")]
            ),
            (
                json!("skill"),
                "code-picture".to_string(),
                vec!["same:user=same+卸 bring<here@user>@/h/p [user✗ /h/p]".to_string()]
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
    /// 这台的门：家目录是这台的临时家（不碰真机）。
    fn door(&self) -> HomeDoor {
        HomeDoor(self.env.home.clone().unwrap())
    }
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
        let home = self.env.home.clone().unwrap().display().to_string();
        Scanned {
            assets,
            projects: self.projects(),
            problems,
            shared_mcp: crate::accounts::manage::mcp_share_exec::store_file_in(&home).is_some(),
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
            Some(p) => crate::agents::skill_root_at(
                crate::assets::asset_kind().unwrap(),
                Some(Path::new(p)),
            )
            .unwrap(),
            None => skills.clone(),
        };
        let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
        let r = match cmd {
            "skill-read" => super::super::skill_install::answer_read_at(Some(&root), args),
            "skill-install-plan" => {
                super::super::skill_install::answer_plan_with(&NoFacts, Some(&root), args)
            }
            "skill-install-apply" => super::super::skill_flow::answer_install(
                &self.door(),
                &NoFacts,
                Some(&root),
                &record,
                args,
            ),
            "mcp-sync-source" => super::super::mcp_sync_flow::answer_source(
                &self.door(),
                self.env.user_mcp.as_deref(),
                args,
            ),
            "mcp-sync-preview" => {
                super::super::mcp_sync_flow::answer_preview(&self.door(), &NoFacts, args)
            }
            "mcp-sync-apply" => {
                super::super::mcp_sync_flow::answer_apply(&self.door(), &record, args)
            }
            "cc-bus-install-state" => Ok(super::super::cc_bus_install::state_at(&skills)),
            "cc-bus-install" => {
                super::super::cc_bus_install::install_at(&self.door(), &skills, &record)
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
    two_with(tag, |_| {})
}

/// 同 [`two`]，被写的那台（b）在包成 `Arc` 之前先交 `prep` 改一改（比如给它建账号库）。
fn two_with(tag: &str, prep: impl FnOnce(&mut M)) -> (PathBuf, Two) {
    let base = temp(tag);
    let a = Arc::new(machine(&base, "a"));
    let mut b = machine(&base, "b");
    prep(&mut b);
    let b = Arc::new(b);
    let reach: Table = Mutex::new(BTreeMap::new());
    register(
        &reach,
        &json!({ "origin": "laptop", "dial": {"machine": {"host": "h", "port": 22, "user": "u", "keyPath": "/k"}} }),
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
            .get_mut("laptop")
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

/// 那一格「装到…」的来源与建议的那一处。
fn bring_of(c: &ExtCell) -> (Option<String>, ExtScope) {
    let b = c.bring.as_ref().expect("这一格没有「装到…」");
    (b.from.clone(), b.scope.clone())
}

/// 那一格里能卸的那一处（只许一处）。
fn removable(c: &ExtCell) -> ExtLoc {
    let at: Vec<&ExtPlace> = c.places.iter().filter(|p| p.uninstall).collect();
    assert_eq!(at.len(), 1, "该有且只有一处能卸：{c:?}");
    at[0].at.clone()
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
    assert_eq!(list.machines[1].key.as_deref(), Some("laptop"));
    assert_eq!(t.cell(&list, "demo", 0).state, ExtState::Same);
    let there = t.cell(&list, "demo", 1);
    assert_eq!(there.state, ExtState::Missing);
    let (from, scope) = bring_of(&there);
    assert_eq!(from, None, "来源是本机");
    assert_eq!(scope.to, ExtLoc::User, "来源在全局 ⇒ 建议装到全局");
    let args =
        json!({ "kind": "skill", "name": "demo", "from": from, "to": "laptop", "scope": scope });

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
    let at = removable(&there);

    // 卸在被卸那台上判、写（界面直问那台）：装记录里有 ⇒ 只撤装时写的。
    let u = json!({ "kind": "skill", "name": "demo", "at": at });
    let card = answer_uninstall_preview(&t.b.door(), &t.b.env, &u).expect("卸之前那张卡");
    assert_eq!(card["recorded"], true);
    assert_eq!(card["files"], json!(["SKILL.md", "lib/x.txt"]));
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    answer_uninstall_apply(&t.b.door(), &t.b.env, &record, &ua).expect("卸不掉");
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
    let (from, scope) = bring_of(&there);
    assert_eq!(
        scope.to,
        ExtLoc::Project {
            dir: t.b.proj.display().to_string()
        },
        "MCP 只装进项目：替那台先挑了它开过会话的那一个"
    );
    let args = json!({ "kind": "mcp", "name": "m", "from": from, "to": "laptop", "scope": scope });
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
    let at = removable(&there);
    let u = json!({ "kind": "mcp", "name": "m", "at": at });
    let card = answer_uninstall_preview(&t.b.door(), &t.b.env, &u).unwrap();
    assert_eq!(
        (card["recorded"].clone(), card["backup"].clone()),
        (json!(true), Value::Null)
    );
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    answer_uninstall_apply(&t.b.door(), &t.b.env, &record, &ua).expect("卸不掉");
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

/// ★ 装到哪由用户选（正）：本机全局里的一个 skill，在远端那一格「装到…」里全局与那台的项目都能选；选项目 ⇒ 写进那个项目的
/// `.claude/skills/<名>/`，那一格变 ◎、那一处可卸，全局那一处照旧「没有」。
#[tokio::test]
async fn a_user_level_skill_goes_into_a_project_on_the_other_machine() {
    let (base, t) = two("into-project");
    let src = t.a.env.skills.clone().unwrap().join("demo");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("SKILL.md"), "---\ndescription: d\n---\n").unwrap();
    let list = t.list();
    let there = t.cell(&list, "demo", 1);
    let b = there.bring.clone().expect("缺的那一格没有「装到…」");
    let proj = ExtLoc::Project {
        dir: t.b.proj.display().to_string(),
    };
    assert_eq!(
        b.targets
            .iter()
            .map(|x| (x.at.clone(), x.ok))
            .collect::<Vec<_>>(),
        vec![(ExtLoc::User, true), (proj.clone(), true)],
        "skill：全局与那台开过会话的项目都能选"
    );
    let args = json!({ "kind": "skill", "name": "demo", "from": b.from, "to": "laptop",
        "scope": { "from": b.scope.from, "to": proj } });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("看卡");
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect("装不进项目");
    assert!(t.b.proj.join(".claude/skills/demo/SKILL.md").is_file());
    assert!(
        !t.b.env.skills.clone().unwrap().join("demo").exists(),
        "选的是项目，却写进了全局"
    );
    let there = t.cell(&t.list(), "demo", 1);
    assert_eq!(there.state, ExtState::Project);
    assert_eq!(
        brief(&there).split(' ').next().unwrap(),
        format!("project:user=missing,{}=same+卸", t.b.proj.display())
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ MCP 选全局 ⇒ 后端拒（反）：卡上那一项显示但不可选；就算界面硬交上来，枢纽也在发出任何一跳之前拒掉（来源与被写那台都没被问）。
/// 自带的 cc-bus 选项目同样拒。
#[tokio::test]
async fn the_hub_refuses_a_target_the_table_greys_out() {
    let (base, t) = two("refuse-user");
    std::fs::write(
        t.a.proj.join(".mcp.json"),
        json!({ "mcpServers": { "m": { "command": "srv" } } }).to_string(),
    )
    .unwrap();
    let list = t.list();
    let b = t
        .cell(&list, "m", 1)
        .bring
        .expect("那台有项目，该有「装到…」");
    assert_eq!(
        (
            b.targets[0].at.clone(),
            b.targets[0].ok,
            b.targets[0].note.clone()
        ),
        (
            ExtLoc::User,
            false,
            Some(copy_text("beExt.note.userMcpReadOnly", &[]))
        )
    );
    let user = json!({ "level": "user" });
    for (kind, name, to, why) in [
        ("mcp", "m", user.clone(), "beExt.note.userMcpReadOnly"),
        (
            "skill",
            "cc-bus",
            json!({ "level": "project", "dir": t.b.proj.display().to_string() }),
            "beExt.target.builtinUserOnly",
        ),
    ] {
        let args = json!({ "kind": kind, "name": name, "from": b.from, "to": "laptop",
            "scope": { "from": b.scope.from, "to": to }, "tokens": { "source": "s", "target": null } });
        let (code, said) = ext_preview(&t.here, &args, &t.reach, &t.remote)
            .await
            .expect_err("不可选的那一处也给看卡了");
        assert_eq!((code.as_str(), said), ("refused", copy_text(why, &[])));
        let (code, _) = ext_apply(&t.here, &args, &t.reach, &t.remote)
            .await
            .expect_err("不可选的那一处也给装了");
        assert_eq!(code, "refused");
    }
    assert!(
        !t.remote
            .1
            .lock()
            .unwrap()
            .iter()
            .any(|c| c == "mcp-sync-apply" || c.starts_with("cc-bus-install")),
        "拒了还叫那台去写 / 去看 cc-bus"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// 在这台的临时家目录里经账号库那几条帧命令建库（z）、加一个号（b）；之后它的用户级 MCP 就是各账号共用的那一份。
/// 夹具只造结构：登录状态是写死的假邮箱。
#[cfg(unix)]
fn make_library(m: &mut M) {
    use crate::accounts::upstream_select::file_face;
    use crate::faces::accounts_face::{answer, KeyDoor};
    let home = m.env.home.clone().unwrap();
    let w = |rel: &str, body: &str| {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    w(".claude/.credentials.json", "{\"fake\":\"cred\"}");
    w(".claude/settings.json", "{}");
    w(
        ".claude.json",
        &json!({ "oauthAccount": { "emailAddress": "z@example.test" }, "mcpServers": {} })
            .to_string(),
    );
    let table = home.join(".cc-monitor/apikey-credentials.json");
    let set = |v: &Value| file_face::answer_set_at(&table, v);
    let drop = |v: &Value| file_face::answer_drop_at(&table, v);
    let restore = |v: &Value| file_face::answer_restore_at(&table, v);
    let path = || Ok(table.clone());
    let keys = KeyDoor {
        set: &set,
        drop: &drop,
        restore: &restore,
        path: &path,
    };
    let d = HomeDoor(home.clone());
    let init = answer(&d, "accounts-init", &json!({ "name": "z" }), &keys).expect("建库");
    assert_eq!(init["applied"], true, "{init}");
    answer(
        &d,
        "accounts-add",
        &json!({ "name": "b", "kind": "subscription" }),
        &keys,
    )
    .expect("加号");
    let store = crate::accounts::manage::mcp_share_exec::store_file_in(&home.display().to_string())
        .expect("建了库却没有共用的那一份");
    m.env.user_mcp = Some(PathBuf::from(store));
}

/// 各账号共用的那一份里此刻有哪几个名字（经帧命令那一侧读，异源于扩展页）。
#[cfg(unix)]
fn shared_names(m: &M) -> Vec<String> {
    crate::accounts::manage::mcp_share_exec::read(&m.door())
        .expect("读共用的那一份")
        .servers
}

/// ★ 有账号库的那台：MCP「装到全局」能选；装 ⇒ 写进那台各账号共用的那一份（所有号都有）、卡上说清删除只有一条路与什么时候用上；
/// 从全局卸 ⇒ 共用的那一份里没了。没有账号库的那台全局不可选由 `the_hub_refuses_a_target_the_table_greys_out` 钉。
#[cfg(unix)]
#[tokio::test]
async fn an_mcp_entry_goes_global_into_the_shared_set_and_comes_back_off() {
    let (base, t) = two_with("mcp-global", make_library);
    std::fs::write(
        t.a.proj.join(".mcp.json"),
        json!({ "mcpServers": { "m": { "command": "srv", "env": { "KEY": "secret-A" } } } })
            .to_string(),
    )
    .unwrap();
    let list = t.list();
    let row = list
        .rows
        .iter()
        .find(|r| r.name == "m")
        .expect("表里没有 m");
    let there = row.cells[1].bring.clone().expect("有库的那台该有「装到…」");
    assert_eq!(
        (there.targets[0].at.clone(), there.targets[0].ok),
        (ExtLoc::User, true),
        "有账号库的那台：全局可选"
    );
    let args = json!({ "kind": "mcp", "name": "m", "from": there.from, "to": "laptop",
        "scope": { "from": there.scope.from, "to": { "level": "user" } } });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("看卡");
    let suspects = card["suspects"].to_string();
    for k in [
        "beExt.card.sharedDeleteHere",
        "beExt.card.sharedNewSessions",
    ] {
        assert!(
            suspects.contains(&copy_text(k, &[])),
            "卡上缺一句 {k}：{suspects}"
        );
    }
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    apply["fill"] = json!({ "env": { "KEY": "typed-B" } });
    ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect("装不到全局");
    assert_eq!(
        shared_names(&t.b),
        vec!["m".to_string()],
        "共用的那一份里没有它"
    );
    let cell = t.cell(&t.list(), "m", 1);
    assert_eq!(cell.places[0].at, ExtLoc::User);
    assert!(cell.places[0].uninstall, "全局那一处该能卸：{cell:?}");

    let u = json!({ "kind": "mcp", "name": "m", "at": { "level": "user" } });
    let ucard = answer_uninstall_preview(&t.b.door(), &t.b.env, &u).expect("卸之前那张卡");
    assert_eq!(
        ucard["said"],
        copy_text("beExt.uninstall.mcpShared", &[("name", "m")])
    );
    let mut ua = u.clone();
    ua["token"] = ucard["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    answer_uninstall_apply(&t.b.door(), &t.b.env, &record, &ua).expect("从全局卸不掉");
    assert_eq!(
        shared_names(&t.b),
        Vec::<String>::new(),
        "卸完共用的那一份里还有它"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ 备注随目录同步：一台上写的备注记在它自己那一格，整份并进另一台之后那一行带着它；另一台后来改的（`rev` 更大）压过前一条；
/// 清掉（空串）也照样传过去。
#[test]
fn a_note_travels_with_the_catalog_to_the_other_backend() {
    let (base, t) = two("note");
    let key = asset_catalog::entry_key(KIND_SKILL, "cc-bus");
    let (ca, changed) =
        asset_catalog::update_noting(&t.a.cat, t.a.scan(), "a", (&key, "  先装钩子  "), 101)
            .unwrap();
    assert!(changed);
    let row = |cat: &Catalog| {
        table(cat, &[])
            .rows
            .into_iter()
            .find(|r| r.name == "cc-bus")
            .unwrap()
            .note
    };
    assert_eq!(row(&ca), Some("先装钩子".to_string()));
    let cb = t.b.refresh(Some(ca.machines.clone()));
    assert_eq!(
        row(&cb),
        Some("先装钩子".to_string()),
        "备注没随目录到另一台"
    );
    let (cb, _) =
        asset_catalog::update_noting(&t.b.cat, t.b.scan(), "b", (&key, "改过了"), 102).unwrap();
    let ca = t.a.refresh(Some(cb.machines.clone()));
    assert_eq!(
        row(&ca),
        Some("改过了".to_string()),
        "后写的那一条没压过前一条"
    );
    let (ca, _) = asset_catalog::update_noting(&t.a.cat, t.a.scan(), "a", (&key, ""), 103).unwrap();
    assert_eq!(row(&ca), None);
    let cb = t.b.refresh(Some(ca.machines.clone()));
    assert_eq!(row(&cb), None, "清掉的那一下没传过去");
    // 同样的字再存一次：目录不变（不扇出）。
    let (_, again) =
        asset_catalog::update_noting(&t.a.cat, t.a.scan(), "a", (&key, ""), 104).unwrap();
    assert!(!again);
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ cc-bus 是自带的一行：没人装过也在表上；「装到…」交被写那台用它自己二进制里那一份装（不从别的机器拿）；
/// 装完那一格 ●（与本机后端带的那一版相同）、全局那一处可卸（装记录里有）。
#[tokio::test]
async fn cc_bus_installs_from_the_target_binary_and_shows_as_this_version() {
    let (base, t) = two("cc-bus");
    let list = t.list();
    let there = t.cell(&list, "cc-bus", 1);
    assert_eq!(there.state, ExtState::Missing);
    let (from, scope) = bring_of(&there);
    let args =
        json!({ "kind": "skill", "name": "cc-bus", "from": from, "to": "laptop", "scope": scope });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .expect("看卡");
    assert_eq!(card["unchanged"], false);
    assert_eq!(card["suspects"], json!([]), "那台原来没有 ⇒ 不用备份");
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let done = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .expect("装不上");
    assert_eq!(done["changed"], card["writes"]);
    assert_eq!(
        t.remote.1.lock().unwrap().clone(),
        vec![
            "cc-bus-install-state",
            "cc-bus-install-state",
            "cc-bus-install"
        ],
        "自带的那一个只问被写那台"
    );
    let there = t.cell(&t.list(), "cc-bus", 1);
    assert_eq!(there.state, ExtState::Same, "装完那一格没变 ●");
    assert_eq!(removable(&there), ExtLoc::User);
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .unwrap();
    assert_eq!(card["unchanged"], true, "装好之后再看：没有要写的");
    let _ = std::fs::remove_dir_all(&base);
}

/// ★ 扫描判据：全仓生产代码里提到 agent 设置文件（`settings.json`）的那几份 == 登记的只读那几份，而且它们一个写盘的写法都没有。
/// 正控：同一把尺子认得出一段现造的「往 settings.json 写」，也认得出一份真在写盘的模块。
#[test]
fn no_production_code_writes_the_agent_settings_file() {
    const NEEDLE: &str = "settings.json";
    const WRITES: &[&str] = &[
        "fs::write",
        "File::create",
        "OpenOptions",
        "write_all",
        "door::put",
        "files-put",
        "fs::rename",
        "door::rename",
    ];
    const READ_ONLY: &[&str] = &[
        // 每号各一份的那张表里有 `remote-settings.json`（字样同针，不是那份设置文件）。
        "src/backend/agents/claudecode/accounts.rs",
        "src/backend/agents/claudecode/footprint.rs",
        "src/backend/agents/claudecode/paths.rs",
    ];
    let code = |text: &str| guard_core::strip_comment_lines(text);
    let writes = |text: &str| WRITES.iter().any(|w| text.contains(w));
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let files: Vec<(PathBuf, String)> =
        guard_core::scan_tree_excluding(&repo.join("src"), &["rs", "ts"], &[])
            .into_iter()
            .filter(|(p, _)| {
                !p.to_string_lossy()
                    .replace('\\', "/")
                    .contains("/src/vendor/")
            })
            .collect();
    let mut seen: Vec<String> = files
        .iter()
        .filter(|(_, text)| code(text).contains(NEEDLE))
        .map(|(p, _)| {
            p.strip_prefix(&repo)
                .unwrap_or(p)
                .display()
                .to_string()
                .replace('\\', "/")
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen, READ_ONLY,
        "提到 settings.json 的生产代码多了 / 少了一份"
    );
    for (p, text) in &files {
        if code(text).contains(NEEDLE) {
            assert!(
                !writes(&code(text)),
                "{} 提到 settings.json 又有写盘的写法",
                p.display()
            );
        }
    }
    // 正控：现造一段往 settings.json 写的代码 ⇒ 两条都认得出。
    let fake =
        "let p = home.join(\".claude/settings.json\");\nstd::fs::write(&p, b\"{}\").unwrap();";
    assert!(code(fake).contains(NEEDLE) && writes(&code(fake)));
    let writer = files
        .iter()
        .find(|(p, _)| p.ends_with("assets/asset_catalog.rs"))
        .expect("正控：那份写目录文件的模块不在扫描范围里");
    assert!(
        writes(&code(&writer.1)),
        "正控：写法表认不出一份真在写盘的模块"
    );
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
    let card = answer_uninstall_preview(&m.door(), &m.env, &u).unwrap();
    assert_eq!(card["recorded"], false);
    assert_eq!(
        std::path::Path::new(card["backup"].as_str().unwrap()).file_name(),
        Some(std::ffi::OsStr::new("backups")),
        "{card}"
    );
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    std::fs::write(dir.join("SKILL.md"), "edited\n").unwrap();
    let (code, _) =
        answer_uninstall_apply(&m.door(), &m.env, &record, &ua).expect_err("变了还删了");
    assert_eq!(code, "stale");
    assert!(dir.join("SKILL.md").exists());
    let card = answer_uninstall_preview(&m.door(), &m.env, &u).unwrap();
    ua["token"] = card["token"].clone();
    let done = answer_uninstall_apply(&m.door(), &m.env, &record, &ua).expect("卸不掉");
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
    let card = answer_uninstall_preview(&m.door(), &m.env, &u).unwrap();
    assert_eq!(card["recorded"], false);
    let mut ua = u.clone();
    ua["token"] = card["token"].clone();
    let done = answer_uninstall_apply(&m.door(), &m.env, &record, &ua).expect("卸不掉");
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
    // 这台没建账号库：用户级 MCP 只读，卸也拒。
    let (code, _) = answer_uninstall_preview(
        &m.door(),
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
    let (from, scope) = bring_of(&t.cell(&list, "demo", 1));
    let args =
        json!({ "kind": "skill", "name": "demo", "from": from, "to": "laptop", "scope": scope });
    let card = ext_preview(&t.here, &args, &t.reach, &t.remote)
        .await
        .unwrap();
    let mut apply = args.clone();
    apply["tokens"] = card["tokens"].clone();
    let done = ext_apply(&t.here, &apply, &t.reach, &t.remote)
        .await
        .unwrap();
    let u = json!({ "kind": "skill", "name": "demo", "at": { "level": "user" } });
    let ucard = answer_uninstall_preview(&t.b.door(), &t.b.env, &u).unwrap();
    let mut ua = u.clone();
    ua["token"] = ucard["token"].clone();
    let ledger = t.b.env.ledger.clone().unwrap();
    let skills = t.b.env.skills.clone().unwrap();
    let record = |a: &Value| super::super::skill_ledger::record_at(&ledger, Some(&skills), a);
    let udone = answer_uninstall_apply(&t.b.door(), &t.b.env, &record, &ua).unwrap();
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
