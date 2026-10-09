//! 命令表 · 文件管理：`files-*`（读 · 写 · 上传的提交 · 解压 · 删历史会话）。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Fail, Run};

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由本门递给文件管理写面 —— 写面自己一家 agent 的布局都不认（`files/module_boundary_guard.rs` 围栏那一类为零）。
pub(in crate::stream::inbound) const SESSION_PORT: crate::control::files_write::SessionPort =
    crate::control::files_write::SessionPort {
        locate: crate::agents::locate_session_for_delete,
        is_record: crate::agents::is_session_record,
        file_name: crate::agents::session_file_name_of,
    };

pub(super) const SPECS: &[CommandSpec] = &[
    // ── 〔步 `24f` 第二刀 09-20〕`files-read` 这一族上线 ────────────────────────────
    //
    // 🔴 **处理器不住 `control/`，这是本仓第一条** —— 而且它必须不住那里：
    //   `control/` 的定义是「**会改变世界**」（`§1.1` 第二条线），而这一族整族纯读
    //   （边界①，`readonly_guard` 4259 行一行没动）；
    //   而读面 `observe/` 又被入方向头注那条硬约束挡着（`inbound` 不许出现 `observe::`，
    //   `inbound_structure_guards::inbound_never_reaches_into_the_observe_layer` 在钉）。
    //   ⇒ 它住顶层 `files/`（`lib.rs` 一条 `pub mod`，`readonly_guard::BACKEND_CORE_MODULES`
    //   里也是一条独立登记）。入方向头注那句「每条命令的处理器属于 `control/`」
    //   由此收窄成「**不在入方向里实现**」——那才是它真正保的东西。
    //
    // 🔴 **四条的 `run` 长得一模一样，那是刻意的**：命令名从 `r.cmd` 来，
    //   而 `r.cmd` 正是 `lookup()` 用来选中这条 spec 的那个串
    //   （CLI 面同理，`cli_control` 拿 `spec.name` 填它）⇒ 「登记的名字」与
    //   「真正被调的能力」**在类型上是同一个值**，抄错一条能力名这件事不可表示。
    //   翻译（`-` → `.`）只有 `files::answer_wire` 一处，理由整段在那个函数的头注。
    //
    // ⚠ 四条全在 `Run::Blocking`：`files::answer` 是**同步**函数，前两条真的做文件系统
    //   I/O，`files-find` 在 64 万条量纲上的现打外推是 20–50 ms——
    //   那是不该占住 worker 的时长；而把「哪条够快可以走 `Run::Async`」拆成两档，
    //   等于给同一个同步入口记两份账。⇒ 一族一档。
    //   代价如实写：它们因此**取消不掉**，`cancel` 命中时回 `not_cancellable`（不撒谎）。
    // ── 〔步 `24f` 第三刀 09-21〕裁出来的第五、第六条 ──────────────
    //
    // 🔴 **它们补的是那两段「机制」的线上面** —— 那张三段表
    //   （建索引 / 保鲜 / 查询）里，第二刀只把「查」那一段接上了线。
    //   在这两条之前，`files::index::rebuild_once` 与 `files::browse_watch::set_browsing`
    //   **零生产调用方** ⇒ 真机上 `files-find` 恒回 `index_missing: true`。
    //
    // 🔴 **节拍仍然不归后端**，一个字没松：这两条与抓屏那条原语**同一形** ——
    //   「**只做一次**……『隔多久再做一次』留在调用方」（`K37`：后端只给机制，不给偏好），
    //   `no_timer_guard` 在后端侧零容忍地钉着。
    //   ⚠ **别把这一刀读成「那个缺口填上了」**：调用方不发这条命令，
    //   索引照旧永远不会自己变新，而「调用方到底发不发」后端这棵树的判据钉不住。
    //
    // ⚠ 两条的 `run` 与同族那四条逐字同形，理由同上一段（名字从 `r.cmd` 来 ⇒
    //   「登记的名字」与「真被调的能力」在类型上是同一个值）。
    // ── 〔波 5 ㈠〕 **第 2 步**：把那份零消费者的写原语接上 ──
    //
    // 🔴 **这一条是本族第一条会往盘上写的命令，而它不花用户那句「允许」**：
    //   `control/files_write.rs` 自 09-19 起就在 `readonly_guard` 的写白名单上，
    //   射程逐字是「`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录」
    //   ⇒ 接上它**没有放宽任何一层判据**（`readonly_guard` 这一拍一行没动）。
    //   它此前的形状与 `files.ls` 同一形：**能力在那儿，没人接**（那份文件头注逐字
    //   「它今天没有调用方」）。本条就是那个调用方。
    //
    // 🔴 **处理器住 `control/`，与 `files-read` 那六条刻意不同层**：`control/` 的定义是
    //   「**会改变世界**」（`§1.1` 第二条线）—— 这一条真的改变世界，所以它回到了那一层；
    //   而 `files/` 那一族整族纯读（边界①），**一个字节都不许被这一条带脏**。
    //   ⇒ 两面分家：`files-*` 这个线上前缀底下从此有两族，
    //   由 `inbound_structure_guards` 那两条**互不相交**的相等断言各钉一族。
    //
    // ⚠ **CLI 面是自动来的，不是选的**：`cli_control::cli_exposed` = 非 `Run::Builtin`
    //   ⇒ 这一条同拍上了 `lib.rs::SUBCOMMANDS`（不加就当未知 flag、静默进流模式）。
    //   ⇒ 「入口窄」这件事不靠命令面，靠 `readonly_guard` 第三层那条
    //   「**谁引用得到 `control/files_write`**」。理由整段在那个模块的命令面那一节。
    CommandSpec {
        name: "files-create",
        summary: "在文件管理目标根底下新建一份此前不存在的文件",
        codes: &[
            "bad_args",
            "bad_name",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
        ],
        fields: &[out("bytes", "这一趟写进去了几个字节"), arg("content", "要写进去的字节"), out("path", "真正落盘的那个绝对路径，**解完 symlink 的**（原始字节形）"), arg("rel", "相对 `root` 的那一段"), arg("root", "**用户指定的那个文件管理目标根**"), arg("single", "可选布尔：`true` ⇒ `rel` 只许是**一段名字**（界面就地新建 / 改名敲的那一格）；`files-mkdir` 的 `rel`、`files-rename` 的 `to` 同")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ── 〔波 5 ㈡〕 **第 3 步**：改动既有数据的那五条 ──────────
    //
    // 🔴 **这五条才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
    //   处理器同住 `control/files_write.rs`（`readonly_guard` 第三层唯一登记的模块），
    //   而本文件是那一层登记的**唯一一扇门** —— 后端生产树里别处引用那个模块 ⇒ 红。
    //   ⚠ 本段五条与上面 `files-create` 的 `run` 逐字同形（名字从 `r.cmd` 来）。
    // ⚠ 全在阻塞档：同步文件系统 I/O（外加围栏那几次 `canonicalize`），开跑之后打不断
    //   ⇒ `cancel` 命中时回 `not_cancellable`，不撒谎。
    CommandSpec {
        name: "files-mkdir",
        summary: "新建一个目录",
        codes: &[
            "bad_args",
            "bad_name",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
        ],
        fields: &[out("path", "建出来的那个目录（父目录解完 symlink 的）"), arg("rel", "目标根 ＋ 相对段"), arg("root", "目标根 ＋ 相对段"), arg("single", "可选布尔：`true` ⇒ `rel` 只许一段名字；名字规则同 `files-create`，不合 ⇒ `bad_name`")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-rename",
        summary: "改名 / 同根内移动",
        codes: &[
            "bad_args",
            "bad_name",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
        ],
        fields: &[arg("from", "两个相对段，**各过一遍路径解析**（只解 `from` 的话，`to` 半路一条链接就能把东西搬到根外）"), out("path", "新名字的落点"), arg("root", "目标根"), arg("single", "可选布尔：`true` ⇒ `to` 只许一段名字；`to` 的名字规则同 `files-create`，不合 ⇒ `bad_name`"), arg("to", "两个相对段，**各过一遍路径解析**（只解 `from` 的话，`to` 半路一条链接就能把东西搬到根外）")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-delete",
        summary: "删一个文件或一个空目录",
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        // `recursive`（入）· `removed`（出）：显式才删整棵树，逐条目过围栏。
        // `expect`（入）：给了 ⇒ 盘上逐字节等于它才删一份普通文件，否则 `stale`。
        // `limit`（入）· `remaining`（出）：递归删一趟至多删几条、还剩几条（调用方接着发）。
        fields: &[arg("expect", "可选，字符串或 `{\"b16\": …}`：「我读到的是这一份」"), arg("limit", "可选，只对 `recursive: true`：这一趟至多删几条"), out("path", "删掉的那一项"), arg("recursive", "布尔，**缺省 `false`**"), arg("rel", "目标根 ＋ 相对段"), out("remaining", "还剩几条没删（只有带 `limit` 删够了停下时不是 `0`）：调用方再发一趟同样的请求接着删"), out("removed", "这一趟真删掉了几条（含目标自己；不递归那一支恒 `1`）"), arg("root", "目标根 ＋ 相对段")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-chmod",
        summary: "改 unix 权限位",
        // `no_unix_mode`：这个平台没有 unix 权限位（target 轴从这一格现推 Windows 那一格）。
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "no_unix_mode",
            "refused",
        ],
        fields: &[out("before", "**改之前**的权限位（十进制、低 12 位）：撤销 ＝ 拿它再发一趟"), both("mode", "**十进制数值**（`493` = `0o755`），只收低 12 位；超出 ⇒ `refused`"), out("path", "**解到底**的那个真路径"), arg("rel", "目标根 ＋ 相对段"), arg("root", "目标根 ＋ 相对段")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // 写面第七条：同根内复制。与上面五条同住一个模块、
    //   同一扇门、同档（同步文件 I/O，取消不掉）。它**不给第三层添动词**：由 `O_EXCL` 新建 ＋
    //   换名 ＋ 删自己刚建的那一份拼出来（理由住 `control/files_write.rs::copy_entry`）。
    CommandSpec {
        name: "files-copy",
        summary: "同根内复制一份普通文件",
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        // `recursive`（入）· `files` / `dirs`（出）：显式才复制目录。
        fields: &[out("bytes", "复制了几个字节（整棵时是全部普通文件之和）"), out("dirs", "几个目录（单文件那一形恒是 `1` / `0`）"), out("files", "复制了几个普通文件"), arg("from", "两个相对段"), out("links", "照原样复制了几条符号链接（单文件那一形恒是 `0`）"), arg("overwrite", "🔴 **覆盖策略显式**"), out("path", "落点（父目录解完 symlink 的）"), arg("recursive", "**复制目录显式**"), arg("root", "目标根（与写面其余几条同形）"), arg("to", "两个相对段")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // 解压：处理器住
    //   `control/files_extract.rs`（第三层第四个登记的模块），本文件照旧是那一层唯一的门。阻塞档（同步读包 ＋ 落盘）。
    CommandSpec {
        name: "files-extract",
        summary: "解压到一个新目录",
        codes: &[
            "bad_args",
            "bad_path",
            "exists",
            "io_failed",
            "refused",
            "unsupported",
        ],
        fields: &[out("bytes", "文件字节之和"), out("dirs", "几个目录（含补出来的上级）"), out("files", "建了几份文件（含硬链接落成的拷贝）"), arg("fresh", "可缺席的布尔"), out("links", "几条符号链接"), out("path", "落点目录（父目录解完 symlink 的）"), arg("rel", "包在 `root` 下的相对段；格式按名字后缀认：`.zip` · `.tar` · `.tar.gz` · `.tgz`（不分大小写），其余 ⇒ `unsupported`（「不认这种包」）"), arg("root", "那个目录（字符串或 `{\"b16\": …}`）")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_extract::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // 建一条链接（`files_extract.rs::land_link` 那一个动词，FILES2 已在写面闭集里）：链接那条路径过根底下的解析，
    //   目标文本原样（同 `cp -P`）。阻塞档（一次 `symlink`）。
    CommandSpec {
        name: "files-link",
        summary: "建一条符号链接",
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        fields: &[out("path", "建出来的那条链接（父目录解完 symlink 的）"), arg("rel", "链接自己那条路径：`rel` 在 `root` 下过路径解析（同写面其余几条；字符串或 `{\"b16\": …}`）"), arg("root", "链接自己那条路径：`rel` 在 `root` 下过路径解析（同写面其余几条；字符串或 `{\"b16\": …}`）"), arg("target", "链接的目标文本，**原样**写进去（不解、不判，同 `cp -P`）")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_extract::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-write-text",
        summary: "覆盖写一份已经在的普通文件",
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[out("bytes", "写进去了几个字节"), arg("content", "字符串或 `{\"b16\":…}`"), arg("expect", "🔴 **必须给**，恰好 `{\"sha256\": \"<64 位小写十六进制>\"}`：「我看的时候那一份」的摘要（`files-read-text` 交的那个）"), out("path", "**解到底**的那个真路径（它跟链接，理由同 `files-chmod`）"), arg("rel", "目标根 ＋ 相对段"), arg("root", "目标根 ＋ 相对段"), out("sha256", "写进去那份的摘要 —— 调用方拿它当下一次存的 `expect`（连存两次不自撞）")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ── 用户文件的读改写 ＋ 删历史会话 ─────────────────────────
    //
    // 🔴 只有后端的文件管理部分写**用户的文件**，本机也算 ⇒ monitor 进程
    //   不再直接写用户文件；本机与远端都经这三条（`call(origin, …)`，同一条路）。处理器同住
    //   `control/files_write.rs`（第三层），本文件照旧是那一层唯一的门。阻塞档（同步文件 I/O）。
    //   `files-delete-session` 只收 sid（理由住那个模块的 `delete_session`）。
    // 上一版说它是「会话文件围栏**唯一的例外**」—— FN1 之后文件管理写面已不设会话文件围栏，
    //   这一条「只许删会话形状那一份」的限制是它自己的，不是谁的例外。
    CommandSpec {
        name: "files-peek",
        summary: "读改写的读那一半",
        codes: &[
            "bad_args",
            "bad_path",
            "io_failed",
            "not_text",
            "refused",
            "too_large",
        ],
        fields: &[out("exists", "`false` ⇒ **确定不存在**（`text` 为 `null`）"), out("path", "读的是哪一份（最后一段是链接时是解到底的那一份）"), arg("rel", "与写面其余几条同形；**与 `files-put` 同一道围栏**（读的那一份就是写的那一份）"), arg("root", "与写面其余几条同形；**与 `files-put` 同一道围栏**（读的那一份就是写的那一份）"), out("text", "全文（UTF-8）；不在时 `null`")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-put",
        summary: "整份替换一份文本文件",
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[arg("backup", "可缺席的布尔（缺省否）"), out("bytes", "新内容的字节数"), out("changed", "真的写了吗（新内容与盘上逐字节相同 ⇒ `false`，一个字节不动）"), arg("content", "新全文（字符串或 `{\"b16\":…}`），**必须给**"), out("created", "这份文件是这一次新建的"), arg("expect", "🔴 **必须给**：`null` = 「我读的时候它不在」；字符串 / b16 = 「我读到的就是这一份」"), arg("parents", "可缺席的布尔（缺省否）"), out("path", "落点"), arg("rel", "同 `files-peek`"), arg("root", "同 `files-peek`")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-delete-session",
        summary: "删一份历史会话",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("path", "删掉的那一份"), arg("sid", "🔴 **只收 sid**：多给任何一个键 ⇒ `bad_args`")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_write::answer_wire(&r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ──：上传的**提交** ──────────────────────
    //
    // 🔴 SFTP 缩成只做传输之后，上传只写暂存区（`~/.cc-monitor/staging/<key>.part`）；
    //   把它挪进用户目标的**那一下**在这里 —— 用户逐字「现在只允许后端的文件管理部分写文件」。
    //   处理器住 `control/files_commit.rs`（`readonly_guard` 第三层第二个登记的模块），
    //   本文件照旧是那一层唯一的门。阻塞档：同步文件系统 I/O（围栏的 `canonicalize` ＋ 改名）。
    // ── 存盘装不进一条请求行时：逐块进暂存区 ＋ 读回拼起来原地覆盖 ──────────
    //   同住 `control/files_commit.rs`（第三层第二个模块），阻塞档理由同上一条。
    CommandSpec {
        name: "files-stage-chunk",
        summary: "存盘的一块进暂存区",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("bytes", "这一块写进去的字节数"), arg("content", "字符串或 `{\"b16\":…}`，**至少 1 字节**（空块 ⇒ `bad_args`）"), arg("key", "这一次存盘的键：**恰好 32 位小写十六进制**（调用方每次存盘现造一个）"), arg("seq", "块号，从 0 起的非负整数")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-commit-text",
        summary: "按块读回、拼起来、覆盖写",
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[arg("bytes", "拼起来**必须恰好**这么长；最多 8 MiB（`files-read-text` 一趟的天花板：存得回的要读得回来），超了 ⇒ `bad_args`"), arg("chunks", "块数：读回 `0..chunks` 这几块"), arg("expect", "🔴 **必须给**，与 `files-write-text` 的 `expect` 同形同义（摘要形 CAS）：盘上那份对不上 ⇒ `stale`，目标一个字节没动（块照样删掉）"), arg("key", "与 `files-stage-chunk` 同一个键"), out("path", "解到底的那个真路径"), arg("rel", "目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原子地换（权限位沿用；属主 / 硬链接不再保留，见 `files-write-text`）"), arg("root", "目标，语义与 `files-write-text` **完全相同**：必须已经在、是普通文件；跟链接（解到底再判一次）；原子地换（权限位沿用；属主 / 硬链接不再保留，见 `files-write-text`）"), out("sha256", "写进去那份的摘要")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-commit-upload",
        summary: "把暂存区里一份传完的上传件挪进目标",
        codes: &["bad_args", "bad_path", "io_failed", "refused", "stale"],
        fields: &[arg("bytes", "可缺席（缺席 ＝ 此前那一形）"), arg("chunks", "可缺席（缺席 ＝ 此前那一形）"), arg("expect", "🔴 **必须给**，恰好 `{\"sha256\": \"<64 位小写十六进制>\"}`：传输台上传时对**本机那份整份**算的摘要（`transfer` 帧 `end.sha256`，窗口原样交来）"), arg("key", "暂存件的键：**恰好 32 位小写十六进制**"), arg("overwrite", "🔴 **必须给**（`true` / `false`），不给默认值"), out("path", "落点（父目录解过 symlink 的那一个）"), arg("rel", "目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；「会话文件那一问」删了）；`rel` 也收 `{\"b16\": …}`"), arg("root", "目标根 ＋ 相对段，先过写面那两道路径解析（词法 ＋ 父目录解 symlink；「会话文件那一问」删了）；`rel` 也收 `{\"b16\": …}`")],
        takes_input: true,
        run: Run::BlockingData(|r| {
            crate::control::files_commit::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    CommandSpec {
        name: "files-browse",
        summary: "告诉后端「用户现在在看哪几个目录」",
        codes: &["bad_args", "bad_path"],
        // +`watching` · `watch_failed` · `watch_error`（进程里那一个监听器跟上名单）。
        fields: &[out("added", "这一趟新挂上几个"), out("browse_watch_cap", "上限（今天 64）"), arg("dirs", "**此刻的整份名单**（数组，每项是字符串或 `{\"b16\":…}`）"), out("rejected", "超过上限被**拒掉**几个"), out("removed", "这一趟卸掉几个（用户不再看它们了）"), out("watch_error", "这一趟没挂上的条数 ＋ 第一条原因（`null` ＝ 都挂上了）"), out("watch_failed", "这一趟没挂上的条数 ＋ 第一条原因（`null` ＝ 都挂上了）"), out("watching", "此刻**真挂着** watch 的目录数（进程里那一个监听器，跟着名单挂 / 卸）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-rebuild",
        summary: "重建常驻文件名索引：走一遍，做完返回",
        // `already_rebuilding`：非阻塞互斥抢不到那个位。`no_home`：没给根而家目录说不出。
        codes: &["already_rebuilding", "bad_path", "no_home", "unreadable"],
        fields: &[out("entries", "这一趟走出来多少条（目录 ＋ 文件 ＋ 符号链接，根自己不算）"), both("path", "要走的那个**根**"), out("resident_bytes", "新那份索引在后端内存里占多少字节（路径总长 ＋ 5×条数：每条 4 字节界桩 ＋ 1 字节类型，**算得出的量**）"), out("skipped_mounts", "根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在）"), out("truncated", "撞到条目上限、没走完 ⇒ 这份索引是**不完整**的"), out("unreadable_dirs", "这一趟有几个子目录读不进去（权限等）"), out("unreadable_paths", "那几个子目录（前 20 个，同 `files-index-status` 那一格）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **按内容搜**：`{path, needle, ignore_ascii_case?, limit?}` ⇒ 命中的那几份（第一处命中那一行 ＋ 命中几行）
    //   ＋ 走了多少。**可撤** ⇒ 异步档：走那一趟放进阻塞线程池，`cancel` 丢掉这个 future 时守卫置取消位、那一趟随即停
    //   （`files/mod.rs::answer_grep_cancellable`）。纯读。
    CommandSpec {
        name: "files-grep",
        summary: "在一个目录底下按内容搜",
        codes: &["bad_args", "bad_path", "unreadable"],
        fields: &[out("bytes", "这一趟读了几份、多少字节"), out("files", "这一趟读了几份、多少字节"), out("hits", "每份命中的文件一项：`path`"), arg("ignore_ascii_case", "只对 ASCII 段大小写不敏感"), both("limit", "最多回几份命中的文件"), out("links", "碰到几条链接 —— **不跟**（它不一定在这棵树里，跟进去就是出界）；根本身是链接 ⇒ 不进去"), arg("needle", "要找的**字节**子串（字符串或 `{\"b16\":…}`），1 至 256 字节"), both("path", "从哪个目录往下搜（字符串或 `{\"b16\":…}`；也可以是一份文件）；回送原样那一格"), out("skipped_binary", "看着像二进制（前 8 KiB 有 NUL）"), out("skipped_large", "超过 8 MiB"), out("skipped_mounts", "挂在底下的别的文件系统"), out("stopped", "`null`"), out("truncated", "上界到了没走完：`\"hits\"`（命中份数到 `limit`）/ `\"bytes\"`（一趟累计读到 256 MiB）；走完 ⇒ `false`"), out("unreadable", "读不了的，各跳过几项")],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::files::answer_grep_cancellable(r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "files-find",
        summary: "在常驻索引里查",
        codes: &["bad_args", "bad_path", "bad_query", "superseded"],
        fields: &[out("cover_root", "要搜全这一趟，重走该走哪个根：手上那份盖得住 ⇒ 它的根；否则范围在家目录里 ⇒ 家目录；否则 ⇒ 范围本身（都说不出 ⇒ `null`）"), both("desc", "`true` ⇒ 倒过来（默认 `false`）"), out("hits", "这一屏的命中，每条一个对象：`path` · `kind` · `location` · `size` · `mtime_secs` · `mtime_text`（修改时间的短写法，这台本地钟写好）· `marks`"), out("index_age_secs", "答这一趟用的那份索引，是多久以前建的"), out("index_missing", "索引还没建过 ⇒ 几个计数全是 0，而那不是「没搜到」；客户端要自己发 `files-index-rebuild`"), out("index_root", "手上那份索引的根（没建过 ⇒ `null`）"), arg("limit", "这一屏最多回几条"), both("offset", "从第几条命中起回（前面的只数不回）"), out("out_of_index", "这一趟的范围（`under` 或家目录）不在手上那份索引里 ⇒ 结果只是索引里碰巧有的那一部分"), arg("query", "原样的搜索词（字符串）"), out("scanned", "这一趟扫了几条（= 索引条目数）"), arg("scope", "`\"under\"`（默认，照 `under`）/ `\"machine\"`（整台机器：范围由这台自己定 —— unix `/`，Windows 家目录那块盘的根；`under` 不看）"), both("seq", "这一趟的号（非负整数，可不给）"), both("sort", "按哪一列排：`relevance`（默认）· `name` · `location` · `mtime` · `size`"), out("stale", "该重走了（`index_age_secs > rewalk_interval_secs`）"), out("start", "这一趟的搜索起点（`location` 相对它算；说不出 ⇒ `null`）"), arg("stream", "这个号属于哪一个搜索框（字符串，不给 ⇒ 空串）"), out("total_hits", "一共命中几条，**不受分页影响**"), out("truncated", "这一屏之后还有（往下翻：同号、`offset` 加上这一屏的条数）"), arg("under", "只搜这个目录**底下**（不含它自己；字符串或 `{\"b16\":…}`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-index-status",
        summary: "索引的新鲜度 / 条目数 / 常驻字节",
        codes: &[],
        fields: &[out("age_secs", "这份索引建好到现在多少秒"), out("browse_watch_cap", "最多挂几个"), out("browse_watches", "此刻给「用户正在浏览的那几个目录」挂着几个 watch"), out("cold_first_build_secs", "后端**声明**的冷启动首建大约要几秒（今天 10，出处见 `index.rs::COLD_FIRST_BUILD_SECS`：一台 NVMe 上 `find` 的冷缓存读数取上整，**代理指标、不是实测**）"), out("entries", "索引里有几条"), out("index_missing", "还没建过"), out("resident_bytes", "它在后端内存里占多少字节（索引**只在内存里，重启重建**）"), out("rewalk_interval_secs", "🔴 **后端声明的重走周期**（今天 300）"), out("skipped_mounts", "根底下挂着的**别的文件系统**没走进去的个数（设备号比对；那个目录本身照样在索引里，它底下的不在）"), out("stale", "`age_secs > rewalk_interval_secs`"), out("truncated", "上一趟遍历撞到了条目数上限，没走完"), out("unreadable_dirs", "上一趟遍历里有几个目录读不进去（权限等）"), out("unreadable_paths", "那几个读不进去的目录（前 20 个，按遍历先后；字符串或 `{\"b16\":…}`；根自己不在里面 —— 根读不进去是 `files-index-rebuild` 的 `unreadable`）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-ls",
        summary: "列一个目录的直接子项",
        codes: &["bad_path", "denied", "not_dir", "not_found", "unreadable"],
        fields: &[out("entries", "每项一个对象：`path`（原始字节形）· `kind` · `size` · `mtime_secs`（后两个拿不到就**不出这个键**，不填 0）· `mtime_text` · `mtime_full`（跟着 `mtime_secs` 出）"), out("kind", "**闭集四个词**：`dir` / `file` / `symlink` / `other`"), arg("limit", "这一趟最多回几条"), out("link_dir", "只在 `kind` 是 `symlink` 时出：它指向的是不是目录（跟链接问一次）"), out("link_to", "只在 `kind` 是 `symlink` 时出：它指向什么 —— `dir` · `file` · `missing`（断了：指向的东西不在 / 读不到）"), out("mtime_full", "修改时间的完整写法 `YYYY-MM-DD HH:MM:SS`（这台本地钟写好，窗口照抄）"), out("mtime_secs", "Unix 纪元秒"), out("mtime_text", "修改时间列里那一格：今天 `HH:MM` · 今年 `MM-DD` · 往年 `YYYY-MM-DD`（这台本地钟写好，窗口照抄）"), arg("path", "要列的那个目录"), out("size", "字节数"), out("total", "目录里一共读到几项（含没回送的；截断时界面写「前 n / total 项」）"), out("truncated", "目录里的项数多于回送的条数（被 `limit` 截了）"), out("unreadable", "目录打开了、其中几项读不出来（没有回送、不算进 `truncated`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-stat",
        summary: "一个路径的元数据",
        codes: &["bad_path", "unreadable"],
        // +`mode`（能力 `files.stat` 同拍加的那一格；非 unix 缺席）· `owner` · `link_target`（文件窗口「属性」）。
        fields: &[out("kind", "`dir` · `file` · `symlink` · `other`"), out("link_target", "路径**本身**是符号链接 ⇒ 它的目标原文（`readlink`，不解不跟；原始字节形：字符串或 `{\"b16\":…}`）；不是链接 ⇒ `null`"), out("mode", "unix 权限位的低 12 位（十进制数；`420` = `0o644`）"), out("mtime_full", "修改时间的完整写法 `YYYY-MM-DD HH:MM:SS`（这台本地钟写好；跟着 `mtime_secs` 出）"), out("mtime_secs", "Unix 纪元秒（`mtime_secs` 拿不到就不出这个键）"), out("mtime_text", "修改时间的短写法（今天 `HH:MM` · 今年 `MM-DD` · 往年带年；跟着 `mtime_secs` 出）"), out("owner", "属主（跟链接）：用户名；查不到名字 ⇒ uid 的数字串；非 unix ⇒ `null`"), both("path", "入方向是要问的那个路径；出方向原样回送（原始字节形）"), out("readonly", "这个路径此刻是不是只读"), out("size", "字节数")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // ──：窗口换走通道的那两问 ────────────────
    //
    // 🔴 **同一族（`files-read`）的第七、第八条，整族照旧纯读**：编辑器读一份文本 ·
    //   开窗前「那台机器的 home 在哪」。此前窗口为这两问各拨一条 SFTP（
    //   那张欠账表），现在经通道问后端 —— 窗口进程够后端**只剩通道**这一条路。
    // ⚠ `run` 与同族那六条逐字同形（名字从 `r.cmd` 来）；同在 `Run::Blocking`、同样取消不掉。
    CommandSpec {
        name: "files-read-text",
        summary: "读一份文本进编辑器",
        codes: &[
            "bad_args",
            "bad_path",
            "not_text",
            "too_large",
            "unreadable",
        ],
        fields: &[out("bytes", "字节数"), arg("max_bytes", "🔴 **必须给**：编辑上限是**调用方**的（它答的是「这个文本控件打字卡不卡」）"), both("path", "要读的那份文件"), out("sha256", "交出去的那份字节的 SHA-256（64 位小写十六进制）"), out("text", "整份内容（合法 UTF-8）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 读族第十条：按字节寻址分块读回（非 UTF-8 名的下载）。同族同形、同在阻塞档。
    CommandSpec {
        name: "files-read-chunk",
        summary: "按字节寻址分块读回",
        codes: &["bad_args", "bad_path", "not_text", "unreadable"],
        fields: &[out("content", "这一块的原始字节，恒为 `{\"b16\": …}`"), out("eof", "这一块读到了末尾（`offset` 越过末尾 ⇒ 空块、`eof: true`）"), arg("len", "从哪读、读多少；`len` 只收 `1..=READ_CHUNK_MAX_BYTES`（256 KiB），越界 `bad_args`、不夹小"), arg("offset", "从哪读、读多少；`len` 只收 `1..=READ_CHUNK_MAX_BYTES`（256 KiB），越界 `bad_args`、不夹小"), both("path", "一份普通文件（字符串或 `{\"b16\": …}`；非 UTF-8 名的下载就走这一形，SFTP 库的路径是 `String` 寻址不到）"), out("size", "此刻整份多大（调用方据此报进度、判读完）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 读族第九条：算目录大小。与同族那几条逐字同形、同在阻塞档。
    CommandSpec {
        name: "files-size",
        summary: "算一个目录有多大",
        codes: &["bad_path", "unreadable"],
        fields: &[out("bytes", "普通文件的**表观大小**之和（与 `files-ls` 的 `size` 同口径，不是占盘块数）"), out("dirs", "目录数（含顶上那个目录自己）"), out("files", "普通文件数"), out("links", "符号链接数 —— **不跟、不算字节**（它指向的东西不一定在这棵树里）"), out("other", "设备 / 管道 / 套接字之类"), both("path", "要算的那个路径（字符串或 `{\"b16\":…}`）；出方向原样回送（原始字节形）"), out("skipped_mounts", "底下挂着的**别的文件系统**，没走进去的个数（设备号比对）"), out("unreadable_dirs", "读不进去、跳过的目录数（不中断）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "files-home",
        summary: "后端这个用户的 home",
        codes: &["no_home"],
        fields: &[out("path", "后端这个进程环境里的 home（原始字节形）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::files::answer_wire(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
