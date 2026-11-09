//! 号表（账号清单成品）与会话归属哪个号的判据（`accounts::manage::library`）。

use super::test_support::{account_default_golden, golden_library, write_manifest};
use super::*;
use crate::agents::claudecode::accounts as cc_accounts;
use crate::platform::fs::Ro;
use crate::platform::proc::proc_starttime;
use std::fs;

/// 清单的扫描本体摊成行：首行 meta、其后每号一行（不并 apikey 表）。下面几条判的是扫描（读 manifest · 判安全 · 判鉴权），不是哪一个出口。
fn list_accounts(accts: &Path) -> Vec<String> {
    let (meta, rows) = scan_accounts(accts, &|_| false);
    let mut out = vec![serde_json::to_string(&meta).unwrap()];
    out.extend(rows.iter().map(|r| serde_json::to_string(r).unwrap()));
    out
}

/// 清单成品回成 `Value` 好按键看（帧里那一份写法）；盖住 `super::*` 里的同名口。
/// key 表那几行照帧面宿主那样交进去（照账号 id 找那一行的掩码与端点）。
fn list_product_with(
    accts_dir: &Path,
    home: Option<&Path>,
    rows: &[String],
    keys: &[crate::accounts::upstream_select::file_face::KeyFact],
    agent: &str,
    table_agent: &str,
    relay_running: bool,
) -> serde_json::Value {
    serde_json::to_value(super::list_product_with(
        accts_dir,
        home,
        rows,
        &|id| {
            keys.iter()
                .find(|k| k.id == id)
                .map(|k| (k.masked.clone(), k.base_url.clone()))
        },
        agent,
        table_agent,
        relay_running,
    ))
    .unwrap()
}

fn meta(lines: &[String]) -> serde_json::Value {
    serde_json::from_str(&lines[0]).unwrap()
}

// ---- 1. 正常 manifest ----
#[test]
fn list_accounts_happy_path() {
    let root = test_home::scratch("happy");
    let accts = root.join("accts");
    let z = accts.join("z");
    fs::create_dir_all(&z).unwrap();
    fs::write(z.join(".credentials.json"), "{\"tok\":\"SECRET-TOKEN\"}").unwrap();
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"updatedAt":"2026-07-23T00:00:00Z","sharedStore":"/s",
                    "acctsDir":"{a}","accounts":[
                    {{"name":"z","email":"z@example.test","configDir":"{z}","isDefault":true,"mode":"isolated"}},
                    {{"name":"b","email":"","configDir":"{a}/b","isDefault":false,"mode":"isolated"}}]}}"#,
            a = accts.display(),
            z = z.display()
        ),
    );
    let lines = list_accounts(&accts);
    let m = meta(&lines);
    assert_eq!(m["enabled"], true);
    assert_eq!(m["count"], 2);
    assert_eq!(m["updatedAt"], "2026-07-23T00:00:00Z");
    assert_eq!(lines.len(), 3);
    let a0: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(a0["name"], "z");
    assert_eq!(a0["isDefault"], true);
    assert_eq!(a0["exists"], true);
    assert_eq!(a0["loggedIn"], true, "有 .credentials.json 应判已登录");
    let a1: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
    assert_eq!(a1["name"], "b");
    assert_eq!(a1["exists"], false, "目录不存在");
    assert_eq!(a1["loggedIn"], false);
    // 凭据零泄漏
    for l in &lines {
        assert!(!l.contains("SECRET-TOKEN"), "输出里出现了凭据内容：{l}");
    }
    let _ = fs::remove_dir_all(&root);
}

// ---- 2. manifest 缺失 / 畸形 / 版本不支持 → enabled:false 且不失败 ----
#[test]
fn list_accounts_degrades_gracefully() {
    let root = test_home::scratch("degrade");
    // 缺文件
    let m = meta(&list_accounts(&root.join("nope")));
    assert_eq!(m["enabled"], false);
    assert!(copy_core::copy_matches(
        "beAccountsQuery.loadManifest.unreadable",
        m["error"].as_str().unwrap()
    ));
    // 坏 JSON
    let a = root.join("bad");
    write_manifest(&a, "{not json");
    let m = meta(&list_accounts(&a));
    assert_eq!(m["enabled"], false);
    assert!(copy_core::copy_matches(
        "beAccountsQuery.loadManifest.badJson",
        m["error"].as_str().unwrap()
    ));
    // 版本不支持
    let a2 = root.join("v2");
    write_manifest(&a2, r#"{"version":2,"accounts":[]}"#);
    let m = meta(&list_accounts(&a2));
    assert_eq!(m["enabled"], false);
    assert!(copy_core::copy_matches_with(
        "beAccountsQuery.loadManifest.badVersion",
        &[("v", "2")],
        m["error"].as_str().unwrap()
    ));
    // 缺 version
    let a3 = root.join("nover");
    write_manifest(&a3, r#"{"accounts":[]}"#);
    assert_eq!(meta(&list_accounts(&a3))["enabled"], false);
    let _ = fs::remove_dir_all(&root);
}

// ---- 3. 非法 configDir 被丢弃，其余正常 ----
/// 要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；②形。
#[test]
fn unsafe_config_dirs_are_dropped() {
    let root = test_home::scratch("unsafe");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[
                {"name":"ok","configDir":"/home/u/.claude-alt/ok"},
                {"name":"quote","configDir":"/home/u/ac'ts/x"},
                {"name":"dollar","configDir":"/home/u/$(id)/x"},
                {"name":"dotdot","configDir":"/home/u/../etc"},
                {"name":"rel","configDir":"relative/path"},
                {"name":"root","configDir":"/"}]}"#,
    );
    let lines = list_accounts(&accts);
    assert_eq!(meta(&lines)["count"], 1, "只应留下合法的那一个");
    let a: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(a["name"], "ok");
    let _ = fs::remove_dir_all(&root);
}

/// ★★ `NF1cD2` 的**后果面**：那条谓词在清单那一层的表现是「列表恒空 / 不恒空」。
///
/// 只断纯函数不够 —— `NF1cD2` 逐字说的是「**Windows 上列得出来**」，
/// 而列不列得出来是 `list_accounts` 那一层的事（不安全的 `configDir` 会被 `continue` 掉）。
/// ⇒ 本条喂一份 **Windows 形状的 manifest**，断它出得来几个、名字是什么。
///
/// 拆之前：两条都被判不安全 ⇒ `count == 0`，用户看到的是一张**空表**，
/// 而 `enabled` 仍是 `true` —— 那正是「够不着被渲染成你没有账号」那族病的源头。
///
/// ⚠ 同 `nc1`：这是**在 Linux 上喂 Windows 形状的字符串**，不是真机读数。
/// `exists` 在这里必然是 `false`（那个盘符路径在 Linux 上不存在），
/// 而账号**仍然要在列表里** —— 「探不到目录」与「不安全被丢掉」是两回事。
#[test]
fn a_windows_shaped_account_survives_the_listing() {
    let root = test_home::scratch("winshape");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[
                {"name":"alice","configDir":"C:\\Users\\alice\\.claude-alt\\z"},
                {"name":"unc","configDir":"\\\\srv\\share\\accts\\u"},
                {"name":"dodgy","configDir":"C:\\Users\\alice\\..\\..\\etc"},
                {"name":"meta","configDir":"C:\\Users\\a$(id)\\x"}]}"#,
    );
    let lines = list_accounts(&accts);
    let m = meta(&lines);
    assert_eq!(m["enabled"], true);
    assert_eq!(
        m["count"], 2,
        "Windows 形状的账号没能留在列表里（或危险的那两条也混进来了）——\n\
             拆之前这里是 0，那就是「Windows 上账号列表恒空」的**直接读数**。"
    );
    let names: Vec<String> = lines[1..]
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["name"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        vec!["alice", "unc"],
        "留下来的不是该留的那两个 —— 上跳与元字符那两条必须照旧被丢掉"
    );
    let a0: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(
        a0["configDir"], "C:\\Users\\alice\\.claude-alt\\z",
        "configDir 被改写了 —— 本条只该判安不安全，不该动内容"
    );
    assert_eq!(
        a0["exists"], false,
        "这条断的是「探不到目录 ≠ 被丢掉」：账号在表里，`exists` 诚实地说 false"
    );
    let _ = fs::remove_dir_all(&root);
}

// ---- 6. --session-accounts（procStart 身份对拍是核心）----

