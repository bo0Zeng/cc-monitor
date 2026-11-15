//! Codex 账号面：凭据怎么读 · 交给核心续期的那一形（`FACE.login`）· 目录信任怎么写进 `config.toml`（`FACE.trust`）。
//! 夹具全是手写的合成件（假 JWT · 假令牌）；盘只用沙箱目录（`test_home::scratch`），不碰真的 `~/.codex`。

use super::*;
use crate::agents::{IdentityCell, LoginFace, LoginOrigin, WriteLock};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// 一段 JWT：头 · 载荷 · 签名三段 base64url（不补位），载荷是给的 JSON。
fn jwt(payload: &Value) -> String {
    let seg = |b: &[u8]| {
        cmd_wire::b64::b64_encode(b)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    };
    format!(
        "{}.{}.{}",
        seg(br#"{"alg":"RS256","typ":"JWT"}"#),
        seg(payload.to_string().as_bytes()),
        seg(b"sig")
    )
}

const EXP: u64 = 1_900_000_000;
/// `2026-11-01T00:00:00Z` 那一刻（毫秒）。
const LAST_REFRESH_MS: u64 = 1_793_491_200_000;
const EIGHT_DAYS_MS: u64 = 8 * 86_400_000;

/// 一份订阅号的 `auth.json`（形照 codex-rs 的 `codex_login::auth::storage::AuthDotJson`）：带 provenance · login_owner_id · 一格我们不认识的。
fn doc(provenance: bool) -> Value {
    let mut tokens = json!({
        "id_token": jwt(&json!({"email": "a@example.test", "https://api.openai.com/auth": {"chatgpt_plan_type": "pro"}})),
        "access_token": jwt(&json!({"exp": EXP})),
        "refresh_token": "rt-old",
        "account_id": "acct-1",
        "login_owner_id": "owner-1",
    });
    if provenance {
        tokens["oauth_provenance"] =
            json!({"issuer": "https://issuer.example.test/", "client_id": "app_custom"});
    }
    json!({
        "auth_mode": "chatgpt",
        "OPENAI_API_KEY": null,
        "tokens": tokens,
        "last_refresh": "2026-11-01T00:00:00Z",
        "future_field": {"kept": true},
    })
}

fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).expect("json")
}

fn map(v: &Value) -> serde_json::Map<String, Value> {
    v.as_object().expect("对象").clone()
}

fn login() -> LoginFace {
    FACE.login.expect("Codex 的订阅号交核心续期")
}

fn expiry(v: &Value) -> Option<u64> {
    let crate::agents::Expiry::Read(read) = login().expiry else {
        panic!("过期时刻交这一家读（解访问令牌），不是一格毫秒数")
    };
    read(&map(v))
}

// ═══ 凭据 ═══

#[test]
fn a_signed_in_auth_json_gives_the_email_and_the_account() {
    let Creds::Signed(s) = creds_of(Some(&bytes(&doc(false)))) else {
        panic!("应读出订阅号")
    };
    assert_eq!(s.email.as_deref(), Some("a@example.test"));
    assert_eq!(s.account_id.as_deref(), Some("acct-1"));
}

#[test]
fn the_email_falls_back_to_the_profile_claim() {
    let mut d = doc(false);
    d["tokens"]["id_token"] = json!(jwt(
        &json!({"https://api.openai.com/profile": {"email": "p@example.test"}})
    ));
    let Creds::Signed(s) = creds_of(Some(&bytes(&d))) else {
        panic!("应读出订阅号")
    };
    assert_eq!(s.email.as_deref(), Some("p@example.test"));
}

