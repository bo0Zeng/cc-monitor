//! 默认上游与中转：`DefaultUpstream` 中转这一半的格，路由名与 kind 只经 `RouteId` 换。

use super::*;

pub use cmd_wire::AgentKind;

/// 一家在后端里面的路由名（中转路由键第 1 段 · 额度账 · 轮换簿 · apikey 表按它记）。不上线：没有 serde，
/// 只从线上的 kind 换得出来（[`pick_route`]）；出口一律换回 kind（[`kind_of_route`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteId(&'static str);

impl RouteId {
    /// 存进账 · 表那一侧的那个串。
    pub fn id(self) -> &'static str {
        self.0
    }
}

/// 线上的 kind（每条命令收的 `agent` 都是它）⇒ 那一家的路由名。规则同 [`pick_kind`]（空 ⇒ 默认那一家；认不出 ⇒ 那一句）。
pub fn pick_route(kind: Option<&AgentKind>) -> Result<RouteId, String> {
    pick_route_among(REGISTRY, kind.map(AgentKind::as_str))
}

/// [`pick_route`] 的可喂夹具那一半：在给定注册表里换。
pub fn pick_route_among(registry: &[Adapter], kind: Option<&str>) -> Result<RouteId, String> {
    pick_kind_among(registry, kind).map(|(_, f)| RouteId(f.adapter_id))
}

/// 账 · 表 · 路由键里存着的路由名 ⇒ [`RouteId`]（这一换只在这一处）。注册表里没有这个路由名（别家旧条目 · kind 的串）⇒ `None`。
pub fn route_named(route_id: &str) -> Option<RouteId> {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
        .map(|u| RouteId(u.route_id))
}

/// 那个路由名那一家的默认上游（按路由名查的那几格都从它取）。
pub(super) fn upstream_of(route: RouteId) -> Option<&'static DefaultUpstream> {
    let route_id = route.0;
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .find(|u| u.route_id == route_id)
}

/// 账 · 表里记的路由名 ⇒ 线上的 kind（回包里的 `agent` 都写它）；注册表里没有这个路由名 ⇒ 照原样（账上留着的别家旧条目）。
pub fn kind_of_route(route_id: &str) -> AgentKind {
    AgentKind::new(kind_of_route_among(REGISTRY, route_id))
}

/// [`kind_of_route`] 照给定注册表那一半（判据喂合成注册表，换号那一路照它手里那张表问）。
pub fn kind_of_route_among<'a>(registry: &[Adapter], route_id: &'a str) -> &'a str {
    registry
        .iter()
        .find(|a| a.launch.is_some_and(|f| f.adapter_id == route_id))
        .map_or(route_id, |a| a.kind)
}

/// 一个上游协议的流面：把一个原始流事件（SSE `data:` 后面那段原文）折成归一事件。认不出 ⇒ 空。
/// 按上游协议分，不按 agent 分：哪家走哪个协议由那一家的 [`DefaultUpstream::stream`] 说。
#[derive(Clone, Copy)]
pub struct StreamFace {
    pub fold: fn(&str) -> Vec<StreamEv>,
    /// 一件被截断的事件（只有开头那一截）⇒ 只从开头认得出的那几格（类型 · 应答标识）；认不出 ⇒ 空。不许出字。
    pub fold_clipped: fn(&str) -> Vec<StreamEv>,
    /// 一件事件里说出的这一发用量（[`Spent`]）；没说 ⇒ `None`。`None` 这一格 ＝ 这个协议的流说不出用量。
    pub(crate) spent: Option<fn(&str) -> Option<Spent>>,
}

/// 归一流事件（界面只收这个，不收任何一家的原始事件）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum StreamEv {
    /// 一次应答开始：对账键（与记录的 [`RecordFace::response_id`] 同一个值域）。
    Start { rid: String },
    /// 第 `i` 块开始了。
    Block {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        i: u64,
        kind: BlockKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(any(test, feature = "ts"), ts(optional))]
        tool: Option<String>,
    },
    /// 第 `i` 块的一段文字。
    Text {
        #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
        i: u64,
        s: String,
    },
    /// 上游说完了（`ok` ＝ 正常说完；`false` ＝ 上游报错收尾）。
    Stop { ok: bool },
}