#[test]
fn session_accounts_marks_dead_and_bare() {
    let root = test_home::scratch("sess");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    // 一个几乎不可能存在的 pid → alive:false
    fs::write(
        sessions.join("4194300.json"),
        r#"{"sessionId":"sid-dead","cwd":"/w","procStart":"999"}"#,
    )
    .unwrap();
    let lines = session_accounts(&claude, &root.join("no-accts"));
    let mut by_sid = sid_map(&lines);
    assert_eq!(by_sid["sid-dead"]["alive"], false);
    assert_eq!(by_sid["sid-dead"]["bare"], false, "死进程不算裸起");
    assert_eq!(by_sid["sid-dead"]["account"], serde_json::Value::Null);

    // 一个活的子进程 pid + **正确的 procStart** → 身份对拍通过 → alive:true；它的环境里没有 `CLAUDE_CONFIG_DIR` ⇒ 裸起。
    // 不拿本测试进程自己当会话：`/proc/self/environ` 是进程起来那一刻的环境，测试档换家只清得了 `std::env`，
    //   外面会话的 `CLAUDE_CONFIG_DIR` 还在那里面（同 [`bare_child`] 头注）。
    #[cfg(target_os = "linux")]
    {
        let mut live = bare_child();
        let pid = live.id();
        let ticks = proc_starttime(pid).expect("能读子进程的 starttime");
        fs::write(
            sessions.join(format!("{pid}.json")),
            format!(r#"{{"sessionId":"sid-live","cwd":"/w2","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        let lines = session_accounts(&claude, &root.join("no-accts"));
        by_sid = sid_map(&lines);
        assert_eq!(by_sid["sid-live"]["alive"], true, "procStart 相符应判活");
        assert_eq!(by_sid["sid-live"]["bare"], true, "{:?}", by_sid["sid-live"]);
        assert_eq!(by_sid["sid-live"]["configDir"], serde_json::Value::Null);
        let _ = live.kill();
        let _ = live.wait();
    }
    let _ = fs::remove_dir_all(&root);
}

/// R1：PID 复用防御——pidfile 的 procStart 与当前进程不符 → 判死，绝不误归属账号。
#[cfg(target_os = "linux")]
#[test]
fn session_accounts_rejects_pid_reuse() {
    let root = test_home::scratch("reuse");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    let real = proc_starttime(me).unwrap();
    // 同一个活 PID，但 pidfile 记的是**别的** procStart（= 该 PID 曾属于一个已退出的
    // claude，现在被本测试进程复用）→ 必须判死、不归属
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(
            r#"{{"sessionId":"sid-stale","cwd":"/w","procStart":"{}"}}"#,
            real + 12345
        ),
    )
    .unwrap();
    let by = sid_map(&session_accounts(&claude, &root.join("no-accts")));
    assert_eq!(
        by["sid-stale"]["alive"], false,
        "procStart 不符 = PID 被复用 → 判死"
    );
    assert_eq!(by["sid-stale"]["bare"], false);
    assert_eq!(by["sid-stale"]["account"], serde_json::Value::Null);

    // 缺 procStart 的老 pidfile 也保守判死（宁缺毋错）
    fs::write(
        sessions.join(format!("{me}.json")),
        r#"{"sessionId":"sid-noproc","cwd":"/w"}"#,
    )
    .unwrap();
    let by = sid_map(&session_accounts(&claude, &root.join("no-accts")));
    assert_eq!(by["sid-noproc"]["alive"], false, "缺 procStart 保守判死");
    let _ = fs::remove_dir_all(&root);
}

fn sid_map(rows: &[SessionAccount]) -> std::collections::HashMap<String, serde_json::Value> {
    let mut m = std::collections::HashMap::new();
    for r in rows {
        let v = serde_json::json!({
            "pid": r.pid, "sessionId": r.sid, "cwd": r.cwd, "configDir": r.config_dir,
            "account": r.account, "alive": r.alive, "bare": r.bare, "viaRelay": r.via_relay,
        });
        m.insert(r.sid.clone().unwrap(), v);
    }
    m
}

#[test]
fn session_accounts_without_sessions_dir_is_empty() {
    let root = test_home::scratch("nosess");
    assert!(session_accounts(&root, &root).is_empty());
    let _ = fs::remove_dir_all(&root);
}

// ---- 7. 尾斜杠归一 ----
#[test]
fn trailing_slash_normalized() {
    assert_eq!(norm_dir("/a/b/"), "/a/b");
    assert_eq!(norm_dir("/a/b"), "/a/b");
    assert_eq!(norm_dir("/"), "/");
}

// ---- 9. 重要-B：特殊文件不绕过大小上限 ----
#[cfg(unix)]
#[test]
fn special_files_are_rejected_not_read() {
    use std::os::unix::fs::symlink;
    let root = test_home::scratch("special");
    // symlink → /dev/zero：metadata().len() 报 0 会骗过大小检查,read 无上限会 OOM。
    // read_capped 必须靠 is_file() 挡下（跟随 symlink 后目标是字符设备）。
    let link = root.join("evil.json");
    symlink("/dev/zero", &link).unwrap();
    let r = Ro::read_capped(&link, cc_accounts::CONFIG_CAP);
    assert!(
        r.is_err(),
        "指向 /dev/zero 的 symlink 必须被拒，而不是读爆内存"
    );
    let cap = |bytes| cap_core::Cap {
        bytes,
        over: cap_core::Over::Reject,
    };
    // 目录也不是常规文件
    assert!(Ro::read_capped(&root, cap(1024)).is_err());
    // 正常小文件放行
    let ok = root.join("ok.json");
    fs::write(&ok, "{}").unwrap();
    assert_eq!(Ro::read_capped(&ok, cap(1024)).unwrap(), b"{}");
    // 超上限的常规文件被拒
    fs::write(&ok, vec![b'x'; 100]).unwrap();
    assert!(Ro::read_capped(&ok, cap(50)).is_err());
    let _ = fs::remove_dir_all(&root);
}

// ---- 10. 建议1：单个坏账号被跳过而非拖垮整份 manifest ----
#[test]
fn one_bad_account_does_not_kill_the_list() {
    let root = test_home::scratch("badacct");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        // Z01 起「缺 configDir」不再是坏数据（那是账号 0），所以坏样本换成
        // 缺 name / configDir 不安全这两种真·坏法。
        r#"{"version":1,"accounts":[
                {"name":"good","configDir":"/h/.claude-alt/good"},
                {"configDir":"/h/.claude-alt/noname"},
                {"name":"unsafe","configDir":"relative/path"},
                {"name":"good2","configDir":"/h/.claude-alt/good2"}]}"#,
    );
    let lines = list_accounts(&accts);
    assert_eq!(meta(&lines)["enabled"], true);
    assert_eq!(
        meta(&lines)["count"],
        2,
        "缺 name / 路径不安全的被跳过,好的两个留下"
    );
    let names: Vec<String> = lines[1..]
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["name"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(names, vec!["good", "good2"]);
    let _ = fs::remove_dir_all(&root);
}

// ---- 12. Z01：账号 0（configDir 键缺席）----

/// 缺 `configDir` = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿；
/// 帧里 `configDir` 出 **null**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
#[test]
fn account_zero_is_kept_and_probes_shared_store() {
    let root = test_home::scratch("acct0");
    let shared = root.join("claude");
    fs::create_dir_all(&shared).unwrap();
    let accts = root.join("accts");
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"sharedStore":{shared:?},"accounts":[
                    {{"name":"z","configDir":{z:?}}},
                    {{"name":"0","isDefault":false,"mode":"bare"}}]}}"#,
            shared = shared.to_string_lossy(),
            z = root.join("accts/z").to_string_lossy()
        ),
    );
    let lines = list_accounts(&accts);
    assert_eq!(meta(&lines)["count"], 2, "账号 0 不得被静默丢掉");
    let zero: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
    assert_eq!(zero["name"], "0");
    assert_eq!(
        zero["configDir"],
        serde_json::Value::Null,
        "必须是 null，**绝不能是空串**"
    );
    assert_eq!(zero["mode"], "bare");
    assert_eq!(zero["exists"], true, "「裸起」这个状态永远可达");
    assert_eq!(zero["loggedIn"], false, "共享库里还没凭据");

    fs::write(shared.join(".credentials.json"), "{}").unwrap();
    let lines = list_accounts(&accts);
    let zero: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
    assert_eq!(zero["loggedIn"], true, "共享库凭据 = 账号 0 已登录");
    let _ = fs::remove_dir_all(&root);
}

/// ★ 空串 **不是** 缺席。这是整个 Z01 的支点：`CLAUDE_CONFIG_DIR=""` 会被
/// Claude Code 当成一个空路径，与「未设」完全不同。它必须被当坏数据丢掉，
/// **不能**退化成账号 0。
#[test]
fn empty_config_dir_is_not_account_zero() {
    let root = test_home::scratch("acct0empty");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"sharedStore":"/h/.claude","accounts":[
                {"name":"empty","configDir":""}]}"#,
    );
    let lines = list_accounts(&accts);
    assert_eq!(
        meta(&lines)["count"],
        0,
        "空串 configDir 必须被丢掉，不得当成账号 0"
    );
    let _ = fs::remove_dir_all(&root);
}

/// manifest 没写 sharedStore 时，账号 0 的登录态是「不知道」⇒ false，
/// **不得假装已登录**，也不得因此把账号 0 丢掉。
#[test]
fn account_zero_without_shared_store_is_not_logged_in() {
    let root = test_home::scratch("acct0nostore");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
    );
    let lines = list_accounts(&accts);
    assert_eq!(meta(&lines)["count"], 1);
    let zero: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    assert_eq!(zero["loggedIn"], false);
    assert_eq!(zero["configDir"], serde_json::Value::Null);
    let _ = fs::remove_dir_all(&root);
}

/// 一个**环境里确实没有** `CLAUDE_CONFIG_DIR` 的活进程，当「裸起会话」用。
/// 不拿测试进程自己当会话：在 Claude Code 会话里跑测试时它恒有值，见变量有值就 `return` 的用例
/// 在门禁上**从不执行**。`/proc/self/environ` 是进程起来那一刻的环境，
/// 事后 `remove_var` 改不了它 ⇒ 只能另起一个清掉该变量的子进程。
///
/// 回来之前**等 exec 完**（[`wait_exec_done`]）：`spawn` 回来时 exec 可能还没走完，那一刻读环境是 0 字节，
/// 判成「环境读不出来」而不是「裸起」（10-07 整套跑时红过一次，并发起几十份时每千趟红几趟）。
#[cfg(target_os = "linux")]
fn bare_child() -> std::process::Child {
    let child = std::process::Command::new("sleep")
        .arg("30")
        .env_remove("CLAUDE_CONFIG_DIR")
        .spawn()
        .expect("起一个 sleep 子进程当裸起会话");
    wait_exec_done(child.id(), "sleep");
    child
}

/// 等子进程的 exec 走完：`/proc/<pid>/cmdline` 以程序名开头的那一刻。
///
/// `Command::spawn` 在旧程序的 close-on-exec 管道关掉时就回来了，而内核到那时还没给新程序摆好
/// 参数与环境（`arg_start` / `env_start` 是 exec 最后一段才一起写上的）⇒ 那段窗口里 `environ` 读回 0 字节。
/// 看 `cmdline` 而不看 `comm`：`comm` 在 exec 前段就换了名字，换了名字时环境可能还没摆好；
/// `cmdline` 与环境是同一刻摆上的，看见它就一定读得到环境，而 `sleep` 不会再 exec ⇒ 窗口不会再开。
#[cfg(target_os = "linux")]
fn wait_exec_done(pid: u32, prog: &str) {
    let until = std::time::Instant::now() + guard_core::wall::CEILING;
    while !std::fs::read(format!("/proc/{pid}/cmdline"))
        .is_ok_and(|c| c.starts_with(prog.as_bytes()))
    {
        assert!(
            std::time::Instant::now() < until,
            "{prog} 起不来（10 秒没走完 exec）"
        );
        std::thread::yield_now();
    }
}

/// 裸起会话（活着但没设 CLAUDE_CONFIG_DIR）现在归属账号 0。
#[cfg(target_os = "linux")]
#[test]
fn bare_session_is_attributed_to_account_zero() {
    let mut child = bare_child();
    let root = test_home::scratch("acct0sess");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
    );
    let me = child.id();
    let ticks = proc_starttime(me).expect("能读子进程的 starttime");
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(r#"{{"sessionId":"sid-zero","cwd":"/w","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    let by = sid_map(&session_accounts(&claude, &accts));
    assert_eq!(by["sid-zero"]["alive"], true);
    assert_eq!(by["sid-zero"]["bare"], true);
    assert_eq!(by["sid-zero"]["account"], "0", "裸起不再是「归属不明」");
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&root);
}

