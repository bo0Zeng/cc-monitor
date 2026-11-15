//! 要求：MCP 配置里像密钥的值不在机器之间复制 —— 确认卡上是待填或「沿用目标机已有的值」，值出不了来源机、缺了就拒；
//! 写一趟不重算：看过之后被写那份变了 ⇒ `stale`、一个字节不写；用户级 MCP 只读。
use super::*;
use crate::assets::aliases::test_support::FilesHere;
use crate::assets::mcp_sync::There;
use serde_json::json;
use std::cell::RefCell;

/// 判据仍用 JSON 写入参：经 `typed::take` 解成入参类型（同帧面那一跳），回包序列化回 JSON 比。
fn via<R: serde::de::DeserializeOwned, T: serde::Serialize>(
    args: &Value,
    f: impl FnOnce(&R) -> Result<T, crate::common::said::Said>,
) -> Result<Value, crate::common::said::Said> {
    let req = crate::wire::typed::take::<R>(args)?;
    f(&req).map(|t| serde_json::to_value(t).unwrap())
}

fn answer_source(
    d: &dyn crate::common::door::Door,
    user_mcp: Option<&std::path::Path>,
    args: &Value,
) -> Result<Value, crate::common::said::Said> {
    via(args, |r| super::answer_source(d, user_mcp, r))
}

fn answer_preview(
    d: &dyn crate::common::door::Door,
    facts: &dyn crate::assets::mcp_sync::Facts,
    args: &Value,
) -> Result<Value, crate::common::said::Said> {
    via(args, |r| super::answer_preview(d, facts, r))
}

fn answer_apply(
    d: &dyn crate::common::door::Door,
    record: Record,
    args: &Value,
) -> Result<Value, crate::common::said::Said> {
    via(args, |r| super::answer_apply(d, record, r))
}

/// 默认那一家的项目级 MCP 文件（生产注册表上：Claude 的 `.mcp.json`）。
fn default_project_mcp_file() -> &'static str {
    project_mcp_of(REGISTRY, None)
        .expect("默认那一家没有项目级 MCP")
        .0
}

/// 这台上的事实：`PATH` 查不动（`unknown`），路径一律「没有」—— 判据不把开发机烤进去。
struct NoFacts;
impl Facts for NoFacts {
    fn path(&self, _p: &str) -> There {
        There::Absent
    }
    fn command(&self, _name: &str) -> Option<bool> {
        None
    }
}

fn fixture(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ext-mcpflow-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join("dst")).unwrap();
    d
}

fn at(dir: &std::path::Path) -> Value {
    json!({ "level": "project", "dir": dir.display().to_string() })
}

/// 一条带密钥的定义里，所有装在 `env` / `headers` 的值（字符串的叶子）。
fn secret_values(def: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for field in MCP_KEYS_ONLY_FIELDS {
        if let Some(m) = def.get(*field).and_then(Value::as_object) {
            out.extend(m.values().filter_map(Value::as_str).map(str::to_string));
        }
    }
    out
}