/// 归一流里一块是什么：线上形住 `cmd-wire::live`（活卡每一块的种类）。
pub use cmd_wire::live::BlockKind;

/// 一家的默认上游：路由里叫它什么 · 盖掉内置默认的那个旋钮 · 内置默认。三格焊在一起
/// （分开取就写得出「A 家的旋钮配 B 家的默认值」—— 与上游选择那张表的 `Row` 焊住上游与 key 同一条理由）。
pub struct DefaultUpstream {
    /// 中转路由键第 1 段里这一家的名字（monitor 起会话拼 `/t/<它>/…`）。⚠ 与 [`Adapter::kind`]（wire 上的 `agent_kind`）
    /// 是两个值域：Claude 在 wire 上叫 `claude`、在路由里叫 `claude-code` —— 各有各的既有契约，不在这里对齐。
    pub route_id: &'static str,
    /// 盖掉内置默认的环境变量名。**每家一个**（「每家一个」：不留「覆盖哪一家说不清」的全局旋钮）。
    pub env: &'static str,
    /// 没配 `env` 时这一家发到哪儿（配了 ⇒ 一律发到那个值）。
    pub fallback: Fallback,
    /// 这一家的请求里**它自己带着会话标识**的那个头（中转拿它给流打标签）。`None` = 说不出 ⇒ 流不带标签。
    pub session_header: Option<&'static str>,
    /// 这一家的上游说哪种流协议（归一流的折法）。`None` ＝ 它的流不折（活卡认不得）。
    pub stream: Option<StreamFace>,
    /// 这一家要中转原样带回的请求头，各说什么（[`RelayHead`]）。通用层把各家的头名并成一张名单交中转（[`relay_headers`]），
    /// 中转随 0 号那一件交回 (名, 值)；流归位 · 上游选择 · 请求标记各按名从里面拿。空 ＝ 这一家的请求说不出这些。
    pub relay_heads: &'static [RelayHead],
    /// 走中转时随地址一起设的环境（名, 值）：凡是注中转地址的地方（起会话 exec 那一刻 · `--print` 那一行 · 设置文件的 `env` 块）都照这一份一起设。
    /// 例：要这一家的请求带上 [`RelayHead::Class`] 那个头的开关。空 ＝ 没有。
    pub relay_env: &'static [(&'static str, &'static str)],
    /// API key 凭据文件（界面给账号配第三方 key 时写的那一份）里的行挂在这一家名下。
    /// 那份文件没有 agent 这一维 ⇒ 至多一家为真；没有一家为真 ⇒ 上游选择起不来（fail-closed）。
    pub owns_credentials_file: bool,
    /// 直接敲的这一家也走中转（可选，用户自己贴、后端只读）：它的用户级设置文件里写进程环境的那一块。
    /// `None` ＝ 这一家没有这一形，只能经起会话注入。
    pub settings_env: Option<SettingsEnvFace>,
    /// 回包状态 ＋ 回包头（＋ 此刻，unix 秒）→ 通用的额度快照：这一家的读法（头名只住那一家）。
    /// `None` ＝ 这一家的回包说不出额度；`Some` 答 `None` ＝ 这一个回包里没有额度信息。
    pub quota: Option<QuotaRead>,
    /// 额度窗口名 → 语义位（`5h` / `7d`；别的窗口 ⇒ `None`）。界面与换号记录只认语义位，不认各家的窗口名。
    pub window_slot: Option<fn(&str) -> Option<&'static str>>,
    /// 额度窗口名 → 窗口键（轮换的上限 · 单段预算按它配；通用层当不透明的键，只认 `5h` · `7d` 两个缺省键）。
    /// 不算用量窗口的那几档（超额）⇒ `None`。
    pub window_key: Option<fn(&str) -> Option<String>>,
    /// 让这一家的官方客户端报一次某个号的用量（不花模型额度）：怎么起 · 怎么把输出读成窗口。`None` ＝ 这一家没有这一形。
    pub usage: Option<UsageFace>,
    /// 用一个号发一句最短的话、让它的 5 小时窗开始计时：交给 ccm 的那几个参数 · 起它时设上的环境。`None` ＝ 这一家没有这一形。
    pub warm: Option<WarmFace>,
    /// 订阅号登录那一格：令牌住哪、什么格式、怎么续、锁叫什么。`None` ＝ 这一家没有可换的订阅号登录。
    pub login: Option<LoginFace>,
    /// 换到别的号时这一发写哪几个头（[`SwapHead`]）：换之前下游带来的鉴权头全集并上这几个头名整条剥掉，再照写。
    /// 空 ＝ 这一家换号不成立，照起会话那个号走。
    pub swap_heads: &'static [SwapHead],
    /// 起会话时怎么把会话绑到号（[`Bind`]）。
    pub bind: Bind,
    /// 这一家自己认得的「用满」回包（轮换的硬上限用它：这一发不发上游、回这一份）。`None` ＝ 给不出 ⇒ 对它硬上限不成立、按软阈值办。
    pub limit_reply: Option<LimitReplyOf>,
    /// 回了「用满」之后，这一家的客户端自己等到重置再续的最远时长（秒）：重置在这之外 ⇒ 它不续、要人接着。`None` ＝ 它不等、或说不出。
    pub resumes_within: Option<u64>,
    /// 起会话时怎么把这一家指到中转（`ccm` 最终 exec 那一处照这一格做）。是 [`Inject::Env`] 的那一家，
    /// 继承来的那一条地址（用户自己的端点 · 别的号的中转）也只看那个变量。
    pub inject: Inject,
    /// 这一家自己认的出网设置变量（`ccm` 起会话时接管：免代理表加上中转口 · 那条会话的上游描述照它记）。
    pub proxy: ProxyFace,
}

