//! 中转的**宿主**：中转本身（搬字节那一层）是通信层的独立 crate `comms_outward`；本模块是后端起它、喂它的那一半 ——
//! 绑口与在途上界（`listen.rs`）· 钥匙文件（`key.rs`）· 把中转的契约件转给后端别处（上游选择 · tap · 续登录）。
//!
//! 后端别处只经这里的 `pub(crate) use` 认识中转（`layering_guard` 钉着 `comms_outward` 的公开项与登记表两向相等）。

mod key; // 中转口的钥匙：住哪 · 谁铸 · 落盘（读好交给中转）
mod listen; // 绑口 · accept · 在途上界 · 两个期限值 · 起监听之前的接线

/// 中转的入口（常驻后端进程内起）。**上游选择那只手由调用方递进来**（`accounts::upstream_select::host_relay`）。
pub(crate) use listen::{host, our_relay_listening, ENV_PORT};

/// 「把这台的钥匙插进这条地址」—— 上游选择出「直接敲的也走中转」那一段与 `ccm` exec 时用。只交插好的地址，不交钥匙本身。
pub(crate) use key::{keyed_with_key_on_disk, pass_key_on_disk, KeyKind};

/// 钥匙文件的住址（需手动清单照它显示权限那一项）。
pub(crate) use key::key_path;

/// 中转与上游选择之间的契约件 · tee 的第二个落点 · 一问一答的传输原语（都住 `comms_outward`）。
pub(crate) use comms_outward::{
    fetch, segment_is_safe, Ask, AuthSwap, Base, Destination, Destinations, Heard, Mode, Ready,
    RouteKey, Startup, TapBody, TapEvent, TapPort,
};

/// 开上游 TCP 的那一口：照这台的出网描述经出网口（直连或经代理）。后端自己发的那几发（续登录令牌）交它；
/// 中转转发的那一发经 [`open_for`]（先按那条会话的描述）。
pub(crate) fn open_here(
    host: &str,
    port: u16,
    deadline: std::time::Duration,
) -> std::io::Result<std::net::TcpStream> {
    open_by(egress::here(), host, port, deadline)
}

/// 来处 ⇒ 那条会话的出网描述（起会话时接过来的代理）；没有 ⇒ `None`。由宿主递进来（上游选择读它那一份），中转宿主不认识它住哪。
pub(crate) type EgressOf = std::sync::Arc<dyn Fn(&str) -> Option<egress::Egress> + Send + Sync>;

/// 中转转发那一发开上游 TCP 的那一口（来处 · 主机 · 端口 · 期限 ⇒ 一条 TCP；同 `comms_outward` 里 `upstream::OpenFor` 的形状）。
type OpenFor = dyn Fn(Option<&str>, &str, u16, std::time::Duration) -> std::io::Result<std::net::TcpStream>
    + Send
    + Sync;

/// 中转转发那一发开上游 TCP 的那一口：这一发带着来处、那条会话有描述 ⇒ 照它；否则照这台的出网描述。
pub(crate) fn open_for(egress_of: EgressOf) -> std::sync::Arc<OpenFor> {
    std::sync::Arc::new(move |origin, host, port, deadline| {
        match origin.and_then(|o| egress_of(o)) {
            Some(e) => open_by(&e, host, port, deadline),
            None => open_by(egress::here(), host, port, deadline),
        }
    })
}

fn open_by(
    e: &egress::Egress,
    host: &str,
    port: u16,
    deadline: std::time::Duration,
) -> std::io::Result<std::net::TcpStream> {
    egress::open(e, host, port, Some(deadline))
        .map(|o| o.tcp)
        .map_err(Into::into)
}

#[cfg(test)]
#[path = "../../../tests/backend/relay/egress_tests.rs"]
mod egress_tests;

// ── 判据：读的是中转 crate 与上游选择的源码（路径不变），挂在后端这一侧 ──
// 「中转层里没有账号」：四条两向集合相等（残留表已清空 ⇒ 零命中形态）
#[cfg(test)]
#[path = "../../../tests/backend/relay/creds_guard.rs"]
mod creds_guard; // `K-H2a` `KS4`：记日志走白名单 · 写半边只从一处声明开 · 换头前先丢的头名（整体 #[cfg(test)]）
#[cfg(test)]
#[path = "../../../tests/backend/relay/table_guard.rs"]
mod table_guard; // `K-H2` `KH1`：决定点那几条腿（焊接点 · 开上游连接点 · 中转无默认上游）
#[cfg(test)]
#[path = "../../../tests/backend/relay/upstream_selection_guard.rs"]
mod upstream_selection_guard;
#[cfg(test)]
#[path = "../../../tests/backend/relay/wire_golden.rs"]
mod wire_golden; // 「零行为变化」的字节金标准（三条线各一份手写期望）
                 // 组合判据：真中转 ＋ 生产段的上游选择 / tap / 内存探针（纯中转的单测在 `comms_outward` 里）。
#[cfg(test)]
#[path = "../../../tests/backend/relay/member_tests.rs"]
mod member_tests;
#[cfg(test)]
#[path = "../../../tests/backend/relay/observe_retry_tests.rs"]
mod observe_retry_tests;
#[cfg(test)]
#[path = "../../../tests/backend/relay/server_tests.rs"]
mod server_tests;
#[cfg(test)]
#[path = "../../../tests/backend/relay/two_form_tests.rs"]
mod two_form_tests;
