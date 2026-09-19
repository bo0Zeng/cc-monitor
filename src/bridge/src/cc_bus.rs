//! B03：cc-bus 状态的**纯解析层**（无 I/O，可单测）。
//!
//! 为什么单独一层：`~/.cc-bus/` 里的状态文件**实测是脏的**，且脏得比计划预估严重。
//! 2026-07-28 直接读开发机上那份真实数据，结论见
//! `.claude/planned-build/unify-launch/features/B03-dirty-data-samples.md`：
//!   · `spawned.tsv` **15 行里 5 行畸形**（另有 3 行是真空行，按契约不计）——目录名与
//!     任务文本里含 `\n`，把一条记录劈成多行。解析器绝不能"发现坏行就整体报错"，
//!     那样 7 条好记录也一起没了。
//!     （**我原先写的"8 行坏、53%、坏行是多数派"是错的**：8 = 5 畸形 + 3 空行，而
//!     "空行不计入 skipped"恰恰是我自己在下方立的契约。写文档时用了代码里明令禁止的口径。
//!     实际 5/15=33%，非空行口径 5/12=42%，都不是多数派。设计结论不变，错的是记录。）
//!   · `inbox/` 里 `--help.jsonl`、`282.jsonl` 至今仍在盘上（有人敲过 `cc-send --help`，
//!     `--help` 被当成了收件人）。
//!   · `agents.tsv` 结构干净（37 行全 3 字段），它的脏在**陈旧**（最早 10 天前）不在畸形
//!     ——所以「登记 ≠ 在线」必须分开呈现，登记只证明它登记过。
//!
//! 契约：**跳过坏行并计数，永不 panic、永不因坏行丢掉好行**。`skipped` 如实回报给 UI，
//! 显示「N 条无法解析」而不是假装干净。

/// cc-bus id 合法性。**照抄 `shared/ccm:358-362` 的判据，不另发明一套。**
///
/// 关键的一条是 **拒绝前导 `-`**：写这段时我第一版用的是「只含 `[A-Za-z0-9_-]`」，
/// 实测 **`--help` 通过了**——因为 `-` 本来就在字符类里，`[A-Za-z0-9_-]+` 完整匹配 `--help`。
/// 而 id 会被拼进命令行（`cc-send <id> …`、`tmux has-session -t =<id>`），
/// `-` 开头会被下游当成选项解析。ccm 那边同样的理由写着
/// `""|-*) die "非法 tmux 会话名（空或以 - 开头）"`。
pub fn is_valid_bus_id(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `agents.tsv` 的一行：id / pane 地址 / 登记时间。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct CcBusAgent {
    pub id: String,
    pub pane: String,
    pub registered_at: String,
}

/// `spawned.tsv` 的一行：id / 工作目录 / spawn 时间 / 初始任务。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct CcBusSpawned {
    pub id: String,
    pub dir: String,
    pub spawned_at: String,
    pub task: String,
}

/// 一次读回的完整状态。`skipped` = 两个文件里被跳过的坏行总数。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct CcBusState {
    pub agents: Vec<CcBusAgent>,
    pub spawned: Vec<CcBusSpawned>,
    pub skipped: usize,
}

/// 判一行是否该被跳过：字段数不足、或 id 非法。
/// 空行**不计入 skipped**——文件尾部的空行是正常的，把它算成"无法解析"会让 UI 虚报。
fn row_fields(line: &str, want: usize) -> Option<Vec<&str>> {
    // **只把"真空行"当空行**（B03 审计重要-4）：原先用 `line.trim().is_empty()`，
    // 于是 `"\t\t\t"` 这种**有结构无内容**的行被当成空行 → 既不算好行也不计 skipped，
    // 凭空蒸发。按契约它该算坏行。判据改成"去掉空白后为空**且**不含制表符"。
    if line.trim().is_empty() && !line.contains('\t') {
        return None;
    }
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < want {
        return Some(Vec::new()); // 有内容但字段不够 → 坏行（与空行区分开）
    }
    Some(f)
}

/// 解析 `agents.tsv`。返回 (好行, 坏行数)。
pub fn parse_agents_tsv(text: &str) -> (Vec<CcBusAgent>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let Some(f) = row_fields(line, 3) else {
            continue;
        };
        if f.is_empty() || !is_valid_bus_id(f[0]) {
            skipped += 1;
            continue;
        }
        out.push(CcBusAgent {
            id: f[0].to_string(),
            pane: f[1].to_string(),
            // 时间戳解析失败**不丢整行**——UI 标注"时间未知"即可，
            // 一个坏时间戳不该让这个 agent 从驾驶舱里消失。
            registered_at: f[2].to_string(),
        });
    }
    (out, skipped)
}

/// 解析 `spawned.tsv`。返回 (好行, 坏行数)。
pub fn parse_spawned_tsv(text: &str) -> (Vec<CcBusSpawned>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let Some(f) = row_fields(line, 4) else {
            continue;
        };
        if f.is_empty() || !is_valid_bus_id(f[0]) {
            skipped += 1;
            continue;
        }
        out.push(CcBusSpawned {
            id: f[0].to_string(),
            dir: f[1].to_string(),
            spawned_at: f[2].to_string(),
            // **末字段要把余下的都收回来**（B03 审计重要-4）：任务文本里出现一个制表符，
            // 原先的 `f[3]` 只留第一段、后面**静默丢弃**，UI 上看不出来被截断了。
            task: f[3..].join("\t"),
        });
    }
    (out, skipped)
}

/// 把一次 `cat` 读回的两个文件按分隔标记切开（省一次 SSH 往返）。
/// 标记取足够长的固定串，避免与 TSV 内容撞车。
pub const CC_BUS_SPLIT_MARKER: &str = "@@CCMON-CCBUS-SPLIT@@";

/// 读面的**自述头**〔ccbus-win 09-10〕：那条命令自己报「我读得了吗」。
///
/// 形状逐字是 `@@CCMON-CCBUS-HEAD@@ home=<0|1> cat=<0|1>`，
/// 由 [`CC_BUS_CAT_CMD`] 用**清一色 shell 内建**（`[` / `command -v` / `printf`）打出来
/// —— 这一点是全部的关键：外部命令一个都没有的壳里，它照样回得来。
/// 解释归 [`interpret_cc_bus_read`]。
pub const CC_BUS_HEAD_MARKER: &str = "@@CCMON-CCBUS-HEAD@@";

/// ⚠ **本函数的宽容是给「切」用的，不是给「判有没有」用的**〔ccbus-win 09-10 订正〕。
///
/// 原注释写着「缺分隔标记（远端只有一个文件 / cat 部分失败）→ 宽容降级」，
/// 而那两条理由**都不成立**：打标记的是 `printf`（shell 内建），它无条件跑，
/// 既不看第一个 `cat` 的脸色，也不看文件在不在 —— 本机实测过三种情形，
/// 标记每次都在（`cat` 整个缺席时也在）。⇒ 标记**不在**只可能是一件事：
/// **那条命令根本没跑到那里**（壳跑不了这段语法 / 流被掐断 / 输出被半路截掉）。
/// 那是「读不到」，不是「只有一个文件」。
///
/// ⇒ 「有没有标记」这个判断已经上提到 [`interpret_cc_bus_read`]，由它回错。
/// 本函数保持宽容（它只负责切，且 `missing_marker_degrades_gracefully` 钉着这条契约）。
pub fn split_combined<'a>(raw: &'a str, marker: &str) -> (&'a str, &'a str) {
    match raw.split_once(marker) {
        Some((a, b)) => (a, b),
        None => (raw, ""),
    }
}

// ===== IPC 层：按需经 SSH 读一次远端状态。**照抄 `mcp.rs::fetch_remote_claude_json`** =====
//
// 为什么是「按需读」而不是订阅/轮询：cc-bus 的状态全在远端本机 `~/.cc-bus/`，cc-monitor
// 跑在 Windows 只能经 SSH 看。两条备选——复用 daemon 既有 inotify watcher（**违反 daemon
// 零改红线**，且要新增协议帧），或按需刷新。取后者，形状逐条对齐 `mcp.rs`：
// 定值命令（零用户输入拼接 → 零注入面）、30s 超时、32MB 上限、
// **超限拒收**（devbench F10b：不再是「读满就停」的静默截断）、宽容解析（缺/坏 → 空）、
// 大解析进 `spawn_blocking`。**无 setInterval、无后台定时任务**（红线）。

