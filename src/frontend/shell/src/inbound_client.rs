//! monitor 侧的入方向客户端：本体（「Hello 之前不许写」的类型 · 一问一答 · 期限与撤单 · 应答路由）住 `resident-client`
//! （手机核心同一份，`src/comms/resident-client/client.rs`），这里转用；本文件只剩 monitor 自己的那两样 ——
//! 本机后端那一个客户端（[`register`] · [`local_client`]；问别台一律经它 `call_at`）与经它转来的各台此刻那一条连接的能力事实（[`reached`] · [`offer_of`]）。

pub use resident_client::client::*;
// 判据（`inbound_client_tests.rs`，与本体同一份行为）沿用本体那几样词。
#[cfg(test)]
use crate::chan::wire::Withdraw;
use comms_inward::chan::wire::Offer;
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
// 本机后端那一个客户端
// ============================================================================

/// 本机后端此刻那条连接的客户端（写者 = 本机两条载体的读循环：hello 之后登记、断了摘除）。
fn slot() -> &'static Mutex<Option<Arc<InboundClient>>> {
    static S: Mutex<Option<Arc<InboundClient>>> = Mutex::new(None);
    &S
}

/// 登记本机这条连接的客户端（重连 ⇒ 换掉旧的）。
pub fn register(client: Arc<InboundClient>) {
    // 记下它通往哪台：形状对不上那一句按这个名字说；看的这一台的时区，每条请求信封都带。
    client.name_machine(cmd_wire::LOCAL);
    client.set_viewer_tz(host_core::viewer_tz());
    // **先放锁再 shutdown** —— 与 `unregister` 对称。
    let old = lock(slot()).replace(client);
    if let Some(old) = old {
        old.shutdown();
    }
    // 本机后端起没起来看的就是这条通道在不在。
    crate::local_backend_host::local_backend_changed();
}

/// 摘除 —— **只摘自己那条**。重连时新连接可能已经登记上来了，
/// 拿旧的 `Arc` 比一下指针，不是自己的就别动（否则会把新连接摘掉）。
pub fn unregister(mine: &Arc<InboundClient>) {
    let mut s = lock(slot());
    let is_mine = s.as_ref().is_some_and(|cur| Arc::ptr_eq(cur, mine));
    if is_mine {
        *s = None;
    }
    drop(s);
    mine.shutdown();
    if is_mine {
        crate::local_backend_host::local_backend_changed();
    }
}

/// 本机后端此刻那条连接的客户端。没连上 / 还没收到 hello → `None`。
pub fn local_client() -> Option<Arc<InboundClient>> {
    lock(slot()).clone()
}

/// 本机后端那条流还没登记时，等它多久：`LOCAL_WAIT_TRIES × LOCAL_WAIT_INTERVAL_MS` ≈ 3 s。
/// **`wait-for-condition`**（登记在 `rust_timer_registry`）：等的是一次性条件（本机那条流的 hello 到了），
/// 等到就走、等不到就如实报。对端就在本机，从起进程到 hello 是毫秒级。
const LOCAL_WAIT_TRIES: u32 = 60;
const LOCAL_WAIT_INTERVAL_MS: u64 = 50;

/// 本机后端那条流在、且认 `cmd` ⇒ 回它的客户端（有界地等它出现）。**找不到就报，不回落**（`D11`）：
/// 不在 ⇒ 报「本机后端不在」；不认 ⇒ 报「本机后端太旧」。
pub(crate) async fn local_backend_accepting(cmd: &str) -> Result<Arc<InboundClient>, String> {
    for attempt in 0..LOCAL_WAIT_TRIES {
        if let Some(c) = local_client() {
            if !c.accepts(cmd) {
                return Err(copy_core::backend_old(&copy_core::local_machine()));
            }
            return Ok(c);
        }
        if attempt + 1 < LOCAL_WAIT_TRIES {
            tokio::time::sleep(std::time::Duration::from_millis(LOCAL_WAIT_INTERVAL_MS)).await;
        }
    }
    Err(copy_core::copy_text(
        "rsDialHost.local.absent",
        &[(
            "dur",
            &copy_core::format_duration(u64::from(LOCAL_WAIT_TRIES) * LOCAL_WAIT_INTERVAL_MS),
        )],
    ))
}

// ============================================================================
// 经本机后端转来的各台：此刻那一条连接的能力事实
// ============================================================================

/// 一台此刻那一条连接握手时交出的那几样（那台的 hello，本机后端原样转来）。
struct Reached {
    offer: Offer,
    build: String,
}

fn reached_book() -> &'static Mutex<HashMap<cmd_wire::Machine, Reached>> {
    static B: std::sync::OnceLock<Mutex<HashMap<cmd_wire::Machine, Reached>>> =
        std::sync::OnceLock::new();
    B.get_or_init(Default::default)
}

/// 那台这一条连接接上了（收的那一头见到它的 hello）：记下能力事实，回它（同一台换一条连接 ⇒ 换成新的）。
pub(crate) fn reached(machine: &cmd_wire::Machine, hello: &cmd_wire::frame::Hello) -> Offer {
    let offer = Offer::of(hello);
    lock(reached_book()).insert(
        machine.clone(),
        Reached {
            offer: offer.clone(),
            build: hello.build_id.clone(),
        },
    );
    offer
}

/// 那台这一条连接断了。
pub(crate) fn gone(machine: &cmd_wire::Machine) {
    lock(reached_book()).remove(machine);
}

/// 那台的能力事实：本机 ⇒ 本机那条连接握手时交的；别台 ⇒ 本机后端转来的那台 hello 里的（本机没连着 ⇒ 问不到那台，`None`）。
pub fn offer_of(origin: &cmd_wire::Machine) -> Option<Offer> {
    let local = local_client()?;
    if origin.is_local() {
        return Some(local.offer());
    }
    lock(reached_book()).get(origin).map(|r| r.offer.clone())
}

/// 那台这一条连接握手时报的构建标识（诊断信息用）；没连着 ⇒ `None`。
pub fn build_of(machine: &cmd_wire::Machine) -> Option<String> {
    lock(reached_book()).get(machine).map(|r| r.build.clone())
}

/// **测试期本机那一格的独占锁**。
///
/// 那一格是**进程内全局**的，而 cargo 默认并行跑用例 ⇒ 两条都在本机那一格上起真 backend
/// 的用例会互相看见对方登记的通道。
///
/// ⚠ 中毒也要拿到锁（`into_inner`）：一条用例 panic 不该把其余的全变成「锁中毒」。
#[cfg(test)]
pub(crate) fn local_origin_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/inbound_client_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/local_address_tests.rs"]
mod local_address_tests;