#[test]
fn missing_broken_and_key_only_files_say_so() {
    assert_eq!(creds_of(None), Creds::Absent);
    assert_eq!(creds_of(Some(b"{not json")), Creds::Unreadable);
    assert_eq!(creds_of(Some(b"[1,2]")), Creds::Unreadable);
    // API key 号（下一版才进账号库）：没有 `tokens`。
    assert_eq!(
        creds_of(Some(br#"{"OPENAI_API_KEY":"sk-x"}"#)),
        Creds::NoTokens
    );
    // 令牌不是 JWT ⇒ 邮箱读不出，照实是 `None`。
    let mut d = doc(false);
    d["tokens"]["id_token"] = json!("x.y");
    let Creds::Signed(s) = creds_of(Some(&bytes(&d))) else {
        panic!("仍是订阅号")
    };
    assert_eq!(s.email, None);
}

#[test]
fn the_email_is_read_from_the_account_directory() {
    let dir = test_home::scratch("codex-acct-email");
    assert_eq!(email_in(&dir), None);
    std::fs::write(dir.join("auth.json"), bytes(&doc(false))).expect("写夹具");
    assert_eq!(email_in(&dir).as_deref(), Some("a@example.test"));
}

// ═══ 交给核心续期的那一形 ═══

#[test]
fn the_login_cell_names_auth_json_its_tokens_section_and_the_auth_lock() {
    let l = login();
    assert_eq!(l.creds_file, "auth.json");
    assert_eq!(
        (l.section, l.access, l.refresh),
        ("tokens", "access_token", "refresh_token")
    );
    // Codex 整份写 `auth.json` 时拿的是 `auth.lock` 上的排他文件锁（不是 `mkdir` 锁）。
    assert_eq!(l.write_lock, Some(WriteLock::File("auth.lock")));
    // 没记出处的号照 Codex 的默认端点与 client id。
    assert_eq!(l.token_url, "https://auth.openai.com/oauth/token");
    assert_eq!(l.client_id, "app_EMoamEEZ73f0CkXaXp7hrann");
    // 离过期 5 分钟以内就续。
    assert_eq!(l.margin_ms, 5 * 60 * 1000);
    // 锁不罩网络请求 ⇒ 只续此刻没有 Codex 会话在用的号。
    assert_eq!(FACE.renew, crate::agents::Renew::Idle);
    // 账号 0（`~/.codex`）是用户自己的那份：只读，不续不写。
    assert_eq!(l.base_login, crate::agents::BaseLogin::Untouched);
}

#[test]
fn the_expiry_is_the_access_token_exp_or_eight_days_after_the_last_refresh() {
    assert_eq!(expiry(&doc(false)), Some(EXP * 1000));
    // 访问令牌解不出 `exp` ⇒ 上次续的时刻 ＋ 8 天。
    let mut d = doc(false);
    d["tokens"]["access_token"] = json!("opaque");
    assert_eq!(expiry(&d), Some(LAST_REFRESH_MS + EIGHT_DAYS_MS));
    // 两样都没有 ⇒ 说不出几点过期。
    d.as_object_mut().expect("对象").remove("last_refresh");
    assert_eq!(expiry(&d), None);
    assert_eq!(expiry(&json!({"OPENAI_API_KEY": "sk-x"})), None);
}

#[test]
fn provenance_names_the_token_url_and_the_client_id() {
    let origin = login().origin.expect("Codex 在凭据里记出处");
    assert_eq!(
        origin(&map(&doc(true))),
        Some(LoginOrigin {
            token_url: "https://issuer.example.test/oauth/token".into(),
            client_id: Some("app_custom".into()),
        })
    );
    // 没记 · 缺一格 ⇒ 没有出处（核心照缺省那一对）。
    assert_eq!(origin(&map(&doc(false))), None);
    let mut d = doc(true);
    d["tokens"]["oauth_provenance"]["client_id"] = json!("");
    assert_eq!(origin(&map(&d)), None);
}

#[test]
fn a_renewal_stamps_last_refresh_in_rfc3339_that_the_expiry_reads_back() {
    let (key, stamp) = login().renewed_at.expect("续完记 last_refresh");
    assert_eq!(key, "last_refresh");
    const NOW_MS: u64 = 1_893_456_000_123; // 2030-01-01T00:00:00.123Z
    assert_eq!(stamp(NOW_MS), json!("2030-01-01T00:00:00.123Z"));
    let mut d = doc(false);
    d["tokens"]["access_token"] = json!("opaque");
    d["last_refresh"] = stamp(NOW_MS);
    assert_eq!(expiry(&d), Some(NOW_MS + EIGHT_DAYS_MS));
}

#[test]
fn the_account_identity_is_the_account_id_in_auth_json_and_requests_carry_none() {
    let l = login();
    let dir = test_home::scratch("codex-acct-identity");
    let file = (l.identity_file)(&dir, None);
    assert_eq!(file, dir.join("auth.json"));
    assert_eq!((l.identity_in)(&file), None);
    std::fs::write(&file, bytes(&doc(false))).expect("写夹具");
    assert_eq!((l.identity_in)(&file).as_deref(), Some("acct-1"));
    // 账号身份走请求头、不在请求体里 ⇒ 不读 · 不换。
    let body = br#"{"model":"m","input":[]}"#;
    assert_eq!((l.identity_of_body)(body), None);
    assert_eq!((l.rewrite_identity)(body, "acct-2"), IdentityCell::Absent);
}

// ═══ 目录信任 ═══

const CONFIG: &str = r#"# 用户自己的注释
model = "gpt-5"

[projects."/w/other"]
trust_level = "untrusted"

[mcp_servers.x]
command = "run"
args = [
  "[not a header]",
]
"#;

fn trust() -> crate::agents::TrustEdit {
    FACE.trust
        .expect("Codex 有目录信任")
        .edit
        .expect("config.toml 不是 JSON ⇒ 这一家自己的改法")
}

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| k.to_string()).collect()
}

