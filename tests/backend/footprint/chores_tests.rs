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
        hooks: vec![],
        mcp_login: vec![],
        zero_trust: vec![],
        outside_library: vec![],
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
    f.hooks = vec![Hooks {
        kind: "claude".into(),
        ccbus: false,
        installed: false,
        path: "/h/.claude/settings.json".into(),
        text: None,
        items: vec![],
        trust: None,
    }];
    let c = chores(&f);
    assert_eq!(c[0]["id"], json!("cc-bus-hooks:claude"));
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
    f.hooks = vec![Hooks {
        kind: "claude".into(),
        ccbus: true,
        installed: false,
        path: "/h/s.json".into(),
        text: Some(text),
        items: vec![(vec!["hooks".into(), "Stop".into()], json!({"hooks": []}))],
        trust: None,
    }];
    let c = chores(&f);
    let whole: Value = serde_json::from_str(c[0]["whole"].as_str().unwrap()).unwrap();
    assert_eq!(whole["env"]["ANTHROPIC_BASE_URL"], json!("u"));
    assert_eq!(whole["hooks"]["Stop"], json!([{"hooks": []}]));
    assert_eq!(c[0]["whole"], c[1]["whole"]);
    assert_eq!(
        c[0]["wholeCovers"],
        json!(["relay:claude-code", "cc-bus-hooks:claude"])
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
            kind: "claude",
            command: "/mcp".into(),
        },
        McpLogin {
            name: "m-b".into(),
            who: vec![],
            kind: "claude",
            command: "/mcp".into(),
        },
    ];
    let c = chores(&f);
    assert_eq!(
        ids(&c),
        vec!["mcp-login:claude:m-a", "mcp-login:claude:m-b"]
    );
    assert!(c.iter().all(|x| x["kind"] == json!("optional")
        && x["action"] == json!("copyCommand")
        && x["copy"] == json!("/mcp")));
    assert_eq!(
        c[0]["name"],
        json!(copy_text("beChore.mcpLogin.name", &[("name", "m-a")]))
    );
    assert_eq!(c[0]["loc"], json!("p、w"));
    assert_eq!(badge_of(&f), 0);
    f.declined = vec!["mcp-login:claude:m-b".into()];
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
    f.hooks = vec![Hooks {
        kind: "claude".into(),
        ccbus: false,
        installed: false,
        path: "/h/.claude/settings.json".into(),
        text: None,
        items: vec![],
        trust: None,
    }];
    f.mcp_login = vec![McpLogin {
        name: "m".into(),
        who: vec![],
        kind: "claude",
        command: "/mcp".into(),
    }];
    f.declined = vec!["mcp-login:claude:m".into()];
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
            "cc-bus-hooks:claude",
            "dataPage.state.blocked",
            "optional",
            "muted",
        ),
        (
            "mcp-login:claude:m",
            "dataPage.kind.optional",
            "optional",
            "muted",
        ),
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

/// 待办里登录 MCP 那一件复制的那一行照那一家的登录那一格拼（服务器名进 argv、要引的引上），不认哪一家；
/// 那一家没声明登录那一格 ⇒ 不出那一件。替身家：一家声明了、一家没声明。
#[test]
fn mcp_登录那一件照那一家的登录格拼_没声明就不出() {
    use crate::agents::fake;
    let reg = [
        fake::family_row("alpha", "CCM_A_DIR", "a", true, || None),
        crate::agents::Adapter {
            mcp: Some(fake::MCP_NO_LOGIN),
            ..fake::family_row("beta", "CCM_B_DIR", "b", false, || None)
        },
    ];
    let logins = super::gather::mcp_login_among(&reg, |kind| {
        vec![crate::accounts::manage::mcp_share_exec::McpNeed {
            name: format!("{kind} srv"),
            scope: "user",
            login_in: vec![],
            project: None,
        }]
    });
    let mut f = base();
    f.mcp_login = logins;
    let c = chores(&f);
    let mcp: Vec<(&Value, &Value)> = c
        .iter()
        .filter(|x| {
            x["id"]
                .as_str()
                .is_some_and(|s| s.starts_with("mcp-login:"))
        })
        .map(|x| (&x["id"], &x["copy"]))
        .collect();
    assert_eq!(
        mcp,
        vec![(
            &json!("mcp-login:alpha:alpha srv"),
            &json!(format!("{} mcp-auth 'alpha srv'", fake::DEFAULT_COMMAND))
        )],
        "声明了登录的那一家照它拼、没声明的那一家不出"
    );
}

