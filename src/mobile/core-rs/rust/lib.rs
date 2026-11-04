//! **手机嵌的 Rust 核心**（`libmobcore.so`）：只有 FFI 外壳。拨号 · 鉴权 · 复用 · 用法是后端同一份拨号核心（`ssh-dial`）。
//!
//! 边界是**手写 JNI、窄口、走消息**：一条链路就是拨号核心那一条（上行字节 → 下行阶段行 ＋ ack ＋ 字节），
//! Kotlin 只开 · 送 · 改窗口 · 关；分块传（`transfer.rs`）交的是文件描述符；回调三个：下行字节 · Keystore 签名 · 常驻流交的事（`Core.Sink`）。
//! 常驻流（重连 · 握手 · 版本门 · 在看名单 · 一问一答）整条在 `resident-client`（桌面同一份），宿主住 [`resident`]。
//! 不用 UniFFI / JNA（运行时多一份 JNA、两 ABI 多 2.2 MB，量过）。
//!
//! 进程里一份 tokio 多线程运行时（两个 worker，第一次用时建）；JNI 口都不阻塞。每个口在边界上接住 panic
//! （`panic = "unwind"`：嵌在 App 进程里，panic 不许 abort 整个 App）。

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use ssh_dial::env::{Credential, Env, ExternalSigner, Facts, HostKeys, KeySource};
use ssh_dial::sftp::WriteRules;
use ssh_dial::Said;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

pub mod attach;
pub mod batch;
mod busy;
mod copy;
mod host_keys;
pub mod install;
mod keys;
pub mod plan;
pub mod resident;
pub mod transfer;

/// 这一份核心的身份那一行（JSON）：拨号核心的 ack 协议版本 ＋ 认得的用法。Kotlin 侧冒烟与「装上的是不是这一份」用它。
pub fn version_json() -> String {
    serde_json::json!({
        "ack_v": ssh_dial::ACK_V,
        "uses": ssh_dial::USES,
    })
    .to_string()
}

// ═══ 进程里的几样 ═══════════════════════════════════════════════════════════════

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("mobcore")
            .enable_all()
            .build()
            .expect("建不起 tokio 运行时")
    })
}

/// Kotlin 那一侧（`Core.Sink` 的实现）：JVM ＋ 那个对象的全局引用（工作线程上找不到 App 的类，只能按对象调方法）。
struct Sink {
    vm: JavaVM,
    obj: GlobalRef,
}

static SINK: OnceLock<Sink> = OnceLock::new();

impl Sink {
    /// 在当前线程上（必要时挂上 JVM）调一下；Kotlin 抛了就清掉、按「没答」算。
    fn call<T>(
        &self,
        f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
    ) -> Option<T> {
        let mut env = self.vm.attach_current_thread_as_daemon().ok()?;
        let r = f(&mut env, self.obj.as_obj());
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }
        r.ok()
    }

    /// 一条链路的下行字节（`None` ⇒ 这条链路收工了）。
    fn data(&self, handle: i64, bytes: Option<&[u8]>) {
        self.call(|env, obj| {
            let arr = match bytes {
                Some(b) => JObject::from(env.byte_array_from_slice(b)?),
                None => JObject::null(),
            };
            env.call_method(
                obj,
                "data",
                "(J[B)V",
                &[JValue::Long(handle), JValue::Object(&arr)],
            )?;
            Ok(())
        });
    }
}

impl Sink {
    /// 一台常驻流交的事（`resident.rs`）：`(那台的编号, 种类, 文本)`。
    fn resident(&self, handle: i64, kind: &str, body: &str) {
        self.call(|env, obj| {
            let k = env.new_string(kind)?;
            let b = env.new_string(body)?;
            env.call_method(
                obj,
                "resident",
                "(JLjava/lang/String;Ljava/lang/String;)V",
                &[JValue::Long(handle), JValue::Object(&k), JValue::Object(&b)],
            )?;
            Ok(())
        });
    }
}

fn on_resident() -> resident::OnEvent {
    Arc::new(|h, kind, body| {
        if let Some(s) = sink() {
            s.resident(h, kind, &body);
        }
    })
}

fn sink() -> Option<&'static Sink> {
    SINK.get()
}

// ═══ 宿主那一套：钥匙 · host key · 详情 · 写的规矩 ══════════════════════════════════

/// Kotlin 交进来的一样钥匙。同样的一把只存一份（按内容认）；拨号请求的 `key_path` 写它的编号。
#[derive(PartialEq)]
enum Key {
    Pem(String, Option<String>),
    Password(String),
    Keystore(String, russh::keys::PublicKey),
}

/// 表里一格：那把钥匙 ＋ 握着它的连接身份有几份（Kotlin 每份身份交进来一次 · 跟着换一条时多握一次 · 收掉时放一次）。
struct Held {
    key: Key,
    holders: usize,
}

fn keys() -> &'static Mutex<HashMap<String, Held>> {
    static K: OnceLock<Mutex<HashMap<String, Held>>> = OnceLock::new();
    K.get_or_init(Default::default)
}

/// 交进一把钥匙、握一份，回它的编号：同样的那一把已在表里 ⇒ 同一个编号、多握一份。
fn put_key(key: Key) -> String {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let mut table = keys().lock().unwrap_or_else(|e| e.into_inner());
    if let Some((id, held)) = table.iter_mut().find(|(_, h)| h.key == key) {
        held.holders += 1;
        return id.clone();
    }
    let id = format!("k{}", NEXT.fetch_add(1, Ordering::Relaxed));
    table.insert(id.clone(), Held { key, holders: 1 });
    id
}

/// 多握一份 / 放掉一份；放掉的是最后一份 ⇒ 那把钥匙从表里删掉。编号不在表里 ⇒ `false`。
fn hold_key(id: &str, holding: bool) -> bool {
    let mut table = keys().lock().unwrap_or_else(|e| e.into_inner());
    let Some(held) = table.get_mut(id) else {
        return false;
    };
    if holding {
        held.holders += 1;
    } else {
        held.holders -= 1;
        if held.holders == 0 {
            table.remove(id);
        }
    }
    true
}

struct PhoneKeys;

impl KeySource for PhoneKeys {
    fn credential(&self, key: &str) -> Result<Credential, Said> {
        let unreadable = || copy_core::copy_text("beConnect.auth.keyLoad", &[("path", key)]);
        let table = keys().lock().unwrap_or_else(|e| e.into_inner());
        match table.get(key).map(|h| &h.key) {
            Some(Key::Pem(pem, pass)) => russh::keys::decode_secret_key(pem, pass.as_deref())
                .map(|k| Credential::Key(Box::new(k)))
                .map_err(|e| Said::with_raw(unreadable(), &e)),
            Some(Key::Password(p)) => Ok(Credential::Password(p.clone())),
            Some(Key::Keystore(alias, public)) => {
                Ok(Credential::Signer(Arc::new(KeystoreSigner {
                    alias: alias.clone(),
                    public: public.clone(),
                })))
            }
            None => Err(unreadable().into()),
        }
    }
}

/// Android Keystore 里不可导出的 P-256：签名回调进 Kotlin（`Sink.sign`，回 DER；换成 SSH 的格式是拨号核心的事）。
struct KeystoreSigner {
    alias: String,
    public: russh::keys::PublicKey,
}

impl ExternalSigner for KeystoreSigner {
    fn public_key(&self) -> russh::keys::PublicKey {
        self.public.clone()
    }

    fn sign(&self, data: Vec<u8>) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Said>> + Send>> {
        let alias = self.alias.clone();
        let der = sink()
            .and_then(|s| {
                s.call(|env, obj| {
                    let a = env.new_string(&alias)?;
                    let d = env.byte_array_from_slice(&data)?;
                    let r = env
                        .call_method(
                            obj,
                            "sign",
                            "(Ljava/lang/String;[B)[B",
                            &[JValue::Object(&a), JValue::Object(&d)],
                        )?
                        .l()?;
                    if r.is_null() {
                        return Ok(None);
                    }
                    Ok(Some(env.convert_byte_array(JByteArray::from(r))?))
                })
            })
            .flatten()
            .ok_or_else(|| Said::from(copy_core::copy_text("beConnect.auth.keyFailed", &[])));
        Box::pin(async move { der })
    }
}

/// 主机钥匙那份库（[`host_keys`]）：装核心时 Kotlin 交的那个路径，一个进程一份。
static BOOK: OnceLock<host_keys::PhoneHostKeys> = OnceLock::new();

