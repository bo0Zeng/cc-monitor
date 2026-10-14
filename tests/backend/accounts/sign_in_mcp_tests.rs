//! 在 app 里登录一个 MCP 服务器走一遍：假的 `claude mcp login`（`tests/__fixtures__/fake-claude-mcp-login.sh`，要终端）·
//! 假的 MCP 登录服务（本判据里起一个本机 HTTP：授权页 302 回跳到本机、令牌端点拿码换假令牌）· 沙箱家目录里两个号（z · b）。
//! 登在 z 上 ⇒ 那一家自己把令牌写进 z 的凭据文件；跑一趟各号之间的 MCP 登录同步 ⇒ b 也有那一条。核心自己一个字节令牌都不写。
//! 链接 · 码 · 令牌都是假串；没碰真家目录，没起真 claude。
#![cfg(unix)]

use super::*;
use crate::assets::aliases::tests::HomeDoor;
use crate::faces::accounts_face::{tests::answer, KeyDoor};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const CLAUDE_MCP: &str = include_str!("../../__fixtures__/fake-claude-mcp-login.sh");
/// 假服务发的码（只认这一个）与换来的令牌。
const GOOD_CODE: &str = "fake-code-0123456789";
const KEY: &str = "notes|0123456789abcdef";

/// 假的 MCP 登录服务：处理 `n` 个连接就收（判据末尾 join）。授权页 ⇒ 302 回跳 `redirect_uri?code=…&state=…`；
/// 令牌端点 ⇒ 码对 ⇒ 200 JSON，否则 400。
struct Service {
    base: String,
    done: std::thread::JoinHandle<()>,
}

fn serve(n: usize) -> Service {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    let done = std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut s, _)) = l.accept() else { return };
            let _ = s.set_read_timeout(Some(guard_core::wall::CEILING));
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            let head_end = loop {
                if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => break buf.len(),
                    Ok(k) => buf.extend_from_slice(&chunk[..k]),
                }
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let len: usize = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap_or(0))
                })
                .unwrap_or(0);
            while buf.len() < head_end + len {
                match s.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => buf.extend_from_slice(&chunk[..k]),
                }
            }
            let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
            let line = head.lines().next().unwrap_or("").to_string();
            let reply = if let Some(q) = line.strip_prefix("GET /authorize?") {
                let q = q.split(' ').next().unwrap_or("");
                let get = |k: &str| {
                    q.split('&')
                        .find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
                        .unwrap_or("")
                };
                let back = get("redirect_uri").replace("%3A", ":").replace("%2F", "/");
                format!(
                    "HTTP/1.1 302 Found\r\nLocation: {back}?code={GOOD_CODE}&state={}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    get("state")
                )
            } else if line.starts_with("POST /token") && body.trim() == format!("code={GOOD_CODE}")
            {
                let j = r#"{"access_token":"fake-at-mcp-1","refresh_token":"fake-rt-mcp-1","expires_in":3600}"#;
                format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{j}", j.len())
            } else {
                "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string()
            };
            let _ = s.write_all(reply.as_bytes());
        }
    });
    Service { base, done }
}

/// 浏览器那一步：打开授权页，拿它回跳的那一整条地址（地址栏里那一条）。
fn browse(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_at(rest.find('/').unwrap());
    let mut s = std::net::TcpStream::connect(host).unwrap();
    s.set_read_timeout(Some(guard_core::wall::CEILING)).unwrap();
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").unwrap();
    let mut got = String::new();
    s.read_to_string(&mut got).unwrap();
    got.lines()
        .find_map(|l| l.strip_prefix("Location: "))
        .unwrap_or_else(|| panic!("授权页没回跳：{got}"))
        .trim()
        .to_string()
}

