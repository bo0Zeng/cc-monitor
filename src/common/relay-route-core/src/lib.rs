//! **中转门牌**的唯一住址：这台机器的中转在哪个口 · 进门的钥匙放在哪 · 路由路径长什么样。
//!
//! # 它为什么是一个共享 crate（`设计/20 §5` 目标 · `§10` 第 5 条）
//!
//! 〔US1 · 第四波 4D〕先前这几样在两个半边**各写一份**：
//!
//! | 件 | monitor 那一份 | 后端那一份 | 先前靠什么对上 |
//! |---|---|---|---|
//! | 端口 8788 | `payload.rs` 里一个字面量 | `relay/server.rs` 里一个字面量 | **什么都没有**（零对拍） |
//! | 钥匙文件相对路径 | `payload.rs` 里一个字面量 | `relay/door.rs` 里一个字面量 | 后端判据现抠 monitor 源码字面量 |
//! | 两个前缀 · 段闸 · 拼路由 | `payload.rs` 里那一族构造口 | `relay/route.rs` 里那张前缀表与段闸 | 一行样例两侧各解一次 |
//!
//! 两个二进制不共享源码树（后端刻意不在 monitor 的 workspace 里）⇒ 共享 crate 是「一份实现两侧 use」的唯一载体
//! （同 `shell-quote-core` 的形状）。从此漂开**不可表示**：想不一致得先把 `use` 删掉。
//!
//! # 它**不是**业务 crate
//!
//! 这里一个账号 / 凭据 / 上游的名字都没有：两个段是**位置**（第 1/2 段），谁是 agent、谁是账号只在后端上游选择那一层
//! 才有名字（`设计/20 §0` 条 48）。⇒ 通信层成员 `relay/route.rs` 可以 `use` 它（`设计/05 §2` `C2` 禁的是业务 crate）。
//!
//! # 谁用哪几样
//!
//! - 后端：中转 `relay/route.rs::parse`（[`parse_target`]）· 中转宿主 `relay/listen.rs`（[`PORT`]）· 门 `relay/door.rs`（[`KEY_FILE_REL`]）·
//!   上游选择 `accounts/upstream_select/endpoint.rs`（[`base_url`]：起会话那一发注入哪个地址，**只有它拼**）。
//! - monitor：起本机后端时交的端口（[`PORT`]）· 渲染 `$(cat "$HOME/<钥匙>")` 那一段（[`KEY_FILE_REL`]）·
//!   载荷里那条中转地址的 fail-closed 校验（[`base_url_shape_ok`]，[`base_url`] 的逆）· 起会话身份 token 的字符集（[`segment_is_safe`]）。

/// 〔HOST · `设计/05 §5.2` 两个端口〕**常驻监听口**的门牌也住这里（它与中转口是这台机器上后端的两个门）：
/// 这台机器 ＋ 这个 agent 家目录 ⇒ 那一个口。本机宿主（monitor `local_backend_host`）与远端 `--resident-ensure` 同一个函数
/// ⇒ 一台机器一个常驻后端，本机 / 远端视角收敛（`01 §3.3a`）。FNV-1a 写死（`DefaultHasher` 跨 Rust 版本不稳定，升级后要算出同一个口）。
pub fn listen_port_for(home: &str) -> u16 {
    const PORT_BASE: u16 = 49152;
    const PORT_SPAN: u32 = 16384;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in home.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    PORT_BASE + ((h % u64::from(PORT_SPAN)) as u16)
}

/// 〔E2 · V28 · `设计/01 §6.7b`〕**后端的落点**（相对家目录）：那个文件就是后端二进制本身，名字叫 `ccm`；本机与远端同一个。
/// 远端的 `backendPath`（可填的格）删了，monitor 与后端往那台拼命令、推字节都只认这一处。
pub const BACKEND_LANDING_REL: &str = ".cc-monitor/bin/ccm";

/// 同一个落点在远端 POSIX shell 里的写法（远端今天只承诺 POSIX，`01 §6.7b` 表 B）：`$HOME` 在那台上展开，其余字节都是安全字符。
pub const BACKEND_LANDING_SHELL: &str = "\"$HOME\"/.cc-monitor/bin/ccm";

/// 〔HOST〕常驻监听口的钥匙文件（相对家目录；0600，本机宿主与远端 `--resident-ensure` 同一份）。
pub const LISTEN_TOKEN_FILE_REL: &str = ".cc-monitor/listen-token";

/// 中转在回环上听的那个口。**本机**：monitor 起常驻后端时以 `CCM_RELAY_PORT` 交给它（它在进程里起中转）；
/// **远端**：`--resident-ensure` 起那台的常驻后端时交同一个值（本机远端同形）。
pub const PORT: u16 = 8788;