/// 一条**定值**命令读回两个文件，中间插分隔标记 —— 省一次 SSH 往返。
/// `origin` 只用于选连接配置，**不参与命令串拼接**，故此串是常量、零注入面
/// （同 `mcp.rs` 那条 `CMD` 常量的形状）。
/// 尊重 `CC_BUS_HOME`（cc-bus 自己就用这个变量定位状态目录）。
/// 结尾 `true` 保证两个文件都不存在时命令仍 rc=0——"没装 cc-bus"不是错误，是一种状态。
/// ## 为什么这条读面**还没**走 daemon〔`P4a2` 摸底 08-12，有读数〕
///
/// 常被问「远端这条为什么不走已经连着的 daemon，省掉每次一次 SSH 握手」。收益是**真的**，
/// 代价也是真的，两边都量过：
///
/// **收益**：`connect_and_exec_cmd` 每次都 `connect_session` —— **不复用连接**。
/// 实测（08-12，loopback，三次一致）：一次 SSH 握手+鉴权 **≈180ms**；
/// 而已连着的通道上跑一条命令 **≈0ms**。真实远端还要在 180ms 上再加 RTT×握手往返数。
/// ⇒ 每次开驾驶舱省的就是这 180ms 起步。**它不是零，但驾驶舱是按需读、不轮询**，
/// 用户一次点击等 0.2s —— 这个量级不足以单独撑起一次架构改动。
///
/// **代价**：`CC_BUS_CAT_CMD` 逐字知道 `~/.cc-bus/agents.tsv` 长什么样。换传输 = 把这份
/// **文件格式耦合搬进 daemon**，而 `P4b` 作废重写的理由逐字是「**cc-bus 后面肯定还是要变的**」——
/// 把一个正要变的东西焊进 daemon，是拿 180ms 换一次以后更贵的返工。
///
/// **⚠ 解锁条件不是「`P4b` 落地」**（那件 08-12 已签收，但它只删掉了 cc-spawn 的复用判定，
/// **`agents.tsv` 的格式契约一字未动**）——实质条件是**格式契约稳下来**。
/// 届时的正确形状多半**不是**把 shell 串搬过去，而是 daemon 出一条**具名的读命令**
/// （形状抄 `P4d` 那批：stdin JSON 进 / stdout JSON 出 / 能力探测口报得出来）。
///
/// ## 🔴 为什么它要**自己报「我读没读得了」**〔ccbus-win 09-10，有实测〕
///
/// 这条命令的主体是两次 `cat`，而 `cat` 是**外部命令** —— 它不在那个壳的 `PATH` 上时：
/// · `2>/dev/null` 把 "command not found" 一起吃掉（**stderr 里什么都没有**，实测）；
/// · `printf` 与 `true` 是内建，照跑 ⇒ **rc=0**；
/// · stdout 只剩那个分隔标记。
///
/// 本机现打（`bash -lc`，`PATH` 指向一个空目录 vs. 目录整个不存在）：
/// ```text
/// cat 缺席、文件真的在   → "\n@@CCMON-CCBUS-SPLIT@@\n"   rc=0
/// cat 在、目录根本不存在 → "\n@@CCMON-CCBUS-SPLIT@@\n"   rc=0
/// ```
/// **两者逐字节相同**（`cmp` 报 IDENTICAL）。⇒ 「问不出来」与「真的一个都没有」
/// 在这条命令的出口上**同形**，而驾驶舱把后者渲染成「这台机器上没有登记过的 agent」。
/// 那正是本仓一整族判据在防的病（`config_surface.rs` 头注 · `sftp.rs` 的 `TargetBinary`
/// 四值枚举 · `cc_bus_deploy::local_ccm_too_old_warning` 的 Windows 对侧，逐字都写过）。
///
/// ⇒ 前面加一段**自述头**：`home=` 说目录在不在，`cat=` 说这个壳里到底有没有读的手段。
/// **它只用内建**（`[` / `command -v` / `printf`）—— 外部命令一个都没有的壳里也回得来，
/// 于是「头没回来」本身就是一个确定的读数：那条命令根本没跑起来。
///
/// ⚠ 为什么不改成「本机自己 `read_to_string`」：见 [`local_shell_read`] 头注那条
/// 「一段逻辑、两种表示」。而且**换传输解决不了这个病** —— 远端那条走 ssh，
/// `connect_and_exec_cmd` 的 `into_stream()` 连 stderr 与 exit-status 都丢掉
/// （`ssh_source::RemoteExec` 头注逐字记着「远端命令失败时它读到 0 行，
/// 与『查询成功但结果为空』**在类型上不可区分**」）⇒ 两条路都只剩 stdout 可用。
/// **把答案写进 stdout**，两条传输路就都拿得到，且仍然只有一条命令、一处文件布局知识。
const CC_BUS_CAT_CMD: &str = concat!(
    r#"B="${CC_BUS_HOME:-$HOME/.cc-bus}"; ccb_home=0; [ -d "$B" ] && ccb_home=1; "#,
    r#"ccb_cat=0; command -v cat >/dev/null 2>&1 && ccb_cat=1; "#,
    r#"printf '@@CCMON-CCBUS-HEAD@@ home=%s cat=%s\n' "$ccb_home" "$ccb_cat"; "#,
    r#"cat "$B/agents.tsv" 2>/dev/null; "#,
    r#"printf '\n@@CCMON-CCBUS-SPLIT@@\n'; cat "$B/spawned.tsv" 2>/dev/null; true"#
);

/// 读远端 `agents.tsv` + `spawned.tsv` 的合计上限〔devbench F10b 提成具名常量〕。
const CC_BUS_TSV_CAP: u64 = 32 * 1024 * 1024;

async fn fetch_remote_cc_bus(cfg: &crate::ssh_source::RemoteConfig) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD).await?;
        let mut buf = Vec::new();
        // `+ 1` 的用意见 src/backend/common/fs.rs 那条既有注释：
        // 多读一个字节就能分辨「刚好读满」与「其实还有」，否则超限会**静默截断**
        // ——而截断的 TSV 会被下面的解析当成一份完整清单，最后一行悄悄少掉或变形。
        stream
            .take(CC_BUS_TSV_CAP + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读取远端 ~/.cc-bus 失败: {e}"))?;
        if buf.len() as u64 > CC_BUS_TSV_CAP {
            return Err(format!(
                "远端 ~/.cc-bus 的登记表超过 {CC_BUS_TSV_CAP} 字节上限 —— 拒收，不拿截断的清单当完整的用"
            ));
        }
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(30), read)
        .await
        .map_err(|_| format!("远端 '{}' 读取超时（30s）", cfg.origin_label()))??;
    // 非 UTF-8 不报错：`~/.cc-bus/` 里的目录名实测含各种字节，宽容降级即可。
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// [`CC_BUS_CAT_CMD`] 自述头里的两件事。**两个字段各装一件事，不合并**
/// （合成一个 `bool` 就是把「没装」与「读不了」重新捏回同一形，那正是本件在治的病）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CcBusReadHead {
    /// `$CC_BUS_HOME`（默认 `~/.cc-bus`）这个目录在不在。`false` = **真的没装**，
    /// 是一种合法状态、不是错误（那条命令结尾的 `true` 就是为它留的）。
    pub(crate) home: bool,
    /// 那个壳里到底有没有 `cat`。`false` = **读不到**，下面的两段内容一个字都不算数。
    pub(crate) reader: bool,
}

