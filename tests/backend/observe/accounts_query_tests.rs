//! 观测面那几问的判据：配置目录形状关 · 这台活会话里断开的 MCP · 活会话此刻在干什么 · 谁持着一条会话。
//! 号表与会话归属哪个号的判据住 `tests/backend/accounts/manage/library_tests.rs`。

use super::*;
use crate::platform::proc::proc_starttime;
use std::fs;

/// 被 `is_safe_config_dir` 拒掉的 shell 元字符，**测试侧只有这一份**。
///
/// 🔴 `N-F1c`：抽出来是因为它现在要被**两种路径形状**各用一次（POSIX 与 Windows）。
/// 手抄两遍必漂，而漂掉的那一格恰恰是这里最怕的那一格 ——
/// 「为了让 Windows 路径过，顺手把安全那一半也放宽了」。
///
/// ⚠ 如实说清它**不是**源头：生产那一侧是 `is_safe_config_dir` 里的一个
/// `matches!` 臂，没有可 import 的具名常量 ⇒ 这仍是一份**手抄**，
/// 只是从两份收敛成一份。少了一个字符两边会**一起**变绿（同族假阴），
/// 接住它的是下面那条「地板」自检与 `NcM3` 那一刀。
fn shell_meta_chars() -> &'static [char] {
    &[
        '\'', '"', '`', '$', ';', '|', '&', '<', '>', '*', '?', '(', ')', '!',
    ]
}

#[test]
fn safe_config_dir_predicate() {
    assert!(is_safe_config_dir("/home/u/.claude-alt/z"));
    assert!(
        is_safe_config_dir("/home/用户/带 空格/z"),
        "空格与非 ASCII 允许"
    );
    assert!(!is_safe_config_dir("relative"));
    assert!(!is_safe_config_dir("/"));
    assert!(!is_safe_config_dir("/a/../b"));
    assert!(!is_safe_config_dir("/a/b/.."));
    for c in shell_meta_chars() {
        let bad = format!("/a{c}b");
        assert!(!is_safe_config_dir(&bad), "{bad} 应被拒");
    }
    assert!(!is_safe_config_dir("/a\nb"));
    // 🔴 `\` **不在**元字符表里 —— 它是 Windows 的路径分隔符，
    // 拒掉它就等于拒掉每一个 Windows 账号目录。本行是它的**正面记录**。
    // 放行它安全的理由不是「没人拼命令」，是**下游那一层自己会拒**
    // （`acct-core` 的 `config_dir_posix_ok` 明确把 `\` 列进拒绝集）——**分层校验**。
    assert!(
        is_safe_config_dir("/a\\b"),
        "`\\` 已经从元字符表里拿掉了（它是 Windows 的分隔符）"
    );
}

