use super::*;
use std::process::Command;

fn row(id: &str, name: &str, sid: &str, client: &str, program: &str) -> TermRow {
    TermRow {
        id: id.into(),
        pane: None,
        shared: false,
        name: name.into(),
        windows: 1,
        activity: 100,
        sid: sid.into(),
        agent: if sid.is_empty() {
            String::new()
        } else {
            "claude".into()
        },
        client: client.into(),
        program: program.into(),
        cwd: "/p".into(),
        title: "t".into(),
    }
}

fn golden() -> Value {
    serde_json::from_str(include_str!("../../__fixtures__/terminals.golden.json"))
        .expect("金样读不懂")
}

fn codes_of(name: &str) -> Vec<String> {
    crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("注册表里没有 {name}"))
        .codes
        .iter()
        .map(|c| c.to_string())
        .collect()
}

/// 名单原文 ⇒ 行：读得懂的按名字排好；读不懂的不猜、记成「没报全」。
#[test]
fn the_list_reads_only_well_formed_rows_and_says_when_it_is_partial() {
    let (rows, odd) = parse_rows("$2\t%2\t1\t1\tb-cc\t1\t5\t\t\t\tbash\t/b\tt\n$1\t%1\t1\t1\ta-cc\t2\t6\ts\tclaude\tccm\tclaude\t/a\twith\ttab\n");
    assert!(!odd);
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["a-cc", "b-cc"]
    );
    assert_eq!(
        rows[0].title, "with\ttab",
        "标题是最后一格自由文本，里面的 TAB 不许把它切断"
    );
    assert_eq!(rows[0].windows, 2);
    for bad in [
        "x\t%0\t1\t1\tno-dollar\t1\t1\t\t\t\tsh\t/\tt\n",
        "$1\tno-pct\t1\t1\tn\t1\t1\t\t\t\tsh\t/\tt\n",
        "$1\t%1\t1\t1\tshort\t1\n",
        "$1\t%1\t1\t1\tn\tW\t1\t\t\t\tsh\t/\tt\n",
    ] {
        let (rows, odd) = parse_rows(bad);
        assert!(rows.is_empty() && odd, "{bad:?}");
    }
}

/// 一个 tmux 会话里几个窗格各跑一个 claude：每个挂着 sid 的窗格各是一个终端（句柄带窗格、对它下手），活动窗格是个 shell 也不影响；
/// 同一个 sid 挂在几个窗格上（会话那一级的旧标签往下透）只算一个；一个都没挂的会话还是一个终端（看它当前的窗格）。
#[test]
fn each_pane_that_carries_a_sid_is_its_own_terminal() {
    let text = "$1\t%1\t0\t1\ttwo-cc\t1\t9\tsid-a\tclaude\tccm\tclaude\t/p\ta\n\
                $1\t%2\t0\t1\ttwo-cc\t1\t9\tsid-b\tclaude\tccm\tclaude\t/p\tb\n\
                $1\t%3\t1\t1\ttwo-cc\t1\t9\t\tclaude\tccm\tbash\t/p\tsh\n\
                $2\t%4\t0\t1\told-cc\t1\t9\tsid-l\t\t\tbash\t/q\tx\n\
                $2\t%5\t1\t1\told-cc\t1\t9\tsid-l\t\t\tclaude\t/q\ty\n\
                $3\t%6\t0\t1\tplain\t1\t9\t\t\t\tvim\t/r\tv\n\
                $3\t%7\t1\t1\tplain\t1\t9\t\t\t\tbash\t/r\tw\n";
    let (rows, odd) = parse_rows(text);
    assert!(!odd);
    let got: Vec<(String, &str, &str, bool)> = rows
        .iter()
        .map(|r| (r.handle(), r.target(), r.sid.as_str(), r.shared))
        .collect();
    assert_eq!(
        got,
        vec![
            ("tmux-2-5".to_string(), "%5", "sid-l", false),
            ("tmux-3".to_string(), "$3", "", false),
            ("tmux-1-1".to_string(), "%1", "sid-a", true),
            ("tmux-1-2".to_string(), "%2", "sid-b", true),
        ]
    );
    assert_eq!(rows[1].program, "bash", "没挂 sid 的会话看它当前的窗格");
    let t = |v: Value| target_of(&v).expect("形状对");
    assert_eq!(
        find(&rows, &t(json!({ "sid": "sid-b" }))),
        Found::One(&rows[3])
    );
    // 结束得了：同会话里还有别的 claude ⇒ 只关这一个窗格（不看窗口数）。
    let mut wide = rows[2].clone();
    wide.windows = 3;
    assert_eq!(terminal_json(&wide, &[], None)["can"]["end"], true);
}