/// ★ 来源机交出来的那一条里，`env` / `headers` 的值全是空位；整份应答里一个密钥值都找不到（正控：原文里确有这几个值）。
#[test]
fn the_source_hands_over_the_entry_with_every_secret_value_blanked() {
    let d = fixture("redact");
    let def = json!({ "command": "srv", "args": ["--port", "1"], "env": { "API_KEY": "sk-AAA", "MODE": "mode-VAL" }, "headers": { "Authorization": "Bearer BBB" } });
    std::fs::write(
        d.join("src").join(default_project_mcp_file()),
        json!({ "mcpServers": { "s": def, "other": { "command": "o" } } }).to_string(),
    )
    .unwrap();
    let got = answer_source(
        &FilesHere,
        None,
        &json!({ "name": "s", "at": at(&d.join("src")) }),
    )
    .expect("来源读不出");
    let secrets = secret_values(&def);
    assert_eq!(secrets.len(), 3, "正控：原文里该有三个值");
    let wire = got.to_string();
    for v in &secrets {
        assert!(!wire.contains(v.as_str()), "密钥值 {v} 出了来源机：{wire}");
    }
    assert_eq!(
        got["def"],
        json!({ "command": "srv", "args": ["--port", "1"], "env": { "API_KEY": null, "MODE": null }, "headers": { "Authorization": null } })
    );
    assert_eq!(
        got["slots"],
        json!([{ "field": "env", "key": "API_KEY" }, { "field": "env", "key": "MODE" }, { "field": "headers", "key": "Authorization" }])
    );
    // 记号按原样算：只改一个密钥值也换记号（应用时判得出 stale）。
    let mut changed = def.clone();
    changed["env"]["API_KEY"] = json!("sk-CCC");
    std::fs::write(
        d.join("src").join(default_project_mcp_file()),
        json!({ "mcpServers": { "s": changed } }).to_string(),
    )
    .unwrap();
    let again = answer_source(
        &FilesHere,
        None,
        &json!({ "name": "s", "at": at(&d.join("src")) }),
    )
    .unwrap();
    assert_ne!(again["token"], got["token"]);
    assert_eq!(again["def"], got["def"]);
    let _ = std::fs::remove_dir_all(&d);
}

/// 被写那台补空位：填了的用填的、没填的沿用这台原有的、两样都没有 ⇒ `needs_input` 一个字节不写；写完记进装记录。
#[test]
fn the_target_fills_each_slot_from_the_card_or_its_own_value_and_never_from_the_source() {
    let d = fixture("fill");
    let dst = d.join("dst");
    std::fs::write(
        dst.join(default_project_mcp_file()),
        json!({ "mcpServers": { "s": { "command": "old", "env": { "API_KEY": "mine" } }, "keep": { "command": "k" } } }).to_string(),
    )
    .unwrap();
    let def = json!({ "command": "srv", "env": { "API_KEY": null, "TOKEN": null } });
    let pre = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": "s", "at": at(&dst), "def": def }),
    )
    .unwrap();
    assert_eq!(pre["state"], "differs");
    assert_eq!(
        pre["slots"],
        json!([{ "field": "env", "key": "API_KEY", "kept": true }, { "field": "env", "key": "TOKEN", "kept": false }])
    );
    let recorded = RefCell::new(Vec::new());
    let record = |a: &crate::assets::skill_ledger::RecordReq| {
        recorded.borrow_mut().push(serde_json::to_value(a).unwrap());
        Ok(Default::default())
    };
    let before = std::fs::read_to_string(dst.join(default_project_mcp_file())).unwrap();
    let code = answer_apply(
        &FilesHere,
        &record,
        &json!({ "name": "s", "at": at(&dst), "def": def, "fill": {}, "target": pre["target"] }),
    )
    .expect_err("缺值还写了")
    .code;
    assert_eq!(code.as_deref(), Some("needs_input"));
    assert_eq!(
        std::fs::read_to_string(dst.join(default_project_mcp_file())).unwrap(),
        before
    );
    let done = answer_apply(
        &FilesHere,
        &record,
        &json!({ "name": "s", "at": at(&dst), "def": def, "fill": { "env": { "TOKEN": "typed" } }, "target": pre["target"] }),
    )
    .expect("写不上");
    assert_eq!(done["written"], true);
    let after: Value = serde_json::from_str(
        &std::fs::read_to_string(dst.join(default_project_mcp_file())).unwrap(),
    )
    .unwrap();
    assert_eq!(
        after["mcpServers"],
        json!({ "s": { "command": "srv", "env": { "API_KEY": "mine", "TOKEN": "typed" } }, "keep": { "command": "k" } })
    );
    let rec = recorded.borrow();
    assert_eq!(rec.len(), 1);
    assert_eq!(rec[0]["op"], "mcp-add");
    assert_eq!(
        rec[0]["digest"],
        json!(crate::assets::skill_ledger::mcp_digest(
            &after["mcpServers"]["s"]
        ))
    );
    // 交来的定义里夹着值 ⇒ 拒（值只许从卡上或这台来）
    let code = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": "s", "at": at(&dst), "def": { "command": "x", "env": { "API_KEY": "leak" } } }),
    )
    .expect_err("带值的定义也收了")
        .code;
    assert_eq!(code.as_deref(), Some("bad_args"));
    let _ = std::fs::remove_dir_all(&d);
}

