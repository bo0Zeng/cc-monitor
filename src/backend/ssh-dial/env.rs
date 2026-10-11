//! **平台的事从这几个口进**：钥匙从哪来 · 认下的 host key 记到哪 · ssh-agent · 复制详情那几行事实。
//!
//! 一个进程一份（[`install`]，宿主起来时装一次）：后端装它自己那一套（钥匙文件 · `~/.cc-monitor/known_hosts` · 本机 agent ·
//! 后端的时刻与机器）；手机装 JNI 那一套（库里的钥匙 / Keystore 签名 · 回调进 Room · 没有 agent）。
//! 没装就用 [`Env::bare`]：钥匙按路径读文件、认下的 host key 不记、没有 agent、远端一个字节都不许写 —— 宿主不许靠它。

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, OnceLock};

use copy_core::copy_text;
use russh::keys::agent::client::{AgentClient as Client, AgentStream};
use russh::keys::{PrivateKey, PublicKey};

use crate::Said;

/// 一次鉴权要的那样东西。
pub enum Credential {
    /// 私钥本体（桌面：读钥匙文件；手机：库里那份 PEM 解开）。
    Key(Box<PrivateKey>),
    /// 私钥不出外部（手机 Android Keystore）：给公钥、按需签名（[`ExternalSigner`]）。
    Signer(Arc<dyn ExternalSigner>),
    /// 密码（那台只收密码的机器）。
    Password(String),
}

/// 私钥在别处、只肯替我们签名的那一个（手机 Keystore 里不可导出的 ECDSA P-256）。
pub trait ExternalSigner: Send + Sync {
    /// 公钥（`ecdsa-sha2-nistp256`）。
    fn public_key(&self) -> PublicKey;
    /// 对 `data` 签名，回 **DER** 编码的 ECDSA 签名（Keystore 原样给的那一形）；换成 SSH 的格式是本 crate 的事（`connect.rs`）。
    fn sign(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Said>> + Send>>;
}

/// 拨号请求里那一格钥匙（`key_path`）⇒ 一样能鉴权的东西。
pub trait KeySource: Send + Sync {
    /// `key`：桌面是钥匙文件的路径；手机是那台机器的钥匙编号。读不出来 ⇒ 那一句（原因码 `key_unreadable`）。
    fn credential(&self, key: &str) -> Result<Credential, Said>;
}

/// 认下的 host key 记到哪（桌面：cc-monitor 那份 known_hosts，开终端那一跳认同一把；手机：回调进 Room）。
pub trait HostKeys: Send + Sync {
    /// `key` = OpenSSH 公钥那一行的前两段（算法 ＋ base64）。写不了只记日志，不拦这条连接。
    fn remember(&self, host: &str, port: u16, key: &str);
}

/// 连上的 agent（各平台的流类型抹成同一个）。
pub type AgentClient = Client<Box<dyn AgentStream + Send + Unpin + 'static>>;

/// ssh-agent（只桌面有）。
pub trait Agent: Send + Sync {
    /// `sock` = 请求带来的套接字路径（缺 ⇒ 宿主自己的那一份）。连不上 ⇒ 那一句 ＋ 原话。
    fn connect<'a>(
        &'a self,
        sock: Option<&'a str>,
    ) -> Pin<Box<dyn Future<Output = Result<AgentClient, Said>> + Send + 'a>>;
}

/// 复制详情那几行里宿主才知道的事实（时刻 · 机器）＋ 一个钟。
pub trait Facts: Send + Sync {
    /// 一次失败的复制详情：`cmd`（`dial` / `files <op>`）· 码（有就带）· 下层原话。
    fn detail(&self, cmd: &str, code: Option<&str>, raw: Option<&str>) -> String;
    /// 此刻（只用来拼「两个上传者不撞」的那段后缀，不驱动任何等待）。
    fn now(&self) -> std::time::SystemTime;
}

/// 一个进程的那一套。
pub struct Env {
    pub keys: Box<dyn KeySource>,
    pub host_keys: Box<dyn HostKeys>,
    /// `None` ⇒ 这个宿主没有 agent（手机）：没配钥匙就报「agent 连不上」。
    pub agent: Option<Box<dyn Agent>>,
    pub facts: Box<dyn Facts>,
    /// 远端写的规矩（后端：部署那两处；手机：任意路径）。
    pub files: crate::sftp::WriteRules,
}

static ENV: OnceLock<Env> = OnceLock::new();

/// 没装时读到的那一套（不占 [`ENV`] 那一格：早读一次不会让之后的 [`install`] 装不上）。
static BARE: LazyLock<Env> = LazyLock::new(Env::bare);

/// 宿主起来时装一次。已经装过 ⇒ 原样退回 `Err`。
pub fn install(env: Env) -> Result<(), Env> {
    ENV.set(env)
}

/// 这个进程的那一套（还没装 ⇒ [`Env::bare`]）。
pub fn get() -> &'static Env {
    ENV.get().unwrap_or(&BARE)
}

/// 装过了没有（宿主核自己那一套真的在用）。
pub fn installed() -> bool {
    ENV.get().is_some()
}

impl Env {
    /// 没装时的那一套：钥匙按路径读文件 · 认下的不记 · 没有 agent · 详情只写命令、码与原话 · 远端一个字节都不许写。
    pub fn bare() -> Env {
        Env {
            keys: Box::new(KeyFiles),
            host_keys: Box::new(Forget),
            agent: None,
            facts: Box::new(PlainFacts),
            files: crate::sftp::WriteRules::ReadOnly,
        }
    }
}

/// 钥匙是一个 OpenSSH 私钥文件的路径（桌面那一形）。
pub struct KeyFiles;

impl KeySource for KeyFiles {
    fn credential(&self, key: &str) -> Result<Credential, Said> {
        russh::keys::load_secret_key(key, None)
            .map(|k| Credential::Key(Box::new(k)))
            .map_err(|e| Said::with_raw(copy_text("beConnect.auth.keyLoad", &[("path", key)]), &e))
    }
}

/// 认下的 host key 不记。
pub struct Forget;

impl HostKeys for Forget {
    fn remember(&self, _host: &str, _port: u16, _key: &str) {}
}

/// 详情只写命令 · 码 · 原话（没有宿主的时刻与机器）。
pub struct PlainFacts;

impl Facts for PlainFacts {
    fn detail(&self, cmd: &str, code: Option<&str>, raw: Option<&str>) -> String {
        use copy_core::detail::{Detail, Label};
        Detail::new()
            .item(Label::Command, cmd)
            .maybe(Label::Code, code)
            .maybe(Label::Raw, raw)
            .render()
    }

    /// 没有宿主的钟 ⇒ 纪元零点（上传后缀那一段只剩 pid ⊕ 进程内计数，照样两趟不撞；真宿主都交自己的钟）。
    fn now(&self) -> std::time::SystemTime {
        std::time::UNIX_EPOCH
    }
}