/// ★★ 正题：**路径检查分「性质 / 形式」两半，两侧都要成立。**
///
/// # 先证会红
///
/// 第一条写成 `if !p.starts_with('/')` ⇒ 下面「Windows 形状收得进」那一组**全红**；
/// 而它在 `list_accounts` 那一层的表现是**清单恒空**（每个账号都被 `continue` 掉）——
/// 那一格由 `a_windows_shaped_account_survives_the_listing` 单独钉。
///
/// # 分母怎么数的
///
/// 「危险形状照旧拒」那一侧的分母 = 下面四组之和，**现算**、失败时逐个点名：
/// 相对路径 · 裸根 · 上跳（POSIX 与 Windows 各写各的）· shell 元字符（`shell_meta_chars`
/// 那一份，套在 **Windows 形状**上再拒一次 —— 只在 POSIX 形状上验，
/// 「放宽了绝对路径顺手把元字符也放过」这一种会活着走出去）。
///
/// ⚠ 视觉欺骗字符那一族**不在本条的分母里**：它有自己的一条
/// （`deceptive_unicode_rejected`），本条只补 Windows 这一维，不搬家。
/// ⚠ 本条喂的是**字符串给纯函数**，不是真在 Windows 上列一次账号
///（`nc1` 那格诚实边界，解锁条件是一次真机实测）。
#[test]
fn windows_shaped_config_dirs_are_accepted_and_dangerous_ones_still_are_not() {
    // ① Windows 形状**收得进** —— 四形：盘符+反斜杠 / 盘符+正斜杠 / 小写盘符 / UNC。
    let accepted = [
        "C:\\Users\\alice\\.claude-alt\\z",
        "C:/Users/alice/.claude-alt/z",
        "d:\\x",
        "\\\\server\\share\\accts\\z",
    ];
    for good in accepted {
        assert!(
            is_safe_config_dir(good),
            "Windows 形状被判成不安全：{good:?}\n\
                 ⇒ 那就是「照抄 `starts_with('/')`」那一刀：Windows 上账号列表会**恒空**。"
        );
    }

    // ② 危险形状**照旧拒**。四组分开列，坏在哪一组一眼看得出。
    let relative = ["relative", "C:Users\\x", "z\\x"];
    let bare_root = ["/"];
    let updir = [
        "/a/../b",
        "/a/b/..",
        "C:\\Users\\..\\..\\x",
        "C:\\Users\\..",
    ];
    // ★ 元字符那一组套在 **Windows 形状**上 —— `NcM3` 切的正是这一格。
    let meta: Vec<String> = shell_meta_chars()
        .iter()
        .map(|c| format!("C:\\Users\\a{c}b"))
        .collect();

    // 反空真：任何一组塌了，下面的循环就零命中地绿。
    assert!(
        meta.len() >= 14,
        "元字符夹具只剩 {} 个 —— 人群塌了，本条在空转",
        meta.len()
    );
    let refused: Vec<String> = relative
        .iter()
        .chain(bare_root.iter())
        .chain(updir.iter())
        .map(|s| (*s).to_string())
        .chain(meta.iter().cloned())
        .collect();
    assert!(
        refused.len() >= 20,
        "「该拒」的人群只剩 {} 格 —— 分母塌了",
        refused.len()
    );
    let leaked: Vec<&String> = refused.iter().filter(|p| is_safe_config_dir(p)).collect();
    assert!(
        leaked.is_empty(),
        "这些危险形状被放进来了（分母 {} 格，逐个点名）：{leaked:?}\n\
             ⇒ `NcM3` 那一刀的形状就是它：**只放宽绝对路径那一半、顺手把安全那一半也放过**。",
        refused.len()
    );
}

// ---- 11. 建议：Unicode 欺骗字符两端对齐拒绝 ----
#[test]
fn deceptive_unicode_rejected() {
    assert!(!is_safe_config_dir("/home/u/\u{202E}gpj.z")); // RLO 反向覆盖
    assert!(!is_safe_config_dir("/home/u/z\u{200B}b")); // 零宽空格
    assert!(!is_safe_config_dir("/home/u/z\u{00A0}b")); // NBSP
    assert!(!is_safe_config_dir("/home/u/z\u{0085}b")); // NEL
    assert!(!is_safe_config_dir("/home/u/z\u{FEFF}b")); // ZWNBSP/BOM
    assert!(!is_safe_config_dir("/home/u/z\u{2069}b")); // 双向隔离
                                                        // 正常中文与普通空格仍放行
    assert!(is_safe_config_dir("/home/用户/带 空格/z"));
}