/// 换到别的号时这一发写的一个头（[`DefaultUpstream::swap_heads`]）：头名 · 值的前缀 · 值从那个号的凭据里取哪一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapHead {
    pub name: &'static str,
    /// 拼在值前面的那一截（`Bearer ` 一类）；没有 ⇒ 空串。
    pub prefix: &'static str,
    pub cell: CredCell,
}

/// 头值从号的凭据里取哪一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredCell {
    /// 访问令牌（快过期就续）。
    Access,
    /// 凭据文件里那一节（[`LoginFace::section`]）点名的一格（串）。
    Field(&'static str),
}

/// 起会话时怎么把会话绑到号（[`DefaultUpstream::bind`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    /// 起之前先定会话号、随起会话交给它：起会话那个号照路由第 2 段。
    Preset,
    /// 先不知道会话号：中转按第一发带的来处，把这条会话绑到起会话时上游描述里记的那个号。
    Origin,
}

/// 某一家（wire 上的 kind）在给定注册表里起会话时怎么绑号（认不出 ⇒ [`Bind::Preset`]：照路由第 2 段）。
/// 中转那一侧手里是路由名，先经 [`kind_of_route`] 换。
pub fn bind_among(registry: &[Adapter], kind: &str) -> Bind {
    adapter_among(registry, kind)
        .and_then(|a| a.upstream.as_ref())
        .map_or(Bind::Preset, |u| u.bind)
}

