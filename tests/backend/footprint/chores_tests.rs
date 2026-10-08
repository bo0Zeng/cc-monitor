//! 「要你动手」那几件：按事实判类 · 态 · 要贴什么；进角标的只数 要做 ＋ 要装 ＋ 要你定。

use super::*;
use serde_json::json;

fn base() -> Facts {
    Facts {
        windows: false,
        needs_install: vec![],
        stale_ccm: None,
        self_paste: None,
        clashes: vec![],
        dead: vec![],
        relay: None,
        hooks: None,
        declined: vec![],
    }
}

fn ids(v: &[Value]) -> Vec<String> {
    v.iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn 什么事都没有_零件() {
    assert!(chores(&base()).is_empty());
    assert_eq!(badge(&chores(&base())), 0);
}

#[test]
fn 旧_ccm_要做_复制一行命令() {
    let mut f = base();
    f.stale_ccm = Some(StaleCcm {
        path: "/usr/local/bin/ccm".into(),
        needs_root: true,
    });
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["stale-ccm"]);
    assert_eq!(c[0]["kind"], json!("must"));
    assert_eq!(c[0]["state"], json!("todo"));
    assert_eq!(c[0]["action"], json!("copyCommand"));
    assert_eq!(c[0]["copy"], json!("sudo rm '/usr/local/bin/ccm'"));
    assert_eq!(badge(&c), 1);
}

#[test]
fn 我自己贴_没贴好前要做_贴好了已做() {
    let mut f = base();
    f.self_paste = Some(SelfPaste {
        rc: "/h/.bashrc".into(),
        lines: vec!["# BEGIN".into(), "x".into()],
        after_line: 139,
        present: false,
    });
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("self-paste"));
    assert_eq!(c[0]["kind"], json!("must"));
    assert_eq!(c[0]["copy"], json!("# BEGIN\nx"));
    assert_eq!(c[0]["action"], json!("copySnippet"));
    f.self_paste.as_mut().unwrap().present = true;
    let c = chores(&f);
    assert_eq!(c[0]["state"], json!("done"));
    assert_eq!(badge(&c), 0);
}

#[test]
fn 同名_要你定_去定跳到别名页那一项() {
    let mut f = base();
    f.clashes = vec![Clash {
        name: "cc".into(),
        path: "/h/.bashrc".into(),
        line: 125,
        wins: "yours".into(),
    }];
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("clash:cc"));
    assert_eq!(c[0]["kind"], json!("decide"));
    assert_eq!(c[0]["action"], json!("decide"));
    assert_eq!(
        c[0]["go"],
        json!({"page": "machine", "tab": "config", "anchor": "clash"})
    );
    assert_eq!(badge(&c), 1);
}

#[test]
fn 失效行_可选_不进角标_看在哪给位置() {
    let mut f = base();
    f.dead = vec![DeadLines {
        path: "/h/.bashrc".into(),
        lines: vec![(118, "source ~/x.sh".into()), (119, ". ~/y".into())],
    }];
    let c = chores(&f);
    assert_eq!(c[0]["kind"], json!("optional"));
    assert_eq!(c[0]["action"], json!("locate"));
    assert_eq!(c[0]["copy"], json!("/h/.bashrc:118"));
    assert_eq!(badge(&c), 0);
}

#[test]
fn 实时显示_没贴可选_贴过过期升成要做_贴好已做_钥匙遮住() {
    let mut f = base();
    f.relay = Some(Relay {
        state: "absent".into(),
        path: "/h/.claude/settings.json".into(),
        text: Some("{\n  \"env\": {}\n}\n".into()),
        key: "ANTHROPIC_BASE_URL".into(),
        url: Some("http://127.0.0.1:1/k/SECRET/claude".into()),
        secret: Some("SECRET".into()),
    });
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("relay"));
    assert_eq!(c[0]["kind"], json!("optional"));
    assert_eq!(c[0]["state"], json!("todo"));
    assert!(
        c[0]["whole"].as_str().unwrap().contains("SECRET"),
        "复制的是真内容"
    );
    assert_eq!(c[0]["mask"], json!("SECRET"));
    assert!(c[0]["diff"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["op"] == json!("add")));
    f.relay.as_mut().unwrap().state = "stale".into();
    let c = chores(&f);
    assert_eq!(
        (c[0]["kind"].clone(), c[0]["state"].clone()),
        (json!("must"), json!("expired"))
    );
    assert_eq!(badge(&c), 1);
    f.relay.as_mut().unwrap().state = "installed".into();
    assert_eq!(chores(&f)[0]["state"], json!("done"));
}