/// 目标恰好给一个；句柄与 sid 只在名单里对，对不上就是「不在名单」，同一个 sid 落在两个终端上不猜。
#[test]
fn a_target_is_one_handle_or_one_sid_from_the_list_and_nothing_else() {
    for bad in [
        json!({}),
        json!({ "terminal": "tmux-1", "sid": "s" }),
        json!({ "terminal": "" }),
        json!({ "terminal": 1 }),
        json!({ "sid": "x".repeat(300) }),
    ] {
        assert_eq!(
            target_of(&bad).map_err(|e| e.code),
            Err("bad_target".to_string()),
            "{bad}"
        );
    }
    let rows = vec![
        row("$1", "a-cc", "s1", "", "claude"),
        row("$2", "b-cc", "s2", "", "claude"),
        row("$3", "c-cc", "s2", "", "claude"),
    ];
    let t = |v: Value| target_of(&v).expect("形状对");
    assert_eq!(
        find(&rows, &t(json!({ "terminal": "tmux-1" }))),
        Found::One(&rows[0])
    );
    assert_eq!(
        find(&rows, &t(json!({ "sid": "s1" }))),
        Found::One(&rows[0])
    );
    assert_eq!(find(&rows, &t(json!({ "sid": "s2" }))), Found::Ambiguous);
    // 任意 tmux 目标串不是句柄：一律「不在名单」，到不了 tmux。
    for raw in ["$1", "=a-cc:", "a-cc", "%0", "tmux-1 ", "tmux-9"] {
        assert_eq!(
            find(&rows, &t(json!({ "terminal": raw }))),
            Found::NotKnown,
            "{raw}"
        );
    }
}

/// 送字与送键分开、恰好一个；键只认那张有限键表；字面字不许夹控制字符（换行、制表除外）。
#[test]
fn text_and_keys_are_separate_and_keys_come_from_a_closed_table() {
    assert_eq!(
        input_of(&json!({ "text": "/usage" })),
        Ok(Input::Text {
            text: "/usage".into(),
            enter: true
        })
    );
    assert_eq!(
        input_of(&json!({ "text": "2", "enter": false })),
        Ok(Input::Text {
            text: "2".into(),
            enter: false
        })
    );
    for (wire, tmux) in KEYS {
        assert_eq!(input_of(&json!({ "key": wire })), Ok(Input::Key(tmux)));
    }
    let wires: Vec<&str> = KEYS.iter().map(|(w, _)| *w).collect();
    assert_eq!(
        wires,
        [
            "esc",
            "ctrl-c",
            "ctrl-d",
            "up",
            "down",
            "left",
            "right",
            "tab",
            "shift-tab",
            "enter",
            "backspace",
            "page-up",
            "page-down"
        ]
    );
    for bad in [
        json!({}),
        json!({ "text": "a", "key": "esc" }),
        json!({ "key": "C-c" }),
        json!({ "key": "\u{1b}[A" }),
        json!({ "key": "esc", "enter": true }),
        json!({ "text": "" }),
        json!({ "text": "a\u{1b}b" }),
        json!({ "text": "a", "enter": "yes" }),
    ] {
        assert_eq!(
            input_of(&bad).map_err(|e| e.code),
            Err("bad_args".to_string()),
            "{bad}"
        );
    }
}