/// 看过之后被写那份变了 ⇒ `stale`、一个字节不写（不重读重算）；用户级 ⇒ 只读、拒。
#[test]
fn a_target_that_changed_after_the_preview_is_left_alone_and_user_level_is_read_only() {
    let d = fixture("stale");
    let dst = d.join("dst");
    std::fs::write(dst.join(default_project_mcp_file()), "{\"mcpServers\":{}}").unwrap();
    let def = json!({ "command": "a" });
    let pre = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": "a", "at": at(&dst), "def": def }),
    )
    .unwrap();
    std::fs::write(
        dst.join(default_project_mcp_file()),
        "{\"mcpServers\":{\"z\":{}}}",
    )
    .unwrap();
    let record = |_: &crate::assets::skill_ledger::RecordReq| Ok(Default::default());
    let code = answer_apply(
        &FilesHere,
        &record,
        &json!({ "name": "a", "at": at(&dst), "def": def, "target": pre["target"] }),
    )
    .expect_err("看过之后变了还写了")
    .code;
    assert_eq!(code.as_deref(), Some("stale"));
    assert_eq!(
        std::fs::read_to_string(dst.join(default_project_mcp_file())).unwrap(),
        "{\"mcpServers\":{\"z\":{}}}"
    );
    // 这台没建账号库（家目录是临时目录）⇒ 用户级只读。
    let code = answer_preview(
        &crate::assets::aliases::test_support::HomeDoor(d.clone()),
        &NoFacts,
        &json!({ "name": "a", "at": { "level": "user" }, "def": def }),
    )
    .expect_err("用户级也收了")
    .code;
    assert_eq!(code.as_deref(), Some("refused"));
    let _ = std::fs::remove_dir_all(&d);
}

/// 只有替身家一家（默认起的就是它）：项目级 MCP 住 `.fake-mcp.json`、行式格式（假 agent 的 `McpCells`）。
fn lines_family() -> &'static [crate::agents::Adapter] {
    static ONE: std::sync::OnceLock<Vec<crate::agents::Adapter>> = std::sync::OnceLock::new();
    ONE.get_or_init(|| {
        let mut row = crate::agents::fake::full_row(|| None);
        row.launch = row.launch.map(|l| crate::agents::LaunchFace {
            is_default: true,
            ..l
        });
        vec![row]
    })
}