/// 把自述头从原始输出里摘下来。`None` = **头没有原样回来**（要么那条命令没跑起来，
/// 要么它回了一句我们读不懂的东西）—— 两种都是「这次读不算数」，绝不是「零个 agent」。
///
/// ⚠ 用 `find` 而不是「必须在第 0 字节」：ssh 那条路上，对端 rc 里一句 `echo` 就会
/// 在前面垫几行。垫的东西连同头一起丢掉，反而顺手把那类噪声挡在解析器外面。
/// ⚠ 字段读不出来（既不是 `0` 也不是 `1`）⇒ 整个头作废，**不许悄悄当成 `false`**。
pub(crate) fn take_head(raw: &str) -> Option<(CcBusReadHead, &str)> {
    let at = raw.find(CC_BUS_HEAD_MARKER)?;
    let after = &raw[at + CC_BUS_HEAD_MARKER.len()..];
    let (line, rest) = after.split_once('\n').unwrap_or((after, ""));
    let flag = |key: &str| -> Option<bool> {
        match line.split_whitespace().find_map(|w| w.strip_prefix(key))? {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    };
    Some((
        CcBusReadHead {
            home: flag("home=")?,
            reader: flag("cat=")?,
        },
        rest,
    ))
}

/// 🔴 **本件的正题：把「读不到」与「真的没有」分开。**
///
/// 两条传输路（本机 `bash -lc` / 远端 ssh）读回来的都是同一条命令的 stdout，
/// 所以解释也**只有这一处** —— 那正是 `read_cc_bus_state` 头注那句「本机跑同一条
/// `CC_BUS_CAT_CMD`，只是不包进 ssh」在结果这一侧的落点：一条命令、一处解释、
/// 两侧口径不可能漂。
///
/// | 出口 | 意思 |
/// |---|---|
/// | `Err`（头没回来） | 那条命令没跑起来 —— **不知道**有没有 agent |
/// | `Err`（`cat=0`） | 那个壳里没有读的手段 —— **不知道** |
/// | `Err`（没有分隔标记） | 只回来半份 —— **不知道**后半截有什么 |
/// | `Ok`（`home=0`） | cc-bus **真的没装** —— 确实零个 |
/// | `Ok`（`home=1`） | 装着、读到了 —— 读出几个就是几个 |
///
/// ⚠ **今天到不了 UI 的那一格**：`home=0`（没装）与「装着但表是空的」，
/// 前端拿到的都是 `agents: []`，于是那句文案只能写成「没有登记过的 agent（**或**未装
/// cc-bus）」——那个「或」就是这一格还没分开的证据。分开它要给 [`CcBusState`] 加一个
/// 字段并重新生成 `src/generated/`（C05），**不在本件写区内** ⇒ 明写在这里，别当它已经做了。
fn interpret_cc_bus_read(origin: &str, raw: &str) -> Result<CcBusState, String> {
    let Some((head, body)) = take_head(raw) else {
        return Err(format!(
            "读不到 '{origin}' 的 cc-bus 登记表 —— 那条读命令**没有跑起来**\
             （自述头 `{CC_BUS_HEAD_MARKER} home=<0|1> cat=<0|1>` 没有原样回来）。\n\
             ⚠ 这**不是**「一个 agent 都没有」：清单根本没读到，有没有是**不知道**。\n\
             多半是那台机器上的 `bash` 跑不了这段语法（Windows 上 \
             `C:\\Windows\\System32\\bash.exe` 那个没装发行版的 WSL 存根就是这一形），\
             或者登录 shell 中途就退了。"
        ));
    };
    if !head.reader {
        let dir = if head.home { "**在**" } else { "不在" };
        return Err(format!(
            "读不到 '{origin}' 的 cc-bus 登记表 —— 那个壳里**没有 `cat`**\
             （自述头逐字报的 `cat=0`），两张表一个字节都读不出来。\n\
             ⚠ 这**不是**「一个 agent 都没有」：`~/.cc-bus` 这个目录{dir}，\
             有没有 agent 是**不知道**。\n\
             多半是登录 shell 的 `PATH` 里没有 `/usr/bin`（`bash -lc` 会先过 `/etc/profile`）。"
        ));
    }
    if !body.contains(CC_BUS_SPLIT_MARKER) {
        return Err(format!(
            "'{origin}' 的 cc-bus 登记表只回来了半份 —— \
             分隔标记 `{CC_BUS_SPLIT_MARKER}` 没出现。\n\
             ⚠ 打标记的 `printf` 是 shell 内建、无条件跑，所以它不在只可能是\
             「命令跑到一半断了」（流被掐 / 输出被截）。\n\
             半份清单会被当完整的用，⇒ 拒收。"
        ));
    }
    let (a, s) = split_combined(body, CC_BUS_SPLIT_MARKER);
    let (agents, sk1) = parse_agents_tsv(a);
    let (spawned, sk2) = parse_spawned_tsv(s);
    if !head.home {
        tracing::info!(
            "'{origin}' 上没有 cc-bus 的状态目录（自述头 home=0）—— 当作「没装」，不是读不到"
        );
    }
    Ok(CcBusState {
        agents,
        spawned,
        skipped: sk1 + sk2,
    })
}

/// B03 批一：读远端 cc-bus 的**登记态**。**只读**，不写任何远端文件。
///
/// **注意语义**：返回的是「登记过什么」，**不是**「谁还活着」。`agents.tsv` 里最早的条目
/// 实测是 10 天前的（进程早没了）。判在线要另查 `tmux has-session`，那是**第二次往返**，
/// 放在用户点某一行的「检查」上，不在这里默认全量查（见 features/B03-*.md §三）。
///
/// ⚠ **解析不在这里做**，一律经 [`interpret_cc_bus_read`]：直接在这里 `split_combined` +
/// 两个 `parse_*` 会绕过「先证明这次真的读到了」那一步，而绕过之后的症状恰恰是**全绿**
/// （空输入解析出零个 agent，一条断言都不会红）。`the_cockpit_never_renders_an_unread_roster_as_empty`
/// 按位置钉着这一条。
#[tauri::command]
pub async fn read_cc_bus_state(origin: String) -> Result<CcBusState, String> {
    // P4a-Y1：本机跑同一条 `CC_BUS_CAT_CMD`，只是不包进 ssh。
    let raw = if origin == crate::inbound_client::LOCAL_ORIGIN {
        local_shell_read(
            CC_BUS_CAT_CMD,
            CC_BUS_TSV_CAP,
            30,
            "读 ~/.cc-bus",
            // 读的是**数据**（清单），半份会被当完整的用 —— 与远端那条同档。
            OnOverflow::Reject,
        )
        .await?
    } else {
        let cfg = crate::load_remote_config_by_label(&origin)
            .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
        fetch_remote_cc_bus(&cfg).await?
    };
    tokio::task::spawn_blocking(move || interpret_cc_bus_read(&origin, &raw))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

/// B03 批一：查**单个** agent 是否真在线（问 `bus-list` 那份**核过身份**的三态）。
///
/// **这是刻意的第二次往返**：`agents.tsv` 只证明"登记过"，实测最早的条目是 10 天前的
/// （进程早没了）。若在读状态时就顺带全量查在线，一屏 37 个 agent 就是 37 次 tmux 调用
/// ——所以只在用户点某一行的「检查」时查那一行。**不默认全量查、不轮询**（红线）。
///
/// # 🔴 `K-R112`（09-13）：**回落删了 —— 删掉的不是那道保护，是那条第二实现**
///
/// `P4f`（08-13）把主路切到 `bus-list` 时留了一条回落：答不上就退回老探法
/// （`tmux has-session -t '=<id>:'` 拼一条 shell 串，本机交给 bash、远端包进 ssh）。
/// 那条老探法**纯按名字**，而名字会被重用 —— 它正是 `P4f` 当天要治的病
/// （敲门打进陌生人屏幕 · 「收掉 agent」杀了无辜进程）。
/// ⇒ **回落回去等于把病请回来**，只是这一次是在「主路答不上」的时候悄悄请回来，
/// 而那正是最难被人发现的时候。
///
/// ⚠ **代价如实写**：通道不在 / daemon 太旧 / 问不到身份空间时，这盏灯从
/// 「悄悄退回按名字探一次」变成**明确的一句「问不到」**。那句话由 [`unknown_liveness`]
/// 独家造 —— 「问不到」**不许**被渲染成「不在线」（灭灯是一个确定的答案，而我们并不确定），
/// 而删掉回落之后这条性质是**结构性**的：这条路上根本没有地方能造出「不在线」这个答案。
///
/// ⚠ `origin` 是入参不是分支（同 [`send_via_daemon`]）：`client_for` 两侧都答得出。
#[tauri::command]
pub async fn check_cc_bus_agent_online(origin: String, id: String) -> Result<bool, String> {
    online_via_daemon(&origin, &id).await
}

// ===== B03 批二：命令构造抽成**纯函数**，让校验落在可测的地方 =====
//
// 为什么要抽：变异测试实测发现，把 `cc_bus_send` 里那句 id 校验整个删掉，测试**照样全绿**
// ——断言测的是 `is_valid_bus_id` 这个**谓词本身**，而不是"命令构造真的调了它"
// （失效模式③：门禁太窄，断言没覆盖使用处）。这几个 async 命令要 SSH 连接、没法单测，
// 于是把「校验 + 拼串」这段纯逻辑摘出来：async 那层只管连接与读回，构造与校验在这里，
// 测试直接打这里。删掉任何一处校验，对应测试立刻红。

// ★★ `K-R112`（09-13）：**这里原来住着查在线那条老探法的命令构造器** `build_online_cmd`。〔散文墓碑〕
//
// 它拼的是 `tmux has-session -t '=<id>:'` —— **纯按名字**，而名字会被重用。
// `P4f`（08-13）已经把主路切到 `bus-list`（登记 + 身份核过的三态），它只剩「主路答不上」
// 那条回落；本件把回落删了 ⇒ 它成为**零生产调用点的死代码**。
//
// **为什么整块走而不是留个壳**：这个函数自己的历史就是答案 —— 它当初正因为
// 「抽成纯函数、判据打在它身上、而生产调用点内联复制了一份」而成为死代码（B03 阻塞-2），
// 留一个没人调的构造器就是把那一幕请回来。同 `K-R72` 给 `build_guarded_tmux_cmd` 记的那一笔：
// 「把回落改成恒失败的桩留在原地 —— 那不是删，那是把一份实现变成一句谎话」。
//
// **那道 id 白名单没有丢，换了住址**：搬进 [`online_via_daemon`]（同 `K-R98` 给发消息那条的手法）。

/// 读某个 agent 的 inbox。只取尾部 200 行：inbox 是只增文件，全量读会随时间越来越慢，
/// 而驾驶舱只看最近的。
fn build_inbox_cmd(id: &str) -> Result<String, String> {
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝拼入命令）: {id:?}"));
    }
    Ok(format!(
        "B=\"${{CC_BUS_HOME:-$HOME/.cc-bus}}\"; tail -n 200 \"$B/inbox/{id}.jsonl\" 2>/dev/null; true"
    ))
}

/// 图形化 spawn = 远端跑**收编后的** `cc-spawn`（它内部已改经 `ccm`）。
/// **刻意不在 cc-monitor 侧重写起会话**——那正是本工作区消灭的病（账本 K8）。
/// `tool` 走白名单（是枚举不是引用）；`dir`/`task` 是自由文本 → 引用。
///
/// `account`：`Some(名)` → 转发 `--account <名>`；`None` → 转发 `--base`（**显式不注入**）。
/// **刻意不提供"什么都不传"这一档**（L2 / B03 审计重要-5）：不传的话 ccm 会落 manifest 的
/// 默认号，于是从驾驶舱点两下就在默认账号上起真 agent 烧额度，而用户既没选过也不知道用了
/// 哪个号。让调用方**必须表态**——选一个号，或显式说"就用基座"。
// ★★ `K-R112`（09-13）：**这里原来住着广播那条回落的命令构造器** `build_broadcast_cmd`。〔散文墓碑〕
//
// 它拼的是 `cc-broadcast <文本>`，而那条脚本发给 `agents.tsv` 的**每一行** ——
// 用户机器上实测 86 行登记、只有 8 个会话还活着。`P4f` 把主路换成
// 「`bus-list` 挑人 + 逐个 `bus-send`」正是为了不再打进 78 个幽灵收件箱；
// 本件删掉回落之后它零生产调用点 ⇒ 整块走（理由同上一块墓碑）。
//
// **空消息那道校验没有丢，换了住址**：`send_via_daemon` 里那句 `text.trim().is_empty()`
// 是每一条真发出去的消息都要过的那一份（广播逐个调它）。

// ★★ `K-R112`（09-13）：**这里原来住着收掉那条 SSH 主路的命令构造器** `build_kill_cmd`。〔散文墓碑〕
//
// 它拼的是 `cc-kill <id>`。与上面两块不同的是：**这一条走的不是回落，是主路** ——
// 收掉 agent 在本件之前从头到尾就是一条 SSH shell 串，而 daemon 侧的 `bus-kill` 早就在。
// 改走原语换来的两样东西写在 [`kill_via_daemon`] 头注里（`<local>` 通了 · 回值三态分得开）。
//
// **那道 id 白名单没有丢，换了住址**：搬进 [`kill_via_daemon`]，理由在那里写得更硬
// —— 这一条的后果是杀掉一棵进程树。

fn build_spawn_cmd(
    tool: &str,
    dir: &str,
    task: &str,
    account: Option<&str>,
) -> Result<String, String> {
    match tool {
        "claude" | "codex" => {}
        _ => return Err(format!("未知 tool: {tool}（支持 claude|codex）")),
    }
    if dir.trim().is_empty() {
        return Err("工作目录为空".to_string());
    }
    // 账号名会作为 `--account` 的值拼进命令。它来自 manifest（由 cc-monitor 自己维护），
    // 但仍过一遍字符集——**不因为"这是我们自己的数据"就免检**（B03 审计的 `--help` 教训：
    // 盘上真会出现没人预料的 id）。
    if let Some(a) = account {
        if a.is_empty()
            || a.starts_with('-')
            || !a
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!("非法账号名（拒绝拼入命令）: {a:?}"));
        }
    }
    // **`--` 不能省**（B03 审计建议）：`cc-spawn` 的旗标循环（`--new`/`--tool`/`--`）跑在
    // 取位置参数**之前**，所以 `dir` 若是 `--new` 这类词会被它自己吃成旗标，然后把任务文本
    // 当成目录，报出"目录不存在: 分析架构"这种莫名其妙的错。这与我给 id 加前导 `-` 校验的
    // 理由逐字同源，只是当时没施加到 dir 上。用 `--` 显式结束选项即可，不必再加白名单。
    let acct_flag = match account {
        Some(a) => format!(" --account {a}"),
        None => " --base".to_string(),
    };
    let mut cmd = format!(
        "cc-spawn --tool {tool}{acct_flag} -- {}",
        crate::ssh_source::shell_quote(dir)
    );
    if !task.trim().is_empty() {
        cmd.push(' ');
        cmd.push_str(&crate::ssh_source::shell_quote(task));
    }
    cmd.push_str(" 2>&1");
    Ok(cmd)
}

/// inbox 里的一条消息（字段取自盘上真实 jsonl：id/from/to/ts/text/class/…）。
/// 只取渲染要用的四个——多取一个字段就多一处要跟着 cc-bus 演进的耦合。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct CcBusMessage {
    pub from: String,
    pub ts: String,
    pub text: String,
    pub class: String,
}

/// 解析 inbox 的 jsonl。**同 TSV 那两个解析器的契约**：坏行跳过并计数，不抛、不因坏行
/// 丢好行。实测当前 inbox 干净（11 行 0 坏），但 `spawned.tsv` 那 5 条畸形行的教训摆在那里
/// ——"现在干净"不是"以后也干净"，而这层成本只有几行。
pub fn parse_inbox_jsonl(text: &str) -> (Vec<CcBusMessage>, usize) {
    let mut out = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            skipped += 1;
            continue;
        };
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (from, text) = (get("from"), get("text"));
        // from 与 text 全空 = 这行不是一条消息（可能是别的工具写进来的）
        if from.is_empty() && text.is_empty() {
            skipped += 1;
            continue;
        }
        out.push(CcBusMessage {
            from,
            ts: get("ts"),
            text,
            class: get("class"),
        });
    }
    (out, skipped)
}

/// 读某个 agent inbox 的上限〔devbench F10b 提成具名常量〕。
const INBOX_READ_CAP: u64 = 4 * 1024 * 1024;

/// 控制类命令（发消息 / spawn）回显的上限〔devbench F10b 提成具名常量〕。
/// 两处共用一个数：它们回的都是「一句人读的确认」，不是数据。
const CONTROL_REPLY_CAP: u64 = 64 * 1024;

