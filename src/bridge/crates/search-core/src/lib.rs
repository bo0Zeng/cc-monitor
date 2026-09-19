//! 历史全文搜索**口径**的唯一实现 —— monitor 的内存索引（`src/bridge/src/search.rs`）
//! 与 daemon 的 `--search`（`src/backend/observe/search_query.rs`）共用这一份。
//!
//! # 它治的是「今天没漂，而没人拦着它漂」
//!
//! 收口前，下面这 12 个助手在两侧**各写一遍**（`K-R85` 09-12 实测：逐字相同，没漂）：
//! `extract_text_blocks` · `extract_tool_text` · `stringify_json` · `clean_user_text` ·
//! `make_snippet` · `find_ci` · `tail_chars` · `head_chars` · `collapse_ws` ·
//! `collapse_ws_keep_ellipsis` · `truncate_plain` · `truncate_excerpt`。
//! 「没漂」不是保障 —— 同一轮实测还量到：`cross_half_edge_registry::CROSS_EDGES` 17 条
//! 跨轨边里 **search 零命中**，即**没有任何判据在对拍这两份**。两份实现 + 零判据 =
//! 下一次谁改一侧，另一侧静默留在原地，而**搜索结果不一致是不会报错的**
//! （本地一份 snippet、远端另一份，谁都不抛异常）。
//!
//! # 三样东西住在这里，各有各的理由
//!
//! 1. **抽取 / 匹配 / snippet 的 12 个纯函数** —— 上面那一族。
//! 2. **口径常量**（`MAIN_CAP` / `TOOL_CAP` / `SNIPPET_CTX` / `PER_SESSION_CAP` /
//!    `DEFAULT_LIMIT`）—— 它们**就是口径本身**，两侧各写一个字面量 = 两份口径。
//! 3. **snippet 预算**（`SnippetBudget`）—— 见下。
//!
//! # 🔴 为什么预算也在这里：它管的两件事此前混成了一个值
//!
//! 「这条命中要不要给 snippet」有**两个互相独立**的理由说不给：
//! **① 全局预算（`--limit`）用完了** · **② 本会话已经给满 `PER_SESSION_CAP` 条**。
//! 收口前两侧都写成一个 `if kept < limit && hits.len() < CAP`，于是下游只看得到
//! `hitCount > hits.len()` 这**一个**信号 —— 而这两件事对用户的意思完全不同：
//! ① 是「**你的结果被砍了，缩小范围或加大 limit**」，② 是「这个会话话太多，点进去看」。
//! `SnippetVerdict` 把它们拆成两个值，`SnippetBudget::take` 是唯一的判定处。
//!
//! # 🔴 预算的花法：**最近优先**（`sort_by_recency`）
//!
//! 收口前 monitor 按 `updated_at` desc 花预算、daemon 按 `WalkDir`（= `readdir`）
//! 先走到的顺序花。**两侧的展示顺序都是 `updatedAt` desc**（`merge_search_results`
//! 合并后重排、前端照序渲染）⇒ 预算顺序一旦与展示顺序不同，**缺 snippet 的正好是列表最上面
//! 那几张卡**。理由与读数逐条写在 `sort_by_recency` 的文档注释里。

use serde_json::Value;

// ── 口径常量 ─────────────────────────────────────────────────────────────
// ⚠ 这些数**就是口径**。两侧任何一处再写一遍同样的字面量 = 又开了第二份口径，
//   由 `search_kou_jing_guard.rs::the_search_kou_jing_has_exactly_one_home` 逐个字面量扫着。

/// 单条 user/assistant 正文的索引上限（字符）。正文极少超，兜底防超长粘贴。
pub const MAIN_CAP: usize = 20_000;
/// 单条 tool 文本的索引上限（字符）。`tool_result` 可能是几百 KB 的文件 dump，必须封顶。
pub const TOOL_CAP: usize = 4_000;
/// snippet 命中点前后各保留的字符数。
pub const SNIPPET_CTX: usize = 48;
/// 单会话最多返回的 snippet 条数（防一个会话刷屏；命中总数仍报全量）。
pub const PER_SESSION_CAP: usize = 30;
/// `limit` 缺省值（IPC 与 `--limit` 两侧同）。
pub const DEFAULT_LIMIT: usize = 300;
/// `limit` 的下界 / 上界。
pub const LIMIT_MIN: usize = 1;
pub const LIMIT_MAX: usize = 2_000;

