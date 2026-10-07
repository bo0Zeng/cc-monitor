//! 路由表：路由键里 agent ＋ 账号那两段 → 上游与 key 焊在一起的一个值。
//!
//! # 表的键是 `(agent, 账号)`，不是账号
//!
//! 只按账号查，claude 的 3 号与 codex 的 3 号就是同一行 ⇒ codex 会话会拿到 claude 那一行的上游与 key、错发到 Anthropic。
//! 键两段，一段都不许省：查 `(codex, 3)` 查不到 ⇒ `/s/` 回 404，一个字节不发上游。
//! 凭据文件本身没有 agent 这一维（`creds-core` 那份格式只有账号 id）：装表时每一行挂在谁名下由调用方显式交进来
//! （[`build`] 的 `agent` 入参，今天恒是 `super::CREDENTIALS_FILE_AGENT`）。本模块自己不认识任何一个 agent 的名字。
//!
//! # 上游与 key 只能同源
//!
//! 最坏的失效形态是「拿 A 账号的 key 去发 B 账号的请求，而两边看起来都成功了」⇒ 进程里没有「默认上游 ＋ 默认 key」两个各取各的字段。
//! 守它的是：一次请求只拿到一个 `Destination`（上游与 key 是同一个变体的两个字段，`resolve` 只给一个）· [`Row`] 的访问器只有
//! `accounts/` 里面看得见（中转连 `Row` 这个类型都点不到）· 装表只有一处（`table_guard` 的相等断言）。类型挡得住「顺手」、挡不住「有意重建」，
//! 真正量它的是 `KH2`/`KH4` 那几条走真子进程、真转发的行为判据与 `wire_golden` 的字节金标准。
//!
//! # 「这一行的 `base_url` 缺席」与「这一行不在表里」是两件事
//!
//! - 缺席 ⇒ 用这一行所属那个 agent 的默认上游（每 agent 一行，`agents::Adapter::upstream`）。那是一个已经存在的行的字段取默认值，不是回落。
//!   进程级的那一个默认上游（旧址 `server.rs:24`）不存在。
//! - 不在表里 ⇒ 404，一个字节都不发上游。

use crate::relay::{segment_is_safe, Base};
use copy_core::copy_text;
use creds_core::store::{AccountEntry, AuthStyle, AuthStyleSetting};

// `Row` / `RoutingTable` 的访问器是 `pub(super)`：只有 `accounts/` 里面看得见，中转连 `Row` 这个类型都点不到（编译器守）。
use creds_core::SecretKey;
use std::collections::BTreeMap;

/// 表里的一行：这条路由发到哪儿 + 用哪把 key。
///
/// 字段私有；三个访问器（[`Row::base`] · [`Row::key`] · [`Row::auth_style`]）全是 `pub(super)`。中转（`server.rs` / `listen.rs` /
/// `http1.rs` / `tee.rs`）手里只有 `resolve` 递过来的一个 `Destination`。
/// 刻意没有 `derive(Debug)`：`SecretKey` 自己的 `Debug` 是遮蔽形，但少一个能顺手把整行印出来的入口就少一个出口。
pub(crate) struct Row {
    base: Base,
    key: Option<SecretKey>,
    /// 这一把 key 用哪种鉴权头交给上游。焊在行上而不是进程级设置：上游、key、鉴权头形状是同一个决定的三个面，
    /// 分开取就写得出「A 的端点 + B 的 key + C 的头风格」，症状是 401，与「key 打错了」同形。
    /// 它不是方言（见 `creds_core::store::AUTH_STYLE_FIELD` 头注）：中转对 body 零解析。
    auth_style: AuthStyle,
}

impl Row {
    /// 这一行发到哪儿。层间契约要它出去（`Destination::{Passthrough,Substitute}` 带着 `upstream`，连上游是中转的活）；
    /// 可见性只到 `accounts/`。
    pub(super) fn base(&self) -> &Base {
        &self.base
    }

    /// 这一行的鉴权头形状。**不是**方言。
    pub(super) fn auth_style(&self) -> AuthStyle {
        self.auth_style
    }

    /// 这一行的 key。`None` = 原样转发下游那份鉴权头 —— 一个合法状态（订阅登录那一档 `acct_core::AUTH_KIND_SUBSCRIPTION` 本来就不换头），
    /// 不是「这一行不存在」；两件事要两条判据。
    pub(super) fn key(&self) -> Option<&SecretKey> {
        self.key.as_ref()
    }
}

/// `(agent, 账号 id)` → [`Row`]。一个进程一张，跨连接共享。键是二元组而不是拼接串：拼接串要靠「两段里都不许出现分隔符」才不撞。
pub(crate) struct RoutingTable {
    rows: BTreeMap<(String, String), Row>,
}

