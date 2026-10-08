//! CLI 独有子命令的说明：协议参考 `IPC-COMMANDS.md` 的 CLI 那一节由它生成（文档料，不是对外文案）。

/// CLI 独有的子命令（`SUBCOMMANDS` 里不从 `inbound::REGISTRY` 派生的那几条）：`(子命令名（不带打头的 `--`）, 用法, 一句话)`。
/// 协议参考 `IPC-COMMANDS.md` 的 CLI 那一节由它生成；漏一条、多一条生成器都当场失败。
pub const CLI_ONLY_DOCS: &[(&str, &str, &str)] = &[
    ("account-trust", "<configDir> <cwd>", "换号恢复前的信任预检：单行 `{trusted, known, error}`；`configDir` 必须逐字是账号清单里的一个，否则退出 2、`unknown_config_dir`；不回那份配置的内容"),
    ("account-trust-zero", "<cwd>", "账号 0（没启用多账号时那个原生身份）的信任预检，形状同 `--account-trust`；`cwd` 只当查表键，不收路径参数"),
    ("backend-probe", "", "能力探测：回 `{proto, buildId, commands}`，`commands` 是这台真能派发的 CLI 控制面子命令；不读 stdin"),
    ("find-in-session", "[--include-tools] [--limit <n>] --query <q> <jsonl>", "在一份会话里找一段文字：头 `{kind:\"session_find\",v:1}` · 每条命中 `{uuid, kind, before, matched, after, turn, tsMs, tsText}`（`tsText` ＝ 那条的时刻按这台本地钟写好） · 尾 `{kind:\"session_find_end\",count,total}`；`limit` 缺省 500、封顶 2000"),
    ("fork-session", "<args>", "从某条消息处分叉出一个新会话文件，出参 `ForkResult`（见下）"),
    ("list-accounts", "", "账号清单：首行 `{kind:\"accounts-meta\", enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault}`，其后每号一行 `{name, email, configDir, isDefault, mode, exists, loggedIn}`；没启用多账号 ⇒ `enabled:false`、退出 0"),
    ("list-projects", "", "项目清单：每行 `{dirName, projectPath, sessionCount, lastActivityMs}`；工作目录在 `~/.cc-monitor/autostart/` 下的会话不出"),
    ("list-sessions", "<project_dir>", "一个项目的会话：每行 `{sessionId, jsonlPath, startedAtMs, updatedAtMs, messageCountApprox, firstUserExcerpt, aiTitle, cwd}`"),
    ("list-user-inputs", "[--from <offset>] <jsonl>", "「你说过的话」：头 `{kind:\"user_inputs\",v:1,from}` · 每条 `{uuid, timestamp, excerpt}`（对话序）· 尾 `{kind:\"user_inputs_end\",count,end}`；`end` 是下次增量的 `--from`；`offset` 过了文件尾 ⇒ 退出 2"),
    ("read-session", "<jsonl>", "原样透传整份会话字节"),
    ("read-session-from-offset", "[--index] [--until <end>] <jsonl> <offset>", "从字节 `offset` 续读：原样透传 `[offset, EOF)`（`--until` ⇒ `[offset, end)`）；`--index` ⇒ 出骨架索引：头 `{kind:\"session_index\",v:1,from}` · 每个可计行一条 `IndexRow`（见下）· 尾 `{kind:\"session_index_end\",count,end}`。续点用 `line` 帧的 `byte_offset`，别用 `seq`"),
    ("read-session-tail", "<jsonl> <N>", "尾部优先：首行 `{kind:\"snapshot_meta\",total,tail_from}`，随后原样输出最新 N 行 `[tail_from,total)`，再输出 `[0,tail_from)`"),
    ("resident-ensure", "[--replace]", "确保这台的常驻后端在听：已在 ⇒ `{port, token, pid:null}`；没在 ⇒ 起一个脱离的自己、回 `{port, token:null, pid}`（钥匙由它绑上口后写进钥匙文件）；`--replace` 先停掉口上那一位再起"),
    ("resident-stop", "[--grace <秒>]", "停这台的常驻后端：核身份 → SIGTERM → 宽限（缺省 35 秒）→ SIGKILL；回 `{stopped: graceful|killed|not_running, pid}`"),
    ("search", "<query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]", "全库全文搜索：每命中会话一行 `SessionHits`（camelCase，含 `hitsTruncated`），行序 = 最近优先"),
    ("session-accounts", "", "正在跑的会话各属哪个号：每条 `{pid, sessionId, cwd, configDir, account, bare, alive, viaRelay}`；`account:null` ＝ 查不到（不猜）"),
    ("tmux-notify", "<backend_pid> <backend_starttime>", "tmux 钩子用：核对身份后叫正在跑的后端立刻重扫 tmux；身份对不上静默退出 0；不碰文件系统"),
];