/// ★ 欺骗字符**按来源分组**各取一个代表，任何一组从 `acct-core` 的内核里掉出去 ⇒ 红（码位表逐字搬自 monitor 那条）。
///
/// 要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；②形（拒绝集那张表）。
#[test]
fn every_group_of_deceptive_characters_is_rejected_in_a_config_dir() {
    let groups: &[(char, &str)] = &[
        ('\u{0085}', "NEL（C1 换行；is_control 已覆盖）"),
        ('\u{00A0}', "NBSP"),
        ('\u{1680}', "Ogham space mark"),
        ('\u{2003}', "各类空格（U+2000..200A）"),
        ('\u{200B}', "零宽空格/连接符"),
        ('\u{2028}', "行分隔"),
        ('\u{202E}', "双向覆盖（RLO）"),
        ('\u{202F}', "narrow NBSP"),
        ('\u{205F}', "medium mathematical space"),
        ('\u{2060}', "word joiner / 不可见运算符"),
        ('\u{2066}', "双向隔离"),
        ('\u{3000}', "ideographic space"),
        ('\u{FEFF}', "ZWNBSP / BOM"),
    ];
    assert_eq!(groups.len(), 13, "分组表被削了");
    for (c, what) in groups {
        let path = format!("/home/u/.claude-alt/a{c}b");
        assert!(
            !is_safe_config_dir(&path),
            "U+{:04X}（{what}）没被挡下 —— 它能在界面上把账号路径伪造成另一个样子",
            *c as u32
        );
    }
    assert!(
        is_safe_config_dir("/home/u/.claude-alt/ab"),
        "正控失败：干净路径被误判成不安全 —— 上面那些断言全都不算数了"
    );
}

/// 这台活会话里说断开的 MCP：每个名字取说它最晚的那一条（带那条会话的标题与原话）；死了的会话不算（那份原话已经过时），后台任务不算。
/// 夹具：两个号的家各一个活会话（本测试进程的 pid）· 一个死会话（说得最晚，不许取）。
/// 要求：MCP 状态那一块「断开的原因取这台活会话里最近一条；会话都结束了就不画红标」。
#[cfg(unix)]
#[test]
fn live_mcp_failed_takes_the_latest_word_from_live_sessions_only() {
    let me = std::process::id();
    let delta = |at: &str, name: &str, err: &str| serde_json::json!({"type": "attachment", "timestamp": at, "attachment": {"type": "deferred_tools_delta", "failedMcpServers": [{"name": name, "error": err}]}});
    let title = |t: &str| serde_json::json!({"type": "ai-title", "aiTitle": t, "sessionId": "s"});
    let home_with = |tag: &str, pid: u32, sid: &str, recs: &[serde_json::Value]| {
        let home = test_home::scratch(tag);
        fs::create_dir_all(home.join("sessions")).unwrap();
        fs::write(
            home.join("sessions").join(format!("{pid}.json")),
            serde_json::json!({"sessionId": sid, "cwd": "/w", "procStart": proc_starttime(pid).map(|t| t.to_string())}).to_string(),
        )
        .unwrap();
        let dir = home.join("projects").join("-w");
        fs::create_dir_all(&dir).unwrap();
        let body: String = recs.iter().map(|r| format!("{r}\n")).collect();
        fs::write(dir.join(format!("{sid}.jsonl")), body).unwrap();
        home
    };
    let a = home_with(
        "mcpf-a",
        me,
        "sid-a",
        &[
            title("t-a"),
            delta("2026-10-09T10:00:00.000Z", "m-1", "old"),
        ],
    );
    let b = home_with(
        "mcpf-b",
        me,
        "sid-b",
        &[
            title("t-b"),
            delta("2026-10-09T10:31:00.000Z", "m-1", "new"),
        ],
    );
    let dead = home_with(
        "mcpf-dead",
        4_194_304,
        "sid-d",
        &[delta("2026-10-09T11:00:00.000Z", "m-1", "stale")],
    );
    let got = live_mcp_failed(&[a.clone(), b.clone(), dead.clone()]);
    for h in [&a, &b, &dead] {
        let _ = fs::remove_dir_all(h);
    }
    assert_eq!(
        got,
        vec![LiveMcpFailed {
            name: "m-1".into(),
            at_ms: crate::common::time::parse_iso8601_ms("2026-10-09T10:31:00.000Z"),
            detail: Some("new".into()),
            title: "t-b".into(),
        }]
    );
}

