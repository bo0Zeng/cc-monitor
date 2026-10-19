//! 要求：推送公钥是本机后端帧命令 `pubkey-push {machine}`（组请求照 `ssh-dial/machine.rs::resolve`，
//! 与 `remote-probe` 同形），写 `authorized_keys` 经那台（后端不在就经 SSH exec 那一次、只写这一件）；「monitor 零 SSH」。
//!
//! # 两条帧命令，两台各一条
//!
//! - **`pubkey-push {machine, jump?, pubKeyPath?}`**（本机常驻后端答）：读本机那份 `.pub`（显式给的 · 否则私钥同名 `.pub`）→
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

use crate::platform::fs::Ro;
use copy_core::copy_text;
use serde_json::Value;

use crate::common::door::{self, Door};
use crate::platform::shell::posix;

/// 推送结果：key 是新加的还是本就存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum PushOutcome {
    Added,
    Already,
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

/// 校验公钥行时的拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SanitizeKeyErr {
    Empty,
    Multiline,
    ControlChars,
    UnknownType,
    NoBody,
    BadBody,
}

impl SanitizeKeyErr {
    pub(crate) fn into_said(self) -> String {
        match self {
            Self::Empty => copy_text("rsPubkey.sanitizePublicKey.empty", &[]),
            Self::Multiline => copy_text("rsPubkey.sanitizePublicKey.multiline", &[]),
            Self::ControlChars => copy_text("rsPubkey.sanitizePublicKey.controlChars", &[]),
            Self::UnknownType => copy_text("rsPubkey.sanitizePublicKey.unknownType", &[]),
            Self::NoBody => copy_text("rsPubkey.sanitizePublicKey.noBody", &[]),
            Self::BadBody => copy_text("rsPubkey.sanitizePublicKey.badBody", &[]),
        }
    }
}

/// 校验 ＋ 规范化公钥行。取**唯一**非空行（多于一行非空 ⇒ `Err`，防第二行注入）；去首尾空白；
/// 拒空 / 含控制字符；粗校验形态（已知类型前缀 ＋ 至少一个 base64 主体）。回规范化单行 key。**纯函数**。
pub(crate) fn sanitize_public_key(raw: &str) -> Result<String, SanitizeKeyErr> {
    let non_empty: Vec<&str> = raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    match non_empty.len() {
        0 => return Err(SanitizeKeyErr::Empty),
        1 => {}
        _ => return Err(SanitizeKeyErr::Multiline),
    }
    let key = non_empty[0];
    if key.chars().any(|c| c.is_control()) {
        return Err(SanitizeKeyErr::ControlChars);
    }
    let mut parts = key.split_whitespace();
    let ktype = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("");
    if !is_known_key_type(ktype) {
        return Err(SanitizeKeyErr::UnknownType);
    }
    if blob.is_empty() {
        return Err(SanitizeKeyErr::NoBody);
    }
    // base64 字符集轻校验（粗校验收紧）：挡 `ssh-ed25519 !!!` 这类形态。
    if !blob
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
    {
        return Err(SanitizeKeyErr::BadBody);
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
pub(crate) fn answer_add(
    d: &dyn Door,
    req: &KeyAddReq,
) -> Result<KeyAdded, (&'static str, String)> {
    let key = sanitize_public_key(&req.key).map_err(|e| ("refused", e.into_said()))?;
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
    Ok(KeyAdded { outcome })
}

/// `authorized-keys-add` 的入参：一行公钥（本条自己也校验一遍）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyAddReq {
    pub(crate) key: String,
}

/// `authorized-keys-add` 的应答。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct KeyAdded {
    pub(crate) outcome: PushOutcome,
}

crate::wire::typed::codes! {
    /// `authorized-keys-add` 的码：那一行不是一把公钥 · 读写那份文件没成。
    pub(crate) enum KeyAddErr {
        Refused = "refused",
        IoFailed = "io_failed",
    }
}

/// `authorized-keys-add`（被写那台答）：算经本文件，写经文件管理面（CAS ＋ 建父目录 ＋ 700 / 600）。
pub(crate) struct AuthorizedKeysAdd;

impl crate::wire::typed::Command for AuthorizedKeysAdd {
    const NAME: &'static str = "authorized-keys-add";
    type Req = KeyAddReq;
    type Reply = KeyAdded;
    type Err = KeyAddErr;
}

impl crate::wire::typed::Handle for AuthorizedKeysAdd {
    type Needs = crate::wire::typed::needs::Files;
    fn run(
        req: KeyAddReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<KeyAdded, KeyAddErr>> + Send + 'static {
        let (_cx, needs) = cx.split();
        async move {
            let files = needs;

            answer_add(files, &req).map_err(crate::wire::typed::of_pair(KeyAddErr::IoFailed))
        }
    }
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
    /// 问那台后端 `authorized-keys-add {key}`，回它的结局。
    fn ask_add<'a>(
        &'a self,
        machine: &'a str,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<PushOutcome, String>> + Send + 'a>>;
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
        crate::dial::remote_ask::lock(&crate::dial::remote_ask::REACH).contains_key(machine)
    }

    fn ask_add<'a>(
        &'a self,
        machine: &'a str,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<PushOutcome, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let req = KeyAddReq {
                key: key.to_string(),
            };
            crate::dial::remote_ask::ask_json::<AuthorizedKeysAdd>(
                machine,
                &req,
                // 回包里没有时刻（只回 `outcome`）。
                &crate::Tz::default(),
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map(|r| r.outcome)
            .map_err(|s| s.message)
        })
    }

    fn exec_once<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>
    {
        Box::pin(async move {
            let got = crate::dial::remote_ask::capture_full(dial, command).await?;
            match got.exit_status {
                Some(0) => Ok(got.stdout),
                // 那台 shell 的 stderr 是原话：记一行日志，句子只说没成（这一跳的失败形没有原话位）。
                _ => Err(crate::common::said::IntoNote::into_note(
                    crate::common::said::Said::with_raw(
                        copy_text("bePubkey.exec.failed", &[]),
                        got.stderr.trim(),
                    ),
                )),
            }
        })
    }
}