/// 把外部传进来的 `limit` 收进合法区间（两侧同一口径）。
pub fn clamp_limit(n: usize) -> usize {
    n.clamp(LIMIT_MIN, LIMIT_MAX)
}

// ── snippet 预算 ─────────────────────────────────────────────────────────

/// 一条命中要不要给 snippet —— **以及不给的话是为什么**。
///
/// 🔴 `BudgetExhausted` 与 `SessionCapped` 是**两件不同的事**，别再合成一个 bool：
/// 前者是「整份结果被砍了」（该让用户知道，缩小范围/加大 limit），
/// 后者是「这个会话话太多、只列前 30 条」（点进去看就行，结果没被砍）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetVerdict {
    /// 给 snippet（预算已记账）。
    Give,
    /// 🔴 **全局 `limit` 用完了** —— 这一条以及之后的都没有 snippet。
    BudgetExhausted,
    /// 本会话已经给满 `PER_SESSION_CAP` 条。整份结果**没有**被砍。
    SessionCapped,
}

/// 全局 snippet 预算。**判定只有这一处**，两侧都调它。
///
/// 收口前两侧各写一个 `if kept < limit && hits.len() < CAP`：条件相同，但
/// **谁都不记得自己是因为哪一条不给的** ⇒ 下游只好用 `hitCount > hits.len()` 反推，
/// 而那个式子对两种原因给出同一个答案。
#[derive(Debug)]
pub struct SnippetBudget {
    limit: usize,
    spent: usize,
    /// 是否**曾经**因为全局预算用完而拒绝过一条命中。
    starved: bool,
}

impl SnippetBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            spent: 0,
            starved: false,
        }
    }

    /// 判定一条命中。`shown_in_session` = 本会话**已经**给出的 snippet 条数。
    /// 返回 `Give` 时预算已记账，调用方必须真的构造 snippet。
    pub fn take(&mut self, shown_in_session: usize) -> SnippetVerdict {
        if self.spent >= self.limit {
            self.starved = true;
            return SnippetVerdict::BudgetExhausted;
        }
        if shown_in_session >= PER_SESSION_CAP {
            return SnippetVerdict::SessionCapped;
        }
        self.spent += 1;
        SnippetVerdict::Give
    }

    /// 已花掉的 snippet 数。
    pub fn spent(&self) -> usize {
        self.spent
    }

    /// 🔴 **整份结果被全局预算砍过** —— 这才是该告诉用户的那件事。
    /// 单会话超 30 条（`SessionCapped`）**不算**。
    pub fn starved(&self) -> bool {
        self.starved
    }
}

// ── 预算顺序 ─────────────────────────────────────────────────────────────

/// 🔴 **snippet 预算按「最近优先」花** —— 两侧唯一的排序处。
///
/// `key` 取会话的 `updatedAt`（epoch ms，= jsonl 的 mtime）。原地降序。
///
/// # 为什么是「最近优先」，而不是「文件系统先走到的顺序」
///
/// **不是偏好，是因为展示顺序已经定死了。** 合并后 `merge_search_results` 逐字
/// `sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at))`，前端 `renderSearchResults`
/// 照这个顺序渲染 ⇒ **预算顺序 ≠ 展示顺序时，缺 snippet 的正好是列表最上面那几张卡**。
///
/// # 读数（09-13 实测，语料 = 本机 `~/.claude/projects` 169 个 jsonl / 49 个项目目录）
///
/// ① **两种顺序几乎正交**：走序前 3 与最近序前 3 **重合 0/3**；前 10 重合 2/10；前 50 重合 13/50。
///    走序前 10 在「最近序」里的名次是 `[6, 85, 96, 94, 82, 119, 118, 4, 52, 50]`。
/// ② **走序花的是旧会话**：走序前 3 的中位年龄 **16.1 天**，最近序前 3 是 **0.01 天**。
/// ③ 🔴 **决定性的一格 —— 展示序最上面 10 张卡里「一条 snippet 都没有」的张数**
///    （`--limit 50`，三个真实查询词）：
///    `docker` 走序 **6/10** → 最近序 **2/10** · `门禁` 走序 **9/10** → 最近序 **8/10** ·
///    `search` 走序 **10/10** → 最近序 **6/10**。**三个词全部改善。**
/// ④ ⚠ **如实记一格反例，别只报好看的**：按「命中会话里 `hits: []` 的**总**张数」量，
///    最近优先**不占优**（`docker` 21→20 · `门禁` 22→**26** · `search` 41→**46**）——
///    因为最近的那个会话往往命中最多，一口气吃掉大半预算。
///    ⇒ 「最近优先」买到的**不是「少几张空卡」，是「空卡不在你眼前」**。这正好是 ③ 那一格。
/// ⑤ **走序还不是一个「顺序」**：`WalkDir` 走的是 `readdir` 返回序（ext4 上是目录哈希序），
///    它随文件系统状态变、跨机器不同 ⇒ 同一个查询在两台远端上给出的 snippet 集合**不可复现**，
///    而差异与「哪条更相关」无关。最近序是**内容决定的**，两台机器上同义。
pub fn sort_by_recency<T>(items: &mut [T], key: impl Fn(&T) -> i64) {
    items.sort_by(|a, b| key(b).cmp(&key(a)));
}