/// 名单那一行的 `mine` / `can` 与身份门同一个判断：别的前端起的只读；自己起的能动；用户终端起的照名字规则。
#[test]
fn each_row_says_whether_this_caller_may_act_on_it() {
    let shared = row("$1", "demo-cc", "s", "ccm", "claude");
    let theirs = row("$2", "pipe-x", "s2", "mobile", "claude");
    let plain = row("$3", "work", "", "", "bash");
    let mut wide = row("$4", "w-cc", "", "", "vim");
    wide.windows = 2;
    let see = |r: &TermRow, who: Option<&str>| terminal_json(r, &[], who);
    assert_eq!(
        see(&shared, Some("mobile"))["started_by"],
        json!({ "client": "ccm", "mine": true })
    );
    assert_eq!(
        see(&theirs, None)["started_by"],
        json!({ "client": "mobile", "mine": false })
    );
    assert_eq!(
        see(&theirs, None)["can"]["input"],
        json!({ "no": "not_yours", "said": no_said("not_yours") })
    );
    assert_eq!(see(&theirs, Some("mobile"))["can"]["input"], json!(true));
    assert_eq!(
        see(&plain, None)["can"]["end"],
        json!({ "no": "not_managed", "said": no_said("not_managed") })
    );
    assert_eq!(see(&plain, None)["started_by"]["client"], Value::Null);
    assert_eq!(
        see(&wide, None)["can"]["end"],
        json!({ "no": "other_windows", "said": no_said("other_windows") })
    );
    assert_eq!(see(&wide, None)["can"]["input"], json!(true));
    assert_eq!(see(&plain, None)["state"], "idle");
    assert_eq!(
        see(&row("$5", "x-cc", "s", "", "zsh"), None)["state"],
        "program_exited"
    );
    assert_eq!(see(&shared, None)["state"], "running");
    assert_eq!(see(&shared, None)["title"], "t");
    assert_eq!(
        see(&plain, None)["title"],
        "bash",
        "没会话 ⇒ 标题是前台程序"
    );
    assert!(see(&plain, None).get("session").is_none());
    assert_eq!(
        see(&shared, None)["session"],
        json!({ "sid": "s", "agent": "claude" })
    );
    for r in [&shared, &theirs, &plain] {
        let t = see(r, None);
        assert!(
            !t.to_string().contains(&r.id),
            "回话里漏出了 tmux 目标串：{t}"
        );
    }
}

/// 带颜色那一屏 ⇒ 纯文字 ＋ 属性段；指纹不看颜色（带不带颜色抓的同一屏指纹相同）。
#[test]
fn colours_become_spans_and_the_fingerprint_ignores_them() {
    let mut st = Style::default();
    let (text, spans) = parse_line(
        "\u{1b}[1;31mab\u{1b}[0mcd\u{1b}[38;5;196me\u{1b}[48;2;1;2;3mf\u{1b}]8;;http://x\u{7}g",
        &mut st,
    );
    assert_eq!(text, "abcdefg");
    assert_eq!(
        spans,
        vec![
            json!({ "from": 0, "to": 2, "fg": "red", "bold": true }),
            json!({ "from": 4, "to": 5, "fg": "#ff0000" }),
            json!({ "from": 5, "to": 7, "fg": "#ff0000", "bg": "#010203" }),
        ]
    );
    let plain = |t: &str| -> Vec<String> {
        let mut s = Style::default();
        split_lines(t)
            .into_iter()
            .map(|l| parse_line(l, &mut s).0)
            .collect()
    };
    assert_eq!(
        fingerprint(&plain("\u{1b}[32mok\u{1b}[0m  \nnext\n"), 2),
        fingerprint(&plain("ok\nnext\n"), 2)
    );
    assert_ne!(
        fingerprint(&plain("ok\nnext\n"), 2),
        fingerprint(&plain("ok\nnexT\n"), 2)
    );
    assert_eq!(
        fingerprint(&plain("old\nok\nnext\n"), 2),
        fingerprint(&plain("ok\nnext\n"), 2),
        "只看可见那几行"
    );
}

