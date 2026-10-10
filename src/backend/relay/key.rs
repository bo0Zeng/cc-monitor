//! 中转口钥匙的**读盘那一半**〔`INVARIANTS §48.1a` 中转口的钥匙〕：钥匙住哪 · 谁铸 · 怎么落盘。
//! 钥匙的形状与进门三问住中转那一份（`comms_outward::admit`）；本文件只管盘上那一份，读好 / 铸好之后经 `Relay::new` 交给中转。
//!
//! # 钥匙住哪、谁铸
//!
//! - 住址：**中转所在那台机器**的 `$HOME/`[`KEY_FILE_REL`]，出生即只给本人（`creds_core::perm::create_private`，`O_EXCL`）。
//! - 谁铸：**后端**，在中转**绑上口之后**（`listen::prepare`）—— 读回；读不出或形状不对就铸一把新的
//!   （256 位，OS 密码学随机数）、临时文件写满、原子挪过去。只有绑上了口的那一个会写 ⇒ 两个中转抢着铸构造上不存在。
//! - 为什么落盘而不是只在内存：端口是固定常量（monitor `payload::RELAY_PORT`）⇒ 后端重启后老会话手里的 URL
//!   **本来就还有效**；钥匙每次换，一次重启就打断每一条活会话（它们的 env 起会话那一刻就定死了）。
//! - 为什么不照 §48.1 控制口「宿主铸、经 env 交」：远端中转**没有宿主**（它是远端后端起的脱离进程）。
//!   那台后端自己铸 ⇒ 本机远端一形，钥匙一次都不经过 env / argv / 线上帧。
//!
//! # 钥匙怎么到 agent 手里（不在本文件，但本文件的住址是它的另一半）
//!
//! `ccm` 在最终 exec 那一处照那一家的注入格（`agents::Inject`）做：认地址环境变量的那一家，直路在自己进程里读这个文件、
//! 把钥匙拼进那个变量（经本文件 [`keyed_with_key_on_disk`]）；地址只能拼进参数的那一家，拼进参数的是插了**只许直通那一把**
//! （[`KeyKind::Pass`]，[`pass_of`] 从根钥匙派生）的地址。非得经 shell 那一趟：全权那一把写成**读这个文件的命令替换** `$(cat ~/<KEY_FILE_REL>)`，
//! 只许直通那一把在 `ccm` 里现算、照字面写进那个词（它本来就进 argv）
//! （`control/ccm/plan.rs::relay_export`，shell 写法出自 `platform/shell/posix.rs::home_file_between`）。
//! ⇒ 全权那一把只从这个文件进 agent 进程自己的 env；交给终端的那一行、`tmux send-keys` 的 argv、shell 历史、webview 里都没有它。
//! 只许直通那一把会进那一家的 argv 与它自己的日志 —— 它只开得了 `/t/`（永不代入凭据），见 `comms_outward::door`。
//!
//! # 一把根钥匙，两个范围
//!
//! 盘上只有一份文件（全权那一把，根钥匙）。只许直通那一把**不落盘**：从根钥匙派生（[`pass_of`]：`HMAC-SHA256(根钥匙, "ccm-relay/t")`
//! 的 64 位小写十六进制）—— 门上与 `ccm` 插地址用的是同一个函数。漏出去的那一把推不回根钥匙。
//! 两半的相对路径是同一个 const（共享 crate `relay_route_core::KEY_FILE_REL`），不再各写一份再对拍。

use comms_outward::Key;
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 钥匙文件相对家目录的路径。值只住共享 crate `relay_route_core::KEY_FILE_REL`：
/// `ccm` 读钥匙 / 渲 `$(cat ~/…)` 用的也是同一个 const。
pub(crate) const KEY_FILE_REL: &str = relay_route_core::KEY_FILE_REL;

/// 哪一个范围的钥匙。盘上只有全权那一把（根钥匙）；只许直通那一把从它派生（[`pass_of`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyKind {
    /// 全权（`/s/` 与 `/t/`）。
    Full,
    /// 只许直通（`/t/`）。
    Pass,
}

/// 钥匙的熵：32 字节 = 256 位（要求 ≥128 位）。落盘是 64 个小写十六进制字符。
const KEY_BYTES: usize = 32;

/// 派生只许直通那一把时用的标签（换了它，已经发出去的只许直通地址全部作废）。
const PASS_LABEL: &[u8] = b"ccm-relay/t";

/// 这台机器上根钥匙那份文件的路径：家目录（`platform::paths::home_dir_from`）底下那一份。
/// 取值器是注入的 ⇒ 判据喂夹具家目录，不碰进程环境。
pub(crate) fn key_path(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))?;
    Some(home.join(KEY_FILE_REL))
}