/// 🔴🔴 **「环境这一刻取不到」不许被报成「账号 0 + 裸起」。**
///
/// # 它守的那句假话长什么样
///
/// `proc_env_var` 把四件事压成一个 `None` 的话，其中「**这一刻读不出来**」会一路走成
/// `configDir: null` ⇒ 在 `by_dir` 里**正好撞上账号 0 那个 `None` 键**
/// ⇒ 出参是**斩钉截铁**的 `account:"0"` + `bare:true`。
/// 而 `alive` 仍是 `true`（它读 `/proc/<pid>/stat`，与 `environ` **不是同一次读**）
/// ⇒ **无声无息**：一条真跑在账号 Z 下的会话，会被报成账号 0 的。
///
/// # 三个活体：两个是病，一个是对照
///
/// | 活体 | 它让那次读走哪一支 | 该报什么 |
/// |---|---|---|
/// | 甲 · **僵尸**（子进程已退、故意不回收）| `std::fs::read` 回 **`Err`**（mm 已释放）| `account:null` · `bare:false` |
/// | 乙 · **空环境活体**（`env_clear` 起的 `sleep`）| 回 **`Ok(vec![])`**（0 字节）| 同上 |
/// | 丙 · **对照**：环境读得到、非空、确实没设那个键 | `Unset` | `account:"0"` · `bare:true` |
///
/// 🔴 **丙这一格非有不可**：没有它，「三条全是 `null`」也会绿 ——
/// 而那正是本条最容易退化成的样子（把归属整个摘掉也是这个读数）。
/// 🔴 甲乙的 `alive` **都必须是 `true`**：那正是这句假话的杀伤力所在 ——
/// 判活与读环境不是同一次读，所以「活着」与「读不出来」可以同时成立。
///
/// # ⚠ 乙身上有一格**如实登记的重合**（别把本条读宽）
///
/// `env_clear` 起的进程，它的环境**读得到、而且真的是空的** ——
/// 也就是说「0 字节」这个信号自己也装着两件事：「这一刻读不出来」与「环境真的是空的」。
/// 生产上后者不会发生（真 claude 进程至少有 `PATH`/`HOME`），且两者都落到**保守**的
/// 那一侧（报「不知道」而不是报「账号 0」）⇒ **刻意不拆它**，登记在 `§7`。
/// **别把本条读成「backend 分得清这两件事」—— 它分不清。**
///
/// # 🔴 夹具前提：等 exec 窗口关死
///
/// **丙**（对照活体）的前提是「环境读得到、**非空**」，而 `Command::spawn`
/// 只保证 fork 完了、**不保证 exec 完了**；`/proc/<pid>/environ` 在 `execve` 进行
/// **当中**读回 **0 字节**（`platform/proc.rs` 那一支记着它）。
/// 窗口没关就读 ⇒ `c_ok` 为假 ⇒ 本条红在夹具上，不是红在产品上；量过：空载几乎不开，有负载时偶尔开。
/// ⇒ 下面那段屏障关的就是这个窗口（对产品零影响），判据本体只跑一次，一个断言都没放宽。
#[cfg(target_os = "linux")]
#[test]
fn an_unreadable_environ_is_never_reported_as_the_zero_account() {
    let root = test_home::scratch("envhole");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[{"name":"0","mode":"bare"}]}"#,
    );

    // 甲：僵尸 —— 起一个立刻退出的子进程，**故意不 `wait`**（不回收 ⇒ `/proc/<pid>` 还在）。
    let mut zombie = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("起不来 sh —— 活体夹具起不来就**不许当绿**");
    let zpid = zombie.id();
    // 乙：空环境活体（真的在跑，state = S）。
    let mut empty = std::process::Command::new("sleep")
        .arg("60")
        .env_clear()
        .spawn()
        .expect("起不来 sleep（空环境活体）");
    let epid = empty.id();
    // 丙：对照活体 —— 环境读得到、**非空**、就是没设那个键。
    let mut plain = std::process::Command::new("sleep")
        .arg("60")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .spawn()
        .expect("起不来 sleep（对照活体）");
    let ppid = plain.id();

    // 等甲真的成了僵尸（`/proc/<pid>/stat` 的 state 字段 = `Z`）。**不睡死等**：
    // 成不了僵尸就让下面的自检把它打红，而不是让本条零命中地绿。
    let state_of = |pid: u32| -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after = stat.rfind(')')? + 2;
        stat[after..].chars().next()
    };
    for _ in guard_core::wall::rounds() {
        if state_of(zpid) == Some('Z') {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // ── 🔴 **等 exec 窗口关死** ──────
    //
    // `Command::spawn` 只保证 fork 完了，**不保证 exec 完了**；而 `execve` 进行当中
    // `/proc/<pid>/environ` 读回 **0 字节**（`platform/proc.rs` 那一支逐字记着它）。
    // ⇒ 丙那一格的前提（「读得到、**非空**」）在负载下会偶尔不成立 ⇒ 判据红在夹具上。
    //
    // ⚠ **这不是重试、不是放宽、不是睡过去**（同本文件那条同族判据的纪律）：
    //   判「exec 完了」不能只看 environ 读不读得出来 —— exec **之前**也读得出来
    //   （fork 来的那份副本）。所以看 `/proc/<pid>/comm`：exec 前是别的名字，
    //   exec 后是 `sleep`，而 `sleep` **不会再 exec** ⇒ 一旦看见它，窗口**关死了、
    //   开不回来** ⇒ 它在下面那几次读与那一次 `session_accounts` 时仍然成立。
    // ⚠ 等不到也**不许静默通过**：这里只是等，成没成由下面那三格自检说了算。
    let comm_of = |pid: u32| {
        std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()
            .map(|s| s.trim().to_string())
    };
    let env_has_path = |pid: u32| {
        matches!(std::fs::read(format!("/proc/{pid}/environ")),
            Ok(b) if b.windows(5).any(|w| w == b"PATH="))
    };
    for _ in guard_core::wall::rounds() {
        // 乙也等：它要的 0 字节今天有两种来路（空环境 / exec 窗口），
        // 等一下让它落在**前者**上 —— 那格重合本身仍在（头注里登记着），
        // 但至少夹具是它自称的那个夹具。
        if comm_of(epid).as_deref() == Some("sleep")
            && comm_of(ppid).as_deref() == Some("sleep")
            && env_has_path(ppid)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // ── 🔴 反空真自检：三个活体**此刻**真的各在自己那一支上 ──────────────
    // 没有这一段，夹具悄悄退化（比如僵尸被回收了、或空环境没生效）时本条会
    // 零命中地绿 —— 那正是记的那一形。
    let zombie_state = state_of(zpid);
    let read_a = std::fs::read(format!("/proc/{zpid}/environ"));
    let read_b = std::fs::read(format!("/proc/{epid}/environ"));
    let read_c = std::fs::read(format!("/proc/{ppid}/environ"));
    let a_unreadable = match &read_a {
        Err(_) => true,
        Ok(b) => b.is_empty(),
    };
    let b_zero = matches!(&read_b, Ok(b) if b.is_empty());
    let c_ok = matches!(&read_c, Ok(b) if !b.is_empty());

    // 判据本体跑在**三条真活体**上（不是任何复刻）。
    let write_pidfile = |pid: u32, sid: &str| {
        let ticks = proc_starttime(pid)
            .unwrap_or_else(|| panic!("读不到 pid={pid} 的 starttime —— 活体没活着"));
        fs::write(
            sessions.join(format!("{pid}.json")),
            format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"{ticks}"}}"#),
        )
        .unwrap();
    };
    write_pidfile(zpid, "sid-zombie");
    write_pidfile(epid, "sid-emptyenv");
    write_pidfile(ppid, "sid-control");
    let by = sid_map(&session_accounts(&claude, &accts));

    // 先收拾活体，再断言（断言失败也不留孤儿进程）。
    let _ = empty.kill();
    let _ = empty.wait();
    let _ = plain.kill();
    let _ = plain.wait();
    let _ = zombie.wait();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        zombie_state,
        Some('Z'),
        "甲没成僵尸（state={zombie_state:?}）—— 夹具没装上，下面几格量的不是本条要的东西"
    );
    assert!(
        a_unreadable,
        "甲的 environ 这一刻**读得出内容**（{:?}）—— 它就不在「取不到」那一支上了",
        read_a.as_ref().map(|b| b.len())
    );
    assert!(
        b_zero,
        "乙的 environ 不是 0 字节（{:?}）—— `env_clear` 没生效，支四那一格是空转",
        read_b.as_ref().map(|b| b.len())
    );
    assert!(
        c_ok,
        "丙的 environ 读不到或是空的（{:?}）—— 对照就不成其为对照了",
        read_c.as_ref().map(|b| b.len())
    );

    for sid in ["sid-zombie", "sid-emptyenv"] {
        assert_eq!(
            by[sid]["alive"], true,
            "{sid} 没判活 —— 而「活着」与「环境读不出来」同时成立正是这句假话的杀伤力所在"
        );
        assert_eq!(
            by[sid]["account"],
            serde_json::Value::Null,
            "\n🔴🔴 {sid}：环境这一刻**取不到**，而出参斩钉截铁地说它属于账号 0。\n\
                 那不是「裸起」，那是「不知道」——`by_dir` 里账号 0 的键正好也是 `None`，\n\
                 于是「读不出来」与「确实没设」撞在同一格上。\n\
                 ⇒ 一条真跑在账号 Z 下的会话会被报成账号 0 的，而 `alive` 仍是 `true`、无声无息。"
        );
        assert_eq!(
            by[sid]["bare"], false,
            "\n🔴 {sid}：`bare:true` 的含义是「进程活着、**读到了**、就是没设那个变量」。\n\
                 这一刻根本没读到 ⇒ 它说不出这句话。"
        );
        assert_eq!(by[sid]["configDir"], serde_json::Value::Null);
    }
    // 对照：读得到、非空、确实没设 ⇒ 归属**照旧**。掏掉归属那一格这里会红。
    assert_eq!(by["sid-control"]["alive"], true);
    assert_eq!(
        by["sid-control"]["account"], "0",
        "\n🔴 对照红了：环境读得到、非空、确实没设 `CLAUDE_CONFIG_DIR` —— 这就是**真裸起**，\n\
             它必须仍然归到账号 0。上面两格的 `null` 若与这一格同值，本条就退化成\n\
             「把归属整个摘掉也绿」。"
    );
    assert_eq!(by["sid-control"]["bare"], true);
}

/// 反向：manifest 里 **没有** 账号 0 时，裸起会话仍旧行为（account: null）。
/// 钉住「归属来自 manifest」，而不是在 Rust 里硬编码了个 "0"。
#[cfg(target_os = "linux")]
#[test]
fn bare_session_without_account_zero_stays_unattributed() {
    let mut child = bare_child();
    let root = test_home::scratch("acct0none");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let accts = root.join("accts");
    write_manifest(
        &accts,
        r#"{"version":1,"accounts":[{"name":"z","configDir":"/h/.claude-alt/z"}]}"#,
    );
    let me = child.id();
    let ticks = proc_starttime(me).unwrap();
    fs::write(
        sessions.join(format!("{me}.json")),
        format!(r#"{{"sessionId":"sid-none","cwd":"/w","procStart":"{ticks}"}}"#),
    )
    .unwrap();
    let by = sid_map(&session_accounts(&claude, &accts));
    assert_eq!(by["sid-none"]["account"], serde_json::Value::Null);
    let _ = child.kill();
    let _ = child.wait();
    let _ = fs::remove_dir_all(&root);
}

/// 一条刚宣告的会话跑在哪个号下（[`account_of_process`]，宣告那张脸带的就是它）：进程环境里的配置目录对上账号库里那个号；
/// 没设配置目录、账号库里也没有账号 0 ⇒ 不说。
#[cfg(target_os = "linux")]
#[test]
fn an_announced_process_is_attributed_to_the_account_whose_dir_it_runs_under() {
    let root = test_home::scratch("acctannounce");
    let cfg = root.join("acct-b");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"accounts":[{{"name":"b","configDir":"{}"}}]}}"#,
            cfg.display()
        ),
    );
    let face = crate::agents::account_library_face().expect("有账号库的那一家");
    let mut with_dir = std::process::Command::new("sleep")
        .arg("30")
        .env("CLAUDE_CONFIG_DIR", &cfg)
        .spawn()
        .expect("起一个带配置目录的 sleep");
    wait_exec_done(with_dir.id(), "sleep");
    let mut bare = bare_child();
    assert_eq!(
        account_of_process(with_dir.id(), &face, &accts),
        Some("b".to_string())
    );
    assert_eq!(
        account_of_process(bare.id(), &face, &accts),
        None,
        "没设配置目录、账号库里也没有账号 0 ⇒ 不说"
    );
    for c in [&mut with_dir, &mut bare] {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = fs::remove_dir_all(&root);
}