// ── 文本抽取 ─────────────────────────────────────────────────────────────

/// 抽 `content` 里所有 text block（或裸字符串）。user 正文 / assistant 正文都用它。
pub fn extract_text_blocks(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for b in arr {
                if b.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(s) = b.get("text").and_then(Value::as_str) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(s);
                    }
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// 抽 tool 相关内容（可选搜索）。
/// - assistant：`tool_use`（name + input JSON）+ `thinking`
/// - user：`tool_result` 的 content
pub fn extract_tool_text(content: &Value, is_assistant: bool) -> String {
    let Value::Array(arr) = content else {
        return String::new();
    };
    let mut out = String::new();
    let mut push = |s: &str| {
        if s.is_empty() {
            return;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(s);
    };
    for b in arr {
        match b.get("type").and_then(Value::as_str) {
            Some("tool_use") if is_assistant => {
                if let Some(name) = b.get("name").and_then(Value::as_str) {
                    push(name);
                }
                if let Some(input) = b.get("input") {
                    push(&stringify_json(input));
                }
            }
            Some("thinking") if is_assistant => {
                if let Some(t) = b.get("thinking").and_then(Value::as_str) {
                    push(t);
                }
            }
            Some("tool_result") if !is_assistant => {
                if let Some(c) = b.get("content") {
                    push(&stringify_json(c));
                }
            }
            _ => {}
        }
    }
    out
}

/// 把 JSON 值压成可搜索的纯文本（string 直接取；array/object 取其中字符串叶子）。
pub fn stringify_json(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let mut out = String::new();
            for item in arr {
                // `tool_result.content` 常是 `[{type:"text", text:"..."}]`
                let s = if let Some(t) = item.get("text").and_then(Value::as_str) {
                    t.to_string()
                } else {
                    stringify_json(item)
                };
                if !s.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&s);
                }
            }
            out
        }
        Value::Object(_) => v.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
    }
}

/// CLI 注入的 prompt 包装 —— 剥掉它们，搜索只命中真内容（INVARIANT § 20 同一意图，从宽）。
const INJECTED_WRAPPERS: [&str; 5] = [
    "task-notification",
    "system-reminder",
    "local-command-caveat",
    "local-command-stdout",
    "local-command-stderr",
];

/// 去掉 CLI 注入的 prompt 包装 + ESC 中断标记。
pub fn clean_user_text(s: &str) -> String {
    let mut out = s.to_string();
    for tag in INJECTED_WRAPPERS {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(i), Some(j)) = (out.find(&open), out.find(&close)) {
            if j > i {
                out.replace_range(i..j + close.len(), "");
            } else {
                break;
            }
        }
    }
    let trimmed = out.trim();
    // 纯 ESC 中断标记 → 不是真用户内容
    if trimmed.starts_with("[Request interrupted by user") {
        return String::new();
    }
    trimmed.to_string()
}

// ── snippet ──────────────────────────────────────────────────────────────

// 🔴 `#[inline]` 不是「优化癖」，是**为了让「收口径」这件事不带性能代价**〔`K-R100`〕。
//
// 收口前这几个助手与调用点同 crate，编译器随手内联。搬进本 crate 之后它们成了跨 crate
// 调用 —— 泛型/`#[inline]` 之外的函数**不会**跨 crate 内联（除非开 LTO，而 `cargo test`
// 的 profile 不开）。实测（同进程配对台架，5 轮中位 ×3 趟）：不加 `#[inline]` 时
// `make_snippet` 这条路比收口前的本地副本慢 **+1.4% / +1.2% / +0.7%**。
// 数字很小，但**方向是退**，而 `K-R100` 的射程里逐字写着「性能不许退」⇒ 标上。