/// ★ 记录里自报的额度认哪个号（唯一一处）：记录文件落在根表（[`record_roots_among`]）上那一家哪个号的家报的哪一根底下，就是哪个号
/// （记录树那一家一个家一棵树；合成历史那一家一个家可以几根，归档根也算）；落在几根底下 ⇒ 最深那根的号；一根都不是 ⇒ `None`（不猜）。
/// `accounts` ＝ 账号库里这一家的号：（号, 号目录）。
pub fn owner_of_record(
    registry: &[Adapter],
    kind: &str,
    record: &Path,
    accounts: &[(String, PathBuf)],
) -> Option<String> {
    let real = |p: &Path| crate::common::fence::real_path(p).unwrap_or_else(|| p.to_path_buf());
    let file = real(record);
    // 只问 `kind` 那一家的根表：注册表里取出那一行，单独交进去。
    let family = std::slice::from_ref(adapter_among(registry, kind)?);
    accounts
        .iter()
        .flat_map(|(id, dir)| {
            let home = |_: &Adapter| {
                vec![AccountHome {
                    account: Some(id.clone()),
                    home: dir.clone(),
                }]
            };
            record_roots_among(family, &home)
                .into_iter()
                .map(move |r| (real(&r.root), id))
        })
        .filter(|(root, _)| file.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .map(|(_, id)| id.clone())
}

/// 一家认哪几个出网设置变量（按先后，小写在前）。
#[derive(Debug, Clone, Copy)]
pub struct ProxyFace {
    /// 代理那几个（`https_proxy` 一类）。
    pub via: &'static [&'static str],
    /// 免代理表那几个（`no_proxy` 一类）。
    pub skip: &'static [&'static str],
}

/// 某一家（wire 上的 kind）在给定注册表里认的出网设置变量。认不出 / 没登记默认上游 ⇒ `None`。
pub(crate) fn proxy_among(registry: &[Adapter], kind: &str) -> Option<ProxyFace> {
    adapter_among(registry, kind)
        .and_then(|a| a.upstream.as_ref())
        .map(|u| u.proxy)
}

/// [`proxy_among`] 在生产注册表上。
pub fn proxy_of(kind: &str) -> Option<ProxyFace> {
    proxy_among(REGISTRY, kind)
}

/// 怎么把一家指到中转：只管地址放哪。地址由上游选择给（不带钥匙），`ccm` exec 那一刻插钥匙，插哪一把按地址的路由。
#[derive(Clone, Copy)]
pub enum Inject {
    /// 插上钥匙的地址放进这个环境变量。
    Env(&'static str),
    /// 地址拼成这几个参数（垫在透传之前）：地址只能经命令行参数交给它的那一家。计划里是不带钥匙的地址；
    /// 这一家只走 `/t/`（`/s/` 只给凭据文件那一家）⇒ 插进 argv 的恒是只许直通那一把。
    Args(fn(&str) -> Vec<String>),
}

/// 某一家（wire 上的 kind）在给定注册表里怎么指到中转。认不出 / 没登记默认上游 ⇒ `None`。
pub fn inject_among(registry: &[Adapter], kind: &str) -> Option<Inject> {
    adapter_among(registry, kind)
        .and_then(|a| a.upstream.as_ref())
        .map(|u| u.inject)
}

/// `kind` 那一家走中转时随地址一起设的环境（[`DefaultUpstream::relay_env`]）；没登记 ⇒ 空。
pub fn relay_env_among(
    registry: &[Adapter],
    kind: &str,
) -> &'static [(&'static str, &'static str)] {
    adapter_among(registry, kind)
        .and_then(|a| a.upstream.as_ref())
        .map_or(&[], |u| u.relay_env)
}

/// [`inject_among`] 在生产注册表上。
pub(crate) fn inject_of(kind: &str) -> Option<Inject> {
    inject_among(REGISTRY, kind)
}

/// 一家的内置默认上游：一个，或按这一发自己带没带某个头二选一（同一家两种登录各有真上游时；只看头名、不看值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fallback {
    One(&'static str),
    ByHeader {
        header: &'static str,
        present: &'static str,
        absent: &'static str,
    },
}