/// 扩展页每打开一次只读各条活会话记录新长出来的那一截（MCP 那一格接着上一次往下改）；记录变短了（换了一份）⇒ 从头读；
/// 不在这一轮名单里的记录忘掉。夹具：读过一次之后把已读那一截改写成同长的另一个名字 —— 只读尾巴就看不见它。
#[test]
fn the_mcp_word_of_a_live_record_is_read_from_where_the_last_look_stopped() {
    let delta = |name: &str| {
        format!(
            "{}\n",
            serde_json::json!({"type": "attachment", "timestamp": "2026-10-09T10:00:00.000Z", "attachment": {"type": "deferred_tools_delta", "failedMcpServers": [{"name": name, "error": "e"}]}})
        )
    };
    let names = |v: &[crate::observe::recognize::facts::McpTrouble]| -> Vec<String> {
        v.iter().map(|m| m.name.clone()).collect()
    };
    let dir = test_home::scratch("mcp-tail");
    let rec = dir.join("s.jsonl");
    let mut book = McpBook::default();
    fs::write(&rec, delta("m-a")).unwrap();
    assert_eq!(names(&book.read(&rec).unwrap()), ["m-a"]);
    // 已读那一截换成同长的 `m-z`，再长出一行「m-n 要登录」：只读尾巴 ⇒ 断开的还是 m-a。
    let first = delta("m-z");
    let second = format!(
        "{}\n",
        serde_json::json!({"type": "attachment", "timestamp": "2026-10-09T10:01:00.000Z", "attachment": {"type": "deferred_tools_delta", "needsAuthMcpServers": ["m-n"]}})
    );
    fs::write(&rec, format!("{first}{second}")).unwrap();
    let got = book.read(&rec).unwrap();
    assert_eq!(names(&got), ["m-a", "m-n"]);
    // 半行（还在写）不算读过：补完那一行再读才算进去。
    let half = delta("m-c");
    let (h1, h2) = half.split_at(half.len() / 2);
    fs::write(&rec, format!("{first}{second}{h1}")).unwrap();
    assert_eq!(names(&book.read(&rec).unwrap()), ["m-a", "m-n"]);
    fs::write(&rec, format!("{first}{second}{h1}{h2}")).unwrap();
    assert_eq!(names(&book.read(&rec).unwrap()), ["m-c", "m-n"]);
    // 变短了 ⇒ 从头读。
    fs::write(&rec, delta("m-z")).unwrap();
    assert_eq!(names(&book.read(&rec).unwrap()), ["m-z"]);
    // 这一轮没看它 ⇒ 忘掉：之后同长换了内容也从头读（记着的话长度没变、一个字都不读，还说 m-z）。
    book.keep_only(&[]);
    fs::write(&rec, delta("m-y")).unwrap();
    assert_eq!(names(&book.read(&rec).unwrap()), ["m-y"]);
    let _ = fs::remove_dir_all(&dir);
}

