//! 要求：MCP 配置里像密钥的值不在机器之间复制 —— 确认卡上是待填或「沿用目标机已有的值」，值出不了来源机、缺了就拒；
//! 写一趟不重算：看过之后被写那份变了 ⇒ `stale`、一个字节不写；用户级 MCP 只读。
use super::*;
use crate::assets::mcp_sync::There;
use crate::stream::inbound::LocalFiles;
use std::cell::RefCell;

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
        d.join("src").join(MCP_JSON),
        json!({ "mcpServers": { "s": def, "other": { "command": "o" } } }).to_string(),
    )
    .unwrap();
    let got = answer_source(
        &LocalFiles,
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
        d.join("src").join(MCP_JSON),
        json!({ "mcpServers": { "s": changed } }).to_string(),
    )
    .unwrap();
    let again = answer_source(
        &LocalFiles,
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
        dst.join(MCP_JSON),
        json!({ "mcpServers": { "s": { "command": "old", "env": { "API_KEY": "mine" } }, "keep": { "command": "k" } } }).to_string(),
    )
    .unwrap();
    let def = json!({ "command": "srv", "env": { "API_KEY": null, "TOKEN": null } });
    let pre = answer_preview(
        &LocalFiles,
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
    let record = |a: &Value| {
        recorded.borrow_mut().push(a.clone());
        Ok(json!({}))
    };
    let before = std::fs::read_to_string(dst.join(MCP_JSON)).unwrap();
    let (code, _) = answer_apply(
        &LocalFiles,
        &record,
        &json!({ "name": "s", "at": at(&dst), "def": def, "fill": {}, "target": pre["target"] }),
    )
    .expect_err("缺值还写了");
    assert_eq!(code, "needs_input");
    assert_eq!(std::fs::read_to_string(dst.join(MCP_JSON)).unwrap(), before);
    let done = answer_apply(
        &LocalFiles,
        &record,
        &json!({ "name": "s", "at": at(&dst), "def": def, "fill": { "env": { "TOKEN": "typed" } }, "target": pre["target"] }),
    )
    .expect("写不上");
    assert_eq!(done["written"], true);
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(dst.join(MCP_JSON)).unwrap()).unwrap();
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
    let (code, _) = answer_preview(
        &LocalFiles,
        &NoFacts,
        &json!({ "name": "s", "at": at(&dst), "def": { "command": "x", "env": { "API_KEY": "leak" } } }),
    )
    .expect_err("带值的定义也收了");
    assert_eq!(code, "bad_args");
    let _ = std::fs::remove_dir_all(&d);
}

/// 看过之后被写那份变了 ⇒ `stale`、一个字节不写（不重读重算）；用户级 ⇒ 只读、拒。
#[test]
fn a_target_that_changed_after_the_preview_is_left_alone_and_user_level_is_read_only() {
    let d = fixture("stale");
    let dst = d.join("dst");
    std::fs::write(dst.join(MCP_JSON), "{\"mcpServers\":{}}").unwrap();
    let def = json!({ "command": "a" });
    let pre = answer_preview(
        &LocalFiles,
        &NoFacts,
        &json!({ "name": "a", "at": at(&dst), "def": def }),
    )
    .unwrap();
    std::fs::write(dst.join(MCP_JSON), "{\"mcpServers\":{\"z\":{}}}").unwrap();
    let record = |_: &Value| Ok(json!({}));
    let (code, _) = answer_apply(
        &LocalFiles,
        &record,
        &json!({ "name": "a", "at": at(&dst), "def": def, "target": pre["target"] }),
    )
    .expect_err("看过之后变了还写了");
    assert_eq!(code, "stale");
    assert_eq!(
        std::fs::read_to_string(dst.join(MCP_JSON)).unwrap(),
        "{\"mcpServers\":{\"z\":{}}}"
    );
    // 这台没建账号库（家目录是临时目录）⇒ 用户级只读。
    let (code, _) = answer_preview(
        &crate::assets::aliases::tests::HomeDoor(d.clone()),
        &NoFacts,
        &json!({ "name": "a", "at": { "level": "user" }, "def": def }),
    )
    .expect_err("用户级也收了");
    assert_eq!(code, "refused");
    let _ = std::fs::remove_dir_all(&d);
}
