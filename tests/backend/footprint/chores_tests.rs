//! 「待办」那几件：按事实判类 · 态 · 要贴什么；进角标的只数 要做 ＋ 要装 ＋ 待定。

use super::*;
use serde_json::{json, Value};

/// 各件写成线上那一份（下面几条按格读）。
fn chores(f: &Facts) -> Vec<Value> {
    super::chores(f)
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .collect()
}

/// 角标数（按那几件本身数，同成品那一格）。
fn badge_of(f: &Facts) -> usize {
    super::counts(&super::chores(f)).badge
}

fn base() -> Facts {
    Facts {
        windows: false,
        needs_install: vec![],
        stale_ccm: None,
        self_paste: None,
        clashes: vec![],
        dead: vec![],
        relay: vec![],
        hooks: None,
        mcp_login: vec![],
        sshd: None,
        declined: vec![],
    }
}

/// 一家「直接敲的也走中转」那一件的事实（合法取那一家注册表里的那一格）。
fn relay_of(
    face: crate::agents::SettingsEnvFace,
    agent: &str,
    name: &str,
    state: &str,
    path: &str,
    text: Option<&str>,
    url: Option<&str>,
) -> Relay {
    Relay {
        agent: agent.into(),
        name: name.into(),
        slot: face.slot,
        merge: face.merge,
        state: state.into(),
        path: path.into(),
        text: text.map(Into::into),
        url: url.map(Into::into),
        covered: None,
    }
}

fn claude_relay(state: &str, path: &str, text: Option<&str>, url: Option<&str>) -> Relay {
    relay_of(
        crate::agents::claudecode::paths::SETTINGS_ENV,
        "claude-code",
        "Claude Code",
        state,
        path,
        text,
        url,
    )
}

fn ids(v: &[Value]) -> Vec<String> {
    v.iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn 什么事都没有_零件() {
    assert!(chores(&base()).is_empty());
    assert_eq!(badge_of(&base()), 0);
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
    assert_eq!(badge_of(&f), 1);
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
    assert_eq!(badge_of(&f), 0);
}

#[test]
fn 同名_待定_去定跳到别名页那一项() {
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
    assert_eq!(badge_of(&f), 1);
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
    assert_eq!(badge_of(&f), 0);
}

#[test]
fn 实时显示_没贴可选_贴过过期升成要做_贴好已做() {
    let mut f = base();
    f.relay = vec![claude_relay(
        "absent",
        "/h/.claude/settings.json",
        Some("{\n  \"env\": {}\n}\n"),
        Some("http://127.0.0.1:1/k/SECRET/claude"),
    )];
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("relay:claude-code"));
    assert_eq!(c[0]["kind"], json!("optional"));
    assert_eq!(c[0]["state"], json!("todo"));
    assert!(
        c[0]["whole"].as_str().unwrap().contains("SECRET"),
        "整份里是要贴的那条地址"
    );
    assert!(c[0]["diff"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["kind"] == json!("add")));
    f.relay[0].state = "stale".into();
    let c = chores(&f);
    assert_eq!(
        (c[0]["kind"].clone(), c[0]["state"].clone()),
        (json!("must"), json!("expired"))
    );
    assert_eq!(badge_of(&f), 1);
    f.relay[0].state = "installed".into();
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

/// 待办那一件的行差与写类工具那一份同形（`cmd_wire::record::LineDiff`），行尾风格不算差别：
/// 改前以 `\r\n` · `\r` 断行、合进去的那几行以 `\n` 断行，没动的那几行不出删。
#[test]
fn 待办的_diff_是写类工具那一份行差_行尾风格不算差别() {
    let mut f = base();
    f.relay = vec![claude_relay(
        "absent",
        "/h/s.json",
        Some("{\r\n  \"model\": \"opus\"\r}\r\n"),
        Some("u"),
    )];
    let c = super::chores(&f);
    let d: &cmd_wire::record::LineDiff = c[0].diff.as_ref().expect("那一件带行差");
    assert_eq!(d.removed, 0, "行尾风格不同的那几行不算删：{:?}", d.rows);
    assert!(d.added > 0);
    assert!(
        d.rows.iter().all(|r| !r.text.contains('\r')),
        "{:?}",
        d.rows
    );
    let ctx = d
        .rows
        .iter()
        .find(|r| r.kind == cmd_wire::record::LineDiffKind::Ctx)
        .expect("上下带一行原文");
    assert!(ctx.old_no.is_some() && ctx.new_no.is_some(), "{ctx:?}");
}

/// 没有要贴的那几件（命令一类）不带行差。
#[test]
fn 命令一类的待办_没有行差() {
    let mut f = base();
    f.stale_ccm = Some(StaleCcm {
        path: "/usr/local/bin/ccm".into(),
        needs_root: true,
    });
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("stale-ccm"));
    assert_eq!(c[0]["diff"], Value::Null);
}