// ---- 鉴权方式这一维 ----

use acct_core::{AUTH_KIND_API_KEY, AUTH_KIND_SUBSCRIPTION};

/// 鉴权方式金样夹具的**一格**：喂什么、该出什么。
///
/// 字段分两半：`manifest_auth_kind` / `credentials_present` 是**输入**，
/// `expect_auth_kind` / `expect_auth_ready` 是**期望**（写成字面量，**不是**由
/// `acct_core::auth_ready` 算出来的 —— 算出来就成了循环自证）。两者的一致性由
/// [`the_golden_matches_the_rule`] 单独钉住：改了规则不改金样、或反过来，都红。
struct AuthKindParityCase {
    /// 账号名，同时也是它在夹具根下的目录名。
    name: &'static str,
    /// manifest 里 `authKind` 键写什么；`None` = 这个键**不出现**。
    manifest_auth_kind: Option<&'static str>,
    /// 要不要在它的目录里放一个 `CREDENTIALS_NAME`。
    credentials_present: bool,
    /// 期望产出的 `authKind`（金样）。
    expect_auth_kind: &'static str,
    /// 期望产出的 `authReady`（金样）。
    expect_auth_ready: bool,
}

/// 鉴权方式这一维的金样夹具（[`the_account_list_auth_kind_matches_the_golden`] 照它断账号列表的产出）。
///
/// | 格 | 它在守什么 |
/// |---|---|
/// | `sub-cred` | 正常订阅号 |
/// | `sub-nocred` | 阴性对照：订阅号缺凭据仍然不可选 |
/// | `api-nocred` | 正题：api-key 号缺凭据不被判不可用 |
/// | `api-cred` | api-key 号有凭据文件也一样 —— 它不看那个文件 |
/// | `legacy-nokind` | 旧 manifest（键缺席）⇒ 订阅 |
/// | `bogus-kind` | 认不出的值 ⇒ 保守落订阅，不是落 api-key |
const AUTH_KIND_PARITY_CASES: [AuthKindParityCase; 6] = [
    AuthKindParityCase {
        name: "sub-cred",
        manifest_auth_kind: Some(AUTH_KIND_SUBSCRIPTION),
        credentials_present: true,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "sub-nocred",
        manifest_auth_kind: Some(AUTH_KIND_SUBSCRIPTION),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
    AuthKindParityCase {
        name: "api-nocred",
        manifest_auth_kind: Some(AUTH_KIND_API_KEY),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_API_KEY,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "api-cred",
        manifest_auth_kind: Some(AUTH_KIND_API_KEY),
        credentials_present: true,
        expect_auth_kind: AUTH_KIND_API_KEY,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "legacy-nokind",
        manifest_auth_kind: None,
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
    AuthKindParityCase {
        name: "bogus-kind",
        manifest_auth_kind: Some("oauth-device"),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
];

/// 把 [`AUTH_KIND_PARITY_CASES`] 渲染成一份 schema v1 的 manifest（`root` = 夹具根的绝对路径）。
///
/// 目录与凭据文件由调用方按同一张表创建（见 `AuthKindParityCase::credentials_present`）。
fn auth_kind_parity_manifest(root: &str) -> String {
    let mut s =
        String::from("{\"version\":1,\"updatedAt\":\"2026-08-24T00:00:00Z\",\"sharedStore\":\"");
    s.push_str(root);
    s.push_str("/shared\",\"accounts\":[");
    for (i, c) in AUTH_KIND_PARITY_CASES.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":\"");
        s.push_str(c.name);
        s.push_str("\",\"email\":\"");
        s.push_str(c.name);
        s.push_str("@x.test\",\"configDir\":\"");
        s.push_str(root);
        s.push('/');
        s.push_str(c.name);
        s.push_str("\",\"isDefault\":false,\"mode\":\"isolated\"");
        if let Some(k) = c.manifest_auth_kind {
            s.push_str(",\"authKind\":\"");
            s.push_str(k);
            s.push('"');
        }
        s.push('}');
    }
    s.push_str("]}");
    s
}

/// ★ **金样与规则互钉。** 改了 `acct_core::auth_ready` 而没改金样、或反过来，这条红。
///
/// 金样里的 `expect_auth_ready` 是**手写字面量**（不是算出来的），所以本条不是恒真：
/// 把 `auth_ready` 的 api-key 那支改成 `credentials_present`，本条当场红两格。
#[test]
fn the_golden_matches_the_rule() {
    assert_eq!(
        AUTH_KIND_PARITY_CASES.len(),
        6,
        "金样格数变了 —— 先确认下面账号列表那条也照新格断了"
    );
    for c in &AUTH_KIND_PARITY_CASES {
        assert_eq!(
            acct_core::auth_kind_from_manifest(c.manifest_auth_kind),
            c.expect_auth_kind,
            "{}：分类规则与金样不一致",
            c.name
        );
        assert_eq!(
            acct_core::auth_ready(c.expect_auth_kind, c.credentials_present),
            c.expect_auth_ready,
            "{}：就绪规则与金样不一致",
            c.name
        );
    }
}

/// 夹具渲染器：路径真的被替换进去了，且 `manifest_auth_kind: None` 那格**没有**这个键
/// （不是写成 `"authKind":null` —— 缺席与 null 在 serde 上是两件事）。
#[test]
fn the_parity_manifest_renders_absent_keys_as_absent() {
    let m = auth_kind_parity_manifest("/tmp/fixture-root");
    assert!(m.starts_with("{\"version\":1,"), "schema 版本得是 1：{m}");
    assert!(
        m.contains("\"configDir\":\"/tmp/fixture-root/sub-cred\""),
        "{m}"
    );
    assert!(
        m.contains("\"sharedStore\":\"/tmp/fixture-root/shared\""),
        "{m}"
    );
    assert!(!m.contains("null"), "缺席的键不许渲染成 null：{m}");
    // 六格里有五格带 authKind（`legacy-nokind` 那格不带）。
    assert_eq!(m.matches("\"authKind\"").count(), 5, "{m}");
    // 认不出的那格照原样写进去 —— 分类规则由读侧负责，夹具不许提前替它决定。
    assert!(m.contains("\"authKind\":\"oauth-device\""), "{m}");
}

/// ★ **账号列表的鉴权方式一维照金样出。**
///
/// 喂的是 [`auth_kind_parity_manifest`]，断的是 [`AUTH_KIND_PARITY_CASES`]
/// 里手写的金样（`authKind` · `authReady` · `loggedIn`）。这一维的生产者只有本文件这一个。
#[test]
fn the_account_list_auth_kind_matches_the_golden() {
    let root = test_home::scratch("authkind-parity");
    for c in &AUTH_KIND_PARITY_CASES {
        let d = root.join(c.name);
        fs::create_dir_all(&d).unwrap();
        if c.credentials_present {
            fs::write(d.join(CREDENTIALS_NAME), "{\"tok\":\"SECRET-TOKEN\"}").unwrap();
        }
    }
    fs::create_dir_all(root.join("shared")).unwrap();
    write_manifest(&root, &auth_kind_parity_manifest(&root.to_string_lossy()));
    let lines = list_accounts(&root);
    // 首行是 meta，之后一行一个账号 —— 行数自检，防「少了几行也照样逐格绿」。
    assert_eq!(
        lines.len(),
        AUTH_KIND_PARITY_CASES.len() + 1,
        "行数对不上，逐格断言会漏掉没出来的那几个账号：{lines:?}"
    );
    for (i, c) in AUTH_KIND_PARITY_CASES.iter().enumerate() {
        let v: serde_json::Value = serde_json::from_str(&lines[i + 1]).unwrap();
        assert_eq!(v["name"], c.name, "顺序变了，下面几格就对错人了");
        assert_eq!(
            v["authKind"], c.expect_auth_kind,
            "{}：backend 产出的 authKind 与金样不一致",
            c.name
        );
        assert_eq!(
            v["authReady"], c.expect_auth_ready,
            "{}：backend 产出的 authReady 与金样不一致",
            c.name
        );
        // `loggedIn` 逐字节旧语义：仍然只是「凭据文件在不在」。
        assert_eq!(
            v["loggedIn"], c.credentials_present,
            "{}：loggedIn 的语义被这次改动动了（它该只是 stat 结果）",
            c.name
        );
    }
    for l in &lines {
        assert!(!l.contains("SECRET-TOKEN"), "输出里出现了凭据内容：{l}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// ★ `KAY4` 的 **Rust 侧那一格**（vitest 那条守卫扫不到这里）。
///
/// 守的性质：本文件里「鉴权方式这一维」**不许有第二条计算路径** ——
/// `authReady` 只许来自 `acct_core::auth_ready(`，`authKind` 只许来自
/// `acct_core::auth_kind_from_manifest(`。有人在这儿手写一个
/// `if kind == "api-key" { true } else { … }`，本条红。
///
/// ⚠ 射程如实写：它**只管本文件**（这一维的生产者只有本文件这一个），
/// 而且是**字面量扫描** —— 把两个 helper 重新 `use` 成别名就绕得过去。
/// 真正的地板不是它，是 `acct-core` 里只有一份实现。
#[test]
fn the_auth_dimension_has_exactly_one_computation_path() {
    let me = include_str!("../../../../src/backend/accounts/manage/library.rs");
    assert!(
        me.len() > 20_000,
        "include_str! 没读到源码，本条在空转（实得 {} 字节）",
        me.len()
    );
    // 只看生产段：`#[cfg(test)]` 之前的那一半（本文件的测试段自己就会提到这些名字）。
    let marker = "#[cfg(test)]";
    let cut = me
        .find(marker)
        .expect("找不到 #[cfg(test)] 锚点 —— 切法失效了");
    let prod_with_comments = &me[..cut];
    assert!(
        prod_with_comments.len() > 15_000,
        "生产段只切出 {} 字节 —— 锚点挪了，下面几条会零命中地绿",
        prod_with_comments.len()
    );
    // **去注释口径**：本文件的注释里就在解释这一维，按裸文本数会把散文也数进来
    // （第一版正是这么假红的：注释里两处 `api-key` 被当成了第二条计算路径）。
    // 全部是行注释，所以按行剥就够；剥完做锚点自检，防剥过头。
    let prod: String = prod_with_comments
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        prod.contains("fn scan_accounts(") && prod.len() > 8_000,
        "剥注释剥过头了（实得 {} 字节）—— 下面几条会零命中地绿",
        prod.len()
    );
    assert_eq!(
        prod.matches("auth_ready(").count(),
        1,
        "生产段里 `auth_ready(` 出现了不止一次 —— 要么有了第二条计算路径，要么该收进 acct-core"
    );
    assert_eq!(
        prod.matches("auth_kind_from_manifest(").count(),
        1,
        "生产段里 `auth_kind_from_manifest(` 出现了不止一次"
    );
    assert_eq!(
        prod.matches("auth_ready: ready").count(),
        1,
        "`authReady` 这一格在生产段里被写了不止一处"
    );
    // 阴性对照式自检：本文件的生产段里**不许**出现 api-key 这个字面量
    // （分类规则住 acct-core；这里出现它就意味着有人在本地又判了一次）。
    assert_eq!(
        prod.matches("api-key").count(),
        0,
        "生产段里出现了 `api-key` 字面量 —— 鉴权方式的分类只许住 acct-core"
    );
}

// ============================================================================
// 读环境那一侧：射程与文档对拍
// ============================================================================

/// 本文件生产段的**去注释**文本。
///
/// 🔴 **它自己不写剥法，调共享原语** —— 第一版写了一份（切到 `#[cfg(test)]` + 过滤
/// `//` 开头的行），`structural_scan::every_comment_stripping_transformer_is_registered`
/// 当场逮住它，逐字问：「先问共享原语为什么不够 —— 答得出来就登记，答不出来就改成调它」。
/// **答不出来**（`production_code` 做的就是这两件事）⇒ 改成调它。
/// ⚠ 那张登记表住 `src/frontend/shell/src/structural_scan.rs`，**这里不改** ——
/// 而它给的第一条出路本来就不需要动登记表。
fn production_text() -> String {
    let me = include_str!("../../../../src/backend/accounts/manage/library.rs");
    assert!(
        me.len() > 20_000,
        "include_str! 没读到源码，本条在空转（实得 {} 字节）",
        me.len()
    );
    let prod = guard_core::production_code(me);
    guard_core::assert_no_test_code("library.rs", &prod);
    assert!(
        prod.contains("fn session_accounts(agent_home: &Path")
            && prod.contains("struct SessionAccount {")
            && prod.len() > 8_000,
        "剥过头 / 锚点挪了（实得 {} 字节）—— 用它的那几条会零命中地绿",
        prod.len()
    );
    prod
}

/// ★★ **本文件读环境这件事的射程不许悄悄变大**〔本文件头注那条「两个写死的键」的判据〕。
///
/// 🔴 **主语就是「本文件」，不是「backend 全体」**：
/// 本条的分母是 `include_str!("../../../../src/backend/accounts/manage/library.rs")` 的生产段 —— **一个文件**，
/// 与测试名里那个 `this_module` 逐字对齐。
/// ⚠ backend 里**还有别处**在读 `/proc/<pid>/environ`，都在 `control/identity_tag.rs`：
/// `TMUX_PANE`（身份广播那一面）与 `CCM_RBIND_TOKEN`（启动期令牌，值是**敏感数据**）。
/// 它们**都不在本条视野里**，有自己那一把尺子：
/// `control::identity_tag::tests::the_env_keys_this_file_reads_are_exactly_two_named_constants`。
/// 两把尺子各量一个文件，不是同一条铁律的两个住址。别把这一格读成「全后端只读两个键」。
///
/// 守的性质：`/proc/<pid>/environ` 只抠**两个常量键**（`ANTHROPIC_BASE_URL` 只折成 `viaRelay` 一个布尔），键名**不许成为一维参数**。
/// 多一处 `proc_env_var(pid, …)` ⇒ 红，来这里回答「新那个键是什么、为什么它不
/// 把本查询变成任意环境变量读原语」。
#[test]
fn the_only_env_keys_this_module_reads_are_the_two_named_constants() {
    let prod = production_text();
    let total = prod.matches("proc_env_var(pid, ").count();
    assert_eq!(
        total, 2,
        "\n本文件生产段里 `proc_env_var(pid, …)` 有 {total} 处（登记 2 处）。\n\
             **多了** ⇒ 又读了一个环境变量：来模块头注那一格写清它是什么、\n\
             以及为什么这条查询仍然不是「任意环境变量读」原语。\n\
             **少了** ⇒ 有一条读回路被摘掉了。"
    );
    // 两个适配层的键（账号 · 上游地址）收成一处向注册表要（账号库面 `session_env`），键名仍是常量、不是参数。
    assert_eq!(
        prod.matches("crate::agents::account_library_face().map(|f| f.session_env)")
            .count(),
        1,
        "向适配层要那两个键的那一处不见了 / 变形了"
    );
    // 配置目录键只在 `owner_of` 里抠一处；它的三个调用方（会话那几行 · 宣告判号 · 换模型判这条会话用的号）都传适配层给的那个键。
    assert_eq!(
        prod.matches("proc_env_var(pid, key)").count(),
        1,
        "抠 `CLAUDE_CONFIG_DIR` 那一处（`owner_of`）不见了 / 变形了"
    );
    let calls = prod.matches("owner_of(pid, ").count();
    let keyed = prod.matches("owner_of(pid, env_keys.config_dir").count()
        + prod
            .matches("owner_of(pid, face.session_env.config_dir")
            .count();
    assert_eq!(
        (calls, keyed),
        (3, 3),
        "`owner_of` 的调用方变了，或有一处传的不是适配层给的配置目录键"
    );
    assert_eq!(
        prod.matches("proc_env_var(pid, env_keys.base_url)").count(),
        1,
        "抠 `ANTHROPIC_BASE_URL` 那一处不见了 / 变形了（`viaRelay` 的读侧）"
    );
}

/// ★ 本文件头注把读会话进程环境的**两个**键都点了名：加第二个键、却留着「只抠一个键」那句假话 ⇒ 红。
///
/// 只买「那几个名字都在场」；说得对不对是评审的活。
#[test]
fn the_header_names_both_env_keys_we_read() {
    // ── ② 「只抠几个键」那句诚实边界：本文件头注得把两个键点到名 ──
    let header: String = include_str!("../../../../src/backend/accounts/manage/library.rs")
        .lines()
        .take_while(|l| l.starts_with("//!") || l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        header.len() > 2_000,
        "头注只切出 {} 字节 —— 切法坏了，② 那格会零命中地绿",
        header.len()
    );
    for (what, hay) in [("本文件头注", header.as_str())] {
        for key in ["CLAUDE_CONFIG_DIR", "ANTHROPIC_BASE_URL"] {
            assert!(
                hay.contains(key),
                "\n★ {what} 里没点名 `{key}` —— 「`/proc/<pid>/environ` 只抠哪几个键」\n\
                     这句诚实边界在那儿就成了假话（漏一处就是留假话，派工单逐字）。"
            );
        }
    }
}

// ============================================================================
// 帧面出成品：`accounts-list`（并上这台机器自己那份 apikey 表）
// ============================================================================
//
// 要求：「一次性请求那半收口成 `call` —— 按能力分批」· 「一个判定只有一个家」·
// 「中转 ＋ 上游选择住本机常驻后端进程」⇒「账号域读自己那台的 apikey 表、
// 两条规则搬进 `acct-core`、agent 随请求带」。夹具只造结构（目录名 ＋ 占位 manifest），不采真账号数据。

/// 夹具：账号库（一个账号 0 ＋ 两个隔离号，`acct-a` 有订阅凭据、`acct-b` 没有）。回 `(root, accts)`。
fn c4c_fixture(tag: &str, with_zero: bool) -> (PathBuf, PathBuf) {
    let root = test_home::scratch(tag);
    let accts = root.join("accts");
    let a = accts.join("acct-a");
    fs::create_dir_all(&a).unwrap();
    fs::write(a.join(".credentials.json"), "{}").unwrap();
    fs::create_dir_all(accts.join("acct-b")).unwrap();
    fs::create_dir_all(root.join("shared")).unwrap();
    let zero = if with_zero {
        r#"{"name":"zero","isDefault":false},"#
    } else {
        ""
    };
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"updatedAt":"t0","sharedStore":"{s}","accounts":[{zero}
                {{"name":"a","email":"a@x","configDir":"{a}","isDefault":true}},
                {{"name":"b","configDir":"{b}"}}]}}"#,
            s = root.join("shared").display(),
            a = a.display(),
            b = accts.join("acct-b").display(),
        ),
    );
    (root, accts)
}

