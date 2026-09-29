//! 要求住址：主会话 09-28 裁 MIG-3b 报备 2 ——「`push_public_key`〔散文墓碑〕：本机后端帧命令 `pubkey-push {machine}`（组请求照 `dial/machine.rs::resolve`，
//! 与 `remote-probe` 同形），写 `authorized_keys` 经那台（后端不在就经 SSH exec 那一次、只写这一件）」；`设计/99 §2.1 ⑬`「monitor 零 SSH」。
//!
//! # 两条帧命令，两台各一条
//!
//! - **`pubkey-push {machine, saved?, jump?, pubKeyPath?}`**（本机常驻后端答）：读本机那份 `.pub`（显式给的 · 否则私钥同名 `.pub`）→
//!   [`sanitize_public_key`] → 那台此刻在可达表里（它的长连接握过手，`remote-reach`）⇒ 问**那台后端** `authorized-keys-add {key}`；
//!   不在（密钥登录建立之前 · 后端还没装）⇒ 沿池里那条 SSH **一次** exec（[`posix::add_line_once`]，只写这一件）。
//!   ⚠ 按**此刻的状态**分支，不是「后端失败了再退回 shell」（`D11`）。
//! - **`authorized-keys-add {key}`**（被写那台答）：[`plan_authorized_keys`] 算 → 经这台自己的文件管理面写（[`door::edit`]，CAS ＋ 建父目录）
//!   → `.ssh` 700 · `authorized_keys` 600（与 shell 那一串逐条同义）。
//!
//! 原住 monitor `pubkey.rs`（校验 · 规划 · 解记号三个纯函数与那一串 shell 逐字搬来）；那条 Tauri 命令与它的两条路删了。
//!
//! # 买不到的
//!
//! - 纯密码冷 onboarding：拨号只做 publickey / agent 鉴权（F61 已取消）⇒ 已有 key / agent 访问权时追加 / 轮换新公钥。
//! - `pubKeyPath` 是本机那台的路径（界面 · 本机后端同一台）；读它不经文件管理面（那是「这台自己的用户文件」的写面，读本机一份公钥不写）。

use copy_core::copy_text;
use serde_json::{json, Value};

use crate::assets::door::{self, Door};
use crate::platform::shell::posix;

/// 推送结果：key 是新加的还是本就存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushOutcome {
    Added,
    Already,
}

impl PushOutcome {
    fn as_str(self) -> &'static str {
        match self {
            PushOutcome::Added => "added",
            PushOutcome::Already => "already",
        }
    }
}

/// shell 那一串打的两个记号。
const ADDED: &str = "ADDED";
const ALREADY: &str = "ALREADY";

/// `authorized_keys` 在家目录底下住哪。
const SSH_DIR: &str = ".ssh";
const AUTH_FILE: &str = "authorized_keys";

/// 本机那份 `.pub` 一次最多读多少（公钥一行几百字节；超了就不是公钥）。
const PUB_READ_MAX: u64 = 64 * 1024;

/// 已知公钥类型前缀（粗校验形态，拒明显非公钥内容如误选私钥 / 任意文本）。
fn is_known_key_type(t: &str) -> bool {
    matches!(
        t,
        "ssh-ed25519" | "ssh-rsa" | "ssh-dss" | "ssh-ed25519-cert-v01@openssh.com"
    ) || t.starts_with("ecdsa-sha2-")
        || t.starts_with("sk-ssh-")
        || t.starts_with("sk-ecdsa-")
}

