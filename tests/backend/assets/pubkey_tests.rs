//! 要求住址：主会话 09-28 裁 MIG-3b 报备 2 —— 公钥推送进本机后端（`pubkey-push`），写 `authorized_keys` 经那台（后端不在就一次 exec、只写这一件）。
//! 〔MIG-3b 续〕校验 · 规划 · 解记号 · 那一串 shell 的判据原住 `tests/frontend/shell/pubkey_tests.rs`，随实现搬来、期望一字未改。
use super::*;
use crate::assets::aliases::tests::HomeDoor;
use std::sync::Mutex;

/// 替身两条路：记下走了哪一条、交了什么。
struct FakeReach {
    up: bool,
    added: Mutex<Vec<(String, String)>>,
    execs: Mutex<Vec<(Value, String)>>,
    stdout: &'static str,
}

impl FakeReach {
    fn new(up: bool) -> FakeReach {
        FakeReach {
            up,
            added: Mutex::new(Vec::new()),
            execs: Mutex::new(Vec::new()),
            stdout: "ADDED\n",
        }
    }
}

type BoxFut<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;

impl Reach for FakeReach {
    fn backend_up(&self, _machine: &str) -> bool {
        self.up
    }
    fn ask_add<'a>(&'a self, machine: &'a str, key: &'a str) -> BoxFut<'a> {
        self.added
            .lock()
            .unwrap()
            .push((machine.to_string(), key.to_string()));
        Box::pin(async { Ok("already".to_string()) })
    }
    fn exec_once<'a>(&'a self, dial: &'a Value, command: String) -> BoxFut<'a> {
        self.execs.lock().unwrap().push((dial.clone(), command));
        let out = self.stdout.to_string();
        Box::pin(async move { Ok(out) })
    }
}

const KEY: &str = "ssh-ed25519 AAAAC3Nza me@x";

fn args() -> Value {
    json!({
        "machine": { "host": "10.0.0.2", "label": "dev", "user": "u", "keyPath": "/k/id_ed25519" },
        "saved": null,
        "jump": null,
    })
}

/// ★★ **入口真的净化过公钥吗**（原 `the_push_entry_point_actually_sanitizes_the_key`，audit-0805）：喂一个空公钥 ⇒
/// 两条路**一条都不走**就被拒（净化第一条就是「公钥为空」）。净化没接上 ⇒ 它会带着空 key 往下走（替身记得到）。
#[tokio::test]
async fn the_push_entry_point_actually_sanitizes_the_key() {
    for up in [true, false] {
        let r = FakeReach::new(up);
        let err = answer_push(&args(), &r, &|_p| Ok(String::new()))
            .await
            .expect_err("空公钥竟然一路走通了 —— 净化没接上");
        assert!(
            err.1.contains("公钥为空"),
            "拒绝了，但不是净化拒的：{err:?}"
        );
        assert!(
            r.added.lock().unwrap().is_empty() && r.execs.lock().unwrap().is_empty(),
            "带着未净化的 key 越过了这一步"
        );
    }
}

/// 🔴 **按此刻状态分两条路，不是失败退回**：那台在可达表里 ⇒ 只问它 `authorized-keys-add`（带净化过的 key、按那台的名字）；
/// 不在 ⇒ 只跑一次 exec（拨号请求是那三格原样、整串就是 [`authorized_keys_cmd`]）。读的 `.pub` 默认是私钥同名那一份。
#[tokio::test]
async fn it_asks_that_backend_when_it_is_up_and_execs_once_only_when_it_is_not() {
    let read = |p: &str| {
        assert_eq!(p, "/k/id_ed25519.pub", "默认读的不是私钥同名那一份");
        Ok(format!("{KEY}\n"))
    };
    let up = FakeReach::new(true);
    let v = answer_push(&args(), &up, &read).await.unwrap();
    assert_eq!(
        *up.added.lock().unwrap(),
        vec![("dev".to_string(), KEY.to_string())]
    );
    assert!(up.execs.lock().unwrap().is_empty(), "后端在还去 exec 了");
    assert_eq!(
        (v["outcome"].clone(), v["via"].clone()),
        (json!("already"), json!("backend"))
    );

    let down = FakeReach::new(false);
    let v = answer_push(&args(), &down, &read).await.unwrap();
    assert!(down.added.lock().unwrap().is_empty());
    let execs = down.execs.lock().unwrap();
    assert_eq!(execs.len(), 1, "只许一次 exec");
    assert_eq!(execs[0].0["machine"], args()["machine"]);
    assert_eq!(execs[0].1, authorized_keys_cmd(KEY));
    assert_eq!(
        (v["outcome"].clone(), v["via"].clone(), v["pubPath"].clone()),
        (json!("added"), json!("exec"), json!("/k/id_ed25519.pub"))
    );
}