impl RoutingTable {
    /// 整个后端生产段里唯一一处造 `Row` 的地方（`table_guard::the_only_place_that_welds_an_upstream_to_a_key_is_inside_upstream_selection` 钉着）。
    /// 收分开的五样（agent / id / 上游 / key / 鉴权头形状），出焊死的一样。agent 只进键，不进 [`Row`]：「这一行属于谁」是索引，不是要发出去的字节。
    pub(crate) fn build(
        entries: impl IntoIterator<Item = (String, String, Base, Option<SecretKey>, AuthStyle)>,
    ) -> Self {
        let mut rows = BTreeMap::new();
        for (agent, id, base, key, auth_style) in entries {
            // 排版随便拆，形状不能改：`table_guard` 先把生产段的空白全删干净，再找无空白形的针 `Row{base`（`table_guard::WELD`），
            // 要求恰好 1 处。会让它红的是内容：`base` 不再紧跟 `Row {`、`base` 改名、改用 `Row::new(…)` 或 `..` 更新语法、把这处焊接搬出本文件，
            // 或生产段里再出现第二处同形字面量。真要写成那些形状，改的是判据里那根针，不是扭排版去迁就它。
            rows.insert(
                (agent, id),
                Row {
                    base,
                    key,
                    auth_style,
                },
            );
        }
        Self { rows }
    }

    /// 查一条。查不到就是 `None` —— 调用方回 404，不许拿别的行顶上。两段都是键：不许「agent 查不到就只按账号再查一次」。
    pub(super) fn lookup(&self, agent: &str, account: &str) -> Option<&Row> {
        self.rows.get(&(agent.to_string(), account.to_string()))
    }

    /// 这一家在表里有哪几条账号 id（有序）。**「表里有哪几行」的唯一出处**（`file_face::rows_at`）。
    pub(crate) fn ids_of(&self, agent: &str) -> Vec<String> {
        self.rows
            .keys()
            .filter(|(a, _)| a == agent)
            .map(|(_, id)| id.clone())
            .collect()
    }

    /// 表里有几行。只给日志与判据用。
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
}

/// 一条进不了表的账号，以及它为什么进不了。必须说出去（`creds::announce`）：静默丢掉一行的症状是「我明明配了，请求永远 404」。
pub(crate) struct Rejected {
    pub(crate) id: String,
    /// 固定文案（**不含文件内容**）⇒ 它进日志是安全的，理由登记在 `creds_guard::ALLOWED_LOG_ARGS`。
    pub(crate) why: &'static str,
}

/// 一条进了表、但有一件事必须让人知道的账号。不是 `Rejected` 多一个字段：「这一行不能用」（404）与
/// 「这一行能用，但行为与默认不同」（字节变了但仍然发出去）是两件事。`what` 是 `&'static str`：进日志安全由类型兜着。
pub(crate) struct Note {
    pub(crate) id: String,
    pub(crate) what: &'static str,
}

/// 账号 id 当不了路由段。
pub(crate) static WHY_ID_UNUSABLE: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.whyIdUnusable.say", &[]));

/// 明文 http 只许连回环。本地部署（`http://127.0.0.1:11434/...` 这一类）是一等公民 ⇒ 回环上的明文放行；
/// 非回环 + 明文 = 那一行的 key 明着过网线 ⇒ 拒掉并出声，不是出声之后照发。
pub(crate) static WHY_PLAINTEXT_OFF_LOOPBACK: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.whyPlaintextOffLoopback.say", &[]));

/// 一条 `base_url` 能用 ⇒ 装成 `Base`；不能用 ⇒ 那一句（装表与写口 `file_face.rs` 共用）。
/// 明文只许回环判的是**这一刻的字面**，不是「连出去之后落到哪」（解析到回环的域名照样拒）。
pub(crate) fn base_if_usable(url: &str) -> Result<Base, &'static str> {
    let base = Base::parse(url).map_err(|i| i.0)?;
    match upstream_url_core::usable(url) {
        Err(upstream_url_core::Unusable::PlaintextOffLoopback) => {
            Err(WHY_PLAINTEXT_OFF_LOOPBACK.as_str())
        }
        // 形状那几形上面 `Base::parse` 已经说过（同一个 `parse`）。
        _ => Ok(base),
    }
}

/// `auth_style` 写了一个认不出的词。**刻意不回落成默认值**。
pub(crate) static WHY_AUTH_STYLE_UNKNOWN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.whyAuthStyleUnknown.say", &[]));

/// `auth_style` 说「一个鉴权头都不发」，而同一行又配了一把 key：两句话相反 ⇒ 不猜（猜「用 key」就把真 key 发给声明不要鉴权的端点；
/// 猜「不发」就让人以为配好的 key 在生效）⇒ 拒掉并出声，让人自己删掉其中一句。
pub(crate) static WHY_NO_AUTH_WITH_KEY: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.whyNoAuthWithKey.say", &[]));

/// 这一行的 `base_url` 带了路径前缀。
pub(crate) static NOTE_PATH_PREFIX: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.notePathPrefix.say", &[]));

/// 这一行用 `Authorization: Bearer` 换头。今天印不出来（那正是 `AuthStyle::DEFAULT`，默认那个不出声）；
/// 留着：`note_for_auth_style` 的穷尽 `match` 要求每个成员都有话说，默认值哪天换了它不会借隔壁那句报假话。
pub(crate) static NOTE_AUTH_STYLE_BEARER: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.noteAuthStyleBearer.say", &[]));

