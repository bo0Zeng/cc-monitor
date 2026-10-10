//! 命令表 · 历史会话与它的读面：`history-*` · `session-fork` · `resolve` · `tasks-list` · `resync`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // **历史注解**（星标 / 改名 / 隐藏）的读写者换成本机常驻后端 ——
    //   「文件留在原处、同一路径，不迁移、一条不丢」：文件住家里（`<家>/history-metadata.json`，后端按家推），
    //   写是第四层（`history_annotations.rs`，读不懂就拒写、只改那一条、认不出的键原样留着）。三条都是阻塞档（读写一份小文件）。
    CommandSpec {
        name: "history-annotate",
        summary: "改一条历史注解",
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &[out("customTitle", "自定义标题（`null` = 没改过名）"), out("entry", "改完的那一条：`starred` · `customTitle`（`null` = 没改过名）· `hidden` · `updatedAt`（毫秒，= 这一次）"), out("hidden", "隐藏"), arg("patch", "要改的那几格：`starred` / `customTitle` / `hidden`（`customTitle` 也认蛇形 `custom_title`）"), arg("sid", "会话 id"), both("starred", "星标"), out("updatedAt", "毫秒，= 这一次")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::history::history_annotations::answer_annotate(&r.args)
                .map(Some)
        }),
    },
    CommandSpec {
        name: "history-forget",
        summary: "删一条历史注解",
        codes: &[
            "annotations_unreadable",
            "bad_args",
            "io_failed",
            "no_annotations",
        ],
        fields: &[out("removed", "真删了一条没有（`false` = 本来就没有这一条，文件没动）"), arg("sid", "会话 id")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::history::history_annotations::answer_forget(&r.args)
                .map(Some)
        }),
    },
    // 「这台上每条会话上次用哪个号起的」（起会话账号记录，会话所在那台各问一次）。读不懂 ⇒ `unreadable`。
    CommandSpec {
        name: "history-last-accounts",
        summary: "sid → 上次用哪个号起",
        codes: &["unreadable"],
        fields: &[out("accounts", "`{sid: 账号名}`")],
        takes_input: false,
        run: Run::BlockingData(|_| {
            crate::control::launch_account::answer_last_accounts()
                .map(Some)
        }),
    },
    // ── 只读查询面上线 —— 层 1 ＋ ──────────
    //
    // 🔴 **这八条此前全是一次性子命令**：monitor 每问一次就新拨一条 SSH（握手 ＋ 鉴权 ＋ exec），
    //   而这条长连接明明已经在那儿。账号那两条还被一个 10 秒的轮询按台数翻倍。
    //   ⇒ 登记上来，monitor 改走已有的 `inbound_client`，那些逐次拨号与轮询一起删。
    //
    // 🔴 **处理器住顶层 `read_face`，与 `files/` 同一个理由**：入方向不许出现 `observe::`
    //   （`inbound_never_reaches_into_the_observe_layer`），而查询本体住 `observe/`。
    //   `read_face` 只做换壳 —— 每条都调 CLI 那一臂同一个函数，`out` 从 stdout 换成内存。
    //
    // ⚠ **名字刻意不与 CLI 那几条同名**（`history-read` 而不是 `read-session`）：CLI 面从本表自动派生
    //   （`cli_control::cli_exposed`），同名就会把 `--read-session` 从 `history_query::run` 手里抢走、改印一行 JSON。
    //   ⇒ 每条多出一个 CLI 面（`--history-read` …），已进 `lib.rs::SUBCOMMANDS`（不进就静默进流模式）。
    //
    // ⚠ 全在 `Run::Blocking`：它们都做文件 I/O（`history-search` 扫全库）。代价同 `files-*`：
    //   `cancel` 命中时回 `not_cancellable`（不撒谎）。
    // **历史页的平铺清单**（`history_list.rs`）：跨项目一次出成品 —— 每行的状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名。
    //   远端那一支问那台的 CLI 面 `--history-list`（`raw`：那台自己判活、读上次的号），本进程记着、`fresh` 再问；并注解、筛、排都在这台。
    CommandSpec {
        name: "history-list",
        summary: "历史页的平铺会话清单",
        codes: &["bad_args", "failed", "unreachable"],
        fields: &[arg("fresh", "可缺席：`true` ⇒ 远端那一台不用记着的、再问一次（开页 · 「刷新」）"), out("groups", "按项目看时的分组（只数 `rows` 里不是 `context` 的）：`key` · `agent` · `projectName` · `projectPath` · `projectDir` · `count` · `hasLive`（`null` = 有判不了活的、又没有确定在跑的）· `starred`（组里有星标的）· `lastActivity` · `order`（几台的组并成一列时的序，大的在前：档位 × 10¹⁴ ＋ 有星标 × 10¹³ ＋ 最后动过的毫秒；界面只按它并）· `failed`（读不了的那个记录目录 ⇒ 一组、`count` 0、带那一句；别的 ⇒ `null`）· `origin`"), arg("hidden", "可缺席：`true` ⇒ 隐藏的也出（默认不出）"), arg("limit", "可缺席：最多回几行（默认 2000，1–20000）；多出的不回、`truncated`"), out("notice", "注解没并上的那句话；`null` = 并上了"), arg("origin", "可缺席：那台的名字（可达表的键）"), arg("query", "可缺席：只留显示标题（`label`）· 第一句 · 项目名里含这几个字的（不分大小写，子串；不比路径、不搜内容 —— 内容走 `history-search`）"), arg("raw", "可缺席：`true` ⇒ 只回**这台自己**的清单 `{rows, failed}`（不并注解、不筛不排、不认别的入参）—— 远端那一支问的就是它"), out("rows", "每会话一行，按 `at` 倒序：`agent` · `agentTag`（行上那一家的小牌，对用户的叫法）· `atText`（行尾那一格）· `sectionText`（分段头）· `spanText`（内容头那一段）—— 这三格按这台本地钟写好，界面照抄"), arg("sort", "可缺席：`activity`（默认，按最后活动）· `created`（按开始）"), out("total", "筛完留下几个（截之前，不含 `context`）"), out("truncated", "`rows` 被 `limit` 截过"), arg("within_days", "可缺席：只留那个键（同 `sort`）落在最近 N 天里的（1–3650）")],
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
        summary: "全文搜索",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[arg("after_ms", "可选，= `--after-ms`"), arg("include_tools", "可选布尔，= `--include-tools`"), arg("limit", "可选，= `--limit`"), out("lines", "每命中会话一行 `SessionHits`（带 `agent`：只扫记录树 ⇒ 记录树那一家），形状与行序同 `--search`"), arg("query", "搜索词（必填）"), arg("scope", "可选，`user`（人说的）/ `assistant`（那一家说的）/ `report`（agent 回报：子 agent 交回 / 发来的话 · 另一个会话发来的话），= `--scope`"), out("skipped", "内容搜索不覆盖、这台上又有它的会话记录的那几家（对用户的叫法）：它们的会话不在结果里"), arg("titles", "可选布尔：只比会话标题与第一句（不搜内容）；命中的会话照样一行，`hitCount` 为 `0`、`hits` 空"), out("unreadable", "这一趟有几份会话记录读不动、没搜到（权限 / IO 错 / 不是合法 UTF-8）")],
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
        summary: "把各台的搜索结果合成一份",
        codes: &["bad_args"],
        fields: &[out("sessionCount", "会话数"), both("sessions", "各台的会话行（远端的带 `origin`）；回的是合好的、按 `updatedAt` 倒序，每行添 `atText`（行尾那一格）· `spanText`（内容头那一段）：按 `updatedAt`、这台本地钟写好"), out("totalHits", "`hitCount` 之和"), out("truncated", "任一行 `hitsTruncated`")],
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
        summary: "一个子运行的记录",
        codes: &[
            "bad_args",
            "failed",
            "not_found",
            "path_refused",
            "refused",
            "too_large",
        ],
        fields: &[out("end", "读到哪了（最后一个整行之后）"), arg("from", "从这个字节起读（缺 ＝ 0）；续读拿上一次的 `end`"), out("more", "这一页没读到头（再从 `end` 读）"), arg("parent", "父会话的记录路径（读会话那道围栏照旧；越界 ⇒ `path_refused`）"), out("path", "那份子运行记录（不透明，给查看器整份打开用）"), out("rows", "这一页里每一条认得出的记录：`message` 在渲染模型里的样子（与主会话同一套记录成品）· `rid` 它的对账键（没有 ⇒ 省略）"), both("run", "子运行标识（运行表 `session_runs` 里那一格）；与 `tool` 至少给一个，都给以它为准"), arg("tool", "派出它的那次工具调用的 id：后端在父记录里找那次调用的派出链接（适配层 `child_link`）；还没对上（前台子运行跑完才写明是哪一个）⇒ `not_found`")],
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
        summary: "这条会话的记录还在不在",
        codes: &["bad_args"],
        // +`configDir`（可选入参：这次 resume 要用的账号根）。
        fields: &[arg("configDir", "可选：这次 resume 要用的**账号配置目录**"), out("present", "这条会话的记录文件在那棵记录树里找得到（根那一层或项目目录那一层"), out("root", "查的那棵记录树的根（报错时说清查了什么）"), arg("sid", "会话 id")],
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
        summary: "按行号取回一段",
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &[out("eof", "读到了最后一个完整行之后"), both("from", "第一行的行号（缺省 0）"), out("lines", "**记录行**（形状同 `history-page` 的 `lines`）：`[from, next)` 里进界面的那些，第 k 个可计行的行号是 `from + k`（不进界面的照占号、不出现）"), out("next", "下一段从这一行起（恒 ＝ `from` ＋ 这一段的可计行数）"), arg("path", "jsonl 路径，围栏同 `history-read`（越界 ⇒ `refused`）"), arg("summaryOnly", "只要**折起那一行的成品**：每条的 `message` 剥掉正文那几格（`message.content` —— 正文 · 思考 · 工具入参 · 工具结果；`cc-monitor-unrecognized` 的 `raw`；`queue-operation` 的 `content`），折起那一行要用的那几格照给（`timeText` · `userText` · `toolSteps` · `toolCards` · `toolResults` · 链上身份）。缺省 `false` ＝ 给全文（今天的行为）"), arg("until", "可选右端（半开区间 `[from, until)`）；缺 ＝ 到最后一个完整行为止")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-read",
        summary: "按字节分页读一份会话",
        codes: &["bad_args", "failed", "oversized_line", "refused"],
        fields: &[out("eof", "区间到头了（`until` 或读时的文件长度）"), out("next", "下一页从这里起（= `offset` ＋ 这一页的原始字节数）"), arg("offset", "从这个字节起（缺省 0）"), arg("path", "jsonl 路径，围栏同 `--read-session`（越界 ⇒ `refused`）"), out("rows", "这一页里每个**可计行**一条（空白 / 纯 BOM 行不占）：`end` ＝ 这一行（含 `\\n`）之后那个字节的偏移（原始字节，永远说得准"), arg("summaryOnly", "只要**折起那一行的成品**：每条的 `message` 剥掉正文那几格（`message.content` —— 正文 · 思考 · 工具入参 · 工具结果；`cc-monitor-unrecognized` 的 `raw`；`queue-operation` 的 `content`），折起那一行要用的那几格照给（`timeText` · `userText` · `toolSteps` · `toolCards` · `toolResults` · 链上身份）。缺省 `false` ＝ 给全文（今天的行为）"), arg("until", "可选右端（半开区间 `[offset, until)`），= `--until`")],
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
        summary: "按字节分页读，出记录行",
        codes: &[
            "bad_args",
            "failed",
            "oversized_line",
            "refused",
            "too_large",
        ],
        fields: &[out("eof", "同 `history-read`"), out("lines", "只装**进界面**的记录行：`session_id` · `path` · `seq`（第 k 个可计行 ＝ `seq + k`）· `cwd` · `message`（`JsonlRecord`）"), out("next", "同 `history-read`"), out("nextSeq", "下一页第一行的行号"), arg("offset", "同 `history-read`"), arg("path", "同 `history-read`"), arg("seq", "`offset` 那一行的行号（缺省 0）；续页交上一页的 `nextSeq`"), arg("summaryOnly", "同 `history-read`"), arg("until", "同 `history-read`"), arg("whole", "这是「整份读进查看器」那一件：读过 256 MiB 就明拒 `too_large`（那句话说读到了哪；不许静默截断，F06）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 骨架索引与大纲清单上帧面（此前它们在远端走逐次拨号 —— `STILL_DIALED` 那两行）。
    // 同族同档（同步文件 I/O ⇒ 阻塞档）、同一个只读宿主（`read_face::answer`）。
    // 会话内查找上帧面（此前走逐次拨号 —— `STILL_DIALED` 那一行）。同族同档。
    // 会话事实（`read_face.rs` 那一臂 ＋ `observe/facts_query.rs`）。同族同档、同一个只读宿主。
    //   `prior` 是调用方上一次拿到的应答原样（续传令牌）；应答四格即成品。
    // 一轮的摘要（`read_face.rs` 那一臂 ＋ `observe/turns.rs`）。同族同档、同一个只读宿主；`from` 是某一轮的 `at`。
    CommandSpec {
        name: "history-turns",
        summary: "一轮的摘要",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("end", "最后一个完整行的末字节（残尾不计）"), both("from", "可选，缺 ⇒ 0：从这个字节起扫"), arg("path", "jsonl 路径（围栏同 `history-read`）"), out("turns", "这一段里的每一轮，文件序；起止（`start` · `end`）旁边各有一格 `startText` · `endText`：这台本地钟的 `HH:MM`（界面照抄、不换算；解不出 ⇒ 空串）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "sessions-needs",
        summary: "这台上需手动的会话清单",
        codes: &[],
        fields: &[out("needs", "它在等什么：与 `history-facts` 对同一份记录答的 `needs` 是同一份（种类 · 字 · 语气 · 起点都照那一处）；记录找不到 ⇒ 不挂哪一步（`tool` · `call` · `what` 为 `null`），种类照那台说的框"), out("sid", "会话 id"), out("waiting", "此刻活着、那台说在等人的会话，每项 `{sid, needs}`；一个都没有 ⇒ 空数组")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-facts",
        summary: "会话事实",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("agent", "这份记录是哪一家的（线上的 kind，适配层按记录认）；认不出 ⇒ `null`"), out("cost", "全会话花费（记录里那一家自己记的那一条，最后一条为准）`{micros, partial, text}`（`text` 写好）；记录里没有 ⇒ `null`（不按定价自己算）"), out("end", "最后一个完整行的末字节"), out("forkedFrom", "源会话 sid：首条带 `forkedFrom`（`sessionId` 与 `messageUuid` 都是串）的 user / assistant 记录；不是分叉来的 ⇒ `null`"), out("handedBack", "交回了的子运行 id（按「谁说的」认），去重、文件序"), out("lastSay", "最后一段正文的头一行 `{text, at}`；没有 ⇒ `null`"), arg("limits", "可选：设置里的上下文上限表 `{<模型名子串>: 正整数}`（最长匹配的子串胜）；缺 / `null` ⇒ 空表；形状不对 ⇒ `bad_args`"), out("needs", "那台说在等人 ⇒ `{kind, tool, call, what, sinceMs}`（`kind`：approve 批准 · answer 回答 · plan 批准计划 · network 放行联网 · worker 批准协作请求 · goal 确认会话目标 · choose 在对话框里选 · unknown 判不出）；不在等 ⇒ `null`"), arg("path", "jsonl 路径（围栏同 `history-read`）"), out("pending", "还没结果的工具调用 `{id, name, what, at, state, why}`：`state` 在跑 running · 在等你 awaiting · 状态不明 unclear（每次现判）；`why` 只在 unclear 时给：noWriter（没有活进程持着这条会话）· untracked（这一家不留 pidfile，判不了活）"), out("permissionMode", "此刻的许可档（最后一条许可档记录写的那一档，原样）；没有 ⇒ `null`"), arg("prior", "可选：**上一次应答的 `data` 原样**（续传令牌）"), out("projectDir", "会话起在哪个目录（记录开头）；还没读到 ⇒ `null`"), out("retries", "一串相邻的 API 重试按首条的 `uuid` 记一件 `{id, outcome}`：retrying（还没下文）· recovered（后面来了正常回复）· failed（来了报错那条）· interrupted（人发了一句 / 打断）；文件序，至多 200 件"), out("tokens", "全会话用量（按请求去重）`{input, output, cacheRead, cacheWrite5m, cacheWrite1h, requests, text, last}`（写缓存分 5 分钟 / 1 小时两档；`text` 写好）；一条带用量的回复都没有 ⇒ `null`"), out("touchedFiles", "写类工具（Edit / Write / MultiEdit → `file_path`，NotebookEdit → `notebook_path`）碰过的文件，原样、去重、近因序（最近碰的在末尾），至多 1000 条（超 ⇒ 丢最久没碰的）"), out("usage", "文件序最后一条 `input_tokens + cache_creation_input_tokens + cache_read_input_tokens > 0` 的 assistant 记录 ⇒ `{promptTokens, model, peakPromptTokens, limit, limitFrom}`（`model` 缺 ⇒ `null`）；一条都没有 ⇒ `null`"), out("writers", "此刻持着这条会话的活进程 pid（这台的 pidfile），升序；不留 pidfile 的那一家恒空")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-branch",
        summary: "主线外清单",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("end", "最后一个完整行的末字节（之后的由实时帧 `session_branch` 接着说）"), out("off", "回退掉的那几条记录的 `id`（只含进界面的；文件序）；这一家的记录没有链 ⇒ 恒空"), arg("path", "jsonl 路径（围栏同 `history-read`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-find",
        summary: "会话内查找",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("hits", "命中，每条 `{uuid, kind, before, matched, after, turn, tsMs, tsText}`（与 `--find-in-session` 的中段逐行相同）；`tsText` ＝ 那条的时刻按这台本地钟写好（今天 `HH:MM` · 昨天 · 更早带日期；读不出 ⇒ 空串）"), arg("include_tools", "可选，缺省 `false`：工具结果也搜"), arg("limit", "可选，缺省 500、封顶 2000（与 CLI 的 `--limit` 同一对常量）"), arg("path", "jsonl 路径（围栏同 `history-read`）"), arg("query", "查询串（原样；以 `--` 起头也照样是查询，不是选项）"), out("total", "全量命中数（≥ 条数；大于 ⇒ 被上限砍过）")], // 应答出成品：`lines` ⇒ `total` / `hits`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-index",
        summary: "会话骨架索引",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("end", "最后一个完整行的末字节 ＝ 下一次续传该带的 `offset`"), out("from", "起点字节"), arg("offset", "从哪个字节起（缺省 0；续传带上次尾行的 `end`）"), arg("path", "jsonl 路径（围栏同 `history-read`）"), out("rows", "每个可计行一条 `IndexRow`（见第 6 节）"), arg("until", "可选：只收起点 `< until` 的行")], // 应答出成品：`lines` ⇒ `from` / `end` / `rows`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-user-inputs",
        summary: "「你说过的话」清单",
        codes: &["bad_args", "failed", "too_large"],
        fields: &[out("end", "最后一个完整行的末字节 ＝ 下一次增量该带的 `from`"), out("entries", "每条用户输入 `{uuid, timestamp, excerpt}`（对话序；与 `--list-user-inputs` 的中段逐行相同）"), both("from", "增量起点（缺省 0；传上次尾行的 `end`）"), arg("path", "jsonl 路径（围栏同 `history-read`）")], // 应答出成品：`lines` ⇒ `from` / `end` / `entries`
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "history-tail",
        summary: "一份会话的尾段从哪个字节起",
        codes: &["bad_args", "failed"],
        fields: &[out("end", "最后一个完整行之后的字节位置"), arg("n", "要最新几行"), arg("path", "jsonl 路径（路径围栏同 `history-read`）"), out("split_at", "尾段第一行的字节起点"), out("tail_from", "尾段第一行的行号"), out("total", "可计行总数（口径同 `--read-session-tail` 的 `snapshot_meta`）")],
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
        summary: "从某条消息处分叉出一个新会话",
        codes: &["bad_args", "fork_failed"],
        fields: &[out("account", "`process`（源会话进程此刻的配置目录，同 `accounts-sessions`：没设 ＝ 账号 0，设了 ＝ 账号库里认得那个目录的号）|`exited`（源会话已退出）· `live_no_account`（活着，但说不出号：进程名单里没有它 · 那个目录账号库不认得 · 环境这一刻读不出）"), out("cwd", "`record`|`no_cwd`（记录里没有）"), out("from", "`known` 时值的来源"), out("host", "`terminal` 那一格的终端宿主"), out("jsonlPath", "新会话的 id 与落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖）"), out("kind", "`launch` 三格每格的状态：`known` · `unknown`"), out("launch", "起分叉出来的新会话要的三格：`cwd` · `account` · `terminal`，每格 `{kind:\"known\", value, from}` 或 `{kind:\"unknown\", why}`"), out("sessionId", "新会话的 id 与落点（源文件同目录，`O_EXCL` 新建：撞了就失败，绝不覆盖）"), arg("sid", "源会话 id（只收 sid、不收路径：按 sid 在记录树里找那份文件，`branch_core::find_session_file`）"), out("terminal", "`terminal_list`（挂着它的 `@ccm_sid`、前台是 agent 的那一个；命中多个取第一个）|`exited`"), arg("uuid", "从哪条消息处分叉"), out("value", "`known` 时的值"), out("why", "`unknown` 时为什么说不出")],
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
        summary: "手动对齐：重发这条流上的会话状态",
        codes: &["bad_args"],
        fields: &[out("added", "这次补宣告"), out("caught_up", "在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加）"), out("removed", "补移除的会话数（各份 watcher 取最大：看的是同一台机器）"), out("retagged", "这次真写了几处 `@ccm_sid`（值一样的不写）"), arg("sid", "可选"), out("uncancellable", "这台**当下**的能力事实，与 hello 那两格同一个函数、同形（`[{command, code}]` · `[op]`，后者按字母排、当集合用）"), out("unavailable", "这台**当下**的能力事实，与 hello 那两格同一个函数、同形（`[{command, code}]` · `[op]`，后者按字母排、当集合用）"), out("watchers", "几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::resync_face::answer(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ── 功能侧只读查询 —— 远端会话的任务列表（`parity_ledger` `session.tasks`）──
    //
    // 🔴 此前只有 monitor 直读**本机** `tasks/<sid>/` 那一条路，远端 tab 永远拿不到任务。
    //   本机后端与远端后端是同一个二进制 ⇒ 读法搬到这里，monitor 按 origin 问（本机也走这里）。
    // ⚠ 宿主是 `feature_face`，**不是** `read_face`：monitor 侧有一条两向判据数的正是
    //   「交给 `read_face::answer` 的 == `C1` 那八条」，本族不在其中（理由全文在 `feature_face` 头注）。
    // ⚠ 阻塞档：读一个目录 ＋ 每个任务文件各一次。`cancel` 命中回 `not_cancellable`（不撒谎）。
    CommandSpec {
        name: "tasks-list",
        summary: "一个会话的任务列表",
        codes: &["bad_args", "failed", "too_large"],
        // 应答换成成品 `{tasks: [...]}`（原是原样对象的 `lines`）⇒ 后端行为变更，合并那拍 bump。
        fields: &[out("activeForm", "可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串"), out("blockedBy", "串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务）"), out("blocks", "串的数组；原文缺 ⇒ `[]`（原文是 `null` / 别的类型 ⇒ 那个对象不算任务）"), out("description", "可缺（原文缺或 `null` ⇒ 这一格不出现），出现就是串"), out("id", "每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）"), arg("sid", "会话 id"), out("status", "每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）"), out("subject", "每格必有、是串（缺 / 不是串的那个对象不算任务，跳过）"), out("tasks", "**成品**：`<tasks>/<sid>/<数字>.json` 里每个任务一格，按那个数字升序（此前是原样对象的 `lines`，字段由 monitor 解）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::feature_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // U6b-3：第一条**真业务命令**。
    // 一次性 `--resolve` 那条路**逐字不动** —— 契约与仓外 aterm 冻结在 2026-07-18，
    // 两条路复用同一个纯函数。⚠ 它的命令级错误码今天仍叫 `bad_request`（与协议级同名），
    // **刻意不改**：改它会破坏那份冻结的契约。如实登记。
    CommandSpec {
        name: "resolve",
        summary: "按 `ResumeSpec` 推出恢复命令 `CommandPlan`（与一次性 `--resolve` 同一个函数）",
        // 原来只列两个，而 `resolve_from_json` 还会回 `invalid_session_id` /
        // `unsafe_launch_candidate`（B2 两道校验）⇒ 登记表比真回的少两个。补齐；
        // 与跨仓承诺的码全集两向相等由 `resolve_query_tests.rs` 那一族钉着
        //（`stdin_read_failed` 只有一次性那条会出，不在这里）。
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