/// 被写那台：经这台自己的文件管理面写 —— 落在 `<home>/.ssh/authorized_keys`（建父目录）、700 / 600；
/// 再加一次同一把 ⇒ `already`、一个字节不写（原 monitor 那一侧「经文件管理面落地」那一格，随实现搬来）。
#[cfg(unix)]
#[test]
fn the_key_lands_through_this_backends_file_face() {
    use std::os::unix::fs::PermissionsExt;
    let home = std::env::temp_dir().join(format!("mig3b-pubkey-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let d = HomeDoor(home.clone());
    let first = answer_add(&d, &json!({ "key": KEY })).expect("第一次");
    let before = std::fs::metadata(home.join(".ssh/authorized_keys"))
        .unwrap()
        .modified()
        .unwrap();
    let second = answer_add(&d, &json!({ "key": KEY })).expect("第二次");
    let after = std::fs::metadata(home.join(".ssh/authorized_keys"))
        .unwrap()
        .modified()
        .unwrap();
    let text = std::fs::read_to_string(home.join(".ssh/authorized_keys")).unwrap();
    let mode = |p: &str| {
        std::fs::metadata(home.join(p))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    let (dm, fm) = (mode(".ssh"), mode(".ssh/authorized_keys"));
    let _ = std::fs::remove_dir_all(&home);
    assert_eq!(
        (first["outcome"].clone(), second["outcome"].clone()),
        (json!("added"), json!("already"))
    );
    assert_eq!(text, format!("{KEY}\n"));
    assert_eq!(
        (dm, fm),
        (0o700, 0o600),
        "权限要与 shell 那一串的 chmod 逐条同"
    );
    assert_eq!(before, after, "第二次是 already，一个字节都不该写");
    assert_eq!(
        answer_add(&d, &json!({ "key": "rm -rf /" })).unwrap_err().0,
        "refused",
        "被写那台自己也净化"
    );
}

#[test]
fn sanitize_accepts_valid_keys() {
    assert_eq!(
        sanitize_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host\n").unwrap(),
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 user@host"
    );
    assert!(sanitize_public_key("ssh-rsa AAAAB3NzaC1yc2E comment").is_ok());
    assert!(sanitize_public_key("ecdsa-sha2-nistp256 AAAAE2VjZHNh x").is_ok());
    assert!(sanitize_public_key("sk-ssh-ed25519@openssh.com AAAAG x").is_ok());
    // 前后空行/空白仍取那一行并 trim
    assert_eq!(
        sanitize_public_key("\n  ssh-ed25519 AAAA key-comment  \n\n").unwrap(),
        "ssh-ed25519 AAAA key-comment"
    );
}

#[test]
fn sanitize_rejects_bad_input() {
    assert!(sanitize_public_key("").is_err()); // 空
    assert!(sanitize_public_key("   \n  \n").is_err()); // 全空白
                                                        // 双非空行 → 防第二行注入
    assert!(sanitize_public_key("ssh-ed25519 AAAA a\nssh-rsa BBBB b").is_err());
    // 控制字符(含 NUL)
    assert!(sanitize_public_key("ssh-ed25519 AA\0AA x").is_err());
    // 未知类型(误选私钥 / 任意文本 / 注入尝试)
    assert!(sanitize_public_key("rm -rf /").is_err());
    assert!(sanitize_public_key("-----BEGIN OPENSSH PRIVATE KEY-----").is_err());
    // 缺 base64 主体
    assert!(sanitize_public_key("ssh-ed25519").is_err());
    // base64 主体含非法字符
    assert!(sanitize_public_key("ssh-ed25519 !!! comment").is_err());
}

#[test]
fn cmd_follows_aterm_contract() {
    let c = authorized_keys_cmd("ssh-ed25519 AAAA user@host");
    assert!(c.contains("printf '%s\\n'"), "须 printf %s 而非 echo");
    assert!(!c.contains("echo "), "不得用 echo");
    assert!(c.contains("grep -qxF"), "须 grep -qxF 精确去重");
    assert!(c.contains("mkdir -p"));
    assert!(c.contains("chmod 700"));
    assert!(c.contains("chmod 600"));
    assert!(c.contains("ADDED"));
    assert!(c.contains("ALREADY"));
    // key 经 shell_quote 单引号包裹
    assert!(c.contains("'ssh-ed25519 AAAA user@host'"));
}

#[test]
fn cmd_escapes_single_quote_injection() {
    // 公钥正常不含单引号;异常输入下 shell_quote 须逃逸,不破坏命令结构。
    let c = authorized_keys_cmd("ssh-ed25519 AAAA a'b");
    assert!(c.contains(r"'\''"), "单引号须被 shell_quote 逃逸为 '\\''");
}

#[test]
fn parse_outcome_markers() {
    assert_eq!(parse_push_outcome("ADDED\n").unwrap(), PushOutcome::Added);
    assert_eq!(
        parse_push_outcome("ALREADY\n").unwrap(),
        PushOutcome::Already
    );
    assert!(parse_push_outcome("").is_err());
    assert!(parse_push_outcome("mkdir: cannot create: permission denied\n").is_err());
}

/// D-重要1 回归:在真实 `sh` 上跑生成的命令,验证「既有 key 无尾换行时追加不粘行」+
/// 幂等(同 key 再推 → ALREADY)。Windows CI 跳过(cfg unix)。
#[cfg(unix)]
#[test]
fn append_respects_newline_boundary_and_idempotent() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("ccm-f50-{}", std::process::id()));
    let ssh = dir.join(".ssh");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&ssh).unwrap();
    let ak = ssh.join("authorized_keys");
    // 既有 key,故意**不带尾换行**——触发粘行 bug 的条件。
    std::fs::write(&ak, "ssh-ed25519 AAAAEXISTING existing@host").unwrap();

    let key = "ssh-ed25519 AAAANEWKEY new@host";
    let run = |k: &str| {
        Command::new("sh")
            .arg("-c")
            .arg(authorized_keys_cmd(k))
            .env("HOME", &dir)
            .output()
            .unwrap()
    };

    let out = run(key);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("ADDED"),
        "首推应 ADDED"
    );
    let content = std::fs::read_to_string(&ak).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        lines.iter().any(|l| l.contains("AAAAEXISTING")),
        "既有 key 应保留独占一行: {content:?}"
    );
    assert!(
        lines.iter().any(|l| *l == key),
        "新 key 应独占一行: {content:?}"
    );
    assert!(
        !content.contains("AAAAEXISTINGssh-ed25519"),
        "两把 key 不得粘成一行: {content:?}"
    );

    // 同 key 再推 → 幂等 ALREADY,不重复追加。
    let out2 = run(key);
    assert!(
        String::from_utf8_lossy(&out2.stdout).contains("ALREADY"),
        "重复推同 key 应 ALREADY"
    );
    let n = std::fs::read_to_string(&ak)
        .unwrap()
        .matches("AAAANEWKEY")
        .count();
    assert_eq!(n, 1, "新 key 不应被追加两次");

    std::fs::remove_dir_all(&dir).ok();
}

