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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct CcBusAgent {
    pub id: String,
    pub pane: String,
    pub registered_at: String,
}

/// `spawned.tsv` 的一行：id / 工作目录 / spawn 时间 / 初始任务。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct CcBusSpawned {
    pub id: String,
    pub dir: String,
    pub spawned_at: String,
    pub task: String,
}

/// 一次读回的完整状态。`skipped` = 两个文件里被跳过的坏行总数。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
        // `+ 1` 的用意见 remote-daemon-proto/src/common/fs.rs 那条既有注释：
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

/// 查在线的输出上限〔devbench F10b 提成具名常量〕。预期输出是 `ONLINE\n` / `OFFLINE\n`。
const ONLINE_PROBE_CAP: u64 = 4096;

/// B03 批一：查**单个** agent 是否真在线（`tmux has-session`）。
///
/// **这是刻意的第二次往返**：`agents.tsv` 只证明"登记过"，实测最早的条目是 10 天前的
/// （进程早没了）。若在读状态时就顺带全量查在线，一屏 37 个 agent 就是 37 次 tmux 调用
/// ——所以只在用户点某一行的「检查」时查那一行。**不默认全量查、不轮询**（红线）。
///
/// **id 会被拼进命令串**，这正是 `is_valid_bus_id` 存在的理由：盘上真有 `--help` 这种 id，
/// 不挡住的话 `tmux has-session -t --help` 会被当成选项解析。校验不过直接拒绝，不构造命令。
#[tauri::command]
pub async fn check_cc_bus_agent_online(origin: String, id: String) -> Result<bool, String> {
    // **必须走 `build_online_cmd`，不能内联复制一份校验**（B03 审计阻塞-2）：
    // 我原先在这里内联了 `is_valid_bus_id` + 命令拼接，于是 `build_online_cmd` 成了**零生产
    // 调用点的死代码**，而真正在跑的那句校验**零测试覆盖**——删掉它整套测试照样全绿。
    // 这直接证伪了我自己写的「删掉任何一处校验，对应测试立刻红」。抽取纯函数是为了让断言
    // 落在调用点上，结果抽完没接上去，等于白抽。
    // ★★ **先问身份空间**〔P4f 08-13〕：老的探法是 `tmux has-session -t '=<id>:'`，
    //    **纯按名字** —— 而名字会被重用（今天实测过两次：敲门打进陌生人屏幕、
    //    「收掉 agent」杀了无辜进程）。名字被别人占着时这盏灯照样亮，
    //    用户于是把消息发进一个没人读的收件箱，而 UI 一直说它在线。
    // ⇒ `bus-list` 已经能回答这个问题（登记 + 身份核过的三态）。让它去答。
    // ⚠ 答不上时（没通道 / daemon 太旧 / 问不到 tmux）**回落到老探法**，
    //    那是今天的行为，不比现在坏；但**不许**把"问不到"渲染成"不在线"。
    if let Some(live) = online_via_daemon(&origin, &id).await {
        return Ok(live);
    }
    let cmd = build_online_cmd(&id)?;
    // P4a-Y1：本机跑同一条 `build_online_cmd` 产出的串。
    // 截断的 `contains("ONLINE")` 是碰运气的答案 ⇒ 与远端同档，超限拒收。
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        let raw =
            local_shell_read(&cmd, ONLINE_PROBE_CAP, 15, "查在线", OnOverflow::Reject).await?;
        return Ok(raw.contains("ONLINE"));
    }
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let read = async {
        use tokio::io::AsyncReadExt;
        let stream = crate::ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
        let mut buf = Vec::new();
        stream
            .take(ONLINE_PROBE_CAP + 1)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("查在线失败: {e}"))?;
        if buf.len() as u64 > ONLINE_PROBE_CAP {
            // 预期输出 ~7 字节。真吐这么多说明对端在打招呼横幅之类的东西，
            // 而**截断之后 `contains("ONLINE")` 的答案是碰运气的** ——
            // 上限落在末尾就读成「不在线」。宁可报错也不给一个碰运气的布尔。
            return Err(format!(
                "远端查在线的输出超过 {ONLINE_PROBE_CAP} 字节 —— 对端不像只回了 ONLINE/OFFLINE，拒收"
            ));
        }
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(15), read)
        .await
        .map_err(|_| format!("远端 '{origin}' 查在线超时（15s）"))??;
    Ok(String::from_utf8_lossy(&raw).contains("ONLINE"))
}

// ===== B03 批二：命令构造抽成**纯函数**，让校验落在可测的地方 =====
//
// 为什么要抽：变异测试实测发现，把 `cc_bus_send` 里那句 id 校验整个删掉，测试**照样全绿**
// ——断言测的是 `is_valid_bus_id` 这个**谓词本身**，而不是"命令构造真的调了它"
// （失效模式③：门禁太窄，断言没覆盖使用处）。这几个 async 命令要 SSH 连接、没法单测，
// 于是把「校验 + 拼串」这段纯逻辑摘出来：async 那层只管连接与读回，构造与校验在这里，
// 测试直接打这里。删掉任何一处校验，对应测试立刻红。

/// `tmux has-session` 的目标串。`=<名>:` 精确形态（INVARIANTS §31a：裸目标是
/// 「精确→名字开头→glob」三级解析，会命中/误杀兄弟会话）。
fn build_online_cmd(id: &str) -> Result<String, String> {
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝拼入命令）: {id:?}"));
    }
    Ok(format!(
        "tmux has-session -t '={id}:' 2>/dev/null && echo ONLINE || echo OFFLINE"
    ))
}

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

