//! 中转口的**门**〔`INVARIANTS §48.1a` 中转口的钥匙〕：钥匙住哪 · 谁铸 · 进门三问。
//!
//! # 为什么要有这扇门（主会话判「缺口，不是取舍」）
//!
//! 中转听的是回环 TCP，而回环 TCP **没有权限位**：同机任何进程 —— 别的 OS 用户、
//! 浏览器里一张网页向 `127.0.0.1` 发的请求 —— 连得上它。门开着的时候，谁走 `/s/<agent>/<账号>/…`
//! 就能让中转代入那一行的凭据去打上游。路由键第三段（会话 id / nonce）**不是**认证：它是公开可铸的标签。
//!
//! # 钥匙住哪、谁铸
//!
//! - 住址：**中转所在那台机器**的 `$HOME/`[`KEY_FILE_REL`]，出生即只给本人（`creds_core::perm::create_private`，`O_EXCL`）。
//! - 谁铸：**中转自己**，在**绑上口之后**（`listen::prepare`）—— 读回；读不出或形状不对就铸一把新的
//!   （256 位，OS 密码学随机数）、临时文件写满、原子挪过去。只有绑上了口的那一个会写 ⇒ 两个中转抢着铸构造上不存在。
//! - 为什么落盘而不是只在内存：端口是固定常量（monitor `payload::RELAY_PORT`）⇒ 后端重启后老会话手里的 URL
//!   **本来就还有效**；钥匙每次换，一次重启就打断每一条活会话（它们的 env 起会话那一刻就定死了）。
//! - 为什么不照 §48.1 控制口「宿主铸、经 env 交」：远端中转**没有宿主**（它是远端后端起的脱离进程）。
//!   中转自己铸 ⇒ 本机远端一形，钥匙一次都不经过 env / argv / 线上帧。
//!
//! # 钥匙怎么到 agent 手里（不在本文件，但本文件的住址是它的另一半）
//!
//! 起会话载荷渲染 `export ANTHROPIC_BASE_URL=…` 时，钥匙那一段写成**读这个文件的命令替换** `$(cat ~/<KEY_FILE_REL>)`，
//! 在那台机器的 pane shell 里展开（`control/launch_render/payload.rs::relay_env_prefix_posix`，shell 写法出自 `platform/shell/posix.rs::home_file_between`）。
//! ⇒ 钥匙只从这个文件进 agent 进程自己的 env；载荷、`tmux send-keys` 的 argv、shell 历史、webview 里都只有那几个字。
//! 两半的相对路径是同一个 const（共享 crate `relay_route_core::KEY_FILE_REL`），不再各写一份再对拍。
//!
//! # 进门三问（[`admit`]，顺序固定，都在读请求体之前）
//!
//! 1. 带 `Origin` ⇒ [`Verdict::Browser`]（**403**）—— 浏览器发的请求一律带它；claude CLI 现打不带（`RK1.md §5.1`）；
//! 2. `Host` 不是回环字面量（或没有、或不止一个）⇒ [`Verdict::NotLoopbackHost`]（**421**，防 DNS rebinding）；
//! 3. 路径第一段不是钥匙 ⇒ [`Verdict::BadKey`]（**403**）；比对定长时间（[`crate::stream::listen::tokens_match`]，与控制口同一份）。
//!
//! 过了才剥掉 `/<钥匙>`，余下的交给 `route::parse`（一字不改）⇒ 表里没这一行仍是 **404**，与 403 可分。
//!
//! # ⚠ 诚实边界
//!
//! - 钥匙挡的是「**读不到这个文件**的人」。能读你家目录（root、你自己的进程、你起的 agent）的，本来就能以你的身份跑东西。
//! - 文件在中转跑着的时候被人删掉 / 改掉 ⇒ 中转手里那一把与盘上对不上，新会话每一发 403（出声，不静默）；重起中转就好。

use super::http1::RequestHead;
use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 钥匙文件相对家目录的路径。值只住共享 crate `relay_route_core::KEY_FILE_REL`：
/// 起会话载荷渲染 `$(cat ~/…)` 用的 `control/launch_render/payload.rs::RELAY_KEY_FILE_REL` 是同一个 const。
pub(crate) const KEY_FILE_REL: &str = relay_route_core::KEY_FILE_REL;