/// 中转钥匙文件相对家目录的路径（`INVARIANTS §48.1a`）：**中转所在那台机器**上 `0600`，中转绑上口之后自己读回或铸。
/// 注入的 URL 不带钥匙本身，渲染成 `$(cat "$HOME/<本常量>")` 在那台机器的 pane shell 里展开（RK1）。
pub const KEY_FILE_REL: &str = ".cc-monitor/relay-key";

/// 〔P3 · `设计/70 §6.2` · V160「一台机器一个家」〕后端的**上传暂存区**（相对家目录）：SFTP 传输台只往这里写 `<key>.part`，
/// 传完由那台后端提交、挪进目标。后端按它落盘（`control/files_commit.rs::STAGING_DIR`），monitor 的数据位置页按它列出。
pub const STAGING_DIR_REL: &str = ".cc-monitor/staging";

/// 〔P3 · 同上〕代码全景的**索引根**（相对家目录）：那台后端起全景小程序时交的 `--store`（`control/panorama.rs::store_dir`），
/// 被分析的仓零字节；monitor 的数据位置页按它列出。
pub const PANORAMA_INDEX_REL: &str = ".cc-monitor/panorama";

/// 〔P3 · 主会话 09-29 裁「家里的都进唯一枚举」〕这台机器上后端的**退出行为设置**（只有后端写，`control/exit_policy.rs`）。
pub const BACKEND_POLICY_REL: &str = ".cc-monitor/backend.json";

/// 〔P3 · 同上〕别名块文件：POSIX shell 读的那一份（后端 `platform/shell/dialect.rs` 写，rc 里那一行 source 它）。
pub const POSIX_ALIASES_REL: &str = ".cc-monitor/aliases.sh";

/// 〔P3 · 同上〕别名块文件：PowerShell 读的那一份。
pub const PS_ALIASES_REL: &str = ".cc-monitor/aliases.ps1";

/// 〔P3 · 同上〕skill 装记录（后端 `assets/skill_ledger.rs` 写；卸的时候按它删）。
pub const SKILL_LEDGER_REL: &str = ".cc-monitor/skill-installs.json";

/// 〔P3 · 同上〕资产目录（后端 `assets/asset_catalog.rs` 写；各台机器之间自动对上）。
pub const ASSET_CATALOG_REL: &str = ".cc-monitor/assets-catalog.json";

/// 〔P3 · 同上〕cc-acct-iso 的字节落点（后端 `assets/acct_iso_install.rs` 装）。
pub const ACCT_ISO_REL: &str = ".cc-monitor/bin/cc-acct-iso";

/// 〔P3 · 同上〕常驻监听口的进程记录的文件名（与 [`LISTEN_TOKEN_FILE_REL`] 同一个目录）：本机宿主与远端
/// `--resident-ensure` 按同一个口（[`listen_port_for`]）找同一份。
pub fn listen_pid_file_name(port: u16) -> String {
    format!("listen-{port}.pid")
}

/// 相对家目录的一段 ⇒ 最后一截（文件名）。给各写者定自己那个 `FILE_NAME`（临时件的名字从它拼），名字仍只住上面那一处。
pub const fn file_name_of(rel: &'static str) -> &'static str {
    let b = rel.as_bytes();
    let mut i = b.len();
    while i > 0 && b[i - 1] != b'/' {
        i -= 1;
    }
    let (_, tail) = b.split_at(i);
    match core::str::from_utf8(tail) {
        Ok(s) => s,
        Err(_) => rel,
    }
}

/// 钥匙的形状：恰好 64 个小写十六进制字符（32 字节 = 256 位，中转 `relay/door.rs` 铸的就是这一形）。
pub fn key_shape_ok(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// 一条**已经把钥匙段展开进去**的中转地址（`http://127.0.0.1:<口>/<钥匙>/<前缀>/…`，pane shell 展开
/// `$(cat "$HOME/<KEY_FILE_REL>")` 之后 agent 进程环境里的那一形）切成「钥匙之前」「钥匙之后」两半：
/// `("http://127.0.0.1:<口>/", "/<前缀>/<seg1>/<seg2>")`。
///
/// 认的条件全在这里一处：钥匙段过 [`key_shape_ok`] · 两半拼回去（去掉钥匙段）过 [`base_url_shape_ok`]。
/// 认不出 ⇒ `None`（那就不是我们注入的地址，原样对待）。读者：`ccm` 把继承来的地址转进新 pane 时
/// 渲回 `$(cat …)` 形、不把钥匙本身写进 `tmux send-keys` 的 argv（RK1 报 2）。
pub fn split_keyed_base_url(url: &str) -> Option<(&str, &str)> {
    let after_scheme = url.strip_prefix("http://127.0.0.1:")?;
    let head_len = "http://127.0.0.1:".len() + after_scheme.find('/')? + 1;
    let (head, keyed) = url.split_at(head_len);
    let slash = keyed.find('/')?;
    let (key, tail) = keyed.split_at(slash);
    (key_shape_ok(key) && base_url_shape_ok(&format!("{head}{}", &tail[1..])))
        .then_some((head, tail))
}

/// 路由路径第一段的两个前缀 = 两种模式（`设计/20 §2`「为什么用两个前缀而不是一个哨兵段」）。
///
/// `/s/` 代入：上游选择的表里必须有这一行，没有 ⇒ 404；`/t/` 直通：中转**永不**代入凭据，第 2 段只当标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    /// `/s/`
    Substitute,
    /// `/t/`
    Passthrough,
}

impl RouteMode {
    /// 闭集。**两个前缀的字面量只在 [`RouteMode::prefix`] 里。**
    pub const ALL: [RouteMode; 2] = [RouteMode::Substitute, RouteMode::Passthrough];

    /// 这个模式的前缀（带前后两个 `/`）。
    pub fn prefix(self) -> &'static str {
        match self {
            RouteMode::Substitute => "/s/",
            RouteMode::Passthrough => "/t/",
        }
    }
}