struct Rig {
    root: PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Rig {
    /// 沙箱家目录：两个号（z 是从现有登录收成的默认号，b 是加的号），`bin/claude` 是假的登录子命令（指向 `auth`）。
    fn new(tag: &str, auth: &str) -> Rig {
        let root = std::env::temp_dir().join(format!(
            "mcplogin-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|x| x.as_nanos())
                .unwrap_or(0)
        ));
        let rig = Rig { root };
        rig.write(
            "bin/claude",
            &CLAUDE_MCP.replacen("\n", &format!("\nAUTH={auth}\n"), 1),
        );
        std::fs::set_permissions(
            rig.root.join("bin/claude"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        rig.write(".claude/.credentials.json", "{\"fake\":\"cred\"}");
        rig.write(".claude/settings.json", "{}");
        rig.write(
            ".claude.json",
            "{\"oauthAccount\":{\"emailAddress\":\"z@example.test\"},\"mcpServers\":{}}",
        );
        rig.call("accounts-init", json!({ "name": "z" }));
        rig.call(
            "accounts-add",
            json!({ "name": "b", "kind": "subscription" }),
        );
        rig.write(
            ".cc-monitor/accounts/b/.credentials.json",
            r#"{"claudeAiOauth":{"accessToken":"fake-at-b","refreshToken":"fake-rt-b","expiresAt":1900000000000}}"#,
        );
        rig.write(
            ".cc-monitor/accounts/z/mcp-needs-auth-cache.json",
            "{\"notes\":{\"timestamp\":1}}",
        );
        rig
    }

    fn write(&self, rel: &str, body: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn json(&self, rel: &str) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.root.join(rel)).unwrap_or_default())
            .unwrap_or(Value::Null)
    }

    fn door(&self) -> HomeDoor {
        HomeDoor(self.root.clone())
    }

    fn call(&self, cmd: &str, args: Value) {
        let table = self.root.join(".cc-monitor/apikey-credentials.json");
        let own =
            || crate::platform::fs::Own::issue_data_at(table.parent().expect("表所在的那一层"));
        let set = |v: &crate::accounts::upstream_select::file_face::KeySetReq| {
            crate::accounts::upstream_select::file_face::answer_set_at(&own(), v)
        };
        let drop =
            |v: &Value| crate::accounts::upstream_select::file_face::answer_drop_at(&own(), v);
        let restore =
            |v: &Value| crate::accounts::upstream_select::file_face::answer_restore_at(&own(), v);
        let path = || Ok(table.clone());
        let keys = KeyDoor {
            set: &set,
            drop: &drop,
            restore: &restore,
            path: &path,
        };
        answer(&self.door(), cmd, &args, &keys).unwrap_or_else(|e| panic!("{cmd}：{e:?}"));
    }

    fn dir(&self, acct: &str) -> String {
        self.root
            .join(".cc-monitor/accounts")
            .join(acct)
            .display()
            .to_string()
    }

    /// 等假的那一家打完第 `n` 次提示符（它每次打完、要读之前往沙箱家的 `.fake-mcp-asked` 记一行）。
    /// 粘在提示符之前 ⇒ 终端的回显照样有、照样打码，只是落在它还没打完的那几行中间（CI 上见过）；要核回显那一行就得先等它真在问。
    fn asked(&self, n: usize) {
        let p = self.root.join(".fake-mcp-asked");
        let seen = || {
            std::fs::read_to_string(&p)
                .map(|s| s.lines().count())
                .unwrap_or(0)
        };
        for _ in guard_core::wall::rounds() {
            if seen() >= n {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen() >= n, "假的那一家没打出第 {n} 次提示符");
    }

    fn start(&self, target: &'static str, acct: &str) -> Result<Started, Said> {
        let dir: &'static str = Box::leak(self.dir(acct).into_boxed_str());
        let then: &'static [String] = Box::leak(vec!["notes".to_string()].into_boxed_slice());
        let a = SignInAsk {
            face: crate::agents::claudecode::sign_in::MCP_FACE,
            target,
            dir: Some(dir),
            then,
            conn: 900,
            cwd: Some(self.root.clone()),
            path: Some(format!("{}:/usr/bin:/bin", self.root.join("bin").display()).into()),
        };
        start(&a, Box::new(|_, _| {}))
    }
}

fn creds_rel(acct: &str) -> String {
    format!(".cc-monitor/accounts/{acct}/.credentials.json")
}

/// ★ 登在 z 上：链接是假服务的授权页（终端里的超链接已剥成可见的字）；浏览器回跳那一整条粘回去 ⇒ 那一家拿码换令牌、
/// 写进 z 的凭据文件、删掉 z 的需登录缓存 ⇒ 成，回起它时的目标键。跑一趟各号之间的 MCP 登录同步 ⇒ b 也有那一条，令牌一样。
#[test]
fn a_mcp_login_in_one_account_lands_there_and_the_sync_carries_it_to_every_account() {
    let svc = serve(2);
    let rig = Rig::new("ok", &svc.base);
    let s = rig.start("mcp-ok", "z").expect("起得来");
    assert!(
        s.url.starts_with(&format!("{}/authorize?", svc.base)),
        "{}",
        s.url
    );
    assert!(!s.url.contains('\u{1b}'), "{}", s.url);
    let back = browse(&s.url);
    assert_eq!(finish(&s.job, &back), Ok("mcp-ok".to_string()));
    svc.done.join().unwrap();
    let z = rig.json(&creds_rel("z"));
    assert_eq!(z["mcpOAuth"][KEY]["accessToken"], "fake-at-mcp-1", "{z}");
    assert!(!rig
        .root
        .join(".cc-monitor/accounts/z/mcp-needs-auth-cache.json")
        .exists());
    assert!(
        rig.json(&creds_rel("b"))["mcpOAuth"].is_null(),
        "b 是同步之后才有"
    );
    let synced = crate::accounts::manage::mcp_logins_exec::sync(
        &rig.door(),
        &crate::platform::fs::Own::for_tests(),
    )
    .unwrap();
    assert_eq!(synced.changed, vec!["b".to_string()]);
    assert_eq!(
        rig.json(&creds_rel("b"))["mcpOAuth"][KEY],
        z["mcpOAuth"][KEY]
    );
}

/// 粘的不是这一趟的回跳地址 ⇒ 核心不写；那一家说「不像回跳地址」⇒ 作业还在；码被令牌端点拒 ⇒ `Refused`，
/// 原话里有它说的那一句，没有粘进去的码（终端把粘的那一行回显了，打成 `…`）。前两次核心就挡了、没写进去 ⇒ 它还在第一次提示符那儿；
/// 等它真在问了再粘第三次，回显才落在提示符那一行上。
#[test]
fn a_wrong_way_back_keeps_the_job_and_a_refused_code_never_shows_in_what_is_said() {
    let svc = serve(1);
    let rig = Rig::new("bad", &svc.base);
    let s = rig.start("mcp-bad", "z").expect("起得来");
    assert_eq!(
        finish(&s.job, "http://localhost:43117/callback?code=x&state=other"),
        Err(FinishFail::BadPaste)
    );
    assert_eq!(finish(&s.job, "abc#st9"), Err(FinishFail::BadPaste));
    let wrong = "http://localhost:43117/callback?code=wrong-code-9876543210&state=st9";
    rig.asked(1);
    match finish(&s.job, wrong) {
        Err(FinishFail::Refused(said)) => {
            let raw = said.raw.unwrap_or_default();
            assert!(raw.contains("the token endpoint refused the code"), "{raw}");
            assert!(
                raw.contains("Or paste the redirect URL here: …"),
                "回显那一行该在、打了码：{raw}"
            );
            assert!(
                !raw.contains("wrong-code-9876543210"),
                "粘进去的码进了原话：{raw}"
            );
        }
        other => panic!("本该被拒：{other:?}"),
    }
    svc.done.join().unwrap();
    assert!(
        rig.json(&creds_rel("z"))["mcpOAuth"].is_null(),
        "没登成就没有那一条"
    );
}

/// 不在终端里起（照账号那一形用管道）⇒ 那一家说「stdin 不是终端」退出 ⇒ 拿到了链接也登不成：MCP 那一形必须要终端。
#[test]
fn the_mcp_login_refuses_without_a_terminal() {
    let rig = Rig::new("pipe", "http://127.0.0.1:1");
    let mut face = crate::agents::claudecode::sign_in::MCP_FACE;
    face.terminal = false;
    let dir: &'static str = Box::leak(rig.dir("z").into_boxed_str());
    let then: &'static [String] = Box::leak(vec!["notes".to_string()].into_boxed_slice());
    let (tx, rx) = std::sync::mpsc::channel::<SignInEnd>();
    let tx = std::sync::Mutex::new(tx);
    let a = SignInAsk {
        face,
        target: "mcp-pipe",
        dir: Some(dir),
        then,
        conn: 901,
        cwd: Some(rig.root.clone()),
        path: Some(format!("{}:/usr/bin:/bin", rig.root.join("bin").display()).into()),
    };
    let _ = start(
        &a,
        Box::new(move |_, e| {
            let _ = tx.lock().unwrap().send(e.clone());
        }),
    );
    match rx.recv_timeout(guard_core::wall::CEILING) {
        Ok(SignInEnd::Failed(s)) => assert!(s.raw.unwrap_or_default().contains("isn't a terminal")),
        other => panic!("管道里本该登不成：{other:?}"),
    }
}