/// 三条命令共用的「连上去、跑、读回」。抽出来是因为它们的超时/上限/措辞各不相同，
/// 而**连接与读取的形状必须一致**（同 `mcp.rs` 的既有纪律）。
/// 超限怎么办。**两档的分界是「截断有没有毒」，不是「哪个更严格」**〔G 审计逼出来的〕。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnOverflow {
    /// 读的是**数据**，半份会被当完整的用 ⇒ 拒收+回错。
    Reject,
    /// 读的是**回显**（一句确认），或调用方本来就按「坏行跳过」处理 ⇒ 截断+说清。
    ///
    /// ★ 为什么必须有这一档：`cc_bus_send` / `cc_bus_spawn` 的命令**有副作用** ——
    /// 远端 agent 已经起来了、消息已经投递了，此时因为回显太长回一个 Err，
    /// 用户看到「失败」会重试 ⇒ **起两个 agent 在真烧额度**。
    /// 截断一句确认从来不影响副作用是否发生。
    Truncate,
}

async fn exec_read(
    cfg: &crate::ssh_source::RemoteConfig,
    cmd: &str,
    cap: u64,
    secs: u64,
    what: &str,
    on_overflow: OnOverflow,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut overflowed = false;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, cmd).await?;
        let mut buf = Vec::new();
        let mut overflowed = false;
        // `+ 1` 见 src/backend/common/fs.rs：不多读一个字节就分不清
        // 「刚好读满」与「其实还有」，而**分不清就只能静默截断**。
        stream
            .take(cap + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("{what}失败: {e}"))?;
        if buf.len() as u64 > cap {
            match on_overflow {
                OnOverflow::Reject => {
                    return Err(format!(
                        "{what}的输出超过 {cap} 字节上限 —— 拒收，不拿截断的结果当完整的用"
                    ));
                }
                OnOverflow::Truncate => {
                    // 截断到上限，**并说清** —— 定框 E4：静默失败要给身份。
                    buf.truncate(cap as usize);
                    tracing::warn!(
                        "{what}的输出超过 {cap} 字节上限，已截断（副作用已发生，不当失败报）"
                    );
                    overflowed = true;
                }
            }
        }
        Ok::<(Vec<u8>, bool), String>((buf, overflowed))
    };
    let (raw, over) = tokio::time::timeout(std::time::Duration::from_secs(secs), read)
        .await
        .map_err(|_| format!("远端 '{}' {what}超时（{secs}s）", cfg.origin_label()))??;
    overflowed = over;
    let mut out = String::from_utf8_lossy(&raw).into_owned();
    if overflowed {
        // 说给**用户**听，不只写日志：这条串是要显示出去的。
        out.push_str(&format!(
            "\n[cc-monitor] ⚠ 远端输出超过 {cap} 字节上限，以上内容已截断。\
             命令本身已经执行完毕，**不要重试**。"
        ));
    }
    Ok(out)
}

fn cfg_of(origin: &str) -> Result<crate::ssh_source::RemoteConfig, String> {
    // ★ **兜底，不是主路**〔P4a-Y2〕。两个调用方今天都在它之前分了本机
    //   （`read_cc_bus_inbox` 走本机臂直接返回；`cc_bus_spawn` 先过 `refuse_local_write`）。
    //   ⚠〔`K-R112` 09-13〕原文写「三个调用方…`cc_bus_send`/`cc_bus_spawn`」——
    //   发消息那条 `K-R98` 就不经这里了，广播与收掉本件也走掉了 ⇒ 现打是**两个**。
    //
    // 那为什么还要这一条？因为「所有调用方都守规矩」是一个**承诺**，不是结构 ——
    // 下一个人加第四个调用方时，那个承诺对他不可见，而 `<local>` 掉进下面那句的后果是
    // 报「远端 `<local>` 未配置或未启用」：一句与真实原因毫无关系的话
    // （`P4d-Y5` 收口的正是这一族，`local_origin_registry` 按**位置**盯着它）。
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return Err("本机没有「远端配置」这种东西 —— 这条路是远端专属的。\n\
             cc-bus 的读面本机已经通了（走同一条命令串，只是不包进 ssh）；\n\
             写面还没做，归 `P4b`。"
            .to_string());
    }
    crate::load_remote_config_by_label(origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))
}

/// 🔴🔴 **本机那个 `bash` 到底是哪一个** —— 09-10 云端那条红的根因就住在这里〔ccbus-win〕。
///
/// # `Command::new("bash")` 在 Windows 上**未必是你想的那个 bash**
///
/// **机制（三条都有出处）**：
/// 1. Windows 上 `CreateProcess`（`lpApplicationName` 为 NULL 时）的查找顺序里，
///    **32 位系统目录 `C:\Windows\System32` 排在 `PATH` 之前**；Rust 的
///    `std::process` 在 Windows 上刻意照抄了这个顺序 ⇒ **PATH 怎么排都没用**。
/// 2. GitHub 的 windows-2019/2022/2025 镜像**开着 WSL 这个可选功能却不装任何发行版**，
///    于是 `C:\Windows\System32\bash.exe` 这个存根**存在**，跑起来只说
///    "Windows Subsystem for Linux has no installed distributions." 就退
///    （`actions/runner-images` #12646「Fix WSL limbo」逐字，2025-07-23 开的，
///    标着 2019/2022/2025 三个镜像都中）。
/// 3. ⇒ 云端那趟 `bash -lc 'sleep 2.8416'` **起得来**（`local_shell_read` 回的是 `Ok`）、
///    **120.7ms 就回来**、stdout 空 —— 三个读数与「跑的是那个存根」逐条吻合。
///
/// ⚠ **它是产品面的，不只是 CI 的**：cc-monitor v1 就是 Windows 专供。用户机器上
/// · 开了 WSL 没装发行版 ⇒ 与云端同形（本机 cc-bus 读面整个用不了）；
/// · **装了发行版** ⇒ 更坏：`bash -lc` 真的跑起来，但跑在 **WSL 那个文件系统里**，
///   `$HOME/.cc-bus` 指的是 Linux 家目录 ⇒ 自述头回 `home=0`，本文件据此报「没装」，
///   而 Windows 那侧的 `~/.cc-bus` 可能是满的。**这一形本轮没修**。
///
/// ⇒ 修法见 [`resolve_bash`]：**不再按裸名让操作系统替我们猜**。
///
/// ⚠ **只修了这一处，而且这是对的**〔ccbus-win 09-10 第二拍，逐处量过〕。
/// 全仓起 `bash` 的生产点只有三处，而**另外两处在 Windows 上根本不编译**：
/// · `ccm_probe::probe_with` —— 整族带 `#[cfg(not(windows))]`；
/// · `launch.rs::build_local_posix_argv` 那条 `bash -lic` —— 它的下游
///   `launch_local_posix` 在 Windows 上是个 `#[cfg(windows)]` 的诚实拒绝桩
///   （「POSIX 本地拉起不适用于 Windows 宿主」），argv 永远走不到 spawn。
/// ⇒ 给它们加 Windows 定位是**给一条不存在的路铺砖**；而在 POSIX 上裸名恰恰是**对的**
///   （`execvp` 只查 `PATH`，没有「系统目录优先于 `PATH`」这一条）。**逐处判，别一刀切。**
///
/// ⚠ **本轮仍然没修的那一形**：用户机器上 WSL **装了发行版**时，`bash` 若被指到 WSL，
/// 它跑在**另一个文件系统**里，`$HOME/.cc-bus` 是 Linux 家目录而不是这台机器的。
/// 本处的候选表只列 Git for Windows / MSYS2 的绝对路径、并显式挡掉系统目录那个门牌
/// ⇒ **默认不会**选中 WSL；但用户把 [`BASH_OVERRIDE_VAR`] 指过去仍然做得到，
/// 那时我们只挡得住存根（路径在系统目录里），挡不住一个装好发行版的真 WSL 入口。
/// **解锁条件**：要有一台真 Windows 去量「WSL 里的 `$HOME` 长什么样」，本轮宿主是 Linux。
///
/// # 本常量：逃生口 —— 用户显式指定要用哪个 `bash`
///
/// **为什么非有不可**：候选表覆盖不到的装法总会有（scoop / 自己编的 / 装在别的盘）。
/// 「找不到就响亮失败」若没有逃生口，就从「诚实」变成了「装了也用不了」。
const BASH_OVERRIDE_VAR: &str = "CC_MONITOR_BASH";

/// Windows 上 `bash` 的候选位置 —— **顺序即优先级，全是绝对路径，一个裸名都没有**。
///
/// ⚠ 拼接刻意用 `OsString::push` 拼字面的 `\`，不用 `PathBuf::push`：后者在 Linux 上
/// 会把 `Git\bin\bash.exe` 当成**一个**文件名（反斜杠在 POSIX 语义里不是分隔符），
/// 于是同一份代码在两个平台上拼出**不同的串**，判据在 Linux 上就量不到真的那一份。
/// 本函数只在 Windows 上有生产人群，写死 `\` 是**对的**，而且它让判据两边都跑得到。
///
/// ⚠ `C:\Program Files\Git\bin\bash.exe` 这条是**有读数的**：09-10 那趟云端 run 的日志里，
/// runner 自己给每个 bash 步骤的 shell 逐字就是 `C:\Program Files\Git\bin\bash.EXE`。
/// `C:\msys64\usr\bin\bash.exe` 那条是**推的**（windows-2025 镜像预装 MSYS2 但不进 PATH，
/// 装在哪没核过）—— 排在最后，找不到也只是少一条候选。
fn windows_bash_candidates(
    env: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    const FROM_ENV: &[(&str, &str)] = &[
        ("ProgramFiles", r"Git\bin\bash.exe"),
        ("ProgramW6432", r"Git\bin\bash.exe"),
        ("ProgramFiles(x86)", r"Git\bin\bash.exe"),
        ("LOCALAPPDATA", r"Programs\Git\bin\bash.exe"),
    ];
    const FIXED: &[&str] = &[
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\msys64\usr\bin\bash.exe",
    ];
    // ⚠ 去重不是洁癖：`%ProgramFiles%` 展开出来的多半**就是**下面那条写死的，
    //    留着重复会让「找过了哪些」那份清单当着用户的面说两遍同一句话。
    fn add(out: &mut Vec<std::path::PathBuf>, p: std::path::PathBuf) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for &(var, tail) in FROM_ENV {
        if let Some(base) = env(var) {
            let mut p = base;
            p.push("\\");
            p.push(tail);
            add(&mut out, std::path::PathBuf::from(p));
        }
    }
    for fixed in FIXED {
        add(&mut out, std::path::PathBuf::from(*fixed));
    }
    out
}