/// 〔SH1〕并进 `authorized_keys` 的纯规划：整行相等才算已有（`grep -qxF` 同义）· 末行无换行先补一个 · 空文件 / 不在直接一行。
#[test]
fn authorized_keys_plan_matches_the_shell_line() {
    let k = "ssh-ed25519 AAAAC3Nza me@x";
    assert_eq!(
        plan_authorized_keys(None, k),
        (PushOutcome::Added, Some(format!("{k}\n")))
    );
    assert_eq!(
        plan_authorized_keys(Some(""), k),
        (PushOutcome::Added, Some(format!("{k}\n")))
    );
    assert_eq!(
        plan_authorized_keys(Some("ssh-rsa AAAA old"), k),
        (PushOutcome::Added, Some(format!("ssh-rsa AAAA old\n{k}\n"))),
        "末行没换行 ⇒ 先补一个，不许把两把钥匙粘成一行"
    );
    assert_eq!(
        plan_authorized_keys(Some(&format!("a\n{k}\n")), k),
        (PushOutcome::Already, None)
    );
    assert_eq!(
        plan_authorized_keys(Some(&format!("{k} extra\n")), k).0,
        PushOutcome::Added,
        "前缀相同不算已有（整行相等）"
    );
}

/// 应答键集 == 金样 `tests/__fixtures__/pubkey-push.golden.json`（界面 `decodePush` 读同一份）；请求那一格也按金样发得通。
#[tokio::test]
async fn the_push_reply_has_exactly_the_golden_keys() {
    let golden: Value =
        serde_json::from_str(include_str!("../../__fixtures__/pubkey-push.golden.json")).unwrap();
    let r = FakeReach::new(false);
    let got = answer_push(&golden["request"], &r, &|_p| Ok(format!("{KEY}\n")))
        .await
        .unwrap();
    let keys = |v: &Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(keys(&got), keys(&golden["product"]));
    assert_eq!(got, golden["product"]);
}
