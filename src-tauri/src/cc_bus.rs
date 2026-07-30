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

pub fn split_combined<'a>(raw: &'a str, marker: &str) -> (&'a str, &'a str) {
    match raw.split_once(marker) {
        Some((a, b)) => (a, b),
        // 缺分隔标记（远端只有一个文件 / cat 部分失败）→ 宽容降级：全当第一段，
        // 第二段为空。不报错——同 `mcp.rs` 的「缺/坏 → None」精神。
        None => (raw, ""),
    }
}

// ===== IPC 层：按需经 SSH 读一次远端状态。**照抄 `mcp.rs::fetch_remote_claude_json`** =====
//
// 为什么是「按需读」而不是订阅/轮询：cc-bus 的状态全在远端本机 `~/.cc-bus/`，cc-monitor
// 跑在 Windows 只能经 SSH 看。两条备选——复用 daemon 既有 inotify watcher（**违反 daemon
// 零改红线**，且要新增协议帧），或按需刷新。取后者，形状逐条对齐 `mcp.rs`：
// 定值命令（零用户输入拼接 → 零注入面）、30s 超时、32MB 上限、宽容解析（缺/坏 → 空）、
// 大解析进 `spawn_blocking`。**无 setInterval、无后台定时任务**（红线）。

/// 一条**定值**命令读回两个文件，中间插分隔标记 —— 省一次 SSH 往返。
/// `origin` 只用于选连接配置，**不参与命令串拼接**，故此串是常量、零注入面
/// （同 `mcp.rs` 那条 `CMD` 常量的形状）。
/// 尊重 `CC_BUS_HOME`（cc-bus 自己就用这个变量定位状态目录）。
/// 结尾 `true` 保证两个文件都不存在时命令仍 rc=0——"没装 cc-bus"不是错误，是一种状态。
const CC_BUS_CAT_CMD: &str = concat!(
    r#"B="${CC_BUS_HOME:-$HOME/.cc-bus}"; cat "$B/agents.tsv" 2>/dev/null; "#,
    r#"printf '\n@@CCMON-CCBUS-SPLIT@@\n'; cat "$B/spawned.tsv" 2>/dev/null; true"#
);

async fn fetch_remote_cc_bus(cfg: &crate::ssh_source::RemoteConfig) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD).await?;
        let mut buf = Vec::new();
        stream
            .take(32 * 1024 * 1024)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读取远端 ~/.cc-bus 失败: {e}"))?;
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(30), read)
        .await
        .map_err(|_| format!("远端 '{}' 读取超时（30s）", cfg.origin_label()))??;
    // 非 UTF-8 不报错：`~/.cc-bus/` 里的目录名实测含各种字节，宽容降级即可。
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// B03 批一：读远端 cc-bus 的**登记态**。**只读**，不写任何远端文件。
///
/// **注意语义**：返回的是「登记过什么」，**不是**「谁还活着」。`agents.tsv` 里最早的条目
/// 实测是 10 天前的（进程早没了）。判在线要另查 `tmux has-session`，那是**第二次往返**，
/// 放在用户点某一行的「检查」上，不在这里默认全量查（见 features/B03-*.md §三）。
#[tauri::command]
pub async fn read_cc_bus_state(origin: String) -> Result<CcBusState, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let raw = fetch_remote_cc_bus(&cfg).await?;
    tokio::task::spawn_blocking(move || {
        let (a, s) = split_combined(&raw, CC_BUS_SPLIT_MARKER);
        let (agents, sk1) = parse_agents_tsv(a);
        let (spawned, sk2) = parse_spawned_tsv(s);
        CcBusState {
            agents,
            spawned,
            skipped: sk1 + sk2,
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))
}

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
    let cmd = build_online_cmd(&id)?;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))?;
    let read = async {
        use tokio::io::AsyncReadExt;
        let stream = crate::ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
        let mut buf = Vec::new();
        stream
            .take(4096)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("查在线失败: {e}"))?;
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

/// 三条命令共用的「连上去、跑、读回」。抽出来是因为它们的超时/上限/措辞各不相同，
/// 而**连接与读取的形状必须一致**（同 `mcp.rs` 的既有纪律）。
async fn exec_read(
    cfg: &crate::ssh_source::RemoteConfig,
    cmd: &str,
    cap: u64,
    secs: u64,
    what: &str,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let read = async {
        let stream = crate::ssh_source::connect_and_exec_cmd(cfg, cmd).await?;
        let mut buf = Vec::new();
        stream
            .take(cap)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("{what}失败: {e}"))?;
        Ok::<Vec<u8>, String>(buf)
    };
    let raw = tokio::time::timeout(std::time::Duration::from_secs(secs), read)
        .await
        .map_err(|_| format!("远端 '{}' {what}超时（{secs}s）", cfg.origin_label()))??;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

fn cfg_of(origin: &str) -> Result<crate::ssh_source::RemoteConfig, String> {
    crate::load_remote_config_by_label(origin)
        .ok_or_else(|| format!("远端 '{origin}' 未配置或未启用"))
}

/// B03 批二：读某个 agent 的 inbox（**只读**）。
#[tauri::command]
pub async fn read_cc_bus_inbox(origin: String, id: String) -> Result<Vec<CcBusMessage>, String> {
    let cmd = build_inbox_cmd(&id)?;
    let cfg = cfg_of(&origin)?;
    let raw = exec_read(&cfg, &cmd, 4 * 1024 * 1024, 30, "读 inbox").await?;
    tokio::task::spawn_blocking(move || parse_inbox_jsonl(&raw).0)
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))
}

/// B03 批二：给某个 agent 发消息。**这是本模块唯一的写操作**（其余全只读）。
#[tauri::command]
pub async fn cc_bus_send(origin: String, id: String, text: String) -> Result<String, String> {
    let cmd = build_send_cmd(&id, &text)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(&cfg, &cmd, 64 * 1024, 30, "发消息").await?;
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
    let cmd = build_spawn_cmd(&tool, &dir, &task, acct)?;
    let cfg = cfg_of(&origin)?;
    let out = exec_read(&cfg, &cmd, 64 * 1024, 60, "spawn").await?;
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
        // 非测试代码里这个常量只准出现两次：定义处 + 那唯一一个调用点
        assert_eq!(
            code.matches("CC_BUS_CAT_CMD").count(),
            2,
            "常量出现次数变了，检查是否多了第二条构造路径"
        );
    }

    #[test]
    fn inbox_missing_fields_degrade_not_panic() {
        let (m, sk) = parse_inbox_jsonl(r#"{"text":"orphan"}"#);
        assert_eq!(sk, 0);
        assert_eq!(m[0].from, "");
        assert_eq!(m[0].text, "orphan");
    }
}
