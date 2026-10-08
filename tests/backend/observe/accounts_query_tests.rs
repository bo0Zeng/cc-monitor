use super::*;
use crate::agents::claudecode::accounts as cc_accounts;
use std::fs;

fn tmpdir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "ccm-acct-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_manifest(accts: &Path, body: &str) {
    fs::create_dir_all(accts).unwrap();
    fs::write(accts.join("accounts.json"), body).unwrap();
}

fn meta(lines: &[String]) -> serde_json::Value {
    serde_json::from_str(&lines[0]).unwrap()
}

// ---- 1. 正常 manifest ----
#[test]
fn list_accounts_happy_path() {
    let root = tmpdir("happy");
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
    let root = tmpdir("degrade");
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
    let root = tmpdir("unsafe");
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

/// 被 `is_safe_config_dir` 拒掉的 shell 元字符，**测试侧只有这一份**。
///
/// 🔴 `N-F1c`：抽出来是因为它现在要被**两种路径形状**各用一次（POSIX 与 Windows）。
/// 手抄两遍必漂，而漂掉的那一格恰恰是本件最怕的那一格 ——
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
    // 🔴 `N-F1c` 起 `\` **不再**在元字符表里 —— 它是 Windows 的路径分隔符，
    // 拒掉它就等于拒掉每一个 Windows 账号目录。本行是那一格改动的**正面记录**：
    // 从前这里逐字断言 `!is_safe_config_dir("/a\\b")`。
    // 放行它安全的理由不是「没人拼命令」，是**下游那一层自己会拒**
    // （`acct-core` 的 `config_dir_posix_ok` 明确把 `\` 列进拒绝集）——**分层校验**。
    assert!(
        is_safe_config_dir("/a\\b"),
        "`\\` 已经从元字符表里拿掉了（它是 Windows 的分隔符）"
    );
}

/// ★★ `NF1cD2` 正题：**路径检查拆成「性质 / 形式」两半之后，两侧都要成立。**
///
/// # 先证会红
///
/// 拆之前第一条是 `if !p.starts_with('/')` ⇒ 下面「Windows 形状收得进」那一组**全红**；
/// 而它在 `list_accounts` 那一层的表现是**清单恒空**（每个账号都被 `continue` 掉）——
/// 那一格由 `a_windows_shaped_account_survives_the_listing` 单独钉。
///
/// # 分母怎么数的（`NF1cD2` 的 acceptor 逐字要的就是这个）
///
/// 「危险形状照旧拒」那一侧的分母 = 下面四组之和，**现算**、失败时逐个点名：
/// 相对路径 · 裸根 · 上跳（POSIX 与 Windows 各写各的）· shell 元字符（`shell_meta_chars`
/// 那一份，套在 **Windows 形状**上再拒一次 —— 只在 POSIX 形状上验，
/// 「放宽了绝对路径顺手把元字符也放过」这一刀会活着走出去）。
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
    let relative = vec!["relative", "C:Users\\x", "z\\x"];
    let bare_root = vec!["/"];
    let updir = vec![
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
    let root = tmpdir("winshape");
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

// ---- 4. --account-trust ----
#[test]
fn account_trust_paths() {
    let root = tmpdir("trust");
    let accts = root.join("accts");
    let z = accts.join("z");
    fs::create_dir_all(&z).unwrap();
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"accounts":[{{"name":"z","configDir":"{}"}}]}}"#,
            z.display()
        ),
    );
    // 没有 .claude.json → trusted:false / known:false，不是错误
    let out = account_trust(&accts, &z.to_string_lossy(), "/w").unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["trusted"], false);
    assert_eq!(v["known"], false);

    fs::write(
        z.join(".claude.json"),
        r#"{"projects":{"/w":{"hasTrustDialogAccepted":true},"/x":{}},
                "mcpServers":{"gh":{"env":{"GITHUB_TOKEN":"ghp_SUPERSECRET"}}},
                "oauthAccount":{"emailAddress":"z@example.test"}}"#,
    )
    .unwrap();
    let out = account_trust(&accts, &z.to_string_lossy(), "/w").unwrap();
    assert!(!out.contains("ghp_SUPERSECRET"), "绝不能回传文件内容");
    assert!(!out.contains("z@example.test"));
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["trusted"], true);
    assert_eq!(v["known"], true);
    // 已记录但未接受 → known:true, trusted:false
    let v: serde_json::Value =
        serde_json::from_str(&account_trust(&accts, &z.to_string_lossy(), "/x").unwrap()).unwrap();
    assert_eq!(v["known"], true);
    assert_eq!(v["trusted"], false);
    // manifest 之外的 configDir → 拒（防任意文件读）
    let e = account_trust(&accts, "/etc", "/w").unwrap_err();
    assert_eq!(e.0, "unknown_config_dir");
    // 不安全的 configDir → 拒
    let e = account_trust(&accts, "/a'b", "/w").unwrap_err();
    assert_eq!(e.0, "unsafe_config_dir");
    let _ = fs::remove_dir_all(&root);
}

