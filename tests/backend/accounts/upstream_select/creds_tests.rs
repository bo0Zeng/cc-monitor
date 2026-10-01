use super::*;

/// 判据用薄封装：这份文件**恰好读出一条**账号时，取那一条的 key。
///
/// ⚠ 它**故意在条数 != 1 时返回 `None`** —— 下面那几条判据的夹具都是单条文件，
/// 哪天夹具变成多条而判据没跟着改，这里会把它变成一次**红**，
/// 而不是悄悄拿第一条顶上（拿第一条顶上正是本件在治的那一形）。
fn single_key(loaded: &Loaded) -> Option<&creds_core::SecretKey> {
    if loaded.accounts.len() != 1 {
        return None;
    }
    loaded.accounts[0].key.as_ref()
}

/// `announce` 的判据用薄封装：没有被拒的行，行数取读出来的条数。
fn announce_all(loaded: &Loaded, out: &mut dyn std::io::Write) -> usize {
    announce(loaded, loaded.accounts.len(), &[], &[], out)
}

/// 一个只属于本判据的临时目录。**名字中性**（不含被断言的字面），
/// 免得诊断把路径原样印进输出、让「输出里含某句话」靠路径恒真
/// （`brief` 12 逐字点名的那一形）。
fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "ccm-rc-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

/// 这个临时「家」下那份文件的默认落点（数据目录 `~/.cc-monitor` 根上；期望手写，不借生产那一份）。
fn default_file(home: &Path) -> PathBuf {
    home.join(".cc-monitor").join(store::FILE_NAME)
}

/// 取值器：只答 `HOME`（与 `ENV_CREDENTIALS` 那一格，给了才答）。
fn env_with(home: &Path, creds: Option<&str>) -> impl Fn(&str) -> Option<String> {
    let home = home.display().to_string();
    let creds = creds.map(str::to_string);
    move |k| match k {
        "HOME" => Some(home.clone()),
        ENV_CREDENTIALS => creds.clone(),
        _ => None,
    }
}