/// ★ **活会话此刻在干什么**（轮换规则在用名单的状态，与主窗口标签页同一判）：pidfile 的活动态；在等你 ⇒ 配上记录里没结果的那一步判种类
/// （同 `history-facts` 的 `needs`）；记录找不到 ⇒ 判不出（`unknown`），不猜；进程死了 ⇒ 不在表里。pidfile 照真 claude 的形状写。
#[cfg(target_os = "linux")]
#[test]
fn live_doing_reads_activity_and_what_it_waits_for() {
    use crate::agents::SessionActivity as A;
    use crate::observe::recognize::facts::NeedsKind as K;
    let home = test_home::scratch("doing");
    let dir = home.join("projects").join("-n");
    fs::create_dir_all(&dir).unwrap();
    let ask = "dddddddd-1111-2222-3333-444444444444";
    fs::write(
        dir.join(format!("{ask}.jsonl")),
        "{\"type\":\"assistant\",\"timestamp\":\"t1\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"b1\",\"name\":\"Bash\",\"input\":{\"command\":\"ls\"}}]}}\n",
    )
    .unwrap();
    let pids = crate::agents::pidfile_dir(&home);
    fs::create_dir_all(&pids).unwrap();
    let mut kids = Vec::new();
    let mut put = |sid: &str, status: &str| {
        let c = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let ticks = crate::platform::proc::proc_starttime(c.id()).expect("起始时刻");
        fs::write(
            pids.join(format!("{}.json", c.id())),
            format!(r#"{{"pid":{},"sessionId":"{sid}","kind":"interactive","procStart":"{ticks}","status":"{status}","waitingFor":"permission prompt","statusUpdatedAt":1700000000000}}"#, c.id()),
        )
        .unwrap();
        kids.push(c);
    };
    put("s-busy", "busy");
    put("s-idle", "idle");
    put(ask, "waiting");
    put("s-norecord", "waiting");
    put("s-dead", "busy");
    let dead = kids.pop().unwrap();
    let mut dead = dead;
    let _ = dead.kill();
    let _ = dead.wait();
    let got = live_doing(&home);
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = fs::remove_dir_all(&home);
    let d = |activity, needs| Doing {
        activity: Some(activity),
        needs,
    };
    let want: std::collections::BTreeMap<String, Doing> = [
        ("s-busy".to_string(), d(A::Working, None)),
        ("s-idle".to_string(), d(A::Idle, None)),
        (ask.to_string(), d(A::NeedsYou, Some(K::Approve))),
        // 记录找不到：说不出是哪一步，但那台说的是批准框 ⇒ 仍是批准（只是不挂哪一步）。
        ("s-norecord".to_string(), d(A::NeedsYou, Some(K::Approve))),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

/// 「这条会话此刻谁持着它」只有一处（[`session_holders_by`]），照适配层声明的判活方式走：
/// 开着写那一家 ⇒ 此刻开着记录写的进程（关了 ⇒ 判过了、没人）；pidfile 那一家 ⇒ 这台 pidfile（一份都没有 ⇒ 判过了、没人）；
/// 判不了的那一家 ⇒ `None`，哪怕此刻正有进程开着它写（不冒充判过）。
#[cfg(target_os = "linux")]
#[test]
fn who_holds_a_session_is_judged_the_way_its_adapter_declares() {
    use crate::agents::LiveBy;
    let home = test_home::scratch("holders");
    let rec = home.join("hold-sid.jsonl");
    fs::write(&rec, "{}\n").unwrap();
    let open = || fs::OpenOptions::new().append(true).open(&rec).unwrap();
    let me = std::process::id();
    let f = open();
    assert_eq!(
        session_holders_by(LiveBy::HeldOpen, &home, &rec),
        Some(vec![me]),
        "开着写的那一家：本进程开着它写，没认出来"
    );
    assert_eq!(
        session_holders_by(LiveBy::Unknown, &home, &rec),
        None,
        "判不了的那一家冒充判过了"
    );
    drop(f);
    assert_eq!(
        session_holders_by(LiveBy::HeldOpen, &home, &rec),
        Some(vec![]),
        "关了还算持着"
    );
    assert_eq!(
        session_holders_by(LiveBy::Pidfiles, &home, &rec),
        Some(vec![]),
        "pidfile 那一家：一份 pidfile 都没有 ⇒ 判过了、没人"
    );
    let _ = fs::remove_dir_all(&home);
}

/// 判活的声明在注册表那一行（`Adapter::live`）：Claude 认 pidfile，Codex 认开着写；认不出的 kind ⇒ 判不了。
#[test]
fn each_adapter_declares_how_its_sessions_are_judged_alive() {
    use crate::agents::{live_by, LiveBy};
    assert_eq!(live_by("claude"), LiveBy::Pidfiles);
    assert_eq!(live_by("codex"), LiveBy::HeldOpen);
    assert_eq!(live_by("no-such-agent"), LiveBy::Unknown);
}
