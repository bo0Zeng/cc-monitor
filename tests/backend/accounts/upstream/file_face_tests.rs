//! 〔RM1a · 第四波〕`accounts/upstream/file_face.rs` 的判据 —— 这台机器上那份凭据文件的帧面读写口。
//!
//! # 买到的（全在本机临时目录上真读真写，不是源码扫描）
//!
//! - 写进去的那一行，**上游选择真的装得进表**：写口产出的文件交给生产段 `creds::load` ＋ `table::build`
//!   （中转进程里上游选择装表那两步），`(claude-code, 账号)` 那一行在表里、带 key。两侧异源：
//!   写的是本模块，读的是上游选择那条既有的装表路。
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
fn a_written_row_is_really_loaded_by_upstream_selection() {
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

    // ★ 交给上游选择装表那两步（生产段那条既有的路），而不是本模块自己再解析一遍。
    let loaded = super::super::creds::load(&f);
    assert!(
        loaded.problem.is_none(),
        "上游选择读不动写口产出的文件：{:?}",
        loaded.problem
    );
    let base = crate::relay::Base::parse("https://api.example.invalid").expect("夹具上游");
    let (table, rejected, _notes) =
        super::super::table::build(loaded.accounts, super::super::CREDENTIALS_FILE_AGENT, &base);
    assert!(
        rejected.is_empty(),
        "写进去的那一行被上游选择拒了：{} 条",
        rejected.len()
    );
    assert_eq!(table.len(), 1, "表里应当恰好这一行");
    assert!(
        table
            .lookup(super::super::CREDENTIALS_FILE_AGENT, "work")
            .is_some(),
        "上游选择按 (agent, 账号) 查不到写进去的那一行"
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
    // 〔US1〕「表里有哪几行」不再出线（`rows_at` 一份）：读坏了 ⇒ 零条。
    assert!(
        r.get("rows").is_none(),
        "`rows` 又回到了 `apikey-read` 的应答里：{r}"
    );
    assert!(rows_at_with(&f, &|_| None).is_empty());
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
    assert_eq!(rows_at_with(&f, &|_| None), vec!["work".to_string()]);
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
    assert!(rows_at_with(&file_in(&home), &|_| None).is_empty());
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
    assert_eq!(rows_at_with(&f, &|_| None), vec!["ok-1".to_string()]);
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

// ════════════════════════════════════════════════════════════════════════════
// 〔GP1 · 第四波〕本机那一份的写者也换成了本机常驻后端（主会话裁「每台机器一个写者 ＝ 那台的后端」）⇒
// monitor 那侧 `creds_store::write_key_at`〔散文墓碑〕删了，它身上三条**本侧没有同形**的写路判据逐条搬到这里
// （写的是同一份 `creds_core::store` 规则，被测换成这一侧唯一那个写口）。其余几条本侧早有同形：
// 写的那一刻读盘 / 别的行不动（`other_rows_and_unknown_keys_survive_…`）· 出生即只给本人（`the_written_file_is_owner_only_…`）·
// Base URL（`base_url_is_written_with_the_key_…`）· 说不出账号拒写（`bad_arguments_are_refused_…`）。
// 要求住址：`调研/第四波记录/GP1.md §3` · `INVARIANTS §42`（每台机器上的程序写者恰好一个）。
// ════════════════════════════════════════════════════════════════════════════

/// ★★ `KS10` 行为那一半（原 monitor `a_program_write_keeps_everything_the_human_put_there`〔散文墓碑〕）：
/// 人手编的顶层键 · 顶层那一把旧 key（`KH2C3`：读得出来的一行，但**不再是写进去的地方**）一个字节不动；
/// 新的那一把落在 `accounts.<id>`；落盘按键名排序（`KS10②`）。期望全是手写字面量。
#[test]
fn gp1_a_program_write_keeps_everything_the_human_put_there() {
    let home = temp_dir("gp1-interleave");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(
        &f,
        b"{\n  \"_note\": \"human changed this\",\n  \"my_own\": \"keep me\",\n  \"brand_new\": 7,\n  \"api_key\": \"OLD\"\n}\n",
    )
    .unwrap();
    answer_set_at(&f, &json!({"account": "acct-x", "key": "NEW-KEY"})).expect("写");
    let text = std::fs::read_to_string(&f).unwrap();
    let back: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(back["_note"], "human changed this", "人的编辑被盖掉了");
    assert_eq!(back["brand_new"], 7, "人新加的键被吃掉了");
    assert_eq!(back["my_own"], "keep me");
    assert_eq!(back["accounts"]["acct-x"][store::KEY_FIELD], "NEW-KEY");
    assert_eq!(
        back[store::KEY_FIELD],
        "OLD",
        "写侧把顶层那一把盖掉了 —— 那是老用户手上那份文件里唯一那把 key"
    );
    // 顺序稳定：四个各恰好出现一处的键按名字排。
    let at = |k: &str| {
        let hits: Vec<usize> = text
            .match_indices(&format!("\"{k}\""))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(hits.len(), 1, "`{k}` 不是恰好出现一处：{text}");
        hits[0]
    };
    let (ia, ib, ic, id) = (at("_note"), at("accounts"), at("brand_new"), at("my_own"));
    assert!(
        ia < ib && ib < ic && ic < id,
        "落盘不是按键名排序的：{text}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★★★ `K-R1` 写侧那一格（原 monitor `a_saved_key_does_not_swallow_the_hand_written_upstream_or_auth_style`〔散文墓碑〕）：
/// 配一次 key 不许吃掉**同一行**上人手编的 `auth_style` / `base_url` / 未知键；别的行一个字节不动，
/// 也不许凭空给别的行加 `auth_style`。并且装回上游选择那一层之后那两格真的读得出来（不只是「JSON 里还在」）。
#[test]
fn gp1_a_saved_key_does_not_swallow_the_hand_written_upstream_or_auth_style() {
    let home = temp_dir("gp1-keep-row");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(
        &f,
        b"{\n  \"accounts\": {\n    \"acct-x\": {\n      \"api_key\": \"OLD\",\n      \"auth_style\": \"x-api-key\",\n      \"base_url\": \"https://gw.example.com/anthropic\",\n      \"my_own\": \"keep me\"\n    },\n    \"acct-y\": {\n      \"api_key\": \"Y\"\n    }\n  }\n}\n",
    )
    .unwrap();
    answer_set_at(&f, &json!({"account": "acct-x", "key": "NEW-KEY"})).expect("写");
    let back: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    let row = &back["accounts"]["acct-x"];
    assert_eq!(
        row[store::AUTH_STYLE_FIELD],
        "x-api-key",
        "auth_style 被吃了：{back}"
    );
    assert_eq!(
        row[store::BASE_URL_FIELD],
        "https://gw.example.com/anthropic",
        "base_url 被吃了：{back}"
    );
    assert_eq!(row[store::KEY_FIELD], "NEW-KEY", "key 没换：{back}");
    assert_eq!(row["my_own"], "keep me");
    assert_eq!(back["accounts"]["acct-y"][store::KEY_FIELD], "Y");
    assert!(
        back["accounts"]["acct-y"]
            .get(store::AUTH_STYLE_FIELD)
            .is_none(),
        "给没写过 auth_style 的那一行凭空加了一格：{back}"
    );
    let doc = store::parse(&std::fs::read_to_string(&f).unwrap()).expect("解析");
    let rows = store::read_accounts(&doc);
    let x = rows
        .iter()
        .find(|e| e.id == "acct-x")
        .expect("acct-x 该读得出来");
    assert_eq!(
        x.auth_style,
        store::AuthStyleSetting::Known(store::AuthStyle::XApiKey)
    );
    assert_eq!(
        x.base_url.as_deref(),
        Some("https://gw.example.com/anthropic")
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★★★ `KH2C3` 后半（原 monitor `the_write_side_no_longer_targets_the_legacy_top_level_slot`〔散文墓碑〕）：
/// 结构 —— 这一侧写口生产段里 `store::merge_key(`（写顶层那一格的唯一入口）零处，`merge_account_key(` 在（正控）；
/// 行为 —— 全新文件写一次，顶层那一格根本不被创建。
#[test]
fn gp1_the_write_side_never_targets_the_legacy_top_level_slot() {
    let src = guard_core::production_code(include_str!(
        "../../../../src/backend/accounts/upstream/file_face.rs"
    ));
    assert!(
        src.contains("merge_account_key("),
        "连新那个写口都没有 —— 取法坏了，下面恒 0"
    );
    assert_eq!(
        src.matches("store::merge_key(").count(),
        0,
        "写侧还有一处在写顶层那一格（`{}` 那一行谁的会话都命中得了）",
        store::LEGACY_ACCOUNT_ID
    );
    let home = temp_dir("gp1-legacy-slot");
    let f = file_in(&home);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    answer_set_at(&f, &json!({"account": "acct-fresh", "key": "KEY-FRESH"})).expect("写");
    let back: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert!(
        back.get(store::KEY_FIELD).is_none(),
        "全新文件被写出了顶层那一格：{back}"
    );
    assert_eq!(
        back["accounts"]["acct-fresh"][store::KEY_FIELD],
        "KEY-FRESH"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 〔US1 · 4D〕E2：「表里有哪几行」只有一份 —— [`rows_at`] == 上游选择装表**真收进表**的那几行。
///
/// 守的要求：B-decouple §2.1 必须拆 1（monitor `history::apikey_rows_at`〔散文墓碑〕另算人群，头注自认
/// 「`base_url` 写错的号界面说经本机中转、后端 404」）· `设计/05 §14.3`「业务解释只有一个家」。
/// 夹具里四行进不了表（`base_url` 坏 · 明文非回环 · `auth_style` 认不出 · id 当不了路由段）、一行能进；
/// 正控：`store::read_accounts` 读得到全部五行（旧口径下前三行都算「有行」）。
#[test]
fn us1_the_rows_are_exactly_what_the_table_builder_keeps() {
    let dir = temp_dir("us1-rows");
    let path = dir.join("apikey-credentials.json");
    std::fs::write(
        &path,
        r#"{"accounts":{
            "good":{"api_key":"K1"},
            "bad-url":{"api_key":"K2","base_url":"ftp://nope"},
            "plain-off-loopback":{"api_key":"K3","base_url":"http://example.com/v1"},
            "odd-auth":{"api_key":"K4","auth_style":"no-such-style"},
            "bad id":{"api_key":"K5"}
        }}"#,
    )
    .unwrap();
    let doc = store::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(store::read_accounts(&doc).len(), 5, "正控：五行都读得到");
    assert_eq!(rows_at_with(&path, &|_| None), vec!["good".to_string()]);
    // 默认上游旋钮认不出（那一刻中转也起不来）⇒ 零条；文件解析不了 ⇒ 零条。
    assert!(
        rows_at_with(&path, &|k| (k == "CCM_AGENT_UPSTREAM_CLAUDE_CODE")
            .then(|| "not a url".into()))
        .is_empty()
    );
    std::fs::write(&path, "{ not json").unwrap();
    assert!(rows_at_with(&path, &|_| None).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// 〔US1 · 4D，原 monitor `creds_store_tests` 同名一条搬来〕`KS11` 门①那半：那份文件被放宽了，读口**出声**；只给本人时**不出声**（两向）。
/// 守的要求：`INVARIANTS §42`（凭据文件的读写）· `KS11`「权限过宽要在界面上显出来」—— 界面那一格今天就是这台后端 `apikey-read` 的 `notice`。
#[cfg(unix)]
#[test]
fn us1_a_widened_file_is_called_out_and_an_owner_only_one_is_not() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("us1-perm");
    let p = dir.join("apikey-credentials.json");
    std::fs::write(&p, b"{\n  \"api_key\": \"sk-ant-HAND-PLACED\"\n}\n").expect("写夹具");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).expect("收窄");
    assert!(
        read_at(&p)["notice"].is_null(),
        "只给本人的文件被报了权限问题 —— 那条提醒会变成噪音"
    );
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).expect("放宽");
    let notice = read_at(&p)["notice"]
        .as_str()
        .expect("过宽了必须出声")
        .to_string();
    assert!(notice.contains("chmod 600"), "没说清怎么修：{notice}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 〔US1 · 4D，原 monitor `creds_store_tests` 同名一条搬来〕三态：没配 · 配了 · 文件读坏了。**「读坏了」不许退化成「没配」**；
/// 配了 ⇒ 只回掩码（`KS6`：应答整串里零明文，带正控：掩码里有遮蔽符）。
#[test]
fn us1_a_broken_file_is_surfaced_instead_of_looking_unconfigured() {
    let dir = temp_dir("us1-three");
    let p = dir.join("apikey-credentials.json");
    let s0 = read_at(&p);
    assert!(s0["configured"] == json!(false) && s0["problem"].is_null() && s0["notice"].is_null());
    std::fs::write(&p, b"{\n  \"api_key\": \"sk-ant-0123456789ABCDEF\"\n}\n").expect("写夹具");
    let s1 = read_at(&p);
    assert_eq!(s1["configured"], json!(true));
    assert!(!s1.to_string().contains("0123456789"), "回了明文：{s1}");
    assert!(
        s1["masked"].as_str().unwrap().contains('*'),
        "掩码里没有遮蔽符：{s1}"
    );
    std::fs::write(&p, b"{\"api_key\": }").expect("改坏");
    let s2 = read_at(&p);
    assert_eq!(s2["configured"], json!(false));
    assert!(
        s2["problem"]
            .as_str()
            .expect("读坏了必须有说法")
            .contains("手编"),
        "没告诉人这是一份手编的文件：{s2}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 〔US1 · 4D，原 monitor `creds_store_tests::what_the_write_side_wrote_is_exactly_the_row_the_launch_side_looks_for`〔散文墓碑〕〕
/// **写口真写下的那一行，正是起会话那一发的成品用上的那一行**（跨两个读者：写口 `answer_set_at` → 人群 `rows_at` → 决策 `decide_launch`）。
/// 两侧异源：写的是写口，读的是上游选择装表那一条 ＋ 决策表。只配了一个号 ⇒ 另一个号不许被顺带配上；不落 `default` 那一行。
#[test]
fn us1_what_the_write_side_wrote_is_exactly_the_row_the_launch_answer_uses() {
    use super::super::endpoint::{answer_launch_with, answer_routing_with};
    let dir = temp_dir("us1-same-source");
    let p = dir.join("apikey-credentials.json");
    let ask = |d: &str| json!({"agent":"claude-code","account":{"kind":"named","configDir":d},"key":"k-1","allSessions":false});
    // 非空对照排最前：还没写的时候，成品说「不注入」。
    assert!(answer_launch_with(
        &ask("/h/.claude-accts/acct-one"),
        &rows_at_with(&p, &|_| None),
        &|_| true
    )
    .unwrap()["baseUrl"]
        .is_null());
    answer_set_at(&p, &json!({"account":"acct-one","key":"KEY-FOR-ONE"})).expect("写");
    let rows = rows_at_with(&p, &|_| None);
    assert_eq!(
        answer_launch_with(&ask("/h/.claude-accts/acct-one"), &rows, &|_| true).unwrap()["baseUrl"],
        json!("http://127.0.0.1:8788/s/claude-code/acct-one/k-1"),
        "写口写下的那一行，起会话的成品没用上（表里：{rows:?}）"
    );
    assert!(
        answer_launch_with(&ask("/h/.claude-accts/acct-two"), &rows, &|_| true).unwrap()["baseUrl"]
            .is_null()
    );
    assert_eq!(
        answer_routing_with(&json!({"agent":"claude-code","configDirs":["/h/.claude-accts/acct-one","/h/.claude-accts/acct-two"]}), &rows, &|_| true).unwrap()["routed"],
        json!(["/h/.claude-accts/acct-one"])
    );
    assert!(
        !rows.iter().any(|r| r == store::LEGACY_ACCOUNT_ID),
        "写口落在 `default` 那一行上：{rows:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