/// ★★ **`KS9②` 的正主**：只往盘上放一份文件，**一次界面都不开**，上游选择就拿到了 key。
///
/// 走的是**生产段那条真实的路**（`resolve_path` → `load`），
/// 文件是用**裸 `fs::write` 写的**（不是本仓的写入器）—— 那正是「人拿编辑器写了一份」。
#[test]
fn upstream_selection_loads_the_key_from_a_hand_written_file_alone() {
    let home = tmpdir("only-file");
    let p = default_file(&home);
    std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
    // ← 这一行就是「人手写了一份 JSON 放进去」。没有任何界面、没有任何 IPC。
    std::fs::write(
        &p,
        b"{\n  \"_note\": \"my own note\",\n  \"api_key\": \"sk-ant-FROM-A-BARE-FILE\"\n}\n",
    )
    .expect("写夹具");

    let resolved = resolve_path(&env_with(&home, None)).expect("有家目录却推不出");
    assert_eq!(resolved, p, "路径解析没落在契约那条路上");
    let loaded = load(&resolved);
    assert!(loaded.problem.is_none(), "不该有问题：{:?}", loaded.problem);
    assert_eq!(
        single_key(&loaded)
            .expect("应当拿到 key")
            .expose_for_auth_header(),
        "sk-ant-FROM-A-BARE-FILE"
    );
    // 非空对照：同一条路，文件里没 key 时拿不到（不是恒返回一个值）。
    std::fs::write(&p, b"{\"_note\":\"nothing here\"}").expect("改夹具");
    assert!(load(&resolved).accounts.is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn env_overrides_the_default_location() {
    let home = tmpdir("env-override");
    let elsewhere = home.join("somewhere-else.json");
    std::fs::write(&elsewhere, b"{\"api_key\":\"sk-ant-ELSEWHERE\"}").expect("写夹具");
    let got =
        resolve_path(&env_with(&home, Some(&elsewhere.display().to_string()))).expect("显式给了");
    assert_eq!(got, elsewhere);
    assert_eq!(
        single_key(&load(&got))
            .expect("应当拿到 key")
            .expose_for_auth_header(),
        "sk-ant-ELSEWHERE"
    );
    // 非空对照：空串的覆盖**不算覆盖**，回默认路径。
    let dflt = resolve_path(&env_with(&home, Some("   ")));
    assert_eq!(dflt, Ok(default_file(&home)));
    // 默认臂按用户家推、不认 agent 家：给了 `CLAUDE_CONFIG_DIR` 也落在同一份。
    let with_agent_home = |k: &str| match k {
        "CLAUDE_CONFIG_DIR" => Some("/elsewhere/.claude-acct".to_string()),
        _ => env_with(&home, None)(k),
    };
    assert_eq!(resolve_path(&with_agent_home), Ok(default_file(&home)));
    // 数据目录与 monitor 同一条规矩：`CCM_DATA_DIR`（绝对）⇒ 它根上那一份（期望手写）；
    //   设了却是相对路径 ⇒ 出声（`Err`），不退回家目录下那一份；显式给的 `CCM_APIKEY_CREDENTIALS` 仍优先。
    let with_data_dir = |dd: &'static str, creds: Option<&'static str>| {
        let base = env_with(&home, creds);
        move |k: &str| match k {
            "CCM_DATA_DIR" => Some(dd.to_string()),
            _ => base(k),
        }
    };
    assert_eq!(
        resolve_path(&with_data_dir("/iso", None)),
        Ok(PathBuf::from("/iso/apikey-credentials.json"))
    );
    let refused = resolve_path(&with_data_dir("iso", None)).expect_err("相对路径被收下了");
    assert!(
        refused.contains("CCM_DATA_DIR"),
        "那句话没点名是哪一格：{refused}"
    );
    assert_eq!(
        resolve_path(&with_data_dir("iso", Some("/x/c.json"))),
        Ok(PathBuf::from("/x/c.json"))
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// **「读坏了」不许退化成「没配」**（人手编打错一个逗号时的症状差一个量级）。
#[test]
fn a_broken_file_is_a_problem_not_a_silent_not_configured() {
    let home = tmpdir("broken");
    let p = default_file(&home);
    std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
    std::fs::write(&p, b"{\"api_key\": }").expect("写夹具");
    let loaded = load(&p);
    assert!(loaded.accounts.is_empty());
    let problem = loaded.problem.expect("读坏了必须有说法");
    assert!(
        problem.contains("手编"),
        "说法没告诉人这是手编的文件：{problem}"
    );

    // 非空对照：文件**不存在**时 `problem` 是 `None`（「还没配」不是「坏了」）。
    let missing = load(&home.join("nope.json"));
    assert!(missing.problem.is_none());
    assert!(missing.accounts.is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

/// `KS11`：权限过宽 ⇒ **出声**（不是拒绝），而且说得出怎么修。
#[cfg(unix)]
#[test]
fn a_world_readable_file_is_announced_with_a_fix_and_still_serves_the_key() {
    use std::os::unix::fs::PermissionsExt;
    let home = tmpdir("too-wide");
    let p = default_file(&home);
    std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
    std::fs::write(&p, b"{\"api_key\":\"sk-ant-WIDE\"}").expect("写夹具");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).expect("放宽");

    let loaded = load(&p);
    let mut buf: Vec<u8> = Vec::new();
    let lines = announce_all(&loaded, &mut buf);
    let text = String::from_utf8(buf).expect("utf8");

    assert!(text.contains("permissions too wide"), "没出声：{text}");
    assert!(text.contains("how to fix"), "没说怎么修：{text}");
    assert!(text.contains("chmod 600"), "修法不具体：{text}");
    // ★ **出声而不是拒绝**：key 照样交出去（这是件计划定的产品取舍）。
    assert_eq!(
        single_key(&loaded)
            .expect("仍应拿到 key")
            .expose_for_auth_header(),
        "sk-ant-WIDE"
    );
    assert!(lines >= 4, "印的行数 {lines} 太少 —— 量点坏了");

    // ★★ 非空对照：收紧之后**这几句就不该出现**（否则上面全是恒真）。
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).expect("收紧");
    let mut buf2: Vec<u8> = Vec::new();
    announce_all(&load(&p), &mut buf2);
    let text2 = String::from_utf8(buf2).expect("utf8");
    assert!(
        !text2.contains("permissions too wide"),
        "收紧后仍在出声：{text2}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **日志里一个字节的 key 都不许有**（`KS4` 的行为那一半，`KS6` 的日志出口）。
#[test]
fn nothing_that_gets_announced_ever_carries_the_key() {
    let home = tmpdir("announce-clean");
    let p = default_file(&home);
    std::fs::create_dir_all(p.parent().expect("父目录")).expect("建父目录");
    std::fs::write(&p, b"{\"api_key\":\"sk-ant-CANARY-IN-ANNOUNCE\"}").expect("写夹具");
    let loaded = load(&p);
    let mut buf: Vec<u8> = Vec::new();
    announce_all(&loaded, &mut buf);
    let text = String::from_utf8(buf).expect("utf8");
    assert!(
        !text.contains("sk-ant-CANARY-IN-ANNOUNCE"),
        "启动日志里出现了 key：{text}"
    );
    // ⚠ 这里**删掉过一条断言**，经过记下来（它是 `brief` 12 逐字点名那一形的活样本）：
    //   第一版写的是 `assert!(!text.contains("26"))`，意思是「key 的长度也不许漏」。
    //   实测当场红 —— 因为**临时目录名里恰好有 `26`**（`/tmp/ccm-rc-803016-1787867…`），
    //   而这一行输出里本来就带着那条路径。⇒ **路径混进了断言**，
    //   它判的根本不是「有没有印长度」，而是「这台机器这一刻的 pid/纳秒里有没有 26」。
    //   本模块的 `announce` 压根不印任何长度 ⇒ 那条断言既恒脆又证不了东西。
    //   「明文的长度不许漏」这条性质的正确住址是 `SecretKey` 手写的 `Debug`，
    //   由 `creds-core::a_debug_print_never_carries_the_plaintext_or_its_length` 钉着。
    // 非空对照：它确实印了东西，而且印了「配好了」。
    assert!(text.contains("configured"), "什么都没印：{text}");
    let _ = std::fs::remove_dir_all(&home);
}

/// 文件不在时要给模板 —— 否则「导入」这条要靠猜。
#[test]
fn a_missing_file_prints_the_template_so_import_does_not_need_guessing() {
    let home = tmpdir("missing");
    let loaded = load(&default_file(&home));
    let mut buf: Vec<u8> = Vec::new();
    announce_all(&loaded, &mut buf);
    let text = String::from_utf8(buf).expect("utf8");
    assert!(text.contains("not configured"));
    assert!(
        text.contains(store::KEY_FIELD),
        "模板里没点名那个字段：{text}"
    );
    assert!(text.contains("plain JSON"), "没说清它是明文 JSON：{text}");
    // 路径**总是**印（`KS9` 的「文档化」）。
    assert!(text.contains(store::FILE_NAME), "没印出路径：{text}");
    let _ = std::fs::remove_dir_all(&home);
}