/// 拨号核心认的那一口：转给 [`BOOK`]（还没装 ⇒ 一把都没有、记了也不留）。
struct Book;

impl HostKeys for Book {
    fn read(&self) -> Result<String, Said> {
        BOOK.get()
            .map_or(Ok(String::new()), host_keys::PhoneHostKeys::read)
    }

    fn rewrite(&self, edit: &mut dyn FnMut(&str) -> Option<String>) -> Result<(), Said> {
        BOOK.get().map_or(Ok(()), |b| b.rewrite(edit))
    }
}

/// 详情只写命令 · 码 · 原话（手机屏上「复制详情」用）；钟是这台手机的。
struct PhoneFacts;

impl Facts for PhoneFacts {
    fn detail(&self, cmd: &str, code: Option<&str>, raw: Option<&str>) -> String {
        ssh_dial::env::PlainFacts.detail(cmd, code, raw)
    }

    fn now(&self) -> std::time::SystemTime {
        std::time::SystemTime::now()
    }
}

/// 部署建目录收成的权限位：与后端交的 `own_dir::PRIVATE_DIR_MODE` 同值（本 crate 够不着后端，后端那边有一条判据钉相等）。
const DEPLOY_DIR_MODE: u32 = 0o700;

/// 装上手机那一套（一个进程一次）：钥匙在本表 · 主机钥匙那份库在 `known_hosts`（App 私有目录）· 没有 agent ·
/// 远端写只许部署那两处（文件屏走那台后端的 `files-*`，不经手机的 SFTP 写）。
fn install_env(known_hosts: std::path::PathBuf) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = BOOK.set(host_keys::PhoneHostKeys::new(known_hosts));
        let _ = ssh_dial::env::install(Env {
            keys: Box::new(PhoneKeys),
            host_keys: Box::new(Book),
            agent: None,
            facts: Box::new(PhoneFacts),
            files: WriteRules::Deploy {
                dir_mode: DEPLOY_DIR_MODE,
            },
        });
    });
}

// ═══ 钥匙本身（导入时派生公钥）═══════════════════════════════════════════════════════

/// 一把公钥（带上 [comment]）⇒ 交 Kotlin 的那一形：那一行 · 算法名 · 指纹（拨号核心那一份 `known_hosts::fingerprint`，桌面同一个）。
fn ssh_public(
    mut key: russh::keys::PublicKey,
    comment: &str,
) -> Option<cmd_wire::phone::SshPublicKey> {
    key.set_comment(comment);
    Some(cmd_wire::phone::SshPublicKey {
        line: key.to_openssh().ok()?,
        kind: key.algorithm().as_str().to_string(),
        fingerprint: ssh_dial::known_hosts::fingerprint(&key),
    })
}

/// 一段私钥 PEM（OpenSSH / PKCS#8 / …，不带口令）⇒ 它的公钥（[`ssh_public`]）。认不出 ⇒ `None`。
pub fn public_key_of(pem: &str, comment: &str) -> Option<cmd_wire::phone::SshPublicKey> {
    let key = russh::keys::decode_secret_key(pem, None).ok()?;
    ssh_public(key.public_key().clone(), comment)
}

/// 一个 P-256 未压缩点（`04 ‖ X ‖ Y`，Keystore 里那把的公钥）⇒ 它的公钥（[`ssh_public`]）。点坏了 ⇒ `None`。
pub fn public_key_of_point(point: &[u8], comment: &str) -> Option<cmd_wire::phone::SshPublicKey> {
    let ec = russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(point).ok()?;
    ssh_public(russh::keys::PublicKey::from(ec), comment)
}

/// 现生成一把软件 Ed25519（系统随机数做种子）：私钥 PEM（openssh-key-v1，无口令）· 它的公钥。
pub fn generate_ed25519(comment: &str) -> Option<cmd_wire::phone::KeyPair> {
    use ring::rand::SecureRandom as _;
    let mut seed = [0u8; 32];
    ring::rand::SystemRandom::new().fill(&mut seed).ok()?;
    let mut key = russh::keys::PrivateKey::from(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed),
    );
    key.set_comment(comment);
    let pem = key.to_openssh(russh::keys::ssh_key::LineEnding::LF).ok()?;
    Some(cmd_wire::phone::KeyPair {
        pem: pem.to_string(),
        public: ssh_public(key.public_key().clone(), comment)?,
    })
}

/// 一段 `~/.ssh/config` 文本 ⇒ 可导入主机的 JSON 数组（解析住拨号核心 `config_text`）。
pub fn ssh_config_json(text: &str) -> String {
    serde_json::to_string(&ssh_dial::config_text::parse(text)).unwrap_or_else(|_| "[]".to_string())
}

/// 手机那张机器表（`ssh_dial::machine::Host` 的 JSON 数组）；读不懂 ⇒ 那一句（契约错，不当空表）。
fn machine_table(table: &str) -> Result<Vec<ssh_dial::machine::Host>, String> {
    serde_json::from_str(table).map_err(|e| ssh_dial::malformed(&format!("machine table: {e}")))
}

/// 手机那张机器表 ＋ 名字 ⇒ 组拨号的结局（[`cmd_wire::phone::DialPlan`]）：组法是拨号核心那一份（`ssh_dial::machine::request`，
/// 跳板一路照表找、成环 · 找不到 ⇒ 拒，不回落直连）。每一跳的 `key_path` 是那一行交进来的原值（手机写那台的编号）。
pub fn dial_plan_json(table: &str, name: &str) -> String {
    use cmd_wire::phone::DialPlan;
    let plan = machine_table(table)
        .and_then(|hosts| {
            ssh_dial::machine::request(&hosts, name, None, "stream", serde_json::json!({}))
                .map_err(|s| s.said)
        })
        .and_then(|v| serde_json::from_value(v).map_err(|e| ssh_dial::malformed(&e.to_string())))
        .map_or_else(
            |said| DialPlan::Refused { said },
            |request| DialPlan::Ready { request },
        );
    serde_json::to_string(&plan).unwrap_or_default()
}

/// 手机那张机器表 ⇒ 一台一行（名字是拨号核心算的，[`ssh_dial::machine::MachineRow`]）；读不懂 ⇒ 空串（两端契约对不上，不当一台都没有）。
pub fn machines_json(table: &str) -> String {
    let Ok(hosts) = machine_table(table) else {
        return String::new();
    };
    let rows: Vec<_> = hosts
        .into_iter()
        .map(ssh_dial::machine::MachineRow::of)
        .collect();
    serde_json::to_string(&rows).unwrap_or_default()
}

/// 编辑页那张表（连正在编的那一台，第 [`at`] 行）⇒ 判得怎样 ＋ 那一台展开的拨号地址（[`cmd_wire::phone::MachineCheck`]）。
/// 判法 · 说法与桌面 `machines-try` 同一处（`ssh_dial::machine::table_fault`）；表读不懂 ⇒ 空串（不当没毛病）。
pub fn machine_check_json(table: &str, at: usize) -> String {
    let Ok(hosts) = machine_table(table) else {
        return String::new();
    };
    let check = cmd_wire::phone::MachineCheck {
        fault: ssh_dial::machine::table_fault(&hosts),
        endpoints: hosts.get(at).map(|h| h.endpoints()).unwrap_or_default(),
    };
    serde_json::to_string(&check).unwrap_or_default()
}

/// 文件页选中这几项（`files-ls` 回包 `up` 里的原行）· 此刻那一台（[`resident::here`]）⇒ 顶栏 · ⋯ · 点一下做什么
/// （`pick_core::PhoneMenu`）；选中那几行读不动 ⇒ 空串（不按「什么都没选」出菜单）。
pub fn pick_menu_json(rows: &str, here: &pick_core::Here) -> String {
    let Ok(rows) = serde_json::from_str::<Vec<cmd_wire::files::LsEntry>>(rows) else {
        return String::new();
    };
    let picks: Vec<pick_core::PickIn> = rows.iter().map(pick_core::PickIn::of_ls).collect();
    let picks: Vec<pick_core::Pick> = picks.iter().map(pick_core::PickIn::pick).collect();
    serde_json::to_string(&pick_core::phone_menu(&picks, here)).unwrap_or_default()
}