#[test]
fn 同一份文件两件_整份含两件_各自的_diff_按现在的文件算() {
    let mut f = base();
    let text = "{\n  \"model\": \"opus\"\n}\n".to_string();
    f.relay = vec![claude_relay("absent", "/h/s.json", Some(&text), Some("u"))];
    f.hooks = Some(Hooks {
        ccbus: true,
        installed: false,
        path: "/h/s.json".into(),
        text: Some(text),
        items: vec![(vec!["hooks".into(), "Stop".into()], json!({"hooks": []}))],
    });
    let c = chores(&f);
    let whole: Value = serde_json::from_str(c[0]["whole"].as_str().unwrap()).unwrap();
    assert_eq!(whole["env"]["ANTHROPIC_BASE_URL"], json!("u"));
    assert_eq!(whole["hooks"]["Stop"], json!([{"hooks": []}]));
    assert_eq!(c[0]["whole"], c[1]["whole"]);
    assert_eq!(
        c[0]["wholeCovers"],
        json!(["relay:claude-code", "cc-bus-hooks"])
    );
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
        Missing {
            id: "claude-cli",
            name: "Claude Code".into(),
            what: "claude",
            required: true,
            how_url: Some("https://x"),
        },
        Missing {
            id: "git",
            name: "git".into(),
            what: "git",
            required: false,
            how_url: None,
        },
    ];
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["install:claude-cli", "install:git"]);
    assert_eq!(c[0]["kind"], json!("install"));
    assert_eq!(c[1]["kind"], json!("installOptional"));
    assert_eq!(c[0]["action"], json!("how"));
    assert_eq!(badge_of(&f), 1);
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
            "name",
            "said",
            "state",
            "steps",
            "tag",
            "tone",
            "whole",
            "wholeCovers",
            "why"
        ]
    );
}

/// 两家各有一份「直接敲的也走中转」：各出一件、id 按家分、名字带那一家、改法照那一家的格式（Codex 是配置顶层那一行）。
#[test]
fn 实时显示_两家各一件_各按各的格式() {
    let mut f = base();
    f.relay = vec![
        claude_relay(
            "absent",
            "/h/.claude/settings.json",
            Some("{}\n"),
            Some("http://127.0.0.1:1/K1/t/claude-code/_"),
        ),
        relay_of(
            crate::agents::codex::relay::SETTINGS_ENV,
            "codex",
            "Codex",
            "absent",
            "/h/.codex/config.toml",
            Some("model = \"m\"\n"),
            Some("http://127.0.0.1:1/K2/t/codex/_"),
        ),
    ];
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["relay:claude-code", "relay:codex"]);
    assert!(c[0]["name"].as_str().unwrap().contains("Claude Code"));
    assert!(c[1]["name"].as_str().unwrap().contains("Codex"));
    assert_eq!(
        c[1]["copy"],
        json!("openai_base_url = \"http://127.0.0.1:1/K2/t/codex/_\"")
    );
    assert_eq!(
        c[1]["whole"],
        json!("openai_base_url = \"http://127.0.0.1:1/K2/t/codex/_\"\nmodel = \"m\"\n")
    );
    assert_eq!(
        c[1]["steps"][1],
        json!(copy_core::copy_text(
            "beChore.step.insertTop",
            &[("n", "1")]
        ))
    );
    assert!(c[1]["loc"].as_str().unwrap().contains("openai_base_url"));
    assert_eq!(c[1]["wholeCovers"], json!(["relay:codex"]));
}

