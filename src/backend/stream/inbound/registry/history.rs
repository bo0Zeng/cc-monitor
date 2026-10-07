//! 命令表 · 历史会话与它的读面：`history-*` · `session-fork` · `resolve` · `tasks-list` · `resync`。

use crate::stream::inbound::spec::{CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // **历史注解**（星标 / 改名 / 隐藏）的读写者换成本机常驻后端 ——
    //   「文件留在原处、同一路径，不迁移、一条不丢」：文件住家里（`<家>/history-metadata.json`，后端按家推），
    //   写是第四层（`history_annotations.rs`，读不懂就拒写、只改那一条、认不出的键原样留着）。三条都是阻塞档（读写一份小文件）。
    CommandSpec {
        name: "history-annotate",
        doc_anchor: Some("#### `history-annotate`"),
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &[
            "customTitle",
            "entry",
            "hidden",
            "patch",
            "sid",
            "starred",
            "updatedAt",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::history::history_annotations::answer_annotate(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-forget",
        doc_anchor: Some("#### `history-forget`"),
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &["removed", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::history::history_annotations::answer_forget(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 「这台上每条会话上次用哪个号起的」（起会话账号记录，会话所在那台各问一次）。读不懂 ⇒ `unreadable`。
    CommandSpec {
        name: "history-last-accounts",
        doc_anchor: Some("#### `history-last-accounts`"),
        codes: &["unreadable"],
        fields: &["accounts"],
        takes_input: false,
        run: Run::Blocking(|_| {
            crate::control::launch_account::answer_last_accounts()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 只读查询面 ──────────
    // monitor 经已有的那条长连接问（不逐次拨号）。处理器住顶层 `read_face`：入方向不许出现 `observe::`
    // （`inbound_never_reaches_into_the_observe_layer`），查询本体住 `observe/`；`read_face` 只做换壳，每条都调 CLI 那一臂同一个函数，`out` 从 stdout 换成内存。
    // 名字刻意不与 CLI 那几条同名（`history-read` 而不是 `read-session`）：CLI 面从本表自动派生（`cli_control::cli_exposed`），同名就会抢走
    // `history_query::run` 的 `--read-session`；每条多出的 CLI 面（`--history-read` …）已进 `lib.rs::SUBCOMMANDS`。
    // 全在 `Run::Blocking`（文件 I/O，`history-search` 扫全库）；`cancel` 命中回 `not_cancellable`。
    // 历史页的平铺清单（`history_list.rs`）：跨项目一次出成品 —— 每行的状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名。
    // 远端那一支问那台的 CLI 面 `--history-list`（`raw`：那台自己判活、读上次的号），本进程记着、`fresh` 再问；并注解、筛、排都在这台。
    CommandSpec {
        name: "history-list",
        doc_anchor: Some("#### `history-list`"),
        codes: &["bad_args", "failed", "unreachable"],
        fields: &[
            "fresh",
            "groups",
            "hidden",
            "limit",
            "notice",
            "origin",
            "query",
            "raw",
            "rows",
            "sort",
            "total",
            "truncated",
            "within_days",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::history::history_list::answer(r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "history-search",
        doc_anchor: Some("#### `history-search`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &[
            "after_ms",
            "include_tools",
            "limit",
            "lines",
            "query",
            "scope",
            "skipped",
            "titles",
            "unreadable",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **各台 `history-search` 的会话行合成一份**：`updatedAt` 倒序（`search_rules::sort_by_recency`）·
    //   命中数相加 · 任一行被砍 ⇒ `truncated`。本体 `observe/search_query.rs::answer_merge`（经只读宿主 `read_face` 那一臂）；纯计算 ⇒ 不进阻塞档（同 `ping`）。
    CommandSpec {
        name: "history-search-merge",
        doc_anchor: Some("#### `history-search-merge`"),
        codes: &["bad_args"],
        fields: &["sessionCount", "sessions", "totalHits", "truncated"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::faces::read_face::answer(&r.cmd, &r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 一个子运行的记录（`read_face.rs` 那一臂 ＋ `history_query::run_source`）：按运行读，通用层不认任何一家的形状。同族同档、同一个只读宿主。
    CommandSpec {
        name: "history-run",
        doc_anchor: Some("#### `history-run`"),
        codes: &[
            "bad_args",
            "failed",
            "not_found",
            "path_refused",
            "refused",
            "too_large",
        ],
        fields: &[
            "end", "from", "more", "parent", "path", "rows", "run", "tool",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // resume 之前问「这条会话的记录还在不在」。同族同档（一次目录枚举 ⇒ 阻塞档）、
    // 同一个只读宿主。**只收 sid**（找文件那一步与分叉 / 删会话同一份 `agents::find_session_file`）。
    CommandSpec {
        name: "history-record",
        doc_anchor: Some("#### `history-record`"),
        codes: &["bad_args"],
        // +`configDir`（可选入参：这次 resume 要用的账号根）。
        fields: &["configDir", "present", "root", "sid"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 按**行号**取回（没接骨架的会话丢掉的正文从这里要回来）。同族同档、同一个只读宿主。
    CommandSpec {
        name: "history-lines",
        doc_anchor: Some("#### `history-lines`"),
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &["eof", "from", "lines", "next", "path", "until"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-read",
        doc_anchor: Some("#### `history-read`"),
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &["eof", "next", "offset", "path", "rows", "until"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 按字节分页读、出记录行（`read_face.rs` 那一臂 ＋ `observe/record_page.rs`）。同族同档、同一个只读宿主。
    CommandSpec {
        name: "history-page",
        doc_anchor: Some("#### `history-page`"),
        codes: &[
            "bad_args",
            "failed",
            "oversized_line",
            "refused",
            "too_large",
        ],
        fields: &[
            "eof", "lines", "next", "nextSeq", "offset", "path", "seq", "until", "whole",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 骨架索引 · 大纲清单 · 会话内查找：同族同档（同步文件 I/O ⇒ 阻塞档）、同一个只读宿主（`read_face::answer`）。
    // 会话事实（`read_face.rs` 那一臂 ＋ `observe/facts_query.rs`）：`prior` 是调用方上一次拿到的应答原样（续传令牌）；应答四格即成品。
    // 一轮的摘要（`read_face.rs` 那一臂 ＋ `observe/turns.rs`）：`from` 是某一轮的 `at`。
    CommandSpec {
        name: "history-turns",
        doc_anchor: Some("#### `history-turns`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["end", "from", "path", "turns"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-facts",
        doc_anchor: Some("#### `history-facts`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &[
            "agent",
            "end",
            "forkedFrom",
            "path",
            "prior",
            "touchedFiles",
            "usage",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-find",
        doc_anchor: Some("#### `history-find`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["hits", "include_tools", "limit", "path", "query", "total"], // 应答出成品：`lines` ⇒ `total` / `hits`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-index",
        doc_anchor: Some("#### `history-index`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["end", "from", "offset", "path", "rows", "until"], // 应答出成品：`lines` ⇒ `from` / `end` / `rows`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-user-inputs",
        doc_anchor: Some("#### `history-user-inputs`"),
        codes: &["bad_args", "failed", "too_large"],
        fields: &["end", "entries", "from", "path"], // 应答出成品：`lines` ⇒ `from` / `end` / `entries`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-tail",
        doc_anchor: Some("#### `history-tail`"),
        codes: &["bad_args", "failed"],
        fields: &["end", "n", "path", "split_at", "tail_from", "total"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 分叉：与 CLI `--fork-session` 同一个本体（`control/fork_write.rs::run_inner`，
    //   读 → 适配层的分叉变换（`agents::build_branch_records`）→ `O_EXCL` 新建）。本机远端同一条长连接；读整份 jsonl ⇒ 阻塞档。
    //   回复多一格 `launch`：分叉之后起要的三格事实（`control/fork_launch.rs`，宿主 `faces/fork_face.rs` 收齐）。
    //   ⚠ 名字刻意不是 `fork-session`：自动派生的 CLI 面会与对 aterm 冻结的 `--fork-session`（argv 形）撞名。
    CommandSpec {
        name: "session-fork",
        doc_anchor: Some("#### `session-fork`"),
        codes: &["bad_args", "fork_failed"],
        fields: &[
            "account",
            "cwd",
            "from",
            "host",
            "jsonlPath",
            "kind",
            "launch",
            "sessionId",
            "sid",
            "terminal",
            "uuid",
            "value",
            "why",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::fork_face::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 手动对齐：整机（或 `sid` 只对一个会话）重跑起步那套对齐，回差异。阻塞档：等每份 watcher 做完。
    CommandSpec {
        name: "resync",
        doc_anchor: Some("#### `resync`"),
        codes: &["bad_args"],
        fields: &[
            "added",
            "caught_up",
            "removed",
            "retagged",
            "sid",
            "uncancellable",
            "unavailable",
            "watchers",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::resync_face::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 会话的任务列表（`parity_ledger` `session.tasks`）──
    // 本机远端同一个二进制，monitor 按 origin 问（本机也走这里）。宿主是 `feature_face`，不是 `read_face`（理由在 `feature_face` 头注）。
    // 阻塞档：读一个目录 ＋ 每个任务文件各一次。`cancel` 命中回 `not_cancellable`。
    CommandSpec {
        name: "tasks-list",
        doc_anchor: Some("#### `tasks-list`"),
        codes: &["bad_args", "failed", "too_large"],
        // 应答换成成品 `{tasks: [...]}`（原是原样对象的 `lines`）⇒ 后端行为变更，合并那拍 bump。
        fields: &[
            "activeForm",
            "blockedBy",
            "blocks",
            "description",
            "id",
            "sid",
            "status",
            "subject",
            "tasks",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // `resolve`：与一次性 `--resolve` 复用同一个纯函数，那条路与仓外 aterm 的契约冻结着。命令级错误码仍叫 `bad_request`（与协议级同名）：
    // 改它会破坏那份冻结的契约。
    CommandSpec {
        name: "resolve",
        doc_anchor: Some("#### `resolve`"),
        // 与 `resolve_from_json` 真回的码全集两向相等（`resolve_query_tests.rs` 那一族钉着；`stdin_read_failed` 只有一次性那条会出，不在这里）。
        codes: &[
            "bad_request",
            "invalid_session_id",
            "unsafe_launch_candidate",
            "serialize_failed",
        ],
        fields: &[],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let input = serde_json::to_string(&r.args)
                    .map_err(|e| ("bad_request".to_string(), e.to_string()))?;
                crate::control::resolve_query::resolve_json_for_inbound(&input)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
];