/// ★★ 成品里的账号 == 扫描本体那几行（表里没行时逐条相等），meta == 扫描的 meta；成品顶层键集合恒等 `{meta, accounts, notice}`。
#[test]
fn the_list_product_carries_exactly_what_the_scan_reads() {
    let (root, accts) = c4c_fixture("c4c-same", true);
    let cli = list_accounts(&accts);
    let product = list_product_with(&accts, None, &[], &[], "claude-code", "claude-code", false);
    let keys: Vec<&String> = product.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["accounts", "meta", "notice"], "成品顶层键集合变了");
    let cli_meta = meta(&cli);
    // 成品比扫描多出的只有帧面那几格（每号目录的显示形 · key 掩码与端点 · 徽章），其余逐格同一份扫描。
    let mut product = product;
    for a in product["accounts"].as_array_mut().unwrap() {
        let o = a.as_object_mut().unwrap();
        // 家目录说不出 ⇒ 显示形就是原样那一格。
        assert_eq!(o.remove("configDirShown").as_ref(), o.get("configDir"));
        assert_eq!(o.remove("keyMasked"), Some(serde_json::Value::Null));
        assert_eq!(o.remove("endpointHost"), Some(serde_json::Value::Null));
        assert!(o.remove("badge").is_some(), "帧面每个号带徽章");
        assert!(o.remove("plate").is_some(), "帧面每个号带号牌字");
        assert!(o.remove("kindLine").is_some(), "帧面每个号带第二行那一句");
        assert!(o.remove("slots").is_some(), "帧面每个号带额度那几格");
        assert!(
            o.remove("unselectable").is_some(),
            "帧面每个号带选不了的那一句"
        );
        assert!(o.remove("login").is_some(), "帧面每个号带给不给［登录…］");
        assert!(
            o.remove("newSession").is_some(),
            "帧面每个号带进不进新建会话的选号表"
        );
    }
    assert_eq!(
        product["meta"], cli_meta,
        "成品 meta 与扫描的 meta 不是同一份"
    );
    let cli_rows: Vec<serde_json::Value> = cli[1..]
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(product["accounts"], serde_json::Value::Array(cli_rows));
    assert_eq!(
        product["accounts"].as_array().unwrap().len(),
        3,
        "夹具三个号"
    );
    assert!(product["notice"].is_null(), "有账号 0 却出了缺账号 0 那句");
    let _ = fs::remove_dir_all(&root);
}