// ---- 6. --session-accounts（procStart 身份对拍是核心）----
#[test]
fn session_accounts_marks_dead_and_bare() {
    let root = tmpdir("sess");
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

    // 当前进程 pid + **正确的 procStart** → 身份对拍通过 → alive:true
    #[cfg(target_os = "linux")]
    {
        let me = std::process::id();
        let real_ticks = proc_starttime(me).expect("能读自己的 starttime");
        fs::write(
            sessions.join(format!("{me}.json")),
            format!(r#"{{"sessionId":"sid-live","cwd":"/w2","procStart":"{real_ticks}"}}"#),
        )
        .unwrap();
        let lines = session_accounts(&claude, &root.join("no-accts"));
        by_sid = sid_map(&lines);
        assert_eq!(by_sid["sid-live"]["alive"], true, "procStart 相符应判活");
        if std::env::var_os("CLAUDE_CONFIG_DIR").is_none() {
            assert_eq!(by_sid["sid-live"]["bare"], true);
            assert_eq!(by_sid["sid-live"]["configDir"], serde_json::Value::Null);
        }
    }
    let _ = fs::remove_dir_all(&root);
}

/// R1：PID 复用防御——pidfile 的 procStart 与当前进程不符 → 判死，绝不误归属账号。
#[cfg(target_os = "linux")]
#[test]
fn session_accounts_rejects_pid_reuse() {
    let root = tmpdir("reuse");
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

fn sid_map(lines: &[String]) -> std::collections::HashMap<String, serde_json::Value> {
    let mut m = std::collections::HashMap::new();
    for l in lines {
        let v: serde_json::Value = serde_json::from_str(l).unwrap();
        m.insert(v["sessionId"].as_str().unwrap().to_string(), v);
    }
    m
}

#[test]
fn session_accounts_without_sessions_dir_is_empty() {
    let root = tmpdir("nosess");
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
    let root = tmpdir("special");
    // symlink → /dev/zero：metadata().len() 报 0 会骗过大小检查,read 无上限会 OOM。
    // read_regular_capped 必须靠 is_file() 挡下（跟随 symlink 后目标是字符设备）。
    let link = root.join("evil.json");
    symlink("/dev/zero", &link).unwrap();
    let r = read_regular_capped(&link, cc_accounts::MAX_CONFIG_BYTES);
    assert!(
        r.is_err(),
        "指向 /dev/zero 的 symlink 必须被拒，而不是读爆内存"
    );
    // 目录也不是常规文件
    assert!(read_regular_capped(&root, 1024).is_err());
    // 正常小文件放行
    let ok = root.join("ok.json");
    fs::write(&ok, "{}").unwrap();
    assert_eq!(read_regular_capped(&ok, 1024).unwrap(), b"{}");
    // 超上限的常规文件被拒
    fs::write(&ok, vec![b'x'; 100]).unwrap();
    assert!(read_regular_capped(&ok, 50).is_err());
    let _ = fs::remove_dir_all(&root);
}

// ---- 10. 建议1：单个坏账号被跳过而非拖垮整份 manifest ----
#[test]
fn one_bad_account_does_not_kill_the_list() {
    let root = tmpdir("badacct");
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

// ---- 12. Z01：账号 0（configDir 键缺席）----

/// 缺 `configDir` = 账号 0。它的 config dir 就是共享库 ⇒ 登录态查那儿；
/// 帧里 `configDir` 出 **null**（下游据此「不注入 CLAUDE_CONFIG_DIR」）。
#[test]
fn account_zero_is_kept_and_probes_shared_store() {
    let root = tmpdir("acct0");
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
    let root = tmpdir("acct0empty");
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
    let root = tmpdir("acct0nostore");
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

/// 〔TQ1 报 · 合并时修〕一个**环境里确实没有** `CLAUDE_CONFIG_DIR` 的活进程，当「裸起会话」用。
/// 从前两条用例拿测试进程自己当会话、见变量有值就 `return` —— 而在 Claude Code 会话里跑测试时
/// 它恒有值 ⇒ 两条在门禁上**从没执行过**。`/proc/self/environ` 是进程起来那一刻的环境，
/// 事后 `remove_var` 改不了它 ⇒ 只能另起一个清掉该变量的子进程。
#[cfg(target_os = "linux")]
fn bare_child() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("30")
        .env_remove("CLAUDE_CONFIG_DIR")
        .spawn()
        .expect("起一个 sleep 子进程当裸起会话")
}

/// 裸起会话（活着但没设 CLAUDE_CONFIG_DIR）现在归属账号 0。
#[cfg(target_os = "linux")]
#[test]
fn bare_session_is_attributed_to_account_zero() {
    let mut child = bare_child();
    let root = tmpdir("acct0sess");
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

/// 🔴🔴 **`K-R21`：「环境这一刻取不到」不许被报成「账号 0 + 裸起」。**
///
/// # 它守的那句假话长什么样
///
/// `proc_env_var` 从前把四件事压成一个 `None`，其中「**这一刻读不出来**」会一路走成
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
/// 那一侧（报「不知道」而不是报「账号 0」）⇒ `K-R21` **刻意不拆它**，登记在 `§7`。
/// **别把本条读成「backend 分得清这两件事」—— 它分不清。**
///
/// # 🔴🔴 `K-R55`（09-11）：补了一个**真实存在的前提缺口**；
/// #    而「它是不是那条 flaky 的根因」—— **判不了，如实写**
///
/// `K-R52` 交回时把本条登记成「一条 flaky，没定位」（16 趟全量 `cargo test` 红 1 次，
/// 随后单跑 12 趟全绿）。`K-R55` 去查了，结果分成**两半**，别把后一半读进前一半：
///
/// ## ① 缺口是真的（现打读数）
///
/// 本条的**丙**（对照活体）前提逐字是「环境读得到、**非空**」，而 `Command::spawn`
/// 只保证 fork 完了、**不保证 exec 完了**；`/proc/<pid>/environ` 在 `execve` 进行
/// **当中**读回 **0 字节**（`platform/proc.rs` 那一支逐字记着它）。
/// 也就是说：**窗口没关就读 ⇒ `c_ok` 为假 ⇒ 本条红在夹具上，不是红在产品上。**
///
/// 发生率**现打**〔09-11，沙箱 `ccmon-devbox:latest`；量具
/// `tests/evidence/K-R55-fixture-shape-probe.py` —— 照本条的夹具形状复刻一遍、
/// 只量丙那一格，不跑 Rust〕：**空载 200 趟 0 次 · 加载（`nproc`×4 条忙循环）200 趟 2 次**。
/// ⇒ 这个窗口**确实开着**，且确实只在有负载时开。
///
/// ## ② 而「它就是那 1/16」—— **没复现出来，所以判不了**
///
/// 把下面那段屏障**整段摘掉**（`K-R55` 刀 D），在同一个沙箱里现打：
/// · 点名单跑 + 满载忙循环，**400 趟红 0 趟**（`tests/evidence/K-R55-flaky-loop.py`）；
/// · 照门禁的真实条件（全量并行的那个测试二进制）**连跑 32 趟，红 0 趟**
///   （`tests/evidence/K-R55-fullsuite-loop.py`）。
/// ⇒ 缺口补上了，**但我没有把那条 flaky 复现出来一次** ⇒ 不许写成「根因找到了」。
///
/// 🔴 **差什么才判得了**：那一趟红的 **panic 原文**（哪一格断言先红）。
/// `K-R52` 的交回与 `audits/K-R52-PM.md` 里都只记了「红过 1 次」，没有留那段输出
/// ⇒ 今天没有任何办法把那一次归到某一格上。**下次登记 flaky 要连原文一起留。**
///
/// ⇒ 处置：屏障照加（它关的是一个**量得到**的缺口，且对产品零影响），
/// 判据本体仍然只跑一次，一个断言都没放宽。**但这不叫「治好了那条 flaky」。**
#[cfg(target_os = "linux")]
#[test]
fn an_unreadable_environ_is_never_reported_as_the_zero_account() {
    let root = tmpdir("envhole");
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
    for _ in 0..2_000 {
        if state_of(zpid) == Some('Z') {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // ── 🔴 `K-R55`：**等 exec 窗口关死** —— 本条那条 flaky 的根因就在这里 ──────
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
    for _ in 0..2_000 {
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
    let root = tmpdir("acct0none");
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

/// 账号 0 **不得**把 `--account-trust` 变成「读共享库 .claude.json」的口子：
/// 它没有 configDir ⇒ 任何路径都不在 manifest 里 ⇒ 拒。
#[test]
fn account_trust_does_not_accept_shared_store_via_account_zero() {
    let root = tmpdir("acct0trust");
    let shared = root.join("claude");
    fs::create_dir_all(&shared).unwrap();
    fs::write(shared.join(".claude.json"), r#"{"projects":{"/w":{}}}"#).unwrap();
    let accts = root.join("accts");
    write_manifest(
        &accts,
        &format!(
            r#"{{"version":1,"sharedStore":{s:?},"accounts":[{{"name":"0","mode":"bare"}}]}}"#,
            s = shared.to_string_lossy()
        ),
    );
    let e = account_trust(&accts, &shared.to_string_lossy(), "/w").unwrap_err();
    assert_eq!(e.0, "unknown_config_dir");
    let _ = fs::remove_dir_all(&root);
}

/// `agents::claudecode::accounts::trust_of_config` 是两个 trust 入口共用的那份实现（避免第二份）。
/// ⚠ `S3` 把实现搬去了适配层，**本测原地留下**：它测的是"消费侧看到的行为"，
/// 而消费侧（`--account-trust` / `--account-trust-zero`）还在本模块。断言一字未改。
/// 账号 0 走 `$HOME/.claude.json`——声明里 `.claude.json` 的原生根就是 home。
#[test]
fn trust_of_claude_json_reads_only_the_three_booleans() {
    let root = tmpdir("acct0tz");
    fs::create_dir_all(&root).unwrap();
    let cj = root.join(".claude.json");

    // 文件不存在 ⇒ known:false，不是错误
    let v: serde_json::Value =
        serde_json::from_str(&cc_accounts::trust_of_config(&cj, "/w").unwrap()).unwrap();
    assert_eq!(v["known"], false);
    assert_eq!(v["trusted"], false);

    fs::write(
        &cj,
        r#"{"projects":{"/w":{"hasTrustDialogAccepted":true}},
                "mcpServers":{"x":{"env":{"API_KEY":"sk-SECRET"}}}}"#,
    )
    .unwrap();
    let out = cc_accounts::trust_of_config(&cj, "/w").unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["trusted"], true);
    assert_eq!(v["known"], true);
    assert!(
        !out.contains("sk-SECRET"),
        "绝不能把 .claude.json 的内容回传"
    );
    let _ = fs::remove_dir_all(&root);
}

/// ★ **main 必须分发本模块认的每一个子命令** —— v3.4.0 的事故守卫。
///
/// 当时 `--account-trust-zero` 在本模块实现完整（`run` 里有它的臂），但 `main.rs` 的
/// match 只列了三个字面量 ⇒ 它落进 `_ => history_query::run` ⇒ 回 `unknown argument`
/// + exit 2。而 monitor 的账号 0 信任预检**真的在发这条命令**，随 v3.4.0 发了出去。
///
/// **为什么既有测试一条都没红**：它们全都直接调 `accounts_query::run`，
/// **绕过了 main 的调度**——被测的那一半是好的，坏的是没人测的那一半。
/// ⇒ 这条守卫**跨文件**比对：本模块 `run` 里出现的每个 `Some("--x")`，
/// 在 `main.rs` 的生产段里都必须出现。
#[test]
fn main_dispatches_every_subcommand_we_handle() {
    let me = include_str!("../../../src/backend/observe/accounts_query.rs");
    let main_raw = include_str!("../../../src/backend/main.rs");
    // 只看生产段 + 剥行注释：两个文件的散文里都会提到这些字面量，
    // 不剥的话「main 的注释里写了它」也会让守卫变绿——那正是安慰剂。
    // U-1：剥法收敛到 `guard_support`。旧的内联版锚 `mod tests`，而 main.rs 的测试模块
    // 叫 `mod stream_flag_tests` ⇒ 匹配不上 ⇒ **这条守卫一直在拿测试段里那份副本对账**。
    let strip = crate::guard_support::production_code;
    let mine = strip(me);
    let main_prod = strip(main_raw);
    assert!(
        main_prod.len() > 3_000 && main_prod.len() < main_raw.len(),
        "剥完 main 生产段只剩 {} 字节（原文 {}）——剥法坏了",
        main_prod.len(),
        main_raw.len()
    );
    // ★ 上面那条 `len` 自检**检不出「测试段没剥掉」**（光靠剥注释就满足）。
    // 真正的判据是这条：剥完不许再有测试属性。
    crate::guard_support::assert_no_test_code("accounts_query/main.rs", &main_prod);
    crate::guard_support::assert_no_test_code("accounts_query/self", &mine);

    // 抠出本模块 `run` 分发的子命令：形如 `Some("--x") =>`。
    let mut subs: Vec<&str> = Vec::new();
    let needle = format!("{}(\"--", "Some");
    for (i, _) in mine.match_indices(needle.as_str()) {
        let rest = &mine[i + needle.len()..];
        if let Some(end) = rest.find('"') {
            let name = &rest[..end];
            if !subs.contains(&name) {
                subs.push(name);
            }
        }
    }
    // 反向自检：一个都没抠到 = 抠法坏了，而不是「本模块没有子命令」。
    assert_eq!(
        subs.len(),
        4,
        "从本模块抠到 {} 个子命令（真实应为 4）：{subs:?}——加/删子命令时来改这个数",
        subs.len()
    );

    for name in &subs {
        let lit = format!("{}(\"--{name}\")", "Some");
        assert!(
            main_prod.contains(lit.as_str()),
            "`--{name}` 在本模块有完整实现，但 `main.rs` 的调度里找不到 `{lit}`。\n\
                 它会落进 `_` 臂走历史查询 ⇒ 回 `unknown argument` + exit 2，\n\
                 而调用方（monitor）拿到的是一个看起来像「backend 太旧」的失败。\n\
                 **v3.4.0 就是这么漏出去的。** 加子命令时两处都要加。"
        );
    }
}

/// `--account-trust-zero` 只收 cwd，路径写死在代码里 ⇒ 它连「任意文件读」的面都没有。
/// 钉住入口形状（而不是去改 $HOME 跑真的，那在并行测试里是竞态）。
#[test]
fn account_trust_zero_takes_no_path_argument() {
    let me = include_str!("../../../src/backend/observe/accounts_query.rs");
    assert!(
        me.contains("fn account_trust_zero(cwd: &str)"),
        "账号 0 的 trust 入口一旦收了路径参数，就重新开出了任意文件读的面"
    );
    assert!(
        me.contains("trust_in(&home, cwd)"),
        "账号 0 的配置文件必须来自 $HOME（声明里它的原生根是 home）：交给账号库面 `trust_in` 的根就是 `home`。"
    );
    assert!(me.len() > 1000, "include_str! 没读到源码，上面的断言是空转");
}

// ---- K-A1：鉴权方式这一维（生产者①） ----

/// ★ **跨生产者对拍，backend 这一半。**
///
/// 喂的是 `acct_core::auth_kind_parity_manifest`（**两个 crate 共用的那一份**），
/// 断的是 `acct_core::AUTH_KIND_PARITY_CASES` 里手写的金样。
/// `local_accounts.rs` 那半断的是**同一张表**，所以「两个生产者各填一个不同的默认值」
/// 会让其中一半当场红 —— 这正是 `KAY1` 那条 acceptor 点名的失效模式。
///
/// ⚠ 它**只覆盖 `authKind` / `authReady` 这一维**（`KA6c`）：其余 6 个字段今天仍是
/// 两份实现各写一遍，这条对拍看不见它们漂。
#[test]
fn auth_kind_parity_backend_side() {
    let root = tmpdir("authkind-parity");
    for c in &acct_core::AUTH_KIND_PARITY_CASES {
        let d = root.join(c.name);
        fs::create_dir_all(&d).unwrap();
        if c.credentials_present {
            fs::write(d.join(CREDENTIALS_NAME), "{\"tok\":\"SECRET-TOKEN\"}").unwrap();
        }
    }
    fs::create_dir_all(root.join("shared")).unwrap();
    write_manifest(
        &root,
        &acct_core::auth_kind_parity_manifest(&root.to_string_lossy()),
    );
    let lines = list_accounts(&root);
    // 首行是 meta，之后一行一个账号 —— 行数自检，防「少了几行也照样逐格绿」。
    assert_eq!(
        lines.len(),
        acct_core::AUTH_KIND_PARITY_CASES.len() + 1,
        "行数对不上，逐格断言会漏掉没出来的那几个账号：{lines:?}"
    );
    for (i, c) in acct_core::AUTH_KIND_PARITY_CASES.iter().enumerate() {
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
/// ⚠ 射程如实写：它**只管本文件**（另一个生产者 —— monitor 那份本机参照实现 —— 已删，
/// 这一维的生产者今天只剩本文件这一个），
/// 而且是**字面量扫描** —— 把两个 helper 重新 `use` 成别名就绕得过去。
/// 真正的地板不是它，是 `acct-core` 里只有一份实现。
#[test]
fn the_auth_dimension_has_exactly_one_computation_path() {
    let me = include_str!("../../../src/backend/observe/accounts_query.rs");
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
        prod.contains("fn list_accounts(accts_dir: &Path)") && prod.len() > 8_000,
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
        prod.matches("\"authReady\"").count(),
        1,
        "`authReady` 这个键在生产段里被写了不止一处"
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
/// ⚠ 那张登记表住 `src/frontend/shell/src/structural_scan.rs`，**不在本拍写区** ——
/// 而它给的第一条出路本来就不需要动登记表。
fn production_text() -> String {
    let me = include_str!("../../../src/backend/observe/accounts_query.rs");
    assert!(
        me.len() > 20_000,
        "include_str! 没读到源码，本条在空转（实得 {} 字节）",
        me.len()
    );
    let prod = crate::guard_support::production_code(me);
    crate::guard_support::assert_no_test_code("accounts_query.rs", &prod);
    assert!(
        prod.contains("fn session_accounts(agent_home: &Path")
            && prod.contains("struct SessionRow {")
            && prod.len() > 8_000,
        "剥过头 / 锚点挪了（实得 {} 字节）—— 用它的那几条会零命中地绿",
        prod.len()
    );
    prod
}

/// ★★ **本文件读环境这件事的射程不许悄悄变大**〔本文件头注那条「两个写死的键」的判据〕。
///
/// 🔴 **主语就是「本文件」，不是「backend 全体」**〔`K-R21` 09-03 收窄的措辞〕：
/// 本条的分母是 `include_str!("../../../src/backend/observe/accounts_query.rs")` 的生产段 —— **一个文件**，
/// 与测试名里那个 `this_module` 逐字对齐。
/// ⚠ backend 里**还有另外两处**在读 `/proc/<pid>/environ`，都住 `control/identity_tag.rs`：
/// `TMUX_PANE`（身份广播那一面）与 `CCM_RBIND_TOKEN`（启动期令牌）。
/// 〔计数订正：09-03 现打是「第三处」、生产段共 3 个调用方；步 2 加了第四个，**今天是 4**。
///  这两个数都不在任何断言里，它们是散文 —— 写在这儿是为了不让下一个人照着一个过期的数去数。〕
/// 它们**都不在本条视野里**，
/// 而这句话原来读起来像全仓 —— 那正是本仓登记过的「量具的作用域对不上事实」那一形。
/// ⇒ `K-R21` PM 裁定：**不为此装第二把尺子**（那个人群今天产出过 0 条假话，
/// 为它付一把新尺子的固定成本不划算 —— 同 `K-R19` 裁「闸 G 不装」的口径），
/// **改的是这句话的主语**。别把这一格读成「全后端只读两个键」。
///
/// 🔴 **那条 PM 裁定的前提已经变了，尺子装了** ——
/// 变了两处：① `identity_tag.rs` 从读**一个**变量变成读**两个**；
/// ② 新那个变量的值是**敏感数据**。
/// ⇒ 它自己那一把住
/// `control::identity_tag::tests::the_env_keys_this_file_reads_are_exactly_two_named_constants`。
/// **本条的分母一格没动**：两把尺子各量一个文件，不是同一条铁律的两个住址。
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
    assert_eq!(
        prod.matches("proc_env_var(pid, env_keys.config_dir)")
            .count(),
        1,
        "抠 `CLAUDE_CONFIG_DIR` 那一处不见了 / 变形了"
    );
    assert_eq!(
        prod.matches("proc_env_var(pid, env_keys.base_url)").count(),
        1,
        "抠 `ANTHROPIC_BASE_URL` 那一处不见了 / 变形了（`viaRelay` 的读侧）"
    );
}

/// ★★ **「盘上写着的」↔「我们真发的」对拍**〔`KP5FD4` 那句「判据要自己长出来」〕。
///
/// # 它为什么非有不可
///
/// `K-P5f` 摸底现打过：往 `--session-accounts` 加字段这条路**撞 0 道机检**
/// （`protocol_doc_guard` 那两条一条够不着它 —— 出参是本文件里一个就地
/// `serde_json::json!`，不是 `wire.rs` 里的类型；另一条数的是**子命令名**，
/// 而 `--session-accounts` 早在表里）。⇒ 文档那一行**只靠人记得改**。
/// 而「没有闸看着的文档事实」正是本工作区反复判过的假绿源
/// （「写着有、其实没有」同族）。**这条就是那道闸。**
///
/// # 三格
///
/// | 格 | 断的是什么 | 翻掉它的形状 |
/// |---|---|---|
/// | ① | `CLI_ONLY_DOCS` 那一行的出参字段表逐字等于生产段真发的那几个键（**顺序也算**） | 加一个字段不改说明 / 改了名字 |
/// | ② | 本文件头注把**两个**环境变量键都点了名 | 加第二个键、却留着「只抠一个键」那句假话 |
///
/// # ⚠ 它买不到什么
///
/// 只买「**那几个名字都在场**」。文档那一行**说得对不对**（比如 `bare` 的语义
/// 解释）它一个字都判不了 —— 那是评审的活。
#[test]
fn the_protocol_doc_row_for_session_accounts_matches_what_we_emit() {
    // 协议参考的 CLI 那一节从 `CLI_ONLY_DOCS` 生成：那一行就是这里的说明。
    let row = crate::stream::inbound::CLI_ONLY_DOCS
        .iter()
        .find(|(f, _, _)| *f == "session-accounts")
        .map(|(_, _, what)| *what)
        .expect("`CLI_ONLY_DOCS` 里没有 `--session-accounts`");

    // ── ① 出参字段表：从生产段把 `json!` 的键抠出来，与文档里那个花括号表对拍 ──
    let prod = production_text();
    let start = prod
        .find("fn session_accounts(agent_home")
        .expect("找不到 `session_accounts` —— 抽取器坏了");
    let body = &prod[start..];
    let j = body
        .find("serde_json::json!({")
        .expect("`session_accounts` 里找不到出参 `json!` —— 抽取器坏了");
    let mut keys: Vec<String> = Vec::new();
    for l in body[j..].lines().skip(1) {
        let t = l.trim();
        if t == "})" {
            break;
        }
        if let Some(r) = t.strip_prefix('"') {
            if let Some(i) = r.find("\":") {
                keys.push(r[..i].to_string());
            }
        }
    }
    assert_eq!(
        keys.len(),
        8,
        "从出参 `json!` 只抠到 {} 个键（应为 8）—— 抽取器坏了，下面那格会零命中地绿：{keys:?}",
        keys.len()
    );
    let table = format!("{{{}}}", keys.join(", "));
    assert!(
        row.contains(&table),
        "\n★★ `CLI_ONLY_DOCS` 的 `--session-accounts` 那一行里找不到字段表 {table:?}。\n\
             出参加了字段 / 改了名 / 换了顺序，而文档没跟着改 —— **盘上留了一句假话**，\n\
             而这条路撞 0 道机检，除了本条没有任何东西会说。\n\
             那一行现在写的是：\n  {row}"
    );

    // ── ② 「只抠几个键」那句诚实边界：本文件头注得把两个键点到名 ──
    let header: String = include_str!("../../../src/backend/observe/accounts_query.rs")
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
// 帧面出成品：`accounts-list`（并上这台机器自己那份 apikey 表）· `accounts-trust`
// ============================================================================
//
// 要求：「一次性请求那半收口成 `call` —— 按能力分批」· 「一个判定只有一个家」·
// 「中转 ＋ 上游选择住本机常驻后端进程」⇒「账号域读自己那台的 apikey 表、
// 两条规则搬进 `acct-core`、agent 随请求带」。夹具只造结构（目录名 ＋ 占位 manifest），不采真账号数据。

/// 夹具：账号库（一个账号 0 ＋ 两个隔离号，`acct-a` 有订阅凭据、`acct-b` 没有）。回 `(root, accts)`。
fn c4c_fixture(tag: &str, with_zero: bool) -> (PathBuf, PathBuf) {
    let root = tmpdir(tag);
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

/// ★★ 成品里的账号 == CLI 那一臂逐行打印的账号（同一夹具、两个出口；表里没行时逐条相等），
/// meta == CLI 首行去掉 `kind` / `accountZeroAware`；成品顶层键集合恒等 `{meta, accounts, notice}`。
#[test]
fn the_list_product_carries_exactly_what_the_cli_arm_prints() {
    let (root, accts) = c4c_fixture("c4c-same", true);
    let cli = list_accounts(&accts);
    let product = list_product_at(&accts, &[], "claude-code", "claude-code");
    let keys: Vec<&String> = product.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["accounts", "meta", "notice"], "成品顶层键集合变了");
    let mut cli_meta = meta(&cli);
    let o = cli_meta.as_object_mut().unwrap();
    assert_eq!(o.remove("kind"), Some(serde_json::json!("accounts-meta")));
    assert_eq!(o.remove("accountZeroAware"), Some(serde_json::json!(true)));
    // 成品比 CLI 多出的只有帧面那三格（家目录 · 每号 key 掩码与端点），其余逐格同一份扫描。
    let mut product = product;
    assert_eq!(
        product["meta"].as_object_mut().unwrap().remove("home"),
        Some(serde_json::Value::Null)
    );
    for a in product["accounts"].as_array_mut().unwrap() {
        let o = a.as_object_mut().unwrap();
        assert_eq!(o.remove("keyMasked"), Some(serde_json::Value::Null));
        assert_eq!(o.remove("baseUrl"), Some(serde_json::Value::Null));
    }
    assert_eq!(product["meta"], cli_meta, "成品 meta 与 CLI 首行不是同一份");
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
    let merged = list_product_at(
        &accts,
        &["acct-b".to_string()],
        "claude-code",
        "claude-code",
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
        let b = row_b(&list_product_at(&accts, &rows, agent, "claude-code"));
        assert_eq!(b["authKind"], acct_core::AUTH_KIND_SUBSCRIPTION, "{why}");
        assert_eq!(b["authReady"], false, "{why}");
    }
    let _ = fs::remove_dir_all(&root);
}

/// ★ 缺账号 0 ⇒ `notice` 是一句话（不带「远端」—— 本机远端同一条路）；没启用 ⇒ `null`。
#[test]
fn the_list_product_says_when_account_zero_is_missing() {
    let (root, accts) = c4c_fixture("c4c-nozero", false);
    let v = list_product_at(&accts, &[], "claude-code", "claude-code");
    let n = v["notice"].as_str().expect("缺账号 0 却没出那一句");
    assert!(
        n.contains(copy_core::copy_static!(
            "beAccountsQuery.listProductAt.noDefault"
        )) && !n.contains(copy_core::copy_static!("rsConfigSurface.host.remote")),
        "{n}"
    );
    let off = list_product_at(&root.join("nope"), &[], "claude-code", "claude-code");
    assert_eq!(off["meta"]["enabled"], false);
    assert!(off["notice"].is_null(), "没启用谈不上缺账号 0");
    let _ = fs::remove_dir_all(&root);
}

/// ★ 信任预检成品：`{trusted, known}` 两格与 CLI 那一臂同一个函数的答案一致；不在 manifest 的目录照旧拒。
#[test]
fn the_trust_product_answers_through_the_same_function_as_the_cli_arm() {
    let (root, accts) = c4c_fixture("c4c-trust", true);
    let a = accts.join("acct-a");
    fs::write(
        a.join(".claude.json"),
        r#"{"projects":{"/w/p":{"hasTrustDialogAccepted":true},"/w/q":{}}}"#,
    )
    .unwrap();
    let dir = a.to_string_lossy().to_string();
    assert_eq!(
        trust_product_at(&accts, Some(&dir), "/w/p").unwrap(),
        serde_json::json!({"trusted": true, "known": true})
    );
    assert_eq!(
        trust_product_at(&accts, Some(&dir), "/w/q").unwrap(),
        serde_json::json!({"trusted": false, "known": true})
    );
    assert_eq!(
        trust_product_at(&accts, Some(&dir), "/w/r").unwrap(),
        serde_json::json!({"trusted": false, "known": false})
    );
    // 与 CLI 那一臂逐格对拍（同一个函数，两个出口）。
    let cli: serde_json::Value =
        serde_json::from_str(&account_trust(&accts, &dir, "/w/p").unwrap()).unwrap();
    assert_eq!(cli["trusted"], true);
    assert_eq!(cli["known"], true);
    let outside = root.join("elsewhere").to_string_lossy().to_string();
    assert_eq!(
        trust_product_at(&accts, Some(&outside), "/w/p")
            .unwrap_err()
            .0,
        "unknown_config_dir",
        "不在 manifest 里的目录不许读（任意文件读原语）"
    );
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
    assert_eq!(row("b")["baseUrl"], "https://api.example.com");
    assert!(
        row("a")["keyMasked"].is_null() && row("a")["baseUrl"].is_null(),
        "订阅号不带"
    );
    assert_eq!(
        v["meta"]["home"],
        root.join("home").to_string_lossy().as_ref()
    );
    // 表里没有它那一行 ⇒ 两格 null（照样是 api-key 号）。
    let none = list_product_with(
        &accts,
        None,
        &["acct-b".to_string()],
        &[],
        "claude-code",
        "claude-code",
    );
    let b = none["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "b")
        .cloned()
        .unwrap();
    assert!(b["keyMasked"].is_null() && b["baseUrl"].is_null() && none["meta"]["home"].is_null());
    let _ = fs::remove_dir_all(&root);
}

/// ★★ **跨语言金样**：两条成品对同一份夹具 == `tests/__fixtures__/accounts.golden.json`（夹具根替换成 `<root>`）。
/// 那份金样的另一个读者是 TS 解码器（`tests/frontend/ui/accounts-decode.vitest.ts`）⇒ 两侧异源：后端改一个键名本条红，
/// TS 解码器改一个键名那边红。金样手写落盘（本条红时印出现打的成品，人读过再改）。
#[test]
fn the_account_products_match_the_cross_language_golden() {
    let (root, accts) = c4c_fixture("c4c-golden", true);
    fs::write(
        accts.join("acct-a").join(".claude.json"),
        r#"{"projects":{"/w/p":{"hasTrustDialogAccepted":true}}}"#,
    )
    .unwrap();
    let dir = accts.join("acct-a").to_string_lossy().to_string();
    let got = serde_json::json!({
        "accounts-list": list_product_with(
            &accts,
            Some(&root.join("home")),
            &["acct-b".to_string()],
            &golden_key_facts(&root),
            "claude-code",
            "claude-code",
        ),
        "accounts-trust": trust_product_at(&accts, Some(&dir), "/w/p").unwrap(),
    });
    let got: serde_json::Value = serde_json::from_str(
        &got.to_string()
            .replace(&root.to_string_lossy().to_string(), "<root>"),
    )
    .unwrap();
    let want: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/accounts.golden.json"))
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
// 同一份账号 manifest，**两个读者**读出同一张表 —— 带不带 UTF-8 BOM 都一样
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 件 F，逐字「账号库 manifest 带 UTF-8 BOM ⇒ 整块当空表」
// （那次真机读数：PS 5.1 `-Encoding UTF8` 与记事本默认写 BOM）。
//
// 后端读这份文件的有两处：`control/ccm/plan.rs::AccountTable::load`（`ccm --account` 那一条，09-21 已修）
// 与本文件的 `load_manifest`（`--list-accounts` ⇒ 账号页）。前者修了、后者没修 ⇒ 同一台 Windows 上
// `ccm --account work` 起得来，账号页却说「没启用多账号」。
// ⇒ 判据不钉「某一个读者剥了 BOM」，钉**两个读者读出的号两向相等**：以后谁再长出第三种读法、
//   或其中一个改了解析口径，这里当场红。两侧异源：两份各自的解析代码，同一份盘上字节。
#[test]
fn both_readers_of_the_manifest_see_the_same_accounts() {
    let body = r#"{"version":1,"accounts":[{"name":"work","configDir":"/w"},{"name":"play","configDir":"/p","isDefault":true}]}"#;
    for (tag, bytes) in [
        ("plain", body.to_string()),
        ("bom", format!("\u{FEFF}{body}")),
    ] {
        let root = tmpdir(&format!("two-readers-{tag}"));
        let accts = root.join("accts");
        write_manifest(&accts, &bytes);
        // 夹具先自证：带 BOM 那一份真的以 EF BB BF 开头（否则这一格在量一个没有 BOM 的文件）。
        let raw = fs::read(accts.join("accounts.json")).unwrap();
        assert_eq!(
            raw.starts_with(&[0xEF, 0xBB, 0xBF]),
            tag == "bom",
            "夹具不对：{tag}"
        );

        let mut ours: Vec<String> = load_manifest(&accts)
            .unwrap_or_else(|e| panic!("`--list-accounts` 那个读者读不动（{tag}）：{e}"))
            .accounts
            .into_iter()
            .map(|a| a.name)
            .collect();
        let path = accts.join("accounts.json");
        let mut ccm: Vec<String> =
            crate::control::ccm::plan::AccountTable::load(path.to_str().unwrap())
                .accounts
                .into_iter()
                .map(|a| a.name)
                .collect();
        ours.sort();
        ccm.sort();
        assert_eq!(
            ours, ccm,
            "同一份 manifest（{tag}），账号页那个读者与 `ccm --account` 那个读者读出的号不一样"
        );
        assert_eq!(
            ours,
            vec!["play".to_string(), "work".to_string()],
            "（{tag}）两边都读漏了"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

// ════════════════════════════════════════════════════════════════════════════════════════
// monitor 那份本机 manifest 参照实现删了 —— 挂在它身上的三个锚点改指这里（现存实现）
// ════════════════════════════════════════════════════════════════════════════════════════
//
// 要求：「`local_accounts.rs` 的 `list_from_dir`〔散文墓碑〕
// （零生产调用方的本机参照实现）删，**挂着的判据锚点改指现存实现**」。下面三条的断言逐字搬自 monitor
// `tests/frontend/shell/local_accounts_tests.rs` 那三条（U7-4 / audit-0805），被测对象从那份参照实现换成后端这份真在答账号清单的。

/// ★ 读上限的三种失败（不存在 / 不是普通文件 / 过大）**两两分得开**，过大的那句带实际字节数；
/// 正好等于上限**放行**（`>` 不是 `>=`），差一个字节就拒。
#[test]
fn read_regular_capped_keeps_its_three_failures_distinguishable() {
    let root = tmpdir("c4d-capped");
    let missing = root.join("nope.json");
    let dir = root.join("adir");
    fs::create_dir_all(&dir).unwrap();
    let big = root.join("big.json");
    fs::write(&big, vec![b'x'; 10]).unwrap();
    let e_missing = read_regular_capped(&missing, 1024).expect_err("不存在的文件必须是 Err");
    let e_dir = read_regular_capped(&dir, 1024).expect_err("目录必须是 Err");
    let e_big = read_regular_capped(&big, 4).expect_err("超限必须是 Err");
    assert!(e_big.contains("10"), "过大那句没带实际字节数：{e_big}");
    assert!(
        e_missing != e_dir && e_dir != e_big && e_missing != e_big,
        "三种失败给了相同的理由串：不存在={e_missing} / 目录={e_dir} / 过大={e_big}"
    );
    let exact = root.join("exact.json");
    fs::write(&exact, vec![b'y'; 8]).unwrap();
    assert_eq!(
        read_regular_capped(&exact, 8).expect("正好等于上限应当放行"),
        vec![b'y'; 8]
    );
    assert!(
        read_regular_capped(&exact, 7).is_err(),
        "超出一个字节没被拒 —— 上限那一格滑了"
    );
    let _ = fs::remove_dir_all(&root);
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
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/observe/accounts_query.rs"
    ));
    let at = guard_core::find_pinned(&prod, "fn resolve_accts_dir() -> PathBuf {")
        .unwrap_or_else(|e| panic!("缺省解析那一处找不到（恰好一处、不收参数）：{e}"));
    let body_end = prod[at + 1..]
        .find("\nfn ")
        .map_or(prod.len(), |k| at + 1 + k);
    guard_core::find_pinned(&prod[at..body_end], "h.join(ACCOUNTS_DIR_REL)")
        .unwrap_or_else(|e| panic!("缺省解析不再经契约常量拼家目录下那一层：{e}"));
}

/// 守的要求：用户「即后端去.cc-monitor读数据」——账号库的位置只跟着家走，旧位置不认、另指位置的环境变量删掉。
/// ① 产品（`src/` 全树的文本文件 ＋ 两份 README）里旧账号库目录名与那个环境变量名**零处**；
/// ② 那个变量名在测试脚本、门禁、CI 里也零处（全仓只剩 CHANGELOG 旧版本段与 `tests/evidence/` 的冻结读数）。
/// 正控：同一个扫描器在产品里找新位置那一段的唯一住址（宏名），命中的文件集合恰好是契约 crate 与足迹两份；
/// 脚本那一族的人群里真有门禁与 ccm 那套 e2e；合成串里两个针都认得出。
#[test]
fn nothing_in_the_product_still_points_at_the_old_account_library() {
    let root = crate::guard_support::repo_root();
    // 针运行时拼：本文件自己不进人群也不含针。
    let old_dir = format!(".claude-{}", "accts");
    let old_env = format!("CCM_ACCTS_{}", "MANIFEST");
    // 本文件明写在排除名单里（针虽是运行时拼的，扫描口径仍走那一份带「摘不到就 panic」的遍历）。
    const SELF: &str = "tests/backend/observe/accounts_query_tests.rs";
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

/// ★**本机判活源头**（历史跨机 join 用）：pidfile 里的会话 id ＋ 那个进程还在（同 watcher 那一道平台原语）。
///
/// 要求：「本机后端 … 并上注解、出成品」—— 本机那一支的
/// 「活没活」从 monitor 的 `SessionMap` 换到这台后端自己答，答错就是历史列表上一个活会话不亮 / 一个死会话亮着。
/// 夹具：本测试进程自己的 pid（活）· 一个超出 pid 上限的 pid（死）· 没有 `sessionId` 的（不算）· 文件名不是 pid 的（不看）。
#[cfg(unix)]
#[test]
fn live_session_ids_are_the_pidfiles_whose_process_is_still_there() {
    let home = tmpdir("c4d-live");
    let sessions = home.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    let me = std::process::id();
    fs::write(
        sessions.join(format!("{me}.json")),
        r#"{"sessionId":"live-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("4194304.json"),
        r#"{"sessionId":"dead-sid","cwd":"/w"}"#,
    )
    .unwrap();
    fs::write(
        sessions.join("notapid.json"),
        r#"{"sessionId":"ignored","cwd":"/w"}"#,
    )
    .unwrap();
    let got = live_session_ids(&home);
    assert_eq!(
        got,
        ["live-sid".to_string()].into_iter().collect(),
        "活着的那一个要在、死了的那一个不许在"
    );
    // 没有 sessionId 的活进程不算（它说不出是哪个会话）。
    fs::write(sessions.join(format!("{me}.json")), r#"{"cwd":"/w"}"#).unwrap();
    assert!(live_session_ids(&home).is_empty());
    let _ = fs::remove_dir_all(&home);
}

/// **`viaRelay` 答的是「这条活会话的进程环境里，上游地址是不是本机中转那一形」**：
/// 带钥匙段的回环中转地址 ⇒ `true`；别的地址（直连 / 没带钥匙的旧形状）⇒ `false`；没设 ⇒ `false`；进程已死 ⇒ `null`。
/// 而且**值本身不出参**（它带着中转钥匙）。守的要求：「`session_accounts` 多读 `ANTHROPIC_BASE_URL` ·
/// `accounts-sessions` 每行加 `viaRelay: true|false|null`」；`INVARIANTS §48.1a`「钥匙 …… `relay-status` 应答里也没有它」同族。
/// 形状：真子进程（`sleep`，各带一份环境）＋ 真 pidfile（`procStart` 对得上）⇒ 逐条相等；输出全文零命中钥匙（正控：钥匙在子进程环境里）。
#[test]
#[cfg(target_os = "linux")]
fn hx1_via_relay_says_which_live_sessions_point_at_the_local_relay_and_never_leaks_the_url() {
    let root = tmpdir("viarelay");
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
        // 等 exec 完（环境在 exec 之后才是新程序的）：/proc/<pid>/cmdline 变成 sleep 那一刻。
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read(format!("/proc/{pid}/cmdline"))
            .ok()
            .and_then(|c| c.get(..5).map(<[u8]>::to_vec))
            .as_deref()
            != Some(b"sleep".as_slice())
        {
            assert!(std::time::Instant::now() < until, "sleep 起不来");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
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
    let all = lines.join("\n");
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
        "../../__fixtures__/machine-interrupts.golden.json"
    ))
    .expect("金样不是 JSON");
    let cases = golden["cases"].as_array().expect("金样没有 cases");
    assert_eq!(cases.len(), 3, "金样的形数变了 —— 两侧一起改");
    for c in cases {
        let lines: Vec<String> = c["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        let forwards = c["forwards"].as_u64().unwrap() as u32;
        assert_eq!(
            machine_product(&lines, forwards),
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
    let product = list_product_at(&accts, &[], "claude-code", "claude-code");
    assert_eq!(product["meta"]["nextDefault"], serde_json::json!("b"));
    let empty = tmpdir("next-default-none");
    let none = list_product_at(&empty, &[], "claude-code", "claude-code");
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
        r#"{"alive":true,"viaRelay":true}"#.to_string(),
        r#"{"alive":true,"viaRelay":false}"#.to_string(),
    ];
    let zero =
        serde_json::json!({"relayedSessions":0,"relayedMaybe":0,"liveStreams":0,"forwards":0});
    assert_eq!(app_exit_product(false, &lines, 3), zero);
    assert_eq!(
        app_exit_product(true, &lines, 3),
        serde_json::json!({"relayedSessions":1,"relayedMaybe":0,"liveStreams":2,"forwards":3})
    );
}