/// 这条路径是不是系统目录里那个**WSL 存根**。
///
/// 不是「Windows 目录底下的一概不要」，是**逐字挡住 `actions/runner-images` #12646 说的那个**：
/// WSL 功能开着但没装发行版时，`C:\Windows\System32\bash.exe` 是个 placeholder，
/// 跑起来只说 "Windows Subsystem for Linux has no installed distributions." 就退。
/// `Sysnative` / `SysWOW64` 是同一个目录的另外两个门牌，一起挡。
fn is_system_dir_bash(p: &std::path::Path) -> bool {
    let s = p.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
    s.contains(r"\windows\system32\")
        || s.contains(r"\windows\sysnative\")
        || s.contains(r"\windows\syswow64\")
}

/// 一条候选都没命中时说的话。**抽成纯函数**是为了让判据能逐字咬它
/// （错误文案本身就是本件要买的东西：它必须说得出「这不是没有 agent」）。
fn no_bash_error(tried: &[std::path::PathBuf]) -> String {
    let list = tried
        .iter()
        .map(|p| format!("  · {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "这台机器上找不到可用的 `bash` —— cc-bus 的本机读面**跑不起来**。\n\
         ⚠ 这**不是**「一个 agent 都没有」：清单根本没去读，有没有是**不知道**。\n\
         已经找过（顺序即优先级）：\n{list}\n\
         ⚠ **刻意不去 `PATH` 上碰运气**：Windows 的进程创建把 `C:\\Windows\\System32`\
         排在 `PATH` 之前，而 WSL 功能开着却没装发行版时那里有一个 `bash.exe` 存根 ——\
         按裸名找到的多半正是它，跑起来什么都不做就退（那正是本件的病根）。\n\
         装一份 Git for Windows，或把 `{BASH_OVERRIDE_VAR}` 指向你要用的那个 `bash.exe`。"
    )
}

/// 本机那个 `bash` 到底是哪一个 —— **纯函数半**：平台 / 环境 / 盘上有没有，三样全是入参。
///
/// # 为什么平台要当参数传，而不是在函数体里写 `cfg!(windows)`
///
/// 本仓对这件事有逐字的先例：`history.rs::platform_is_windows` 的头注写着
/// 「`cfg!(windows)` 写在调用点上时它是个**常量表达式**，判据没有任何办法让它变」，
/// 那次实测的刀是「`cfg!(windows)` → `false`」⇒ **全绿**，而生产后果是渲染整个失效。
/// ⇒ 收成入参之后它成了**可翻的一维**：Linux 上也量得到 Windows 那一侧的判断。
/// 同理 `exists` 也是入参 —— 否则「找不到」这一形要靠跑测试的机器上恰好没有 Git 才试得出来。
///
/// ⚠ 生产取值口是 [`resolve_bash`]，它把这三样接到真的上面（平台那格**复用**
/// `history::platform_is_windows`，不在这里写第二份 `cfg!`）。
fn resolve_bash_with(
    windows: bool,
    env: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    exists: &dyn Fn(&std::path::Path) -> bool,
) -> Result<std::ffi::OsString, String> {
    if let Some(raw) = env(BASH_OVERRIDE_VAR) {
        let p = std::path::PathBuf::from(&raw);
        if is_system_dir_bash(&p) {
            return Err(format!(
                "`{BASH_OVERRIDE_VAR}` 指的是系统目录里那个 `bash.exe` —— 那是 WSL 的存根，\
                 没装发行版时它什么都不做就退，装了发行版则跑在**另一个文件系统**里\
                 （`$HOME` 是 Linux 家目录，不是这台机器的）。**拒绝用它。**\n\
                 实得：{}",
                p.display()
            ));
        }
        // **不回落到候选表**：用户显式指了一个路径却指错，回落会让他以为自己那条生效了。
        if !exists(&p) {
            return Err(format!(
                "`{BASH_OVERRIDE_VAR}` 指的路径不存在：{}\n\
                 ⚠ 显式指定过就不再去猜 —— 回落到候选表会让你以为自己这条生效了。",
                p.display()
            ));
        }
        return Ok(raw);
    }
    if !windows {
        // POSIX：`execvp` 只查 `PATH`，没有「系统目录优先」那一条 ⇒ 裸名是**对的**，
        // 而且写死 `/bin/bash` 会在 NixOS / Homebrew 这类装法上当场坏掉。
        return Ok("bash".into());
    }
    let tried = windows_bash_candidates(env);
    for c in &tried {
        if !is_system_dir_bash(c) && exists(c) {
            return Ok(c.clone().into_os_string());
        }
    }
    Err(no_bash_error(&tried))
}

/// 生产取值口。**全仓解析 `bash` 只有这一处**，由
/// `the_bash_cc_bus_runs_is_resolved_in_exactly_one_place` 钉住。
fn resolve_bash() -> Result<std::ffi::OsString, String> {
    resolve_bash_with(
        crate::history::platform_is_windows(),
        &|k: &str| std::env::var_os(k),
        &|p: &std::path::Path| p.exists(),
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// 🪦〔散文墓碑〕 `KillTreeGuard` / `reap_whole_tree_on_drop` **搬走了**〔`15 §5.1 A3`，09-18〕
//
// 那一份（Job Object ＋ `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，注释逐字「这就是本件
// 要买的那一下」，还记了 `AssignProcessToJobObject` 的竞态）是设计稿点名的「仓里已经
// 有的正确做法」之一 ⇒ 它现在是 `spawn_managed::Lifetime::JobKillOnClose` 的实现，
// 住 `src/bridge/src/spawn_managed.rs`（连同那份「它买不到什么」的边界一起搬过去）。
//
// ⚠ **这里不留第二份**：本文件今天声明 `Lifetime::JobKillOnClose`，
// 由那个唯一出口去建 Job —— 而**云端读数（run `34473562660` 的 survivors 清单）
// 与 `a_timed_out_local_read_does_not_leave_an_orphan_behind` 那条行为判据一个字没动**，
// 它们数的仍是同一件事：超时之后还剩不剩下子进程。
// ═══════════════════════════════════════════════════════════════════════════

/// P4a-Y1：**本机跑同一条串** —— [`exec_read`] 的孪生兄弟，差别只有「谁来跑它」。
///
/// # 为什么不是在这里重写一遍 cc-bus 的文件布局
///
/// `CC_BUS_CAT_CMD` 逐字知道 `~/.cc-bus/agents.tsv` 长什么样。本机要是自己去 `read_to_string`
/// 那两个文件，仓里就有了**两份**同一件事的表示，而它们会各自漂 ——
/// 这个仓管这叫「一段逻辑、两种表示」，旧 `ccm` 的 `resolve_from_daemon` 〔散文墓碑〕/`resolve_recipe`
/// 那对孪生函数专门为此立了一条 e2e 对拍。
///
/// ⇒ 照 `P3t-Y2` 的先例办（`exec_site_registry` 逐字记着「起本机探针与它**共用同一个常量**」）：
/// **同一条命令串，远端包进 ssh，本机交给 `bash -lc`。** 这就是 `C1`〔用 08-11〕
/// 「本地要和远端一样，只是远端走 ssh，本地不走」在这一族上的落点。
///
/// ⚠ **这条论证管的是「跑什么」，不管「用谁跑」**〔ccbus-win 09-10 第二拍〕。
/// 「一条命令串两条传输路」买到的是**一处文件布局知识**；它从来没有说过
/// 「那个 `bash` 可以按裸名让操作系统替我们猜」。后者是 [`resolve_bash`] 的活。
///
/// # 三件套一件都不许少
///
/// 远端那条有**上限 + 超时 + 溢出处置**。本机看着「自家文件、能出什么事」——
/// 但 `agents.tsv` 是只增文件、`inbox` 更是，而 monitor 与它跑在同一台机器上，
/// 撑爆的是**用户正在用的那个进程**。⇒ 逐件对称，由
/// `the_local_cc_bus_read_keeps_every_guard_the_remote_one_has` 钉住。
///
/// ⚠ 用 `bash -lc` 而不是 `-lic`：这里只要 `$HOME` / `$CC_BUS_HOME`，不需要交互式 rc
/// （`ccm_probe` 那条要 `-lic` 是因为它得到用户 PATH 里找 `ccm`，需求不同，别互抄）。
///
/// ⚠ 解析放在**超时之外**：解析失败是「这台机器上没有 bash」，
/// 把它算进那 30 秒里、再报成「超时」是又一次拿错误的名字说话。
///
/// ⚠ **收尾靠 `Lifetime::JobKillOnClose`，不是只靠 `kill_on_drop`**〔第七拍〕：
/// Windows 上 `Git\bin\bash.exe` 是外壳，`kill_on_drop` 只杀得到它。
/// 〔`15 §5.1 A3` 09-18：那份 Job 的建法搬进 `spawn_managed` 了，做的事一个字没变。〕
async fn local_shell_read(
    cmd: &str,
    cap: u64,
    secs: u64,
    what: &str,
    on_overflow: OnOverflow,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let shell = resolve_bash()?;
    let read = async {
        let mut c = tokio::process::Command::new(&shell);
        c.args(["-lc", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped());
        // ★★ 三条策略（`00 §1.5.2`）—— **超时那条路全靠中间那一条**〔D 阶段补审 08-12〕。
        //
        // · `Hidden`：Windows 上这一跳起的是 `Git\bin\bash.exe`（控制台子系统），
        //   不带 `CREATE_NO_WINDOW` 就是每读一次 cc-bus 闪一个黑框。
        //   🔴 **这一格先前没人回答过** —— 走唯一出口之后它才被逼着说了一句。
        // · `JobKillOnClose`：先前的 `kill_on_drop(true)` ＋ 那份 Job 收尾凭据
        //   两句现在是**同一条策略**。下面那句显式 `start_kill()` 只在**成功路径**上；
        //   超时是 `tokio::time::timeout` 把整个 future 丢掉，走不到那里，而 tokio 的
        //   `Child` **默认不因句柄被 drop 而杀子进程** ⇒ 每超时一次漏一个 `sleep`/`cat`。
        //   本会话已经因为「我自己留下的孤儿进程」栽过一次：`#60` 的八轮实测里，
        //   有五个孤儿 daemon 把读数全带偏了，我却先后猜了七个错误的病因。
        //   ⇒ 教训的产物不是「以后小心」，是
        //   `a_timed_out_local_read_does_not_leave_an_orphan_behind` 那条判据。
        // · `Null`：这一跳唯一有用的字节在 stdout 上；rc/shell 的抱怨不是我们的诊断。
        //
        // ⚠ **析构序仍然是「先关 Job、再走句柄」** —— 它现在由 `ManagedTokioChild`
        //   的字段声明序保证（那边头注写着为什么），不再靠这里两个局部变量的先后。
        let mut child = crate::spawn_managed::spawn_managed_tokio(
            &mut c,
            crate::spawn_managed::ConsolePolicy::Hidden,
            crate::spawn_managed::Lifetime::JobKillOnClose,
            crate::spawn_managed::StderrSink::Null,
        )
        .map_err(|e| format!("本机{what}失败（起不了 bash）: {e}"))?;
        let mut out = child
            .stdout
            .take()
            .ok_or_else(|| format!("本机{what}失败：拿不到 stdout"))?;
        let mut buf = Vec::new();
        // `+ 1` 的用意同远端那条：不多读一个字节就分不清「刚好读满」与「其实还有」，
        // 而分不清就只能静默截断。
        (&mut out)
            .take(cap + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("本机{what}失败: {e}"))?;
        // 读够了就别再等它 —— 否则 `cat` 一个超大文件时我们会陪它跑完。
        let _ = child.start_kill();
        let _ = child.wait().await;
        if buf.len() as u64 > cap {
            match on_overflow {
                OnOverflow::Reject => {
                    return Err(format!(
                        "本机{what}的输出超过 {cap} 字节上限 —— 拒收，不拿截断的当完整的用"
                    ));
                }
                OnOverflow::Truncate => {
                    buf.truncate(cap as usize);
                    tracing::warn!("本机{what}的输出超过 {cap} 字节，已截断");
                }
            }
        }
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(secs), read)
        .await
        .map_err(|_| format!("本机{what}超时（{secs}s）"))??;
    // 非 UTF-8 不报错：理由同远端那条（`~/.cc-bus/` 里的目录名实测含各种字节）。
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// 写面对 `<local>` 的诚实拒绝〔P4a-Y2〕。
///
/// 不加这一条的话，`<local>` 会掉进 `cfg_of`，报「远端 `<local>` 未配置或未启用」——
/// 又一句与真实原因毫无关系的话（`P4d-Y5` 收口的正是这一族）。
///
/// ⚠〔`K-R112` 09-13〕**今天只剩一个调用方**：`cc_bus_spawn`。
/// 发消息（`P4f`）· 广播（`P4f`）· 收掉（本件）三条都已改走 daemon 原语，
/// 它们的 `origin` 是入参不是分支 ⇒ 没有「本机走不到」这回事了。
/// **留着这一份不是为了对称**：spawn 那条今天真的没有对侧（daemon 帧面 10 条里没有 spawn），
/// 而这句话要说清「缺的是后端能力」，不是「配置没找到」。
fn refuse_local_write(origin: &str, what: &str) -> Option<String> {
    if origin != crate::inbound_client::LOCAL_ORIGIN {
        return None;
    }
    Some(format!(
        "本机还不能{what}：cc-bus 的这一面在本机没有对侧（daemon 侧没有它的原语）。\n\
         读面（清单 / 在线 / inbox）与写面里的发消息 / 广播 / 收掉今天都通了；\n\
         剩下这一条等的是**后端先长出那条原语**，不是等谁记得接线。"
    ))
}

/// 从 `bus-list` 的回值里取某个 id 的在线状态 —— **纯函数**。
///
/// `None` 有两种来源，**它们都不是"不在线"**：
/// · 这个 id 不在总线名单里（那它根本不是 agent）；
/// · `live` 是 `null`（daemon 问不到身份空间）。
/// ⇒ 调用方拿到 `None` 时**诚实报「问不到」**（[`unknown_liveness`]），而不是渲染成一盏灭灯。
///
/// ⚠〔`K-R112` 09-13〕原文这一行写的是「**回落**到老探法」—— 那半句今天假了：
/// 老探法（按名字 `tmux has-session`）连同它那条 shell 路一起删净了。
/// **保住的是「不许灭灯」那半，删掉的是「按名字再探一次」那半** —— 两半别混。
pub(crate) fn live_of(agents: &[serde_json::Value], id: &str) -> Option<bool> {
    agents
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(id))
        .and_then(|a| a.get("live"))
        .and_then(|v| v.as_bool())
}

/// 「在不在线**问不到**」的唯一造句处 —— 纯函数〔`K-R112` 09-13〕。
///
/// 🔴 它存在的全部意义是一句话：**「问不到」不是「不在线」。**
/// 删掉老探法回落之后，这条路上每一种答不上都从这里出口 ⇒
/// 「灭灯」这个答案在结构上**没有地方可以被造出来**，而不是靠谁记得别灭灯。
pub(crate) fn unknown_liveness(origin: &str, id: &str, why: &str) -> String {
    format!(
        "{}上问不到 `{id}` 在不在线：{why}\n\
         ⚠ 这是**问不到**，不是**不在线** —— 别把它读成一盏灭灯。",
        machine_label(origin)
    )
}

/// 问 daemon「这个 agent 在线吗」。**没有第二条路**〔`K-R112` 09-13〕。
///
/// `Err` 的五种来源逐条经 [`unknown_liveness`]：没通道 · daemon 太旧 · 那一趟失败 ·
/// 应答形状不认识 · 它不在名单里 /`live` 是 null。**一种都不是「不在线」。**
async fn online_via_daemon(origin: &str, id: &str) -> Result<bool, String> {
    // ⚠ **校验留在这一侧**（同 [`send_via_daemon`]）：id 今天不再被拼进任何命令串，
    //   但「调用方不能靠对端校验」这条规矩不因为注入面没了就作废。
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝去问它在不在线）: {id:?}"));
    }
    let unknown = |why: String| unknown_liveness(origin, id, &why);
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return Err(unknown(
            "这台的 daemon 通道没起来（设置里可以起/停每台机器的 daemon）".to_string(),
        ));
    };
    // 能力协商放在问之前 —— 同 `send_via_daemon` 那条理由：「这台的后端太旧」
    // 是**问得出答案**的，不许与超时同形。
    if !client.accepts(BUS_LIST) {
        return Err(unknown(format!(
            "这台的 daemon 太旧 —— 它没声明 `{BUS_LIST}` 这条命令\
             （**能力协商**问出来的，不是超时、也不是网络错）"
        )));
    }
    let listed = client
        .call(
            BUS_LIST,
            serde_json::json!({}),
            std::time::Duration::from_secs(15),
        )
        .await
        .map_err(|e| unknown(describe_bus_error("这一趟没问到", &e)))?;
    let agents = listed
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            unknown(format!(
                "`{BUS_LIST}` 的应答里没有 `agents` 数组 —— 这一端与那一端的契约漂开了"
            ))
        })?;
    live_of(agents, id).ok_or_else(|| {
        unknown("它不在总线名单里，或者 daemon 问不到身份空间（`live` 是 null）".to_string())
    })
}

/// 广播走 daemon 那条路的两种失败：能不能回落到老路。
///
/// 与 `daemon_route::Routed` 同一条纪律：**只有能证明"一条都没发出去"时才允许回落**。
pub(crate) enum BroadcastRoute {
    /// 一条都没发出去（没通道 / daemon 太旧）⇒ 远端可以回落。
    NoChannel(String),
    /// 已经发了一部分，或 daemon 明确拒绝 ⇒ **不许回落**（回落会把一部分人收到两遍）。
    Failed(String),
}

/// 广播要发给谁 —— **纯函数**（`agents` 是 `bus-list` 的回值）。
///
/// | 情形 | 做法 |
/// |---|---|
/// | 身份空间答得上（有 true/false） | **只发 `live == true` 的** |
/// | 全是 `null`（问不到 tmux） | 退回「发给所有登记的」，并标记 `liveness_unknown` |
///
/// ★ 第二行是刻意的：**「问不到」不等于「都不在」**。若问不到就谁都不发，
/// 用户会看到一次「已广播给 0 个」——那是把不知道渲染成了确定。
pub(crate) fn pick_broadcast_targets(agents: &[serde_json::Value], me: &str) -> BroadcastPlan {
    let known: bool = agents
        .iter()
        .any(|a| a.get("live").map(|v| !v.is_null()).unwrap_or(false));
    let mut targets = Vec::new();
    let mut skipped_offline = 0usize;
    for a in agents {
        let Some(id) = a.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if id == me {
            continue; // 不发给自己（同 cc-broadcast）
        }
        let live = a.get("live").and_then(|v| v.as_bool());
        if known && live != Some(true) {
            skipped_offline += 1;
            continue;
        }
        targets.push(id.to_string());
    }
    BroadcastPlan {
        targets,
        skipped_offline,
        liveness_unknown: !known,
    }
}

pub(crate) struct BroadcastPlan {
    pub(crate) targets: Vec<String>,
    pub(crate) skipped_offline: usize,
    pub(crate) liveness_unknown: bool,
}

/// 广播走 daemon：`bus-list` 挑人 → 逐个 `bus-send`。
async fn broadcast_via_daemon(origin: &str, text: &str) -> Result<String, BroadcastRoute> {
    use crate::inbound_client::client_for;
    let Some(client) = client_for(origin) else {
        return Err(BroadcastRoute::NoChannel(format!(
            "[{origin}] 没有可用的控制通道"
        )));
    };
    let listed = client
        .call(
            BUS_LIST,
            serde_json::json!({}),
            std::time::Duration::from_secs(30),
        )
        .await
        // ⚠ **分流走那唯一的一份**（`daemon_route::route_call_error`）——
        //   我第一版在这儿自己 match 了一遍 `CallError`，守卫当场逮住：
        //   「那是分流规则的第二份实现，它一旦与本模块漂开，一次 `wrong_owner`
        //    就可能被另一条路重做一遍」。逮得对。
        .map_err(|e| {
            match crate::backend::control::daemon_route::route_call_error(&e, |code, message| {
                format!("列总线成员被拒：{code}：{message}")
            }) {
                // 「证明没发出去」⇒ 远端可以回落到老路
                crate::backend::control::daemon_route::Routed::NoChannel(why) => {
                    BroadcastRoute::NoChannel(why)
                }
                crate::backend::control::daemon_route::Routed::Refused(why) => {
                    BroadcastRoute::Failed(why)
                }
                crate::backend::control::daemon_route::Routed::Done => {
                    BroadcastRoute::Failed("分流器判成已完成，这不该发生".into())
                }
            }
        })?;
    let agents = listed
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let plan = pick_broadcast_targets(&agents, MONITOR_BUS_ID);
    let mut ok = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for id in &plan.targets {
        let args = serde_json::json!({ "to": id, "text": text, "from": MONITOR_BUS_ID });
        match client
            .call(BUS_SEND, args, std::time::Duration::from_secs(30))
            .await
        {
            Ok(_) => ok += 1,
            // ⚠ 已经发出去一部分了 ⇒ **不许回落**（回落会让一部分人收到两遍）。
            Err(e) => failed.push(format!("{id}（{e}）")),
        }
    }
    Ok(describe_broadcast(&plan, ok, &failed))
}

/// 广播结果讲成人话 —— 纯函数。
///
/// ★ 三个数**分开说**：发到几个、因为不在线跳过几个、失败几个。
/// 合成一个「已向 N 个 agent 发出广播」正是老路的病 —— 那个 N 把 78 个幽灵也算了进去。
pub(crate) fn describe_broadcast(plan: &BroadcastPlan, ok: usize, failed: &[String]) -> String {
    let mut msg = if plan.liveness_unknown {
        format!("已广播给 {ok} 个**已登记** agent（问不到谁在线，所以全发了）")
    } else {
        format!("已广播给 {ok} 个**在线** agent")
    };
    if plan.skipped_offline > 0 {
        msg.push_str(&format!(
            "；跳过 {} 个不在线的（它们的收件箱今天没人读）",
            plan.skipped_offline
        ));
    }
    if !failed.is_empty() {
        msg.push_str(&format!("；{} 个失败：{}", failed.len(), failed.join("、")));
    }
    msg
}

/// cc-monitor 自己在总线上的身份 —— **发消息时用它，别让收信人看到 `unknown`**。
pub(crate) const MONITOR_BUS_ID: &str = "cc-monitor";

/// daemon 那条发消息原语的名字（`P4f`；实现住 `src/backend` 的 `control/cc_bus.rs`）。
const BUS_SEND: &str = "bus-send";

/// daemon 那条**列总线成员**原语的名字（`P4f`；三态在线就出自它的 `live` 字段）。
///
/// ⚠ 它有两个消费者（查在线 · 广播挑人）—— 名字**只许有一个住址**，别在调用点写字面量。
const BUS_LIST: &str = "bus-list";

/// daemon 那条**收掉 agent** 原语的名字（`P4f` 续 p2c，`BUILD_ID` 里逐字记着那一拍）。
const BUS_KILL: &str = "bus-kill";

/// 一句话里怎么称呼这台机器 —— **纯函数**。
///
/// 🔴 它是「本机与远端两条路可观测行为等价」这条性质的**承重件**〔`KR98D1` 第③刀〕：
/// 两侧的每一句话都由**同一个** `format!` 渲染出来，唯一允许不同的就是这里回的这个称呼。
/// 一旦有人给本机另写一句「更亲切的」话，两条路就从措辞开始漂 ——
/// 而措辞正是用户唯一看得见的那一面。
pub(crate) fn machine_label(origin: &str) -> String {
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        "本机".to_string()
    } else {
        origin.to_string()
    }
}

/// 「这台的 daemon 通道没起来」讲成人话 —— 纯函数，**每条走 daemon 的 cc-bus 命令共用这一份**。
///
/// ⚠〔`K-R112` 09-13〕原来这一份只服务发消息（`what` / `outcome` 两处都写死）。
/// 收掉 agent 改走原语之后要说同一句话 ⇒ **提参数，不抄第二份**：
/// 抄一份的代价不是重复，是两份措辞会各自漂，而措辞正是用户唯一看得见的那一面。
pub(crate) fn describe_no_channel_for(origin: &str, what: &str, outcome: &str) -> String {
    format!(
        "{} 的 daemon 通道没起来 —— {what}要经它（设置里可以起/停每台机器的 daemon）；\
         {outcome}。",
        machine_label(origin)
    )
}

/// 发消息那一档的说法（输出与提参数之前**逐字节相同**）。
pub(crate) fn describe_no_channel(origin: &str, id: &str) -> String {
    describe_no_channel_for(origin, "发消息", &format!("{id} 的消息**没有发出去**"))
}

/// 「这台的 daemon 太旧」讲成人话 —— **能力协商的结论**，纯函数，两条路共用这一份。
///
/// # 🔴 它为什么必须与超时 / 断连长得不一样〔`KR98D2`〕
///
/// 「这台机器的后端没有这条命令」是一件**问得出答案**的事（`hello` 里那张命令表），
/// 而「超时」「连接断了」是**问不出答案**的事。把它们压成同一句「发消息失败」，
/// 就是本工作区最贵的那一形 —— **一个值装了两件事**：用户拿到它既不知道该升级，
/// 也不知道该重试，只能两样都试一遍。
pub(crate) fn describe_daemon_too_old_for(origin: &str, cmd: &str, outcome: &str) -> String {
    format!(
        "{} 的 daemon 太旧：它没声明 `{cmd}` 这条命令（这是**能力协商**问出来的，\
         不是超时、也不是网络错）—— {outcome}；把这台的后端升到新版就能用。",
        machine_label(origin)
    )
}

/// 发消息那一档的说法（输出与提参数之前**逐字节相同**）。
pub(crate) fn describe_daemon_too_old(origin: &str, id: &str) -> String {
    describe_daemon_too_old_for(origin, BUS_SEND, &format!("{id} 的消息**没有发出去**"))
}

/// 发消息那一趟的失败讲成人话 —— 纯函数，两条路共用这一份。
///
/// ⚠ **分流走共用的那一份**（`daemon_route::route_call_error`）：
///   `daemon_route` 的登记表逐字要求「新增一个发送端就必须在这里表态」，
///   而它自己的头注记着为什么 —— 分流规则一旦有第二份实现，
///   「被门拒绝」就会在某一份里被洗成「换条路重做」。
/// ★ 本发送端**没有第二条路可回落**（shell 写面正是 `P4a` 拒掉、`K-R98` 删净的东西）
///   ⇒ 三档结果都只是给用户的一句话，`Routed` 的回落语义在这里是空的。
pub(crate) fn describe_bus_error(outcome: &str, e: &crate::inbound_client::CallError) -> String {
    use crate::backend::control::daemon_route::{route_call_error, Routed};
    match route_call_error(e, |code, message| format!("{code}：{message}")) {
        Routed::NoChannel(why) => format!("{why}（{outcome}）"),
        Routed::Refused(why) => why,
        Routed::Done => format!("{outcome} —— 分流器判成已完成，这不该发生"),
    }
}

/// 发消息那一档的说法。**分流仍然只有一份**〔`K-R112`：提参数之后仍然只有一处 `route_call_error`〕。
pub(crate) fn describe_send_error(id: &str, e: &crate::inbound_client::CallError) -> String {
    describe_bus_error(&format!("{id} 的消息**没有发出去**"), e)
}

/// 发消息：走 daemon 的 `bus-send` 原语（`P4f`）。**本机与远端同一条路**〔`K-R98` 09-13〕。
///
/// # 为什么不是"再拼一条 shell 串"
///
/// 那正是 `C1` 排除的东西（一份语义两处实现）。daemon 那条原语自己带着**六档错误码**
/// 与**三态在线**，这条路只做一件事：把它们讲成人话。
///
/// # 🔴 `origin` 是原语的一个入参，不是一个分支
///
/// 本机与远端**共用这整个函数体**：`client_for(origin)` 两侧都答得出（`<local>` 也是一个
/// origin —— `C1` 逐字「只是远端走 ssh，本地不走」）。⇒ 「同一输入 ⇒ 同一结果形状」
/// **不是**靠两处代码互相照抄维持的，是结构上只有一处可抄。
///
/// ⚠ 老 daemon 没有这条命令 ⇒ 开场先用 `accepts` 问一句（**能力协商，不是超时**）
/// ⇒ 报「这台的 daemon 太旧」，而不是含糊的失败。
async fn send_via_daemon(origin: &str, id: &str, text: &str) -> Result<String, String> {
    use crate::inbound_client::client_for;
    // ⚠ **两道校验留在这一侧**〔`K-R98`〕：id 今天不再被拼进任何命令串（那条 shell 路本件
    //   删净了），但「调用方不能靠对端校验」这条规矩不因为注入面没了就作废 ——
    //   `--help` 这种 id 在盘上真出现过（`~/.cc-bus/inbox/--help.jsonl`，188 字节），
    //   放它过去只会在对面造出一个没人读的收件箱。
    // ★ 它同时是**两条路等价**的一部分：老远端那条走已删的 shell 构造器，两道校验本来就在；
    //   本机那条**没有** ⇒ 同一条空消息在两台机器上是两种结果。收成一处，这个差别才真没了。
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝发给它）: {id:?}"));
    }
    if text.trim().is_empty() {
        return Err("消息为空".to_string());
    }
    let Some(client) = client_for(origin) else {
        return Err(describe_no_channel(origin, id));
    };
    // ⚠ **能力协商放在发之前**：老 daemon 没有这条命令时 `call` 自己也会回一个「没发出去」，
    //   但那一档经分流器出来与「入方向排队满了」同形。这里先问一句，是为了让
    //   **「这台的 daemon 太旧」说得出口** —— `KR98D2` 判的正是它不许与超时/网络错同形。
    //   ★ 分流本身仍然只有一份（下面 `describe_send_error` 里那个 `route_call_error`）：
    //     本行判的是「**发之前**这台机器认不认这条命令」，不是「这次失败该不该回落」。
    if !client.accepts(BUS_SEND) {
        return Err(describe_daemon_too_old(origin, id));
    }
    // ★ **以谁的身份发**〔08-13 实测〕：不给 `from` 的话，daemon 跑 `cc-send` 时不在任何
    //   tmux pane 里，`cc-whoami` 解不出身份 ⇒ 收信人看到「来自 unknown」，
    //   而它给的回复方式是 `cc-send unknown "…"` —— **回复直接掉进没人读的收件箱**。
    // ⚠ 用 `MONITOR_BUS_ID` 这个固定身份：收信人至少知道**这条是从 cc-monitor 发来的**。
    //   ⚠ 回复仍然没有归宿（没人读 `cc-monitor` 的收件箱）—— 那条已记进 ROADMAP `U17`，
    //   不在这一刀里假装解决。
    let args = serde_json::json!({ "to": id, "text": text, "from": MONITOR_BUS_ID });
    match client
        .call(BUS_SEND, args, std::time::Duration::from_secs(30))
        .await
    {
        Ok(reply) => Ok(describe_send_reply(id, reply.as_ref())),
        Err(e) => Err(describe_send_error(id, &e)),
    }
}

/// 把 `bus-send` 的回值讲成人话 —— 纯函数。
///
/// ★ 要紧的是**三态在线**别在这一层被抹平：
/// 「发出去了」和「发出去了但没人会读」对用户是两件事（`P4f §11`）。
pub(crate) fn describe_send_reply(id: &str, reply: Option<&serde_json::Value>) -> String {
    let get = |k: &str| reply.and_then(|r| r.get(k)).cloned();
    let registered = get("registered").and_then(|v| v.as_bool());
    let live = get("live").and_then(|v| v.as_bool());
    match (registered, live) {
        (Some(false), _) => format!(
            "已投递给 {id}，但**这个名字没在总线上登记过** —— 今天没有任何进程会读它的收件箱（名字打错了吗？）"
        ),
        (_, Some(false)) => format!(
            "已投递给 {id}，但**它当前不在线** —— 消息留在收件箱里，它下次起来才会读到"
        ),
        (_, Some(true)) => format!("已投递给 {id}"),
        // daemon 问不到身份空间（没装 tmux 等）⇒ **不假装知道**
        _ => format!("已投递给 {id}（在不在线：问不到）"),
    }
}

/// B03 批二：读某个 agent 的 inbox（**只读**）。
#[tauri::command]
pub async fn read_cc_bus_inbox(origin: String, id: String) -> Result<Vec<CcBusMessage>, String> {
    let cmd = build_inbox_cmd(&id)?;
    // P4a-Y1：本机跑同一条 `build_inbox_cmd` 产出的串（`tail`，零副作用）。
    let raw = if origin == crate::inbound_client::LOCAL_ORIGIN {
        local_shell_read(&cmd, INBOX_READ_CAP, 30, "读 inbox", OnOverflow::Truncate).await?
    } else {
        let cfg = cfg_of(&origin)?;
        exec_read(
            &cfg,
            &cmd,
            INBOX_READ_CAP,
            30,
            "读 inbox",
            OnOverflow::Truncate,
        )
        .await?
    };
    tokio::task::spawn_blocking(move || parse_inbox_jsonl(&raw).0)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))
}