/// 校验 ＋ 规范化公钥行。取**唯一**非空行（多于一行非空 ⇒ `Err`，防第二行注入）；去首尾空白；
/// 拒空 / 含控制字符；粗校验形态（已知类型前缀 ＋ 至少一个 base64 主体）。回规范化单行 key。**纯函数**。
pub(crate) fn sanitize_public_key(raw: &str) -> Result<String, String> {
    let non_empty: Vec<&str> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    match non_empty.len() {
        0 => return Err(copy_text("rsPubkey.sanitizePublicKey.empty", &[])),
        1 => {}
        _ => return Err(copy_text("rsPubkey.sanitizePublicKey.multiline", &[])),
    }
    let key = non_empty[0];
    if key.chars().any(|c| c.is_control()) {
        return Err(copy_text("rsPubkey.sanitizePublicKey.controlChars", &[]));
    }
    let mut parts = key.split_whitespace();
    let ktype = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("");
    if !is_known_key_type(ktype) {
        return Err(copy_text(
            "rsPubkey.sanitizePublicKey.unknownType",
            &[("ktype", ktype)],
        ));
    }
    if blob.is_empty() {
        return Err(copy_text("rsPubkey.sanitizePublicKey.noBody", &[]));
    }
    // base64 字符集轻校验（粗校验收紧）：挡 `ssh-ed25519 !!!` 这类形态。
    if !blob
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    {
        return Err(copy_text("rsPubkey.sanitizePublicKey.badBody", &[]));
    }
    Ok(key.to_string())
}

/// 那一次 exec 的整串（key 经唯一的 quote 成词）。**纯函数**。
pub(crate) fn authorized_keys_cmd(key: &str) -> String {
    posix::add_line_once(
        SSH_DIR,
        AUTH_FILE,
        &shell_quote_core::posix_quote(key),
        ADDED,
        ALREADY,
    )
}

/// 那一次 exec 的 stdout → 结局。两个记号互斥（命令末尾只跑一个 `printf`）；都没有 ⇒ `Err`（`mkdir` / `chmod` 中途失败，命令链断在记号前）。
pub(crate) fn parse_push_outcome(output: &str) -> Result<PushOutcome, String> {
    if output.contains(ADDED) {
        Ok(PushOutcome::Added)
    } else if output.contains(ALREADY) {
        Ok(PushOutcome::Already)
    } else {
        Err(copy_text(
            "rsPubkey.parsePushOutcome.noMarker",
            &[("output", output.trim())],
        ))
    }
}

/// 把一行公钥并进 `authorized_keys` 的原文 —— 与 shell 那一串逐条同义：已有**整行相等**的一行 ⇒ `Already`、不写（`grep -qxF`）；
/// 否则末行没换行先补一个再追加 ⇒ `Added`。**纯函数**。
pub(crate) fn plan_authorized_keys(
    existing: Option<&str>,
    key: &str,
) -> (PushOutcome, Option<String>) {
    let text = existing.unwrap_or("");
    if text.lines().any(|l| l == key) {
        return (PushOutcome::Already, None);
    }
    let sep = if !text.is_empty() && !text.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    (PushOutcome::Added, Some(format!("{text}{sep}{key}\n")))
}

/// `authorized-keys-add {key}`（被写那台）：算 → 经这台自己的文件管理面写 → 收紧权限。
pub(crate) fn answer_add(d: &dyn Door, args: &Value) -> Result<Value, (&'static str, String)> {
    let raw = args.get("key").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `key` (string)"),
    ))?;
    let key = sanitize_public_key(raw).map_err(|e| ("refused", e))?;
    let home = door::home(d).map_err(|e| ("io_failed", e))?;
    let rel = format!("{SSH_DIR}/{AUTH_FILE}");
    let mut outcome = PushOutcome::Already;
    door::edit(d, &home, &rel, false, true, |existing| {
        let (o, next) = plan_authorized_keys(existing, &key);
        outcome = o;
        Ok(next)
    })
    .map_err(|e| ("io_failed", e))?;
    door::chmod(d, &home, SSH_DIR, 0o700).map_err(|e| ("io_failed", e))?;
    door::chmod(d, &home, &rel, 0o600).map_err(|e| ("io_failed", e))?;
    Ok(json!({ "outcome": outcome.as_str() }))
}

/// 本机那份 `.pub` 在哪：显式给的 ⇒ 它；否则私钥同名 `.pub`；都没有 ⇒ 说一句（界面让用户挑文件）。**纯函数**。
pub(crate) fn pub_key_path(
    explicit: Option<&str>,
    key_path: Option<&str>,
) -> Result<String, String> {
    explicit
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .or_else(|| {
            key_path
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|kp| format!("{kp}.pub"))
        })
        .ok_or_else(|| copy_text("rsPubkey.pushPublicKey.noKey", &[]))
}

