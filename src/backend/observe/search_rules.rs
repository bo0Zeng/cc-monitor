//! 要求：「通用搜索口径与多机合并排序 → 后端 `observe/`」· `INVARIANTS §42`（`--search` 条：
//! 「① **行序 = snippet 预算顺序 = 最近优先**」）· 口径只有一个家。
//!
//! 原共享 crate `search-core` 的**通用那一半**：口径常量 · snippet 预算（「全局预算用完」与「单会话满」拆成两个值）·
//! 预算顺序（最近优先）· 片段 / 找词 / 截断 · 标题三选一。与哪一家 agent 无关；记录里正文 / 工具内容怎么抽、
//! CLI 注入的包装怎么剥是 Claude 记录格式的知识，住适配层 `agents/claudecode/text.rs`（通用层经注册表 `agents::TextFace` 够）。
//! 全仓只有这一份，由 `search_rules_tests.rs::the_search_kou_jing_has_exactly_one_home` 钉。

// ── 口径常量 ─────────────────────────────────────────────────────────────
// ⚠ 这些数**就是口径**。任何一处再写一遍同样的字面量 = 又开了第二份口径，
//   由 `search_rules_tests.rs::the_search_kou_jing_has_exactly_one_home` 逐个字面量扫着。

/// 单条 user/assistant 正文的索引上限（字符）。正文极少超，兜底防超长粘贴。
pub(crate) const MAIN_CAP: usize = 20_000;
/// 单条 tool 文本的索引上限（字符）。`tool_result` 可能是几百 KB 的文件 dump，必须封顶。
pub(crate) const TOOL_CAP: usize = 4_000;
/// snippet 命中点前后各保留的字符数。
pub(crate) const SNIPPET_CTX: usize = 48;
/// 单会话最多返回的 snippet 条数（防一个会话刷屏；命中总数仍报全量）。
pub(crate) const PER_SESSION_CAP: usize = 30;
/// `limit` 缺省值（IPC 与 `--limit` 两侧同）。
pub(crate) const DEFAULT_LIMIT: usize = 300;
/// `limit` 的下界 / 上界。
pub(crate) const LIMIT_MIN: usize = 1;
pub(crate) const LIMIT_MAX: usize = 2_000;

/// 把外部传进来的 `limit` 收进合法区间（两侧同一口径）。
pub(crate) fn clamp_limit(n: usize) -> usize {
    n.clamp(LIMIT_MIN, LIMIT_MAX)
}

// ── snippet 预算 ─────────────────────────────────────────────────────────

/// 一条命中要不要给 snippet —— **以及不给的话是为什么**。
///
/// 🔴 `BudgetExhausted` 与 `SessionCapped` 是**两件不同的事**，别再合成一个 bool：
/// 前者是「整份结果被砍了」（该让用户知道，缩小范围/加大 limit），
/// 后者是「这个会话话太多、只列前 30 条」（点进去看就行，结果没被砍）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnippetVerdict {
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
pub(crate) struct SnippetBudget {
    limit: usize,
    spent: usize,
    /// 是否**曾经**因为全局预算用完而拒绝过一条命中。
    starved: bool,
}

impl SnippetBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            spent: 0,
            starved: false,
        }
    }

    /// 判定一条命中。`shown_in_session` = 本会话**已经**给出的 snippet 条数。
    /// 返回 `Give` 时预算已记账，调用方必须真的构造 snippet。
    pub(crate) fn take(&mut self, shown_in_session: usize) -> SnippetVerdict {
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
    pub(crate) fn spent(&self) -> usize {
        self.spent
    }

    /// 🔴 **整份结果被全局预算砍过** —— 这才是该告诉用户的那件事。
    /// 单会话超 30 条（`SessionCapped`）**不算**。
    pub(crate) fn starved(&self) -> bool {
        self.starved
    }
}

// ── 预算顺序 ─────────────────────────────────────────────────────────────

/// snippet 预算按「最近优先」花 —— 两侧唯一的排序处。`key` 取会话的 `updatedAt`（epoch ms，= jsonl 的 mtime），原地降序。
///
/// 展示顺序已经定死了：合并那一步（前端 `history-search.ts` 的 `mergeSearchResults`）按 `updatedAt` 倒序，前端照这个顺序渲染
/// ⇒ 预算顺序 ≠ 展示顺序时，缺 snippet 的正好是列表最上面那几张卡。按走序花，展示序最上面 10 张里没有 snippet 的张数明显更多。
/// 买到的不是「少几张空卡」（最近的会话往往命中最多、一口气吃掉大半预算，空卡总数不一定少），是「空卡不在你眼前」。
/// 走序也不是一个可复现的顺序：`WalkDir` 走的是 `readdir` 返回序（ext4 上是目录哈希序），跨机器不同；最近序由内容决定。
pub(crate) fn sort_by_recency<T>(items: &mut [T], key: impl Fn(&T) -> i64) {
    items.sort_by(|a, b| key(b).cmp(&key(a)));
}

// ── snippet ──────────────────────────────────────────────────────────────

/// 在 `text` 里大小写不敏感地定位 `needle_lc`（已小写），返回前 / 中 / 后三段。
/// 找不到（理论上不会，调用前已 `contains` 粗筛过）则退化成开头窗口。
pub(crate) fn make_snippet(text: &str, needle_lc: &str) -> (String, String, String) {
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
pub(crate) fn find_ci(hay: &str, needle_lc: &str) -> Option<(usize, usize)> {
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
pub(crate) fn tail_chars(s: &str, n: usize) -> String {
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
pub(crate) fn head_chars(s: &str, n: usize) -> String {
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
pub(crate) fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 同 `collapse_ws` 但保留尾部 `…`。
pub(crate) fn collapse_ws_keep_ellipsis(s: &str) -> String {
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
pub(crate) fn truncate_plain(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n).collect()
}

/// 按字符截断（换行折成空格，加 `…`，用于标题 excerpt）。
pub(crate) fn truncate_excerpt(s: &str, n: usize) -> String {
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
pub(crate) fn session_title(
    ai_title: Option<&str>,
    first_user_excerpt: &str,
    session_id: &str,
) -> String {
    ai_title
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| (!first_user_excerpt.is_empty()).then(|| first_user_excerpt.to_string()))
        .unwrap_or_else(|| session_id.chars().take(8).collect())
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/search_rules_tests.rs"]
mod tests;