/// ★ 推 / 拉项目级 MCP 照那一家的文件与格式：来源那台从行式配置里读那一条，被写那台只换那一行，别的行与写法一字不变；
/// 不写成 JSON、不另起别的文件。
#[test]
fn pushing_a_project_mcp_reads_and_writes_the_familys_own_file_and_format() {
    let d = fixture("lines");
    let (src, dst) = (d.join("src"), d.join("dst"));
    std::fs::write(
        src.join(".fake-mcp.json"),
        "# 来源\nserver s {\"command\":\"srv\",\"env\":{\"K\":\"v\"}}\nserver o {\"command\":\"o\"}\n",
    )
    .unwrap();
    let mine = "# 我的设置\nserver s {\"command\":\"old\",\"env\":{\"K\":\"mine\"}}\nlevel 3\nserver keep {\"command\":\"k\"}\n";
    std::fs::write(dst.join(".fake-mcp.json"), mine).unwrap();
    let reg = lines_family();
    let got = via(&json!({ "name": "s", "at": at(&src) }), |r| {
        source_among(&FilesHere, reg, None, r)
    })
    .expect("来源那份行式配置读不出");
    assert_eq!(
        got["def"],
        json!({ "command": "srv", "env": { "K": null } })
    );
    let pre = via(
        &json!({ "name": "s", "at": at(&dst), "def": got["def"] }),
        |r| preview_among(&FilesHere, reg, &NoFacts, r),
    )
    .expect("被写那份行式配置读不出");
    assert_eq!(
        (pre["state"].clone(), pre["path"].clone()),
        (
            json!("differs"),
            json!(dst.join(".fake-mcp.json").display().to_string())
        )
    );
    let record = |_: &crate::assets::skill_ledger::RecordReq| Ok(Default::default());
    let done = via(
        &json!({ "name": "s", "at": at(&dst), "def": got["def"], "target": pre["target"] }),
        |r| apply_among(&FilesHere, reg, &record, r),
    )
    .expect("写不上");
    assert_eq!(done["written"], true);
    assert_eq!(
        std::fs::read_to_string(dst.join(".fake-mcp.json")).unwrap(),
        "# 我的设置\nserver s {\"command\":\"srv\",\"env\":{\"K\":\"mine\"}}\nlevel 3\nserver keep {\"command\":\"k\"}\n",
        "被写那份不是只换了那一行"
    );
    assert!(!dst.join(".mcp.json").exists(), "照别家的文件名另起了一份");
    // 被写那台还没有那份 ⇒ 照那一家的格式新建，只有那一行。
    let fresh = d.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    let pre = via(
        &json!({ "name": "s", "at": at(&fresh), "def": got["def"] }),
        |r| preview_among(&FilesHere, reg, &NoFacts, r),
    )
    .unwrap();
    assert_eq!(pre["state"], "new");
    via(
        &json!({ "name": "s", "at": at(&fresh), "def": got["def"], "fill": { "env": { "K": "typed" } }, "target": pre["target"] }),
        |r| apply_among(&FilesHere, reg, &record, r),
    )
    .expect("新建写不上");
    assert_eq!(
        std::fs::read_to_string(fresh.join(".fake-mcp.json")).unwrap(),
        "server s {\"command\":\"srv\",\"env\":{\"K\":\"typed\"}}\n",
        "新建那份不是那一家格式的那一行"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 两个替身家（`fake` 默认 · `fake2`）：两家同一种行式格式、项目级文件各一份（`.fake-mcp.json` · `fake2-mcp.conf`）。
fn two() -> &'static [crate::agents::Adapter] {
    crate::agents::fake::two_families()
}

/// 推 · 拉一整趟（来源 → 看 → 写），请求里的家照 `agent` 带（`None` ⇒ 不带那一格）。交回写成了吗。
fn push(
    reg: &[crate::agents::Adapter],
    src: &std::path::Path,
    dst: &std::path::Path,
    agent: Option<&str>,
) -> Result<Value, crate::common::said::Said> {
    let with = |mut v: Value| {
        if let Some(a) = agent {
            v["agent"] = json!(a);
        }
        v
    };
    let got = via(&with(json!({ "name": "s", "at": at(src) })), |r| {
        source_among(&FilesHere, reg, None, r)
    })?;
    let pre = via(
        &with(json!({ "name": "s", "at": at(dst), "def": got["def"] })),
        |r| preview_among(&FilesHere, reg, &NoFacts, r),
    )?;
    let record = |_: &crate::assets::skill_ledger::RecordReq| Ok(Default::default());
    via(
        &with(json!({ "name": "s", "at": at(dst), "def": got["def"], "target": pre["target"] })),
        |r| apply_among(&FilesHere, reg, &record, r),
    )
}

/// ★ 推 · 拉照请求里的家：指名第二家 ⇒ 读的 · 改的都是第二家那份行式项目配置，默认那一家那份一字不变；
/// 不带 `agent` ⇒ 照旧改默认那一家那份，第二家那份一字不变。
#[test]
fn pushing_a_project_mcp_follows_the_family_named_in_the_request() {
    let d = fixture("fam");
    let (src, dst) = (d.join("src"), d.join("dst"));
    std::fs::write(
        src.join(".fake-mcp.json"),
        "server s {\"command\":\"from-default\"}\n",
    )
    .unwrap();
    std::fs::write(
        src.join("fake2-mcp.conf"),
        "server s {\"command\":\"from-second\"}\n",
    )
    .unwrap();
    let mine_a = "# 默认那一家\nserver keep {\"command\":\"a\"}\n";
    let mine_b = "# 第二家\nserver keep {\"command\":\"b\"}\n";
    std::fs::write(dst.join(".fake-mcp.json"), mine_a).unwrap();
    std::fs::write(dst.join("fake2-mcp.conf"), mine_b).unwrap();

    let done = push(two(), &src, &dst, Some("fake2")).expect("指名第二家推不过去");
    assert_eq!(
        (done["written"].clone(), done["path"].clone()),
        (
            json!(true),
            json!(dst.join("fake2-mcp.conf").display().to_string())
        )
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("fake2-mcp.conf")).unwrap(),
        "# 第二家\nserver keep {\"command\":\"b\"}\nserver s {\"command\":\"from-second\"}\n",
        "第二家那份不是只加了第二家来源的那一行"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join(".fake-mcp.json")).unwrap(),
        mine_a,
        "指名第二家却动了默认那一家那份"
    );

    std::fs::write(dst.join("fake2-mcp.conf"), mine_b).unwrap();
    push(two(), &src, &dst, None).expect("不带家推不过去");
    assert_eq!(
        std::fs::read_to_string(dst.join(".fake-mcp.json")).unwrap(),
        "# 默认那一家\nserver keep {\"command\":\"a\"}\nserver s {\"command\":\"from-default\"}\n",
        "不带家没照旧改默认那一家那份"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("fake2-mcp.conf")).unwrap(),
        mine_b,
        "不带家却动了第二家那份"
    );

    // 指名默认那一家 ＝ 不带家。
    std::fs::write(dst.join(".fake-mcp.json"), mine_a).unwrap();
    push(two(), &src, &dst, Some("fake")).expect("指名默认那一家推不过去");
    assert_eq!(
        std::fs::read_to_string(dst.join(".fake-mcp.json")).unwrap(),
        "# 默认那一家\nserver keep {\"command\":\"a\"}\nserver s {\"command\":\"from-default\"}\n",
        "指名默认那一家没改默认那一家那份"
    );
    assert_eq!(
        std::fs::read_to_string(dst.join("fake2-mcp.conf")).unwrap(),
        mine_b,
        "指名默认那一家却动了第二家那份"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 认不出的家 ⇒ 来源 · 看 · 写三条都 `bad_args`，不退回默认那一家、一个字节不写。
#[test]
fn an_unknown_family_in_a_push_is_bad_args_on_every_hop() {
    let d = fixture("ghost");
    let (src, dst) = (d.join("src"), d.join("dst"));
    std::fs::write(src.join(".fake-mcp.json"), "server s {\"command\":\"c\"}\n").unwrap();
    let mine = "server keep {\"command\":\"a\"}\n";
    std::fs::write(dst.join(".fake-mcp.json"), mine).unwrap();
    let reg = two();
    // 拒的是「认不出这一家」（话里点名那一家），不是入参多了一格。
    let code = |r: Result<Value, crate::common::said::Said>| {
        let e = r.expect_err("认不出的家也收了");
        assert!(format!("{e:?}").contains("ghost"), "拒的不是那一家：{e:?}");
        e.code
    };
    assert_eq!(
        code(via(
            &json!({ "name": "s", "at": at(&src), "agent": "ghost" }),
            |r| source_among(&FilesHere, reg, None, r)
        ))
        .as_deref(),
        Some("bad_args")
    );
    let def = json!({ "command": "c" });
    assert_eq!(
        code(via(
            &json!({ "name": "s", "at": at(&dst), "def": def, "agent": "ghost" }),
            |r| preview_among(&FilesHere, reg, &NoFacts, r)
        ))
        .as_deref(),
        Some("bad_args")
    );
    let record = |_: &crate::assets::skill_ledger::RecordReq| Ok(Default::default());
    assert_eq!(
        code(via(
            &json!({ "name": "s", "at": at(&dst), "def": def, "target": crate::common::digest::fnv_digest(mine), "agent": "ghost" }),
            |r| apply_among(&FilesHere, reg, &record, r)
        ))
        .as_deref(),
        Some("bad_args")
    );
    assert_eq!(
        std::fs::read_to_string(dst.join(".fake-mcp.json")).unwrap(),
        mine
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// ★ 别家的全局那一份不拿默认那一家那份冒充：来源指名第二家、在全局那一级 ⇒ 拒；不带家照读（正控）。
#[test]
fn a_second_familys_user_level_source_is_not_read_from_the_default_familys_file() {
    let d = fixture("userfam");
    let user = d.join("user.json");
    std::fs::write(
        &user,
        json!({ "mcpServers": { "s": { "command": "c" } } }).to_string(),
    )
    .unwrap();
    let reg = two();
    via(&json!({ "name": "s", "at": { "level": "user" } }), |r| {
        source_among(&FilesHere, reg, Some(&user), r)
    })
    .expect("正控：不带家读不出全局那一份");
    // 指名默认那一家 ＝ 不带家（规整成缺，照读）。
    via(
        &json!({ "name": "s", "at": { "level": "user" }, "agent": "fake" }),
        |r| source_among(&FilesHere, reg, Some(&user), r),
    )
    .expect("指名默认那一家读不出全局那一份");
    let code = via(
        &json!({ "name": "s", "at": { "level": "user" }, "agent": "fake2" }),
        |r| source_among(&FilesHere, reg, Some(&user), r),
    )
    .expect_err("第二家的全局来源读了默认那一家那份")
    .code;
    assert_eq!(code.as_deref(), Some("refused"));
    let _ = std::fs::remove_dir_all(&d);
}

// ───────────────────────── 改过的用户文件 · 撤回（A100） ─────────────────────────

/// 沙箱里那份装记录：路径 · 真写口（同生产那一个 `record_at`，家是临时目录）。
struct Book {
    ledger: std::path::PathBuf,
    own: crate::platform::fs::Own,
}

impl Book {
    fn at(home: &std::path::Path) -> Book {
        std::fs::create_dir_all(home).unwrap();
        Book {
            ledger: home.join(relay_route_core::SKILL_LEDGER_REL),
            own: crate::platform::fs::Own::issue_at(home),
        }
    }
    fn record(
        &self,
        r: &crate::assets::skill_ledger::RecordReq,
    ) -> Result<crate::assets::skill_ledger::RecordReply, Said> {
        crate::assets::skill_ledger::record_at(
            &self.own,
            crate::assets::skill_install::Roots::here_with(None),
            r,
        )
    }
    /// 装记录里记着「cc-monitor 改过」的那几份（「文件与数据 → 改过的用户文件」一份一行照它接，`footprint::data::with_touched`）。
    fn listed(&self) -> Vec<String> {
        crate::assets::skill_ledger::load_at(&self.ledger)
            .expect("装记录读不懂")
            .touched
            .into_keys()
            .collect()
    }
}

/// 装一条到 `dst` 那个项目（先看卡、照卡上的记号写），写口是沙箱那份装记录。
fn apply_to(dst: &std::path::Path, book: &Book, name: &str, def: &Value) -> Value {
    let pre = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": name, "at": at(dst), "def": def }),
    )
    .expect("看不了卡");
    answer_apply(
        &FilesHere,
        &|r| book.record(r),
        &json!({ "name": name, "at": at(dst), "def": def, "target": pre["target"] }),
    )
    .expect("写不上")
}

/// ★ 改一条用户自己写的条目 ⇒「改过的用户文件」多一处（就是那份项目文件）；撤回 ⇒ 字节与改之前逐字节相同、那一处没了。
#[test]
fn editing_a_users_own_entry_is_listed_and_revert_restores_the_bytes() {
    let d = fixture("touch-edit");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    // 用户手写的：缩进 · 键序 · 尾换行都是他自己的写法。
    let mine = "{\n    \"mcpServers\": {\n        \"s\": { \"command\": \"mine\" },\n        \"keep\": { \"command\": \"k\" }\n    }\n}\n";
    std::fs::write(&file, mine).unwrap();
    assert!(book.listed().is_empty(), "正控：没改之前清单是空的");
    let done = apply_to(&dst, &book, "s", &json!({ "command": "theirs" }));
    assert_eq!(done["written"], true);
    assert_ne!(
        std::fs::read_to_string(&file).unwrap(),
        mine,
        "正控：真改了"
    );
    let listed = book.listed();
    assert_eq!(listed, vec![done["path"].as_str().unwrap().to_string()]);
    // 再改一次：清单还是那一处，留底的仍是头一回改之前那一份。
    apply_to(&dst, &book, "s", &json!({ "command": "again" }));
    assert_eq!(book.listed(), listed);
    let got =
        revert_project(&FilesHere, &book.ledger, &|r| book.record(r), &listed[0]).expect("撤不回");
    assert!(!got.removed);
    assert_eq!(std::fs::read(&file).unwrap(), mine.as_bytes());
    assert!(book.listed().is_empty(), "撤回之后清单里还在");
    let _ = std::fs::remove_dir_all(&d);
}

/// 两回改之间用户手改过 ⇒ 撤回只回到他手改的那一份（第二回改之前），不抹掉手改的。
#[test]
fn a_hand_edit_between_two_edits_survives_the_revert() {
    let d = fixture("touch-between");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    std::fs::write(&file, "{\"mcpServers\":{\"o\":{\"command\":\"orig\"}}}").unwrap();
    apply_to(&dst, &book, "a", &json!({ "command": "a" }));
    let hand = "{\"mcpServers\":{\"o\":{\"command\":\"hand\"},\"a\":{\"command\":\"a\"}}}";
    std::fs::write(&file, hand).unwrap();
    let done = apply_to(&dst, &book, "b", &json!({ "command": "b" }));
    assert_ne!(
        std::fs::read_to_string(&file).unwrap(),
        hand,
        "正控：又改了"
    );
    revert_project(
        &FilesHere,
        &book.ledger,
        &|r| book.record(r),
        done["path"].as_str().unwrap(),
    )
    .expect("撤不回");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        hand,
        "手改的被抹掉了"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 改之前没有那份文件 ⇒ 撤回是删掉它。
#[test]
fn reverting_a_file_that_did_not_exist_removes_it() {
    let d = fixture("touch-new");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    assert!(!file.exists());
    let done = apply_to(&dst, &book, "s", &json!({ "command": "c" }));
    assert!(file.exists(), "正控：写出来了");
    let got = revert_project(
        &FilesHere,
        &book.ledger,
        &|r| book.record(r),
        done["path"].as_str().unwrap(),
    )
    .expect("撤不回");
    assert!(got.removed);
    assert!(!file.exists(), "改之前没有，撤回后还在");
    assert!(book.listed().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

/// 改之后用户又手改过 ⇒ 撤回拒 `stale`，一个字节不动、清单那一处还在；没记过的那一份 ⇒ `not_found`。
#[test]
fn a_file_changed_after_the_edit_is_not_reverted() {
    let d = fixture("touch-stale");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    std::fs::write(&file, "{\"mcpServers\":{\"s\":{\"command\":\"mine\"}}}").unwrap();
    let done = apply_to(&dst, &book, "s", &json!({ "command": "theirs" }));
    let path = done["path"].as_str().unwrap().to_string();
    let hand = "{\"mcpServers\":{\"s\":{\"command\":\"hand\"}}}";
    std::fs::write(&file, hand).unwrap();
    let e = revert_project(&FilesHere, &book.ledger, &|r| book.record(r), &path)
        .expect_err("之后改过还撤了");
    assert_eq!(e.code.as_deref(), Some("stale"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), hand);
    assert_eq!(book.listed(), vec![path]);
    let e = revert_project(
        &FilesHere,
        &book.ledger,
        &|r| book.record(r),
        &d.join("nowhere.json").display().to_string(),
    )
    .expect_err("没记过的也撤了");
    assert_eq!(e.code.as_deref(), Some("not_found"));
    let _ = std::fs::remove_dir_all(&d);
}

/// 帧命令 `mcp-project-revert {file}`：入参多一个键 ⇒ `bad_args`、一个字节不动；回包 `{path, removed}`（装记录摘成了不带 `recordFailed`）；
/// 下层的 `stale` · `not_found` 原样是这条命令码表里的码。
#[test]
fn the_revert_command_takes_only_the_file_and_answers_path_and_removed() {
    use crate::assets::ext::{answer_revert, McpRevertCmd, McpRevertErr, McpRevertReq};
    use crate::wire::typed::{Codes, Command};
    let d = fixture("touch-cmd");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    let done = apply_to(&dst, &book, "s", &json!({ "command": "c" }));
    let path = done["path"].as_str().unwrap();
    let code = crate::wire::typed::take::<McpRevertReq>(&json!({ "file": path, "extra": 1 }))
        .expect_err("多一个键也收了")
        .code;
    assert_eq!(code.as_deref(), Some("bad_args"));
    assert!(file.exists());
    assert_eq!(McpRevertCmd::NAME, "mcp-project-revert");
    for c in ["stale", "not_found", "refused", "ledger_unreadable"] {
        assert!(McpRevertErr::ALL.contains(&c), "码表里没有 {c}");
    }
    let req = crate::wire::typed::take::<McpRevertReq>(&json!({ "file": path })).unwrap();
    let got = answer_revert(&FilesHere, &book.ledger, &|r| book.record(r), &req).expect("撤不回");
    assert_eq!(
        serde_json::to_value(got).unwrap(),
        json!({ "path": path, "removed": true })
    );
    assert!(!file.exists());
    let e = answer_revert(&FilesHere, &book.ledger, &|r| book.record(r), &req)
        .expect_err("撤过一回又撤了");
    assert_eq!(e.into_said().code.as_deref(), Some("not_found"));
    let _ = std::fs::remove_dir_all(&d);
}

/// 那份读不懂 ⇒ 照旧拒绝覆盖：一个字节不写、不记账、清单不多一处。
#[test]
fn an_unreadable_file_is_refused_and_not_listed() {
    let d = fixture("touch-bad");
    let dst = d.join("dst");
    let book = Book::at(&d.join("home"));
    let file = dst.join(default_project_mcp_file());
    std::fs::write(&file, "{ not json").unwrap();
    let def = json!({ "command": "c" });
    let e = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": "s", "at": at(&dst), "def": def }),
    )
    .expect_err("读不懂也给看卡");
    assert_eq!(e.code.as_deref(), Some("bad_file"));
    let e = answer_apply(
        &FilesHere,
        &|r| book.record(r),
        &json!({ "name": "s", "at": at(&dst), "def": def, "target": crate::common::digest::fnv_digest("{ not json") }),
    )
    .expect_err("读不懂也盖了");
    assert_eq!(e.code.as_deref(), Some("bad_file"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
    assert!(book.listed().is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

/// 先记账再写文件：装记录记不上（读不懂 · 写不进）⇒ 那份配置一个字节不写、整趟报错（原文只有装记录这一份留底）。
#[test]
fn a_ledger_that_cannot_record_leaves_the_project_file_unwritten() {
    let d = fixture("record-first");
    let dst = d.join("dst");
    std::fs::write(dst.join(default_project_mcp_file()), "{\"mcpServers\":{}}").unwrap();
    let def = json!({ "command": "a" });
    let pre = answer_preview(
        &FilesHere,
        &NoFacts,
        &json!({ "name": "a", "at": at(&dst), "def": def }),
    )
    .unwrap();
    let record = |_: &crate::assets::skill_ledger::RecordReq| {
        Err(crate::wire::spec::fail(
            "ledger_unreadable",
            crate::common::said::Said::from("记不上".to_string()),
        ))
    };
    let e = answer_apply(
        &FilesHere,
        &record,
        &json!({ "name": "a", "at": at(&dst), "def": def, "target": pre["target"] }),
    )
    .expect_err("记不上还写了");
    assert_eq!(e.code.as_deref(), Some("ledger_unreadable"));
    assert_eq!(
        std::fs::read_to_string(dst.join(default_project_mcp_file())).unwrap(),
        "{\"mcpServers\":{}}"
    );
    let _ = std::fs::remove_dir_all(&d);
}