/// 推给那台的两条路（生产 = [`Wire`]；判据用替身数「问了哪一条」）。
pub(crate) trait Reach: Send + Sync {
    /// 那台此刻在可达表里吗（它的长连接握过手）。
    fn backend_up(&self, machine: &str) -> bool;
    /// 问那台后端 `authorized-keys-add {key}`，回它的结局（`added` / `already`）。
    fn ask_add<'a>(
        &'a self,
        machine: &'a str,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;
    /// 沿那台的拨号请求跑**一次** exec，回 stdout（退出码非 0 ⇒ `Err` 带 stderr）。
    fn exec_once<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;
}

/// 生产那两条路：可达表 ＋ `remote_ask`（问那台后端）· `remote_ask::capture_full`（一次 exec）。
pub(crate) struct Wire;

impl Reach for Wire {
    fn backend_up(&self, machine: &str) -> bool {
        crate::stream::remote_ask::lock(&crate::stream::remote_ask::REACH).contains_key(machine)
    }

    fn ask_add<'a>(
        &'a self,
        machine: &'a str,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let v = crate::stream::remote_ask::ask_json(
                machine,
                "authorized-keys-add",
                &json!({ "key": key }),
                &crate::stream::remote_ask::REACH,
                &crate::stream::remote_ask::DialRemote,
            )
            .await
            .map_err(|s| s.message)?;
            v.get("outcome")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| copy_text("bePubkey.add.noOutcome", &[("machine", machine)]))
        })
    }

    fn exec_once<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let (got, _ack) = crate::stream::remote_ask::capture_full(dial, command).await?;
            match got.exit_status {
                Some(0) => Ok(got.stdout),
                _ => Err(copy_text(
                    "bePubkey.exec.failed",
                    &[("said", got.stderr.trim())],
                )),
            }
        })
    }
}

/// `pubkey-push {machine, saved?, jump?, pubKeyPath?}`（本机常驻后端）：见头注。回 `{outcome, pubPath, via}`（`via` = `backend` / `exec`，给人看走了哪条）。
pub(crate) async fn answer_push(
    args: &Value,
    reach: &dyn Reach,
    read_pub: &(dyn Fn(&str) -> Result<String, (&'static str, String)> + Send + Sync),
) -> Result<Value, (&'static str, String)> {
    let (machine, _saved, _jump) = crate::dial::machine::from_args(args)?;
    let path = pub_key_path(
        args.get("pubKeyPath").and_then(Value::as_str),
        machine.key_path.as_deref(),
    )
    .map_err(|e| ("refused", e))?;
    let key = sanitize_public_key(&read_pub(&path)?).map_err(|e| ("refused", e))?;
    let name = machine.name().to_string();
    let (outcome, via) = if reach.backend_up(&name) {
        let o = reach
            .ask_add(&name, &key)
            .await
            .map_err(|e| ("failed", e))?;
        match o.as_str() {
            "added" | "already" => (o, "backend"),
            _ => {
                return Err((
                    "failed",
                    copy_text("bePubkey.add.noOutcome", &[("machine", &name)]),
                ))
            }
        }
    } else {
        let dial = json!({
            "machine": args.get("machine"),
            "saved": args.get("saved"),
            "jump": args.get("jump"),
        });
        let out = reach
            .exec_once(&dial, authorized_keys_cmd(&key))
            .await
            .map_err(|e| ("failed", e))?;
        let o = parse_push_outcome(&out).map_err(|e| ("failed", e))?;
        (o.as_str().to_string(), "exec")
    };
    Ok(json!({ "outcome": outcome, "pubPath": path, "via": via }))
}

/// 生产那一个读口：本机那份 `.pub`（有界，超了就不是公钥）。
pub(crate) fn read_local_pub(path: &str) -> Result<String, (&'static str, String)> {
    let said = |e: String| {
        (
            "refused",
            copy_text(
                "rsPubkey.pushPublicKey.readFailed",
                &[("path", path), ("e", &e)],
            ),
        )
    };
    let meta = std::fs::metadata(path).map_err(|e| said(e.to_string()))?;
    if meta.len() > PUB_READ_MAX {
        return Err(said(copy_text("bePubkey.read.tooBig", &[])));
    }
    std::fs::read_to_string(path).map_err(|e| said(e.to_string()))
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/pubkey_tests.rs"]
mod tests;
