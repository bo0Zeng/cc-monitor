//! 信任那一格的读与标：Claude 那一家的格式与假适配层的格式各走一遍（通用层只按适配层声明的几格读写）；
//! 标的时候只换那几格，别的字节一个不动（键序 · 排版 · 别的项目那几项原样）。

use super::*;
use serde_json::json;

const CLAUDE: TrustCells = crate::agents::claudecode::accounts::TRUST_CELLS;
const FAKE: TrustCells = crate::agents::fake::TRUST_CELLS;

fn set(dirs: &[&str]) -> BTreeSet<String> {
    dirs.iter().map(|d| d.to_string()).collect()
}

/// Claude 自己写的那种排版：键不按字母序（`zeta` 在 `alpha` 前），里面有登录与 MCP。
const CLAUDE_TEXT: &str = r#"{
  "numStartups": 7,
  "oauthAccount": {
    "emailAddress": "a@example.test",
    "accountUuid": "00000000-0000-0000-0000-000000000000"
  },
  "mcpServers": {
    "cclsp": {
      "type": "stdio",
      "command": "cclsp"
    }
  },
  "projects": {
    "/w/old": {
      "zeta": 1,
      "allowedTools": [],
      "hasTrustDialogAccepted": true,
      "alpha": 0.1000
    },
    "/w/no": {
      "hasTrustDialogAccepted": false,
      "zeta": 2
    },
    "/w/bare": {
      "allowedTools": []
    }
  },
  "tipsHistory": {
    "x": 3
  }
}
"#;

#[test]
fn trusted_reads_only_the_cells_the_adapter_declares() {
    let v: Value = serde_json::from_str(CLAUDE_TEXT).unwrap();
    assert_eq!(trusted(&CLAUDE, &v), set(&["/w/old"]));
    assert_eq!(
        trusted(&FAKE, &v),
        set(&[]),
        "假那一家的表名不同 ⇒ 读不出 Claude 的"
    );
    let f = json!({ "trusted": { "/a": true, "/b": false, "/c": "yes" }, "projects": { "/d": { "hasTrustDialogAccepted": true } } });
    assert_eq!(trusted(&FAKE, &f), set(&["/a"]));
    assert_eq!(trusted(&CLAUDE, &json!({ "projects": [] })), set(&[]));
}

/// 新目录补一项只含那一格的对象；已有一项但没信任的只翻那一格；别的字节原样。
#[test]
fn mark_changes_only_the_trust_cells() {
    let out = mark(&CLAUDE, CLAUDE_TEXT, &set(&["/w/old", "/w/no", "/w/new"]))
        .unwrap()
        .expect("有要标的");
    let mut want: Value = serde_json::from_str(CLAUDE_TEXT).unwrap();
    want["projects"]["/w/no"]["hasTrustDialogAccepted"] = json!(true);
    want["projects"]["/w/new"] = json!({ "hasTrustDialogAccepted": true });
    let got: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(got, want);
    // 原文里没动的几段逐字还在（键序 · 数字写法 · 排版）。
    let head = &CLAUDE_TEXT[..CLAUDE_TEXT.find("\"projects\"").unwrap()];
    assert!(out.starts_with(head), "projects 之前的字节变了：\n{out}");
    for kept in [
        "\"/w/old\": {\n      \"zeta\": 1,\n      \"allowedTools\": [],\n      \"hasTrustDialogAccepted\": true,\n      \"alpha\": 0.1000\n    }",
        "\"hasTrustDialogAccepted\": true,\n      \"zeta\": 2",
        "\"/w/bare\": {\n      \"allowedTools\": []\n    }",
        "  \"tipsHistory\": {\n    \"x\": 3\n  }\n}\n",
    ] {
        assert!(out.contains(kept), "少了原样那一段 {kept:?}：\n{out}");
    }
}

#[test]
fn mark_with_nothing_missing_writes_nothing() {
    assert_eq!(mark(&CLAUDE, CLAUDE_TEXT, &set(&["/w/old"])).unwrap(), None);
    assert_eq!(mark(&CLAUDE, CLAUDE_TEXT, &set(&[])).unwrap(), None);
}

#[test]
fn mark_builds_the_table_when_it_is_not_there() {
    let text = "{\n  \"numStartups\": 1\n}\n";
    let out = mark(&CLAUDE, text, &set(&["/x"])).unwrap().unwrap();
    let got: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        got,
        json!({ "numStartups": 1, "projects": { "/x": { "hasTrustDialogAccepted": true } } })
    );
    let out = mark(&CLAUDE, "{}\n", &set(&["/x"])).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap(),
        json!({ "projects": { "/x": { "hasTrustDialogAccepted": true } } })
    );
}

/// 假适配层那一形：目录那一项本身就是那个布尔。
#[test]
fn mark_follows_the_fake_adapters_shape() {
    let text =
        "{\n  \"servers\": {},\n  \"trusted\": {\n    \"/a\": true,\n    \"/b\": false\n  }\n}\n";
    let out = mark(&FAKE, text, &set(&["/a", "/b", "/c"]))
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap(),
        json!({ "servers": {}, "trusted": { "/a": true, "/b": true, "/c": true } })
    );
    assert!(!out.contains("projects") && !out.contains("hasTrustDialogAccepted"));
}

#[test]
fn mark_refuses_what_it_cannot_edit_in_place() {
    assert!(mark(&CLAUDE, "not json", &set(&["/x"])).is_err());
    assert!(mark(&CLAUDE, "[1]", &set(&["/x"])).is_err());
    assert!(mark(&CLAUDE, "{\"projects\": 3}", &set(&["/x"])).is_err());
    assert!(mark(&CLAUDE, "{\"projects\": {\"/x\": 1}}", &set(&["/x"])).is_err());
}

/// 通用层零直呼：同步与预标那两份源码里不出现任何一家的键名 / 文件名 / 模块（只按适配层声明的几格读写）。
#[test]
fn the_generic_layer_names_no_agent() {
    for (name, src) in [
        (
            "trust_share.rs",
            include_str!("../../../../src/backend/accounts/manage/trust_share.rs"),
        ),
        (
            "trust_share_exec.rs",
            include_str!("../../../../src/backend/accounts/manage/trust_share_exec.rs"),
        ),
    ] {
        for word in [
            "hasTrustDialogAccepted",
            "\"projects\"",
            "claudecode",
            ".claude.json",
            "\"trusted\"",
        ] {
            assert!(!src.contains(word), "{name} 直呼了 {word}");
        }
    }
}
