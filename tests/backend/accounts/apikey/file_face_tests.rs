//! 〔RM1a · 第四波〕`accounts/apikey/file_face.rs` 的判据 —— 这台机器上那份凭据文件的帧面读写口。
//!
//! # 买到的（全在本机临时目录上真读真写，不是源码扫描）
//!
//! - 写进去的那一行，**账号层真的装得进表**：写口产出的文件交给生产段 `creds::load` ＋ `table::build`
//!   （中转进程里账号层装表那两步），`(claude-code, 账号)` 那一行在表里、带 key。两侧异源：
//!   写的是本模块，读的是层 2 那条既有的装表路。
//! - `KS10` 那组性质在这一侧同样成立：别的行与两层的未知键一个不动 · 写的那一刻读盘 ·
//!   解析不了 ⇒ 拒绝且**逐字节不动** · 不留临时文件 · 文件只给本人（unix 位）。
//! - 🔴 明文不出这一层：两条应答（序列化之后的整串）里**零命中**那把 key（带正控：同一把尺子
//!   在写进去的文件里**数得到**它 —— 不然零命中可能只是尺子瞎了）。
//! - 入参闸：缺字段 / 账号 id 当不了路由段 / 空 key 各拒一次，且拒的时候**一个字节都没写**。
//!
//! # 买不到的
//!
//! - 「写的这一份 == 那台机器上中转读的那一份」：两边调**同一个函数、同一个家目录出处**
//!   （[`super::machine_path`] 与 `main.rs` 的 `--relay` 臂），那是构造上的事；
//!   拿两边的返回值互比是两侧同源的恒等（恒真），本族刻意不写。环境不同（中转不是后端起的）时
//!   两边会解到两处 —— 远端那台的中转由 `relay-ensure` 起、继承后端的环境，才是这条的前提。
//! - 🔴 真远端 · 真 Windows（DACL 那一支）一格没跑过。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-apikey-face-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

/// 那份文件在临时「家目录」下的位置（与生产同一个相对落点：`<家>/claudecode-frontend/<FILE_NAME>`）。
fn file_in(home: &Path) -> PathBuf {
    store::path_under_claude_home(home)
}

/// 夹具 key：**结构样本**，不是任何真 key。
const PLAIN: &str = "sk-FIXTURE-0123456789abcdef-NOT-A-REAL-KEY";

