//! monitor 侧的入方向客户端：本体（「Hello 之前不许写」的类型 · 一问一答 · 期限与撤单 · 应答路由）住 `resident-client`
//! （手机核心同一份，`src/comms/resident-client/client.rs`），这里转用；本文件只剩 monitor 自己的那两样 ——
//! 每台一个客户端的登记表（[`register`] · [`client_for`]）与那台此刻连没连着（[`Link`]）。

pub use resident_client::client::*;
// 判据（`inbound_client_tests.rs`，与本体同一份行为）沿用本体那几样词。
#[cfg(test)]
use crate::chan::wire::Withdraw;
#[cfg(test)]
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;

/// 这一帧是 hello ⇒ 见证（[`BackendHello::of`]）；别的帧 ⇒ `None`。
pub fn witness_of(frame: &crate::stream_source::Frame) -> Option<BackendHello> {
    match frame {
        cmd_wire::frame::Frame::Hello(h) => Some(BackendHello::of(h)),
        _ => None,
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ============================================================================
// 每台远端主机一个客户端：origin → 当前连接的客户端
// ============================================================================

/// 形状同 `stream_source::announced_registry`：origin 是那台机器的稳定身份。
/// 写者 = 远端各台的收事那一头 `stream_source::consume_remote`（接上时登记、断了摘除）· 本机那条流。
fn registry() -> &'static Mutex<HashMap<String, Arc<InboundClient>>> {
    static R: std::sync::OnceLock<Mutex<HashMap<String, Arc<InboundClient>>>> =
        std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 登记本连接的客户端。同 origin 的旧条目会被替换（重连）。
pub fn register(origin: &str, client: Arc<InboundClient>) {
    // 记下它通往哪台：形状对不上那一句按这个名字说（一条连接只登记一次）；看的这一台的时区，每条请求信封都带。
    client.name_machine(origin);
    client.set_viewer_tz(host_core::viewer_tz());
    // **先放锁再 shutdown** —— 与 `unregister` 对称。今天两者都不会死锁（没有任何路径是
    // pending → registry），但两处写法不一致就是给后来人埋的坑。
    let old = lock(registry()).insert(origin.to_string(), client);
    if let Some(old) = old {
        old.shutdown();
    }
    set_link(origin, Link::Up);
    // 换了一条新连接 ⇒ 不论那台原先是不是 `Up`（新的先登记、旧的后摘时一直是）都醒一次订阅者：
    //   `stream_watch` 据它给新连接重报「在看」名单（新连接 ＝ 没报过）。
    lock(link_book()).tick.send_modify(|t| *t += 1);
    // 本机那一台的状态成品看的就是这条通道在不在；本机后端可能刚重启、远端各台的状态表是空的 ⇒ 每台最新那一轮重交。
    if origin == LOCAL_ORIGIN {
        crate::local_backend_host::machine_changed();
        crate::stream_source::rehand_rounds();
    }
}

/// 摘除 —— **只摘自己那条**。重连时新连接可能已经登记上来了，
/// 拿旧的 `Arc` 比一下指针，不是自己的就别动（否则会把新连接摘掉）。
pub fn unregister(origin: &str, mine: &Arc<InboundClient>) {
    let mut r = lock(registry());
    let is_mine = r.get(origin).is_some_and(|cur| Arc::ptr_eq(cur, mine));
    if is_mine {
        r.remove(origin);
    }
    drop(r);
    mine.shutdown();
    if is_mine {
        set_link(origin, Link::Reconnecting);
        if origin == LOCAL_ORIGIN {
            crate::local_backend_host::machine_changed();
        }
    }
}

// ============================================================================
// 那台机器此刻连没连着（文件窗口「断开 · 重新连接中…」「离线 · 采样 …」那一条的事实）
// ============================================================================

/// 一台机器那条长连接此刻的样子（由连接循环写：握上手 ⇒ `Up`；断了 ⇒ `Reconnecting`；重连一轮没连上 ⇒ `Down`）。
///
/// 窗口不猜：它订 `link` 那条流（`chan/host.rs`），这里是那条流的唯一来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Up,
    /// 刚断、连接循环正在重连（还没有一轮失败）。
    Reconnecting,
    /// 重连至少一轮没连上（或从没连上过）。
    Down,
}

struct LinkBook {
    links: HashMap<String, Link>,
    /// 每改一格 +1（订阅者据它醒）。
    tick: tokio::sync::watch::Sender<u64>,
}

fn link_book() -> &'static Mutex<LinkBook> {
    static B: std::sync::OnceLock<Mutex<LinkBook>> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        Mutex::new(LinkBook {
            links: HashMap::new(),
            tick: tokio::sync::watch::channel(0).0,
        })
    })
}

fn set_link(origin: &str, l: Link) {
    let mut b = lock(link_book());
    if b.links.get(origin) == Some(&l) {
        return;
    }
    b.links.insert(origin.to_string(), l);
    b.tick.send_modify(|t| *t += 1);
}

/// 连接循环一轮没连上（这一轮里一次 hello 都没收到）⇒ 那台记成 `Down`。
pub fn note_round_failed(origin: &str) {
    set_link(origin, Link::Down);
}

/// 那台此刻的样子。从没登记过（连接循环还没起 / 一次都没连上）⇒ `Down`。
pub fn link_of(origin: &str) -> Link {
    lock(link_book())
        .links
        .get(origin)
        .copied()
        .unwrap_or(Link::Down)
}

/// 每改一格就醒一次的那个收端（`chan/host.rs` 的 `link` 流拿它等）。
pub fn link_changes() -> tokio::sync::watch::Receiver<u64> {
    lock(link_book()).tick.subscribe()
}

/// 「重新连接」：叫醒那台的连接循环，不等退避睡满（循环住 `resident-client`，`stream_source::wake` 拨它）。
/// 连着 ⇒ 不做事；断着而它此刻正在拨 ⇒ 这一轮没成就不睡；连上那一刻这一下作废，
/// 不会留到下一次断线时白白跳过一轮退避。主窗口与文件窗口的［重新连接］都只走这一处。
pub fn kick(origin: &str) {
    if link_of(origin) == Link::Up {
        return;
    }
    crate::stream_source::wake(origin);
}

// 本机后端在 registry 里的 key（`LOCAL_ORIGIN`）住 `resident-client`（称呼那一句认它），上面整份转用。

/// **测试期 `<local>` 的独占锁**。
///
/// 登记表是**进程内全局**的，而 cargo 默认并行跑用例 ⇒ 两条都在 `<local>` 键上起真 backend
/// 的用例会互相看见对方登记的通道。实测形态：一条用例的 `等通道出现` 立刻为真
/// （其实是另一条登记的），随后 `起来了却没有 pid`。
///
/// ⚠ 中毒也要拿到锁（`into_inner`）：一条用例 panic 不该把其余的全变成「锁中毒」，
/// 那会把**一个**真失败放大成一片假失败，反而盖住原因。
#[cfg(test)]
pub(crate) fn local_origin_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

/// 此刻连着的每一台与它的客户端（`stream_watch` 据它给每条新连接报名单）。
pub fn connected() -> Vec<(String, Arc<InboundClient>)> {
    lock(registry())
        .iter()
        .map(|(o, c)| (o.clone(), c.clone()))
        .collect()
}

/// 取某台主机当前的入方向客户端。没连上 / 还没收到 hello → `None`。
pub fn client_for(origin: &str) -> Option<Arc<InboundClient>> {
    lock(registry()).get(origin).cloned()
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/inbound_client_tests.rs"]
mod tests;