/// 只许直通那一把：`HMAC-SHA256(根钥匙, "ccm-relay/t")` 的 64 位小写十六进制。门上与 `ccm` 插地址同一个函数（纯函数）。
pub(crate) fn pass_of(full: &Key) -> Key {
    let k = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, full.expose().as_bytes());
    let tag = ring::hmac::sign(&k, PASS_LABEL);
    let hex: String = tag.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    Key::from_text(&hex).expect("HMAC-SHA256 的 32 字节写成 64 位小写十六进制，形状恒对")
}

/// 根钥匙 ⇒ 那个范围的那一把。
pub(crate) fn scoped(full: Key, kind: KeyKind) -> Key {
    match kind {
        KeyKind::Full => full,
        KeyKind::Pass => pass_of(&full),
    }
}

/// 这台盘上根钥匙派生出来的只许直通那一把（[`pass_of`]）的串；读不到 ⇒ `None`（**只读**）。
pub(crate) fn pass_key_on_disk(home: &Path) -> Option<String> {
    read_key(&home.join(KEY_FILE_REL)).map(|full| pass_of(&full).expose().to_string())
}

/// 读回盘上那一把。不在 / 读不动 / 形状不对 ⇒ `None`（**只读**：探针与门都走这里）。
pub(crate) fn read_key(path: &Path) -> Option<Key> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| Key::from_text(&s))
}

/// 给一条中转地址（构造口产物）插上这台盘上 `kind` 那一把钥匙（**只读**）⇒ agent 进程要的展开形
/// （环境变量里的、参数里的、用户自己贴进设置文件的那一段 —— 那里写不了 `$(cat …)`）。钥匙在这里插、不以裸值出本模块；
/// 钥匙文件不在 / 形状不对 / 地址不是构造口产物 ⇒ `None`。
pub(crate) fn keyed_with_key_on_disk(home: &Path, url: &str, kind: KeyKind) -> Option<String> {
    let key = scoped(read_key(&home.join(KEY_FILE_REL))?, kind);
    relay_route_core::keyed_base_url(url, key.expose())
}

/// 中转起来时拿钥匙：读回；没有或坏了就铸一把新的落盘。**本模块唯一的写口**
/// （`readonly_guard` 第四层登记；门是 `relay/listen.rs`，只在绑上口之后调）。
pub(crate) fn ensure_key(path: &Path) -> Result<Key, crate::common::said::Said> {
    if let Some(k) = read_key(path) {
        return Ok(k);
    }
    // 读 → 铸 → 写整段在那个目录的跨进程锁里（`platform/lock.rs`），锁里再读一次：
    //   两个进程同时发现没钥匙时，只有先拿到锁的那一个铸，后一个读回它那一把。
    let dir = path.parent().ok_or_else(|| {
        copy_text(
            "beDoor.fs.noParent",
            &[("path", &path.display().to_string())],
        )
    })?;
    // 只建那一层、建的那一下就是 0700（`own_dir`：后端建自家目录的那一个函数）。挪到拿锁之前：锁的是这个目录，它得先在。
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        crate::common::said::Said::with_raw(
            copy_text(
                "beDoor.fs.mkdirFailed",
                &[
                    ("dir", &dir.display().to_string()),
                    ("why", &copy_core::io_reason(e.kind())),
                ],
            ),
            &e,
        )
    })?;
    let _lock = crate::platform::lock::hold(dir).map_err(crate::common::said::Said::from)?;
    if let Some(k) = read_key(path) {
        return Ok(k);
    }
    let k = mint()?;
    write_key(path, &k)?;
    Ok(k)
}

/// 铸一把：OS 密码学随机数（经 `rustls` 已经带进来的 `ring`，不新增依赖）。
fn mint() -> Result<Key, String> {
    let mut buf = [0u8; KEY_BYTES];
    rustls::crypto::ring::default_provider()
        .secure_random
        .fill(&mut buf)
        .map_err(|_| copy_text("beDoor.key.noRandom", &[]))?;
    let mut s = String::with_capacity(2 * KEY_BYTES);
    for b in buf {
        s.push_str(&format!("{b:02x}"));
    }
    Key::from_text(&s).ok_or_else(|| copy_text("beDoor.key.noRandom", &[]))
}

/// 落盘（经 `own_state`：出生即只给本人 → 写满 → 原子挪过去）。那一层目录由 [`ensure_key`] 在拿锁前建。
/// 报错里只有路径，**永远没有钥匙本身**。
fn write_key(path: &Path, k: &Key) -> Result<(), crate::common::said::Said> {
    crate::common::own_state::write(path, k.expose().as_bytes())
}

// 判据 ＋ 同层判据共用的夹具（`TEST_KEY` · `test_key` · `seed_test_home`）都住这一份测试文件里。
#[cfg(test)]
#[path = "../../../tests/backend/relay/key_tests.rs"]
pub(super) mod key_tests;
