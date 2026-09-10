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
const CC_BUS_CAT_CMD: &str = concat!(
    r#"B="${CC_BUS_HOME:-$HOME/.cc-bus}"; cat "$B/agents.tsv" 2>/dev/null; "#,
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

/// B03 批一：读远端 cc-bus 的**登记态**。**只读**，不写任何远端文件。
///
/// **注意语义**：返回的是「登记过什么」，**不是**「谁还活着」。`agents.tsv` 里最早的条目
/// 实测是 10 天前的（进程早没了）。判在线要另查 `tmux has-session`，那是**第二次往返**，
/// 放在用户点某一行的「检查」上，不在这里默认全量查（见 features/B03-*.md §三）。
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
/// # 三件套一件都不许少
///
/// 远端那条有**上限 + 超时 + 溢出处置**。本机看着「自家文件、能出什么事」——
/// 但 `agents.tsv` 是只增文件、`inbox` 更是，而 monitor 与它跑在同一台机器上，
/// 撑爆的是**用户正在用的那个进程**。⇒ 逐件对称，由
/// `the_local_cc_bus_read_keeps_every_guard_the_remote_one_has` 钉住。
///
/// ⚠ 用 `bash -lc` 而不是 `-lic`：这里只要 `$HOME` / `$CC_BUS_HOME`，不需要交互式 rc
/// （`ccm_probe` 那条要 `-lic` 是因为它得到用户 PATH 里找 `ccm`，需求不同，别互抄）。
async fn local_shell_read(
    cmd: &str,
    cap: u64,
    secs: u64,
    what: &str,
    on_overflow: OnOverflow,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let read = async {
        let mut child = tokio::process::Command::new("bash")
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
    /// 本条真起进程、真等超时（~1.3s）—— 起的是 `sleep`，不是任何会烧额度的东西。
    #[tokio::test]
    async fn a_timed_out_local_read_does_not_leave_an_orphan_behind() {
        // ⚠ marker **不能只写在注释里**：`bash -lc '<单条命令>'` 会 **exec 掉自己**，
        // 于是注释从任何 cmdline 上都消失，`pgrep` 数到 0 ⇒ **本判据首跑就是假绿**
        // （实测栽过一次）。⇒ 把 marker 放进那个必然存活的进程**自己的 argv** 里。
        let marker = format!("30.{}", std::process::id());
        let cmd = format!("sleep {marker}");
        let r = local_shell_read(&cmd, 4096, 1, "超时探针", OnOverflow::Reject).await;
        assert!(r.is_err(), "1 秒上限跑 sleep 30 竟然没超时 —— 本判据在空转");
        // 给 tokio 的收尸队一点时间。
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let out = std::process::Command::new("pgrep")
            .args(["-fc", &format!("sleep {marker}")])
            .output()
            .expect("pgrep 跑不起来");
        let n: usize = String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        assert_eq!(
            n, 0,
            "超时之后还留着 {n} 个子进程（marker={marker}）—— 每超时一次漏一个。\n\
             显式 `start_kill()` 只在成功路径上；超时那条要靠 `kill_on_drop(true)`。"
        );
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