/// 这一行用 `x-api-key` 换头。
pub(crate) static NOTE_AUTH_STYLE_X_API_KEY: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.noteAuthStyleXApiKey.say", &[]));

/// 这一行一个鉴权头都不发。
pub(crate) static NOTE_AUTH_STYLE_NO_AUTH: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("beUpstreamTable.noteAuthStyleNoAuth.say", &[]));

/// 一个非默认的鉴权头形状该报哪一句；默认那个 ⇒ `None`（每次都印等于噪音）。「哪个是默认」问 `AuthStyle::DEFAULT`，
/// `every_auth_style_other_than_the_default_gets_announced` 核「非默认的每一个都有话说」。
fn note_for_auth_style(style: AuthStyle) -> Option<&'static str> {
    if style == AuthStyle::DEFAULT {
        return None;
    }
    // 穷尽 `match`：加一个成员编译不过，而不是静默地不出声。
    Some(match style {
        AuthStyle::Bearer => NOTE_AUTH_STYLE_BEARER.as_str(),
        AuthStyle::XApiKey => NOTE_AUTH_STYLE_X_API_KEY.as_str(),
        AuthStyle::NoAuth => NOTE_AUTH_STYLE_NO_AUTH.as_str(),
    })
}

/// 把文件里读出来的那些条，装成一张表。每一条都挂在 `agent` 名下。
///
/// `default_base` 是 `agent` 那一家的默认上游（`agents::Adapter::upstream`），只给「这一行没写 `base_url`」那一格取值，不是回落。
///
/// # 「装不进去」的判断都在这里，都出声
///
/// 1. 账号 id 当不了路由段 —— 走 [`segment_is_safe`]，与 `route::parse` 同一个谓词；装进去也永远匹配不上任何请求。
/// 2. `base_url` 解析不了 —— 走 `Base::parse`。不回落到默认上游：打错的端点回落到官方端点，症状是「配了第三方 API，它却在用官方的」。
/// 3. 明文 http 指向非回环 —— 见 [`WHY_PLAINTEXT_OFF_LOOPBACK`]。
/// 4. `auth_style` 认不出 / 说不发头却又配了 key —— 见 [`WHY_AUTH_STYLE_UNKNOWN`] / [`WHY_NO_AUTH_WITH_KEY`]。都不回落。
///
/// 次序：id → `base_url` 形状 → 明文/回环 → `auth_style` 认不认得 → 「不发头」与 key 的矛盾（越前面越是整行用不了）；一条只报第一个理由。
///
/// 不判 key 像不像一把 key（人写进去什么，上游就该收到什么）；不判路径前缀对不对（那要打真网才知道）⇒ 那一格换来一条 [`Note`]。
pub(crate) fn build(
    entries: Vec<AccountEntry>,
    agent: &str,
    default_base: &Base,
) -> (RoutingTable, Vec<Rejected>, Vec<Note>) {
    let mut rows: Vec<(
        String,
        String,
        Base,
        Option<creds_core::SecretKey>,
        AuthStyle,
    )> = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();

    for e in entries {
        if !segment_is_safe(&e.id) {
            rejected.push(Rejected {
                id: e.id,
                why: WHY_ID_UNUSABLE.as_str(),
            });
            continue;
        }
        // 「能不能用」（形状 ＋ 明文只许回环）只有一份：`upstream_url_core::usable`，与写口 · 界面同一条；理由来自判定自己（逐形一句）。
        let base = match e.base_url.as_deref() {
            None => default_base.clone(),
            Some(u) => match base_if_usable(u) {
                Ok(b) => b,
                Err(why) => {
                    rejected.push(Rejected { id: e.id, why });
                    continue;
                }
            },
        };
        let auth_style = match e.auth_style {
            AuthStyleSetting::Absent => AuthStyle::DEFAULT,
            AuthStyleSetting::Known(s) => s,
            AuthStyleSetting::Unknown => {
                rejected.push(Rejected {
                    id: e.id,
                    why: WHY_AUTH_STYLE_UNKNOWN.as_str(),
                });
                continue;
            }
        };
        if auth_style == AuthStyle::NoAuth && e.key.is_some() {
            rejected.push(Rejected {
                id: e.id,
                why: WHY_NO_AUTH_WITH_KEY.as_str(),
            });
            continue;
        }
        // ⇒ 到这里这一行是**能用的**；下面两条只是「它的行为与默认不同，得让人知道」。
        if !base.path.is_empty() {
            notes.push(Note {
                id: e.id.clone(),
                what: NOTE_PATH_PREFIX.as_str(),
            });
        }
        if let Some(what) = note_for_auth_style(auth_style) {
            notes.push(Note {
                id: e.id.clone(),
                what,
            });
        }
        rows.push((agent.to_string(), e.id, base, e.key, auth_style));
    }

    (RoutingTable::build(rows), rejected, notes)
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/table_tests.rs"]
mod tests;