/// 钥匙的熵：32 字节 = 256 位（题面要 ≥128 位）。落盘是 64 个小写十六进制字符。
const KEY_BYTES: usize = 32;

/// 门拒绝时回的两个状态行。
pub(super) const FORBIDDEN: &str = "403 Forbidden";
pub(super) const MISDIRECTED: &str = "421 Misdirected Request";

/// 一把钥匙。**刻意不派生 `Debug` / `Display`**：值只经 [`Key::expose`] 一处拿得出来，
/// 谁想把它 `{:?}` 进日志，编译器先拦。
#[derive(Clone)]
pub(crate) struct Key(String);

impl Key {
    /// 唯一的取值口。生产段只有本模块用它：落盘 · 门里比对 · 给用户要贴的那一段插钥匙（[`keyed_with_key_on_disk`]）。
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// 从一个串认一把钥匙：形状不对就不认（空 / 短 / 非小写十六进制都算不对）。
    pub(crate) fn from_text(s: &str) -> Option<Key> {
        let t = s.trim();
        key_shape_ok(t).then(|| Key(t.to_string()))
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

/// 钥匙的形状：恰好 `2 × KEY_BYTES` 个小写十六进制字符。唯一住址是共享 crate（`ccm` 认继承来的地址也用它）；
/// 本模块铸的长度与它对得上由 `door_tests` 那条「铸出来的过形状闸」钉着。
pub(crate) use relay_route_core::key_shape_ok;

/// 这台机器上钥匙文件的路径：家目录（`platform::paths::home_dir_from`）底下那一份。
/// 取值器是注入的 ⇒ 判据喂夹具家目录，不碰进程环境。
pub(crate) fn key_path(get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into))?;
    Some(home.join(KEY_FILE_REL))
}

/// 读回盘上那一把。不在 / 读不动 / 形状不对 ⇒ `None`（**只读**：探针与门都走这里）。
pub(crate) fn read_key(path: &Path) -> Option<Key> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| Key::from_text(&s))
}

/// 给一条中转地址（构造口产物）插上这台盘上那把钥匙（家目录底下 [`KEY_FILE_REL`]，**只读**）⇒
/// 用户自己贴进 agent 设置文件的那一段要的展开形（那里写不了 `$(cat …)`）。钥匙在这里插、不以裸值出本模块；
/// 钥匙文件不在 / 形状不对 / 地址不是构造口产物 ⇒ `None`。
pub(crate) fn keyed_with_key_on_disk(home: &Path, url: &str) -> Option<String> {
    let key = read_key(&home.join(KEY_FILE_REL))?;
    relay_route_core::keyed_base_url(url, key.expose())
}

