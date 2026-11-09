//! 要求：公钥推送进本机后端（`pubkey-push`），写 `authorized_keys` 只经那台后端（不在就没成）。
//! 判校验 · 规划 · 只问那台 · 被写那台经文件管理面落地。
use super::*;
use crate::assets::aliases::test_support::HomeDoor;
use serde_json::{json, Value};
use std::sync::Mutex;

// 线上那一份入参经 typed 那一处收（同分派），再交本体；回包写成线上那一份（判据照线上形比）。
async fn answer_push(
    args: &Value,
    reach: &dyn Reach,
    read_pub: &(dyn Fn(&str) -> Result<String, crate::common::said::Said> + Send + Sync),
) -> Result<Value, crate::common::said::Said> {
    let req: PubkeyPushReq = crate::wire::typed::take(args).map_err(|f| f.coded("bad_args"))?;
    super::answer_push(&req, reach, read_pub)
        .await
        .map(|r| serde_json::to_value(r).unwrap())
}

fn answer_add(d: &dyn Door, args: &Value) -> Result<Value, crate::common::said::Said> {
    let req: KeyAddReq = crate::wire::typed::take(args).map_err(|f| f.coded("bad_args"))?;
    super::answer_add(d, &req).map(|r| serde_json::to_value(r).unwrap())
}

/// 替身那一条路：记下问了哪台、交了什么；那台答什么由判据定。
struct FakeReach {
    answer: Result<PushOutcome, &'static str>,
    added: Mutex<Vec<(String, String)>>,
}

impl FakeReach {
    /// 那台后端在，答 `outcome`。
    fn answering(outcome: PushOutcome) -> FakeReach {
        FakeReach {
            answer: Ok(outcome),
            added: Mutex::new(Vec::new()),
        }
    }

    /// 那台后端不在：问它就回问不到那一句（码 `unreachable`，同 `remote_ask` 查无那一台）。
    fn unanswered() -> FakeReach {
        FakeReach {
            answer: Err(NOT_THERE),
            added: Mutex::new(Vec::new()),
        }
    }
}

const NOT_THERE: &str = "那台后端不在";

impl Reach for FakeReach {
    fn ask_add<'a>(
        &'a self,
        machine: &'a str,
        key: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<PushOutcome, crate::common::said::Said>>
                + Send
                + 'a,
        >,
    > {
        self.added
            .lock()
            .unwrap()
            .push((machine.to_string(), key.to_string()));
        let answer = self
            .answer
            .map_err(|s| crate::common::said::Said::new("unreachable", s.to_string()));
        Box::pin(async move { answer })
    }
}

const KEY: &str = "ssh-ed25519 AAAAC3Nza me@x";

fn args() -> Value {
    json!({
        "machine": { "host": "192.0.2.2", "label": "dev", "user": "u", "keyPath": "/k/id_ed25519" },
    })
}

/// ★★ **入口真的净化过公钥吗**（原 `the_push_entry_point_actually_sanitizes_the_key`，audit-0805）：喂一个空公钥 ⇒
/// 那台**问都不问**就被拒（净化第一条就是「公钥为空」）。净化没接上 ⇒ 它会带着空 key 往下走（替身记得到）。
#[tokio::test]
async fn the_push_entry_point_actually_sanitizes_the_key() {
    let r = FakeReach::answering(PushOutcome::Added);
    let err = answer_push(&args(), &r, &|_p| Ok(String::new()))
        .await
        .expect_err("空公钥竟然一路走通了 —— 净化没接上");
    assert!(
        err.said
            .contains(copy_core::copy_static!("rsPubkey.sanitizePublicKey.empty")),
        "拒绝了，但不是净化拒的：{err:?}"
    );
    assert!(
        r.added.lock().unwrap().is_empty(),
        "带着未净化的 key 越过了这一步"
    );
}