/// B03 批二：给某个 agent 发消息。**这是本模块唯一的写操作**（其余全只读）。
#[tauri::command]
pub async fn cc_bus_send(origin: String, id: String, text: String) -> Result<String, String> {
    // ★★ **本机写面接上了**〔P4f 08-13〕。
    //
    // `refuse_local_write` 的拒绝理由逐字写着「写面归 `P4b`（cc-bus 改调 daemon 原语）——
    // 用户 08-12 已裁『**先把确切的命令组件做出来，然后 cc-bus 可以去调用**』」。
    // **那些命令组件今天做出来了**（`P4f` 的 `bus-send`）⇒ 前提到期，这一条不再拒。
    //
    // ⚠ 其余三条（广播 / kill / spawn）**仍然拒**：daemon 侧没有对应的原语。
    // 拒绝理由是逐条的，不是一句通用话 —— 别把它们一起放行。
    //
    // 🔴🔴 **`K-R98`（09-13）：远端这半也改走原语了。**
    //   在此之前它还在**拼一条 `cc-send …` 的 shell 串走 SSH** —— 而 daemon 侧那条
    //   `bus-send` 早就有（`P4f` 逐字「cc-bus 的基础命令」）⇒ **不是缺能力，是还没改走**。
    //   改走之后这个函数体里**再没有「本机怎么走 / 远端怎么走」这个分支**：
    //   origin 只是原语的一个入参，两侧走的是同一份代码、说的是同一句话。
    //   ⚠ 上面那句「其余三条仍然拒」**一个字都没松** —— 本件放行的是**一条**。
    send_via_daemon(&origin, &id, &text).await
}