/// 一段路由里允许的字符 —— 白名单：ASCII 字母数字与 `-` `_`，1..=128 字节。
///
/// `.` 与 `/` 不在里面 ⇒ `..` 构造不出来；路由段要进 tee 行与日志，放开任意字节等于给换行 / 控制字符开一条路。
/// ⚠ 起会话身份 token（`CCM_LAUNCH_ID`）也用这一条字符集；中转给流打标签的请求头值也过它（〔V141〕流标签不再是路由段）。
pub fn segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 拼 `/<前缀>/<seg1>/<seg2>`。**任一段过不了 [`segment_is_safe`] ⇒ `None`**（fail-closed：
/// 拼错一段的症状是中转回一个查不出来的 404，所以宁可当场拒）。
///
/// 〔V141〕没有第 3 段：会话 id 归 agent 自己，启动器不往地址里塞会话身份 ⇒ 这条地址**不随会话变**，
/// 中转从 agent 请求里自带的头认会话。
pub fn route_path(mode: RouteMode, seg1: &str, seg2: &str) -> Option<String> {
    (segment_is_safe(seg1) && segment_is_safe(seg2))
        .then(|| format!("{}{seg1}/{seg2}", mode.prefix()))
}

/// 注入给 agent 的 base URL：`http://127.0.0.1:<port>` ＋ [`route_path`]。**恒回环**（回环是自指的：
/// 同一个字面串写进哪台机器就指哪台）。
pub fn base_url(port: u16, mode: RouteMode, seg1: &str, seg2: &str) -> Option<String> {
    if port == 0 {
        return None;
    }
    route_path(mode, seg1, seg2).map(|p| format!("http://127.0.0.1:{port}{p}"))
}

/// [`base_url`] 的**逆**：`http://127.0.0.1:<1–65535>` ＋ 一个前缀 ＋ 恰好两段、每段过闸。别的一律 `false`
/// （`localhost` · `https` · 查询串 · 尾斜杠 · 少段多段 · 端口前导空）。
pub fn base_url_shape_ok(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    let Some(slash) = rest.find('/') else {
        return false;
    };
    let (port, path) = rest.split_at(slash);
    let port_ok = !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|p| p != 0);
    let Some(segs) = RouteMode::ALL
        .iter()
        .find_map(|m| path.strip_prefix(m.prefix()))
    else {
        return false;
    };
    let parts: Vec<&str> = segs.split('/').collect();
    port_ok && parts.len() == 2 && parts.iter().all(|p| segment_is_safe(p))
}

/// 请求目标 `/<前缀>/<seg1>/<seg2>/<rest>` 切出来的样子（中转那一侧用）。
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    pub mode: RouteMode,
    pub seg1: &'a str,
    pub seg2: &'a str,
    /// 第 2 段之后的全部（**不带**开头那个 `/`），原样交上游。
    pub rest: &'a str,
}

/// 切请求目标。不是这个形状（前缀不认得 · 少段 · 某段过不了闸）⇒ `None`。
pub fn parse_target(target: &str) -> Option<Parsed<'_>> {
    let (mode, after) = RouteMode::ALL
        .iter()
        .find_map(|m| target.strip_prefix(m.prefix()).map(|r| (*m, r)))?;
    let (seg1, after) = after.split_once('/')?;
    let (seg2, rest) = after.split_once('/')?;
    (segment_is_safe(seg1) && segment_is_safe(seg2)).then_some(Parsed {
        mode,
        seg1,
        seg2,
        rest,
    })
}

#[cfg(test)]
#[path = "../../../../tests/common/relay-route-core/lib_tests.rs"]
mod tests;