/// 一问一答撞了同名（码 `exists` 的 `data`：同名清单）⇒ 抽屉那几行（[`batch::clash_rows`]，一组那一问同一份）；读不动 ⇒ 空串（不当一项都不撞）。
pub fn clash_rows_json(data: &str) -> String {
    serde_json::from_str::<cmd_wire::batch::Conflicts>(data)
        .map(|c| serde_json::to_string(&batch::clash_rows(&c.conflicts)).unwrap_or_default())
        .unwrap_or_default()
}

/// 几台的搜内容结果合成一份：`{q, answered: [{origin, reply}]}`（`q` 搜的词；`reply` ＝ 那台 `history-search` 回包原样）⇒ `Merged`。
/// 合的那一步是本机后端扇出完调的同一份（`face_core::search::merge`）：手机逐台直连问完，在这里合，不经哪一台当信差。
/// 某台那一份照类型解不开 ⇒ 那台进 `down`（serde 那一句），别台照合；整份读不动 ⇒ 空串。
pub fn search_merge_json(data: &str) -> String {
    let Ok(mut ask) = serde_json::from_str::<serde_json::Value>(data) else {
        return String::new();
    };
    let (Some(q), Ok(all)) = (
        ask["q"].as_str().map(str::to_string),
        serde_json::from_value::<Vec<serde_json::Value>>(ask["answered"].take()),
    ) else {
        return String::new();
    };
    let mut answered = Vec::new();
    let mut down = Vec::new();
    for mut one in all {
        let Ok(origin) = serde_json::from_value::<cmd_wire::Machine>(one["origin"].take()) else {
            return String::new();
        };
        match serde_json::from_value::<cmd_wire::search::HistorySearchReply>(one["reply"].take()) {
            Ok(r) => answered.push((origin, r)),
            Err(e) => down.push(cmd_wire::search::SearchDown {
                origin,
                said: e.to_string(),
            }),
        }
    }
    serde_json::to_string(&face_core::search::merge(&q, answered, down)).unwrap_or_default()
}

/// 几台的组合成一份：`{answered: [{origin, reply}]}`（`reply` ＝ 那台 `groups-here` 回包原样，照机器表先后）⇒ `GroupsRead`。
/// 合的那一步是本机后端 `groups-read` 调的同一份（`face_core::groups::merge`）：手机逐台直连问完，在这里照组 id 合一次。
/// 某台那一片照类型解不开 ⇒ 那台读不到（serde 那一句）、别台照合；整份读不动 ⇒ 空串。
pub fn groups_merge_json(data: &str) -> String {
    use cmd_wire::groups::{GroupsHere, GroupsMachine};
    use cmd_wire::listing::HistoryMachineRead;
    use face_core::groups::Half;
    let Ok(mut ask) = serde_json::from_str::<serde_json::Value>(data) else {
        return String::new();
    };
    let Ok(all) = serde_json::from_value::<Vec<serde_json::Value>>(ask["answered"].take()) else {
        return String::new();
    };
    let mut halves = Vec::new();
    let mut heads = Vec::new();
    for mut one in all {
        let Ok(origin) = serde_json::from_value::<cmd_wire::Machine>(one["origin"].take()) else {
            return String::new();
        };
        let (half, read) = match serde_json::from_value::<GroupsHere>(one["reply"].take()) {
            Ok(h) => (Half::Now(h), HistoryMachineRead::Ok { notice: None }),
            Err(e) => (
                Half::None,
                HistoryMachineRead::Failed { why: e.to_string() },
            ),
        };
        heads.push(GroupsMachine {
            origin: origin.clone(),
            read,
        });
        halves.push((origin, half));
    }
    serde_json::to_string(&face_core::groups::merge(&halves, heads)).unwrap_or_default()
}

/// 会话屏勾上的那几台 ⇒ 并好的一列（`ChatsColumnAsk` ⇒ `ChatsColumn`；本机后端并历史页那一份并法，`face_core::column`）；读不动 ⇒ 空串。
pub fn chats_column_json(ask: &str) -> String {
    serde_json::from_str::<face_core::column::ChatsColumnAsk>(ask)
        .map(|a| serde_json::to_string(&face_core::column::column(&a)).unwrap_or_default())
        .unwrap_or_default()
}

// ═══ 链路表 ═════════════════════════════════════════════════════════════════════

struct Link {
    up: mpsc::UnboundedSender<Vec<u8>>,
    resize: mpsc::UnboundedSender<(u32, u32)>,
    task: tokio::task::JoinHandle<()>,
}

fn links() -> &'static Mutex<HashMap<i64, Link>> {
    static L: OnceLock<Mutex<HashMap<i64, Link>>> = OnceLock::new();
    L.get_or_init(Default::default)
}

/// 下行字节交到哪（`None` ⇒ 这条链路收工了）。
pub type OnData = Arc<dyn Fn(i64, Option<&[u8]>) + Send + Sync>;

/// 开一条链路（拨号请求 JSON，组好的那一形，同拨号核心 `DialRequest`）。回编号；请求读不动 ⇒ `Err(原话)`。
pub fn open_link(request: &str, on_data: OnData) -> Result<i64, String> {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    let req: ssh_dial::DialRequest = serde_json::from_str(request).map_err(|e| e.to_string())?;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (up_tx, mut up_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let (resize_tx, resize_rx) = mpsc::unbounded_channel();
    let rt = runtime();
    let _guard = rt.enter();
    let (mut up_w, up_r) = tokio::io::duplex(256 * 1024);
    let (mut down_w, mut down_r) = tokio::io::duplex(256 * 1024);
    // 上行：Kotlin 送来的块按序写进链路；Kotlin 关了（发端没了）⇒ 上行 EOF。
    rt.spawn(async move {
        while let Some(b) = up_rx.recv().await {
            if up_w.write_all(&b).await.is_err() {
                break;
            }
        }
    });
    // 开的那一段照共用循环那一个期限（[`opened_within`]）；开成 / 没开成写成交给 Kotlin 的阶段行与那一行 ack，开成之后按用法对拷。
    let task = rt.spawn(async move {
        let stages = ssh_dial::StageSink::new(req.stages);
        let control = ssh_dial::uses::Control {
            resize: Some(resize_rx),
        };
        let progress = resident_client::feed::Progress::default();
        let opened = opened_within(
            &progress,
            ssh_dial::uses::open(&req, &stages, |_| progress.step()),
        )
        .await;
        let ack = match &opened {
            Ok(o) => o.ack(),
            Err(f) => ssh_dial::DialAck::of_fail(f.clone()),
        };
        if ssh_dial::write_stages_then_ack(&mut down_w, &stages, &ack)
            .await
            .is_ok()
        {
            if let Ok(o) = opened {
                o.pump(up_r, &mut down_w, control).await;
            }
        }
        let _ = down_w.shutdown().await;
    });
    // 下行：读到的块原样交给 Kotlin；链路收工（含被 [`close_link`] 收掉）⇒ 交一个「没了」。
    rt.spawn(async move {
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            match down_r.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => on_data(id, Some(&buf[..n])),
            }
        }
        on_data(id, None);
    });
    links().lock().unwrap_or_else(|e| e.into_inner()).insert(
        id,
        Link {
            up: up_tx,
            resize: resize_tx,
            task,
        },
    );
    Ok(id)
}

/// **Kotlin 开的每一条链路（握着的那条 · 终端 · 文件 · 一次性 exec），开成之前那一段**：拨号核心每报一步进展就重新计拨号期限
/// （`resident_client::feed::dial_within`，共用循环开常驻流那一拨同一处；Kotlin 一个数都不写）。一步都没来的那个期限 ⇒
/// 当没开成（`reason: timeout`），那一拨随 `dial` 被丢掉一并收掉（挂在 SYN · banner 上的那条不留着占这个身份的拨号锁）。
async fn opened_within<T>(
    progress: &resident_client::feed::Progress,
    dial: impl std::future::Future<Output = Result<T, ssh_dial::DialFail>>,
) -> Result<T, ssh_dial::DialFail> {
    resident_client::feed::dial_within(progress, dial)
        .await
        .unwrap_or_else(|| {
            Err(ssh_dial::DialFail {
                said: copy_core::copy_text("reason.link.noAnswer", &[]),
                detail: Some(format!(
                    "{}s",
                    resident_client::backoff::DIAL_WITHIN.as_secs()
                )),
                fingerprint: None,
                reason: Some(ssh_dial::Why::Timeout),
                key_type: None,
                expected_fingerprint: None,
            })
        })
}

