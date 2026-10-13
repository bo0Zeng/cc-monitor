//! 命令表 · 文件管理：`files-*`（读 · 写 · 上传的提交 · 解压 · 删历史会话）。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Fail, Run};
use crate::stream::inbound::typed;

/// 删历史会话那一条要问的两件事，从原生那一侧（适配层注册表）的窄口取来，
/// 由本门递给文件管理写面 —— 写面自己一家 agent 的布局都不认（`files/module_boundary_guard.rs` 围栏那一类为零）。
pub(in crate::stream::inbound) const SESSION_PORT: crate::control::files_write::SessionPort =
    crate::control::files_write::SessionPort {
        locate: crate::agents::locate_session_for_delete,
        is_record: crate::agents::is_session_record,
        file_name: crate::agents::session_file_name_of,
    };

pub(super) const SPECS: &[CommandSpec] = &[
    // ── `files-read` 这一族上线 ────────────────────────────
    //
    // 🔴 **处理器不住 `control/`，这是本仓第一条** —— 而且它必须不住那里：
    //   `control/` 的定义是「**会改变世界**」（`§1.1` 第二条线），而这一族整族纯读
    //   （边界①，`readonly_guard` 4259 行一行没动）；
    //   而读面 `observe/` 又被入方向头注那条硬约束挡着（`inbound` 不许出现 `observe::`，
    //   `inbound_structure_guards::inbound_never_reaches_into_the_observe_layer` 在钉）。
    //   ⇒ 它住顶层 `files/`（`lib.rs` 一条 `pub mod`）。入方向头注那句「每条命令的处理器属于 `control/`」
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
    // ── 裁出来的第五、第六条 ──────────────
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
    // ── 把那份零消费者的写原语接上 ──
    //
    // 🔴 **这一条是本族第一条会往盘上写的命令，而它不花用户那句「允许」**：
    //   `control/files_write.rs` 09-19 那时在 `readonly_guard` 的写白名单上（今天写经 `Face`），
    //   射程逐字是「`O_EXCL` 新建一份**此前不存在**的文件；不删、不改名、不覆盖、不建目录」
    //   ⇒ 接上它**没有放宽任何一层判据**（`readonly_guard` 一行没动）。
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
    //   ⇒ 「入口窄」这件事不靠命令面，靠写句柄：本族用 `Run::BlockingFace`，分派那一层才递 `Face`，
    //   别族拿不到它。理由整段在 `control/files_write` 头注「写怎么被挡住」那一节。
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ── 改动既有数据的那五条 ──────────
    //
    // 🔴 **这五条才花掉用户那句话**：「现在只允许后端的文件管理部分写文件」。
    //   处理器同住 `control/files_write.rs`，写都经分派递进来的 `Face`（`Run::BlockingFace` 只许本族用）。
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // 写面第七条：同根内复制。与上面五条同住一个模块、
    //   同一扇门、同档（同步文件 I/O，取消不掉）。它**不给 `Face` 添动词**：由 `O_EXCL` 新建 ＋
    //   换名 ＋ 删自己刚建的那一份拼出来（理由住 `control/files_write.rs::copy_entry`）。
    CommandSpec {
        name: "files-copy",
        summary: "同根内复制一份普通文件",
        codes: &["bad_args", "bad_path", "io_failed", "refused"],
        // `recursive`（入）· `files` / `dirs`（出）：显式才复制目录。
        fields: &[out("bytes", "复制了几个字节（整棵时是全部普通文件之和）"), out("dirs", "几个目录（单文件那一形恒是 `1` / `0`）"), out("files", "复制了几个普通文件"), arg("from", "两个相对段"), out("links", "照原样复制了几条符号链接（单文件那一形恒是 `0`）"), arg("overwrite", "🔴 **覆盖策略显式**"), out("path", "落点（父目录解完 symlink 的）"), arg("recursive", "**复制目录显式**"), arg("root", "目标根（与写面其余几条同形）"), arg("to", "两个相对段")],
        takes_input: true,
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // 解压：处理器住
    //   `control/files_extract.rs`，写经 `Face`。阻塞档（同步读包 ＋ 落盘）。
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_extract::answer_wire(face, &r.cmd, &r.args)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_extract::answer_wire(face, &r.cmd, &r.args)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ── 用户文件的读改写 ＋ 删历史会话 ─────────────────────────
    //
    // 🔴 只有后端的文件管理部分写**用户的文件**，本机也算 ⇒ monitor 进程
    //   不再直接写用户文件；本机与远端都经这三条（`call(origin, …)`，同一条路）。处理器同住
    //   `control/files_write.rs`，写经 `Face`。阻塞档（同步文件 I/O）。
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_write::answer_wire(face, &r.cmd, &r.args, &SESSION_PORT)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    // ── 上传的**提交** ──────────────────────
    //
    // 🔴 SFTP 缩成只做传输之后，上传只写暂存区（`~/.cc-monitor/staging/<key>.part`）；
    //   把它挪进用户目标的**那一下**在这里 —— 用户逐字「现在只允许后端的文件管理部分写文件」。
    //   处理器住 `control/files_commit.rs`，写经 `Face`。阻塞档：同步文件系统 I/O（路径解析的 `canonicalize` ＋ 改名）。
    // ── 存盘装不进一条请求行时：逐块进暂存区 ＋ 读回拼起来原地覆盖 ──────────
    //   同住 `control/files_commit.rs`，阻塞档理由同上一条。
    CommandSpec {
        name: "files-stage-chunk",
        summary: "存盘的一块进暂存区",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("bytes", "这一块写进去的字节数"), arg("content", "字符串或 `{\"b16\":…}`，**至少 1 字节**（空块 ⇒ `bad_args`）"), arg("key", "这一次存盘的键：**恰好 32 位小写十六进制**（调用方每次存盘现造一个）"), arg("seq", "块号，从 0 起的非负整数")],
        takes_input: true,
        run: Run::BlockingFace(|r, face| {
            crate::control::files_commit::answer_wire(face, &r.cmd, &r.args)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_commit::answer_wire(face, &r.cmd, &r.args)
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
        run: Run::BlockingFace(|r, face| {
            crate::control::files_commit::answer_wire(face, &r.cmd, &r.args)
                .map(Some)
                .map_err(|f| Fail::new(f.code, f.said).with_raw(f.raw.as_deref()))
        }),
    },
    typed::blocking::<crate::stream::inbound::files_face::FilesBrowse>("告诉后端「用户现在在看哪几个目录」"),
    typed::blocking::<crate::stream::inbound::files_face::FilesIndexRebuild>("重建常驻文件名索引：走一遍，做完返回"),
    // **按内容搜**：`{path, needle, ignore_ascii_case?, limit?}` ⇒ 命中的那几份（第一处命中那一行 ＋ 命中几行）
    //   ＋ 走了多少。**可撤** ⇒ 异步档：走那一趟放进阻塞线程池，`cancel` 丢掉这个 future 时守卫置取消位、那一趟随即停
    //   （`files/mod.rs::answer_grep_cancellable`）。纯读。
    typed::asynchronous::<crate::stream::inbound::files_face::FilesGrep>("在一个目录底下按内容搜"),
    typed::blocking::<crate::stream::inbound::files_face::FilesFind>("在常驻索引里查"),
    typed::blocking::<crate::stream::inbound::files_face::FilesIndexStatus>("索引的新鲜度 / 条目数 / 常驻字节"),
    typed::blocking::<crate::stream::inbound::files_face::FilesLs>("列一个目录的直接子项"),
    typed::blocking::<crate::stream::inbound::files_face::FilesStat>("一个路径的元数据"),
    // ── 窗口换走通道的那两问 ────────────────
    //
    // 🔴 **同一族（`files-read`）的第七、第八条，整族照旧纯读**：编辑器读一份文本 ·
    //   开窗前「那台机器的 home 在哪」。此前窗口为这两问各拨一条 SFTP（
    //   那张欠账表），现在经通道问后端 —— 窗口进程够后端**只剩通道**这一条路。
    // ⚠ `run` 与同族那六条逐字同形（名字从 `r.cmd` 来）；同在 `Run::Blocking`、同样取消不掉。
    typed::blocking::<crate::stream::inbound::files_face::FilesReadText>("读一份文本进编辑器"),
    // 读族第十条：按字节寻址分块读回（非 UTF-8 名的下载）。同族同形、同在阻塞档。
    typed::blocking::<crate::stream::inbound::files_face::FilesReadChunk>("按字节寻址分块读回"),
    // 读族第九条：算目录大小。与同族那几条逐字同形、同在阻塞档。
    typed::blocking::<crate::stream::inbound::files_face::FilesSize>("算一个目录有多大"),
    typed::blocking::<crate::stream::inbound::files_face::FilesHome>("后端这个用户的 home"),
];

// 文件窗口读的那几条回包的线上字节钉在夹具里（换形前后逐字节比）。
#[cfg(test)]
#[path = "../../../../../tests/backend/stream/file_manager_wire_tests.rs"]
mod wire_tests;