/// ★★ **apikey 表真并上了**：表里有 `acct-b` 那一行 ⇒ 它（没有订阅凭据）变成 `api-key` 且可选；
/// 别家 agent ⇒ 一格不动；空表 ⇒ 一格不动；表里的 id 对不上任何号 ⇒ 一格不动。
#[test]
fn the_list_product_merges_this_machines_apikey_table_per_agent() {
    let (root, accts) = c4c_fixture("c4c-table", true);
    let row_b = |v: &serde_json::Value| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == "b")
            .cloned()
            .unwrap()
    };
    let merged = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &[],
        "claude-code",
        "claude-code",
        false,
    );
    let b = row_b(&merged);
    assert_eq!(
        b["authKind"],
        acct_core::AUTH_KIND_API_KEY,
        "表里有行却没按 api-key 算"
    );
    assert_eq!(b["authReady"], true, "api-key 号不看订阅凭据，应当可选");
    assert_eq!(b["loggedIn"], false, "订阅凭据那一格照旧是 stat 结果");
    // 表里的行只动它那一个号。
    let a = merged["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "a")
        .cloned()
        .unwrap();
    assert_eq!(a["authKind"], acct_core::AUTH_KIND_SUBSCRIPTION);
    for (rows, agent, why) in [
        (vec!["acct-b".to_string()], "codex", "别家的号不许借这张表"),
        (Vec::new(), "claude-code", "空表"),
        (
            vec!["acct-z".to_string()],
            "claude-code",
            "表里的 id 对不上任何号",
        ),
    ] {
        let b = row_b(&list_product_with(
            &accts,
            None,
            &rows,
            &[],
            agent,
            "claude-code",
            false,
        ));
        assert_eq!(b["authKind"], acct_core::AUTH_KIND_SUBSCRIPTION, "{why}");
        assert_eq!(b["authReady"], false, "{why}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// ★ 缺账号 0 ⇒ `notice` 是一句话（不带「远端」—— 本机远端同一条路）；没启用 ⇒ `null`。
#[test]
fn the_list_product_says_when_account_zero_is_missing() {
    let (root, accts) = c4c_fixture("c4c-nozero", false);
    let v = list_product_with(&accts, None, &[], &[], "claude-code", "claude-code", false);
    let n = v["notice"].as_str().expect("缺账号 0 却没出那一句");
    assert!(
        n.contains(copy_core::copy_static!(
            "beAccountsQuery.listProductAt.noDefault"
        )),
        "{n}"
    );
    let off = list_product_with(
        &root.join("nope"),
        None,
        &[],
        &[],
        "claude-code",
        "claude-code",
        false,
    );
    assert_eq!(off["meta"]["enabled"], false);
    assert!(off["notice"].is_null(), "没启用谈不上缺账号 0");
    let _ = fs::remove_dir_all(&root);
}

/// 金样那份 apikey 表：`acct-b` 一行（key ＋ 端点）· `acct-a` 一行只有端点（订阅号不带出去）。经真读口 `key_facts_at` 读回。
fn golden_key_facts(root: &Path) -> Vec<crate::accounts::upstream_select::file_face::KeyFact> {
    let p = root.join("apikey-credentials.json");
    fs::write(
        &p,
        r#"{"accounts":{"acct-b":{"api_key":"sk-ant-0123456789-a1b2","base_url":"https://api.example.com"},"acct-a":{"base_url":"https://x.example.com"}}}"#,
    )
    .unwrap();
    crate::accounts::upstream_select::file_face::key_facts_at(&p)
}

/// ★★ API 号带出那台表里的掩码（只留末四位）与端点；订阅号 · 表里没它 · 只配了端点没配 key 的那一格是 `null`；
/// 明文一个字节不出去。
#[test]
fn the_list_product_carries_each_api_accounts_masked_key_and_address() {
    let (root, accts) = c4c_fixture("c4c-keys", true);
    let facts = golden_key_facts(&root);
    let v = list_product_with(
        &accts,
        Some(&root.join("home")),
        &["acct-b".to_string()],
        &facts,
        "claude-code",
        "claude-code",
        false,
    );
    assert!(!v.to_string().contains("0123456789"), "回了明文：{v}");
    let row = |n: &str| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == n)
            .cloned()
            .unwrap()
    };
    assert_eq!(row("b")["keyMasked"], "••••••••a1b2");
    // 端点只出写好的主机那一格（账号行第二行那一句里的主机也照它），原地址不出帧面。
    assert_eq!(row("b")["endpointHost"], "api.example.com");
    assert!(row("b").get("baseUrl").is_none(), "原地址还在帧面上");
    assert!(
        row("a")["keyMasked"].is_null() && row("a")["endpointHost"].is_null(),
        "订阅号不带"
    );
    // 号目录的显示形由这台按自己的家目录写（家 ＝ `root/home`，号目录不在它下面 ⇒ 原样）；只为缩写带的 `meta.home` 不再出。
    assert_eq!(row("b")["configDirShown"], row("b")["configDir"]);
    assert!(
        v["meta"].get("home").is_none(),
        "meta 里还有 home：{}",
        v["meta"]
    );
    let under = list_product_with(
        &accts,
        Some(&root),
        &[],
        &[],
        "claude-code",
        "claude-code",
        false,
    );
    let b = under["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "b")
        .cloned()
        .unwrap();
    assert_eq!(b["configDirShown"], "~/accts/acct-b");
    let zero = under["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["configDir"].is_null())
        .cloned();
    if let Some(z) = zero {
        assert!(
            z["configDirShown"].is_null(),
            "账号 0 没有号目录 ⇒ 显示形也是 null"
        );
    }
    // 表里没有它那一行 ⇒ 两格 null（照样是 api-key 号）。
    let none = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &[],
        "claude-code",
        "claude-code",
        false,
    );
    let b = none["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "b")
        .cloned()
        .unwrap();
    assert!(
        b["keyMasked"].is_null() && b["endpointHost"].is_null() && none["meta"]["home"].is_null()
    );
    let _ = fs::remove_dir_all(&root);
}

/// ★★ 账号行第二行那一句是后端写好的 `kindLine`：类型字与额度卡首行同一处写（`name_words::kind_text`），
/// 后面跟这个号的那一格（订阅已登录 ⇒ 邮箱 · 订阅没登录 ⇒ 需登录 · 按量号配了端点 ⇒ 端点的主机）；两端照摆、不照 `authKind` 选字。
#[test]
fn each_account_row_carries_its_written_kind_line() {
    let (root, accts) = c4c_fixture("c4c-kind-line", true);
    let sep = copy_text("kit.text.sep", &[]);
    let sub = copy_text("acct.kind.sub", &[]);
    let api = copy_text("acct.kind.api", &[]);
    let line = |v: &serde_json::Value, n: &str| {
        v["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == n)
            .map(|a| a["kindLine"].clone())
            .unwrap()
    };
    let routed = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &golden_key_facts(&root),
        "claude-code",
        "claude-code",
        false,
    );
    assert_eq!(
        line(&routed, "a"),
        format!("{sub}{sep}a@x"),
        "订阅已登录 ⇒ 邮箱"
    );
    assert_eq!(
        line(&routed, "b"),
        format!("{api}{sep}api.example.com"),
        "按量号配了端点 ⇒ 端点的主机"
    );
    assert_eq!(
        line(&routed, "zero"),
        format!("{sub}{sep}{}", copy_text("acct.tag.login", &[])),
        "订阅没登录 ⇒ 需登录"
    );
    let bare = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &[],
        "claude-code",
        "claude-code",
        false,
    );
    assert_eq!(line(&bare, "b"), api, "按量号没配端点 ⇒ 只有类型字");
    let _ = fs::remove_dir_all(&root);
}

/// ★★ 端点的主机只在后端写一处：`endpointHost` 去掉 scheme · 路径 · 查询，留端口；账号行第二行那一句里的主机就是这一格。
#[test]
fn the_endpoint_host_is_written_once_and_the_kind_line_reuses_it() {
    let (root, accts) = c4c_fixture("c4c-endpoint-host", true);
    let p = root.join("apikey-credentials.json");
    fs::write(
        &p,
        r#"{"accounts":{"acct-b":{"api_key":"sk-ant-0123456789-a1b2","base_url":"https://gw.example.com:8443/v1?x=1#f"}}}"#,
    )
    .unwrap();
    let facts = crate::accounts::upstream_select::file_face::key_facts_at(&p);
    let v = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &facts,
        "claude-code",
        "claude-code",
        false,
    );
    let b = v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "b")
        .cloned()
        .unwrap();
    assert_eq!(b["endpointHost"], "gw.example.com:8443");
    let sep = copy_text("kit.text.sep", &[]);
    assert_eq!(
        b["kindLine"],
        format!(
            "{}{sep}gw.example.com:8443",
            copy_text("acct.kind.api", &[])
        )
    );
    let _ = fs::remove_dir_all(&root);
}

/// ★★ **金样**：账号清单成品对同一份夹具 == `tests/__fixtures__/accounts.golden.json`（夹具根替换成 `<root>`）。
/// 界面那一侧按生成类型收（形状由握手指纹管），不再另读这份。金样手写落盘（本条红时印出现打的成品，人读过再改）。
#[test]
fn the_account_products_match_the_cross_language_golden() {
    let (root, accts) = c4c_fixture("c4c-golden", true);
    let got = serde_json::json!({
        "accounts-list": list_product_with(
            &accts,
            Some(&root.join("home")),
            &["acct-b".to_string()],
            &golden_key_facts(&root),
            "claude-code",
            "claude-code",
            true,
        ),
    });
    let got: serde_json::Value = serde_json::from_str(
        &got.to_string()
            .replace(&root.to_string_lossy().to_string(), "<root>"),
    )
    .unwrap();
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../../__fixtures__/accounts.golden.json"))
            .expect("金样不是合法 JSON");
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        got,
        want,
        "帧面成品与跨语言金样不一致。现打：\n{}",
        serde_json::to_string_pretty(&got).unwrap()
    );
}

// ════════════════════════════════════════════════════════════════════════════════════════
// 账号清单读 manifest 的锚点 ——被测对象是后端这份真在答账号清单的实现
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 判据锚点挂在现存实现上，不挂零生产调用方的参照实现。

/// ★ 账号库住后端的家里（`<家目录>/.cc-monitor/accounts/accounts.json`）：写账号库 / 读清单 / 起会话三侧共用契约 crate 那两格，
/// 写死成字面量核对；而这里的解析**只跟着家走**（不收参数、不读环境变量）、恰经那一个常量拼出来。
#[test]
fn the_accounts_library_lives_under_the_contract_directory_name() {
    assert_eq!(
        (
            relay_route_core::ACCOUNTS_DIR_REL,
            relay_route_core::ACCOUNTS_MANIFEST_NAME
        ),
        (".cc-monitor/accounts", "accounts.json"),
        "账号库位置变了 —— 改了它后端就去别处找账号库，界面上只表现为「一个账号都没有」"
    );
    let prod = guard_core::production_code(include_str!(
        "../../../../src/backend/accounts/manage/library.rs"
    ));
    let at = guard_core::find_pinned(&prod, "fn resolve_accts_dir() -> PathBuf {")
        .unwrap_or_else(|e| panic!("缺省解析那一处找不到（恰好一处、不收参数）：{e}"));
    let body_end = prod[at + 1..]
        .find("\nfn ")
        .map_or(prod.len(), |k| at + 1 + k);
    guard_core::find_pinned(&prod[at..body_end], "home_join(&h, ACCOUNTS_DIR_REL)")
        .unwrap_or_else(|e| panic!("缺省解析不再经契约常量拼家目录下那一层：{e}"));
}