/// 声明了信任的那一家：钩子那一件之后跟一件信任（id 带那一家）；没对上 ⇒ 照那一家并好的整份出改法、复制的就是加的那几行；
/// 对上 ⇒ 已做；cc-bus 没装 ⇒ 不出（钩子那一件先决）。
#[test]
fn 收信钩子的信任_没对上出改法_对上已做_没装不出() {
    let hooks = |ccbus: bool, trusted: bool| Hooks {
        kind: "fake".into(),
        ccbus,
        installed: true,
        path: "/h/a/hooks.json".into(),
        text: Some("{}".into()),
        items: vec![],
        trust: Some(HookTrustTodo {
            path: "/h/a/trust.txt".into(),
            text: Some("x = 1\n".into()),
            trusted,
            whole: Some("x = 1\nk = v\n".into()),
        }),
    };
    let mut f = base();
    f.hooks = vec![hooks(true, false)];
    let c = chores(&f);
    let t = c
        .iter()
        .find(|x| x["id"] == json!("cc-bus-trust:fake"))
        .expect("信任那一件");
    assert_eq!(t["state"], json!("todo"));
    assert_eq!(t["file"], json!("/h/a/trust.txt"));
    assert_eq!(t["copy"], json!("k = v"));
    assert_eq!(t["whole"], json!("x = 1\nk = v\n"));
    f.hooks = vec![hooks(true, true)];
    let c = chores(&f);
    let t = c
        .iter()
        .find(|x| x["id"] == json!("cc-bus-trust:fake"))
        .unwrap();
    assert_eq!(t["state"], json!("done"));
    f.hooks = vec![hooks(false, false)];
    assert!(chores(&f)
        .iter()
        .all(|x| x["id"] != json!("cc-bus-trust:fake")));
}

/// 几家各一份钩子：合成「整份」的是落在同一份文件的那两件 —— 别家同样有实时显示、钩子已装的那一份不掺进来。
#[test]
fn 同一份文件两件_整份只拼那一份文件里的两件() {
    let mut f = base();
    let x = "{\n  \"model\": \"x\"\n}\n".to_string();
    let y = "{\n  \"model\": \"y\"\n}\n".to_string();
    f.relay = vec![
        claude_relay("absent", "/h/x.json", Some(&x), Some("u")),
        claude_relay("absent", "/h/y.json", Some(&y), Some("u")),
    ];
    let hooks = |kind: &str, path: &str, text: &str, installed: bool| Hooks {
        kind: kind.into(),
        ccbus: true,
        installed,
        path: path.into(),
        text: Some(text.into()),
        items: vec![(vec!["onStop".into()], json!({ "run": kind }))],
        trust: None,
    };
    f.hooks = vec![
        hooks("a", "/h/x.json", &x, true),
        hooks("b", "/h/y.json", &y, false),
    ];
    let c = chores(&f);
    let b = c
        .iter()
        .find(|x| x["id"] == json!("cc-bus-hooks:b"))
        .expect("乙那一件");
    let whole: Value = serde_json::from_str(b["whole"].as_str().unwrap()).unwrap();
    assert_eq!(whole["model"], json!("y"), "{whole}");
    assert_eq!(whole["onStop"], json!([{ "run": "b" }]), "{whole}");
}