/// 往一条链路上行送字节。链路不在 ⇒ `false`。
pub fn send_link(id: i64, bytes: Vec<u8>) -> bool {
    links()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .is_some_and(|l| l.up.send(bytes).is_ok())
}

/// 改 PTY 窗口（只有 `use: shell` 那一形认它）。
pub fn resize_link(id: i64, cols: u32, rows: u32) {
    if let Some(l) = links().lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
        let _ = l.resize.send((cols, rows));
    }
}

/// 关一条链路：上行 EOF ＋ 收掉那一趟（池里那条连接别的链路还在用就留着）。
pub fn close_link(id: i64) {
    if let Some(l) = links()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id)
    {
        drop(l.up);
        l.task.abort();
    }
}

// ═══ JNI 口（`com.ccmonitor.mobile.core.rs.Core`）═════════════════════════════════════

fn string(env: &mut JNIEnv, s: &JString) -> Option<String> {
    if s.is_null() {
        return None;
    }
    env.get_string(s).ok().map(Into::into)
}

fn new_jstring(env: &JNIEnv, text: impl AsRef<str>) -> jstring {
    env.new_string(text)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn guarded<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// `Core.version()`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_version<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    new_jstring(&env, guarded("{}".to_string(), version_json))
}

/// `Core.init(sink, knownHosts)`：记下 Kotlin 那一侧、装上手机那一套（主机钥匙那份库在 `knownHosts`）。只认第一次。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_init<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    sink_obj: JObject<'l>,
    known_hosts: JString<'l>,
) {
    guarded((), || {
        if SINK.get().is_none() {
            if let (Ok(vm), Ok(obj)) = (env.get_java_vm(), env.new_global_ref(sink_obj)) {
                let _ = SINK.set(Sink { vm, obj });
            }
        }
        if let Some(p) = string(&mut env, &known_hosts) {
            install_env(p.into());
        }
    });
}

/// `Core.migrateHostKeys(rows)`：Room 旧表（平铺：主机 · 口 · 类型 · base64 四个一行）迁进库、读回比；对上 ⇒ `null`（Kotlin 清空旧表），
/// 对不上 ⇒ 那一句（之后拨号一律拒，下次启动再迁）。只活这一版。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_migrateHostKeys<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    rows: JObjectArray<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let rows = host_keys::OldRow::flat(&copy::strings(&mut env, &rows));
        match BOOK.get().and_then(|b| b.migrate(&rows)) {
            Some(said) => new_jstring(&env, said),
            None => std::ptr::null_mut(),
        }
    })
}

/// `Core.forgetAllHostKeys()`：迁移对不上那一句的出路（点了、确认过才调）：清空库、开闸；`true` ⇒ Kotlin 删旧表。没对不上 · 写不成 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_forgetAllHostKeys<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || match BOOK.get().map(|b| b.forget_all()) {
        Some(true) => JNI_TRUE,
        _ => JNI_FALSE,
    })
}

/// `Core.forgetHost(host, port)`：忘掉这台的主机钥匙（「钥匙变了」页上点了才调）。写不成 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_forgetHost<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    host: JString<'l>,
    port: jint,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let (Some(h), Ok(p)) = (string(&mut env, &host), u16::try_from(port)) else {
            return JNI_FALSE;
        };
        match BOOK.get().map(|b| b.forget(&h, p)) {
            Some(Ok(())) => JNI_TRUE,
            _ => JNI_FALSE,
        }
    })
}

/// `Core.putPassword(password)`：握一份，回编号；读不动 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putPassword<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    password: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || match string(&mut env, &password) {
        Some(p) => new_jstring(&env, put_key(Key::Password(p))),
        None => std::ptr::null_mut(),
    })
}

/// `Core.putPem(pem, passphrase?)`：握一份，回编号；读不动 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putPem<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pem: JByteArray<'l>,
    passphrase: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let pass = string(&mut env, &passphrase);
        let pem = env
            .convert_byte_array(&pem)
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        match pem {
            Some(pem) => new_jstring(&env, put_key(Key::Pem(pem, pass))),
            None => std::ptr::null_mut(),
        }
    })
}

/// `Core.putKeystore(alias, sec1Point)`：公钥是 P-256 的未压缩点（`04 ‖ X ‖ Y`）。握一份，回编号；认不出那个点 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putKeystore<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    alias: JString<'l>,
    point: JByteArray<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let pt = env.convert_byte_array(&point).ok();
        let (Some(alias), Some(pt)) = (string(&mut env, &alias), pt) else {
            return std::ptr::null_mut();
        };
        let Ok(ec) = russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(&pt) else {
            return std::ptr::null_mut();
        };
        new_jstring(
            &env,
            put_key(Key::Keystore(alias, russh::keys::PublicKey::from(ec))),
        )
    })
}

/// `Core.holdKey(id, holding)`：多握一份 / 放掉一份（最后一份放掉 ⇒ 删）。编号不在 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_holdKey<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: JString<'l>,
    holding: jboolean,
) -> jboolean {
    guarded(JNI_FALSE, || match string(&mut env, &id) {
        Some(id) if hold_key(&id, holding != JNI_FALSE) => JNI_TRUE,
        _ => JNI_FALSE,
    })
}

/// 一把公钥 ⇒ 它的 JSON（[`cmd_wire::phone::SshPublicKey`]）；没有 ⇒ `null`。
fn public_or_null(env: &JNIEnv, key: Option<cmd_wire::phone::SshPublicKey>) -> jstring {
    match key.and_then(|k| serde_json::to_string(&k).ok()) {
        Some(json) => new_jstring(env, json),
        None => std::ptr::null_mut(),
    }
}

/// `Core.publicKeyOf(pem, comment)`：导入一把私钥时派生它的公钥（JSON [`cmd_wire::phone::SshPublicKey`]）；认不出 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_publicKeyOf<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    pem: JByteArray<'l>,
    comment: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let pem = env
            .convert_byte_array(&pem)
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        let comment = string(&mut env, &comment).unwrap_or_default();
        public_or_null(&env, pem.and_then(|p| public_key_of(&p, &comment)))
    })
}

/// `Core.publicKeyOfPoint(point, comment)`：Keystore 里那把（P-256 未压缩点）的公钥（JSON [`cmd_wire::phone::SshPublicKey`]）；点坏了 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_publicKeyOfPoint<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    point: JByteArray<'l>,
    comment: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let pt = env.convert_byte_array(&point).ok();
        let comment = string(&mut env, &comment).unwrap_or_default();
        public_or_null(&env, pt.and_then(|p| public_key_of_point(&p, &comment)))
    })
}

/// `Core.generateEd25519(comment)`：回 JSON [`cmd_wire::phone::KeyPair`]；生成不了 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_generateEd25519<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    comment: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let comment = string(&mut env, &comment).unwrap_or_default();
        match generate_ed25519(&comment).and_then(|k| serde_json::to_string(&k).ok()) {
            Some(json) => new_jstring(&env, json),
            None => std::ptr::null_mut(),
        }
    })
}

/// `Core.parseSshConfig(text)`：可导入主机的 JSON 数组（`alias` · `host_name` · `port` · `user` · `identity_file`）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_parseSshConfig<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    text: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &text).unwrap_or_default();
        new_jstring(&env, ssh_config_json(&text))
    })
}

/// `Core.dialPlan(table, name)`：照机器表按名字组拨号的结局（[`dial_plan_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_dialPlan<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    table: JString<'l>,
    name: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let table = string(&mut env, &table).unwrap_or_default();
        let name = string(&mut env, &name).unwrap_or_default();
        new_jstring(&env, dial_plan_json(&table, &name))
    })
}

/// `Core.machines(table)`：机器表一台一行、带拨号核心算的名字（[`machines_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_machines<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    table: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let table = string(&mut env, &table).unwrap_or_default();
        new_jstring(&env, machines_json(&table))
    })
}

/// `Core.machineCheck(table, at)`：编辑页那张表判得怎样 ＋ 第 `at` 台展开的地址（[`machine_check_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_machineCheck<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    table: JString<'l>,
    at: jint,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let table = string(&mut env, &table).unwrap_or_default();
        new_jstring(
            &env,
            machine_check_json(&table, usize::try_from(at).unwrap_or(usize::MAX)),
        )
    })
}