/// 一家的用户级设置文件里「上游地址」那一格：住哪 · 怎么读出来 · 怎么合进去（格式知识与那一次只读都在这一家）。
#[derive(Debug, Clone, Copy)]
pub struct SettingsEnvFace {
    /// 家目录（`$HOME`）→ 那份文件（各号共用的那一份）＋ 里面写的上游地址（**只读**）。
    pub read: fn(&Path) -> (PathBuf, SettingsBaseUrl),
    /// （那份文件现在的内容, 地址）→ 合好的整份（只算不写；「待办」按它算 diff）。现在的内容读不懂 ⇒ `None`。
    pub merge: fn(&str, &str) -> Option<String>,
    /// 地址住那份文件里哪一格（「待办」那一件的位置行）。
    pub slot: &'static str,
    /// 项目目录 → 那个项目里自己写了上游地址、盖住用户级那一格的那份设置文件（**只读**；没有 · 读不懂 ⇒ `None`）。
    pub covered_in: fn(&Path) -> Option<PathBuf>,
}

/// 设置文件里上游地址那一格读出来的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsBaseUrl {
    /// 文件不在，或没写（或写了空串）。
    Unset,
    /// 写了这个地址。
    Set(String),
    /// 读不了 / 读不懂 ⇒ 装没装说不清。
    Unreadable(SettingsUnreadable),
}

/// 为什么读不了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsUnreadable {
    /// 读的时候出错（权限 · I/O），带系统的原话。
    Io(String),
    /// 那不是一个普通文件。
    NotFile,
    /// 超过这个字节数。
    TooLarge(u64),
    /// 读得到但读不懂（坏 JSON · 顶层或那一块不是对象 · 值不是串）。
    BadShape,
}

/// 登记了默认上游的每一家：`(路由名, 那一格)`。**上游选择读默认上游的唯一入口**。
pub fn default_upstreams() -> impl Iterator<Item = &'static DefaultUpstream> {
    REGISTRY.iter().filter_map(|a| a.upstream.as_ref())
}

/// `registry` 里声明了「直接敲的也走中转」那一格的各家（注册表序）：（路由名, 给人看的名字（[`family_said_among`]）, 那一格）。
pub fn settings_env_families(registry: &[Adapter]) -> Vec<(&'static str, String, SettingsEnvFace)> {
    registry
        .iter()
        .filter_map(|a| {
            let u = a.upstream.as_ref()?;
            let face = u.settings_env?;
            Some((u.route_id, family_said_among(registry, a.kind), face))
        })
        .collect()
}

/// 各家登记的会话标识头（注册序、去重）：中转经上游选择拿到这份名单，按它从请求里认会话 —— 会话 id 归 agent 自己。
pub fn session_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for h in REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref()?.session_header)
    {
        if !v.contains(&h) {
            v.push(h);
        }
    }
    v
}

/// 一家要中转原样带回的一个请求头，它说什么（[`DefaultUpstream::relay_heads`]）。中转只认头名、不认这几种意思。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayHead {
    /// 请求本身就说出「我是哪个子运行」的那个头（值 ＝ 子运行的标识，与 [`RecordFace::run_of`] 同一个值域）。
    /// 声明了 ⇒ 带头的请求归那个子运行、不带头的就是主运行（当场定）；没声明 ⇒ 请求认不出运行 ⇒ 通用层按记录对账归位。
    Run(&'static str),
    /// 请求自己说出「这一发是不是对话」的那个头 ＋ 哪几个值算旁路（预测下一句 · 起标题 · 压缩总结那一类：不进记录、不上活卡）。
    /// 没声明 ⇒ 这一家的请求说不出 ⇒ 每一发都当对话。
    Class(SideClass),
    /// 请求里说明「这一轮用的是扩展上下文」的那一项：那个头的逗号列表里有一项以 `item` 开头（大小写不论）。会话事实据此定上限。
    Wide {
        header: &'static str,
        item: &'static str,
    },
}

impl RelayHead {
    fn name(&self) -> &'static str {
        match *self {
            Self::Run(h) | Self::Wide { header: h, .. } => h,
            Self::Class(c) => c.header,
        }
    }
}