/// 中转起来时拿钥匙：读回；没有或坏了就铸一把新的落盘。**本模块唯一的写口**
/// （`readonly_guard` 第四层登记；门是 `relay/listen.rs`，只在绑上口之后调）。
pub(crate) fn ensure_key(path: &Path) -> Result<Key, String> {
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
        copy_text(
            "beDoor.fs.mkdirFailed",
            &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    let _lock = crate::platform::lock::hold(dir)?;
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
    Ok(Key(s))
}

/// 落盘：临时文件出生即只给本人（`O_EXCL`）→ 写满 → 原子挪过去；失败删自己的临时文件（那一层目录由 [`ensure_key`] 在拿锁前建）。
/// 报错里只有路径，**永远没有钥匙本身**。
fn write_key(path: &Path, k: &Key) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path.parent().ok_or_else(|| {
        copy_text(
            "beDoor.fs.noParent",
            &[("path", &path.display().to_string())],
        )
    })?;
    let tmp = dir.join(format!("relay-key.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = creds_core::perm::create_private(&tmp).map_err(|e| {
            copy_text(
                "beDoor.fs.tmpCreateFailed",
                &[("tmp", &tmp.display().to_string()), ("e", &e.to_string())],
            )
        })?;
        f.write_all(k.expose().as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| {
                copy_text(
                    "beDoor.fs.tmpWriteFailed",
                    &[("tmp", &tmp.display().to_string()), ("e", &e.to_string())],
                )
            })?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| {
            copy_text(
                "beDoor.fs.renameFailed",
                &[
                    ("tmp", &tmp.display().to_string()),
                    ("path", &path.display().to_string()),
                    ("e", &e.to_string()),
                ],
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 进门三问的结局。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 过了：剥掉 `/<钥匙>` 之后的目标（交给 `route::parse`）。
    Pass(String),
    /// 带 `Origin` —— 浏览器发的。
    Browser,
    /// `Host` 不是回环字面量（或没有 / 不止一个）。
    NotLoopbackHost,
    /// 没钥匙 / 钥匙不对。
    BadKey,
}

impl Verdict {
    /// 拒绝那几格回什么：状态行 ＋ 原因头的值（`server::REASON_HEADER`）＋ 一句为什么。`Pass` ⇒ `None`。
    pub(super) fn refusal(&self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Verdict::Pass(_) => None,
            Verdict::Browser => Some((
                FORBIDDEN,
                "browser-origin",
                "relay: requests carrying an Origin header are refused (browser pages may not use this port)",
            )),
            Verdict::NotLoopbackHost => Some((
                MISDIRECTED,
                "host-not-loopback",
                "relay: the Host header must be a loopback literal (127.0.0.1 / localhost / [::1])",
            )),
            Verdict::BadKey => Some((
                FORBIDDEN,
                "bad-key",
                "relay: missing or wrong relay key (first path segment)",
            )),
        }
    }
}

/// 进门三问。纯函数：只看请求头与钥匙，不读网络、不读盘。
pub(crate) fn admit(head: &RequestHead, key: &Key) -> Verdict {
    if head
        .headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("origin"))
    {
        return Verdict::Browser;
    }
    let mut hosts = head
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("host"));
    match (hosts.next(), hosts.next()) {
        (Some((_, h)), None) if host_header_is_loopback_literal(h) => {}
        _ => return Verdict::NotLoopbackHost,
    }
    let Some(after) = head.target.strip_prefix('/') else {
        return Verdict::BadKey;
    };
    let (seg, rest) = match after.find('/') {
        Some(i) => (&after[..i], &after[i..]),
        None => return Verdict::BadKey,
    };
    if !crate::stream::listen::tokens_match(seg, key.expose()) {
        return Verdict::BadKey;
    }
    Verdict::Pass(rest.to_string())
}

/// `Host` 那一格是不是回环字面量：`127.0.0.1` · `localhost` · `[::1]`，可带 `:<十进制口>`。大小写不敏感。
/// 〔主会话 09-26 裁（丙）〕防 DNS 重绑，只认三个字面量是设计；与上游那条「这个地址在不在本机」
/// （`upstream_url_core::upstream_is_loopback`，整个 `127/8`）是两个判定，不许并。
pub(crate) fn host_header_is_loopback_literal(raw: &str) -> bool {
    let h = raw.trim().to_ascii_lowercase();
    let name = if let Some(r) = h.strip_prefix('[') {
        match r.split_once(']') {
            Some((inner, tail)) if tail.is_empty() || port_ok(tail) => format!("[{inner}]"),
            _ => return false,
        }
    } else {
        match h.rsplit_once(':') {
            Some((n, p)) if port_ok(&format!(":{p}")) => n.to_string(),
            Some(_) => return false,
            None => h.clone(),
        }
    };
    matches!(name.as_str(), "127.0.0.1" | "localhost" | "[::1]")
}

fn port_ok(tail: &str) -> bool {
    tail.strip_prefix(':').is_some_and(|p| {
        !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()
    })
}

// 判据 ＋ 同层判据共用的夹具（`TEST_KEY` · `Key::for_tests` · `seed_test_home`）都住这一份测试文件里，
// 生产文件里不留 `#[cfg(test)]` 支撑项（`structural_scan` 那条只许降的计数）。
#[cfg(test)]
#[path = "../../../tests/backend/relay/door_tests.rs"]
pub(super) mod door_tests;