#[test]
fn cc_bus_收信_没装_cc_bus_先决_windows_不出() {
    let mut f = base();
    f.hooks = Some(Hooks {
        ccbus: false,
        installed: false,
        path: "/h/.claude/settings.json".into(),
        text: None,
        items: vec![],
    });
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("cc-bus-hooks"));
    assert_eq!(c[0]["state"], json!("blocked"));
    assert_eq!(c[0]["go"], json!({"page": "ext"}));
    f.windows = true;
    assert!(chores(&f).is_empty());
}

#[test]
fn 同一份文件两件_整份含两件_各自的_diff_按现在的文件算() {
    let mut f = base();
    let text = "{\n  \"model\": \"opus\"\n}\n".to_string();
    f.relay = Some(Relay {
        state: "absent".into(),
        path: "/h/s.json".into(),
        text: Some(text.clone()),
        key: "K".into(),
        url: Some("u".into()),
        secret: None,
    });
    f.hooks = Some(Hooks {
        ccbus: true,
        installed: false,
        path: "/h/s.json".into(),
        text: Some(text),
        items: vec![("Stop".into(), json!({"hooks": []}))],
    });
    let c = chores(&f);
    let whole: Value = serde_json::from_str(c[0]["whole"].as_str().unwrap()).unwrap();
    assert_eq!(whole["env"]["K"], json!("u"));
    assert_eq!(whole["hooks"]["Stop"], json!([{"hooks": []}]));
    assert_eq!(c[0]["whole"], c[1]["whole"]);
    assert_eq!(c[0]["wholeCovers"], json!(["relay", "cc-bus-hooks"]));
}

#[test]
fn 不做_记过的可选收进不做_要做的不许不做() {
    let mut f = base();
    f.dead = vec![DeadLines {
        path: "/h/.bashrc".into(),
        lines: vec![(1, "source /nope".into())],
    }];
    f.stale_ccm = Some(StaleCcm {
        path: "/x/ccm".into(),
        needs_root: false,
    });
    f.declined = vec!["dead:/h/.bashrc".into(), "stale-ccm".into()];
    let c = chores(&f);
    let dead = c
        .iter()
        .find(|c| c["id"] == json!("dead:/h/.bashrc"))
        .unwrap();
    assert_eq!(dead["state"], json!("declined"));
    let stale = c.iter().find(|c| c["id"] == json!("stale-ccm")).unwrap();
    assert_eq!(stale["state"], json!("todo"));
    assert_eq!(stale["copy"], json!("rm '/x/ccm'"));
}

#[test]
fn 要装并进来_缺了起不了会话的进角标() {
    let mut f = base();
    f.needs_install = vec![
        json!({"id": "claude-cli", "name": "Claude Code", "what": "claude", "required": true, "howUrl": "https://x"}),
        json!({"id": "git", "name": "git", "what": "git", "required": false, "howUrl": null}),
    ];
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["install:claude-cli", "install:git"]);
    assert_eq!(c[0]["kind"], json!("install"));
    assert_eq!(c[1]["kind"], json!("installOptional"));
    assert_eq!(c[0]["action"], json!("how"));
    assert_eq!(badge(&c), 1);
}

#[test]
fn 每件的线上形状_键集恰好() {
    let mut f = base();
    f.stale_ccm = Some(StaleCcm {
        path: "/x/ccm".into(),
        needs_root: false,
    });
    let c = chores(&f);
    let mut keys: Vec<&str> = c[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "action",
            "copy",
            "diff",
            "file",
            "go",
            "howUrl",
            "id",
            "kind",
            "loc",
            "mask",
            "name",
            "said",
            "state",
            "steps",
            "whole",
            "wholeCovers",
            "why"
        ]
    );
}