/// 中转要原样带回的请求头名单（注册序、去重、不分大小写）：各家 [`DefaultUpstream::relay_heads`] 的头名，
/// 加上默认上游按带没带某个头二选一的那个头（[`Fallback::ByHeader`]）。中转经上游选择拿到这一份，值随 0 号那一件交回。
pub fn relay_headers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = Vec::new();
    for u in REGISTRY.iter().filter_map(|a| a.upstream.as_ref()) {
        let picks = match u.fallback {
            Fallback::ByHeader { header, .. } => Some(header),
            Fallback::One(_) => None,
        };
        for h in u.relay_heads.iter().map(RelayHead::name).chain(picks) {
            if !v.iter().any(|x| x.eq_ignore_ascii_case(h)) {
                v.push(h);
            }
        }
    }
    v
}

/// 带回的头里名为 `name` 的那一个的值（头名不分大小写；中转那一侧同名的已并成一个值）。
/// 扫的是这一发请求的头，不是注册表。
pub fn head_value<'h, V: AsRef<str>>(heads: &'h [(&str, V)], name: &str) -> Option<&'h str> {
    for (k, v) in heads {
        if k.eq_ignore_ascii_case(name) {
            return Some(v.as_ref());
        }
    }
    None
}

/// 这一发请求（中转带回的那几个头）说的是不是扩展上下文：有一家登记的 [`RelayHead::Wide`] 那个头里有那一项。
pub fn wide_context<V: AsRef<str>>(heads: &[(&str, V)]) -> bool {
    REGISTRY
        .iter()
        .filter_map(|a| a.upstream.as_ref())
        .flat_map(|u| u.relay_heads.iter())
        .any(|h| match *h {
            RelayHead::Wide { header, item } => head_value(heads, header).is_some_and(|v| {
                v.split(',').any(|t| {
                    let t = t.trim();
                    t.len() >= item.len() && t[..item.len()].eq_ignore_ascii_case(item)
                })
            }),
            _ => false,
        })
}

/// 一家的请求怎么说出「这一发是不是对话」（[`RelayHead::Class`]）：哪个头 · 那个头的哪几个值是旁路。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideClass {
    pub header: &'static str,
    pub side: &'static [&'static str],
}

impl SideClass {
    /// 这个值（请求里那个头的值）是不是旁路。
    pub fn is_side(&self, class: &str) -> bool {
        self.side.contains(&class)
    }
}

/// 一家上游在流归位那里的样子：它的流协议面 ＋ 它的请求自报运行的那个头（[`RelayHead::Run`]）＋ 怎么认旁路（[`RelayHead::Class`]）。
#[derive(Clone, Copy)]
pub struct StreamFamily {
    pub face: StreamFace,
    /// 自报运行的头：声明了 ⇒ 带头的归那个子运行、不带头的就是主运行（当场定，不挂起）。
    pub run: Option<&'static str>,
    /// 请求的分类头报的是旁路 ⇒ 那一段不上活卡。
    pub side: Option<SideClass>,
}

/// 各家上游的流协议面（注册序）。一条应答用哪一个，由通用层按头一件事认（认得出「开始」的那一个）。
pub fn stream_families() -> Vec<StreamFamily> {
    REGISTRY
        .iter()
        .filter_map(|a| {
            let u = a.upstream.as_ref()?;
            let mut f = StreamFamily {
                face: u.stream?,
                run: None,
                side: None,
            };
            // 这一家自己那几个头里，自报运行的与分类的各是哪一个。
            for h in u.relay_heads {
                match *h {
                    RelayHead::Run(n) => f.run = Some(n),
                    RelayHead::Class(c) => f.side = Some(c),
                    RelayHead::Wide { .. } => {}
                }
            }
            Some(f)
        })
        .collect()
}

/// 某一家找上游读的那个环境变量（注入格是 [`Inject::Env`] 的那个变量）。认不出这家 / 没登记上游 / 地址只能拼进参数 ⇒ `None`
/// （`ccm` 对它不往环境里注入中转地址、也不清继承来的那一条）。
pub fn base_url_env_of(kind: &str) -> Option<&'static str> {
    match inject_of(kind) {
        Some(Inject::Env(v)) => Some(v),
        _ => None,
    }
}