/// 在 `text` 里大小写不敏感地定位 `needle_lc`（已小写），返回前 / 中 / 后三段。
/// 找不到（理论上不会，调用前已 `contains` 粗筛过）则退化成开头窗口。
#[inline]
pub fn make_snippet(text: &str, needle_lc: &str) -> (String, String, String) {
    match find_ci(text, needle_lc) {
        Some((start, end)) => (
            tail_chars(&text[..start], SNIPPET_CTX),
            collapse_ws(&text[start..end]),
            head_chars(&text[end..], SNIPPET_CTX),
        ),
        None => (
            String::new(),
            String::new(),
            head_chars(text, SNIPPET_CTX * 2),
        ),
    }
}

/// 大小写不敏感子串查找：返回原文里匹配区间的 `(起始字节, 结束字节)`。
/// 逐字符比对原文的小写展开 vs needle（needle 已小写）。只对粗筛命中的 ≤limit 条跑，
/// `O(n·m)` 可接受。能正确处理 CJK（无大小写）与 ASCII，多字符小写展开也对齐到原文字符边界。
#[inline]
pub fn find_ci(hay: &str, needle_lc: &str) -> Option<(usize, usize)> {
    if needle_lc.is_empty() {
        return None;
    }
    let needle: Vec<char> = needle_lc.chars().collect();
    let hay_idx: Vec<(usize, char)> = hay.char_indices().collect();
    let n = hay_idx.len();

    for i in 0..n {
        let mut hi = i; // 原文字符下标
        let mut ni = 0; // needle 字符下标
        let mut pending: Vec<char> = Vec::new(); // 当前原文字符的小写展开缓冲
        let mut pi = 0;
        let start_byte = hay_idx[i].0;
        let mut end_byte = start_byte;
        let mut ok = true;

        while ni < needle.len() {
            if pi >= pending.len() {
                if hi >= n {
                    ok = false;
                    break;
                }
                pending.clear();
                pending.extend(hay_idx[hi].1.to_lowercase());
                pi = 0;
                end_byte = if hi + 1 < n {
                    hay_idx[hi + 1].0
                } else {
                    hay.len()
                };
                hi += 1;
            }
            if pending[pi] != needle[ni] {
                ok = false;
                break;
            }
            pi += 1;
            ni += 1;
        }
        if ok && ni == needle.len() {
            return Some((start_byte, end_byte));
        }
    }
    None
}

/// 取字符串末尾 n 个字符（折叠空白），不足则全取；截断时前缀 `…`。
#[inline]
pub fn tail_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let truncated = chars.len() > n;
    let slice: String = chars[chars.len().saturating_sub(n)..].iter().collect();
    let collapsed = collapse_ws(&slice);
    if truncated {
        format!("…{collapsed}")
    } else {
        collapsed
    }
}

/// 取字符串开头 n 个字符（折叠空白），截断时后缀 `…`。
#[inline]
pub fn head_chars(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (count, ch) in s.chars().enumerate() {
        if count >= n {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    collapse_ws_keep_ellipsis(&out)
}

/// 折叠所有空白（含换行）为单空格，trim。
#[inline]
pub fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 同 `collapse_ws` 但保留尾部 `…`。
#[inline]
pub fn collapse_ws_keep_ellipsis(s: &str) -> String {
    let has_ellipsis = s.ends_with('…');
    let core = if has_ellipsis {
        &s[..s.len() - '…'.len_utf8()]
    } else {
        s
    };
    let collapsed = collapse_ws(core);
    if has_ellipsis {
        format!("{collapsed}…")
    } else {
        collapsed
    }
}

/// 按字符硬截断（不加 `…`，用于索引正文封顶）。
#[inline]
pub fn truncate_plain(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect()
}

/// 按字符截断（换行折成空格，加 `…`，用于标题 excerpt）。
#[inline]
pub fn truncate_excerpt(s: &str, n: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        out.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
    }
    out
}

/// 会话标题：ai-title / 首条 user 摘要 / sid 前 8 位，三选一（两侧同一口径）。
pub fn session_title(ai_title: Option<&str>, first_user_excerpt: &str, session_id: &str) -> String {
    ai_title
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| (!first_user_excerpt.is_empty()).then(|| first_user_excerpt.to_string()))
        .unwrap_or_else(|| session_id.chars().take(8).collect())
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/crates/search-core/lib_tests.rs"]
mod tests;