#[test]
fn the_trust_cell_reads_only_trusted_directories() {
    let t = trust();
    assert_eq!((t.trusted)(CONFIG), Ok(set(&[])));
    assert_eq!((t.trusted)(""), Ok(set(&[])));
    let c = "[projects.\"/w/a\"]\ntrust_level = \"trusted\"\n[projects]\n\"/w/b\" = { trust_level = 'trusted' }\n\"/w/c\" = { trust_level = 'untrusted' }\n";
    assert_eq!((t.trusted)(c), Ok(set(&["/w/a", "/w/b"])));
    assert!((t.trusted)("a = \"unterminated\n").is_err());
}

#[test]
fn the_trust_cell_marks_every_key_and_leaves_the_rest_alone() {
    let t = trust();
    let out = (t.mark)(CONFIG, &set(&["/w/p", "/w/other"]))
        .expect("读得懂")
        .expect("要写");
    assert_eq!((t.trusted)(&out), Ok(set(&["/w/other", "/w/p"])));
    // 原文每一行照次序都在（`/w/other` 那一行只改了值），多出来的只有 `/w/p` 那一节。
    let before = CONFIG.replace("trust_level = \"untrusted\"", "trust_level = \"trusted\"");
    assert_eq!(
        added_lines(&before, &out),
        ["[projects.\"/w/p\"]", "trust_level = \"trusted\""]
    );
    // 都已经是 ⇒ 不出新原文；读不懂 ⇒ `Err`。
    assert_eq!((t.mark)(&out, &set(&["/w/p"])), Ok(None));
    assert!((t.mark)("[projects.\"/w/p\"\n", &set(&["/w/p"])).is_err());
}

/// `before` 的每一行照次序都在 `after` 里（一行不改）⇒ `after` 多出来的那几行（空行不算）；不是这样 ⇒ 判据失败。
fn added_lines<'a>(before: &str, after: &'a str) -> Vec<&'a str> {
    let mut want = before.lines().filter(|l| !l.trim().is_empty()).peekable();
    let mut extra = Vec::new();
    for l in after.lines().filter(|l| !l.trim().is_empty()) {
        if want.peek() == Some(&l) {
            want.next();
        } else {
            extra.push(l);
        }
    }
    assert!(want.next().is_none(), "原文有一行不见了或挪了：\n{after}");
    extra
}

#[test]
fn the_trust_cell_keys_a_directory_by_its_resolved_path() {
    let cells = FACE.trust.expect("Codex 有目录信任");
    let dir = test_home::scratch("codex-trust-cell-key");
    let real = dir.join("real");
    std::fs::create_dir_all(&real).expect("建目录");
    let cwd = real.to_string_lossy();
    assert_eq!((cells.dir_keys)(&cwd), trust_dir_keys(&cwd));
}

#[test]
fn trust_is_written_into_its_own_section_and_nothing_else_moves() {
    let TrustEdit::Write(out) = trust_edit(CONFIG, "/w/p") else {
        panic!("应写")
    };
    // 原文一行不动，只多那一节（放在别的 `[projects.…]` 旁边，跟 Codex 自己写进去的位置一样由 `toml_edit` 定）。
    assert_eq!(
        added_lines(CONFIG, &out),
        ["[projects.\"/w/p\"]", "trust_level = \"trusted\""]
    );
    assert_eq!(trust_edit(&out, "/w/p"), TrustEdit::Already);
}

#[test]
fn an_existing_section_gets_only_its_trust_line() {
    let TrustEdit::Write(out) = trust_edit(CONFIG, "/w/other") else {
        panic!("应写")
    };
    assert_eq!(
        out,
        CONFIG.replace("trust_level = \"untrusted\"", "trust_level = \"trusted\"")
    );
    let bare = "[projects.\"/w/q\"]\nsandbox = 1\n";
    let TrustEdit::Write(out) = trust_edit(bare, "/w/q") else {
        panic!("应写")
    };
    assert_eq!(added_lines(bare, &out), ["trust_level = \"trusted\""]);
}

