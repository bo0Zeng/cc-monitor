//! **手机嵌的 Rust 核心**（`libmobcore.so`）：只有 FFI 外壳。拨号 · 鉴权 · 复用 · 用法是后端同一份拨号核心（`ssh-dial`）。
//!
//! 边界是**手写 JNI、窄口、走消息**：一条链路就是拨号核心那一条（上行字节 → 下行阶段行 ＋ ack ＋ 字节），
//! Kotlin 只开 · 送 · 改窗口 · 关；分块传（`transfer.rs`）交的是文件描述符；回调四个：下行字节 · Keystore 签名 · 认下的 host key · 常驻流交的事（`Core.Sink`）。
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

use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use ssh_dial::env::{Credential, Env, ExternalSigner, Facts, HostKeys, KeySource};
use ssh_dial::sftp::WriteRules;
use ssh_dial::Said;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

pub mod batch;
pub mod install;
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

/// Kotlin 交进来的一样钥匙（按编号存；拨号请求的 `key_path` 写这个编号）。
#[derive(Clone)]
enum Key {
    Pem(String, Option<String>),
    Password(String),
    Keystore(String, russh::keys::PublicKey),
}

fn keys() -> &'static Mutex<HashMap<String, Key>> {
    static K: OnceLock<Mutex<HashMap<String, Key>>> = OnceLock::new();
    K.get_or_init(Default::default)
}

fn put_key(id: String, key: Key) {
    keys()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, key);
}

struct PhoneKeys;