/// 🔴 **只问那台**：一律问它 `authorized-keys-add`（带净化过的 key、按那台的名字）；那台答了 ⇒ 那一句照它的结局写；
/// 问不到 ⇒ 那一句照问不到那一句、落到线上是 `failed`。读的 `.pub` 默认是私钥同名那一份。
/// 不再有往那台写 `authorized_keys` 的 shell 那一臂：问那台的那一条路（[`Reach`]）只有 `ask_add` 这一口（生产里是问那台后端的那一次子命令）。
#[tokio::test]
async fn it_only_ever_asks_that_backend_and_fails_when_it_is_not_there() {
    let read = |p: &str| {
        assert_eq!(p, "/k/id_ed25519.pub", "默认读的不是私钥同名那一份");
        Ok(format!("{KEY}\n"))
    };
    let up = FakeReach::answering(PushOutcome::Already);
    let v = answer_push(&args(), &up, &read).await.unwrap();
    assert_eq!(
        *up.added.lock().unwrap(),
        vec![("dev".to_string(), KEY.to_string())]
    );
    // 那一句后端写好（本来就有 · 带推的那一份），界面照摆；回包只这一格。
    assert_eq!(
        v,
        json!({ "said": copy_text(
            "machineCard.pushKey.already",
            &[("path", "/k/id_ed25519.pub")]
        ) })
    );

    // 那台后端不在（还没装 · 没接上）：照样只问它，问不到就是没成 —— 一次 exec 都不跑（「先装后端」）。
    let gone = FakeReach::unanswered();
    let err = answer_push(&args(), &gone, &read)
        .await
        .expect_err("那台后端不在，推公钥竟然成了");
    assert_eq!(
        *gone.added.lock().unwrap(),
        vec![("dev".to_string(), KEY.to_string())],
        "后端不在也只问那台"
    );
    assert_eq!(err.said, NOT_THERE);
    assert!(
        matches!(
            crate::wire::typed::of_fail_with(PushErr::Failed)(err),
            PushErr::Failed(_)
        ),
        "问不到那台，线上不是 failed"
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
    // 结局（本机后端配带路径那一句用）＋ 写好的那一句（不带路径，手机照摆）。
    assert_eq!(
        (first, second),
        (
            json!({ "outcome": "added", "said": copy_text("mobKeyPush.result.added", &[]) }),
            json!({ "outcome": "already", "said": copy_text("mobKeyPush.result.already", &[]) })
        )
    );
    assert_eq!(text, format!("{KEY}\n"));
    assert_eq!(
        (dm, fm),
        (0o700, 0o600),
        ".ssh 要 700、authorized_keys 要 600"
    );
    assert_eq!(before, after, "第二次是 already，一个字节都不该写");
    assert_eq!(
        answer_add(&d, &json!({ "key": "rm -rf /" }))
            .unwrap_err()
            .code
            .as_deref(),
        Some("refused"),
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

/// 并进 `authorized_keys` 的纯规划：整行相等才算已有 · 末行无换行先补一个 · 空文件 / 不在直接一行。
#[test]
fn authorized_keys_plan_appends_one_whole_line() {
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
    let r = FakeReach::answering(PushOutcome::Added);
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

/// 推公钥「认不出的类型」拒绝体只说「认不出这份公钥的类型」与文件名，不回文件内容（哨兵串）。
#[tokio::test]
async fn push_public_key_unknown_type_omits_sentinel_content_and_reports_file() {
    let sentinel = "SENTINEL_SECRET_TOKEN_PUBKEY_98765";
    let fake_reach = FakeReach::answering(PushOutcome::Added);
    let read = |_p: &str| Ok(format!("{sentinel} foo@bar\n"));
    let err = answer_push(&args(), &fake_reach, &read)
        .await
        .expect_err("应拒绝未知公钥类型");
    assert_eq!(err.code.as_deref(), Some("refused"));
    assert!(
        !err.said.contains(sentinel),
        "拒绝体中泄露了文件内容哨兵串：{}",
        err.said
    );
    assert!(
        err.said.contains("/k/id_ed25519.pub"),
        "拒绝体未包含文件名：{}",
        err.said
    );
    assert!(
        copy_core::copy_matches_with(
            "rsPubkey.pushPublicKey.unknownType",
            &[("path", "/k/id_ed25519.pub")],
            &err.said,
        ),
        "拒绝体未按规范使用文案表：{}",
        err.said
    );
}

/// 私钥框里写的 `~/.ssh/…` 推公钥时读的那份 `.pub` 也按这台家目录展开再读；读不出来那一句里的路径照用户写的。
#[test]
fn the_pub_file_beside_a_tilde_key_path_is_read_under_the_home() {
    let home = std::env::temp_dir().join(format!("ccm-pub-tilde-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh").join("id_t.pub"), "ssh-ed25519 AAAA t\n").unwrap();
    assert_eq!(
        read_pub_file("~/.ssh/id_t.pub", Some(&home)).map_err(|e| e.said),
        Ok("ssh-ed25519 AAAA t\n".to_string())
    );
    let said = read_pub_file("~/.ssh/nope.pub", Some(&home))
        .unwrap_err()
        .said;
    assert!(
        said.contains("~/.ssh/nope.pub"),
        "报错句里的路径不是用户写的那一形：{said}"
    );
    let _ = std::fs::remove_dir_all(&home);
}