#[test]
fn already_trusted_in_any_spelling_is_not_rewritten() {
    for c in [
        "[projects.\"/w/p\"]\ntrust_level = \"trusted\"\n",
        "[projects]\n\"/w/p\" = { trust_level = 'trusted' }\n",
        "projects.\"/w/p\".trust_level = \"trusted\"\n",
    ] {
        assert_eq!(trust_edit(c, "/w/p"), TrustEdit::Already, "{c}");
    }
}

#[test]
fn every_way_of_writing_the_section_is_edited_in_its_own_way() {
    // 那一项写成行内表 · 点号键 · 空的行内表 ⇒ 照它原来的写法补上那一格（`toml_edit` 改得动），读回来是 trusted。
    for c in [
        "[projects]\n\"/w/p\" = { sandbox = 1 }\n",
        "projects.\"/w/p\".sandbox = 1\n",
        "projects = { }\n",
    ] {
        let TrustEdit::Write(out) = trust_edit(c, "/w/p") else {
            panic!("应写：{c}")
        };
        assert_eq!(trust_edit(&out, "/w/p"), TrustEdit::Already, "{c} ⇒ {out}");
    }
}

#[test]
fn unreadable_or_not_a_table_is_left_alone() {
    // 原文读不懂 · 半路那一格不是表 ⇒ 不写。
    for c in [
        "[projects.\"/w/p\"\n",
        "a = \"unterminated\n",
        "projects = 1\n",
        "[projects]\n\"/w/p\" = \"x\"\n",
        "a = 1\na = 2\n",
    ] {
        assert_eq!(trust_edit(c, "/w/p"), TrustEdit::Unreadable, "{c}");
    }
}

