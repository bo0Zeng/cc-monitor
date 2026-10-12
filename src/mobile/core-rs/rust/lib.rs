//! **手机嵌的 Rust 核心**（`libmobcore.so`）：只有 FFI 外壳。拨号 · 鉴权 · 复用 · 用法是后端同一份拨号核心（`ssh-dial`）。
//!
//! 边界是**手写 JNI、窄口、走消息**：一条链路就是拨号核心那一条（上行字节 → 下行阶段行 ＋ ack ＋ 字节），
//! Kotlin 只开 · 送 · 改窗口 · 关；回调三个：下行字节 · Keystore 签名 · 认下的 host key（`Core.Sink`）。
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

/// 装上手机那一套（一个进程一次）：钥匙在本表 · host key 回调进 Kotlin · 没有 agent · 远端写任意路径（文件屏）。
fn install_env() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = ssh_dial::env::install(Env {
            keys: Box::new(PhoneKeys),
            host_keys: Box::new(PhoneHostKeys),
            agent: None,
            facts: Box::new(PhoneFacts),
            files: WriteRules::Anywhere,
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
    // 下行：读到的块原样交给 Kotlin；链路收工 ⇒ 交一个「没了」。
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
    let task = rt.spawn(async move {
        let stages = ssh_dial::StageSink::new(req.stages);
        let control = ssh_dial::uses::Control {
            resize: Some(resize_rx),
        };
        ssh_dial::uses::run_with(&req, &stages, up_r, &mut down_w, control).await;
        let _ = down_w.shutdown().await;
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

#[cfg(test)]
mod tests {
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