/// P4c（#77/#78）：向**所有**已登记 agent 广播一条消息。
///
/// 面板此前只能给**单个**收件人发。⚠ 爆炸半径：实测本机 `agents.tsv` 有 86 行 ——
/// UI 侧的确认必须**带数字**，一个不带数字的「确定吗」等于没问。
#[tauri::command]
pub async fn cc_bus_broadcast(origin: String, text: String) -> Result<String, String> {
    // ★★ **广播是组合，不是原语**〔P4f 08-13〕：列成员（`bus-list`）+ 逐个发（`bus-send`）。
    //
    // 这么做同时修掉一条**实测出来的真事故**：`cc-broadcast` 发给 `agents.tsv` 的**每一行**，
    // 而那份名单会过期 —— 用户机器上实测 **86 行登记、只有 8 个会话还活着**
    // ⇒ 一次广播打进 **78 个没人读的收件箱**，而它报「已向 86 个 agent 发出广播」。
    //
    // 🔴🔴 **`K-R112`（09-13）：远端那条 SSH 回落删了 —— 两侧同一条路。**
    //
    //   在此之前，远端拿不到 daemon 能力时会退回 `cc-broadcast` 那条 shell 串（`C7` 过渡期）。
    //   ⚠ 那条串发给 `agents.tsv` 的**每一行** —— 正是上面那段实测事故（86 行登记 / 8 个活着）
    //   的原样复发，只是改在「主路答不上」的时候悄悄发生，**而那正是最难被发现的时候**。
    //   ⇒ 回落删净：三个数分开说的那份诚实（发到几个 / 因不在线跳过几个 / 失败几个）
    //   不许被一条老路绕过去。
    //
    // ⚠ **代价如实写**（同 `K-R72` 给送键 / 杀会话记的那一笔）：通道不在时，广播从
    //   「换条路悄悄发掉」变成**明确失败**。两侧说的是同一句话 —— 本机此前就没有第二条路，
    //   今天远端也没有了，于是这句话不再分本机 / 远端两种写法。
    match broadcast_via_daemon(&origin, &text).await {
        Ok(msg) => Ok(msg),
        Err(BroadcastRoute::NoChannel(why)) => Err(format!(
            "{why}（没有第二条路可走 —— 广播只走后端这一条；\
             `K-R112` 起没有 SSH 兜底那条了，所以这不是「再试一次」能过去的）"
        )),
        Err(BroadcastRoute::Failed(why)) => Err(why),
    }
}