/// 守的要求：用户「即后端去.cc-monitor读数据」——账号库的位置只跟着家走，旧位置不认、另指位置的环境变量删掉。
/// ① 产品（`src/` 全树的文本文件 ＋ 两份 README）里旧账号库目录名与那个环境变量名**零处**；
/// ② 那个变量名在测试脚本、门禁、CI 里也零处（全仓只剩 CHANGELOG 旧版本段与 `tests/evidence/` 的冻结读数）。
/// 正控：同一个扫描器在产品里找新位置那一段的唯一住址（宏名），命中的文件集合恰好是契约 crate 与足迹两份；
/// 脚本那一族的人群里真有门禁与 ccm 那套 e2e；合成串里两个针都认得出。
#[test]
fn nothing_in_the_product_still_points_at_the_old_account_library() {
    let root = guard_core::addr::repo_root();
    // 针运行时拼：本文件自己不进人群也不含针。
    let old_dir = format!(".claude-{}", "accts");
    let old_env = format!("CCM_ACCTS_{}", "MANIFEST");
    // 本文件明写在排除名单里（针虽是运行时拼的，扫描口径仍走那一份带「摘不到就 panic」的遍历）。
    const SELF: &str = "tests/backend/accounts/manage/library_tests.rs";
    let rel = |p: &Path| {
        p.strip_prefix(&root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let hits = |pop: &[(PathBuf, String)], needle: &str| -> std::collections::BTreeSet<String> {
        pop.iter()
            .filter(|(_, t)| t.contains(needle))
            .map(|(p, _)| rel(p))
            .collect()
    };
    let mut product = guard_core::scan_tree_excluding(&root.join("src"), &[], &[]);
    for f in ["README.md", "README.en.md"] {
        product.push((root.join(f), fs::read_to_string(root.join(f)).unwrap()));
    }
    let mut scripts: Vec<(PathBuf, String)> =
        guard_core::scan_tree_excluding(&root.join("tests"), &[], &[SELF])
            .into_iter()
            .filter(|(p, _)| !rel(p).starts_with("tests/evidence/"))
            .collect();
    scripts.extend(guard_core::scan_tree_excluding(
        &root.join(".github"),
        &[],
        &[],
    ));
    scripts.push((
        root.join("package.json"),
        fs::read_to_string(root.join("package.json")).unwrap(),
    ));

    // 正控
    assert_eq!(
        hits(&product, "accounts_dir_rel!"),
        [
            "src/backend/agents/claudecode/footprint.rs",
            "src/common/relay-route-core/src/lib.rs"
        ]
        .map(String::from)
        .into(),
        "新位置那一段的住址不是恰好这两份 —— 扫描器没读到产品树，或住址多了一处"
    );
    for must in ["tests/scripts/gate.sh", "tests/e2e/ccm-cli.test.sh"] {
        assert!(
            scripts.iter().any(|(p, _)| rel(p) == must),
            "脚本那一族的人群里没有 {must} —— 扫描器没读到"
        );
    }
    let fake = vec![(root.join("x"), format!("a {old_dir}/b {old_env}=c"))];
    assert_eq!(
        hits(&fake, &old_dir).len() + hits(&fake, &old_env).len(),
        2,
        "合成串里的针没认出来"
    );

    // 正题
    let empty = std::collections::BTreeSet::<String>::new();
    assert_eq!(hits(&product, &old_dir), empty, "产品里还有旧账号库目录名");
    assert_eq!(
        hits(&product, &old_env),
        empty,
        "产品里还有另指账号库位置的环境变量"
    );
    assert_eq!(
        hits(&scripts, &old_env),
        empty,
        "测试脚本 / 门禁里还有另指账号库位置的环境变量"
    );
}

/// **`viaRelay` 答的是「这条活会话的进程环境里，上游地址是不是本机中转那一形」**：
/// 带钥匙段的回环中转地址 ⇒ `true`；别的地址（直连 / 没带钥匙的旧形状）⇒ `false`；没设 ⇒ `false`；进程已死 ⇒ `null`。
/// 而且**值本身不出参**（它带着中转钥匙）。守的要求：「`session_accounts` 多读 `ANTHROPIC_BASE_URL` ·
/// `accounts-sessions` 每行加 `viaRelay: true|false|null`」；`INVARIANTS §48.1a`「钥匙 …… `relay-status` 应答里也没有它」同族。
/// 形状：真子进程（`sleep`，各带一份环境）＋ 真 pidfile（`procStart` 对得上）⇒ 逐条相等；输出全文零命中钥匙（正控：钥匙在子进程环境里）。
#[test]
#[cfg(target_os = "linux")]
fn hx1_via_relay_says_which_live_sessions_point_at_the_local_relay_and_never_leaks_the_url() {
    let root = test_home::scratch("viarelay");
    let claude = root.join("claude");
    let sessions = claude.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let key = "0123456789abcdef".repeat(4);
    let keyed = format!("http://127.0.0.1:8788/{key}/s/claude-code/acct1");
    let cases: [(&str, Option<String>, serde_json::Value); 3] = [
        ("sid-relay", Some(keyed.clone()), serde_json::json!(true)),
        (
            "sid-direct",
            Some("https://api.example.invalid".into()),
            serde_json::json!(false),
        ),
        ("sid-unset", None, serde_json::json!(false)),
    ];
    let mut kids = Vec::new();
    for (sid, url, _) in &cases {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("30").env_remove("ANTHROPIC_BASE_URL");
        if let Some(u) = url {
            cmd.env("ANTHROPIC_BASE_URL", u);
        }
        let child = cmd.spawn().expect("起 sleep");
        let pid = child.id();
        wait_exec_done(pid, "sleep"); // 环境在 exec 走完之后才是新程序的
        let ticks = proc_starttime(pid).expect("starttime");
        fs::write(
            sessions.join(format!("{pid}.json")),
            format!(r#"{{"sessionId":"{sid}","cwd":"/w","procStart":"{ticks}"}}"#),
        )
        .unwrap();
        kids.push(child);
    }
    // 一条已死的：pid 不在 ⇒ null。
    fs::write(
        sessions.join("4194301.json"),
        r#"{"sessionId":"sid-dead","cwd":"/w","procStart":"1"}"#,
    )
    .unwrap();
    let lines = session_accounts(&claude, &root.join("no-accts"));
    for k in &mut kids {
        let _ = k.kill();
        let _ = k.wait();
    }
    let by = sid_map(&lines);
    for (sid, _, want) in &cases {
        assert_eq!(by[*sid]["viaRelay"], *want, "{sid}：{:?}", by[*sid]);
    }
    assert_eq!(by["sid-dead"]["viaRelay"], serde_json::Value::Null);
    let all = format!("{lines:?}");
    assert!(
        !all.contains(&key) && !all.contains("api.example.invalid"),
        "上游地址（带钥匙）进了出参：{all}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `machine-interrupts` 金样：每一形由生产函数现产，与 `tests/__fixtures__/machine-interrupts.golden.json` 逐格相等；
/// 界面那一侧（`tests/frontend/ui/settings/interrupts.vitest.ts`）读同一份严格解码。
#[test]
fn the_machine_product_matches_the_cross_language_golden() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../../__fixtures__/machine-interrupts.golden.json"
    ))
    .expect("金样不是 JSON");
    let cases = golden["cases"].as_array().expect("金样没有 cases");
    assert_eq!(cases.len(), 3, "金样的形数变了 —— 两侧一起改");
    for c in cases {
        let lines: Vec<SessionAccount> = c["lines"]
            .as_array()
            .unwrap()
            .iter()
            // 金样里那行「不是 JSON」：结构体那一侧没有读不懂的行，读不懂的不进名单。
            .filter_map(|l| row_of(l.as_str().unwrap()))
            .collect();
        let forwards = c["forwards"].as_u64().unwrap() as u32;
        assert_eq!(
            serde_json::to_value(machine_product(&lines, forwards)).unwrap(),
            c["reply"],
            "「{}」：现产的成品与金样对不上",
            c["name"]
        );
    }
}

/// 删掉默认号之后新会话默认谁（`meta.nextDefault`）：与删号那一条同一个答案（写侧 `Manifest::without`）——
/// 夹具里默认号是 `a`，删它之后接班的是清单里下一个具名号；没有清单 ⇒ `null`。
#[test]
fn the_list_meta_says_who_becomes_default_once_the_default_is_removed() {
    let (root, accts) = c4c_fixture("next-default", false);
    let product = list_product_with(&accts, None, &[], &[], "claude-code", "claude-code", false);
    assert_eq!(product["meta"]["nextDefault"], serde_json::json!("b"));
    let empty = test_home::scratch("next-default-none");
    let none = list_product_with(&empty, None, &[], &[], "claude-code", "claude-code", false);
    assert!(
        none["meta"]["nextDefault"].is_null(),
        "没有清单却答出了接班的号"
    );
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&empty);
}

/// 「现在重启 cc-monitor」那一问（`appExit`）：这台后端选了随 cc-monitor 退出一起停 ⇒ 这台的会话 ＋ 账上全部转发都会断；
/// 选了留着 ⇒ 什么都不断（全零，界面就不弹框）。
#[test]
fn app_exit_interrupts_follow_this_backends_own_exit_choice() {
    let lines = vec![
        row_of(r#"{"alive":true,"viaRelay":true}"#).unwrap(),
        row_of(r#"{"alive":true,"viaRelay":false}"#).unwrap(),
    ];
    let zero =
        serde_json::json!({"relayedSessions":0,"relayedMaybe":0,"liveStreams":0,"forwards":0});
    let wire = |p: Interrupts| serde_json::to_value(p).unwrap();
    assert_eq!(wire(app_exit_product(false, &lines, 3)), zero);
    assert_eq!(
        wire(app_exit_product(true, &lines, 3)),
        serde_json::json!({"relayedSessions":1,"relayedMaybe":0,"liveStreams":2,"forwards":3})
    );
}

// ---- 能不能选 · 默认号是谁（跨三处金样 `tests/__fixtures__/account-default.golden.json`）----

/// ★★ 清单成品逐号带 `selectable`、`meta` 带 `effectiveDefault`，与金样逐格相等。
/// 起会话跟随（控制面 `launch_account::pick`）读的就是这一份成品，那一半挂主 crate（`accounts_query_on_upper_tests.rs`）。
#[test]
fn the_list_product_says_who_is_selectable_and_who_is_the_default() {
    let golden = account_default_golden();
    let cases = golden["cases"].as_array().unwrap();
    assert!(cases.len() >= 6, "金样被删薄了");
    for (i, case) in cases.iter().enumerate() {
        let what = case["what"].as_str().unwrap();
        let (root, accts) = golden_library(&format!("dflt{i}"), case);
        let product =
            list_product_with(&accts, None, &[], &[], "claude-code", "claude-code", false);
        let got: Vec<&str> = product["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["selectable"] == serde_json::json!(true))
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        let want: Vec<&str> = case["selectable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(got, want, "{what}：能选的号");
        for a in product["accounts"].as_array().unwrap() {
            assert!(a["selectable"].is_boolean(), "{what}：每号都带 selectable");
        }
        assert_eq!(
            product["meta"]["effectiveDefault"], case["effectiveDefault"],
            "{what}：默认号"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

/// ★★ 进不进新建会话的选号表是成品写好的一格（`newSession`），两个出口照它列、不自己判：
/// 没有配置目录（账号 0）⇒ 不进；原地模式 ⇒ 不进；隔离、有目录 ⇒ 进（哪怕还需登录：选不选得了另看 `selectable`）。
#[test]
fn the_list_product_says_who_goes_into_the_new_session_picker() {
    let root = test_home::scratch("newsession");
    let accts = root.join("accts");
    let iso = accts.join("iso");
    let inplace = accts.join("inplace");
    fs::create_dir_all(&iso).unwrap();
    fs::create_dir_all(&inplace).unwrap();
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"acctsDir":"{a}","accounts":[
                {{"name":"base","email":"","isDefault":true,"mode":"isolated"}},
                {{"name":"iso","email":"","configDir":"{i}","isDefault":false,"mode":"isolated"}},
                {{"name":"inplace","email":"","configDir":"{p}","isDefault":false,"mode":"in-place"}}]}}"#,
            a = accts.display(),
            i = iso.display(),
            p = inplace.display()
        ),
    );
    let product = list_product_with(&accts, None, &[], &[], "claude-code", "claude-code", false);
    let cell = |name: &str| {
        product["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap_or_else(|| panic!("清单里没有 {name}"))["newSession"]
            .clone()
    };
    assert_eq!(
        cell("base"),
        serde_json::json!(false),
        "没有配置目录 ⇒ 不进选号表"
    );
    assert_eq!(
        cell("inplace"),
        serde_json::json!(false),
        "原地模式 ⇒ 不进选号表"
    );
    assert_eq!(
        cell("iso"),
        serde_json::json!(true),
        "隔离、有目录 ⇒ 进（需登录照样列，灰着）"
    );
    let _ = fs::remove_dir_all(&root);
}

/// ★★ 徽章一格只出自这一处（`badge_of`，界面照抄）：原地模式 · API key 号按「key 表里有没有它那一行」×「这台的中转在不在」·
/// 账号 0 的 API key 号说不出有没有那一行 ⇒ 同「没配上」那一句 · 订阅号按登录了没有。
#[test]
fn the_badge_is_written_by_the_core() {
    let t = |k: &str| copy_core::copy_text(k, &[]);
    let b = |in_place, api, ready, routed, running| {
        let x = badge_of(in_place, api, ready, routed, running);
        (x.text.0, x.warn)
    };
    assert_eq!(
        b(true, true, true, Some(true), true),
        (t("accounts.badge.inPlace"), true)
    );
    assert_eq!(
        b(false, true, false, Some(true), true),
        (t("accounts.badge.apikeyRelayed"), false)
    );
    assert_eq!(
        b(false, true, false, Some(true), false),
        (t("accounts.badge.apikeyRelayDown"), true)
    );
    assert_eq!(
        b(false, true, false, Some(false), true),
        (t("accounts.badge.apikeyNoEndpoint"), true)
    );
    assert_eq!(
        b(false, true, false, None, true),
        (t("accounts.badge.apikeyNoEndpoint"), true)
    );
    assert_eq!(
        b(false, false, false, None, true),
        (t("acct.tag.login"), true)
    );
    assert_eq!(
        b(false, false, true, None, false),
        (t("accounts.badge.signedIn"), false)
    );
}

/// ★ 号色：清单每号出 `color`（界面只画它）。盘上有就照盘上；还没补过色的旧清单照补色那一处（`Manifest::with_colors`）现算，
/// 与常驻那一个补完写回去的一样 —— 名字哈希同格的 team / lab 读出来也不同色；读这一下不写盘。
#[test]
fn the_listing_hands_over_each_accounts_color() {
    let root = test_home::scratch("colors");
    let accts = root.join("accts");
    for n in ["team", "lab", "old"] {
        fs::create_dir_all(accts.join(n)).unwrap();
    }
    let body = format!(
        r#"{{"version":1,"sharedStore":"{s}","accounts":[
            {{"name":"team","configDir":"{t}"}},
            {{"name":"old","configDir":"{o}","color":2}},
            {{"name":"lab","configDir":"{l}"}},
            {{"name":"0","email":"","isDefault":false,"mode":"bare"}}]}}"#,
        s = root.join("shared").display(),
        t = accts.join("team").display(),
        o = accts.join("old").display(),
        l = accts.join("lab").display(),
    );
    write_manifest(&accts, &body);
    let lines = list_accounts(&accts);
    let color = |name: &str| {
        lines[1..]
            .iter()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .find(|v| v["name"] == name)
            .map(|v| v["color"].clone())
            .unwrap_or_else(|| panic!("没有 {name}：{lines:?}"))
    };
    assert_eq!(color("team"), 0);
    assert_eq!(color("old"), 2, "盘上那一格照抄");
    assert_eq!(color("lab"), 1);
    assert_eq!(color("0"), 3, "账号 0 排在库序末尾");
    assert_eq!(
        fs::read_to_string(accts.join("accounts.json")).unwrap(),
        body,
        "读清单不写盘（补色只由常驻那一个写）"
    );
    let _ = fs::remove_dir_all(&root);
}

/// 每个号带一格 `label`：号在人眼里叫什么（后端写好，界面 · 手机照抄）—— 账号 0 叫「~/.claude」那一句，其余照原名。
/// 行的格就是原来那几格加这一格（别的不动）。
#[test]
fn each_row_carries_the_label_people_see() {
    let root = test_home::scratch("label");
    let accts = root.join("accts");
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"sharedStore":"/s","accounts":[
                    {{"name":"z","configDir":{z:?}}},
                    {{"name":"0","isDefault":false,"mode":"bare"}}]}}"#,
            z = accts.join("z").to_string_lossy()
        ),
    );
    let lines = list_accounts(&accts);
    let z: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
    let zero: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
    assert_eq!(z["label"], "z");
    assert_eq!(zero["label"], copy_core::copy_text("acct.home.name", &[]));
    let keys: Vec<&str> = z.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "authKind",
            "authReady",
            "color",
            "configDir",
            "email",
            "exists",
            "isDefault",
            "label",
            "loggedIn",
            "mode",
            "name",
            "selectable"
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