/// `Core.pickMenu(handle, rows)`：文件页选中这几项（`files-ls` 的原行）⇒ 顶栏 · ⋯ · 点一下做什么（[`pick_menu_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_pickMenu<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    rows: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &rows).unwrap_or_default();
        env.new_string(pick_menu_json(&text, &resident::here(handle)))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `Core.clashRows(data)`：同名清单 ⇒ 抽屉那几行（[`clash_rows_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_clashRows<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    data: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &data).unwrap_or_default();
        env.new_string(clash_rows_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `Core.searchMerge(data)`：几台的搜内容结果合成一份（[`search_merge_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_searchMerge<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    data: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &data).unwrap_or_default();
        env.new_string(search_merge_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `Core.answerOf(keys, answer)`：手机那一侧问的同名（下载 · 上传）那一答 ⇒ 每个键一格（[`batch::decided_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_answerOf<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    keys: JString<'l>,
    answer: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let keys = string(&mut env, &keys).unwrap_or_default();
        let answer = string(&mut env, &answer);
        env.new_string(batch::decided_json(&keys, answer.as_deref()))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `Core.attachFace(want)`：聊天输入框上方那一排附件的字（[`attach::face_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_attachFace<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    want: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &want).unwrap_or_default();
        env.new_string(attach::face_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `PlanFace.face(want)`：手机计划三屏那一屏的字（[`plan::face_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_claude_link_PlanFace_face<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    want: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &want).unwrap_or_default();
        env.new_string(plan::face_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `ColumnFace.column(ask)`：会话屏几台并成一列（[`chats_column_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_claude_link_ColumnFace_column<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    ask: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &ask).unwrap_or_default();
        env.new_string(chats_column_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `GroupsFace.merge(data)`：几台的组合成一份（[`groups_merge_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_claude_link_GroupsFace_merge<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    data: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &data).unwrap_or_default();
        env.new_string(groups_merge_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    })
}

/// `Core.open(request)`：回链路编号；请求读不动 ⇒ 0（Kotlin 当它开不成）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_open<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    request: JString<'l>,
) -> jlong {
    guarded(0, || {
        let Some(req) = string(&mut env, &request) else {
            return 0;
        };
        let on_data: OnData = Arc::new(|id, b| {
            if let Some(s) = sink() {
                s.data(id, b);
            }
        });
        open_link(&req, on_data).unwrap_or(0)
    })
}

/// `Core.send(handle, bytes)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_send<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    bytes: JByteArray<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || match env.convert_byte_array(&bytes) {
        Ok(b) => {
            if send_link(handle, b) {
                JNI_TRUE
            } else {
                JNI_FALSE
            }
        }
        Err(_) => JNI_FALSE,
    })
}

/// `Core.resize(handle, cols, rows)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_resize<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    cols: jint,
    rows: jint,
) {
    guarded((), || {
        resize_link(
            handle,
            u32::try_from(cols).unwrap_or(80),
            u32::try_from(rows).unwrap_or(24),
        );
    });
}

/// `Core.close(handle)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_close<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) {
    guarded((), || close_link(handle));
}

// ═══ 常驻流的口（`resident.rs`；`Core.resident*`）═══════════════════════════════════════

/// `Core.residentOpen(machine, mine, tz)`：看一台，回编号。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentOpen<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    machine: JString<'l>,
    mine: JString<'l>,
    tz: JString<'l>,
) -> jlong {
    guarded(0, || {
        let machine = string(&mut env, &machine).unwrap_or_default();
        let mine = string(&mut env, &mine);
        let tz = string(&mut env, &tz);
        let _rt = runtime().enter();
        resident::open(machine, mine, tz, on_resident())
    })
}

/// `Core.residentSsh(handle, request)`：SSH 那层此刻的连接（拨号请求 JSON；`null` ⇒ 没通）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentSsh<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    request: JString<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let req = string(&mut env, &request);
        let _rt = runtime().enter();
        if resident::ssh(handle, req.as_deref()) {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    })
}

/// `Core.residentCall(handle, cmd, args, withinMs)`：回问答编号（结局经 `Sink.resident(…, "reply", …)`）；这台不在 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentCall<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    cmd: JString<'l>,
    args: JString<'l>,
    within_ms: jlong,
) -> jlong {
    guarded(0, || {
        let (Some(cmd), Some(args)) = (string(&mut env, &cmd), string(&mut env, &args)) else {
            return 0;
        };
        let _rt = runtime().enter();
        resident::call(
            handle,
            cmd,
            &args,
            u64::try_from(within_ms).unwrap_or(0),
            on_resident(),
        )
    })
}

/// `Core.residentWatch(handle, sid, holding)`：多一处 / 少一处在看这条会话。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentWatch<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    sid: JString<'l>,
    holding: jboolean,
) {
    guarded((), || {
        if let Some(sid) = string(&mut env, &sid) {
            resident::watch(handle, &sid, holding != JNI_FALSE);
        }
    });
}

/// `Core.residentKick(handle, retry)`：［重试］（`retry`）· 回前台 / 网络变了。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentKick<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    retry: jboolean,
) {
    guarded((), || resident::kick(handle, retry != JNI_FALSE));
}

/// `Core.deploy(handle, slots)`：给那台装 / 升级后端（`slots` = 包里那几格解到的文件，JSON）；事经 `Sink.resident(…, "deploy", …)` 交回。
/// 那台不在 · 正在装 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_deploy<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    slots: JString<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let slots = string(&mut env, &slots).unwrap_or_default();
        let _rt = runtime().enter();
        if install::start(handle, &slots, on_resident()) {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    })
}

/// Kotlin 交来的文件描述符（`ParcelFileDescriptor.detachFd()`）⇒ 归核心的一份文件，用完就关。不是 unix ⇒ 没有这一说。
pub(crate) fn file_of(fd: jint) -> Option<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd as _;
        // SAFETY: Kotlin 交出（detach）了这个描述符的所有权，此后只有这一份 `File` 持有它。
        (fd >= 0).then(|| unsafe { std::fs::File::from_raw_fd(fd) })
    }
    #[cfg(not(unix))]
    {
        let _ = fd;
        None
    }
}

/// `Core.transferStage(handle, fd, name, withinMs)`：本机那一份（`fd`，读）逐块送进那台的暂存区（`transfer.rs`）；
/// 回问答编号，走到哪经 `moved`、结局经 `reply` 交回。这台不在 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_transferStage<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    fd: jint,
    name: JString<'l>,
    within_ms: jlong,
) -> jlong {
    guarded(0, || {
        let Some(file) = file_of(fd) else { return 0 };
        let name = string(&mut env, &name).unwrap_or_default();
        let _rt = runtime().enter();
        transfer::stage(
            handle,
            file,
            name,
            u64::try_from(within_ms).unwrap_or(0),
            on_resident(),
        )
    })
}

/// `Core.transferRead(handle, path, cap, withinMs)`：那台的 `path`（线上 `WirePath` 的 JSON）逐块读进内存、过 `cap` 就停（`transfer.rs`）；
/// 回问答编号，走到哪经 `moved`、结局（`{bytes, content}`）经 `reply` 交回。这台不在 · 路径读不出 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_transferRead<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    path: JString<'l>,
    cap: jlong,
    within_ms: jlong,
) -> jlong {
    guarded(0, || {
        let Some(path) = string(&mut env, &path)
            .and_then(|p| serde_json::from_str::<cmd_wire::files::WirePath>(&p).ok())
        else {
            return 0;
        };
        let _rt = runtime().enter();
        transfer::read(
            handle,
            path,
            u64::try_from(cap).unwrap_or(0),
            u64::try_from(within_ms).unwrap_or(0),
            on_resident(),
        )
    })
}

/// `Core.transferStop(id)`：撤问答编号 `id` 那一趟（［停］）：在问的那一块当场丢下，本机那一份不再写；之后不回结局那一行。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_transferStop<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) {
    guarded((), || {
        transfer::halt(id);
    });
}

/// `Core.batchStart(handle, want)`：起一组（`batch.rs::plan_of` 那几种「要做什么」，入参由核心拼；`{kept}` ⇒ 接回落盘的那一组）；回编号，
/// 进度 · 同名那一问 · 结局经 `Sink.resident(…, "batch", …)` 回来，落盘那一份经 `"batch_keep"`。这台不在 · 读不动 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_batchStart<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    want: JString<'l>,
) -> jlong {
    guarded(0, || {
        let Some(want) = string(&mut env, &want) else {
            return 0;
        };
        let _rt = runtime().enter();
        batch::start(handle, &want, on_resident())
    })
}