/// 发消息。**两道防线**：`id` 是位置参数（`--help` 会被当选项）→ 白名单校验；
/// `text` 是任意用户输入 → `shell_quote` 单引号逃逸。
/// 投递管线（ACL/限流/去重/灭环）一律归 `cc-bus-lib.sh`，**cc-monitor 侧不重实现**。
fn build_send_cmd(id: &str, text: &str) -> Result<String, String> {
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝拼入命令）: {id:?}"));
    }
    if text.trim().is_empty() {
        return Err("消息为空".to_string());
    }
    Ok(format!(
        "cc-send {id} {} 2>&1",
        crate::ssh_source::shell_quote(text)
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
/// P4c：广播。**只收消息文本**（无 id 可校验），文本过 `shell_quote`。
///
/// ⚠ 它走的是与 `cc-send` **同一条路由管线** —— `cc-broadcast` 头注逐字
/// 「实现=循环调 `cc-send --broadcast`，因此**自动继承整条路由管线**(ACL/限流/去抖/队列/daemon)，不绕过」。
/// ⇒ 本处不必也不该再加一层自己的限流。
fn build_broadcast_cmd(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("消息为空".to_string());
    }
    Ok(format!(
        "cc-broadcast {} 2>&1",
        crate::ssh_source::shell_quote(text)
    ))
}

/// P4c：收掉一个 agent。**破坏性且不可撤销**（`cc-kill` 头注逐字「杀会话+进程树 + 清名册/台账」）。
///
/// ⚠ `id` 会被拼进命令串 ⇒ 必须过 `is_valid_bus_id`。
/// `cc-kill` 自己也校验，但**调用方不能靠对端校验** —— 那是 `build_send_cmd` 头注立的规矩，
/// 而它的理由在这里更硬：这一条的后果是杀掉一棵进程树。
fn build_kill_cmd(id: &str) -> Result<String, String> {
    if !is_valid_bus_id(id) {
        return Err(format!("非法 agent id（拒绝拼入命令）: {id:?}"));
    }
    Ok(format!("cc-kill {id} 2>&1"))
}

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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
        // `+ 1` 见 remote-daemon-proto/src/common/fs.rs：不多读一个字节就分不清
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
    // ★ **兜底，不是主路**〔P4a-Y2〕。三个调用方今天都在它之前分了本机
    //   （`read_cc_bus_inbox` 走本机臂直接返回；`cc_bus_send`/`cc_bus_spawn` 先过 `refuse_local_write`）。
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

/// P4a-Y1：**本机跑同一条串** —— [`exec_read`] 的孪生兄弟，差别只有「谁来跑它」。
///
/// # 为什么不是在这里重写一遍 cc-bus 的文件布局
///
/// `CC_BUS_CAT_CMD` 逐字知道 `~/.cc-bus/agents.tsv` 长什么样。本机要是自己去 `read_to_string`
/// 那两个文件，仓里就有了**两份**同一件事的表示，而它们会各自漂 ——
/// 这个仓管这叫「一段逻辑、两种表示」，`ccm` 的 `resolve_from_daemon`/`resolve_recipe`
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
        let mut child = tokio::process::Command::new(&shell)
            .args(["-lc", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            // ★★ **超时那条路全靠它**〔D 阶段补审 08-12〕。
            //
            // 下面那句显式 `start_kill()` 只在**成功路径**上；超时是
            // `tokio::time::timeout` 把整个 future 丢掉，走不到那里。而 tokio 的 `Child`
            // **默认不因句柄被 drop 而杀子进程** ⇒ 每超时一次漏一个 `sleep`/`cat`。
            //
            // 本会话已经因为「我自己留下的孤儿进程」栽过一次：`#60` 的八轮实测里，
            // 有五个孤儿 daemon 把读数全带偏了，我却先后猜了七个错误的病因。
            // ⇒ 教训的产物不是「以后小心」，是
            // `a_timed_out_local_read_does_not_leave_an_orphan_behind` 那条判据。
            .kill_on_drop(true)
            .spawn()
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
fn refuse_local_write(origin: &str, what: &str) -> Option<String> {
    if origin != crate::inbound_client::LOCAL_ORIGIN {
        return None;
    }
    Some(format!(
        "本机还不能{what}：cc-bus 的**写**面在本机没有对侧。\n\
         读面（清单 / 在线 / inbox）本机已经通了，写面归 `P4b`（cc-bus 改调 daemon 原语）——\n\
         用户 08-12 已裁「先把确切的命令组件做出来，然后 cc-bus 可以去调用」。"
    ))
}

/// 从 `bus-list` 的回值里取某个 id 的在线状态 —— **纯函数**。
///
/// `None` 有两种来源，**它们都不是"不在线"**：
/// · 这个 id 不在总线名单里（那它根本不是 agent）；
/// · `live` 是 `null`（daemon 问不到身份空间）。
/// ⇒ 调用方拿到 `None` 时**回落**到老探法，而不是渲染成一盏灭灯。
pub(crate) fn live_of(agents: &[serde_json::Value], id: &str) -> Option<bool> {
    agents
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(id))
        .and_then(|a| a.get("live"))
        .and_then(|v| v.as_bool())
}

/// 问 daemon「这个 agent 在线吗」。`None` = 它答不上（调用方回落）。
async fn online_via_daemon(origin: &str, id: &str) -> Option<bool> {
    let client = crate::inbound_client::client_for(origin)?;
    let listed = client
        .call(
            "bus-list",
            serde_json::json!({}),
            std::time::Duration::from_secs(15),
        )
        .await
        .ok()?;
    let agents = listed
        .as_ref()
        .and_then(|v| v.get("agents"))
        .and_then(|v| v.as_array())?;
    live_of(agents, id)
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
            "bus-list",
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
            .call("bus-send", args, std::time::Duration::from_secs(30))
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

/// 本机发消息：走 daemon 的 `bus-send` 原语（`P4f`）。
///
/// # 为什么不是"本机也拼一条 shell 串"
///
/// 那正是 `C1` 排除的东西（一份语义两处实现）。daemon 那条原语自己带着**六档错误码**
/// 与**三态在线**，本机这条路只做一件事：把它们讲成人话。
///
/// ⚠ 老 daemon 没有这条命令 ⇒ `CallError::Unsupported`（能力协商，不是超时）
/// ⇒ 报「这台的 daemon 太旧」，而不是含糊的失败。
async fn send_via_local_daemon(id: &str, text: &str) -> Result<String, String> {
    use crate::inbound_client::{client_for, LOCAL_ORIGIN};
    let Some(client) = client_for(LOCAL_ORIGIN) else {
        return Err(
            "本机 daemon 通道没起来 —— 发消息要经它（设置里可以起/停本机 daemon）。".into(),
        );
    };
    // ★ **以谁的身份发**〔08-13 实测〕：不给 `from` 的话，daemon 跑 `cc-send` 时不在任何
    //   tmux pane 里，`cc-whoami` 解不出身份 ⇒ 收信人看到「来自 unknown」，
    //   而它给的回复方式是 `cc-send unknown "…"` —— **回复直接掉进没人读的收件箱**。
    // ⚠ 用 `MONITOR_BUS_ID` 这个固定身份：收信人至少知道**这条是从 cc-monitor 发来的**。
    //   ⚠ 回复仍然没有归宿（没人读 `cc-monitor` 的收件箱）—— 那条已记进 ROADMAP `U17`，
    //   不在这一刀里假装解决。
    let args = serde_json::json!({ "to": id, "text": text, "from": MONITOR_BUS_ID });
    match client
        .call("bus-send", args, std::time::Duration::from_secs(30))
        .await
    {
        Ok(reply) => Ok(describe_send_reply(id, reply.as_ref())),
        // ⚠ **分流走共用的那一份**（`daemon_route::route_call_error`）：
        //   `daemon_route` 的登记表逐字要求「新增一个发送端就必须在这里表态」，
        //   而它自己的头注记着为什么 —— 分流规则一旦有第二份实现，
        //   「被门拒绝」就会在某一份里被洗成「换条路重做」。
        // ★ 本发送端**没有第二条路可回落**（本机 shell 写面正是 `P4a` 拒掉的东西）
        //   ⇒ 两档结果都只是给用户的一句话，`Routed` 的回落语义在这里是空的。
        Err(e) => Err(
            match crate::backend::control::daemon_route::route_call_error(&e, |code, message| {
                format!("{code}：{message}")
            }) {
                crate::backend::control::daemon_route::Routed::NoChannel(why) => {
                    format!("{why}（{id} 的消息**没有发出去**）")
                }
                crate::backend::control::daemon_route::Routed::Refused(why) => why,
                crate::backend::control::daemon_route::Routed::Done => {
                    "发消息失败（分流器判成已完成，这不该发生）".to_string()
                }
            },
        ),
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
    if origin == crate::inbound_client::LOCAL_ORIGIN {
        return send_via_local_daemon(&id, &text).await;
    }
    let cmd = build_send_cmd(&id, &text)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        30,
        "发消息",
        OnOverflow::Truncate,
    )
    .await?;
    Ok(out.trim().to_string())
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
    // ⚠ 本机**没有回落**（本机 shell 写面正是 `P4a` 拒掉的东西）；远端拿不到 daemon 能力时
    //   回落到老的 SSH 路径（`C7` 过渡期），并如实说清那次是老行为。
    match broadcast_via_daemon(&origin, &text).await {
        Ok(msg) => return Ok(msg),
        Err(BroadcastRoute::NoChannel(why)) => {
            if origin == crate::inbound_client::LOCAL_ORIGIN {
                return Err(format!("{why}（本机没有第二条路可走）"));
            }
            // 远端：回落到老路（下面那段），但把原因带上
            tracing::info!("[{origin}] 广播回落到 SSH 路径：{why}");
        }
        Err(BroadcastRoute::Failed(why)) => return Err(why),
    }
    let cmd = build_broadcast_cmd(&text)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        30,
        "广播",
        OnOverflow::Truncate,
    )
    .await?;
    Ok(out.trim().to_string())
}

/// P4c（#77/#78）：收掉一个 agent。**破坏性，不可撤销** —— UI 侧必须两步确认（同 spawn）。
#[tauri::command]
pub async fn cc_bus_kill(origin: String, id: String) -> Result<String, String> {
    if let Some(why) = refuse_local_write(&origin, "收掉 agent") {
        return Err(why);
    }
    let cmd = build_kill_cmd(&id)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(
        &cfg,
        &cmd,
        CONTROL_REPLY_CAP,
        30,
        "收掉 agent",
        OnOverflow::Truncate,
    )
    .await?;
    Ok(out.trim().to_string())
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
mod tests {
    use super::*;

    // ===== id 校验：`--help` 这条是真实盘面数据，不是构造的边角 =====
    #[test]
    fn rejects_leading_dash_ids_from_real_disk() {
        // 盘上真实存在 `~/.cc-bus/inbox/--help.jsonl`（188 字节）与 `282.jsonl`。
        assert!(
            !is_valid_bus_id("--help"),
            "--help 必须被拒（会被当成 flag）"
        );
        assert!(!is_valid_bus_id("-x"));
        assert!(!is_valid_bus_id(""));
        // 纯数字是合法的（`282` 虽然是误用产生的，但它本身不构成注入面）
        assert!(is_valid_bus_id("282"));
    }

    #[test]
    fn accepts_real_ids() {
        for id in ["proj_cc", "cc-9d66c46d", "KVM_cc", "EasyTier_cc", "x_y_cc"] {
            assert!(is_valid_bus_id(id), "{id} 应合法");
        }
    }

    #[test]
    fn rejects_shell_metachars_and_control() {
        for bad in [
            "a b", "a;rm", "a$(x)", "a\nb", "a\tb", "a/b", "a.b", "a:b", "a*",
        ] {
            assert!(!is_valid_bus_id(bad), "{bad:?} 应被拒");
        }
    }

    // ===== 真实脏数据：目录名含换行，把一条记录劈成 2+3 字段两行 =====
    #[test]
    fn survives_embedded_newline_in_dir_real_sample() {
        let text = "good_cc\t/tmp/a\t2026-07-18T19:00:00-07:00\t任务\n\
                    x_y_cc\t/tmp/tmp.o6LGcLq9Qq/x\n\
                    y\t2026-07-18T19:29:07-07:00\t\n\
                    other_cc\t/tmp/b\t2026-07-18T20:00:00-07:00\t\n";
        let (rows, skipped) = parse_spawned_tsv(text);
        // 两条好行必须活下来——坏行不能连累它们
        assert_eq!(rows.len(), 2, "好行应全部解出，实得 {rows:?}");
        assert_eq!(rows[0].id, "good_cc");
        assert_eq!(rows[1].id, "other_cc");
        assert_eq!(skipped, 2, "两条畸形行应被计数");
    }

    // ===== 真实脏数据：任务文本含换行，产生 0/1 字段行 =====
    #[test]
    fn survives_multiline_task_text_real_sample() {
        let text = "a_cc\t/tmp/a\t2026-07-18T19:00:00-07:00\t背景:android-terminal\n\
                    (aterm,手机 SSH 终端 App)这边准备接进\n\
                    \n\
                    b_cc\t/tmp/b\t2026-07-18T21:00:00-07:00\t\n";
        let (rows, skipped) = parse_spawned_tsv(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(skipped, 1, "空行不计入 skipped，只有那一条有内容的坏行算");
    }

    #[test]
    fn many_bad_lines_still_yield_all_good_rows() {
        // 坏行很多时也不能整体失败。（原名叫 "majority"，但真实盘面是 5/15=33%，
        // 并非多数派——名字与事实不符会误导后来人，已改名。这里构造 8 条纯属压力形态。）
        let mut text = String::new();
        for i in 0..7 {
            text.push_str(&format!(
                "ok{i}_cc\t/tmp/{i}\t2026-07-18T19:00:00-07:00\tt\n"
            ));
        }
        for i in 0..8 {
            text.push_str(&format!("broken{i}\n"));
        }
        let (rows, skipped) = parse_spawned_tsv(&text);
        assert_eq!(rows.len(), 7);
        assert_eq!(skipped, 8);
    }

    #[test]
    fn garbage_id_rows_are_skipped_not_rendered() {
        let text = "--help\t/tmp/x\t2026-07-18T19:00:00-07:00\tt\n\
                    good_cc\t/tmp/y\t2026-07-18T19:00:00-07:00\tt\n";
        let (rows, skipped) = parse_spawned_tsv(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "good_cc");
        assert_eq!(skipped, 1, "--help 这行必须被跳过，不能渲染进驾驶舱");
    }

    // ===== agents.tsv：结构干净，但校验仍要在 =====
    #[test]
    fn parses_clean_agents_tsv() {
        let text = "cc-9d66c46d\tcc-9d66c46d:0.0\t2026-07-28T11:48:32-07:00\n\
                    KVM_cc\tKVM_cc:0.0\t2026-07-18T07:26:31-07:00\n";
        let (rows, skipped) = parse_agents_tsv(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(skipped, 0);
        assert_eq!(rows[1].registered_at, "2026-07-18T07:26:31-07:00");
    }

    #[test]
    fn bad_timestamp_does_not_drop_the_row() {
        // 时间戳坏掉不该让这个 agent 从驾驶舱消失——UI 标"时间未知"即可
        let text = "a_cc\ta_cc:0.0\tnot-a-timestamp\n";
        let (rows, skipped) = parse_agents_tsv(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(skipped, 0);
        assert_eq!(rows[0].registered_at, "not-a-timestamp");
    }

    #[test]
    fn empty_and_missing_input_are_not_errors() {
        assert_eq!(parse_agents_tsv(""), (vec![], 0));
        assert_eq!(parse_spawned_tsv("\n\n\n"), (vec![], 0));
    }

    // ===== 合并读回的切分 =====
    #[test]
    fn splits_combined_payload() {
        let raw = format!("AGENTS\n{CC_BUS_SPLIT_MARKER}\nSPAWNED\n");
        let (a, b) = split_combined(&raw, CC_BUS_SPLIT_MARKER);
        assert_eq!(a.trim(), "AGENTS");
        assert_eq!(b.trim(), "SPAWNED");
    }

    #[test]
    fn missing_marker_degrades_gracefully() {
        let (a, b) = split_combined("only one file", CC_BUS_SPLIT_MARKER);
        assert_eq!(a, "only one file");
        assert_eq!(b, "");
    }

    /// 造一份自述头。测试自己拼，是为了让下面每一格喂的**逐字**是什么一眼看得见。
    fn head_line(home: u8, cat: u8) -> String {
        format!("{CC_BUS_HEAD_MARKER} home={home} cat={cat}\n")
    }

    /// 🔴🔴 **本件的正题判据：「读不到」不许长成「一个都没有」。**
    ///
    /// # 它为什么存在（**云端有读数，不是设想**）
    ///
    /// 09-10 云端 `windows-latest` 那趟 `cargo test` 1323 过 1 红，红的那条报的是
    /// 「`bash -lc 'sleep 2.8416'` 只花了 120.7456ms 就回来了」——**那个壳里外部命令跑不起来**。
    /// 而 `CC_BUS_CAT_CMD` 的主体正是两次 `cat`，归同一个 `PATH`。
    ///
    /// 本机现打（`bash -lc`，两种情形各跑一趟，`cmp` 逐字节比）：
    /// · `cat` 缺席、两张表**真的在** → stdout 逐字 `"\n@@CCMON-CCBUS-SPLIT@@\n"`，rc=0；
    /// · `cat` 在、`$CC_BUS_HOME` **整个不存在** → stdout **一模一样**，rc=0。
    /// ⇒ 修之前，这两件事在解析器眼里是同一份输入 ⇒ 驾驶舱两次都写
    /// 「这台机器上没有登记过的 cc-bus agent」。**后者对，前者是假话。**
    ///
    /// # 🔴 铁律 12：**修之前它一定红** —— 静态推演（跑不了测试，所以逐格推）
    ///
    /// 把 [`interpret_cc_bus_read`] 换回本件之前那三行
    /// （`split_combined` → `parse_agents_tsv` → `parse_spawned_tsv`，出口是裸 `CcBusState`），
    /// 喂下面 ② 那格的输入 `"\n@@CCMON-CCBUS-SPLIT@@\n"`：
    /// 1. `split_combined` 命中标记 ⇒ `a == "\n"`、`s == "\n"`；
    /// 2. `parse_agents_tsv("\n")`：`text.lines()` 给出一行 `""`，`row_fields` 判
    ///    「`trim().is_empty()` 且不含 `\t`」⇒ `None` ⇒ `continue` ⇒ `(vec![], 0)`；
    /// 3. `parse_spawned_tsv("\n")` 同理；
    /// 4. 出口 `CcBusState { agents: [], spawned: [], skipped: 0 }` —— **是 `Ok` 不是 `Err`**
    ///    ⇒ ② 那格的 `is_err()` **红**。①③⑥ 同理（都拿得到一个空 `CcBusState`）。
    /// ⇒ 本判据**不是恒绿**，它逐格咬住的正是被修掉的那个行为。
    ///
    /// # 反向：**不许恒红**
    ///
    /// ④「真的没装」与 ⑤「真有 agent」两格要求 `Ok`，且 ⑤ 要求数得出行 ——
    /// 一个「凡是零个就报错」的糊涂修法会在 ④ 红。这两格是本判据的非空对照。
    #[test]
    fn a_read_that_could_not_happen_is_never_rendered_as_an_empty_roster() {
        // ① 那条命令根本没跑起来（`bash` 是个存根 / 壳不认这段语法）⇒ 空串
        let e1 = interpret_cc_bus_read("<local>", "").expect_err("空输出必须是错，不是零个");
        assert!(e1.contains("没有跑起来"), "错误没说清是哪一步：{e1}");

        // ② **今天生产上 `cat` 缺席时逐字节的输出**（本机实测过的那一串）
        let e2 = interpret_cc_bus_read("<local>", "\n@@CCMON-CCBUS-SPLIT@@\n")
            .expect_err("只有分隔标记 = 自述头没回来 = 读不到");
        assert!(e2.contains("没有跑起来"), "{e2}");

        // ③ 头回来了，而它自己说这个壳里没有 `cat`
        let raw3 = format!("{}\n{CC_BUS_SPLIT_MARKER}\n", head_line(1, 0));
        let e3 = interpret_cc_bus_read("<local>", &raw3).expect_err("cat=0 必须是错");
        assert!(e3.contains("没有 `cat`"), "{e3}");
        assert!(
            e3.contains("不是"),
            "错误里必须写明它不是「一个 agent 都没有」，否则用户读到的还是同一句：{e3}"
        );

        // ④ **非空对照**：cc-bus 真的没装 —— 这是一种**合法状态**，不许顺手判成错。
        let raw4 = format!("{}\n{CC_BUS_SPLIT_MARKER}\n", head_line(0, 1));
        let st4 = interpret_cc_bus_read("<local>", &raw4).expect("没装是状态不是错误");
        assert_eq!(st4.agents.len(), 0);
        assert_eq!(st4.spawned.len(), 0);
        assert_eq!(st4.skipped, 0, "自述头不许被当成坏行计进 skipped");

        // ⑤ **非空对照**：真读到了 —— 证明这把尺子不是恒红，且头被干净地摘掉了。
        let raw5 = format!(
            "{}a_cc\ta_cc:0.0\t2026-01-01\n{CC_BUS_SPLIT_MARKER}\nb_cc\t/d\t2026-01-01\ttask\n",
            head_line(1, 1)
        );
        let st5 = interpret_cc_bus_read("<local>", &raw5).expect("正常读回不该报错");
        assert_eq!(st5.agents.len(), 1, "头没摘干净或行被吃了");
        assert_eq!(st5.agents[0].id, "a_cc");
        assert_eq!(st5.spawned.len(), 1);
        assert_eq!(st5.skipped, 0, "自述头串进了解析器");

        // ⑥ 只回来半份（流被掐 / 输出被截）⇒ 拒收，不拿半份清单当完整的用。
        let raw6 = format!("{}a_cc\ta_cc:0.0\t2026-01-01\n", head_line(1, 1));
        let e6 = interpret_cc_bus_read("<local>", &raw6).expect_err("半份必须拒收");
        assert!(e6.contains("半份"), "{e6}");

        // ⑦ 头在、但字段读不出来 ⇒ 整个头作废，**不许悄悄当成 `false`/`true`**。
        let bad_head = format!("{CC_BUS_HEAD_MARKER} home=? cat=1");
        let raw7 = format!("{bad_head}\n\n{CC_BUS_SPLIT_MARKER}\n");
        assert!(
            interpret_cc_bus_read("<local>", &raw7).is_err(),
            "读不懂的自述头被当成了一次成功的读"
        );
    }

    /// ★ **断言落在使用处**：`read_cc_bus_state` 必须**经** [`interpret_cc_bus_read`] 出结果。
    ///
    /// ⚠ 这条是上面那条的搭档，缺了它上面那条就守不住 —— 本仓有过逐字的先例：
    /// `build_online_cmd` 抽成纯函数、判据打在纯函数上，而生产调用点**内联复制了一份**，
    /// 于是「删掉真正在跑的那句校验，整套测试照样全绿」（`check_cc_bus_agent_online` 头注
    /// 逐字记着这次）。本条钉的是**接线**：命令函数里不许自己解析。
    ///
    /// # 🔴 铁律 12：修之前它一定红（静态推演）
    ///
    /// 本件之前，`read_cc_bus_state` 的函数体逐字含
    /// `let (a, s) = split_combined(&raw, CC_BUS_SPLIT_MARKER);` 与两个 `parse_*_tsv(`，
    /// 且**不含** `interpret_cc_bus_read(` ⇒ 下面第一条 `assert!(body.contains(...))` 直接红。
    /// 反向变异（把解析搬回命令函数里）⇒ 第二、三条红。
    #[test]
    fn the_cockpit_never_renders_an_unread_roster_as_empty() {
        let code = non_test_code();
        assert_eq!(
            code.matches("fn interpret_cc_bus_read(").count(),
            1,
            "解释点不是恰好一处 —— 两条传输路一旦各解释各的，口径立刻会漂"
        );
        let at = code
            .find("pub async fn read_cc_bus_state(")
            .expect("生产段找不到读状态命令 —— 判据在空转");
        let rest = &code[at..];
        let end = rest[1..]
            .find("\npub ")
            .map(|k| k + 1)
            .unwrap_or_else(|| rest.len().min(1600));
        let body = &rest[..end];
        // 窗口自检：取错窗口的话下面三条会变成空真（本仓这两条判据自己栽过这个坑）。
        assert!(
            body.contains("CC_BUS_CAT_CMD"),
            "窗口取错了 —— 里面看不见那条命令常量，下面整条是空真。实得：{body}"
        );
        assert!(
            body.contains("interpret_cc_bus_read("),
            "读状态没有经过那道「先证明这次真的读到了」的解释 —— \n\
             空输出会被解析成零个 agent，而驾驶舱把零个渲染成「这台机器上没有 agent」。"
        );
        for inline in ["split_combined(", "parse_agents_tsv(", "parse_spawned_tsv("] {
            assert!(
                !body.contains(inline),
                "`{inline}` 又被内联进了读状态命令 —— 那正好绕过自述头那一步，\n\
                 而绕过之后的症状是**全绿**：空输入解析出零个 agent，一条断言都不会红。"
            );
        }
    }

    #[test]
    fn never_panics_on_adversarial_input() {
        for bad in [
            "\0\0\0",
            "\t\t\t\t\t",
            "a\t\t\t",
            &"x".repeat(100_000),
            "\u{feff}a_cc\tp\tt\tx",
        ] {
            let _ = parse_agents_tsv(bad);
            let _ = parse_spawned_tsv(bad);
        }
    }

    // ===== 定值命令：零注入面（同 mcp.rs 那条 CMD 常量的形状）=====
    #[test]
    fn cat_command_is_a_constant_with_no_interpolation() {
        // origin 只用于选连接配置，绝不能出现在命令串里
        assert!(!CC_BUS_CAT_CMD.contains("{}"));
        assert!(!CC_BUS_CAT_CMD.contains("$1"));
        assert!(CC_BUS_CAT_CMD.contains(CC_BUS_SPLIT_MARKER));
        // 只读：不得出现任何写操作
        for w in [
            "rm ", "mv ", "> ", ">>", "tee ", "truncate", "chmod", "kill",
        ] {
            assert!(!CC_BUS_CAT_CMD.contains(w), "定值命令里不该有写操作 {w:?}");
        }
        // 尊重 CC_BUS_HOME，且两文件缺失时仍 rc=0
        assert!(CC_BUS_CAT_CMD.contains("CC_BUS_HOME"));
        assert!(CC_BUS_CAT_CMD.trim_end().ends_with("true"));
        // ★〔ccbus-win 09-10〕自述头：**这条命令必须自己报「我读没读得了」**。
        // 没有它，`cat` 缺席与「目录根本不存在」在 stdout 上逐字节相同（本机 `cmp` 实测）。
        assert!(
            CC_BUS_CAT_CMD.contains(CC_BUS_HEAD_MARKER),
            "读命令不再自报家门 —— 「读不到」会重新长回「一个都没有」"
        );
        assert!(
            CC_BUS_CAT_CMD.contains("command -v cat"),
            "自述头不再报 `cat` 在不在 —— 那一格正是 09-10 云端红出来的那一形"
        );
        // 自述头必须由**内建**打出来：外部命令一个都没有的壳里它也得回得来，
        // 否则「头没回来」这个读数本身就随着 PATH 一起失效了。
        assert!(
            CC_BUS_CAT_CMD.contains("printf '@@CCMON-CCBUS-HEAD@@"),
            "自述头不是用内建 `printf` 打的 —— 换成外部命令之后，\
             「头没回来」这个读数会跟着 PATH 一起失效"
        );
        assert!(
            !CC_BUS_CAT_CMD.contains("which "),
            "探 `cat` 在不在用了外部的 `which` —— 它自己也可能不在 PATH 上，\
             那是拿一个问不出来的答案去回答另一个问不出来的问题（`command -v` 是内建）"
        );
    }

    // ===== 在线检查：id 必须先过校验才允许拼进命令（盘上真有 `--help` 这种 id）=====
    #[test]
    fn online_check_rejects_ids_before_building_command() {
        // 这条守的是「命令构造前先校验」这个顺序本身：凡 is_valid_bus_id 拒的，
        // 都不该有机会进入 `tmux has-session -t '=<id>:'`。
        for bad in ["--help", "-t", "a b", "a;rm -rf /", "a$(id)", "", "a'b"] {
            assert!(!is_valid_bus_id(bad), "{bad:?} 必须在构造命令前被拒");
        }
        // 反向：合法 id 拼出来的目标是精确形态
        let id = "proj_cc";
        assert!(is_valid_bus_id(id));
        let cmd =
            format!("tmux has-session -t '={id}:' 2>/dev/null && echo ONLINE || echo OFFLINE");
        assert!(
            cmd.contains("'=proj_cc:'"),
            "必须用 =<名>: 精确形态（INVARIANTS §31a）"
        );
    }

    // ===================== B03 批二 =====================
    //
    // **断言方式的两个教训，都写在这里免得再犯**：
    //  ① 第一版我写 `assert!(!cmd.contains("; rm -rf ~;"))` —— 错的。正确逃逸的结果本来
    //     就**包含**那个危险子串，只是它落在单引号内、完全惰性。断言"危险子串不出现"是在
    //     检查一个错误的性质。真正要证的是「这一整坨仍是**一个** shell 词，内容逐字等于
    //     原文」→ 用**往返还原**证。
    //  ② 第二版我把断言打在 `is_valid_bus_id` 这个谓词上，结果把 `cc_bus_send` 里那句
    //     校验整个删掉，测试**照样全绿**（失效模式③：门禁太窄）。所以现在一律打在
    //     `build_*_cmd` 这些**真正构造命令的函数**上。

    /// POSIX 单引号形态的最小逆运算：把 `shell_quote` 的产物还原回原文。
    /// 只认它产出的那一种形状；遇到**裸单引号**返回 None——那正是"能逃出去"的标志。
    fn unquote_posix(q: &str) -> Option<String> {
        let b = q.as_bytes();
        if b.len() < 2 || b[0] != b'\'' || b[b.len() - 1] != b'\'' {
            return None;
        }
        let esc = "'\\''"; // 单引号 反斜杠 单引号 单引号
        let mut out = String::new();
        let mut rest = &q[1..q.len() - 1];
        loop {
            match rest.find('\'') {
                None => {
                    out.push_str(rest);
                    return Some(out);
                }
                Some(i) => {
                    out.push_str(&rest[..i]);
                    if !rest[i..].starts_with(esc) {
                        return None;
                    }
                    out.push('\'');
                    rest = &rest[i + esc.len()..];
                }
            }
        }
    }

    #[test]
    fn unquote_helper_itself_rejects_unescaped_quotes() {
        // 守住这个测试助手本身：它若把裸引号也"还原"了，下面几条就全成了摆设
        assert_eq!(unquote_posix("'a'b'"), None);
        assert_eq!(unquote_posix("noquotes"), None);
        assert_eq!(unquote_posix("'ok'").as_deref(), Some("ok"));
    }

    #[test]
    fn quote_roundtrip_is_the_real_property() {
        for evil in [
            "hi'; rm -rf ~; echo '",
            "$(id)",
            "`whoami`",
            "a\nb",
            "中文 带空格",
            "'",
            "''",
        ] {
            let q = crate::ssh_source::shell_quote(evil);
            assert_eq!(
                unquote_posix(&q).as_deref(),
                Some(evil),
                "逃逸后必须能逐字还原（说明它仍是一个完整的 shell 词）: {q}"
            );
        }
    }

    // ===== 校验落在构造函数上（删掉任何一处校验，这些立刻红）=====
    #[test]
    fn builders_reject_bad_ids_at_the_call_site() {
        for bad in ["--help", "-t", "a b", "a;id", "", "a'b", "a/b"] {
            assert!(build_online_cmd(bad).is_err(), "online: {bad:?} 应被拒");
            assert!(build_inbox_cmd(bad).is_err(), "inbox: {bad:?} 应被拒");
            assert!(build_send_cmd(bad, "hi").is_err(), "send: {bad:?} 应被拒");
        }
    }

    #[test]
    fn online_cmd_uses_exact_target_form() {
        let c = build_online_cmd("proj_cc").unwrap();
        assert!(
            c.contains("'=proj_cc:'"),
            "必须 =<名>: 精确形态（§31a）: {c}"
        );
        assert!(c.contains("ONLINE") && c.contains("OFFLINE"));
    }

    #[test]
    fn inbox_cmd_is_readonly_and_bounded() {
        let c = build_inbox_cmd("proj_cc").unwrap();
        assert!(c.contains("tail -n 200"), "必须有上界: {c}");
        assert!(c.contains("CC_BUS_HOME"), "须尊重 CC_BUS_HOME: {c}");
        for w in ["rm ", "mv ", ">>", "tee ", "kill"] {
            assert!(!c.contains(w), "只读命令里不该有 {w:?}: {c}");
        }
    }

    #[test]
    fn send_cmd_makes_free_text_one_word() {
        let evil = "hi'; rm -rf ~; echo '";
        let c = build_send_cmd("proj_cc", evil).unwrap();
        assert!(c.starts_with("cc-send proj_cc '"));
        assert!(c.ends_with("' 2>&1"));
        let body = &c["cc-send proj_cc ".len()..c.len() - " 2>&1".len()];
        assert_eq!(unquote_posix(body).as_deref(), Some(evil));
        assert!(build_send_cmd("proj_cc", "   ").is_err(), "空消息应被拒");
    }

    #[test]
    fn spawn_cmd_whitelists_tool_and_quotes_paths() {
        for bad in ["bash", "claude; id", "", "CLAUDE"] {
            assert!(
                build_spawn_cmd(bad, "/tmp", "", None).is_err(),
                "tool {bad:?} 应被拒"
            );
        }
        let dir = "/tmp/has space/and'quote";
        let task = "分析; whoami";
        let c = build_spawn_cmd("codex", dir, task, None).unwrap();
        // `--` 结束选项：dir 若是 `--new` 这类词，不加它会被 cc-spawn 的旗标循环吃掉
        assert!(c.starts_with("cc-spawn --tool codex --base -- '"));
        let rest = &c["cc-spawn --tool codex --base -- ".len()..c.len() - " 2>&1".len()];
        let (qd, qt) = rest.split_at(crate::ssh_source::shell_quote(dir).len());
        assert_eq!(unquote_posix(qd).as_deref(), Some(dir));
        assert_eq!(unquote_posix(qt.trim_start()).as_deref(), Some(task));
        // 无任务时不得留下空参数
        let c2 = build_spawn_cmd("claude", "/tmp", "", None).unwrap();
        assert_eq!(c2, "cc-spawn --tool claude --base -- '/tmp' 2>&1");
        assert!(
            build_spawn_cmd("claude", "  ", "t", None).is_err(),
            "空目录应被拒"
        );
    }

    // ===== L2：spawn 必须显式表态用哪个账号（B03 审计重要-5）=====

    /// **不传账号 = 显式用基座**，而不是"什么都不说、让 ccm 落默认号"。
    /// 原实现就是后者：从驾驶舱点两下就在 manifest 默认账号上起真 agent 烧额度，
    /// 用户既没选过也不知道用了哪个号。
    #[test]
    fn spawn_always_states_an_account_choice() {
        let c = build_spawn_cmd("claude", "/d", "", None).unwrap();
        assert!(c.contains(" --base "), "不选账号必须显式 --base，实得: {c}");
        let c2 = build_spawn_cmd("claude", "/d", "", Some("acctz")).unwrap();
        assert!(c2.contains(" --account acctz "), "选了号要转发，实得: {c2}");
        // 两者互斥：命令里不得同时出现
        assert!(!c2.contains("--base"));
        assert!(!c.contains("--account"));
    }

    /// 账号名来自 manifest（我们自己维护），但**仍要过字符集**——
    /// B03 审计的 `--help` 教训：盘上真会出现没人预料的 id，
    /// "这是我们自己的数据"不是免检理由。
    #[test]
    fn account_name_is_validated_before_joining_the_command() {
        for bad in ["--base", "-x", "a b", "a;id", "", "a'b", "a/b", "$(id)"] {
            assert!(
                build_spawn_cmd("claude", "/d", "", Some(bad)).is_err(),
                "账号名 {bad:?} 必须被拒"
            );
        }
        for ok in ["z", "acct_b", "team-1", "A9"] {
            assert!(
                build_spawn_cmd("claude", "/d", "", Some(ok)).is_ok(),
                "{ok} 应合法"
            );
        }
    }

    // ===== inbox 解析同样守"坏行跳过并计数" =====
    #[test]
    fn parses_real_inbox_line() {
        let l = r#"{"id":"KVM_cc-178-31346","from":"KVM_cc","to":"cc-9d66c46d","ts":"2026-07-26T05:06:19-07:00","text":"【告知】A 大半就绪","class":"direct","hops":"1"}"#;
        let (m, sk) = parse_inbox_jsonl(l);
        assert_eq!(sk, 0);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].from, "KVM_cc");
        assert_eq!(m[0].class, "direct");
        assert_eq!(m[0].text, "【告知】A 大半就绪");
    }

    #[test]
    fn inbox_bad_lines_skipped_not_fatal() {
        let text = concat!(
            r#"{"from":"a","text":"ok1","ts":"t","class":"direct"}"#,
            "\n这不是 json\n\n",
            r#"{"from":"b","text":"ok2","ts":"t","class":"broadcast"}"#,
            "\n",
            r#"{"nothing":"useful"}"#,
            "\n"
        );
        let (m, sk) = parse_inbox_jsonl(text);
        assert_eq!(m.len(), 2, "好行必须全解出，实得 {m:?}");
        assert_eq!(sk, 2, "坏 json + 无有效字段各一条；空行不计");
    }

    // ===== B03 审计逼出来的补漏 =====

    /// **阻塞-2 的守卫**：断言在线检查**真的经过** `build_online_cmd`。
    /// 光测 `build_online_cmd` 本身不够——它曾经是零生产调用点的死代码，
    /// 而真正在跑的那份内联校验零覆盖。这条测的是「构造逻辑只有一份」。
    #[test]
    fn online_check_has_exactly_one_command_construction() {
        let code = non_test_code();
        assert!(code.contains("pub async fn check_cc_bus_agent_online"));
        // 非测试**代码**里，这个命令模板只准出现一次（在 build_online_cmd 里）
        assert_eq!(
            code.matches("tmux has-session -t").count(),
            1,
            "在线检查的命令串只准构造一处；多处 = 又内联复制了一份（阻塞-2 原样复发）"
        );
        let f = code
            .split("fn build_online_cmd")
            .nth(1)
            .expect("build_online_cmd 应存在");
        assert!(f.contains("tmux has-session -t"));
        // 且 check_cc_bus_agent_online 必须**调用**它，而不是自己拼
        let g = code
            .split("pub async fn check_cc_bus_agent_online")
            .nth(1)
            .expect("函数应存在");
        assert!(
            g.contains("build_online_cmd(&id)?"),
            "必须走 build_online_cmd"
        );
    }

    /// 取本文件的**非测试、非注释**代码。
    /// **扫源码的守卫必须先剥注释**——本轮我有两条守卫栽在这上面：一条把文档注释里提到的
    /// 命令名也数进去（3 != 1），另一条把错误消息里的 `format!` 当成命令构造。
    /// 守卫扫错东西 = 假红，和恒绿一样坏。
    /// 抠出一段源码里所有普通字符串字面量（**保留源码形态**：`\"` 与 `{{` 原样留着，
    /// 这样拿它回头在源码里数出现次数才对得上）。
    fn string_literals(body: &str) -> Vec<String> {
        let b: Vec<char> = body.chars().collect();
        let (mut out, mut i) = (Vec::new(), 0usize);
        while i < b.len() {
            if b[i] == '"' {
                let (mut j, mut lit) = (i + 1, String::new());
                while j < b.len() && b[j] != '"' {
                    if b[j] == '\\' && j + 1 < b.len() {
                        lit.push(b[j]);
                        lit.push(b[j + 1]);
                        j += 2;
                        continue;
                    }
                    lit.push(b[j]);
                    j += 1;
                }
                out.push(lit);
                i = j + 1;
                continue;
            }
            i += 1;
        }
        out
    }

    /// 一个 `format!` 模板里**最长的静态片段** —— 占位符 `{…}` 是变的，静态片段才是
    /// 「这条命令长什么样」。`{{` / `}}` 是转义的花括号，算静态。
    fn longest_static_run(lit: &str) -> String {
        let c: Vec<char> = lit.chars().collect();
        let (mut best, mut cur, mut i) = (String::new(), String::new(), 0usize);
        while i < c.len() {
            if c[i] == '{' && i + 1 < c.len() && c[i + 1] == '{' {
                cur.push_str("{{");
                i += 2;
                continue;
            }
            if c[i] == '}' && i + 1 < c.len() && c[i + 1] == '}' {
                cur.push_str("}}");
                i += 2;
                continue;
            }
            if c[i] == '{' {
                if cur.chars().count() > best.chars().count() {
                    best = cur.clone();
                }
                cur.clear();
                while i < c.len() && c[i] != '}' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            cur.push(c[i]);
            i += 1;
        }
        if cur.chars().count() > best.chars().count() {
            best = cur;
        }
        best
    }

    /// 取 `fn <name>` 的函数体：从签名那行起，**到下一个顶格行为止**。
    ///
    /// ⚠ 第一版写成「到下一个顶格 `fn ` 为止」，于是 `build_spawn_cmd` 的体一路吃到了
    /// 它下面那个 struct 的属性里，把 `"../../src/generated/"` 当成了命令模板（出现 4 次）。
    /// **同一族的错第 N 次**：我以为的那个对象，与切片实际圈住的那个对象不是同一个。
    /// 顶格行 = 函数自己的收尾行，或下一个顶层项 —— 两者都是正确的边界。
    ///
    /// ⚠ 刻意**不写花括号字面量**来找收尾：本文件会被 `production_code` 一族按括号配平剥，
    /// 而一个落单的右花括号会打坏那个配平（本工作区真踩过一次，红了整轮）。
    fn fn_body(code: &str, name: &str) -> String {
        let start = code
            .find(&format!("fn {name}"))
            .unwrap_or_else(|| panic!("生产段里没有 fn {name} —— 抽取器坏了"));
        let mut out = Vec::new();
        for (i, line) in code[start..].lines().enumerate() {
            // ⚠ 多行签名的收尾行 `) -> Result<…> {` 也顶格 —— 它是**头的一部分**，
            // 不是边界。第一版漏了这条，`build_spawn_cmd` 的体被切在签名处、抠出空串
            // （长度自检当场报出来了 —— 自检存在的意义就在这里）。
            let top_level = !line.is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with(')');
            if i > 0 && top_level {
                break;
            }
            out.push(line);
        }
        out.join("\n")
    }

    /// ★★ **每条远端命令模板都只准构造一处**〔audit-0805 08-07，Phase G 第 39 件〕。
    ///
    /// # 它补的是一个「只挡住了自己那一条」的 singleton
    ///
    /// 隔壁 `online_check_has_exactly_one_command_construction` 钉的是
    /// **`tmux has-session -t` 这一个字面量只准出现一次** —— 那条是对的，
    /// 但它的人群是**当初出事的那一条路**（阻塞-2：有人内联复制了一份在线检查）。
    /// 另外三个构造器（inbox / send / spawn）**一个都没被这条性质覆盖**。
    ///
    /// 08-07 实测：在生产段内联一份
    /// `format!("cc-send {id} {text} 2>&1")`（绕开 `build_send_cmd` 的 id 白名单
    /// **与** `shell_quote` 引用），全仓 **973 条判据一条都不红**。
    /// 而这条路把**任意用户文本**送进远端 shell —— 它是本模块里赌注最高的一条。
    ///
    /// # 人群从构造器本身派生
    ///
    /// 不手写模板清单（手写清单就是下一个「只挡住我列的那几条」）：
    /// 扫出所有 `fn build_*_cmd`，从每个的字符串字面量里取**最长静态片段**
    /// （占位符是变的，静态片段才是「这条命令长什么样」），要求它在生产段恰好出现一次。
    #[test]
    fn every_remote_command_template_is_built_in_exactly_one_place() {
        let code = non_test_code();
        // 人群 = 生产段里所有命令构造器（派生，不是手写清单）。
        let names: Vec<String> = code
            .match_indices("fn build_")
            .map(|(i, _)| {
                code[i + 3..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .collect();
        // ★ 抽取器自检：抓不到构造器时下面整条空转。
        assert!(
            names.len() >= 4,
            "生产段只找到 {} 个 `build_*_cmd`（08-07 实测 4：online/inbox/send/spawn）\
             —— 抽取器坏了或构造器改名了，本条此刻无效：{names:?}",
            names.len()
        );

        for name in &names {
            let body = fn_body(&code, name);
            let body = body.as_str();
            // 两道语义过滤，否则挑中的是**错误消息**而不是命令模板：
            // ① 跳过错误路径那几行（`build_send_cmd` 最长的字面量其实是那句
            //    「非法 agent id（拒绝拼入命令）」，三个构造器共用 ⇒ 出现 3 次；
            //    这一版第一次跑就被自己逮出来了）；
            // ② 命令模板要送进**远端 shell**，必然是 ASCII —— 带中文的一定不是它。
            // ⚠ 两道都只会把候选**变少**：过滤过头 ⇒ 下面那条长度自检当场红（不是静默变绿）。
            let prod_lines: String = body
                .lines()
                .filter(|l| !l.contains("Err("))
                .collect::<Vec<_>>()
                .join("\n");
            let run = string_literals(&prod_lines)
                .iter()
                .map(|l| longest_static_run(l))
                .filter(|s| s.is_ascii())
                .max_by_key(|s| s.chars().count())
                .unwrap_or_default();
            // 自检：片段太短就不足以标识一条命令，本条对它是空转。
            assert!(
                run.chars().count() >= 8,
                "`{name}` 里抠不出足够长的命令静态片段（实得 {:?}）—— \
                 要么它不再用 `format!` 拼命令，要么抽取器坏了。两种都要人来看一眼。",
                run
            );
            assert_eq!(
                code.matches(run.as_str()).count(),
                1,
                "命令模板 {run:?}（属于 `{name}`）在生产段出现了 {} 次，应当恰好 1 次。\n\
                 多出来的那处 = **又内联复制了一份命令构造**，而复制品不会带上构造器里的\n\
                 那几道防线（id 白名单 / `shell_quote` 引用 / 空值拒绝）。\n\
                 ⚠ 这正是阻塞-2 的形状，只是当时只在 `build_online_cmd` 那一条上补了判据。\n\
                 修法是**调用 `{name}`**，不是把这条判据放宽。",
                code.matches(run.as_str()).count()
            );
        }
    }

    fn non_test_code() -> String {
        let src = include_str!("cc_bus.rs");
        let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
        code.lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with('*') && !t.starts_with("/*")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn non_test_code_helper_is_sane() {
        // 守住这个助手本身：剥过头（剥成空）或没剥干净，上下两条守卫就都成了摆设
        let c = non_test_code();
        assert!(
            c.contains("pub async fn check_cc_bus_agent_online"),
            "剥过头了"
        );
        assert!(c.contains("fn build_online_cmd"), "剥过头了");
        assert!(!c.contains("阻塞-2 原样复发"), "注释没剥干净");
        assert!(c.len() > 2000, "剩下的代码太少，守卫形同虚设");
    }

    /// **重要-2 的守卫**：`parse_agents_tsv` 的 id 校验此前**没有会红的断言**
    /// （`never_panics_on_adversarial_input` 用 `let _ =` 丢结果，只守 panic 不守语义）。
    /// 对照 `parse_spawned_tsv` 有 `garbage_id_rows_are_skipped_not_rendered` 守着——
    /// 两个同构解析器只守了一个。
    #[test]
    fn agents_garbage_id_rows_are_skipped_not_rendered() {
        let text = "--help\thelp:0.0\t2026-07-18T07:26:31-07:00\n\
                    good_cc\tgood_cc:0.0\t2026-07-28T11:48:32-07:00\n";
        let (rows, skipped) = parse_agents_tsv(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "good_cc");
        assert_eq!(skipped, 1, "--help 这行必须被跳过并计数");
    }

    /// **重要-4a**：只含制表符的行有结构无内容 → 该算坏行，不该凭空蒸发。
    #[test]
    fn tab_only_line_counts_as_bad_not_vanished() {
        let (rows, skipped) = parse_spawned_tsv("\t\t\t\n");
        assert_eq!(rows.len(), 0);
        assert_eq!(skipped, 1, "有结构无内容的行必须计入 skipped");
        // 真空行仍然不计（这是既有契约，别修坏）
        let (_, sk2) = parse_spawned_tsv("\n\n   \n");
        assert_eq!(sk2, 0, "真空行不计入 skipped，否则 UI 虚报");
    }

    /// **重要-4b**：任务文本里有制表符时，末字段要把余下的都收回来，不能静默截断。
    #[test]
    fn task_with_tabs_is_not_silently_truncated() {
        let (rows, _) = parse_spawned_tsv("a_cc\t/d\t2026\tpart1\tpart2\tpart3\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task, "part1\tpart2\tpart3", "多余字段不得丢");
    }

    /// ★ P4a-Y1：**本机那条跑的是同一条串** —— 命令只有一个构造点。
    ///
    /// 钉「有本机分支」很容易，钉不住「它跑的是同一条串」：本机臂里自己拼一句
    /// `cat ~/.cc-bus/agents.tsv` 照样绿，而那就是**第二份文件布局知识**，
    /// 两份会各自漂（这个仓管这叫「一段逻辑、两种表示」）。
    ///
    /// ⇒ 判据钉**构造点唯一**：三条读面命令各自的构造器在生产段只准出现一次
    /// （定义处不算），且生产段不许长出新的 `.tsv` 字面量。
    #[test]
    fn the_local_read_path_runs_the_very_same_command_string() {
        let code = non_test_code();
        for builder in ["build_online_cmd(&id)", "build_inbox_cmd(&id)"] {
            assert_eq!(
                code.matches(builder).count(),
                1,
                "`{builder}` 在生产段出现了不止一次 —— 多半是本机臂自己又构了一条命令。\n\
                 本机与远端必须用**同一个 `cmd`**：构造在分支之前，分支只决定谁来跑它。"
            );
        }
        // `.tsv` 只准出现在 `CC_BUS_CAT_CMD` 那个常量里（两次：agents / spawned）。
        assert_eq!(
            code.matches(".tsv").count(),
            2,
            "生产段出现了新的 `.tsv` 字面量 —— cc-bus 的文件布局只准有一份表示，\n\
             它住在 `CC_BUS_CAT_CMD` 里。本机要读同一批文件，就跑同一条串。"
        );
    }

    /// ★ D 阶段补审：**超时之后，那个子进程还在不在。**
    ///
    /// `local_shell_read` 的显式 `start_kill()` 只在**成功路径**上。超时那条路是
    /// `tokio::time::timeout` 把整个 future 丢掉 —— 而 tokio 的 `Child`
    /// **默认不因句柄被 drop 而杀掉子进程**（全仓 `kill_on_drop` 命中曾是 0）。
    ///
    /// ⇒ 一次超时留一个孤儿 `bash`。这与本会话在 `#60` 上栽的那次同族：
    /// **我自己留下的孤儿进程**把后面八轮实测全带偏了。那次的教训不是「以后小心」，
    /// 是「留一条判据去数它」。
    ///
    /// 本条真起进程、真等超时（阳性对照 ~0.2s ＋ 超时 ~1.4s）——
    /// 起的是一个 shell 内建的空转循环，不是任何会烧额度的东西。
    ///
    /// # 🔴 夹具换掉了 `sleep`，为什么〔ccbus-win 09-10〕
    ///
    /// 09-09 那版用 `sleep 30.<pid>` 当阻塞体，并加了一格「先证明 `sleep` 真的阻塞」的自检。
    /// **那一格 09-10 在云端 `windows-latest` 打中了**：`bash -lc 'sleep 2.8416'`
    /// 只花了 120.7456ms 就回来，且 `local_shell_read` 回的是 `Ok` ——
    /// 即 `bash` **起得来**，但那条 `sleep` 没有阻塞。
    ///
    /// ⇒ 那版夹具的阻塞性**是环境的函数**（`sleep` 是外部命令，要 PATH 上有它）。
    /// 本版换成 `while :; do : <marker>; done`：`while` / `:` 全是 **shell 内建**，
    /// 阻塞与否只取决于 bash 的语义，不取决于这台机器上装了什么。
    /// 本机实测（`/bin/bash -lc`）：进程存活、`pgrep -fc <marker>` 数到 **1**、
    /// `/proc/<pid>/cmdline` 逐字是 `/bin/bash -lc while :; do : <marker>; done`
    /// —— 即 bash **不会** exec 掉自己（`while` 不是 simple command），marker 留在 argv 上。
    ///
    /// # 剩下的那一格自检：**阳性对照**
    ///
    /// 内建阻塞体唯一会「不阻塞」的情形是 **bash 根本没跑我们这段脚本**
    /// （比如 `bash` 解析到的是 `C:\Windows\System32\bash.exe` 那个没装发行版的 WSL 存根）。
    /// 那一形用一条纯内建的 `printf` 探针直接量出来 —— 它同时是**产品面**的读数：
    /// `CC_BUS_CAT_CMD` 走的是同一个 `local_shell_read`、同一个 `bash -lc`。
    ///
    /// ⚠ **诚实边界**：这一格红 = 这台机器上本机 cc-bus 的读面根本用不了。
    /// 那时本条守的性质（超时不漏工作进程）**在这台机器上没人守** —— 这是事实，不是可以关掉的理由。
    ///
    /// ⚠ 空转循环会占满一个核约 1.4 秒。换来的是「阻塞性不再是环境的函数」，值这个价。
    ///
    /// # 🔴🔴 每一格自带上限，因为**挂死比红更坏**〔ccbus-win 09-10 第四拍〕
    ///
    /// 云端 run `34462442459`（`32d527c`，windows runner）逐字只留下两行有用的：
    /// ```text
    /// 09:52:09  test cc_bus::tests::a_timed_out_local_read_does_not_leave_an_orphan_behind
    ///           has been running for over 60 seconds
    /// 10:17:16  ##[error]The operation was canceled.
    /// ```
    /// **跑了 25 分钟没回来，是人工取消的**（同一台机器上一趟整个 `cargo test` 只用 4m26s）。
    /// 一条红的测试会告诉你哪里坏了；一条挂死的测试**什么都不说**，还把后面全部拖住。
    ///
    /// ⇒ 本条改成**逐格设限**：每一格自己带上限，超了带着**格号**炸。
    /// 病在哪当时没量到，所以这里**不猜**，只让下一趟能自己说出来。
    ///
    /// ⚠ **逐格设限盖不住什么**：`tokio::time::timeout` 只约束 **await 点**。
    /// 某一格里若是一个**同步**调用卡住（`Command::spawn` 自己 / `Path::exists`），
    /// current_thread 运行时上的计时器根本没机会跑。
    ///
    /// # 🔴🔴 那句「万一还挂，那本身就是一个读数」**兑现了**〔第五拍〕
    ///
    /// 云端 run `34468962797`（`4016d6a`）：`fmt`/`clippy` 全过、同一个 binary 里
    /// **别的测试 11:08:26 就全跑完了**，而本条 11:08:09 起、到 11:35:08 作业闸掐断 ——
    /// **独自跑了 27 分钟，零输出，`【格N】` 一个都没印出来**。
    /// ⇒ 没有任何一格的上限炸过 ⇒ **卡的是同步调用，不是 await**。
    ///
    /// ⇒ 本拍改成**整条跑在裸线程上、由主线程 `recv_timeout` 收一个结论**
    /// （形状抄 [`bounded`] —— 那是这几趟里唯一没被卡住的形状，不是巧合）。
    /// **主线程只等一个 `Result`，它不可能被子线程里的任何同步调用拖住** ⇒
    /// 无论卡在哪，本条都会在有界时间内给出**一个读数**：绿、红、或者
    /// 「整条超过 N 秒没回来」。
    ///
    /// ⚠ **为什么不是换 `flavor = "multi_thread"`**：那只能救「同步调用卡住」这一形，
    /// 救不了**运行时析构**那一形 —— Windows 上子进程 stdio 走 tokio 的 blocking 池，
    /// 而运行时 drop 时会等正在跑的 blocking 任务。裸线程这条把 `Runtime` 的**析构也包在
    /// 上限里面**，两形一起兜住。
    ///
    /// ⚠ 一条挂死的测试**不只是自己没读数**：它把同一趟里其余所有读数一起吃掉
    /// （那三趟里「生成物必须最新」与 vendor 那格 `cargo test` 一次都没跑到）。
    #[test]
    fn a_timed_out_local_read_does_not_leave_an_orphan_behind() {
        // 🔴 **整条的硬上限**。四格各自的上限加起来最坏 ~130s，这里给 210s 的外框：
        //    外框先炸就说明卡在四格**之外**（解析 bash / spawn / 运行时析构）。
        const TOTAL_SECS: u64 = 210;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("ccbus-orphan-judge".to_string())
            .spawn(move || {
                let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("起不了 tokio 运行时");
                    rt.block_on(orphan_judge_body());
                    // ⚠ `rt` 在这一行之后才 drop —— Windows 上子进程 stdio 走 blocking 池，
                    //    运行时析构会等正在跑的 blocking 任务，**析构本身也可能挂**。
                    //    所以它必须留在这条上限**里面**。
                    breadcrumb("四格全过，开始析构 tokio 运行时");
                }));
                breadcrumb("判据线程收尾（运行时已析构）");
                let _ = tx.send(out);
            })
            .expect("起不了判据线程");
        match rx.recv_timeout(std::time::Duration::from_secs(TOTAL_SECS)) {
            Ok(Ok(())) => {}
            // 子线程里的 panic 原样抬回来 —— 失败消息与从前逐字一致。
            Ok(Err(payload)) => std::panic::resume_unwind(payload),
            Err(e) => panic!(
                "整条判据超过 {TOTAL_SECS}s 没回来（{e:?}）。\n\
                 🔴 **这是硬上限炸的，不是被测性质失败** —— 别读成「超时漏了工作进程」。\n\
                 ⇒ 去日志里找 `[ccbus-orphan]` 那几行面包屑：**最后印出来的那一行\n\
                 就是它走到的最远处**，下一行要做的事就是卡住的那一步。\n\
                 （面包屑绕开了 libtest 的输出捕获直接写 fd 2，正是为挂死这一形准备的。）"
            ),
        }
    }

    /// 面包屑：**绕开 libtest 的输出捕获**，直接写进程的 fd 2。
    ///
    /// 🔴 为什么不是 `eprintln!`〔第五拍〕：`eprintln!` 走 `std::io::_eprint`，
    /// 它先看 libtest 装的那个**线程局部**捕获缓冲 ⇒ 只有**测试失败或成功**时才转印得出来。
    /// 而 09-10 云端那三趟正是**挂死**：既不失败也不成功，捕获里的东西一个字都到不了日志
    /// —— 那三趟合起来零输出，我们只能靠推。`std::io::stderr()` 的 `Write` 不经过那一层。
    ///
    /// ⇒ 这几行**不是给绿的时候看的**，是给「万一还挂」准备的：日志会停在
    /// 最后一条面包屑上，下一步就是卡住的那一步。
    ///
    /// ⚠ 还有第二重保险，两重是刻意叠的：本判据的正文跑在**自己起的那条裸线程**上，
    /// 而 libtest 的捕获是**线程局部**的 —— 那条线程上压根没装捕获。
    /// 两重都指望不上的话，我们就又回到「零输出只能靠推」那一趟了。
    fn breadcrumb(what: &str) {
        use std::io::Write;
        let mut err = std::io::stderr();
        let _ = writeln!(err, "[ccbus-orphan] {what}");
        let _ = err.flush();
    }

    /// 本判据的正文 —— 四格。被 [`a_timed_out_local_read_does_not_leave_an_orphan_behind`]
    /// 放在裸线程上跑，好让整条有一个不可能被同步调用拖住的硬上限。
    async fn orphan_judge_body() {
        // ★★ **阳性对照**：先证明这台机器的 `bash -lc` 真的跑了我们这段脚本、
        //    而且它的 stdout 真的回到了我们手上。**清一色内建**（`printf`），
        //    所以它量的是「壳活着吗」，不掺任何 PATH 的运气。
        //
        // 没有这一格时，两件完全不同的事在输出上**一模一样**：
        //   ① `bash` 是个存根 / 当场就退 ⇒ stdout 立刻 EOF ⇒ `local_shell_read` 回 `Ok("")`；
        //   ② 超时那一格真的失效了（本条要买的那一面）。
        const HELLO: &str = "CCBUS-SHELL-ALIVE";
        let probe = format!("printf %s {HELLO}");

        // ★ **先把两个同步嫌疑点拆开**〔第五拍〕：`local_shell_read` 里同步的只有两处 ——
        //   `resolve_bash()`（`env::var_os` + 最多 6 次 `Path::exists`）与 `Command::spawn()`。
        //   在这里先单独跑一次 `resolve_bash()` 并前后各留一条面包屑，
        //   下一趟即使还挂，日志也分得出是这两处里的哪一处。
        breadcrumb("格①之前：开始 resolve_bash()（同步：env::var_os + Path::exists）");
        let which = resolve_bash();
        breadcrumb(&format!("格①之前：resolve_bash() 回来了 -> {which:?}"));

        breadcrumb("进入格①·壳自检（下一步是同步的 Command::spawn）");
        // 内层 30s → 10s：一条 `printf` 回不来的话，多等 20 秒买不到任何东西。
        let alive = stage(
            "格①·壳自检",
            20,
            local_shell_read(&probe, 4096, 10, "壳自检", OnOverflow::Reject),
        )
        .await;
        breadcrumb("格①·壳自检回来了");
        // ⚠ 用 `contains` 不用逐字相等：`-l` 会过 `/etc/profile`，有的机器的 rc 会往
        //   stdout 上垫东西。垫东西不影响本格要证的事（脚本跑了、stdout 回得来），
        //   而逐字相等会把「rc 话多」误报成「壳是死的」。产品那侧同理，见 `take_head`。
        let got = alive.as_deref().unwrap_or("");
        // ★ `which` 在上面那条面包屑里已经拿到了 —— 它同时进错误消息，也同时进日志
        //   〔第二拍立、第五拍改成面包屑：挂死的时候错误消息根本印不出来〕。
        assert!(
            got.contains(HELLO),
            "【格①】阳性对照没回来（跑的是 `bash -lc '{probe}'`，全是内建）。实得：{alive:?}\n\
             解析到的 bash：{which:?}\n\
             ⇒ 这台机器上 `bash` 解析到的那个东西**根本没跑我们的脚本**\n\
             （Windows 上 `C:\\Windows\\System32\\bash.exe` 那个没装发行版的 WSL 存根\
             就是这一形）。\n\
             ⚠ 这不只是本判据的事：`CC_BUS_CAT_CMD` 走的是**同一个** `bash -lc` ——\n\
             这一格红 ⇒ 本机 cc-bus 的读面在这台机器上也读不了（那是产品面的事）。\n\
             ⚠ 同时意味着本条守的性质（超时不漏工作进程）在这台机器上**没人守**。"
        );

        // ★★ **格②：先量尺子本身，而且在起那个空转进程之前量**〔ccbus-win 09-10 第四拍〕。
        //
        // 拿一个**一定在**的针去问一次：本测试进程自己。它买两件事——
        //   ① 尺子答得出一个数（Windows 那半 09-10 之前从没真跑过，见 `count_live_processes`）；
        //   ② 它**答得及时**（那一趟 25 分钟没回来，而这条路上唯一无界的就是数进程那一格）。
        // ⚠ 顺序是刻意的：先量尺子、再起空转进程。反过来的话，尺子一卡，
        //   那个 100% 占核的空转进程就会陪着它一起烧到作业被掐。
        let me = std::env::current_exe().expect("【格②】拿不到本测试进程自己的路径");
        let stem = me.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        assert!(
            !stem.is_empty(),
            "【格②】本测试进程的文件名取不出来：{me:?}"
        );
        breadcrumb("进入格②·尺子自检（数进程，跑在它自己的裸线程上）");
        let seen_self = count_live_processes(stem, 45, "格②·尺子自检").await;
        breadcrumb(&format!("格②·尺子自检回来了 -> {seen_self}"));
        assert!(
            seen_self >= 1,
            "【格②】尺子连**本测试进程自己**都数不到（针=`{stem}`，实得 {seen_self}）。\n\
             ⇒ 下面那格的「0 个孤儿」是**空真** —— 不是没有孤儿，是尺子看不见东西。\n\
             （本仓最高频的那一类病：尺子的作用域对不上事实。）"
        );

        // ⚠ marker **不能只写在注释里**：`bash -lc '<单条 simple command>'` 会 **exec 掉自己**，
        // 于是注释从任何 cmdline 上都消失，`pgrep` 数到 0 ⇒ **判据假绿**（09-09 实测栽过一次）。
        // ⇒ 把 marker 放进那个必然存活的进程**自己的 argv** 里。
        // （`while` 循环不是 simple command，bash 不会 exec 掉自己 —— 本机现打验过。）
        let marker = format!("ccbus-orphan-{}", std::process::id());
        let cmd = format!("while :; do : {marker}; done");

        breadcrumb("进入格③·超时探针（下一步又是同步的 Command::spawn）");
        let r = stage(
            "格③·超时探针",
            20,
            local_shell_read(&cmd, 4096, 1, "超时探针", OnOverflow::Reject),
        )
        .await;
        breadcrumb("格③·超时探针回来了");
        assert!(
            r.is_err(),
            "【格③】1 秒上限跑一个内建死循环竟然没超时 —— 本判据在空转。实得：{r:?}"
        );
        // 给 tokio 的收尸队一点时间。
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        breadcrumb("进入格④·数孤儿");
        let n = count_live_processes(&marker, 45, "格④·数孤儿").await;
        breadcrumb(&format!("格④·数孤儿回来了 -> {n}"));
        // 只在**要红的时候**才多问一次：绿的那条路上一次都不问。
        let survivors = if n == 0 {
            String::new()
        } else {
            describe_live_processes(&marker, 45, "格④·倒出").await
        };
        assert_eq!(
            n, 0,
            "【格④】超时之后还留着 {n} 个子进程（marker={marker}）—— 每超时一次漏一个。\n\
             显式 `start_kill()` 只在成功路径上；超时那条要靠 `kill_on_drop(true)`。\n\
             留下来的是：\n{survivors}\n\
             ⚠ **这一格红有两种读法，别只挑顺手的那一种**〔ccbus-win 09-10 第四拍〕：\n\
             ㈠ 真漏 —— `local_shell_read` 超时那条路没把工作进程收干净（那是产品面的事）；\n\
             ㈡ **夹具的锅** —— Windows 上 `{which:?}` 若是个**外壳**（起完真 bash 自己就退），\n\
                `kill_on_drop` 杀掉的是外壳，真 bash 还在转。\n\
             ⚠ ㈡ **本轮没有任何读数**（宿主是 Linux，Windows 的进程树语义没人量过）。\n\
                分辨法就在上面那份清单里：留下来那个的 cmdline 是不是 `bash -lc while …` 本身，\n\
                它的 ppid 指向谁。**在量到之前别下结论。**"
        );
    }

    /// 给一格套一个自己的上限：超了带着**格号**炸，而不是把整条测试拖成挂死。
    ///
    /// 🔴 立项理由〔ccbus-win 09-10 第四拍〕：云端那趟本条**跑了 25 分钟没回来**，
    /// 日志里只有一句 "has been running for over 60 seconds"，之后是人工取消 ——
    /// 而当时 `ci.yml` 一条 `timeout-minutes` 都没有 ⇒ 不取消就烧到 GitHub 的 6 小时上限。
    /// **一条挂死的测试比一条红的更坏**：红的会说哪里坏了，挂死的什么都不说。
    ///
    /// ⚠ **它约束的只有 await 点**（跑在 current_thread 运行时上）：某一格里若是
    /// 同步调用卡住，计时器压根没机会跑 —— 09-10 云端那趟 27 分钟零输出，实测就是这一形。
    /// ⇒ 这一层**不是**万能的兜底，它只是把「已知会 await 的那几格」变成有名有姓的红。
    /// 真正的兜底在两处：数进程那一格自己的裸线程（[`count_live_processes`]），
    /// 以及**整条判据外面那个硬上限**（见
    /// [`a_timed_out_local_read_does_not_leave_an_orphan_behind`] 头一段）。
    async fn stage<T>(label: &str, secs: u64, f: impl std::future::Future<Output = T>) -> T {
        match tokio::time::timeout(std::time::Duration::from_secs(secs), f).await {
            Ok(v) => v,
            Err(_) => panic!(
                "【{label}】超过 {secs}s 没回来 —— 本条就挂在这一格上。\n\
                 ⚠ 这是**上限炸的**，不是被测性质失败 —— 别读成「性质不成立」。"
            ),
        }
    }

    /// 数「cmdline 里含 `needle` 的活进程」有几个 —— **两个平台各一把尺子，量同一件事**。
    ///
    /// POSIX 用 `pgrep -fc`；Windows 上**根本没有 `pgrep`**（Git for Windows 不带
    /// procps），那边问 WMI 的 `Win32_Process`。这不是把 Windows 那半关掉，
    /// 是给同一个量换一把这台机器上真存在的尺子。
    ///
    /// 🔴 **数不出来一律红，绝不回 0**〔09-09 收紧〕：原写法是 `.unwrap_or(0)`，
    /// 而「尺子坏了」与「一个孤儿都没有」在它下面**同形** —— 后者正是本判据要买的那一面。
    /// （`pgrep -fc` 零命中时打印 `0`、退出码非零 ⇒ 这一形照旧解析得出，POSIX 行为逐字不变。）
    ///
    /// ⚠ 诚实边界：Windows 那把尺子在交回本件时**没有在任何机器上跑过**
    /// （宿主是 Linux，且本件不许跑测试）。它坏掉的表现是**红**，不是绿。
    ///
    /// ⚠⚠ **09-10 那趟是它的第一次真跑**〔ccbus-win 第二拍预言、第四拍兑现〕：
    /// 09-09/09-10 前两趟都在阳性对照就红了，根本走不到这里；[`resolve_bash`] 落地之后
    /// 前两格过了，这一行才第一次真在 Windows 上跑 —— **然后整条测试 25 分钟没回来**。
    ///
    /// # 🔴 上限是本函数的一部分，不是调用方的自觉〔第四拍〕
    ///
    /// 病在哪**没量到**，所以这里不猜。但有一件事是确定的：`Command::output()`
    /// **没有超时形态**（`ccm_probe::probe_with` 的头注早就逐字记着这句），
    /// 而它是这条测试路径上**原先唯一无界的一格**。⇒ 上限收进来。
    ///
    /// ⚠ **超时是「红」，不是「0 个孤儿」** —— 口径与 09-09 那次收紧一个字不差：
    /// 「尺子答不上」与「一个孤儿都没有」在 `usize` 上同形，而后者正是本判据要买的那一面。
    ///
    /// ⚠ 为什么用**裸线程 + 轮询**，而不是 `spawn_blocking` + `timeout`：
    /// tokio 的运行时在 **drop 时会等正在跑的 blocking 任务跑完** ——
    /// 真卡住的话，我们 panic 完照样卡在运行时析构里，又变回一条挂死的测试。
    /// 裸线程漏掉就漏掉，进程退出时一起走。
    ///
    /// ⚠ **诚实边界**：本函数**没有**验证「尺子看得见一个 `bash` 子进程的 cmdline」——
    /// 它只验证「尺子答得出一个数」。调用点用「本测试进程自己」当针做了那一格自检
    /// （格②），那覆盖的是「尺子整个坏了 / 卡住」，**不**覆盖「看得见 exe、看不见 bash」。
    /// 要买那一格得有一台真 Windows，本轮宿主是 Linux。
    async fn count_live_processes(needle: &str, secs: u64, label: &str) -> usize {
        let n = needle.to_string();
        bounded(label, secs, "数进程", move || count_now(&n)).await
    }

    /// [`count_live_processes`] 的同步半 —— 真正去问这台机器的那一下。
    fn count_now(needle: &str) -> usize {
        let ps = format!(
            "@(Get-CimInstance Win32_Process | \
             Where-Object {{ $_.CommandLine -like '*{needle}*' }}).Count"
        );
        let (prog, argv): (&str, Vec<&str>) = if cfg!(windows) {
            ("powershell", vec!["-NoProfile", "-Command", ps.as_str()])
        } else {
            ("pgrep", vec!["-fc", needle])
        };
        let out = std::process::Command::new(prog)
            .args(&argv)
            .output()
            .expect("数进程那条命令起不来 —— 数不出来就不许当成绿");
        let raw = String::from_utf8_lossy(&out.stdout);
        let raw = raw.trim();
        let parsed = raw.parse::<usize>();
        assert!(
            parsed.is_ok(),
            "`{prog}` 没回出一个数（实得 {raw:?}）—— 数不出来就不许当成绿。\n\
             ⚠ 原写法 `.unwrap_or(0)` 会把「尺子坏了」读成「一个孤儿都没有」。"
        );
        parsed.expect("上面那条断言已经保证它是 Ok")
    }

    /// **只在失败那条路上用**：把匹配到的进程原样倒出来（pid / ppid / cmdline）。
    ///
    /// # 它为什么值这几行〔ccbus-win 09-10 第四拍〕
    ///
    /// 「超时之后还留着 1 个」有**两种**读法（真漏 / 夹具的锅，见格④那条断言），
    /// 而分辨它们只要一样东西：**留下来那个到底是谁**。没有它，下一趟的红仍然是
    /// 「留了 1 个，自己去查」——而「自己去查」在云端等于再烧一趟 CI。
    ///
    /// ⚠ **它是诊断，不是尺子**：这里的失败一律降级成一句话塞进消息里，
    /// **绝不 panic、绝不参与判定**。别把这份宽容读成 [`count_now`] 也可以宽容 ——
    /// 那一个数不出来必须红，口径一个字没变。
    async fn describe_live_processes(needle: &str, secs: u64, label: &str) -> String {
        let n = needle.to_string();
        bounded(label, secs, "倒出进程", move || {
            let ps = format!(
                "Get-CimInstance Win32_Process | \
                 Where-Object {{ $_.CommandLine -like '*{n}*' }} | ForEach-Object {{ \
                 ($_.ProcessId).ToString() + ' ppid=' + ($_.ParentProcessId).ToString() \
                 + ' ' + $_.CommandLine }}"
            );
            let (prog, argv): (&str, Vec<&str>) = if cfg!(windows) {
                ("powershell", vec!["-NoProfile", "-Command", ps.as_str()])
            } else {
                ("pgrep", vec!["-af", n.as_str()])
            };
            match std::process::Command::new(prog).args(&argv).output() {
                Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
                Err(e) => format!("（倒不出来：`{prog}` 起不来：{e}）"),
            }
        })
        .await
    }

    /// 在**裸线程**上跑一件会阻塞的活，并给它一个上限；超了带着格号 panic。
    ///
    /// 🔴 上限收在这里，不靠调用方自觉：`Command::output()` **没有超时形态**
    /// （`ccm_probe::probe_with` 头注早就逐字记着），而它是这条测试路径上
    /// **原先唯一无界的一格** —— 09-10 云端那趟整条测试 25 分钟没回来。
    ///
    /// ⚠ 为什么是**裸线程**而不是 `spawn_blocking` + `timeout`：tokio 的运行时
    /// **在 drop 时会等正在跑的 blocking 任务跑完** ⇒ 真卡住的话，我们 panic 完
    /// 照样卡在运行时析构里，又变回一条挂死的测试。裸线程漏掉就漏掉，进程退出时一起走。
    ///
    /// ⚠ 上限炸出来的是「**答不上**」，调用方**不许**把它读成一个具体的答案
    /// （数进程那处：超时 ≠「0 个孤儿」，两者在 `usize` 上同形，而后者正是判据要买的那一面）。
    async fn bounded<T: Send + 'static>(
        label: &str,
        secs: u64,
        what: &str,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(job());
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            match rx.try_recv() {
                Ok(v) => return v,
                // 发送端没了 = 那条线程里 panic 了（`count_now` 自己会 panic）。
                Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!(
                    "【{label}】{what}那条线程没把结果送回来（多半是它自己 panic 了，\
                     真因在它那条 panic 上）—— 答不上就不许当成绿。"
                ),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            assert!(
                std::time::Instant::now() < deadline,
                "【{label}】{what}超过 {secs}s 没回来。\n\
                 🔴 **这是「答不上」，不是一个答案** —— 数进程那处尤其要紧：\n\
                 「尺子答不上」与「一个孤儿都没有」在 `usize` 上同形，\n\
                 而后者正是本判据要买的那一面，绝不许拿一次超时冒充它。\n\
                 Windows 那半跑的是 `powershell -NoProfile -Command …Get-CimInstance…`，\n\
                 而 `Command::output()` 从来没有超时形态 —— 本上限就是为它加的\n\
                 （09-10 云端那趟整条测试 25 分钟没回来，这一格是当时唯一无界的一格）。"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// ★ 在线灯**必须先问身份空间**，不能只有那条按名字的老探法。
    ///
    /// ⚠ 变异实测：把「先问 daemon」那一步整个拿掉，上面那条纯函数判据**照样绿** ——
    /// 它钉的是"答案怎么算"，不是"有没有去问"。⇒ 这条钉接线本身（位置：问在前，探在后）。
    ///
    /// ⚠⚠ **本条的射程如实写**：它按**字面量位置**判，所以
    /// · 真删掉那一步 ⇒ **红**（实测）；
    /// · 把调用留在原地却不用它的结果（`if let Some(x) = None { … online_via_daemon(…) }`）
    ///   ⇒ **绿**（我自己第一次的变异恰好是这个形状，它溜过去了）。
    /// 后者要靠行为判据（真起 daemon 跑一遍）才逮得住，那归 daemon 侧那套 e2e。
    /// **写下来**是因为：不写的话，下一个人会以为这条比它实际能做的更强。
    #[test]
    fn the_online_lamp_asks_the_identity_space_before_probing_by_name() {
        let code = non_test_code();
        let at = code
            .find("pub async fn check_cc_bus_agent_online(")
            .expect("生产段找不到在线检查 —— 判据在空转");
        let rest = &code[at..];
        let end = rest[1..]
            .find("\npub ")
            .map(|k| k + 1)
            .unwrap_or_else(|| rest.len().min(1600));
        let body = &rest[..end];
        let ask = body
            .find("online_via_daemon(")
            .expect("在线灯没有先问身份空间 —— 那盏灯又变成「名字在就亮」了");
        let probe = body
            .find("build_online_cmd(")
            .expect("找不到老探法 —— 判据的参照物没了");
        assert!(
            ask < probe,
            "先按名字探、再问身份空间 ⇒ 名字被别人占着时那盏灯照样亮"
        );
    }

    /// ★★ **给 `agents.tsv` 加一列，不许打破读它的人**〔08-13〕。
    ///
    /// 今天为身份核对加了第 4 列（登记时的 pane 根进程 pid）。这张表有**四个读者**：
    /// 本解析器 · `cc-list` · `cc-broadcast` · `cc-kill`/`cc-agents`。
    /// 三个 shell 读者用 `read -r a b c`（多出的落进最后一个变量、且它们都不用它）；
    /// 本解析器用 `row_fields(line, 3)`，判的是 `len < want` ⇒ **多列照过**。
    ///
    /// ⇒ 兼容是**设计成立的**，不是碰巧 —— 但没有判据的话，下一个把它改成
    /// `f.len() != want` 的人不会知道自己拆掉了什么。这条钉住两个方向。
    #[test]
    fn adding_a_column_to_agents_tsv_does_not_break_the_reader() {
        // 老格式（3 列）——用户机器上那 86 行今天还是这个形状
        let (old, bad_old) = parse_agents_tsv(
            "a_cc	a_cc:0.0	2026-07-18T07:26:31-07:00
",
        );
        assert_eq!(old.len(), 1, "老三列行读不出来了");
        assert_eq!(bad_old, 0);
        assert_eq!(old[0].pane, "a_cc:0.0");
        // 新格式（4 列，第 4 列是 pane 根进程 pid）
        let (new, bad_new) = parse_agents_tsv(
            "b_cc	b_cc:0.0	2026-08-13T00:00:00-07:00	12345
",
        );
        assert_eq!(new.len(), 1, "四列行被当成坏行了 —— 加一列就把驾驶舱清空了");
        assert_eq!(bad_new, 0);
        assert_eq!(new[0].id, "b_cc");
        assert_eq!(
            new[0].registered_at, "2026-08-13T00:00:00-07:00",
            "多出的那列串进了时间戳"
        );
        // 两种混在一起也要都认（迁移期的真实形状）
        let (both, bad_both) = parse_agents_tsv(
            "a_cc	a_cc:0.0	ts
b_cc	b_cc:0.0	ts	12345
",
        );
        assert_eq!(both.len(), 2, "新老混排时丢了行");
        assert_eq!(bad_both, 0);
        // 字段**不够**仍要算坏行（别把"宽容多列"做成"什么都收"）
        let (short, bad_short) = parse_agents_tsv(
            "c_cc	c_cc:0.0
",
        );
        assert!(short.is_empty());
        assert_eq!(bad_short, 1, "两列行该算坏行");
    }

    /// ★ 在线灯：**「问不到」不许渲染成「不在线」**。
    ///
    /// 老探法是 `tmux has-session -t '=<id>:'`（纯按名字）——名字被别人占着时它照样说在线。
    /// 换成问 `bus-list` 之后，`None` 有两种来源（不在名单 / `live` 是 null），
    /// **两种都要回落到老探法**，而不是直接灭灯：灭灯是一个确定的答案，而我们并不确定。
    #[test]
    fn the_online_lamp_falls_back_instead_of_guessing_dark() {
        use serde_json::json;
        let agents = vec![
            json!({"id": "a_cc", "live": true}),
            json!({"id": "b_cc", "live": false}),
            json!({"id": "c_cc", "live": null}),
        ];
        assert_eq!(live_of(&agents, "a_cc"), Some(true));
        assert_eq!(
            live_of(&agents, "b_cc"),
            Some(false),
            "确定不在线要答得出来"
        );
        assert_eq!(
            live_of(&agents, "c_cc"),
            None,
            "live=null 是「问不到」，不是「不在」"
        );
        assert_eq!(
            live_of(&agents, "nobody_cc"),
            None,
            "不在名单里也是「答不上」"
        );
    }

    /// ★★ **广播不许再打进幽灵收件箱**〔P4f 08-13，用户机器上实测出来的〕。
    ///
    /// 老路（`cc-broadcast` 脚本）发给 `agents.tsv` 的**每一行**。用户机器实测：
    /// **86 行登记、只有 8 个会话还活着** ⇒ 一次广播打进 **78 个没人读的收件箱**，
    /// 而它报「已向 86 个 agent 发出广播」—— 那个数把 78 个幽灵也算了进去。
    #[test]
    fn broadcast_only_goes_to_the_ones_that_are_actually_there() {
        use serde_json::json;
        let agents = vec![
            json!({"id": "a_cc", "live": true}),
            json!({"id": "b_cc", "live": false}),
            json!({"id": "c_cc", "live": true}),
            json!({"id": MONITOR_BUS_ID, "live": true}),
        ];
        let plan = pick_broadcast_targets(&agents, MONITOR_BUS_ID);
        assert_eq!(plan.targets, vec!["a_cc", "c_cc"], "只该发给活着的");
        assert_eq!(plan.skipped_offline, 1, "不在线的要计数，不是悄悄丢掉");
        assert!(!plan.liveness_unknown);
        assert!(
            !plan.targets.iter().any(|t| t == MONITOR_BUS_ID),
            "不发给自己"
        );
    }

    /// ★ **「问不到」不等于「都不在」**。
    ///
    /// 身份空间答不上时（没装 tmux 等，`live` 全是 `null`），退回「发给所有登记的」——
    /// 若问不到就谁都不发，用户会看到一次「已广播给 0 个」，那是**把不知道渲染成了确定**。
    #[test]
    fn unknown_liveness_does_not_silently_become_nobody() {
        use serde_json::json;
        let agents = vec![
            json!({"id": "a_cc", "live": null}),
            json!({"id": "b_cc", "live": null}),
        ];
        let plan = pick_broadcast_targets(&agents, MONITOR_BUS_ID);
        assert_eq!(plan.targets.len(), 2, "问不到时不许把人全滤掉");
        assert!(
            plan.liveness_unknown,
            "而且要**标出来**是问不到，不是装作知道"
        );
        assert_eq!(plan.skipped_offline, 0);
        let said = describe_broadcast(&plan, 2, &[]);
        assert!(said.contains("问不到谁在线"), "话没说清：{said}");
    }

    /// ★ 三个数**分开说**：发到几个 / 跳过几个 / 失败几个。
    #[test]
    fn the_broadcast_wording_keeps_the_three_counts_apart() {
        let plan = BroadcastPlan {
            targets: vec!["a_cc".into(), "b_cc".into()],
            skipped_offline: 78,
            liveness_unknown: false,
        };
        let said = describe_broadcast(&plan, 1, &["b_cc（超时）".to_string()]);
        assert!(said.contains("1 个**在线**"), "{said}");
        assert!(said.contains("跳过 78 个"), "跳过的没说：{said}");
        assert!(said.contains("1 个失败"), "失败的没说：{said}");
        // 老路那句话的形状（把所有人算成一个 N）不许回来
        assert!(!said.contains("已向 79"), "又把跳过的算进总数了：{said}");
    }

    /// ★★ **三态在线不许在讲人话这一层被抹平**〔P4f 08-13，变异 M2 逼出来的〕。
    ///
    /// daemon 的 `bus-send` 回 `registered` + 三态 `live`，而 UI 拿到的是一句话。
    /// 变异实测：把 `describe_send_reply` 改成恒说「已投递给 X」——**60 条测试全绿**。
    /// 也就是说那三态一路传到最后一米，然后被一句话吃掉，没有任何东西看着。
    ///
    /// 「发出去了」和「发出去了但没人会读」对用户是两件事：后者要么名字打错了，
    /// 要么对方不在线（消息躺在收件箱里等它下次起来）。
    #[test]
    fn the_delivery_wording_keeps_the_three_states_apart() {
        use serde_json::json;
        let say = |v: serde_json::Value| describe_send_reply("proj_cc", Some(&v));
        let ok = say(json!({"registered": true, "live": true}));
        let offline = say(json!({"registered": true, "live": false}));
        let ghost = say(json!({"registered": false, "live": null}));
        let unknown = say(json!({"registered": true, "live": null}));
        for (a, b, why) in [
            (&ok, &offline, "「在线」与「不在线」"),
            (&ok, &ghost, "「在线」与「名字没登记过」"),
            (&offline, &ghost, "「不在线」与「名字没登记过」"),
            (&ok, &unknown, "「在线」与「问不到在不在线」"),
        ] {
            assert_ne!(a, b, "{why} 说的是同一句话 —— 三态被抹平了");
        }
        // 各自要点到实处（不是只要求"不一样"就行）
        assert!(offline.contains("不在线"), "{offline}");
        assert!(ghost.contains("没在总线上登记过"), "{ghost}");
        assert!(unknown.contains("问不到"), "{unknown}");
        // 四种都得说「已投递」—— 投递是照做的，三态只是附加说明
        for m in [&ok, &offline, &ghost, &unknown] {
            assert!(m.contains("已投递"), "投递本身没说清：{m}");
        }
    }

    /// ★ P4a-Y2：**写面两条必须在 `cfg_of` 之前就把本机挡掉。**
    ///
    /// ⚠ 本条是**变异逼出来的**：M5（拿掉 `cc_bus_send` 的本机拒绝）第一次跑
    /// **照样绿** —— 因为 `local_origin_registry` 只扫**直接**调
    /// `load_remote_config_by_label(` 的地方，而这两条走的是 `cfg_of` 这个**包装**。
    /// ⇒ 那条护栏对「隔了一层包装」是瞎的（已在它的头注里登记）。
    ///
    /// 钉**位置**而不是「有没有这句话」：本机分支必须在 `cfg_of` 之前，
    /// 否则用户拿到的是 `cfg_of` 那句通用话，而不是这条路真实的说法。
    ///
    /// ⚠⚠ **08-13 P4f 改过一次口径**：本条原来钉的是 `refuse_local_write(` 这个**写法**。
    /// 而 `cc_bus_send` 的本机路今天**不再是拒绝** —— 它走 daemon 的 `bus-send` 原语
    /// （拒绝理由逐字写着「等命令组件做出来」，那些组件做出来了）。
    /// ⇒ 钉的东西从「有没有那句拒绝」改成**「本机分支在不在 `cfg_of` 前面」**：
    /// 前者是实现，后者才是这条判据真正要保的性质。
    /// **两种形态都算数**：`refuse_local_write(` 或 `== LOCAL_ORIGIN` 的早返回。
    #[test]
    fn the_write_face_branches_on_local_before_it_asks_for_a_remote_config() {
        let code = non_test_code();
        let mut checked = 0usize;
        for (name, what) in [
            // ⚠ 这里的"说清在做什么"必须是**代码里**的词（`non_test_code` 剥注释）：
            //   `cc_bus_send` 的本机分支是一次真调用，名字自己就说清了；
            //   `cc_bus_spawn` 仍是拒绝，说清的是拒绝文案里那句。
            ("pub async fn cc_bus_send(", "send_via_local_daemon"),
            ("pub async fn cc_bus_spawn(", "spawn 一个 agent"),
        ] {
            let at = code
                .find(name)
                .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
            // ⚠⚠ **窗口要按函数边界截**〔08-13 当场撞到〕：原来是「从函数名起取 1400 字」，
            //   而 `cc_bus_send` 比 1400 字短 ⇒ 窗口**越进了下一个函数**
            //  （`cc_bus_broadcast`），把邻居的 `refuse_local_write(` 当成了自己的，
            //   于是位置比较拿到的是**别人的**那处，判据当场误红。
            //   ★ 这正是本判据头注自己警告过的「块粒度」病 —— 而它发生在判据脚下。
            let rest = &code[at..];
            let end = rest[1..]
                .find("\npub async fn ")
                .or_else(|| rest[1..].find("\npub fn "))
                .map(|k| k + 1)
                .unwrap_or_else(|| rest.len().min(1400));
            let body: String = rest[..end].to_string();
            // 本机分支有**两种形态**（早返回走 daemon / 拒绝），取**先出现**的那个位置。
            let refuse = [
                body.find("refuse_local_write(&origin, \""),
                body.find("origin == crate::inbound_client::LOCAL_ORIGIN"),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(|| {
                panic!("{name} 没有本机分支 —— `<local>` 会掉进 `cfg_of` 拿到一句通用话")
            });
            let cfg = body
                .find("cfg_of(&origin)")
                .unwrap_or_else(|| panic!("{name} 里找不到 `cfg_of(&origin)` —— 判据的参照物没了"));
            assert!(
                refuse < cfg,
                "{name} 的本机拒绝排在 `cfg_of` **后面** —— 那就永远走不到，\n\
                 用户看到的仍是「远端 `<local>` 未配置或未启用」。"
            );
            assert!(
                body[refuse..].contains(what),
                "{name} 的本机分支没说清它在做什么（应含 {what:?}）——\n\
                 一句不说清是哪件事的错误，与那句「未找到远端配置」是同一族。"
            );
            checked += 1;
        }
        assert_eq!(checked, 2, "只核到 {checked} 条写面命令 —— 本断言在空转");
    }

    /// ★ P4c-Y1：两条新命令的**构造器**逐条打校验。
    ///
    /// ⚠ 只测 happy path 是不够的 —— 那正是 `cc_bus_send` 那次变异实测的教训：
    /// 「断言测的是谓词本身，而不是**命令构造真的调了它**」。
    #[test]
    fn broadcast_and_kill_commands_validate_before_they_build() {
        // 广播：空消息不许构造出命令（对 86 个 agent 发一条空消息是纯噪声）。
        assert!(build_broadcast_cmd("").is_err());
        assert!(build_broadcast_cmd("   ").is_err());
        let ok = build_broadcast_cmd("hi there").expect("正常消息该能构造");
        assert!(ok.starts_with("cc-broadcast "), "命令名不对: {ok}");
        // 文本必须过引用 —— 否则一条带引号的消息就能拼出别的命令。
        //
        // ⚠ 第一版我禁的是 `"; rm -rf /'"` 这个子串 —— **那条断言本身是错的**：
        // `shell_quote` 的正确产物就是 `'a'\''b; rm -rf /'`，它合法地以 `/'` 结尾。
        // ⇒ 钉「引用**真的发生了**」：内嵌单引号被转义成 `'\''`，那是 POSIX 单引号法的指纹。
        let quoted = build_broadcast_cmd("a'b; rm -rf /").expect("该能构造");
        assert!(
            quoted.contains("'\\''"),
            "文本没过 shell_quote（内嵌单引号没被转义）: {quoted}"
        );
        // 且危险字符全在引号里 —— 命令名之后只有一个 shell 词。
        assert!(
            quoted.starts_with("cc-broadcast '") && quoted.ends_with("' 2>&1"),
            "载荷不是一个被完整引起来的词: {quoted}"
        );

        // 收掉：非法 id 拒绝构造。**不能靠对端校验** —— 这一条的后果是杀掉一棵进程树。
        for bad in ["", "a b", "--help", "x;y", "../etc"] {
            assert!(
                build_kill_cmd(bad).is_err(),
                "非法 id {bad:?} 竟然构造出了命令 —— 它会被拼进 `cc-kill` 的命令串"
            );
        }
        assert_eq!(build_kill_cmd("proj_cc").unwrap(), "cc-kill proj_cc 2>&1");
    }

    /// ★ P4c：两条新命令与既有写面**同样**对 `<local>` 诚实拒绝。
    ///
    /// 钉**位置**：拒绝必须在 `cfg_of` 之前（同 `P4a-Y2` 的理由 —— 排在后面就永远走不到）。
    #[test]
    fn the_new_write_commands_refuse_local_before_asking_for_a_remote_config() {
        let code = non_test_code();
        // ⚠ 与上一条同口径〔08-13 P4f 两次改口径〕：钉的是**本机分支在 `cfg_of` 之前**
        //   这条性质，不是 `refuse_local_write(` 这个写法 —— `cc_bus_broadcast` 的本机路
        //   今天是「走 daemon 组合」，不是拒绝。窗口同样按**函数边界**截，
        //   否则会读到邻居的分支（那正是这两条判据自己栽过的坑）。
        for (name, what) in [
            ("pub async fn cc_bus_broadcast(", "broadcast_via_daemon"),
            ("pub async fn cc_bus_kill(", "收掉 agent"),
        ] {
            let at = code
                .find(name)
                .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
            let rest = &code[at..];
            let end = rest[1..]
                .find("\npub async fn ")
                .or_else(|| rest[1..].find("\npub fn "))
                .map(|k| k + 1)
                .unwrap_or_else(|| rest.len().min(1200));
            let body: String = rest[..end].to_string();
            let refuse = [
                body.find("refuse_local_write(&origin, \""),
                body.find("origin == crate::inbound_client::LOCAL_ORIGIN"),
            ]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_else(|| panic!("{name} 没有本机分支"));
            let cfg = body
                .find("cfg_of(&origin)")
                .unwrap_or_else(|| panic!("{name} 里找不到 `cfg_of(&origin)`"));
            assert!(
                refuse < cfg,
                "{name} 的本机分支排在 `cfg_of` 后面 —— 永远走不到"
            );
            assert!(
                body[..refuse].contains(what) || body[refuse..].contains(what),
                "{name} 的本机分支没说清它在做什么（应含 {what:?}）"
            );
        }
    }

    /// ★★ **解析 `bash` 的地方恰好一处，而且那一处不是「按裸名让操作系统猜」**〔ccbus-win 09-10〕。
    ///
    /// # 它买的是什么
    ///
    /// 09-10 云端那条红的根因是 `Command::new("bash")`：Windows 的进程创建把
    /// `C:\Windows\System32` 排在 `PATH` 之前，而那里有一个 WSL 存根
    /// （`actions/runner-images` #12646）。⇒ **裸名这一形本身就是缺陷**，
    /// 不是「今天恰好没配好」。本条把它从生产段里彻底赶出去。
    ///
    /// 第二半治的是「一段逻辑三种表示」：解析点一多，就会有人在第二处写个略有不同的候选表，
    /// 而两份候选表会各自漂 —— 与 `local_shell_read` 头注里那条「一段逻辑、两种表示」同族。
    ///
    /// # 🔴 铁律 12：修之前它一定红（静态推演，跑不了测试所以逐条推）
    ///
    /// 本件之前，`local_shell_read` 的函数体逐字含
    /// `let mut child = tokio::process::Command::new("bash")`，而生产段里
    /// **没有任何** `fn resolve_bash` ⇒
    /// · 第一条（`Command::new("bash")` 计数为 0）实得 1 ⇒ **红**；
    /// · 第二条（`fn resolve_bash(` 恰好 1 处）实得 0 ⇒ **红**；
    /// · 第四条（`local_shell_read` 窗口里有 `resolve_bash()?`）实得没有 ⇒ **红**。
    ///
    /// # ⚠ 它的射程（写下来，别读成比它强）
    ///
    /// 本条只看**本文件**。全仓另外两处 `bash` 起进程（`ccm_probe::probe_with`、
    /// `launch.rs` 那条 `-lic` 的下游）**在 Windows 上根本不编译**，裸名在 POSIX 上是对的
    /// ⇒ 本轮刻意不动它们。但「全仓不许有 Windows 够得到的裸名 bash」这条**仓级**判据
    /// 今天**没有人立** —— 它的正确落点是 `write_site_registry::spawn_sites`
    /// （那张表已经按 `Command::new(` 派生人群），不在本件写区。**已上报，别当它有。**
    #[test]
    fn the_bash_cc_bus_runs_is_resolved_in_exactly_one_place() {
        let code = non_test_code();
        assert_eq!(
            code.matches(concat!("Command::", "new(\"bash\")")).count(),
            0,
            "生产段又出现了按裸名起 bash —— Windows 上那会拿到 System32 里的 WSL 存根\n\
             （进程创建把系统目录排在 PATH 之前，PATH 怎么排都没用）。走 `resolve_bash()`。"
        );
        assert_eq!(
            code.matches("fn resolve_bash(").count(),
            1,
            "解析 `bash` 的生产取值口不是恰好一处 —— 两份候选表会各自漂"
        );
        assert_eq!(
            code.matches("fn resolve_bash_with(").count(),
            1,
            "纯函数半不是恰好一处"
        );
        // 平台那一格必须**复用**既有的唯一真相源，不许在本文件里再写一份 `cfg!(windows)`
        // —— `history::platform_is_windows` 的头注逐字写着「只有这一处说得出这句话」，
        // 而写在调用点上的 `cfg!(windows)` 是常量表达式、判据翻不动它（那次的刀实测全绿）。
        assert!(
            code.contains("crate::history::platform_is_windows()"),
            "平台那一格没走 `history::platform_is_windows` —— 本文件自己写 `cfg!(windows)` \n\
             会让判据翻不动它，而且那句话就有了第二个家。"
        );
        assert_eq!(
            code.matches(concat!("cfg!(", "windows)")).count(),
            0,
            "本文件自己写了 `cfg!(windows)` —— 那句话只准有一个家"
        );
        let at = code
            .find("async fn local_shell_read(")
            .expect("生产段找不到本机执行口 —— 判据在空转");
        let body: String = code[at..].chars().take(1200).collect();
        assert!(
            body.contains("resolve_bash()?"),
            "本机执行口没有先解析 `bash` —— 判据的参照物没了。实得窗口：{body}"
        );
    }

    /// ★★ **找不到 `bash` 要响亮地失败，不许退化成「读到空」**〔ccbus-win 09-10〕。
    ///
    /// 那正是本件上半场那个缺陷换个地方重演：一次读不到，被渲染成「一个 agent 都没有」。
    ///
    /// # 为什么这条在 Linux 上也跑得到（这是刻意设计的）
    ///
    /// `resolve_bash_with` 把**平台 / 环境 / 盘上有没有**三样全收成入参。若写成
    /// `#[cfg(windows)]`，本条在 Linux 上就一格都量不到，而本仓 `launch.rs` 头注逐字记着
    /// 那次教训：`cfg!(windows)` 是常量表达式，刀「`cfg!(windows)` → `false`」实测**全绿**。
    ///
    /// # 🔴 铁律 12：修之前它一定红
    ///
    /// 本件之前 `resolve_bash_with` **根本不存在** ⇒ 编译不过 ⇒ 红。
    /// 而更要紧的是**行为**：那时 `local_shell_read` 拿到的是裸 `"bash"`，
    /// 「这台机器上没有可用的 bash」这一形**根本没有任何代码路径会回 `Err`**
    /// —— 它会成功起一个存根，然后回 `Ok("")`。⇒ 第一格要的那个 `Err` 当时造不出来。
    ///
    /// # 反向：不许恒错
    ///
    /// 第二格（候选表里有一条真在盘上）与第四格（POSIX）都要求 `Ok` ——
    /// 一个「一律报错」的糊涂修法在那两格当场红。
    #[test]
    fn a_bash_that_cannot_be_found_is_a_loud_error_not_a_silent_empty_read() {
        use std::ffi::OsString;
        use std::path::{Path, PathBuf};
        let no_env = |_: &str| -> Option<OsString> { None };
        let nothing_exists = |_: &Path| false;

        // ① Windows + 一条候选都不在盘上 ⇒ **响亮失败**，且说得出「不是没有 agent」。
        let e = resolve_bash_with(true, &no_env, &nothing_exists)
            .expect_err("找不到 bash 必须是错，不是一个能跑的裸名");
        assert!(e.contains("找不到可用的 `bash`"), "{e}");
        assert!(
            e.contains("不是"),
            "错误没写明它不是「一个 agent 都没有」—— 那就是上半场那个缺陷换个地方重演：{e}"
        );
        assert!(
            e.contains(BASH_OVERRIDE_VAR),
            "响亮失败没给逃生口 —— 那就从「诚实」变成了「装了也用不了」：{e}"
        );

        // ② **非空对照**：候选表里那条有读数的路径真在盘上 ⇒ 挑中它（证明本条不恒错）。
        let git_bash = r"C:\Program Files\Git\bin\bash.exe";
        let only_git = |p: &Path| p.to_string_lossy() == git_bash;
        let picked = resolve_bash_with(true, &no_env, &only_git).expect("盘上有就该挑中");
        assert_eq!(picked, OsString::from(git_bash));

        // ③ 逃生口指到系统目录里那个存根 ⇒ **拒绝**（那正是根因本身）。
        let stub = r"C:\Windows\System32\bash.exe";
        let env_stub = |k: &str| (k == BASH_OVERRIDE_VAR).then(|| OsString::from(stub));
        let all_exist = |_: &Path| true;
        let e3 = resolve_bash_with(true, &env_stub, &all_exist).expect_err("存根必须被拒");
        assert!(e3.contains("WSL"), "拒了但没说清拒的是什么：{e3}");
        // 候选表里若混进同一个门牌，也一样挡住（不只逃生口那一条路）。
        assert!(is_system_dir_bash(&PathBuf::from(stub)));
        // 大小写与正斜杠都要认（`SysWOW64` / `Sysnative` 是同一个目录的另外两个门牌）。
        let syswow = PathBuf::from(r"C:/WINDOWS/SysWOW64/bash.exe");
        assert!(is_system_dir_bash(&syswow));
        assert!(!is_system_dir_bash(&PathBuf::from(git_bash)));

        // ④ **非空对照**：POSIX 上裸名是对的，不许被这条改坏
        //    （`execvp` 只查 PATH；写死 `/bin/bash` 会在 NixOS/Homebrew 上当场坏掉）。
        let posix = resolve_bash_with(false, &no_env, &nothing_exists).expect("POSIX 不该失败");
        assert_eq!(posix, OsString::from("bash"));

        // ⑤ 逃生口指了一个不存在的路径 ⇒ 报错，**不许悄悄回落到候选表**
        //    （回落会让用户以为自己指的那条生效了）。
        let missing = r"D:\nope\bash.exe";
        let env_missing = |k: &str| (k == BASH_OVERRIDE_VAR).then(|| OsString::from(missing));
        let e5 = resolve_bash_with(true, &env_missing, &only_git).expect_err("指错了要说");
        assert!(e5.contains(missing), "{e5}");

        // ⑥ 候选表**每一条都是绝对路径**——一个裸名都不许有。
        //    ⚠ 不能用 `Path::is_absolute()`：它在 Linux 上按 POSIX 判，
        //    会把 `C:\…` 判成相对路径，于是这一格在 Linux 上恒真、等于没测。
        let env_win = |k: &str| match k {
            "ProgramFiles" => Some(OsString::from(r"C:\Program Files")),
            "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\u\AppData\Local")),
            _ => None,
        };
        let cands = windows_bash_candidates(&env_win);
        let n = cands.len();
        assert!(n >= 3, "候选表只剩 {n} 条 —— 抽取器坏了");
        // 去重真的发生了：`%ProgramFiles%` 展开出来的与写死那条**就是**同一个，
        // 留着重复会让「找过了哪些」那份清单当着用户的面说两遍同一句话。
        let mut uniq = cands.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), n, "候选表里有重复项");
        for c in &cands {
            let s = c.to_string_lossy().into_owned();
            assert!(
                s.contains(":\\") || s.starts_with("\\\\"),
                "候选 {s:?} 不是绝对路径 —— 裸名会被系统目录抢走，那正是本件的病根"
            );
            assert!(s.ends_with("bash.exe"), "候选 {s:?} 指的不是 bash.exe");
        }
        assert!(
            cands.iter().any(|c| c.to_string_lossy() == git_bash),
            "候选表里没有 `{git_bash}` —— 那是唯一一条有读数的路径\n\
             （09-10 云端 run 的日志里 runner 自己给 bash 步骤的 shell 逐字就是它）"
        );
    }

    /// ★ P4a-Y3：**本机那条执行口，远端有的守卫一件都不许少。**
    ///
    /// 本机看着「自家文件、能出什么事」—— 但 `agents.tsv` / `inbox` 都是只增文件，
    /// 而 monitor 与它跑在**同一台机器**上，撑爆的是用户正在用的那个进程。
    ///
    /// ⇒ 逐件对着远端那条核：上限（多读一字节才分得清「刚好满」与「其实还有」）·
    /// 超时 · 溢出两档各自有处置。
    #[test]
    fn the_local_cc_bus_read_keeps_every_guard_the_remote_one_has() {
        let code = non_test_code();
        let body = |name: &str| -> String {
            let at = code
                .find(name)
                .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
            // 取到下一个顶层 `}` 之后一点，够覆盖函数体即可。
            code[at..].chars().take(2600).collect()
        };
        let local = body("async fn local_shell_read(");
        let remote = body("async fn exec_read(");
        for (what, needle) in [
            ("上限（多读一字节）", "take(cap + 1)"),
            ("超时", "tokio::time::timeout"),
            ("溢出·拒收", "OnOverflow::Reject"),
            ("溢出·截断", "OnOverflow::Truncate"),
        ] {
            assert!(
                remote.contains(needle),
                "远端那条 `exec_read` 里找不到{what}（`{needle}`）—— 本判据的参照物没了，它此刻在空转"
            );
            assert!(
                local.contains(needle),
                "本机那条 `local_shell_read` 缺{what}（`{needle}`）。\n\
                 远端有而本机没有 = 「本地 = 不走 ssh 的远端」这句话在这一格是假的。"
            );
        }
    }

    /// **重要-6 的守卫**：「定值命令零插值」此前断言打在常量上，
    /// 没有任何东西守「`fetch_remote_cc_bus` 原样把它交出去」。往里塞一个 `format!` 就穿了。
    #[test]
    fn cat_command_reaches_ssh_unmodified() {
        // **断言打在调用点**：光断言 `CC_BUS_CAT_CMD` 这个常量长得干净不够
        // （B03 审计重要-6），得守住"它原样到达 SSH"——往中间塞一层 format! 就穿了。
        // **不能**断言"函数体内没有 format!"：那过宽，错误消息用 format! 是正当的
        // （我第一版就是这么写的，当场假红）。
        let code = non_test_code();
        assert!(
            code.contains("connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD)"),
            "定值命令必须原样交给 SSH（不得包 format!/push_str）"
        );
        // ★ P4a（08-12）：**从「那一处长这样」升成「每一处都是原样传参」**。
        //
        // 本条原来钉的是一个**硬编码的调用形状**（`connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD)`）
        // 加一个计数 2。P4a 给本机加了第二条传输路（同一条串，只是不包进 ssh）之后，
        // 计数当场红 —— **那是对的**，它逐字问的正是「是否多了第二条构造路径」。
        // 但答案是「多了第二条**传输**路、串没变」，⇒ 光把 2 改成 3 会让本条退回
        // 「只盯着远端那一处」：本机那处塞个 `format!` 它照样绿。
        //
        // 现在逐处核**用法**：每一次出现要么是定义处，要么是**裸着当实参传**
        // （前面是 `(`/`,`/空白，后面是 `,`/`)`）。拼接、`format!`、`push_str` 都会破坏这个形状。
        let mut sites = 0usize;
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("CC_BUS_CAT_CMD") {
            let at = from + rel;
            let end = at + "CC_BUS_CAT_CMD".len();
            from = end;
            let before = code[..at].chars().next_back().unwrap_or(' ');
            let after = code[end..].chars().next().unwrap_or(' ');
            // 定义处：`const CC_BUS_CAT_CMD: &str = …`
            if after == ':' {
                continue;
            }
            sites += 1;
            assert!(
                matches!(before, '(' | ',' | ' ' | '\n' | '\t'),
                "`CC_BUS_CAT_CMD` 第 {sites} 处用法前面是 {before:?} —— 它被拼进了别的东西，\n\
                 而本条的全部意义是「定值命令原样到达执行口」。"
            );
            assert!(
                matches!(after, ',' | ')'),
                "`CC_BUS_CAT_CMD` 第 {sites} 处用法后面是 {after:?} —— 同上，它没有裸着当实参传。"
            );
        }
        // 计数仍然守着「有没有人新开一条路」——只是现在它不再是唯一的防线。
        assert_eq!(
            sites, 2,
            "传输路条数变了（今天两条：远端 ssh + 本机 bash）。\n\
             新增一条要回来改这个数，并确认它也是**原样传参**。"
        );
    }

    #[test]
    fn inbox_missing_fields_degrade_not_panic() {
        let (m, sk) = parse_inbox_jsonl(r#"{"text":"orphan"}"#);
        assert_eq!(sk, 0);
        assert_eq!(m[0].from, "");
        assert_eq!(m[0].text, "orphan");
    }

    /// `P4a2`：「为什么这条读面还没走 daemon」的**读数**必须留在代码里。
    ///
    /// # 为什么这段散文值得一条判据
    ///
    /// 它挡的是**重复摸底**：这个问题（「省一次握手不好吗」）已经被问过两轮，
    /// 而两轮的结论都靠一个**具体数**（180ms）与一个**具体代价**（把正要变的文件格式
    /// 焊进 daemon）。删掉那个数，下一个人只能靠感觉重答一遍。
    ///
    /// ★ 更要紧的是钉住**解锁条件的实质**：`P4a2` 原文写的是「`P4b` 落地之后」，
    /// 而 `P4b` 08-12 已签收 —— 照字面读就该开工了。但它只删掉了 cc-spawn 的复用判定，
    /// **`agents.tsv` 的格式契约一字未动**。⇒ 判据钉「解锁条件不是『P4b 落地』」这句话在。
    #[test]
    fn why_the_read_face_is_not_on_the_daemon_yet_stays_measured() {
        let prod = guard_core::production_source(include_str!("cc_bus.rs"));
        for needle in ["180ms", "格式契约稳下来", "解锁条件不是"] {
            assert!(
                prod.contains(needle),
                "读面头注里少了「{needle}」—— 那段是 `P4a2` 摸底的全部产出，\
                 删了它下一个人会拿感觉重答一遍「省一次握手不好吗」"
            );
        }
    }
}