/// ★ 选不了的那一句只出自一处、说的是四条里没过的那一条（不隔离 · 号目录不在 · 按量号没配 key · 订阅号没登录）；
/// 选得了 · 账号 0（不经选号）⇒ 没有。号牌字照名字取（字母数字开头两个字 · 别的一个字 · 空名 `?`）。
#[test]
fn a_row_says_why_it_cannot_be_picked_and_its_plate() {
    let row =
        |mode: &str, exists: bool, api: bool, ready: bool, dir: Option<&str>| ScannedAccount {
            name: "n".into(),
            label: "n".into(),
            email: String::new(),
            config_dir: dir.map(str::to_string),
            is_default: false,
            mode: mode.into(),
            exists,
            logged_in: ready,
            auth_kind: if api {
                acct_core::AUTH_KIND_API_KEY
            } else {
                acct_core::AUTH_KIND_SUBSCRIPTION
            }
            .into(),
            auth_ready: ready,
            selectable: acct_core::account_selectable(mode, ready, exists, dir.is_some()),
            color: Some(0),
        };
    let why = |a: ScannedAccount| unselectable_of(&a, a.auth_kind == acct_core::AUTH_KIND_API_KEY);
    let t = |k: &str| Some(copy_core::copy_text(k, &[]));
    assert_eq!(
        why(row("isolated", true, false, true, Some("/d"))),
        None,
        "选得了"
    );
    assert_eq!(
        why(row("isolated", true, false, false, None)),
        None,
        "账号 0"
    );
    assert_eq!(
        why(row("in-place", true, false, true, Some("/d"))),
        t("accounts.badge.inPlace")
    );
    assert_eq!(
        why(row("isolated", false, false, true, Some("/d"))),
        t("accounts.row.missing")
    );
    assert_eq!(
        why(row("isolated", true, true, false, Some("/d"))),
        t("acct.tag.key")
    );
    assert_eq!(
        why(row("isolated", true, false, false, Some("/d"))),
        t("acct.tag.login")
    );
    use crate::accounts::quota::name_words::plate_text;
    assert_eq!(
        ["work", "w", "工作", "", "0"].map(plate_text),
        ["wo", "w", "工", "?", "0"].map(str::to_string)
    );
}

/// ★ 每个号给不给［登录…］：只差登录、这一家能在 app 里登录 ⇒ 给；按量号没配 key · 号目录不在 · 不隔离 · 已就绪 ⇒ 不给；
/// 只差登录却登不了 ⇒ 不给、那一句说「需登录 · 不支持在 app 里登录」。
#[test]
fn only_an_account_that_only_needs_signing_in_gets_the_login_button() {
    use test_support::row_with;
    let list = |sign_in: bool| {
        let mut l = test_support::list(vec![
            row_with("ready", |_| {}),
            row_with("need", |a| a.auth_ready = false),
            row_with("api", |a| {
                a.auth_kind = acct_core::AUTH_KIND_API_KEY.to_string();
                a.auth_ready = false;
            }),
            row_with("gone", |a| {
                a.exists = false;
                a.auth_ready = false;
            }),
            row_with("here", |a| {
                a.mode = "in-place".to_string();
                a.auth_ready = false;
            }),
        ]);
        with_sign_in(&mut l, sign_in);
        l.accounts
            .iter()
            .map(|r| (r.scanned.name.clone(), r.login))
            .collect::<Vec<_>>()
    };
    let named = |v: Vec<(String, bool)>| {
        v.into_iter()
            .filter(|(_, b)| *b)
            .map(|(n, _)| n)
            .collect::<Vec<_>>()
    };
    assert_eq!(named(list(true)), vec!["need".to_string()]);
    assert!(named(list(false)).is_empty());
    let mut l = test_support::list(vec![row_with("need", |a| a.auth_ready = false)]);
    with_sign_in(&mut l, false);
    assert_eq!(
        l.accounts[0].unselectable.as_ref().map(|w| w.0.clone()),
        Some(copy_core::copy_text("accounts.row.loginElsewhere", &[]))
    );
}

// 换模型写的那份设置共不共用（帧面那一句经 `faces::session_model_face`）那一条挂主 crate：`accounts_query_on_upper_tests.rs`。

/// 金样那一形（一行账号归属的 JSON）⇒ 结构体：`machine_product` 只看活着与走不走中转两格。
fn row_of(line: &str) -> Option<SessionAccount> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    Some(SessionAccount {
        pid: 0,
        sid: v["sessionId"].as_str().map(str::to_string),
        cwd: None,
        config_dir: None,
        account: None,
        alive: v["alive"] == serde_json::Value::Bool(true),
        bare: false,
        via_relay: v["viaRelay"].as_bool(),
    })
}