/// 把 `bus-kill` 的回值讲成人话 —— 纯函数。
///
/// ★ **三态分开说**，这是 daemon 那条原语刻意回三个字段的理由（`control/cc_bus.rs`
/// 逐字：「回值要说清"到底动了什么"：会话是被杀了，还是身份对不上只摘了登记？」）：
/// 真杀了会话 · 身份对不上**只摘了陈旧登记而会话没动** · 两样都没发生。
///
/// ⚠ 第四档是**应答形状不认识**：那时我们**不知道它动没动**，所以不许说成「没杀成」——
/// 同 `daemon_kill::killed_from_reply` 的那条理由（破坏性动作上把未知说成否定，
/// 下一步就是在未知状态上再做一次）。
pub(crate) fn describe_kill_reply(id: &str, reply: Option<&serde_json::Value>) -> String {
    let get = |k: &str| reply.and_then(|r| r.get(k)).and_then(|v| v.as_bool());
    match (get("killed"), get("stale_only")) {
        (Some(true), _) => format!("已收掉 {id}（会话与进程树都杀了）"),
        (Some(false), Some(true)) => format!(
            "{id} 的**登记摘掉了，会话没动** —— 那个名字今天挂在别人的会话上（身份对不上）"
        ),
        (Some(false), Some(false)) => format!(
            "{id} 没有被收掉：daemon 说它既没杀会话、也没摘登记（多半这个名字根本不在总线上）"
        ),
        _ => format!(
            "收掉 {id} 的应答形状不认识（缺 `killed` / `stale_only`）—— 这一端与那一端的契约漂开了；\
             ⚠ **不知道它到底动没动**，别当成「没杀成」重来一次"
        ),
    }
}

/// 收掉一个 agent：走 daemon 的 `bus-kill` 原语〔`K-R112` 09-13〕。
///
/// # 🔴 为什么不是"再拼一条 `cc-kill` 的 shell 串"
///
/// 在本件之前这条命令的主路**就是 SSH**（`build_kill_cmd` + `exec_read`）〔散文墓碑〕，
/// 而 daemon 侧那条 `bus-kill` 早就有（`P4f` 续 p2c）⇒ **不是缺能力，是还没改走**
/// —— 与 `K-R98` 给发消息记的那句逐字同形。
///
/// 改走之后换来两样具体的东西，都不是「架构更整齐」这种空话：
/// 1. **`<local>` 通了**：本机此前被 `refuse_local_write` 拦着（那句拒绝逐字写着
///    「写面归 `P4b`（cc-bus 改调 daemon 原语）」）—— 前提到期了，这一条不再拒。
/// 2. **回值从「一坨回显」变成三个字段**：老路把 `cc-kill` 的 stdout `trim` 一下就交给用户，
///    「杀了会话」与「只摘了陈旧登记」在那一坨里分不开；`bus-kill` 分得开，
///    [`describe_kill_reply`] 把它讲成三句不同的话。
///
/// ⚠ **两道校验留在这一侧**（同 [`send_via_daemon`]）：id 今天不再被拼进任何命令串，
/// 但「调用方不能靠对端校验」这条规矩不因为注入面没了就作废 —— 这一条的后果是**杀掉一棵进程树**。
async fn kill_via_daemon(origin: &str, id: &str) -> Result<String, String> {
    let outcome = format!("`{id}` **没有被收掉**");
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝收掉它）: {id:?}"));
    }
    let Some(client) = crate::inbound_client::client_for(origin) else {
        return Err(describe_no_channel_for(origin, "收掉 agent", &outcome));
    };
    // 能力协商放在动手之前：老 daemon 没有这条命令时也回「没发出去」，但那一档经分流器
    // 出来与「入方向排队满了」同形 —— 先问一句，是为了让「这台的后端太旧」说得出口。
    if !client.accepts(BUS_KILL) {
        return Err(describe_daemon_too_old_for(origin, BUS_KILL, &outcome));
    }
    match client
        .call(
            BUS_KILL,
            serde_json::json!({ "id": id }),
            std::time::Duration::from_secs(30),
        )
        .await
    {
        Ok(reply) => Ok(describe_kill_reply(id, reply.as_ref())),
        Err(e) => Err(describe_bus_error(&outcome, &e)),
    }
}

/// P4c（#77/#78）：收掉一个 agent。**破坏性，不可撤销** —— UI 侧必须两步确认（同 spawn）。
///
/// # 🔴 `origin` 是原语的一个入参，不是一个分支〔`K-R112` 09-13〕
///
/// 本机与远端**共用这整个函数体**（同 `cc_bus_send`）：`client_for(origin)` 两侧都答得出。
/// ⚠ `cc_bus_spawn` **仍然拒** `<local>` —— daemon 侧没有 spawn 原语。
/// 拒绝理由是**逐条**的，不是一句通用话，别把两条一起放行。
#[tauri::command]
pub async fn cc_bus_kill(origin: String, id: String) -> Result<String, String> {
    kill_via_daemon(&origin, &id).await
}

/// B03 批二：图形化 spawn。**注意这会起一个真实 agent 进程（消耗额度）**
///
/// `account`：`None` 或空串 = 显式用基座（转发 `--base`）；否则用该账号。
/// —— UI 侧必须先让用户确认。
#[tauri::command]
pub async fn cc_bus_spawn(
    origin: String,
    dir: String,
    task: String,
    tool: String,
    account: Option<String>,
) -> Result<String, String> {
    let acct = account.as_deref().filter(|a| !a.is_empty());
    if let Some(why) = refuse_local_write(&origin, "spawn 一个 agent") {
        return Err(why);
    }
    let cmd = build_spawn_cmd(&tool, &dir, &task, acct)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        60,
        "spawn",
        OnOverflow::Truncate,
    )
    .await?;
    Ok(out.trim().to_string())
}

#[cfg(test)]
#[path = "../../../tests/bridge/cc_bus_tests.rs"]
mod tests;