/// `Core.batchAnswer(id, answer)`：同名那一问的答（`{all}` · `{each}`；`null` ⇒ 取消）。没人在等 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_batchAnswer<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
    answer: JString<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let text = string(&mut env, &answer);
        if batch::answer(id, text.as_deref()) {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    })
}

/// `Core.batchStop(id)`：撤这一组（同一张票发 `files-batch-stop`）。那一组不在 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_batchStop<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: jlong,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let _rt = runtime().enter();
        if batch::stop(id) {
            JNI_TRUE
        } else {
            JNI_FALSE
        }
    })
}

/// `Core.residentClose(handle)`：不再看这台。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_residentClose<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) {
    guarded((), || resident::close(handle));
}

#[cfg(test)]
mod tests {
    use resident_client::backoff::DIAL_WITHIN;

    /// 手机那张机器表（Room 一行一台，`keyPath` 写 Room 那台的 id）：照它交给核心的那一形。
    fn room(rows: &[(&str, &str, &str)]) -> String {
        let v: Vec<serde_json::Value> = rows
            .iter()
            .map(|(id, label, jump)| {
                serde_json::json!({"label": label, "host": format!("{label}.lan"), "port": 22, "user": "u",
                    "keyPath": id, "addresses": ["192.0.2.9:2222"], "jump": jump, "connect": true})
            })
            .collect();
        serde_json::to_string(&v).unwrap()
    }

    /// Room 里 A→B→A：连 A ⇒ 拒（之前手机截成 A→B 照连）；拒的那一句是拨号核心那一句。
    #[test]
    fn a_jump_loop_in_the_room_table_is_refused() {
        let table = room(&[("id-a", "A", "B"), ("id-b", "B", "A")]);
        let plan: cmd_wire::phone::DialPlan =
            serde_json::from_str(&super::dial_plan_json(&table, "A")).unwrap();
        let cmd_wire::phone::DialPlan::Refused { said } = plan else {
            panic!("A→B→A 该拒")
        };
        assert_eq!(said, copy_core::copy_text("beMachine.jump.loop", &[]));
    }

    /// 跳板找不到 · 表读不懂也拒；一跳照表组好：地址展开在核心（备用地址跟在首选后面），每一跳的 `key_path` 是那台的 id。
    #[test]
    fn the_room_table_plans_a_dial_by_name() {
        let refused = |table: &str| {
            matches!(
                serde_json::from_str(&super::dial_plan_json(table, "A")).unwrap(),
                cmd_wire::phone::DialPlan::Refused { .. }
            )
        };
        assert!(refused(&room(&[("id-a", "A", "Z")])));
        assert!(refused("{"));
        let table = room(&[("id-a", "A", "B"), ("id-b", "B", "")]);
        let plan: cmd_wire::phone::DialPlan =
            serde_json::from_str(&super::dial_plan_json(&table, "A")).unwrap();
        let cmd_wire::phone::DialPlan::Ready { request } = plan else {
            panic!("A→B 该组得出来")
        };
        assert_eq!(
            (request.host.as_str(), request.key_path.as_deref()),
            ("A.lan", Some("id-a"))
        );
        assert_eq!(
            request
                .endpoints
                .iter()
                .map(|e| format!("{}:{}", e.host, e.port))
                .collect::<Vec<_>>(),
            ["A.lan:22", "192.0.2.9:2222"]
        );
        let hop = request.jump.expect("经 B");
        assert_eq!(
            (
                hop.host.as_str(),
                hop.key_path.as_deref(),
                hop.jump.is_none()
            ),
            ("B.lan", Some("id-b"), true)
        );
    }