#[test]
fn a_written_row_is_really_loaded_by_the_account_layer() {
    let home = temp_dir("load");
    let f = file_in(&home);
    std::fs::create_dir_all(&home).unwrap();
    let got = answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).expect("写应当成功");
    assert_eq!(got["account"], "work");
    assert_eq!(got["path"], f.display().to_string());
    assert_eq!(
        got["masked"],
        SecretKey::new(PLAIN).masked(),
        "回的掩码应当是盘上那一行的掩码"
    );

    // ★ 交给账号层装表那两步（生产段那条既有的路），而不是本模块自己再解析一遍。
    let loaded = super::super::creds::load(&f);
    assert!(
        loaded.problem.is_none(),
        "账号层读不动写口产出的文件：{:?}",
        loaded.problem
    );
    let base = crate::relay::Base::parse("https://api.example.invalid").expect("夹具上游");
    let (table, rejected, _notes) =
        super::super::table::build(loaded.accounts, super::super::CREDENTIALS_FILE_AGENT, &base);
    assert!(
        rejected.is_empty(),
        "写进去的那一行被账号层拒了：{} 条",
        rejected.len()
    );
    assert_eq!(table.len(), 1, "表里应当恰好这一行");
    assert!(
        table
            .lookup(super::super::CREDENTIALS_FILE_AGENT, "work")
            .is_some(),
        "账号层按 (agent, 账号) 查不到写进去的那一行"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn other_rows_and_unknown_keys_survive_and_the_disk_is_read_at_write_time() {
    let home = temp_dir("merge");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    // 人手编过的一份：顶层未知键 · 另一条账号（带 base_url 与它自己的未知键）。
    let hand = r#"{
  "accounts": {
    "other": { "api_key": "sk-OTHER-FIXTURE", "base_url": "https://up.example.invalid", "note": "手写的" }
  },
  "zzz_unknown": 7
}"#;
    std::fs::write(&f, hand).unwrap();
    answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).expect("第一次写");
    // ★ 写的那一刻读盘：两次写之间**绕过写口**在盘上加一格，第二次写必须留着它。
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    doc["between_writes"] = json!("盘上后来加的");
    std::fs::write(&f, serde_json::to_string(&doc).unwrap()).unwrap();
    answer_set_at(
        &f,
        &json!({"account": "work", "key": "sk-SECOND-FIXTURE-KEY"}),
    )
    .expect("第二次写");

    let after: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(after["zzz_unknown"], 7, "顶层未知键被吃了");
    assert_eq!(
        after["between_writes"], "盘上后来加的",
        "两次写之间盘上加的那一格被陈旧副本盖掉了"
    );
    assert_eq!(
        after["accounts"]["other"]["api_key"], "sk-OTHER-FIXTURE",
        "别的行被改了"
    );
    assert_eq!(
        after["accounts"]["other"]["note"], "手写的",
        "别的行自己的未知键被吃了"
    );
    assert_eq!(
        after["accounts"]["work"]["api_key"],
        "sk-SECOND-FIXTURE-KEY"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_unparseable_file_is_refused_and_left_byte_for_byte() {
    let home = temp_dir("badfile");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    let broken = "{ \"accounts\": { \"work\": { \"api_key\": \"sk-x\" } ";
    std::fs::write(&f, broken).unwrap();
    let err = answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).unwrap_err();
    assert_eq!(err.0, "bad_file", "解析不了应当回 bad_file，实得 {err:?}");
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        broken,
        "解析不了的文件被动过了"
    );
    // 读口同样**不退化成「没配」**：说出读坏了。
    let r = read_at(&f);
    assert!(
        r["problem"].is_string(),
        "读口把读坏了的文件报成了没问题：{r}"
    );
    assert_eq!(r["rows"], json!([]));
    assert_no_residue(f.parent().unwrap());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn bad_arguments_are_refused_before_a_single_byte_is_written() {
    let home = temp_dir("args");
    let f = file_in(&home);
    std::fs::create_dir_all(&home).unwrap();
    let cases = [
        (json!({"key": PLAIN}), "缺 account"),
        (json!({"account": "work"}), "缺 key"),
        (
            json!({"account": "../x", "key": PLAIN}),
            "account 带路径分隔",
        ),
        (json!({"account": "a b", "key": PLAIN}), "account 带空白"),
        (json!({"account": "", "key": PLAIN}), "account 空"),
        (json!({"account": "work", "key": "   "}), "key 全空白"),
        (json!({"account": 3, "key": PLAIN}), "account 不是字符串"),
    ];
    for (args, what) in cases {
        let err = answer_set_at(&f, &args).unwrap_err();
        assert_eq!(err.0, "bad_args", "「{what}」应当是 bad_args，实得 {err:?}");
        assert!(!err.1.contains(PLAIN), "「{what}」的报错文案里带着明文 key");
        assert!(!f.exists(), "「{what}」被拒了，却写出了文件");
        assert!(
            !f.parent().unwrap().exists(),
            "「{what}」被拒了，却建出了目录"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_plaintext_never_leaves_in_either_answer() {
    let home = temp_dir("plain");
    let f = file_in(&home);
    std::fs::create_dir_all(&home).unwrap();
    let set = answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).unwrap();
    let read = read_at(&f);
    // 正控：同一把尺子（子串）在盘上那份文件里**数得到**它 —— 否则下面的零命中可能是尺子瞎了。
    let on_disk = std::fs::read_to_string(&f).unwrap();
    assert_eq!(
        on_disk.matches(PLAIN).count(),
        1,
        "正控：盘上那份应当恰好含一次明文"
    );
    for (name, v) in [("apikey-key-set", &set), ("apikey-read", &read)] {
        let wire = serde_json::to_string(v).unwrap();
        assert_eq!(
            wire.matches(PLAIN).count(),
            0,
            "`{name}` 的应答里带着明文：{wire}"
        );
        // 更狠一格：明文里任何一段 ≥ 12 字符的子串都不许出现（掩码只留头尾几位）。
        let chars: Vec<char> = PLAIN.chars().collect();
        for w in chars.windows(12) {
            let piece: String = w.iter().collect();
            assert!(
                !wire.contains(&piece),
                "`{name}` 的应答里带着明文的一段 {piece:?}"
            );
        }
    }
    assert_eq!(read["rows"], json!(["work"]));
    assert_eq!(
        read["configured"], false,
        "顶层那一把没配，`configured` 说的是顶层那一把"
    );
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_written_file_is_owner_only_and_no_temp_file_is_left() {
    let home = temp_dir("perm");
    let f = file_in(&home);
    std::fs::create_dir_all(&home).unwrap();
    answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "凭据文件不是只给本人：{mode:o}");
    }
    // 读口的权限判断在这份文件上不出声（只给本人 ⇒ `notice` 为空）。
    assert!(
        read_at(&f)["notice"].is_null(),
        "只给本人的文件被报了权限问题"
    );
    assert_no_residue(f.parent().unwrap());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_missing_agent_home_is_said_and_not_created_for_you() {
    let home = temp_dir("nohome");
    // 家目录本身不在：只许建 `claudecode-frontend/` 这一层，不替 agent 建它的家。
    let f = file_in(&home.join("not-there"));
    let err = answer_set_at(&f, &json!({"account": "work", "key": PLAIN})).unwrap_err();
    assert_eq!(err.0, "io_failed", "实得 {err:?}");
    assert!(!home.join("not-there").exists(), "替 agent 建了家目录");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_read_face_on_an_absent_file_says_nothing_is_there() {
    let home = temp_dir("absent");
    let r = read_at(&file_in(&home));
    assert_eq!(r["configured"], false);
    assert_eq!(r["masked"], "");
    assert!(r["notice"].is_null(), "文件不在时不该报权限问题：{r}");
    assert!(r["problem"].is_null(), "文件不在不是读坏了：{r}");
    assert_eq!(r["rows"], json!([]));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn rows_that_cannot_be_a_route_segment_are_not_reported() {
    let home = temp_dir("rows");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(
        &f,
        r#"{"accounts":{"ok-1":{"api_key":"sk-a"},"bad id":{"api_key":"sk-b"}}}"#,
    )
    .unwrap();
    assert_eq!(read_at(&f)["rows"], json!(["ok-1"]));
    let _ = std::fs::remove_dir_all(&home);
}

/// 写口的临时文件名是确定的（`<FILE_NAME>.<pid>.tmp`）⇒ 直接问那一个名字在不在，不遍历目录。
fn assert_no_residue(dir: &Path) {
    let tmp = dir.join(format!("{}.{}.tmp", store::FILE_NAME, std::process::id()));
    assert!(!tmp.exists(), "留下了临时文件：{}", tmp.display());
}

/// 〔ST2 × RM1a〕Base URL 跟着 key 写进**这台机器**那一份：给了 ⇒ 落进这一行、读回；
/// 只配 key（缺席 / null / 空串）⇒ **已有端点原样留着**；形状不对 ⇒ 整次不写（key 也不落，文件逐字节不动）。
#[test]
fn base_url_is_written_with_the_key_and_left_alone_when_only_the_key_changes() {
    let home = temp_dir("baseurl");
    let f = file_in(&home);
    std::fs::create_dir_all(&home).unwrap();
    let got = answer_set_at(
        &f,
        &json!({"account": "work", "key": PLAIN, "baseUrl": " https://up.example.invalid/v1 "}),
    )
    .expect("带 Base URL 的写应当成功");
    assert_eq!(
        got["baseUrl"], "https://up.example.invalid/v1",
        "读回的端点不对：{got}"
    );
    for only_key in [
        json!({"account": "work", "key": "sk-SECOND-FIXTURE"}),
        json!({"account": "work", "key": "sk-THIRD-FIXTURE", "baseUrl": null}),
        json!({"account": "work", "key": "sk-FOURTH-FIXTURE", "baseUrl": "  "}),
    ] {
        let got = answer_set_at(&f, &only_key).expect("只配 key 应当成功");
        assert_eq!(
            got["baseUrl"], "https://up.example.invalid/v1",
            "只配 key 把已有端点动了：{only_key}"
        );
    }
    let before = std::fs::read(&f).unwrap();
    for bad in ["ftp://x", "https://", "up.example.invalid", "https://a b"] {
        let err = answer_set_at(
            &f,
            &json!({"account": "work", "key": PLAIN, "baseUrl": bad}),
        )
        .expect_err("坏形状还写了");
        assert_eq!(err.0, "bad_args", "{bad:?} 应当是 bad_args，实得 {err:?}");
        assert!(!err.1.contains(PLAIN), "报错里带着明文");
    }
    let err =
        answer_set_at(&f, &json!({"account": "work", "key": PLAIN, "baseUrl": 3})).unwrap_err();
    assert_eq!(err.0, "bad_args");
    assert_eq!(std::fs::read(&f).unwrap(), before, "形状不对却动了文件");
    // 与本机那一侧是**同一条**形状关（同一个函数），不是两份：拿同一组输入问它，结论一致。
    for s in ["ftp://x", "https://", "https://ok.example"] {
        assert_eq!(
            store::check_base_url_shape(s).is_ok(),
            s == "https://ok.example",
            "{s:?} 的判法变了"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}