/// 金样：请求过解析、原料喂纯构造器 == 成品、码集合 == 注册表。
#[test]
fn the_products_match_the_golden() {
    let g = golden();
    let l = &g["terminals-list"];
    let (rows, odd) = parse_rows(l["panes"].as_str().unwrap());
    let clients = parse_clients(l["clients"].as_str().unwrap());
    let who = crate::control::gate::requester_of(&l["request"]).unwrap();
    let got = list_reply(&rows, &clients, !odd, who.as_deref());
    assert_eq!(
        got,
        l["reply"],
        "terminals-list 成品：{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
    assert_eq!(
        serde_json::to_value(codes_of("terminals-list")).unwrap(),
        l["codes"]
    );

    let p = &g["terminal-preview"];
    assert!(target_of(&p["request"]).is_ok());
    let geo = &p["geometry"];
    let view = View {
        text: p["capture"].as_str().unwrap().into(),
        cols: geo["cols"].as_u64().unwrap() as u32,
        rows: geo["rows"].as_u64().unwrap() as u32,
        cursor_x: geo["x"].as_u64().unwrap() as u32,
        cursor_y: geo["y"].as_u64().unwrap() as u32,
        cursor_visible: geo["visible"].as_bool().unwrap(),
    };
    let got = preview_reply(
        &view,
        true,
        false,
        p["captured_at"].as_u64().unwrap(),
        p["captured_at_text"].as_str().unwrap(),
    );
    assert_eq!(
        got,
        p["reply"],
        "terminal-preview 成品：{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
    assert_eq!(
        serde_json::to_value(codes_of("terminal-preview")).unwrap(),
        p["codes"]
    );

    let i = &g["terminal-input"];
    for r in i["requests"].as_array().unwrap() {
        assert!(target_of(r).is_ok() && input_of(r).is_ok(), "{r}");
    }
    let built = [
        input_reply("delivered", None, None),
        input_reply("refused", Some("screen_changed"), Some("0000000000000000")),
        input_reply("unsure", None, None),
    ];
    assert_eq!(serde_json::to_value(built).unwrap(), i["replies"]);
    assert_eq!(
        serde_json::to_value(codes_of("terminal-input")).unwrap(),
        i["codes"]
    );
}

// ═══ 真 tmux（隔离 socket，显式 `-S`；不碰任何默认 socket；收尾按 server 的 pid 收）═══

struct Iso {
    sock: std::path::PathBuf,
}

impl Iso {
    fn new(tag: &str) -> Iso {
        let dir = std::env::temp_dir().join(format!("ccm-term-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        Iso {
            sock: dir.join("s"),
        }
    }
    fn on(&self) -> On<'_> {
        On {
            socket: self.sock.to_str(),
        }
    }
    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .arg("-S")
            .arg(&self.sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本测试要求环境有 tmux")
    }
    /// 起一个会话：前台一个 `cat`（回显打进去的字），可选声明 `@ccm_client` / 设 `@ccm_sid`。
    fn session(&self, name: &str, client: &str, sid: &str) {
        let o = self.tmux(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-x",
            "80",
            "-y",
            "10",
            "-s",
            name,
            "cat",
        ]);
        assert!(
            o.status.success(),
            "建会话失败：{}",
            String::from_utf8_lossy(&o.stderr)
        );
        if !client.is_empty() {
            self.tmux(&[
                "set-option",
                "-t",
                &format!("={name}:"),
                "@ccm_client",
                client,
            ]);
        }
        if !sid.is_empty() {
            self.tmux(&["set-option", "-t", &format!("={name}:"), "@ccm_sid", sid]);
        }
    }
    fn handle_of(&self, name: &str) -> String {
        let l = list_on(self.on(), &json!({})).expect("名单");
        l["terminals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["tmux_name"] == name)
            .unwrap_or_else(|| panic!("名单里没有 {name}：{l}"))["terminal"]
            .as_str()
            .unwrap()
            .to_string()
    }
    /// 有界等待（夹具侧）：那一屏里出现了 `needle`。
    fn shows(&self, handle: &str, needle: &str) -> bool {
        self.shows_times(handle, needle, 1)
    }
    /// 有界等待（夹具侧）：那一屏里 `needle` 至少出现了 `times` 次。
    fn shows_times(&self, handle: &str, needle: &str, times: usize) -> bool {
        (0..150).any(|_| {
            let p = preview_on(self.on(), &json!({ "terminal": handle, "color": false }));
            let hit = p.is_ok_and(|v| v["lines"].to_string().matches(needle).count() >= times);
            if !hit {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            hit
        })
    }
}

impl Drop for Iso {
    fn drop(&mut self) {
        let o = self.tmux(&["display-message", "-p", "#{pid}"]);
        let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
            let _ = Command::new("kill").arg(&pid).status();
        }
        let _ = std::fs::remove_dir_all(self.sock.parent().unwrap());
    }
}

/// 真 tmux 上走一遍：名单有它 · 抓一屏 · 送字真进去 · 送键 · 画面变了不送 · 别的前端的不送 · 句柄之外的串到不了 tmux。
#[test]
fn on_a_real_tmux_the_three_commands_do_what_they_say() {
    let iso = Iso::new("live");
    iso.session("mif-cc", "", "");
    iso.session("pipe-x", "mobile", "sid-p");
    let h = iso.handle_of("mif-cc");
    let theirs = iso.handle_of("pipe-x");

    let l = list_on(iso.on(), &json!({})).unwrap();
    assert_eq!(l["complete"], true);
    assert_eq!(l["terminals"].as_array().unwrap().len(), 2);

    // 送字：原样进去（`--` 打头的也不被当成 tmux 的旗），回 delivered。
    let r = input_on(iso.on(), &json!({ "terminal": h, "text": "--zq7-marker" })).unwrap();
    assert_eq!(r, json!({ "result": "delivered" }));
    assert!(iso.shows(&h, "--zq7-marker"), "送进去的字没出现在那一屏里");
    // 多行按粘贴送。
    let r = input_on(
        iso.on(),
        &json!({ "terminal": h, "text": "l1-zq\nl2-zq", "enter": true }),
    )
    .unwrap();
    assert_eq!(r["result"], "delivered");
    // 等到 `cat` 把最后一行吐回来（回显一次 ＋ `cat` 自己印一次 = 两次）：只等回显那一次，`cat` 那一行可能还在路上，
    //   下面两次抓屏之间画面会变（负载 40–130 时并发跑红过：带色 / 不带色指纹不同 · 带着旧指纹送被拒成 screen_changed）。
    assert!(
        iso.shows_times(&h, "l2-zq", 2),
        "cat 没把多行粘贴的最后一行印回来"
    );

    // 抓一屏：带色与不带色同一屏指纹相同；尺寸是那个窗格的。
    let a = preview_on(iso.on(), &json!({ "terminal": h, "color": true })).unwrap();
    let b = preview_on(iso.on(), &json!({ "terminal": h, "color": false })).unwrap();
    assert_eq!(a["screen"], b["screen"]);
    assert_eq!(
        (a["cols"].as_u64(), a["rows"].as_u64()),
        (Some(80), Some(10))
    );
    assert!(b["lines"][0].get("spans").is_none());
    let big = preview_on(iso.on(), &json!({ "terminal": h, "scrollback": 999_999 })).unwrap();
    assert_eq!(big["capped"], true);

    // 看见的那一屏没变 ⇒ 送；变了 ⇒ 不送、交回新指纹。
    let seen = b["screen"].as_str().unwrap();
    let r = input_on(
        iso.on(),
        &json!({ "terminal": h, "text": "keep-zq", "enter": false, "seen_screen": seen }),
    )
    .unwrap();
    assert_eq!(r["result"], "delivered");
    assert!(iso.shows(&h, "keep-zq"));
    let r = input_on(
        iso.on(),
        &json!({ "terminal": h, "text": "never-zq", "seen_screen": seen }),
    )
    .unwrap();
    assert_eq!(r["result"], "refused");
    assert_eq!(r["why"], "screen_changed");
    assert_eq!(r["said"], no_said("screen_changed"));
    assert_ne!(r["screen"], json!(seen));

    // 送键：ctrl-c 把前台的 cat 停掉 ⇒ 那个会话随之结束、从名单里消失。
    let r = input_on(iso.on(), &json!({ "terminal": h, "key": "ctrl-c" })).unwrap();
    assert_eq!(r["result"], "delivered");
    let gone = (0..150).any(|_| {
        let l = list_on(iso.on(), &json!({})).unwrap();
        let still = l["terminals"].to_string().contains("mif-cc");
        if still {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        !still
    });
    assert!(gone, "ctrl-c 没送到（cat 还在）");

    // 别的前端起的：不报自己 / 报错了都不送；报对了才送。
    for who in [json!(null), json!("desktop")] {
        let mut args = json!({ "terminal": theirs, "text": "nope-zq" });
        if !who.is_null() {
            args["client"] = who;
        }
        assert_eq!(
            input_on(iso.on(), &args).unwrap(),
            json!({ "result": "refused", "why": "not_yours", "said": no_said("not_yours") })
        );
    }
    let r = input_on(
        iso.on(),
        &json!({ "sid": "sid-p", "text": "mine-zq", "client": "mobile" }),
    )
    .unwrap();
    assert_eq!(r["result"], "delivered");
    assert!(iso.shows(&theirs, "mine-zq"));
    let p = preview_on(iso.on(), &json!({ "terminal": theirs, "color": false })).unwrap();
    assert!(
        !p["lines"].to_string().contains("nope-zq"),
        "被拒的那两次还是打进去了"
    );

    // 句柄之外的串：不在名单 ⇒ 不送、不抓。
    for raw in ["=pipe-x:", "$1", "%0", "pipe-x"] {
        assert_eq!(
            input_on(iso.on(), &json!({ "terminal": raw, "text": "x" })).unwrap(),
            json!({ "result": "refused", "why": "not_known", "said": no_said("not_known") })
        );
        assert_eq!(
            preview_on(iso.on(), &json!({ "terminal": raw })).map_err(|e| e.code),
            Err("not_known".to_string())
        );
    }
}

/// 真 tmux：一个 tmux 会话里两个窗格各跑一个「claude」（`cat`，窗格级标签各挂一个 sid），活动窗格是另一个 ⇒
/// 名单里两个终端；按 sid 抓屏 / 送字都落在挂着它的那个窗格，另一个窗格一个字都没收到；
/// 名字不像我们铸的会话，sid 只挂在非活动窗格上（活动窗格是个没标签的）⇒ 身份按挂着 sid 的那个窗格判，照样送得进。
#[test]
fn on_a_real_tmux_a_sid_reaches_its_own_pane_not_the_active_one() {
    let iso = Iso::new("panes");
    let pane_of = |args: &[&str]| -> String {
        let o = iso.tmux(args);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };
    let new = |name: &str| {
        pane_of(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-x",
            "80",
            "-y",
            "10",
            "-s",
            name,
            "-P",
            "-F",
            "#{pane_id}",
            "cat",
        ])
    };
    let a = new("dual-cc");
    let b = pane_of(&[
        "split-window",
        "-t",
        "=dual-cc:",
        "-P",
        "-F",
        "#{pane_id}",
        "cat",
    ]);
    iso.tmux(&["set-option", "-p", "-t", &a, "@ccm_sid", "sid-a"]);
    iso.tmux(&["set-option", "-p", "-t", &b, "@ccm_sid", "sid-b"]);
    iso.tmux(&["select-pane", "-t", &b]);
    let c = new("box");
    let d = pane_of(&[
        "split-window",
        "-t",
        "=box:",
        "-P",
        "-F",
        "#{pane_id}",
        "cat",
    ]);
    iso.tmux(&["set-option", "-p", "-t", &c, "@ccm_sid", "sid-c"]);
    iso.tmux(&["select-pane", "-t", &d]);

    let l = list_on(iso.on(), &json!({})).unwrap();
    let sids: Vec<String> = l["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| format!("{}:{}", t["tmux_name"], t["session"]["sid"]))
        .collect();
    assert_eq!(
        sids,
        [
            "\"box\":\"sid-c\"",
            "\"dual-cc\":\"sid-a\"",
            "\"dual-cc\":\"sid-b\""
        ],
        "{l}"
    );
    let handle = |sid: &str| -> String {
        l["terminals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["session"]["sid"] == sid)
            .unwrap()["terminal"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (ha, hb, hc) = (handle("sid-a"), handle("sid-b"), handle("sid-c"));

    // 按 sid 送字：落在 A 的窗格（活动的是 B）。
    let r = input_on(iso.on(), &json!({ "sid": "sid-a", "text": "to-a-zq" })).unwrap();
    assert_eq!(r, json!({ "result": "delivered" }));
    assert!(iso.shows(&ha, "to-a-zq"), "送进去的字没出现在 A 那一屏");
    let shot_a = preview_on(iso.on(), &json!({ "sid": "sid-a", "color": false })).unwrap();
    assert!(
        shot_a["lines"].to_string().contains("to-a-zq"),
        "按 sid 抓的不是 A 那一屏"
    );
    let shot_b = preview_on(iso.on(), &json!({ "terminal": hb, "color": false })).unwrap();
    assert!(
        !shot_b["lines"].to_string().contains("to-a-zq"),
        "给 A 的字落进了活动窗格 B"
    );

    // 名字不像我们铸的、活动窗格没标签：按挂着 sid 的那个窗格判身份 ⇒ 送得进，且落在它上面。
    let r = input_on(iso.on(), &json!({ "sid": "sid-c", "text": "to-c-zq" })).unwrap();
    assert_eq!(r, json!({ "result": "delivered" }));
    assert!(iso.shows(&hc, "to-c-zq"));
    let p = pane_of(&["capture-pane", "-p", "-t", &d]);
    assert!(!p.contains("to-c-zq"), "给 C 的字落进了活动窗格");
}

/// 容器那一格的句柄 == 名单里同一会话的句柄（同一台、同一窗格）。走打标那一条生产路（`identity_tag::tag`），
/// tmux 换成指向隔离 socket 的那一个；被打标的是非活动窗口里的窗格（句柄必须落在它上面，不是活动的那个）。
#[cfg(target_os = "linux")]
#[test]
fn the_container_handle_is_the_list_handle_of_the_same_pane() {
    use crate::stream::wire::{SessionContainer, TerminalHost};
    let iso = Iso::new("container");
    iso.session("box-cc", "", "");
    let o = iso.tmux(&["new-window", "-t", "=box-cc:", "cat"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let pid_out = iso.tmux(&["display-message", "-p", "-t", "=box-cc:0.0", "#{pane_pid}"]);
    let pid: u32 = String::from_utf8_lossy(&pid_out.stdout)
        .trim()
        .parse()
        .expect("pane_pid");
    // 有界等待（夹具侧）：那个窗格的进程 exec 成 `cat` **而且做完了**（之前读到的环境是 tmux 自己的）。
    //   只看 `comm` 不够：内核在 exec 里先换上新的地址空间、再改名，环境那几页要再往后才摆好 ⇒ 这中间
    //   `comm` 已是 `cat`、`/proc/<pid>/environ` 却读回 0 字节，打标那一下读环境落在这里就判 `PaneUnknown`
    //   （负载 100 时并发跑四百多趟红过一次）。改成名字对上、环境也读得出来才算起好。
    let ready = (0..150).any(|_| {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let env = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        let hit = comm.trim() == "cat" && !env.is_empty();
        if !hit {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        hit
    });
    assert!(ready, "窗格进程没起成 cat");
    let script = iso.sock.parent().unwrap().join("tmux");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nexec tmux -S '{}' \"$@\"\n", iso.sock.display()),
    )
    .expect("写 tmux 转接");
    let _door = crate::control::identity_tag::door::isolate_with(&script);

    let tagged = crate::control::identity_tag::tag(pid, "sid-box");
    let again = crate::control::identity_tag::tag(pid, "sid-box");
    let l = list_on(iso.on(), &json!({})).unwrap();
    let listed = l["terminals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["session"]["sid"] == "sid-box")
        .unwrap_or_else(|| {
            panic!("名单里没有挂着 sid-box 的终端（打标两次：{tagged:?} · {again:?}）：{l}")
        })["terminal"]
        .as_str()
        .unwrap()
        .to_string();
    let want = Some(SessionContainer::Hosted {
        host: TerminalHost::Tmux,
        terminal: Some(listed.clone()),
    });
    assert!(
        matches!(tagged, crate::control::identity_tag::Outcome::Tagged(_)),
        "{tagged:?}"
    );
    assert_eq!(tagged.container(), want, "打标那一刻报的句柄与名单对不上");
    assert!(
        matches!(
            again,
            crate::control::identity_tag::Outcome::AlreadyCurrent(_)
        ),
        "{again:?}"
    );
    assert_eq!(
        again.container(),
        want,
        "已经打过的那一形报的句柄与名单对不上"
    );
    // 正控：活动的是另一个窗口，句柄不许指向它。
    let active = iso.tmux(&["display-message", "-p", "-t", "=box-cc:", "#{pane_id}"]);
    let active = String::from_utf8_lossy(&active.stdout)
        .trim()
        .trim_start_matches('%')
        .to_string();
    assert!(
        !listed.ends_with(&format!("-{active}")),
        "句柄落在了活动窗格上：{listed}"
    );
}

/// 原因码（线上那一形，下划线）各有一句（彼此不同的那几种不同句；`not_known` 与 `ended` 都是「终端已不在」）；认不出的落「被拒」。
#[test]
fn every_refusal_reason_has_its_sentence() {
    let codes = [
        "not_yours",
        "not_managed",
        "other_windows",
        "not_known",
        "ambiguous",
        "screen_changed",
    ];
    let said: Vec<String> = codes.iter().map(|c| no_said(c)).collect();
    let mut uniq = said.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), codes.len(), "两种原因同一句：{said:?}");
    assert_eq!(no_said("ended"), no_said("not_known"));
    assert_eq!(no_said("what"), copy_text("beTerminal.no.other", &[]));
    assert!(!said.contains(&no_said("what")));
    let r = input_reply("refused", Some("not_yours"), None);
    assert_eq!(r["said"], no_said("not_yours"));
}

/// 终端这一族线上的原因码与状态（名单 `can.*.no` · `terminal-input` 的 `why` · 名单行的 `state`）和同族命令级码一个写法：
/// 小写 ＋ 下划线，不带短横。从金样里真出的成品逐格取，不另列一份码表。
#[test]
fn terminal_reason_codes_use_underscores_like_command_codes() {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if matches!(k.as_str(), "no" | "why" | "state") {
                        if let Some(s) = x.as_str() {
                            out.push(s.to_string());
                        }
                    }
                    walk(x, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut codes = Vec::new();
    walk(&golden(), &mut codes);
    assert!(
        codes.len() >= 4,
        "金样里取到的码太少（{codes:?}）—— 本条在空转"
    );
    for c in &codes {
        assert!(
            !c.is_empty() && c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'),
            "终端码 `{c}` 不是小写下划线写法"
        );
    }
    for code in codes_of("terminal-input")
        .iter()
        .chain(codes_of("terminals-list").iter())
    {
        assert!(!code.contains('-'), "命令级码 `{code}` 带短横");
    }
}

/// ★ `terminal-input` 只收它真用得上的那几格；`take`（「先接管输入」）删了 —— 原先收了、校验了、然后丢掉，
/// 第二个前端照字段表画了一颗按钮、按下去什么都不变。再送它 ⇒ 照「多送一格就拒」回 `bad_args`，在碰 tmux 之前。
#[test]
fn terminal_input_refuses_fields_it_does_not_use() {
    let on = On {
        socket: Some("/nonexistent/cli2nd/tmux.sock"),
    };
    for extra in ["take", "nonsense"] {
        let mut args = json!({ "terminal": "tmux-1", "text": "x" });
        args[extra] = json!(true);
        let (e, diag) = crate::common::contract::tests::diag(|| {
            input_on(on, &args).expect_err("多送的一格该被拒")
        });
        assert_eq!(e.code, "bad_args", "{extra}：码不对：{e:?}");
        assert!(
            diag.contains(extra),
            "{extra}：诊断没点名多出来的那一格：{diag}"
        );
    }
}

/// ★ 抓屏不过身份门、恒可用 ⇒ 不在「这个调用方能做什么」（`can`）里占一格：放在那里读起来像会变的能力，实际恒真。
#[test]
fn can_holds_only_what_can_vary() {
    for r in [
        row("$1", "a-cc", "", "", "bash"),
        row("$2", "b-cc", "sid-b", "mobile", "claude"),
    ]
    .iter()
    {
        let can = &terminal_json(r, &[], None)["can"];
        let keys: Vec<&str> = can
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["end", "input"], "`can` 里多了 / 少了格：{can}");
    }
}

/// ★ 每行只带会变的格：`purpose` 原先恒为 `normal`（只有这一种），读起来像会变、实际不变 ⇒ 删。
/// `host` 留着（同样恒为 `tmux`，但它是宿主的名字，换宿主时就是这一格在变）。字段表里也不许再登记它。
#[test]
fn a_row_carries_no_constant_purpose() {
    for r in [
        row("$1", "a-cc", "", "", "bash"),
        row("$2", "b-cc", "sid-b", "mobile", "claude"),
    ]
    .iter()
    {
        let t = terminal_json(r, &[], None);
        assert!(t.get("purpose").is_none(), "行里还有 `purpose`：{t}");
        assert_eq!(t["host"], "tmux", "`host` 该留着");
    }
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|c| c.name == "terminals-list")
        .expect("没有 terminals-list");
    assert!(
        !spec.fields.iter().any(|f| f.name == "purpose"),
        "terminals-list 的字段表还登记着 `purpose`"
    );
}