/// `pubkey-push` 的入参：拨哪台那几格（同 `remote-probe`）＋ 本机那份 `.pub` 的路径。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct PubkeyPushReq {
    /// 那台的配置（设置页表单里那台，可能还没保存）。
    pub(crate) machine: crate::dial::pick::MachineConfig,
    /// 跳板那一台。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) jump: Option<crate::dial::pick::MachineConfig>,
    /// 本机那份 `.pub` 的路径；缺 / 空 ⇒ 私钥同名 `.pub`（两样都没有 ⇒ `refused`，界面让人挑文件）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) pub_key_path: Option<String>,
}

/// 走了哪条：那台后端的文件管理面 · 那一次 exec。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "lowercase")]
pub(crate) enum PushVia {
    Backend,
    Exec,
}

/// `pubkey-push` 的应答。
#[derive(Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PubkeyPushed {
    /// `added`（新加的）· `already`（本就有整行相等的一行，没写）。
    pub(crate) outcome: PushOutcome,
    /// 实际推的是哪一份（给人看）。
    pub(crate) pub_path: String,
    /// 走了哪条。
    pub(crate) via: PushVia,
}

crate::wire::typed::codes! {
    /// `pubkey-push` 的码：跳板查无 / 成环 · 那份 `.pub` 找不到 / 不是公钥 · 那台没加上。
    pub(crate) enum PushErr {
        BadJump = "bad_jump",
        Refused = "refused",
        Failed = "failed",
    }
}

/// `pubkey-push`（本机常驻后端答）：见头注。真异步（拨号 / 问那台）。
pub(crate) struct PubkeyPush;

impl crate::wire::typed::Command for PubkeyPush {
    const NAME: &'static str = "pubkey-push";
    type Req = PubkeyPushReq;
    type Reply = PubkeyPushed;
    type Err = PushErr;
}

impl crate::wire::typed::Handle for PubkeyPush {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: PubkeyPushReq,
        _cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<PubkeyPushed, PushErr>> + Send + 'static {
        async move {
            answer_push(&req, &Wire, &read_local_pub)
                .await
                .map_err(crate::wire::typed::of_pair(PushErr::Failed))
        }
    }
}

/// `pubkey-push` 的本体（可达表 · 读 `.pub` 由调用方给，判据用替身）。
pub(crate) async fn answer_push(
    req: &PubkeyPushReq,
    reach: &dyn Reach,
    read_pub: &(dyn Fn(&str) -> Result<String, (&'static str, String)> + Send + Sync),
) -> Result<PubkeyPushed, (&'static str, String)> {
    let pick = crate::dial::pick::Pick {
        machine: &req.machine,
        jump: req.jump.as_ref(),
        prefer: None,
    };
    let (machine, _jump) = pick.machines()?;
    let path = pub_key_path(req.pub_key_path.as_deref(), machine.key_path.as_deref())
        .map_err(|e| ("refused", e))?;
    let key = match sanitize_public_key(&read_pub(&path)?) {
        Ok(k) => k,
        Err(SanitizeKeyErr::UnknownType) => {
            return Err((
                "refused",
                copy_text("rsPubkey.pushPublicKey.unknownType", &[("path", &path)]),
            ));
        }
        Err(e) => return Err(("refused", e.into_said())),
    };
    let name = machine.name().to_string();
    let (outcome, via) = if reach.backend_up(&name) {
        let outcome = reach
            .ask_add(&name, &key)
            .await
            .map_err(|e| ("failed", e))?;
        (outcome, PushVia::Backend)
    } else {
        let out = reach
            .exec_once(&pick.dial_value(), authorized_keys_cmd(&key))
            .await
            .map_err(|e| ("failed", e))?;
        (
            parse_push_outcome(&out).map_err(|e| ("failed", e))?,
            PushVia::Exec,
        )
    };
    Ok(PubkeyPushed {
        outcome,
        pub_path: path,
        via,
    })
}

/// 生产那一个读口：本机那份 `.pub`（有界，超了就不是公钥）；`~` 按这台家目录展开（[`read_pub_file`]）。
pub(crate) fn read_local_pub(path: &str) -> Result<String, (&'static str, String)> {
    read_pub_file(path, crate::platform::paths::home_dir().as_deref())
}

/// `path` 那份 `.pub`：打头的 `~` 按 `home` 展开再读（[`crate::platform::paths::expand_tilde_from`]）；说给人看的路径照用户写的。
pub(crate) fn read_pub_file(
    path: &str,
    home: Option<&std::path::Path>,
) -> Result<String, (&'static str, String)> {
    let at = crate::platform::paths::expand_tilde_from(path, home);
    let said = |why: &str| {
        (
            "refused",
            copy_text("reason.fs.readFailed", &[("path", path), ("why", why)]),
        )
    };
    // 系统原话记一行日志；句子只带原因词。
    let io = |e: std::io::Error| {
        let (c, m) = said(&copy_core::io_reason(e.kind()));
        (
            c,
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(m, e)),
        )
    };
    let meta = Ro::metadata(&at).map_err(io)?;
    if meta.len() > PUB_READ_MAX {
        return Err(said(&copy_text("bePubkey.read.tooBig", &[])));
    }
    Ro::read_to_string(&at).map_err(io)
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/pubkey_tests.rs"]
mod tests;