/// 用户自己配的 MCP 要登录 ⇒ 一个服务器一件，可选、不进角标；号名进位置那一格；复制的是 `/mcp`。点过「不用了」⇒ 已不用。
/// 要求：「用户自己配的要登录的，在待办里算一件，类 optional，不进角标」。
#[test]
fn 自己配的_mcp_要登录_可选一件_不进角标() {
    let mut f = base();
    f.mcp_login = vec![
        McpLogin {
            name: "m-a".into(),
            who: vec!["p".into(), "w".into()],
            command: "/mcp",
        },
        McpLogin {
            name: "m-b".into(),
            who: vec![],
            command: "/mcp",
        },
    ];
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["mcp-login:m-a", "mcp-login:m-b"]);
    assert!(c.iter().all(|x| x["kind"] == json!("optional")
        && x["action"] == json!("copyCommand")
        && x["copy"] == json!("/mcp")));
    assert_eq!(
        c[0]["name"],
        json!(copy_text("beChore.mcpLogin.name", &[("name", "m-a")]))
    );
    assert_eq!(c[0]["loc"], json!("p、w"));
    assert_eq!(badge_of(&f), 0);
    f.declined = vec!["mcp-login:m-b".into()];
    assert_eq!(chores(&f)[1]["state"], json!("declined"));
}

/// 假的那台 sshd 关了端口转发 ⇒ 多一件要做：位置是那一行，diff 删那一行加改好的一行，复制改好的那一行；进角标。
#[test]
fn sshd_关了转发_要做_换那一行() {
    let mut f = base();
    f.sshd = Some(super::sshd::ForwardingOff {
        path: "/etc/ssh/sshd_config".into(),
        line: 7,
        text: "AllowTcpForwarding no".into(),
        fix: "AllowTcpForwarding yes".into(),
    });
    let c = chores(&f);
    assert_eq!(ids(&c), vec!["sshd-forwarding"]);
    assert_eq!(c[0]["kind"], json!("must"));
    assert_eq!(c[0]["state"], json!("todo"));
    assert_eq!(c[0]["action"], json!("copySnippet"));
    assert_eq!(c[0]["copy"], json!("AllowTcpForwarding yes"));
    assert_eq!(c[0]["file"], json!("/etc/ssh/sshd_config"));
    assert_eq!(
        c[0]["diff"],
        json!({
            "rows": [
                {"kind": "del", "text": "AllowTcpForwarding no", "oldNo": 7},
                {"kind": "add", "text": "AllowTcpForwarding yes", "newNo": 7}
            ],
            "shown": 2,
            "truncated": false,
            "added": 1,
            "removed": 1,
            "stat": "+1 −1"
        })
    );
    assert_eq!(c[0]["steps"].as_array().unwrap().len(), 3);
    assert!(c[0]["loc"]
        .as_str()
        .unwrap()
        .contains("/etc/ssh/sshd_config"));
    assert_eq!(badge_of(&f), 1);
}