/// 两个替身家各两个号（同 `trust_share_exec_tests` 那一份：`fa` 记 JSON、账号 0 住 `shared/`；`fb` 记行式、账号 0 住 `$HOME`）。
/// 同步不写账号 0 ⇒ 其余号信任过、账号 0 未记上的目录进待办：每家一件（可选 · 复制片段，diff 只加缺的那一格，整份不下发）；
/// 账号 0 补齐后那一家不出。
#[test]
fn 账号_0_缺的信任每家一件_补齐后不出() {
    use crate::accounts::manage::model::{Account, Entry, Manifest};
    use crate::accounts::manage::trust_share_exec::zero_gaps_among;
    let home = test_home::scratch("zero-trust").canonicalize().unwrap();
    let h = home.display().to_string();
    let write = |rel: &str, body: &str| {
        let p = home.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    let fa = crate::agents::fake::family_row("fa", "CCM_FA_DIR", "fa", true, || None);
    let fb_face = crate::agents::AccountsFace {
        identity: &[(
            "fake.conf",
            crate::agents::IdentityRoot::Home,
            crate::agents::IdentityClass::State,
        )],
        config_file: "fake.conf",
        trust: Some(crate::agents::fake::LINE_TRUST_CELLS),
        ..fa.accounts.expect("替身家有账号面")
    };
    let fb = crate::agents::Adapter {
        accounts: Some(fb_face),
        ..crate::agents::fake::family_row("fb", "CCM_FB_DIR", "fb", false, || None)
    };
    let registry = [fa, fb];
    write(
        "shared/fake-config.json",
        "{\"trusted\": {\"/a0\": true}}\n",
    );
    write("fake.conf", "trust /b0\n");
    write(
        ".cc-monitor/accounts/a1/fake-config.json",
        "{\"trusted\": {\"/a0\": true, \"/a1\": true}}\n",
    );
    write(
        ".cc-monitor/accounts/a2/fake-config.json",
        "{\"trusted\": {\"/a0\": true}}\n",
    );
    write(
        ".cc-monitor/accounts/b1/fake.conf",
        "trust /b0\ntrust /b1\n",
    );
    write(".cc-monitor/accounts/b2/fake.conf", "trust /b0\n");
    let row = |name: &str, kind: &str| {
        Entry::Managed(Account {
            name: name.to_string(),
            email: String::new(),
            config_dir: format!("{h}/.cc-monitor/accounts/{name}"),
            is_default: false,
            auth_kind: None,
            color: None,
            agent: Some(kind.to_string()),
            extra: Default::default(),
        })
    };
    let m = Manifest {
        entries: vec![
            row("a1", "fa"),
            row("b1", "fb"),
            row("a2", "fa"),
            row("b2", "fb"),
        ],
        ..Manifest::default()
    };
    let todo = |gaps| {
        chores(&Facts {
            zero_trust: gaps,
            ..base()
        })
    };

    let items = todo(zero_gaps_among(&h, &registry, &m));
    let ids: Vec<&str> = items.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["zero-trust:fa", "zero-trust:fb"], "{items:?}");
    let a = &items[0];
    assert_eq!(a["kind"], json!("optional"));
    assert_eq!(a["state"], json!("todo"));
    assert_eq!(a["action"], json!("copySnippet"));
    assert_eq!(a["file"], json!(format!("{h}/shared/fake-config.json")));
    assert_eq!(a["whole"], Value::Null, "账号 0 那一份整份不下发");
    let added: Vec<&str> = a["diff"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == json!("add"))
        .map(|r| r["text"].as_str().unwrap())
        .collect();
    assert!(added.iter().any(|l| l.contains("/a1")), "{added:?}");
    assert!(
        !added.iter().any(|l| l.contains("/b1")),
        "别家的目录进了 fa 那一件：{added:?}"
    );
    let b = &items[1];
    assert_eq!(b["file"], json!(format!("{h}/fake.conf")));
    assert!(b["copy"].as_str().unwrap().contains("trust /b1"), "{b}");

    write(
        "shared/fake-config.json",
        "{\"trusted\": {\"/a0\": true, \"/a1\": true}}\n",
    );
    write("fake.conf", "trust /b0\ntrust /b1\n");
    assert!(
        todo(zero_gaps_among(&h, &registry, &m)).is_empty(),
        "补齐了还出"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 清单里号目录指到账号库外的号：一个号一件「需手动」，指着清单那一份、说改回账号库里的目录；没有这样的号不出。
#[test]
fn 号目录在账号库外的号一号一件_指着清单() {
    use crate::accounts::manage::trust_share_exec::OutsideLibrary;
    let o = |name: &str| OutsideLibrary {
        name: name.into(),
        dir: format!("/h/.own-{name}"),
        manifest: "/h/.cc-monitor/accounts/accounts.json".into(),
        accts: "/h/.cc-monitor/accounts".into(),
        in_place: false,
    };
    let items = chores(&Facts {
        outside_library: vec![o("p"), o("q")],
        ..base()
    });
    // 旧的就地号单列那一句（说明那一格换一句，别的同库外那一件）。
    let in_place = chores(&Facts {
        outside_library: vec![OutsideLibrary {
            in_place: true,
            ..o("p")
        }],
        ..base()
    });
    assert_eq!(
        in_place[0]["why"],
        json!(copy_core::copy_text(
            "beChore.outsideLibrary.inPlace",
            &[("path", "/h/.own-p"), ("accts", "/h/.cc-monitor/accounts")]
        ))
    );
    assert_ne!(in_place[0]["why"], items[0]["why"]);
    let ids: Vec<&str> = items.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["outside-library:p", "outside-library:q"], "{items:?}");
    let p = &items[0];
    assert_eq!(p["kind"], json!("optional"));
    assert_eq!(p["state"], json!("todo"));
    assert_eq!(p["action"], json!("locate"));
    assert_eq!(p["file"], json!("/h/.cc-monitor/accounts/accounts.json"));
    assert_eq!(p["copy"], json!("/h/.cc-monitor/accounts/accounts.json"));
    assert_eq!(p["steps"].as_array().map(Vec::len), Some(2), "{p}");
    assert!(chores(&base())
        .iter()
        .all(|c| !c["id"].as_str().unwrap().starts_with("outside-library:")));
}