    /// 一张表 ⇒ 一台一行（名字是拨号核心算的）；判第几台：表那一道的毛病（桌面同一句）· 这一台展开的地址。
    #[test]
    fn the_table_reads_back_named_and_checked() {
        let table = room(&[("id-a", "", ""), ("id-b", "B", "")]);
        let rows: Vec<ssh_dial::machine::MachineRow> =
            serde_json::from_str(&super::machines_json(&table)).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            [".lan", "B"]
        );
        let twin = room(&[("id-a", "B", ""), ("id-b", "B", "")]);
        let c: cmd_wire::phone::MachineCheck =
            serde_json::from_str(&super::machine_check_json(&twin, 1)).unwrap();
        assert_eq!(
            c.fault.map(|f| (f.code, f.said)),
            Some((
                cmd_wire::machine::MachineFaultCode::NameTaken,
                copy_core::copy_text("addMachine.err.taken", &[("name", "B")])
            ))
        );
        assert_eq!(c.endpoints.len(), 2);
        let fine: cmd_wire::phone::MachineCheck =
            serde_json::from_str(&super::machine_check_json(&table, 1)).unwrap();
        assert_eq!(fine.fault, None);
    }

    /// 文件页那一问：交进 `files-ls` 的原行 ⇒ 顶栏 · ⋯（每项四格）· 点一下做什么；那台断着 ⇒ 灰着说离线；读不动 ⇒ 什么都不给。
    #[test]
    fn the_pick_menu_reads_the_ls_rows_as_they_came() {
        let rows = r#"[{"path":"/h/docs","shown":"~/docs","name":"docs","kind":"dir","class":"folder","type_text":"","mtime_secs":12}]"#;
        let up = pick_core::Here::default();
        let v: serde_json::Value = serde_json::from_str(&super::pick_menu_json(rows, &up)).unwrap();
        assert_eq!(v["bar"][0]["id"], "download");
        assert_eq!(v["bar"][0]["enabled"], false);
        assert!(v["bar"][0]["why"]
            .as_str()
            .is_some_and(|w| w.contains("docs")));
        assert_eq!(v["more"][0].as_object().map(|o| o.len()), Some(4));
        assert_eq!(v["tap"], "open");
        let off = pick_core::Here {
            offline: Some(pick_core::Cut::Machine),
            unavailable: vec![],
        };
        let v: serde_json::Value =
            serde_json::from_str(&super::pick_menu_json(rows, &off)).unwrap();
        assert_eq!(
            v["more"][0]["why"],
            copy_core::copy_text("acctPage.offline.hover", &[])
        );
        assert_eq!(
            super::pick_menu_json("{", &up),
            "",
            "选中那几行读不动 ⇒ 空串（契约错），不按「什么都没选」出菜单"
        );
    }

    /// 🔴 几个 JSON 口读不动一律回空串（Kotlin 那一侧照它报两端契约对不上），不交一份合法的空值冒充正常：
    /// 机器表读不懂 ≠ 一台都没有 · ≠ 没毛病；同名清单读不懂 ≠ 一项都不撞。
    #[test]
    fn an_unreadable_input_comes_back_empty_not_as_a_valid_nothing() {
        for (what, got) in [
            ("machines", super::machines_json("nope")),
            ("machine_check", super::machine_check_json("nope", 0)),
            ("clash_rows", super::clash_rows_json("{")),
            (
                "pick_menu",
                super::pick_menu_json("[1]", &pick_core::Here::default()),
            ),
        ] {
            assert_eq!(got, "", "{what}：读不动该回空串");
        }
        // 正控：读得懂的空表照回空表。
        assert_eq!(super::machines_json("[]"), "[]");
    }

    /// 一问一答撞了同名：清单 ⇒ 抽屉那几行（说明由核心写、答法照那台给的）。
    #[test]
    fn a_clash_list_becomes_the_rows_of_the_sheet() {
        let data = r#"{"conflicts":[{"key":"0","name":"up.bin","kind":"file","size_text":"9 字节","there":"file","there_size_text":"3 字节","answers":["overwrite","keep_both"]}]}"#;
        let v: serde_json::Value = serde_json::from_str(&super::clash_rows_json(data)).unwrap();
        let c: cmd_wire::batch::Conflicts = serde_json::from_str(data).unwrap();
        assert_eq!(
            v,
            serde_json::json!([{ "key": "0", "name": "up.bin",
                "meta": batch_core::conflict_meta(&c.conflicts[0]),
                "choices": batch_core::choices(&c.conflicts[0].answers), "there": null }])
        );
        assert_eq!(
            v[0]["choices"]
                .as_array()
                .map(|a| a.iter().map(|o| o["decide"].clone()).collect::<Vec<_>>()),
            Some(vec![
                serde_json::json!("keep_both"),
                serde_json::json!("overwrite")
            ]),
            "只出那台给了的答，先后照核心"
        );
    }

    use std::time::Duration;

    /// 一拨替身：每隔 `gaps` 秒报一步进展，`then` ＝ 最后开成（`true`）还是从此不动（`false`）；交 [`super::opened_within`]，
    /// 回它的结局 ＋ 在虚拟钟上用了多久。
    async fn within_of(gaps: Vec<u64>, then: bool) -> (Result<(), ssh_dial::DialFail>, Duration) {
        let progress = resident_client::feed::Progress::default();
        let p = progress.clone();
        let dial = async move {
            for gap in gaps {
                tokio::time::sleep(Duration::from_secs(gap)).await;
                p.step();
            }
            if !then {
                std::future::pending::<()>().await;
            }
            Ok(())
        };
        let t0 = tokio::time::Instant::now();
        let got = super::opened_within(&progress, dial).await;
        (got, t0.elapsed())
    }

    /// **Kotlin 开的每一条链路，开成之前照共用循环那一个期限**（`resident_client::feed::dial_within`）：慢链路上每一步隔 8 秒、合计远超
    /// 期限也照样等到开成（进展是拨号核心交的回调，Kotlin 一行也看不到）。
    #[tokio::test(start_paused = true)]
    async fn a_slow_dial_that_keeps_moving_gets_opened() {
        let (got, took) = within_of(vec![8, 8, 8, 8], true).await;
        assert!(got.is_ok(), "{got:?}");
        assert_eq!(took, Duration::from_secs(32));
    }

    /// 一步都不动的那一拨：最后一步之后 [`DIAL_WITHIN`] 当没开成（原因 `timeout`，那一句是拨号超时那一句），那一拨随之收掉
    /// （挂在 SYN · banner 上的那一条不留着占这个身份的拨号锁）。
    #[tokio::test(start_paused = true)]
    async fn a_dial_that_stops_moving_fails_with_timeout_one_window_after_its_last_step() {
        let (got, took) = within_of(vec![3], false).await;
        assert_eq!(took, Duration::from_secs(3) + DIAL_WITHIN);
        let f = got.expect_err("不动的那一拨居然开成了");
        assert_eq!(f.reason, Some(cmd_wire::machine::Why::Timeout));
        assert_eq!(f.said, copy_core::copy_text("reason.link.noAnswer", &[]));
    }

    /// 表里与 `key` 同样的那一把有几格、共被握着几份。
    fn copies_of(key: &super::Key) -> (usize, usize) {
        let table = super::keys().lock().unwrap();
        let same: Vec<_> = table.values().filter(|h| h.key == *key).collect();
        (same.len(), same.iter().map(|h| h.holders).sum())
    }

    /// **钥匙表跟着连接身份走**：同一把钥匙连 20 次（每次交进来一份）表里也只一格；每份身份收掉时放一份，
    /// 全放掉 ⇒ 表里没有它、拨号核心再要它就拿不到（报读不动那一句）。
    #[test]
    fn a_key_put_for_every_dial_is_stored_once_and_gone_when_the_last_holder_lets_go() {
        use ssh_dial::env::KeySource as _;
        let pw = || super::Key::Password("pw-held-20-times".into());
        let ids: Vec<String> = (0..20).map(|_| super::put_key(pw())).collect();
        assert!(ids.iter().all(|id| *id == ids[0]), "{ids:?}");
        assert_eq!(copies_of(&pw()), (1, 20));
        assert!(matches!(
            super::PhoneKeys.credential(&ids[0]),
            Ok(ssh_dial::env::Credential::Password(p)) if p == "pw-held-20-times"
        ));
        for id in &ids {
            assert!(super::hold_key(id, false));
        }
        assert_eq!(copies_of(&pw()), (0, 0));
        assert!(!super::hold_key(&ids[0], true));
        assert!(super::PhoneKeys.credential(&ids[0]).is_err());
    }

    /// 两把不同的钥匙各一格、编号不同；跟着换一条时多握的那一份也要放掉，表里才没有它。
    #[test]
    fn different_keys_get_their_own_ids_and_an_extra_hold_needs_its_own_release() {
        let a = super::put_key(super::Key::Pem("pem-a".into(), None));
        let b = super::put_key(super::Key::Pem("pem-a".into(), Some("pass".into())));
        assert_ne!(a, b);
        assert!(super::hold_key(&a, true));
        assert!(super::hold_key(&a, false));
        assert_eq!(copies_of(&super::Key::Pem("pem-a".into(), None)), (1, 1));
        assert!(super::hold_key(&a, false));
        assert!(super::hold_key(&b, false));
        assert_eq!(copies_of(&super::Key::Pem("pem-a".into(), None)), (0, 0));
        assert_eq!(
            copies_of(&super::Key::Pem("pem-a".into(), Some("pass".into()))),
            (0, 0)
        );
    }

    /// 手机进程装的写规矩是部署那一种（只许后端那两处，建出来的目录收成只给本人）：文件屏走那台后端的 `files-*`，
    /// 手机自己经 SFTP 只剩部署要写的那两处。
    #[test]
    fn the_phone_installs_the_deploy_write_rule() {
        super::install_env(
            std::env::temp_dir()
                .join(format!("mobcore-env-{}", std::process::id()))
                .join("known_hosts"),
        );
        assert!(ssh_dial::env::installed());
        assert_eq!(
            ssh_dial::env::get().files,
            ssh_dial::sftp::WriteRules::Deploy {
                dir_mode: super::DEPLOY_DIR_MODE
            }
        );
    }

    /// 身份那一行是 JSON、带拨号核心的 ack 协议版本与它认得的用法（PTY · 握着连接那两种也在）。
    #[test]
    fn the_version_line_names_the_dial_core_it_carries() {
        let v: serde_json::Value = serde_json::from_str(&super::version_json()).expect("不是 JSON");
        assert_eq!(v["ack_v"], ssh_dial::ACK_V);
        for u in ["shell", "hold"] {
            assert!(
                v["uses"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|x| x == u)),
                "{v}"
            );
        }
    }

    /// 交 Kotlin 的那一形对得上那把钥匙：那一行逐字是它的 OpenSSH 形、算法名是它的、指纹是拨号核心那一份（桌面同一个）。
    fn assert_is(got: &cmd_wire::phone::SshPublicKey, key: &russh::keys::PublicKey, kind: &str) {
        assert_eq!(got.line, key.to_openssh().unwrap());
        assert_eq!(got.kind, kind);
        assert_eq!(got.fingerprint, ssh_dial::known_hosts::fingerprint(key));
        assert!(
            got.fingerprint.starts_with("SHA256:") && got.fingerprint.len() == 50,
            "{got:?}"
        );
    }

    /// 导入时派生公钥：一把 OpenSSH 格式的私钥 ⇒ 它自己的公钥（那一行 · 算法 · 指纹）；乱码 ⇒ `None`。
    #[test]
    fn the_public_key_of_a_private_key_is_its_own_public_key() {
        let key = russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7u8; 32]),
        );
        let pem = key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .unwrap();
        let mut want = key.public_key().clone();
        want.set_comment("phone");
        let got = super::public_key_of(&pem, "phone").expect("认不出");
        assert_is(&got, &want, "ssh-ed25519");
        assert_eq!(super::public_key_of("not a key", ""), None);
    }

    /// Keystore 那把：P-256 未压缩点 ⇒ 它的公钥（`ecdsa-sha2-nistp256` · 指纹同一份）；点坏了 ⇒ `None`。
    #[test]
    fn the_public_key_of_a_p256_point_is_that_key() {
        let rng = ring::rand::SystemRandom::new();
        let alg = &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING;
        let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(alg, &rng).unwrap();
        let pair = ring::signature::EcdsaKeyPair::from_pkcs8(alg, pkcs8.as_ref(), &rng).unwrap();
        use ring::signature::KeyPair as _;
        let point = pair.public_key().as_ref().to_vec();
        let got = super::public_key_of_point(&point, "phone@ks").expect("认不出那个点");
        let mut want = russh::keys::PublicKey::from(
            russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(&point).unwrap(),
        );
        want.set_comment("phone@ks");
        assert_is(&got, &want, "ecdsa-sha2-nistp256");
        assert!(got.line.ends_with(" phone@ks"), "{got:?}");
        assert_eq!(super::public_key_of_point(&[4u8; 10], ""), None);
    }

    /// 现生成的那一把：私钥解得回来、公钥就是它的公钥（同一形）、两把不一样（真随机）。
    #[test]
    fn a_generated_key_round_trips_and_is_fresh() {
        let pair = super::generate_ed25519("phone").expect("生成不了");
        assert!(pair.pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert_eq!(
            super::public_key_of(&pair.pem, "phone").as_ref(),
            Some(&pair.public)
        );
        assert_eq!(pair.public.kind, "ssh-ed25519");
        assert!(
            pair.public.line.starts_with("ssh-ed25519 ") && pair.public.line.ends_with(" phone"),
            "{:?}",
            pair.public
        );
        assert_ne!(
            super::generate_ed25519("").unwrap().public,
            super::generate_ed25519("").unwrap().public
        );
        let hosts: serde_json::Value =
            serde_json::from_str(&super::ssh_config_json("Host a\n Port 2200")).unwrap();
        assert_eq!(hosts[0]["alias"], "a");
        assert_eq!(hosts[0]["port"], 2200);
    }

    /// **看门狗到期 ⇒ 拨号核心里那条 TCP 没了**：一条还没等到 ack 的链路（对面接了 TCP、一个字节都不说：黑洞那一形）被关
    /// （Kotlin 那一侧握手看门狗到点 ⇒ `Core.close`），那一拨在核心里跟着收掉，对面看到这条 TCP 关了 —— 不留一拨挂着占拨号锁。
    #[test]
    fn closing_a_link_before_its_ack_takes_its_tcp_down() {
        let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        let req =
            serde_json::json!({ "host": "127.0.0.1", "port": port, "user": "u", "use": "hold" });
        let id =
            super::open_link(&req.to_string(), std::sync::Arc::new(|_, _| {})).expect("开不成");
        let (mut peer, _) = sock.accept().expect("核心没拨过来");
        peer.set_read_timeout(Some(guard_core::wall::CEILING))
            .unwrap();
        super::close_link(id);
        let mut buf = [0u8; 256];
        loop {
            match std::io::Read::read(&mut peer, &mut buf) {
                Ok(0) => break,
                Ok(_) => continue, // 核心先发的 SSH 头
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => break,
                Err(e) => panic!("关了链路，那条 TCP 还在（{e}）"),
            }
        }
    }

    /// 链路表：读不动的请求开不成；开成的链路先交下行字节、收工时交一个「没了」；关掉之后再送 ⇒ `false`。
    #[test]
    fn a_link_opens_reports_its_end_and_refuses_sends_once_closed() {
        assert!(super::open_link("not json", std::sync::Arc::new(|_, _| {})).is_err());
        let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
        let tx = std::sync::Mutex::new(tx);
        // 拨一个绑着不听的本机口：拨号失败的 ack 先到，链路随后收工。
        let sock = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        drop(sock);
        let req = serde_json::json!({
            "host": "127.0.0.1", "port": port, "user": "u",
            "use": "capture", "capture": {"max_bytes": 10}
        });
        let id = super::open_link(
            &req.to_string(),
            std::sync::Arc::new(move |_, b: Option<&[u8]>| {
                let _ = tx.lock().unwrap().send(b.map(<[u8]>::to_vec));
            }),
        )
        .expect("开不成");
        let mut got = Vec::new();
        while let Some(b) = rx.recv().expect("链路没交「没了」就断了") {
            got.extend(b);
        }
        let first = got.split(|b| *b == b'\n').next().unwrap();
        let ack: serde_json::Value = serde_json::from_slice(first).unwrap();
        assert_eq!(ack["ok"], false, "{ack}");
        super::close_link(id);
        assert!(!super::send_link(id, b"x".to_vec()));
    }

    /// 组几台合一份：B 把 C 上的 c 与 E 上的 e 收成 g1，E 那一片的名字更新 ⇒ 手机核心并出一组 g1、名字「乙」、组员 c · e
    /// （`face_core::groups::merge` 那一份，本机后端 `groups-read` 调的同一个）；某台那一片解不开 ⇒ 那台读不到、别台照合。
    #[test]
    fn the_phone_merges_group_slices_by_group_id() {
        use serde_json::{json, Value};
        let face =
            serde_json::to_value(face_core::book::unseen(cmd_wire::cells::Words("f".into())))
                .unwrap();
        let slice = |name: &str, named: i64, sid: &str| {
            json!({
                "groups": [{"id": "g1", "name": name, "made": 1, "named": named}],
                "members": [{"sid": sid, "groups": ["g1"], "pinned": false, "face": face,
                    "facts": {"title": sid, "cwd": null, "record": null, "account": null, "bg": false, "at": null}}],
            })
        };
        let text = json!({"answered": [
            {"origin": "C", "reply": slice("甲", 1, "c")},
            {"origin": "E", "reply": slice("乙", 2, "e")},
            {"origin": "F", "reply": {"groups": 3}},
        ]})
        .to_string();
        let got: Value = serde_json::from_str(&super::groups_merge_json(&text)).unwrap();
        let groups = got["groups"].as_array().expect("groups");
        assert_eq!(groups.len(), 1, "同一个组 id 并成一组：{got}");
        assert_eq!(groups[0]["id"], "g1");
        assert_eq!(groups[0]["name"], "乙", "名字取 named 新的那片");
        let who: Vec<(&str, &str)> = groups[0]["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| (m["origin"].as_str().unwrap(), m["sid"].as_str().unwrap()))
            .collect();
        assert_eq!(who, [("C", "c"), ("E", "e")]);
        let heads: Vec<&str> = got["machines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["read"]["is"].as_str().unwrap())
            .collect();
        assert_eq!(heads, ["ok", "ok", "failed"], "解不开的那台照实说读不到");
        assert_eq!(super::groups_merge_json("不是 JSON"), "");
    }

    /// 搜内容几台合一份：手机核心合出的就是 `face_core::search::merge` 那一份（本机后端扇出完调的同一个）；
    /// 交进来的是各台真回包的形（金样那一行：`updatedAt` · `face.order` · `hits[].offset` 这类整数格），合完整数格原样是整数；
    /// 某台那一份解不开 ⇒ 那台进 `down`、别台照合。
    #[test]
    fn the_phone_merges_search_replies_with_the_one_shared_merge() {
        use serde_json::{json, Value};
        let golden: Value = serde_json::from_str(include_str!(
            "../../../../tests/__fixtures__/history-search-row.golden.json"
        ))
        .unwrap();
        let line = |sid: &str, at: i64| {
            let mut r: Value = serde_json::from_str(golden["lines"][0].as_str().unwrap()).unwrap();
            r["sessionId"] = json!(sid);
            r["updatedAt"] = json!(at);
            r["atText"] = json!("09:05");
            r["spanText"] = json!("09:05–09:05");
            r["label"] = json!(sid);
            r["starred"] = json!(true);
            r["hidden"] = json!(null);
            r
        };
        let a = json!({"sessions": [line("a1", 100)], "unreadable": 1, "skipped": ["Codex"]});
        let b =
            json!({"sessions": [line("b1", 1_767_225_600_123)], "unreadable": 0, "skipped": []});
        let text = json!({"q": "夹具词", "answered": [
            {"origin": "phone-a", "reply": a},
            {"origin": "phone-b", "reply": b},
            {"origin": "phone-c", "reply": {"sessions": [{"updatedAt": 1.5}], "unreadable": 0, "skipped": []}},
        ]})
        .to_string();
        let got: Value = serde_json::from_str(&super::search_merge_json(&text)).unwrap();
        let want = face_core::search::merge(
            "夹具词",
            vec![
                (
                    cmd_wire::Machine::named("phone-a"),
                    serde_json::from_value(a).unwrap(),
                ),
                (
                    cmd_wire::Machine::named("phone-b"),
                    serde_json::from_value(b).unwrap(),
                ),
            ],
            vec![],
        );
        let mut want = serde_json::to_value(&want).unwrap();
        assert_eq!(got["down"][0]["origin"], "phone-c", "{got}");
        want["down"] = got["down"].clone();
        assert_eq!(got, want, "手机核心合出的不是那一份合并");
        assert_eq!(got["sessions"][0]["sessionId"], "b1");
        assert!(
            got["sessions"][0]["updatedAt"].is_i64()
                && got["sessions"][0]["face"]["order"].is_u64(),
            "{got}"
        );
        assert_eq!(super::search_merge_json("不是 JSON"), "");
    }
}