impl KeySource for PhoneKeys {
    fn credential(&self, key: &str) -> Result<Credential, Said> {
        let found = keys()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned();
        let unreadable = || copy_core::copy_text("beConnect.auth.keyLoad", &[("path", key)]);
        match found {
            Some(Key::Pem(pem, pass)) => russh::keys::decode_secret_key(&pem, pass.as_deref())
                .map(|k| Credential::Key(Box::new(k)))
                .map_err(|e| Said::with_raw(unreadable(), &e)),
            Some(Key::Password(p)) => Ok(Credential::Password(p)),
            Some(Key::Keystore(alias, public)) => {
                Ok(Credential::Signer(Arc::new(KeystoreSigner {
                    alias,
                    public,
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

/// 认下的 host key 交回 Kotlin（那边等鉴权过了才落 Room）。
struct PhoneHostKeys;

impl HostKeys for PhoneHostKeys {
    fn remember(&self, host: &str, port: u16, key: &str) {
        if let Some(s) = sink() {
            s.call(|env, obj| {
                let h = env.new_string(host)?;
                let k = env.new_string(key)?;
                env.call_method(
                    obj,
                    "hostKey",
                    "(Ljava/lang/String;ILjava/lang/String;)V",
                    &[
                        JValue::Object(&h),
                        JValue::Int(i32::from(port)),
                        JValue::Object(&k),
                    ],
                )?;
                Ok(())
            });
        }
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

/// 装上手机那一套（一个进程一次）：钥匙在本表 · host key 回调进 Kotlin · 没有 agent ·
/// 远端写只许部署那两处（文件屏走那台后端的 `files-*`，不经手机的 SFTP 写）。
fn install_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = ssh_dial::env::install(Env {
            keys: Box::new(PhoneKeys),
            host_keys: Box::new(PhoneHostKeys),
            agent: None,
            facts: Box::new(PhoneFacts),
            files: WriteRules::Deploy {
                dir_mode: DEPLOY_DIR_MODE,
            },
        });
    });
}

// ═══ 钥匙本身（导入时派生公钥）═══════════════════════════════════════════════════════

/// 一段私钥 PEM（OpenSSH / PKCS#8 / …，不带口令）⇒ 它的公钥那一行（`<算法> <base64> [注释]`）。认不出 ⇒ `None`。
pub fn public_line_of(pem: &str, comment: &str) -> Option<String> {
    let key = russh::keys::decode_secret_key(pem, None).ok()?;
    let mut public = key.public_key().clone();
    public.set_comment(comment);
    public.to_openssh().ok()
}

/// 现生成一把软件 Ed25519（系统随机数做种子）：回 `(私钥 PEM（openssh-key-v1，无口令）, 公钥那一行)`。
pub fn generate_ed25519(comment: &str) -> Option<(String, String)> {
    use ring::rand::SecureRandom as _;
    let mut seed = [0u8; 32];
    ring::rand::SystemRandom::new().fill(&mut seed).ok()?;
    let mut key = russh::keys::PrivateKey::from(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed),
    );
    key.set_comment(comment);
    let pem = key.to_openssh(russh::keys::ssh_key::LineEnding::LF).ok()?;
    let public = key.public_key().to_openssh().ok()?;
    Some((pem.to_string(), public))
}

/// 一段 `~/.ssh/config` 文本 ⇒ 可导入主机的 JSON 数组（解析住拨号核心 `config_text`）。
pub fn ssh_config_json(text: &str) -> String {
    serde_json::to_string(&ssh_dial::config_text::parse(text)).unwrap_or_else(|_| "[]".to_string())
}

/// 文件页选中这几项（`files-ls` 回包 `entries[]` 里的原行）· 此刻那一台（[`resident::here`]）⇒ 顶栏 · ⋯ · 点一下做什么
/// （`pick_core::PhoneMenu`）；读不动 ⇒ 什么都不给。
pub fn pick_menu_json(rows: &str, here: &pick_core::Here) -> String {
    let picks: Vec<pick_core::PickIn> = serde_json::from_str::<Vec<cmd_wire::files::LsEntry>>(rows)
        .map(|v| v.iter().map(pick_core::PickIn::of_ls).collect())
        .unwrap_or_default();
    let picks: Vec<pick_core::Pick> = picks.iter().map(pick_core::PickIn::pick).collect();
    serde_json::to_string(&pick_core::phone_menu(&picks, here)).unwrap_or_default()
}

/// 一问一答撞了同名（码 `exists` 的 `data`：同名清单）⇒ 抽屉那几行（[`batch::clash_rows`]，一组那一问同一份）；读不动 ⇒ `[]`。
pub fn clash_rows_json(data: &str) -> String {
    serde_json::from_str::<cmd_wire::batch::Conflicts>(data)
        .map(|c| batch::clash_rows(&c.conflicts).to_string())
        .unwrap_or_else(|_| "[]".to_string())
}

/// 下载一组那几格（[`batch::download_event`]）；读不动 ⇒ 空串。
pub fn download_event_json(want: &str) -> String {
    serde_json::from_str::<serde_json::Value>(want)
        .ok()
        .and_then(|w| batch::download_event(&w))
        .map(|v| v.to_string())
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
    // ack 之前照共用循环那一个期限（[`ack_within`]）：要拨号核心报进展。
    let mut req = req;
    req.progress = true;
    let task = rt.spawn(async move {
        let stages = ssh_dial::StageSink::new(req.stages);
        let control = ssh_dial::uses::Control {
            resize: Some(resize_rx),
        };
        ssh_dial::uses::run_with(&req, &stages, up_r, &mut down_w, control).await;
        let _ = down_w.shutdown().await;
    });
    let dialing = task.abort_handle();
    // 下行：ack 之前那一段经 [`ack_within`]（到期限一步都没动 ⇒ 那一拨收掉）；之后读到的块原样交给 Kotlin；链路收工 ⇒ 交一个「没了」。
    rt.spawn(async move {
        if ack_within(&mut down_r, &mut |b: &[u8]| on_data(id, Some(b))).await {
            let mut buf = vec![0u8; 32 * 1024];
            loop {
                match down_r.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => on_data(id, Some(&buf[..n])),
                }
            }
        } else {
            dialing.abort();
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

/// **Kotlin 开的每一条链路（握着的那条 · 终端 · 文件 · 一次性 exec），ack 之前那一段**：拨号核心报的进展行在这里吃掉、每一步重新计拨号期限
/// （`resident_client::feed::dial_within`，共用循环开常驻流那一拨同一处；Kotlin 一个数都不写），阶段行与 ack 原样交给 `forward`。
/// 回 `true` ＝ ack 交了（成或不成都是拨号核心的话）；一步都没来的那个期限 ⇒ 交一行没拨成的 ack（`reason: timeout`）、回 `false`，
/// 调用方收掉那一拨（挂在 SYN · banner 上的那条不留着占这个身份的拨号锁）；没给 ack 就收工 ⇒ `false`。
async fn ack_within<R: tokio::io::AsyncRead + Unpin>(
    down: &mut R,
    forward: &mut impl FnMut(&[u8]),
) -> bool {
    let progress = resident_client::feed::Progress::default();
    let head = async {
        loop {
            let line = read_line(down).await?;
            let v: serde_json::Value = serde_json::from_slice(&line).unwrap_or_default();
            if v.get("progress").is_some() {
                progress.step();
                continue;
            }
            let mut whole = line;
            whole.push(b'\n');
            forward(&whole);
            if v.get("stage").is_none() {
                return Some(());
            }
        }
    };
    match resident_client::feed::dial_within(&progress, head).await {
        Some(got) => got.is_some(),
        None => {
            let ack = serde_json::json!({
                "ok": false,
                "error": copy_core::copy_text("reason.dial.timeout", &[]),
                "reason": "timeout",
                "detail": format!("{}s", resident_client::backoff::DIAL_WITHIN.as_secs()),
            });
            forward(format!("{ack}\n").as_bytes());
            false
        }
    }
}

/// 一字节一字节读一行（不含换行；ack 之后紧跟的是用法的字节，不许预读过头）。收工 ⇒ `None`。
async fn read_line<R: tokio::io::AsyncRead + Unpin>(down: &mut R) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match down.read(&mut one).await {
            Ok(0) | Err(_) => return None,
            Ok(_) if one[0] == b'\n' => return Some(out),
            Ok(_) => out.push(one[0]),
        }
    }
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

fn guarded<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// `Core.version()`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_version<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
) -> jstring {
    let text = guarded("{}".to_string(), version_json);
    env.new_string(text)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// `Core.init(sink)`：记下 Kotlin 那一侧、装上手机那一套。只认第一次。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_init<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    sink_obj: JObject<'l>,
) {
    guarded((), || {
        if SINK.get().is_none() {
            if let (Ok(vm), Ok(obj)) = (env.get_java_vm(), env.new_global_ref(sink_obj)) {
                let _ = SINK.set(Sink { vm, obj });
            }
        }
        install_env();
    });
}

/// `Core.putPassword(id, password)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putPassword<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: JString<'l>,
    password: JString<'l>,
) {
    guarded((), || {
        if let (Some(id), Some(p)) = (string(&mut env, &id), string(&mut env, &password)) {
            put_key(id, Key::Password(p));
        }
    });
}

/// `Core.putPem(id, pem, passphrase?)`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putPem<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: JString<'l>,
    pem: JByteArray<'l>,
    passphrase: JString<'l>,
) {
    guarded((), || {
        let pass = string(&mut env, &passphrase);
        let pem = env
            .convert_byte_array(&pem)
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        if let (Some(id), Some(pem)) = (string(&mut env, &id), pem) {
            put_key(id, Key::Pem(pem, pass));
        }
    });
}

/// `Core.putKeystore(id, alias, sec1Point)`：公钥是 P-256 的未压缩点（`04 ‖ X ‖ Y`）。认不出那个点 ⇒ `false`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_putKeystore<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    id: JString<'l>,
    alias: JString<'l>,
    point: JByteArray<'l>,
) -> jboolean {
    guarded(JNI_FALSE, || {
        let pt = env.convert_byte_array(&point).ok();
        let (Some(id), Some(alias), Some(pt)) =
            (string(&mut env, &id), string(&mut env, &alias), pt)
        else {
            return JNI_FALSE;
        };
        let Ok(ec) = russh::keys::ssh_key::public::EcdsaPublicKey::from_sec1_bytes(&pt) else {
            return JNI_FALSE;
        };
        put_key(id, Key::Keystore(alias, russh::keys::PublicKey::from(ec)));
        JNI_TRUE
    })
}

/// `Core.publicKeyOf(pem, comment)`：导入一把私钥时派生它的公钥那一行；认不出 ⇒ `null`。
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
        match pem.and_then(|p| public_line_of(&p, &comment)) {
            Some(line) => env
                .new_string(line)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            None => std::ptr::null_mut(),
        }
    })
}

/// `Core.generateEd25519(comment)`：回 JSON `{pem, public}`；生成不了 ⇒ `null`。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_generateEd25519<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    comment: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let comment = string(&mut env, &comment).unwrap_or_default();
        match generate_ed25519(&comment) {
            Some((pem, public)) => env
                .new_string(serde_json::json!({"pem": pem, "public": public}).to_string())
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
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
        env.new_string(ssh_config_json(&text))
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
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

/// `Core.downloadEvent(want)`：下载一组那几格（[`download_event_json`]）。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_downloadEvent<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    want: JString<'l>,
) -> jstring {
    guarded(std::ptr::null_mut(), || {
        let text = string(&mut env, &want).unwrap_or_default();
        env.new_string(download_event_json(&text))
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
fn file_of(fd: jint) -> Option<std::fs::File> {
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

/// `Core.transferPull(handle, fd, args, shown, withinMs)`：那台上的 `args.path` 逐块读回、写进本机那一份（`fd`，写；`shown` 是它在句子里的名字）。
/// 回问答编号（同 [`Java_com_ccmonitor_mobile_core_rs_Core_transferStage`]）；这台不在 · 入参读不动 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_transferPull<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    fd: jint,
    args: JString<'l>,
    shown: JString<'l>,
    within_ms: jlong,
) -> jlong {
    guarded(0, || {
        let Some(file) = file_of(fd) else { return 0 };
        let (Some(args), shown) = (string(&mut env, &args), string(&mut env, &shown).unwrap_or_default()) else {
            return 0;
        };
        let _rt = runtime().enter();
        transfer::pull(
            handle,
            file,
            &args,
            shown,
            u64::try_from(within_ms).unwrap_or(0),
            on_resident(),
        )
    })
}

/// `Core.batchStart(handle, want)`：起一组（`batch.rs::plan_of` 那几种「要做什么」，入参由核心拼）；回编号，
/// 进度 · 同名那一问 · 结局经 `Sink.resident(…, "batch", …)` 回来。这台不在 · 读不动 ⇒ 0。
#[no_mangle]
pub extern "system" fn Java_com_ccmonitor_mobile_core_rs_Core_batchStart<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    want: JString<'l>,
) -> jlong {
    guarded(0, || {
        let Some(want) = string(&mut env, &want) else { return 0 };
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

    /// 文件页那一问：交进 `files-ls` 的原行 ⇒ 顶栏 · ⋯（每项四格）· 点一下做什么；那台断着 ⇒ 灰着说离线；读不动 ⇒ 什么都不给。
    #[test]
    fn the_pick_menu_reads_the_ls_rows_as_they_came() {
        let rows = r#"[{"path":"/h/docs","name":"docs","kind":"dir","class":"folder","type_text":"","mtime_secs":12}]"#;
        let up = pick_core::Here::default();
        let v: serde_json::Value =
            serde_json::from_str(&super::pick_menu_json(rows, &up)).unwrap();
        assert_eq!(v["bar"][0]["id"], "download");
        assert_eq!(v["bar"][0]["enabled"], false);
        assert!(v["bar"][0]["why"].as_str().is_some_and(|w| w.contains("docs")));
        assert_eq!(v["more"][0].as_object().map(|o| o.len()), Some(4));
        assert_eq!(v["tap"], "open");
        let off = pick_core::Here {
            offline: true,
            unavailable: vec![],
        };
        let v: serde_json::Value =
            serde_json::from_str(&super::pick_menu_json(rows, &off)).unwrap();
        assert_eq!(
            v["more"][0]["why"],
            copy_core::copy_text("rsFilewinChrome.link.readOnly", &[])
        );
        let none: serde_json::Value =
            serde_json::from_str(&super::pick_menu_json("{", &up)).unwrap();
        assert_eq!((none["bar"].clone(), none["tap"].clone()), (serde_json::json!([]), serde_json::Value::Null));
    }

    /// 一问一答撞了同名：清单 ⇒ 抽屉那几行（说明由核心写、答法照那台给的）；读不动 ⇒ `[]`。
    #[test]
    fn a_clash_list_becomes_the_rows_of_the_sheet() {
        let data = r#"{"conflicts":[{"key":"0","name":"up.bin","kind":"file","size_text":"9 字节","there":"file","there_size_text":"3 字节","answers":["overwrite","keep_both"]}]}"#;
        let v: serde_json::Value = serde_json::from_str(&super::clash_rows_json(data)).unwrap();
        let c: cmd_wire::batch::Conflicts = serde_json::from_str(data).unwrap();
        assert_eq!(
            v,
            serde_json::json!([{ "key": "0", "name": "up.bin",
                "meta": batch_core::conflict_meta(&c.conflicts[0]), "answers": ["overwrite", "keep_both"] }])
        );
        assert_eq!(super::clash_rows_json("{"), "[]");
    }

    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    /// 喂一段拨号核心的下行（`(隔多久, 那一行)`），收 [`super::ack_within`] 交给 Kotlin 的那几行 ＋ 它的结局 ＋ 在虚拟钟上用了多久。
    async fn head_of(script: Vec<(u64, &'static str)>) -> (Vec<String>, bool, Duration) {
        let (mut w, mut r) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            for (gap, line) in script {
                tokio::time::sleep(Duration::from_secs(gap)).await;
                if w.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    return;
                }
            }
            std::future::pending::<()>().await;
        });
        let t0 = tokio::time::Instant::now();
        let mut got = Vec::<u8>::new();
        let acked = super::ack_within(&mut r, &mut |b: &[u8]| got.extend_from_slice(b)).await;
        let lines = String::from_utf8(got)
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        (lines, acked, t0.elapsed())
    }

    /// **Kotlin 开的每一条链路，ack 之前照共用循环那一个期限**（`resident_client::feed::dial_within`）：慢链路上每一步隔 8 秒、合计远超
    /// 期限也照样等到 ack；进展行在核心里吃掉（Kotlin 一行也看不到），阶段行与 ack 原样交。
    #[tokio::test(start_paused = true)]
    async fn a_slow_dial_that_keeps_moving_reaches_its_ack_and_kotlin_sees_no_progress_lines() {
        let (lines, acked, took) = head_of(vec![
            (8, r#"{"progress":"tcp"}"#),
            (8, r#"{"progress":"banner"}"#),
            (1, r#"{"stage":{"kind":"won"}}"#),
            (7, r#"{"progress":"auth"}"#),
            (8, r#"{"ok":true}"#),
        ])
        .await;
        assert!(acked, "{lines:?}");
        assert_eq!(lines, vec![r#"{"stage":{"kind":"won"}}"#, r#"{"ok":true}"#]);
        assert_eq!(took, Duration::from_secs(32));
    }

    /// 一步都不动的那一拨：最后一步之后 [`DIAL_WITHIN`] 交一行没拨成的 ack（原因 `timeout`，那一句是拨号超时那一句），回 `false`
    /// （调用方收掉那一拨：挂在 SYN · banner 上的那一条不留着占这个身份的拨号锁）。
    #[tokio::test(start_paused = true)]
    async fn a_dial_that_stops_moving_gets_a_timeout_ack_one_window_after_its_last_step() {
        let (lines, acked, took) = head_of(vec![(3, r#"{"progress":"tcp"}"#)]).await;
        assert!(!acked);
        assert_eq!(took, Duration::from_secs(3) + DIAL_WITHIN);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let ack: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(ack["ok"], false);
        assert_eq!(ack["reason"], "timeout");
        assert_eq!(
            ack["error"],
            copy_core::copy_text("reason.dial.timeout", &[])
        );
    }

    /// 链路没给 ack 就收工 ⇒ `false`，不另造一行（拨号核心自己的失败 ack 已经在它前面交了的那种不走这里）。
    #[tokio::test(start_paused = true)]
    async fn a_link_that_ends_before_its_ack_is_just_over() {
        let (mut w, mut r) = tokio::io::duplex(64);
        w.write_all(b"{\"progress\":\"tcp\"}\n").await.unwrap();
        drop(w);
        let mut got = Vec::<u8>::new();
        assert!(!super::ack_within(&mut r, &mut |b: &[u8]| got.extend_from_slice(b)).await);
        assert!(got.is_empty());
    }

    /// 手机进程装的写规矩是部署那一种（只许后端那两处，建出来的目录收成只给本人）：文件屏走那台后端的 `files-*`，
    /// 手机自己经 SFTP 只剩部署要写的那两处。
    #[test]
    fn the_phone_installs_the_deploy_write_rule() {
        super::install_env();
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

    /// 导入时派生公钥：一把 OpenSSH 格式的私钥 ⇒ 与它自己的公钥那一行逐字相同；乱码 ⇒ `None`。
    #[test]
    fn the_public_line_of_a_private_key_is_its_own_public_key() {
        let key = russh::keys::PrivateKey::from(
            russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7u8; 32]),
        );
        let pem = key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .unwrap();
        let mut want = key.public_key().clone();
        want.set_comment("phone");
        assert_eq!(
            super::public_line_of(&pem, "phone"),
            Some(want.to_openssh().unwrap())
        );
        assert_eq!(super::public_line_of("not a key", ""), None);
    }

    /// 现生成的那一把：私钥解得回来、公钥就是它的公钥、两把不一样（真随机）。
    #[test]
    fn a_generated_key_round_trips_and_is_fresh() {
        let (pem, public) = super::generate_ed25519("phone").expect("生成不了");
        assert!(pem.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert_eq!(
            super::public_line_of(&pem, "phone").as_deref(),
            Some(public.as_str())
        );
        assert!(
            public.starts_with("ssh-ed25519 ") && public.ends_with(" phone"),
            "{public}"
        );
        assert_ne!(
            super::generate_ed25519("").unwrap().1,
            super::generate_ed25519("").unwrap().1
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
}