#[test]
fn a_directory_with_quotes_and_backslashes_is_escaped() {
    let TrustEdit::Write(out) = trust_edit("", r#"C:\w\"q""#) else {
        panic!("应写")
    };
    // 键怎么转义由 `toml_edit` 定（这里写成字面串），读回来就是那个目录。
    assert_eq!(trust_edit(&out, r#"C:\w\"q""#), TrustEdit::Already, "{out}");
    assert_eq!((trust().trusted)(&out), Ok(set(&[r#"C:\w\"q""#])));
}

#[test]
fn the_trust_key_is_the_resolved_working_directory() {
    let dir = test_home::scratch("codex-trust-key");
    let real = dir.join("real");
    std::fs::create_dir_all(&real).expect("建目录");
    let keys = trust_dir_keys(&real.to_string_lossy());
    let want = std::fs::canonicalize(&real).expect("解开");
    let want = want.to_string_lossy();
    let want = if cfg!(windows) {
        want.trim_start_matches(r"\\?\").to_ascii_lowercase()
    } else {
        want.to_string()
    };
    assert_eq!(keys, vec![want]);
    // 解不开（不在）⇒ 原样。
    assert_eq!(
        trust_dir_keys("/no/such/dir-x"),
        vec!["/no/such/dir-x".to_string()]
    );
}

// ═══ 账号库布局 ═══

#[test]
fn the_face_is_the_codex_home_layout() {
    assert_eq!(FACE.config_file, "config.toml");
    assert_eq!(FACE.session_env.config_dir, "CODEX_HOME");
    assert_eq!(super::super::ADAPTER.account_env, Some("CODEX_HOME"));
    // Codex 不从环境读中转地址（经 `-c openai_base_url=…` 交）。
    assert_eq!(FACE.session_env.base_url, None);
    assert!(FACE.identity.iter().any(|(n, r, c)| *n == "auth.json"
        && *r == crate::agents::IdentityRoot::ConfigDir
        && *c == crate::agents::IdentityClass::Secret));
    assert_eq!(
        (FACE.shared_root)(std::path::Path::new("/h")),
        std::path::Path::new("/h").join(".codex")
    );
}

#[test]
fn an_unterminated_multiline_string_ending_in_a_backslash_is_unreadable() {
    assert_eq!(trust_edit("a = \"\"\"x\\", "/w/p"), TrustEdit::Unreadable);
    assert_eq!(trust_edit("a = '''x", "/w/p"), TrustEdit::Unreadable);
}

#[test]
fn the_account_mcp_cell_is_config_toml_mcp_servers() {
    let cells = FACE.mcp.expect("Codex 各号之间同步 MCP");
    assert_eq!(cells.file, "config.toml");
    let text = "model = \"m\"\n\n[mcp_servers.a]\ncommand = \"x\"\n";
    let m = (cells.read)(text).unwrap();
    assert_eq!(m["a"], serde_json::json!({"command": "x"}));
    let out = (cells.edit)(text, "a", None).unwrap();
    assert!((cells.read)(&out).unwrap().is_empty());
    assert!(out.starts_with("model = \"m\"\n"), "{out}");
}

/// 续成换 `tokens.id_token` 之前那一核，跟 Codex 自己读它那一下一样严（codex-rs `rust-v0.167.0`
/// `codex_login::token_data::parse_chatgpt_jwt_claims`：取前三段、都不空；中段 URL-safe、不带 `=` 地解 base64；
/// 解出来照 `IdClaims` 反序列化 —— 认得的几格类型不对就整个解不开）。Codex 加载 `auth.json` 也走这一解：
/// 我们写进去一个它解不开的，那个号就登不上了 ⇒ 这一核不许比它宽（比它严只是这一趟不落盘）。
#[test]
fn the_id_token_check_takes_only_what_codex_itself_can_read() {
    let check = login().renewed_cells[0].check.expect("id_token 那一格有核");
    let ok = |p: Value| check(&jwt(&p));
    // 寻常那一形 · 什么声明都没有 · 认不得的格随便。
    assert!(ok(
        json!({"email": "a@example.test", "https://api.openai.com/auth": {"chatgpt_plan_type": "pro", "chatgpt_account_id": "acc"}})
    ));
    assert!(ok(json!({})));
    assert!(ok(json!({"other": [1, 2], "email": null})));
    assert!(ok(
        json!({"https://api.openai.com/auth": {"chatgpt_plan_type": "brand_new_plan", "chatgpt_account_is_fedramp": true}})
    ));
    // 认得的格类型不对 ⇒ Codex 解不开 ⇒ 不收。
    for bad in [
        json!({"email": 5}),
        json!({"https://api.openai.com/profile": {"email": true}}),
        json!({"https://api.openai.com/profile": "x"}),
        json!({"https://api.openai.com/auth": "x"}),
        json!({"https://api.openai.com/auth": {"chatgpt_plan_type": 3}}),
        json!({"https://api.openai.com/auth": {"chatgpt_user_id": 1}}),
        json!({"https://api.openai.com/auth": {"user_id": {}}}),
        json!({"https://api.openai.com/auth": {"chatgpt_account_id": []}}),
        json!({"https://api.openai.com/auth": {"chatgpt_account_is_fedramp": null}}),
        json!({"https://api.openai.com/auth": {"chatgpt_account_is_fedramp": "yes"}}),
    ] {
        assert!(!ok(bad.clone()), "{bad}");
    }
    // 外形：中段带 `=` 补齐 · 标准字母表的 `+` `/` ⇒ Codex 那种解法解不开；三段以上只看前三段（同 Codex）。
    let good = jwt(&json!({"email": "a@example.test"}));
    let mid = good.split('.').nth(1).unwrap().to_string();
    assert!(!check(&format!("h.{mid}==.s")));
    assert!(!check("h.ab+/.s"));
    assert!(check(&format!("{good}.extra")));
    assert!(!check("h..s") && !check("x.y") && !check(""));
}

/// 同一份 `config.toml`，目录信任那一格与 MCP 那一格「读不读得懂」判得一样：都经同一个读法（`toml_edit`）。
/// 样本是两套解析曾经判得不一样的那几种：重复键 · 重复表头 · 串里写了 TOML 不认的转义 · 同一个目录的表头写两遍。
#[test]
fn the_trust_cell_and_the_mcp_cell_agree_on_what_is_readable() {
    let t = trust();
    for c in [
        "a = 1\na = 2\n",
        "[x]\n[x]\n",
        "a = \"\\q\"\n",
        "[projects.\"/w/p\"]\ntrust_level = \"trusted\"\n[projects.\"/w/p\"]\n",
        CONFIG,
        "",
    ] {
        let mcp = super::super::mcp::servers_in(c).is_ok();
        assert_eq!(
            (t.trusted)(c).is_ok(),
            mcp,
            "信任读法与 MCP 读法判得不一样：{c:?}"
        );
        assert_eq!(
            (t.mark)(c, &set(&["/w/p"])).is_ok(),
            mcp,
            "信任改法与 MCP 读法判得不一样：{c:?}"
        );
    }
}