/// 每一件带写好的小标签（字 ＋ 管颜色的类）与点色，界面照摆、不照类 · 态自己选：
/// 过期 ⇒「过期 · 要做」照要做上色；先决没满足 ⇒「先决未满足」照本类上色；别的照本类出字。
/// 点色：已做 绿 · 过期 / 要做 红 · 待定 / 要装 琥珀 · 余下（要装 · 可选 · 可选 · 不用了）空心。
#[test]
fn 每件带写好的小标签与点色() {
    let mut f = base();
    f.stale_ccm = Some(StaleCcm {
        path: "/x/ccm".into(),
        needs_root: false,
    });
    f.self_paste = Some(SelfPaste {
        rc: "/h/.bashrc".into(),
        lines: vec!["x".into()],
        after_line: 1,
        present: true,
    });
    f.clashes = vec![Clash {
        name: "cc".into(),
        path: "/h/.bashrc".into(),
        line: 3,
        wins: "yours".into(),
    }];
    f.needs_install = vec![
        Missing {
            id: "claude-cli",
            name: "Claude Code".into(),
            what: "claude",
            required: true,
            how_url: None,
        },
        Missing {
            id: "git",
            name: "git".into(),
            what: "git",
            required: false,
            how_url: None,
        },
    ];
    f.dead = vec![DeadLines {
        path: "/h/.bashrc".into(),
        lines: vec![(1, "source /nope".into())],
    }];
    f.relay = vec![claude_relay(
        "stale",
        "/h/.claude/settings.json",
        Some("{}"),
        Some("http://127.0.0.1:1/k/K/claude"),
    )];
    f.hooks = Some(Hooks {
        ccbus: false,
        installed: false,
        path: "/h/.claude/settings.json".into(),
        text: None,
        items: vec![],
    });
    f.mcp_login = vec![McpLogin {
        name: "m".into(),
        who: vec![],
        command: "/mcp",
    }];
    f.declined = vec!["mcp-login:m".into()];
    let c = chores(&f);
    let of = |id: &str| {
        let x = c.iter().find(|x| x["id"] == json!(id)).unwrap();
        (x["tag"].clone(), x["tone"].clone())
    };
    // （id, 小标签的字的文案键, 小标签的类, 点色）
    let want = [
        ("stale-ccm", "dataPage.kind.must", "must", "bad"),
        ("self-paste", "dataPage.kind.must", "must", "ok"),
        ("relay:claude-code", "dataPage.state.expired", "must", "bad"),
        ("clash:cc", "dataPage.kind.decide", "decide", "warn"),
        (
            "install:claude-cli",
            "dataPage.install.kind",
            "install",
            "warn",
        ),
        (
            "install:git",
            "dataPage.install.kindOptional",
            "installOptional",
            "muted",
        ),
        (
            "dead:/h/.bashrc",
            "dataPage.kind.optional",
            "optional",
            "muted",
        ),
        (
            "cc-bus-hooks",
            "dataPage.state.blocked",
            "optional",
            "muted",
        ),
        ("mcp-login:m", "dataPage.kind.optional", "optional", "muted"),
    ];
    for (id, key, kind, tone) in want {
        let tag = json!({"text": copy_text(key, &[]), "kind": kind});
        assert_eq!(of(id), (tag, json!(tone)), "{id}");
    }
}

/// 贴好了、但某个项目里的设置文件自己写了上游地址 ⇒ 照旧已做，说明换成「被那个项目盖住」，原因行指那份文件。
#[test]
fn 实时显示_已做但被项目里的设置盖住() {
    let mut f = base();
    f.relay = vec![claude_relay(
        "installed",
        "/h/.claude/settings.json",
        Some("{}"),
        None,
    )];
    let plain = chores(&f)[0].clone();
    f.relay[0].covered = Some(Covered {
        project: "/w/app".into(),
        file: "/w/app/.claude/settings.local.json".into(),
    });
    let c = chores(&f);
    assert_eq!(c[0]["state"], json!("done"));
    assert_ne!(c[0]["said"], plain["said"], "被盖住要另说");
    assert!(c[0]["said"].as_str().unwrap().contains("/w/app"));
    assert!(c[0]["why"]
        .as_str()
        .unwrap()
        .contains("/w/app/.claude/settings.local.json"));
    assert_eq!(badge_of(&f), 0);
}

/// 藏钥匙：一行里每条插好钥匙的我们那一形地址，钥匙段换成 `…`、其余逐字不变；不是那一形的地址原样。
#[test]
fn hiding_the_key_replaces_only_the_key_segment_of_our_keyed_urls() {
    let k1 = "0123456789abcdef".repeat(4);
    let k2 = "fedcba9876543210".repeat(4);
    let json =
        format!(r#"    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/{k1}/t/claude-code/_","#);
    assert_eq!(
        hide_key(&json),
        r#"    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/…/t/claude-code/_","#
    );
    let toml = format!(
        "openai_base_url = \"http://127.0.0.1:8788/{k1}/s/codex/w\" # http://127.0.0.1:1/{k2}/t/a/b/~tok"
    );
    assert_eq!(
        hide_key(&toml),
        "openai_base_url = \"http://127.0.0.1:8788/…/s/codex/w\" # http://127.0.0.1:1/…/t/a/b/~tok"
    );
    for plain in [
        format!("x http://127.0.0.1:8788/{}/t/claude-code/_ y", &k1[..63]),
        format!("http://localhost:8788/{k1}/t/claude-code/_"),
        format!("http://127.0.0.1:8788/{k1}/x/a/b"),
        "http://127.0.0.1:8788/t/claude-code/_".to_string(),
        format!("token = \"{k1}\""),
        "中文 http://127.0.0.1:".to_string(),
    ] {
        assert_eq!(hide_key(&plain), plain, "不是那一形 ⇒ 原样");
    }
}
