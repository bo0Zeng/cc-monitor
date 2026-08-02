//! F51 tmux 反查 / F60 画面预览(控制类命令走通道 B = russh exec,不干扰前台 PowerShell 终端)。
//!
//! tab 右键菜单打开时按需查询远端 tmux 会话列表,前端按 `pane_current_path==cwd +
//! pane_current_command==claude` 反查该 tab 的 Claude 正跑在哪个 tmux 会话,命中则一键
//! `ssh -t … tmux attach -t <名>`(交互走通道 A = PowerShell)。
//!
//! **最隐蔽的重写坑(调研 03 档 §3.1)**:`tmux ls -F` 的格式串**不解释**字面 `\t`——给什么
//! 字节原样输出。所以分隔符必须是**真 TAB 字节(0x09)**。Rust 里 `"\t"` 是真 TAB(勿写
//! `\\t`),`parse_tmux_ls` 按真 TAB `split`。F60 `capture_remote_pane` 已续挂本模块;kill/rename
//! 明确不做(见 MASTERPLAN 不做清单),F52 短路门未扩本模块。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

/// `tmux ls -F` 的格式串。字段以**真 TAB**分隔(见模块注释):
/// name ⇥ pane_current_path ⇥ pane_current_command ⇥ attached(1/0) ⇥ windows ⇥ @ccm_sid。
/// **F74**:末列 `#{@ccm_sid}` 是 `__ccm_rbind` 写的 tmux user option = 「这个 tmux 此刻在跑
/// 哪个 CC sid」的权威信号(pane title 被 Claude 活动标题抢写、不可靠;user option Claude 碰
/// 不到)。**未设置的会话此列为空串**(老会话 / 未装 wrapper)→ 解析成 `sid: None`,消费方回退
/// 旧的 path/cmd 匹配,向后兼容。
const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}\t#{@ccm_sid}";

/// 一个远端 tmux 会话(反查 + 未来管理用)。
///
/// G6：加 ts-rs 导出。此前前端在 `tabs.ts` 里**手抄了一份同名 interface**——两份各写各的，
/// 谁也不知道对方漂了没。既然要把它搬进包装层（`ipc/commands.ts` 的返回类型一律用生成物），
/// 顺手把手抄那份也换成生成物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TmuxSession {
    pub name: String,
    pub path: String,
    pub command: String,
    pub attached: bool,
    pub windows: u32,
    /// F74:`@ccm_sid` user option——此 tmux 当前所跑 CC 会话的 sid(`__ccm_rbind` 写,随
    /// `/branch` 漂移实时更新)。未设置(空串)→ `None`。cc-monitor 用它精确认「哪个 tmux 跑
    /// 目标 sid」,取代按目录/名字取第一个(同目录多 claude 会撞错会话)。
    pub sid: Option<String>,
}

/// 解析 `tmux ls -F '<TMUX_LS_FMT>'` 输出(真 TAB 分列)。字段数不符 / name 空的行跳过
/// (半截行、非法行不进结果);windows 非数字回退 0;末列 `@ccm_sid` 空串→ `None`。
pub fn parse_tmux_ls(output: &str) -> Vec<TmuxSession> {
    output
        .lines()
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 6 || f[0].is_empty() {
                return None;
            }
            Some(TmuxSession {
                name: f[0].to_string(),
                path: f[1].to_string(),
                command: f[2].to_string(),
                attached: f[3] == "1",
                windows: f[4].parse().unwrap_or(0),
                // 只认合法 sid 字符集 [A-Za-z0-9_-]:空串(未设 @ccm_sid)当 None;含别的字符也当
                // None——**极老 tmux(<3.0)可能不展开 `#{@ccm_sid}`、原样保留字面 `#{@ccm_sid}`**
                // (含 `#{}`),若当成 sid 会让 `findClaudeTmux` 的 anySidKnown 恒真 → 老 wrapper 用户
                // 永远走不到 cwd 回退。字符集校验一并挡掉未展开格式串与任何杂质(§30 见 doc/INVARIANTS.md)。
                sid: if !f[5].is_empty()
                    && f[5]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    Some(f[5].to_string())
                } else {
                    None
                },
            })
        })
        .collect()
}

/// F10：一次性用量探针会话的命名前缀（`src-tauri/src/account_usage.rs` 唯一使用这个前缀建
/// 会话）。**不**用新 tmux user-option 打标——`TMUX_LS_FMT` 是机器化锁死的"双写点"（红线 I8，
/// 见本文件 `tmux_ls_fmt_double_write_point_stays_in_sync` 测试：daemon 的 `watcher.rs` 也用
/// 这个格式串做自己的 idle-tmux 对账轮询，两侧必须逐字节一致），改格式串代价和风险都不成
/// 比例。探针会话名完全由本功能自己控制，前缀足够独特，用它做识别零风险、不碰任何双写点。
const USAGE_PROBE_NAME_PREFIX: &str = "ccm-usage-";

/// F10：判定一个 tmux 会话名是否是本功能建的一次性用量探针——纯字符串前缀匹配，不涉及 IO。
pub(crate) fn is_usage_probe_session(name: &str) -> bool {
    name.starts_with(USAGE_PROBE_NAME_PREFIX)
}

/// 列远端 tmux 会话(通道 B,一次性 exec)。`command -v tmux` 门控:无 tmux → 哨兵 `NO_TMUX`
/// → 返 `None`(前端隐藏 attach 项);有 tmux 但无会话 → `Some(空)`。
///
/// F10：过滤掉 `is_usage_probe_session` 命中的一次性用量探针会话——见
/// `parse_visible_tmux_sessions`。
#[tauri::command]
pub async fn list_remote_tmux(origin: String) -> Result<Option<Vec<TmuxSession>>, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    // `tmux ls` 无会话时非零退出("no server running")→ `|| true` 吞掉,得空输出=空列表。
    let cmd = format!(
        "if command -v tmux >/dev/null 2>&1; then tmux ls -F '{TMUX_LS_FMT}' 2>/dev/null || true; else printf 'NO_TMUX\\n'; fi"
    );
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    // lossy 解码(对齐全批 exec 输出读取:非 UTF-8 字节不该整体失败)。
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("读 tmux 列表失败: {e}"))?;
    let out = String::from_utf8_lossy(&buf);
    if out.trim() == "NO_TMUX" {
        return Ok(None);
    }
    Ok(Some(parse_visible_tmux_sessions(&out)))
}

/// `tmux ls` 原始输出 → **前端可见**的会话列表：解析 + 滤掉一次性用量探针会话（F10）。
///
/// 探针会话对 `findClaudeTmux`/tab 徽章/kill 授权判据等全部下游消费者应当不可见——它们寿命
/// 以秒计、用完即清，混进正牌列表只会让 tab 右键菜单短暂冒出一个不属于任何 tab 的幽灵条目。
///
/// **为什么单独提一个函数**（F10 Phase D 审计）：原先「解析 + 过滤」内联在 `list_remote_tmux`
/// 里，而 `#[tauri::command]` 需要真实远端连接、单测碰不到；于是那条测试把过滤表达式在测试体里
/// **又抄了一遍**——删掉生产侧的 `.filter(...)` 它照样绿，是典型的伪测试。提成纯函数后，
/// 生产与测试走的是同一条代码路径，删过滤会立刻红。
pub(crate) fn parse_visible_tmux_sessions(raw: &str) -> Vec<TmuxSession> {
    parse_tmux_ls(raw)
        .into_iter()
        .filter(|s| !is_usage_probe_session(&s.name))
        .collect()
}

// ---------- P1（zero-poll-liveness）：`TmuxSessions.observation` 的取值 ----------
//
// **第三个双写点**（前两个：`TMUX_LS_FMT` · `NO_TMUX` 哨兵）。monitor 与 daemon 分属两个
// 独立 crate、不能共享类型，所以这三个字符串两侧各写一份，由
// `observation_tokens_double_write_point_stays_in_sync` 测试逐字节钉住（同 `TMUX_LS_FMT`
// 那条守卫的做法：`include_str!` daemon 源 + 锚定 const 定义行，改任一侧忘同步即红）。
//
// **为什么用字符串而不是布尔**：P3 会加「server 已死」vs「server 活着但零会话」的细分
// （两者对 retire 决策等价、只对复活监视有意义）。字符串枚举加一个取值是 additive；
// 布尔字段加第二个就得改帧形状。
/// daemon 确证零会话（`tmux ls` rc=0 但 stdout 空 = `exit-empty off`；或 rc=1 = server 不在）。
const OBS_ZERO_SESSIONS: &str = "zero_sessions";
/// 远端没装 tmux（`command -v tmux` 失败）——与既有 `NO_TMUX` 哨兵同义，显式化。
const OBS_NO_TMUX: &str = "no_tmux";
/// 观测无效（`tmux ls` 以非 0/1 退出、或 exec 本身失败）⇒ 必须跳过，**绝不当零会话**。
const OBS_UNOBSERVABLE: &str = "unobservable";

/// P1（zero-poll-liveness）：一帧 `TmuxSessions` 的**观测分类**结果。
///
/// 存在的理由：这个判断原先是 `ssh_source::stream_loop` 里那条
/// `if raw.trim() != "NO_TMUX" { … if !backend.is_empty() { … } }` 内联 if——
/// 它把**五种语义完全不同的观测压成两条路**，而且住在一个需要真远端连接的
/// `async fn` 里、单测碰不到。提成纯函数后生产与测试走同一条路径。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TmuxObservation {
    /// 有效观测：某后端自报正在跑的 sid 集。
    ///
    /// **可能为空集**——空集 = daemon **确证**该主机零会话（不是"观测失败"）。
    /// 空集照常进对账、照常累计缺失，**这正是 P1 修掉的那个 bug**：
    /// 原先空 backend 一律保守跳过 ⇒ 当被杀的是该 origin 最后一个 tmux 会话时
    /// （server 随之退出、`tmux ls` 回空）⇒ 对账整段跳过 ⇒ idle 灰灯**卡到断连才清**。
    Backend(std::collections::HashSet<String>),
    /// 观测无效 ⇒ 本轮跳过、**不累计缺失**（否则 ssh 抖动会批量误灰）。
    Skip,
}

/// P1：把一帧 `TmuxSessions` 分类。`observation` = daemon 的显式分类字段
/// （P1 起的 additive wire 字段；旧 daemon 为 `None`）。
///
/// **判据只用 rc + stdout 空否**（daemon 侧已折成 `observation`），**绝不看 stderr 文本**——
/// P0 实测 stderr 有两种措辞（`no server running on …` / `error connecting to … `），
/// 且拿英文消息当判据本身就是错的。
///
/// 未知的 `observation` 取值 → **落回 raw 判据**（向前兼容：未来 daemon 加新分类时，
/// 老 monitor 退化成今天的保守行为，不会误灰）。
pub(crate) fn classify_tmux_observation(raw: &str, observation: Option<&str>) -> TmuxObservation {
    // NO_TMUX 哨兵（旧 daemon 唯一能表达的"后端不存在"）：远端没装 tmux ⇒ 无从对账。
    if raw.trim() == "NO_TMUX" {
        return TmuxObservation::Skip;
    }
    // P1：daemon 的显式分类优先。**未知取值刻意不在此匹配** ⇒ 落回下方 raw 判据（向前兼容）。
    match observation {
        Some(OBS_NO_TMUX) | Some(OBS_UNOBSERVABLE) => return TmuxObservation::Skip,
        // 只在 raw 确实为空时认这条——帧内部自相矛盾（说零会话却带着会话行）时以
        // **数据**为准、落回 raw 判据，不凭一个字符串把明明在跑的会话判死。
        Some(OBS_ZERO_SESSIONS) if raw.trim().is_empty() => {
            return TmuxObservation::Backend(std::collections::HashSet::new());
        }
        _ => {}
    }
    let backend: std::collections::HashSet<String> = parse_tmux_ls(raw)
        .iter()
        .filter_map(|s| s.sid.clone())
        .collect();
    // 旧 daemon 的空串语义不可分（零会话 / `|| true` 吞掉的错，两者同形）⇒ 保守跳过。
    // **新 daemon 走不到这里**：它零会话时带 `zero_sessions`、出错时带 `unobservable`。
    if backend.is_empty() {
        return TmuxObservation::Skip;
    }
    TmuxObservation::Backend(backend)
}

/// F60(纯函数,单测):判定 `capture_remote_pane` 的 stdout——哨兵 `NO_TMUX`(无 tmux)/
/// `NO_PANE`(会话不存在 / 抓屏失败)→ Err;否则原样返回抓到的屏幕文本。
/// (理论边角:pane 内容 `trim_end` 后恰等于某哨兵串 → 误判,概率可忽略。)
fn classify_capture_output(raw: &str) -> Result<String, String> {
    match raw.trim_end() {
        "NO_TMUX" => Err("远端未安装 tmux".to_string()),
        "NO_PANE" => Err("tmux 会话不存在或无法抓屏(可能刚结束)".to_string()),
        _ => Ok(raw.to_string()),
    }
}

/// F01：tmux `-t <target>` 的**精确匹配**包装。
///
/// **裸 `-t <名>` 不是精确匹配**：tmux 依次按「精确名 → **名字开头** → **glob**」解析。
/// 实测（tmux 3.6，隔离 `-L` socket）——只有 `sib-2` 存在时：
///   - `kill-session -t sib` → **杀掉 `sib-2` 且 rc=0**（当成功回报）
///   - `send-keys -t sib 'HELLO' Enter` → 投进 `sib-2`
///   - `capture-pane -p -t sib` → 抓的是 `sib-2`
///   - `kill-session -t 'si*'` → glob 命中并杀掉
/// 本仓必然踩：`pickFreshTmuxName` 刻意造 `<sid8>-cc-2/-3`，终端 `cct` 造 `<dir>_cc-2/-3`。
///
/// **为什么是 `=name:` 而不是 `=name`**（别"简化"掉尾冒号）：`=` 前缀只在 target-**session**
/// 解析路径上被识别。`send-keys`/`capture-pane` 收的是 target-**pane**，`set-option`/`show-options`
/// 走 pane 解析后上溯——这些路径上 `=name` 直接 `can't find pane`、**rc=1 完全失效**（实测）。
/// 尾冒号把串强制成 `session:` 形态（当前 window、活动 pane），`=` 才落在会话名段上被正确识别。
/// `=name:` 是唯一在全部动词上都既通用又精确的形式。矩阵见 `.claude/planned-build/unify-launch/MASTERPLAN.md` §5.3。
///
/// 删掉它会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，而 UI 报告「已重启」。
///
/// F04 Gate 1（恒强制）：**空 target 必须被拒**——`=:` 会被 tmux 解析成「当前会话」，是本模块
/// 唯一真正的危险默认值（今天 `capture_remote_pane` 是唯一无门的入口，见其函数头注）。
///
/// **只查空串，不额外收紧字符集**——glob/元字符（`*`/`;`/`$`/空格）不在这里挡：`shell_quote`
/// 已经把任意内容安全引号化（不会脱出 shell），字符集层面的收紧是**另一层职责**（TS 侧
/// `isValidNewTmuxName` 只在**创建路径**禁 glob，`isValidTmuxName` 对 attach 到已有会话故意
/// 宽松——见 INVARIANTS §31a"第二道防线"）。这里若也收紧字符集会让 `si*` 这类合法 attach 目标
/// （已有会话名里含 glob 字符）在 Gate 1 就被拒，与既有 `tmux_targets_use_exact_match` 测试
/// 钉死的"glob 名被引号原样包住、不脱出"这一既定行为冲突——**空** 是唯一需要在这一层拦的语义
/// 陷阱（`=:` 落到当前会话），其余交给引号化 + 上层校验。
fn is_safe_tmux_target(target: &str) -> bool {
    !target.is_empty()
}

/// F04：`exact_target` 现在是 fallible——Gate 1 折进这一个函数本身，任何未来新增的 tmux 命令
/// 构造点都**结构性不可能**绕过它（不是"记得检查"，是没有第二条路可走）。
///
/// `pub(crate)`（F10 Phase D 后端审计）：`account_usage.rs` 的探针会话曾手抄
/// `shell_quote(&format!("={session}:"))`——那正是本函数头注声称"结构性不可能"的绕过。
/// 开放给 crate 内复用，让"新增 tmux 命令构造点"这件事只有一条路可走，不是靠自觉。
pub(crate) fn exact_target(target: &str) -> Result<String, String> {
    if !is_safe_tmux_target(target) {
        return Err(format!("非法 tmux 目标（空）：{target:?}"));
    }
    Ok(ssh_source::shell_quote(&format!("={target}:")))
}

/// `capture-pane` 远端命令串（提纯以便单测——D 审计：内联 `format!` 让 3 个 `-t` 位点里 2 个
/// 无测试覆盖，把 `exact_target` 改回裸目标 `cargo test` 依旧全绿）。
/// 只过 Gate 1（空/非法字符）——**只读快照**，MASTERPLAN 明确不为它加身份门（F04 计划 §1）。
fn build_capture_pane_cmd(target: &str) -> Result<String, String> {
    let t = exact_target(target)?;
    Ok(format!(
        "if command -v tmux >/dev/null 2>&1; then tmux capture-pane -p -t {t} 2>/dev/null || printf 'NO_PANE\\n'; else printf 'NO_TMUX\\n'; fi"
    ))
}

/// F04：三道门里 Gate 2 远端半支（`@ccm_sid` 已设）+ Gate 3（仅破坏性动作，`windows==1`）折进
/// **一条原子远端命令**——同一 round-trip 里"查完再判再动"，不给中间留可被抢跑的窗口（TOCTOU
/// 教训，见 MASTERPLAN §5.2）。
///
/// `need_sid`：Gate 2 的远端半支是否需要——`is_ccm_tmux_name`（本地、零 IO）命中时不需要，
/// 因为名字前缀本身已经是"这是我们的会话"的证明（Gate 2 = `@ccm_sid` ∪ `cc-*`，OR 的另一支
/// 已经在调用方满足）。`need_windows`：Gate 3 是否需要——只有 kill 需要，send-keys 不需要。
///
/// 用 `tmux display-message -p -t <target> '<fmt>'` 而非 `show-options`——后者对未设置的
/// option 是 `rc=1` + stderr、需要脆弱的 rc/stderr 联合判断；`display-message` 走这个仓库已经
/// 在生产验证过的格式串插值惯例（`TMUX_LS_FMT` 本身、`shared/ccm` 的 cwd 探测同款），未设置的
/// option 静默展开成空串。**总是**在格式串里带 `#{session_windows}`（哪怕 `need_windows=false`
/// 也不检查它）——它对一个存在的会话恒为正整数，用来当"目标是否存在"的判据：目标不存在时
/// `display-message` 连这个字段都取不到、整条捕获串为空；目标存在但 `@ccm_sid` 未设时，
/// 捕获串因为 `session_windows` 非空而不为空，两种情况因此被同一个 `[ -z "$info" ]` 干净分开。
///
/// `need_sid=false && need_windows=false` 时退化成今天的精确原样一行（零改动、零额外 round
/// trip）——覆盖 100% 的现有真实流量（`cc-*` 命名的 send-keys）。
fn build_guarded_tmux_cmd(
    target: &str,
    need_sid: bool,
    need_windows: bool,
    build_action: impl Fn(&str) -> String,
) -> Result<String, String> {
    let t = exact_target(target)?;
    let action = build_action(&t);
    if !need_sid && !need_windows {
        return Ok(format!(
            "if command -v tmux >/dev/null 2>&1; then {action}; else printf 'NO_TMUX\\n'; fi"
        ));
    }
    let fmt = if need_sid {
        "#{session_windows}\t#{@ccm_sid}"
    } else {
        "#{session_windows}"
    };
    let extract = if need_sid {
        "w=\"$(printf '%s' \"$info\" | cut -f1)\"; sid=\"$(printf '%s' \"$info\" | cut -f2)\";"
    } else {
        "w=\"$info\";"
    };
    let guard = match (need_sid, need_windows) {
        (true, true) => "[ -n \"$sid\" ] && [ \"$w\" = \"1\" ]",
        (true, false) => "[ -n \"$sid\" ]",
        (false, true) => "[ \"$w\" = \"1\" ]",
        (false, false) => unreachable!("已在上面的退化分支处理"),
    };
    // F04 Phase D 审计发现：`reject_msg` 曾恒带 `windows=%s`（只要 `need_sid`），即便 send-keys
    // （`need_windows=false`）根本不受 Gate 3 约束——`$w` 只是existence-marker、从未参与 guard
    // 判断，混进拒绝消息会让用户误以为 windows 数也影响了 send-keys 的拒绝判断。按
    // `(need_sid, need_windows)` 组合精确匹配 guard 实际用了哪些字段，消息只报告真正参与判断的。
    let reject_msg = match (need_sid, need_windows) {
        (true, true) => "printf 'CCM_GUARD_REJECTED sid=%s windows=%s\\n' \"$sid\" \"$w\"",
        (true, false) => "printf 'CCM_GUARD_REJECTED sid=%s\\n' \"$sid\"",
        (false, true) => "printf 'CCM_GUARD_REJECTED windows=%s\\n' \"$w\"",
        (false, false) => unreachable!("已在上面的退化分支处理"),
    };
    Ok(format!(
        "if command -v tmux >/dev/null 2>&1; then \
info=\"$(tmux display-message -p -t {t} '{fmt}' 2>/dev/null)\"; \
if [ -z \"$info\" ]; then printf 'CCM_NO_SESSION\\n'; else {extract} \
if {guard}; then {action}; else {reject_msg}; fi; fi; \
else printf 'NO_TMUX\\n'; fi"
    ))
}

/// F60:抓一个远端 tmux 会话当前窗口/pane 的屏幕文本(**只读快照,非 attach**)。
/// `tmux capture-pane -p -t <target>`(`-p` 打 stdout、`-t` 选会话)。`command -v tmux` 门控
/// (无 → `NO_TMUX`);会话不存在 / 抓屏失败 → `NO_PANE`(`|| printf`)。target 经 `shell_quote`
/// (来自 `list_remote_tmux` 的真实会话名,仍防御转义)。通道 B 一次性 exec,不干扰前台终端。
#[tauri::command]
pub async fn capture_remote_pane(origin: String, target: String) -> Result<String, String> {
    let cmd = build_capture_pane_cmd(&target)?;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    // lossy 解码:capture-pane 抓任意终端屏,非 UTF-8 字节(CP437 画框 / ANSI art / 二进制)
    // 常见——严格 UTF-8 会整体失败并被误报「会话刚结束」;有损展示远胜报错(Phase G 对齐)。
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("读 pane 快照失败: {e}"))?;
    classify_capture_output(&String::from_utf8_lossy(&buf))
}

/// `kill-session` 远端命令串（提纯以便单测，理由同 `build_capture_pane_cmd`）。**破坏性动作**——
/// 正是「杀错会话」那条生产 bug 的一端，必须被回归测试钉死。
///
/// F04 三道门：Gate 1（`exact_target` 内部）恒强制；Gate 2 = 本地 `is_ccm_tmux_name` 命中 **或**
/// 远端 `@ccm_sid` 已设（union，不删除前缀检查——旧无 `@ccm_sid` 的 `cc-*` 会话仍必须可杀，
/// 否则是向后兼容回归）；Gate 3（仅 kill）= 远端 `windows==1`。Gate 2 远端半支 + Gate 3 折进
/// `build_guarded_tmux_cmd` 的一条原子命令，不给"查完再杀"之间留竞态窗口。
fn build_kill_session_cmd(target: &str) -> Result<String, String> {
    let name_owned = is_ccm_tmux_name(target);
    build_guarded_tmux_cmd(target, !name_owned, true, |t| {
        format!("tmux kill-session -t {t} 2>&1")
    })
}

/// F79(#38)：杀死远端 tmux 会话（`tmux kill-session -t <target>`）。**破坏性操作**——前端二次确认后才调。
/// `target` 经 `shell_quote`（来自 `list_remote_tmux` 的真实会话名，仍防御转义）。杀完 tab 变灰由 #60-A
/// 的 tmux 存活对账兜（本命令不主动 archive，守 §24）。成功无输出；失败（会话不存在等）经 `2>&1` 捕获报错。
#[tauri::command]
pub async fn kill_remote_tmux(origin: String, target: String) -> Result<(), String> {
    // 命令构造（含 Gate 1/2 本地半支）先于配置查找——本地校验失败不该先花一次配置查找。
    let cmd = build_kill_session_cmd(&target)?;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("杀 tmux 会话失败: {e}"))?;
    let out = String::from_utf8_lossy(&buf);
    let trimmed = out.trim();
    if trimmed == "NO_TMUX" {
        return Err("远端未安装 tmux".to_string());
    }
    if trimmed == "CCM_NO_SESSION" {
        return Err("远端会话已不存在（可能已被终止）".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("CCM_GUARD_REJECTED ") {
        return Err(format!(
            "拒绝 kill：目标未通过身份/窗口守卫（{rest}）——可能不是本工具管理的会话，或已被扩展出额外窗口（避免误杀你自己的 tmux 会话；请到该 tmux 里自行处理）"
        ));
    }
    // kill-session 成功无输出；非空 = stderr 里的失败信息（如 "can't find session"）。
    if !trimmed.is_empty() {
        return Err(format!("tmux kill-session: {trimmed}"));
    }
    Ok(())
}

/// send-keys 远端命令串（提纯以便单测——补 R1「命令构造测缺」）。`enter=true` 时尾附 `Enter` 键
/// （如 `/compact`、`/exit` 这类要回车提交的）；`enter=false` 只发裸键（如 `Escape` 打断当前回合，
/// **不能**带尾回车，否则可能误提交输入框里的队列文本）。target/keys 均经 `shell_quote`。
///
/// F04：Gate 2 远端半支——`is_ccm_tmux_name` 本地命中时跳过（零额外 round trip，覆盖今天 100%
/// 的真实流量：`cc-*` 命名目标）；未命中时原子核验远端 `@ccm_sid` 已设才发送。无 Gate 3——
/// send-keys 不删除任何东西，`windows` 数量与它无关。
fn build_send_keys_remote_cmd(target: &str, keys: &str, enter: bool) -> Result<String, String> {
    let tail = if enter { " Enter" } else { "" };
    let keys_q = ssh_source::shell_quote(keys);
    let name_owned = is_ccm_tmux_name(target);
    build_guarded_tmux_cmd(target, !name_owned, false, |t| {
        format!("tmux send-keys -t {t} {keys_q}{tail} 2>&1")
    })
}

/// A5：向远端 tmux 会话发按键（headless ssh，如换号重启前在旧号上 send `/compact`、或优雅退出的
/// `Escape`/`/exit`）。**只发按键、不杀不建**，走一次性 ssh、**daemon 不参与**（守只读边界）。
/// `keys` 是字面串或 tmux 键名（`/compact` / `/exit` / `Escape`）；`enter`（可选，**默认 true** 向后兼容
/// A5 旧调用）决定是否尾附 `Enter`——优雅退出的 `Escape` 传 `enter=false`。
/// keys 经 `shell_quote`。成功无输出；失败（会话不存在等）经 `2>&1` 捕获报错。
///
/// F04：安全判据从"只认 `is_ccm_tmux_name`"改为 Gate 2 union（`cc-*` 前缀本地命中 **或** 远端
/// `@ccm_sid` 已设）——`build_send_keys_remote_cmd` 内部处理；非 `cc-*` 名不再在客户端提前拒绝
/// （F02 允许 `--tmux=<自定义名>`，这类会话是 `ccm` 拥有的、只是名字不含前缀，必须走远端核验
/// 而非按名字形状一刀切拒绝，否则是新引入的向后不兼容）。
#[tauri::command]
pub async fn tmux_send_keys(
    origin: String,
    target: String,
    keys: String,
    enter: Option<bool>,
) -> Result<(), String> {
    // 缺省（前端旧调用不传）→ true，与 A5 原行为逐字节等价。命令构造先于配置查找（同 kill）。
    let cmd = build_send_keys_remote_cmd(&target, &keys, enter.unwrap_or(true))?;
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("send-keys 失败: {e}"))?;
    let out = String::from_utf8_lossy(&buf);
    let trimmed = out.trim();
    if trimmed == "NO_TMUX" {
        return Err("远端未安装 tmux".to_string());
    }
    if trimmed == "CCM_NO_SESSION" {
        return Err("远端会话已不存在（可能已被终止）".to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("CCM_GUARD_REJECTED ") {
        return Err(format!(
            "拒绝 send-keys：目标未通过身份守卫（{rest}）——可能不是本工具管理的会话"
        ));
    }
    if !trimmed.is_empty() {
        return Err(format!("tmux send-keys: {trimmed}"));
    }
    Ok(())
}

/// 本工具建的 tmux 会话名判定：`cc-` 前缀 + 只含 `[A-Za-z0-9_-]`（`cc-<sid8>[-N]` 恒满足）。
///
/// F04：**不再是唯一身份判据**，降级为 Gate 2（identity）union 的本地半支——`@ccm_sid` 已设
/// 是远端半支（`build_guarded_tmux_cmd` 里核验）。命中此判据即可跳过远端核验（零 IO，覆盖今天
/// 100% 的真实流量）；未命中不代表拒绝，只代表"需要问远端 `@ccm_sid`"。**不删除**——F02 之前的
/// 老 `cc-*` 会话没有 `@ccm_sid`，只靠这条前缀判据仍必须可 kill/send-keys，否则是向后兼容回归。
fn is_ccm_tmux_name(name: &str) -> bool {
    // S4b-3b（用户 2026-07-31）：新会话叫 `<X>-cc`（撞名时 `<X>-cc-2`）。
    // **老的 `cc-` 前缀一并保留、绝不删** —— 本函数头注逐字写着理由：F02 之前的老 `cc-*`
    // 会话没有 `@ccm_sid`，只靠这条前缀判据仍必须可 kill/send-keys。删了就是把用户
    // **正在跑的**会话变成 issue #76 那种「失管会话」。
    let charset_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let old_prefix = name.starts_with("cc-") && name.len() > 3;
    // 后缀形态：`<X>-cc` 或 `<X>-cc-<N>`（撞名避让）。要求 `<X>` 非空，
    // 否则裸 `-cc` 这种退化名也会命中。
    let new_suffix = name
        .split("-cc")
        .next()
        .is_some_and(|head| !head.is_empty() && head.len() < name.len())
        && (name.ends_with("-cc")
            || name
                .rsplit_once("-cc-")
                .is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())));
    charset_ok && (old_prefix || new_suffix)
}

/// daemon 侧 `watcher.rs` 的源码路径 —— **跨 crate 硬路径的单一落点**。
///
/// # 为什么要有这个常量
///
/// monitor 的两条对拍守卫用 `include_str!` 读 daemon 的源码（两个 crate 不能共享 `const`，
/// 只能靠「读对方源码 + 断言」防跨语言/跨 crate 漂移）。U2 的 Phase D 审计点名过：
/// **这类硬路径在 daemon 重构时会一起断，而且断的是编译期**。
///
/// U3 把 `watcher.rs` 搬进 `observe/` 时它**当场兑现** —— `cargo test --lib` 直接
/// `couldn't read src/../../remote-daemon-proto/src/watcher.rs`。
/// 好消息是它**响**（编译错，不是静默假绿）；坏消息是它有两处、还散着。收进一个常量，
/// 下次 daemon 再搬家只改这一行。
///
/// ⚠ **必须是 `macro_rules!` 不能是 `const`**：`include_str!` 只接受**字面量 token**，
/// 喂给它一个 `const` 会报 `argument must be a string literal`（我第一版就这么写的）。
/// 宏能展开成字面量，于是既拿到了单一落点、又满足 `include_str!` 的要求。
// 只在 `#[cfg(test)]` 的两条对拍守卫里用 —— 不加这个属性会留一条
// `unused macro definition` 告警（Phase D 审计 I6）。
#[cfg(test)]
macro_rules! daemon_watcher_src {
    () => {
        "../../remote-daemon-proto/src/observe/watcher.rs"
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- P1（zero-poll-liveness）：观测分类 ----------

    fn sids(o: &TmuxObservation) -> Vec<String> {
        match o {
            TmuxObservation::Backend(s) => {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort();
                v
            }
            TmuxObservation::Skip => panic!("期望 Backend，实得 Skip"),
        }
    }

    /// ★ P1 的回归测试（**这条在修之前是红的**）：daemon 确证零会话 ⇒ 必须是**有效观测（空集）**，
    /// 不是跳过。这就是 `doc/INVARIANTS.md` §24bis 那条残留 bug 的机理：
    /// 杀掉某 origin 仅剩的 tmux 会话 → server 随之退出 → `tmux ls` 回空 →
    /// 旧代码保守跳过 → idle 灰灯卡到断连 flush 才清。
    #[test]
    fn zero_sessions_is_a_valid_observation_not_a_skip() {
        assert_eq!(
            classify_tmux_observation("", Some("zero_sessions")),
            TmuxObservation::Backend(std::collections::HashSet::new()),
            "daemon 确证零会话时必须进对账（空集），否则灰灯永不清"
        );
    }

    /// 旧 daemon（无 `observation` 字段）+ 空 raw ⇒ **保持今天的保守行为**。
    /// 空 raw 在旧 daemon 那里同时意味着「零会话」和「`tmux ls` 出错被 `|| true` 吞了」，
    /// 分不开 ⇒ 只能跳过。**新旧混搭不许回归。**
    #[test]
    fn old_daemon_empty_raw_still_skips() {
        assert_eq!(
            classify_tmux_observation("", None),
            TmuxObservation::Skip,
            "旧 daemon 的空串语义不可分，必须保守跳过"
        );
    }

    /// 远端没装 tmux ⇒ 跳过（哨兵与显式分类**两条路都要认**）。
    #[test]
    fn no_tmux_skips_both_via_sentinel_and_field() {
        assert_eq!(
            classify_tmux_observation("NO_TMUX", None),
            TmuxObservation::Skip
        );
        assert_eq!(
            classify_tmux_observation("NO_TMUX\n", Some("no_tmux")),
            TmuxObservation::Skip
        );
    }

    /// 观测无效（`tmux ls` 非 0/1 退出、exec 失败）⇒ 跳过，**绝不当成零会话**。
    #[test]
    fn unobservable_skips() {
        assert_eq!(
            classify_tmux_observation("", Some("unobservable")),
            TmuxObservation::Skip,
            "观测失败当成零会话会批量误灰"
        );
    }

    /// 有会话时照常解析出 sid 集（`@ccm_sid` 为空的会话不进集合，见 `parse_tmux_ls`）。
    #[test]
    fn sessions_parse_into_sid_set() {
        let raw = "s1\t/p\tclaude\t1\t2\tsid-a\ns2\t/q\tnode\t0\t1\t\ns3\t/r\tbash\t0\t1\tsid-c";
        let o = classify_tmux_observation(raw, None);
        assert_eq!(sids(&o), vec!["sid-a".to_string(), "sid-c".to_string()]);
    }

    /// 向前兼容：未来 daemon 加了本 monitor 不认识的分类 ⇒ **落回 raw 判据**，
    /// 退化成今天的保守行为，不误灰。
    #[test]
    fn unknown_observation_falls_back_to_raw() {
        assert_eq!(
            classify_tmux_observation("", Some("some_future_kind")),
            TmuxObservation::Skip
        );
        let o = classify_tmux_observation("s1\t/p\tclaude\t1\t1\tsid-a", Some("some_future_kind"));
        assert_eq!(sids(&o), vec!["sid-a".to_string()]);
    }

    /// **P1 刻意保留的一处不对称**：`observation` 说有会话、但 raw 里一个 `@ccm_sid` 都没有
    /// （老会话 / 未装 wrapper）⇒ 仍然跳过。因为对账的判据是 sid 集，没有 sid 就无从判断，
    /// 而"有 tmux 会话但都没绑 sid"**不等于**"零会话"。
    #[test]
    fn sessions_without_any_ccm_sid_still_skips() {
        assert_eq!(
            classify_tmux_observation("s1\t/p\tbash\t0\t1\t", None),
            TmuxObservation::Skip,
            "有会话但无 @ccm_sid ≠ 零会话，不许当空集喂进对账"
        );
    }

    /// P1：`observation` 取值集是 monitor↔daemon 的**第三个双写点**（前两个：`TMUX_LS_FMT` ·
    /// `NO_TMUX` 哨兵）。两个独立 crate 不能共享类型 ⇒ 用与
    /// `tmux_ls_fmt_double_write_point_stays_in_sync` 相同的办法钉住：`include_str!` 读 daemon 源
    /// + **锚定 const 定义行**（不是裸字面量——否则该串若出现在某条注释里会掩盖真漂移）。
    ///   **双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
    #[test]
    fn observation_tokens_double_write_point_stays_in_sync() {
        let daemon_src = include_str!(daemon_watcher_src!());
        for (name, value) in [
            ("OBS_ZERO_SESSIONS", OBS_ZERO_SESSIONS),
            ("OBS_NO_TMUX", OBS_NO_TMUX),
            ("OBS_UNOBSERVABLE", OBS_UNOBSERVABLE),
        ] {
            let expected_def = format!("const {name}: &str = \"{value}\";");
            assert!(
                daemon_src.contains(&expected_def),
                "observation 双写点漂移：daemon watcher.rs 不含 {expected_def:?}\n\
                 （改了分类取值就得两侧同步——同 TMUX_LS_FMT 的纪律）"
            );
        }
        // 反向自检：断言的是「扫到了 daemon 源」而不是「命中若干条」——阈值不能挂在
        // 被检查的量上（rust-ts-boundary 的教训）。
        assert!(
            daemon_src.len() > 1000,
            "include_str! 没读到 daemon 源，上面三条断言全是空转"
        );
    }

    /// A5：send-keys 目标白名单——只认本工具的 cc-* 会话名，拒用户别的 tmux。
    #[test]
    fn ccm_tmux_name_whitelist() {
        assert!(is_ccm_tmux_name("cc-abc12345"));
        // ★ S4b-3b：新命名 `<X>-cc`（撞名时 `<X>-cc-<N>`）也要本地命中，
        // 否则每次 kill/send-keys 都要多跑一趟远端去核 `@ccm_sid`。
        assert!(is_ccm_tmux_name("abc12345-cc"));
        assert!(is_ccm_tmux_name("abc12345-cc-2"));
        assert!(is_ccm_tmux_name("my-proj-cc"));
        // **老前缀必须继续命中** —— 用户机器上正跑着的会话就是这个形状，
        // 不认它们等于把它们变成 issue #76 那种「失管会话」。
        assert!(is_ccm_tmux_name("cc-proj"));
        // 退化名不该命中：`-cc` 前面得有东西。
        assert!(!is_ccm_tmux_name("-cc"));
        // 名字里恰好含 `-cc` 但不是以它结尾、也不是 `-cc-<数字>` ⇒ 不认
        //（那多半是别人的会话，误认会让我们跳过远端核验就去 kill）。
        assert!(!is_ccm_tmux_name("foo-ccx"));
        assert!(!is_ccm_tmux_name("foo-cc-bar"));
        assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
        assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
        assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
        assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
        assert!(!is_ccm_tmux_name("cc-a b")); // 空格（注入面）
        assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
        assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
    }

    /// F04 Gate 1：**只有空 target** 恒被拒——`=:` 会解析成「当前会话」，是唯一真正危险的默认值。
    /// 在本地就地失败，不发起任何 SSH 连接（此测直接调纯函数，不依赖任何 origin 配置存在）。
    /// 含 glob/元字符但非空的 target **不**在这一层被拒（`shell_quote` 已安全引号化，字符集收紧
    /// 是 TS 侧 `isValidNewTmuxName`/`isValidTmuxName` 的职责，见 `is_safe_tmux_target` 头注）。
    #[test]
    fn gate1_rejects_only_empty_target() {
        assert!(
            build_kill_session_cmd("").is_err(),
            "空 target 应被 Gate 1 拒绝（kill）"
        );
        assert!(
            build_send_keys_remote_cmd("", "/exit", true).is_err(),
            "空 target 应被 Gate 1 拒绝（send-keys）"
        );
        assert!(
            build_capture_pane_cmd("").is_err(),
            "空 target 应被 Gate 1 拒绝（capture-pane，今天唯一无门的入口）"
        );
        // 非空、含元字符/glob 的 target 不被 Gate 1 拒——shell_quote 已使其安全，字符集收紧是
        // 另一层（TS 侧）职责，见 `gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject`
        // 与 `tmux_targets_use_exact_match` 里 `si*`/`a'b` 的既定通过行为。
        for safe_nonempty in ["cc-a b", "cc-a;rm", "cc-a$x", "si*", "a'b"] {
            assert!(
                build_capture_pane_cmd(safe_nonempty).is_ok(),
                "非空 target {safe_nonempty:?} 不该被 Gate 1 拒绝: {:?}",
                build_capture_pane_cmd(safe_nonempty)
            );
        }
    }

    /// audit-fixes F02(I1) → F04 更新：非 `cc-*` 但**字符安全**的名字（如 F02 `--tmux=<自定义名>`
    /// 建的会话）**不再在客户端被一刀切拒绝**——Gate 2 是 union，未命中本地前缀判据时改为构造
    /// 一条原子远端核验 `@ccm_sid` 的命令，而不是立即 Err。这是本次 F04 的核心行为变化：
    /// 变异锚点——如果 Gate 2 退化回"只认前缀"，下面的命令就不会含 `@ccm_sid` 查询。
    #[test]
    fn gate2_non_prefixed_safe_name_builds_remote_check_not_instant_reject() {
        for safe_non_ccm in ["work", "web", "my-session", "0"] {
            let kill = build_kill_session_cmd(safe_non_ccm)
                .unwrap_or_else(|e| panic!("{safe_non_ccm:?} 是安全字符集,不该被 Gate 1 拒: {e}"));
            assert!(
                kill.contains("@ccm_sid"),
                "非 cc-* 安全名 {safe_non_ccm:?} 的 kill 命令必须核验远端 @ccm_sid（Gate 2 union）: {kill}"
            );
            let sk = build_send_keys_remote_cmd(safe_non_ccm, "/exit", true)
                .unwrap_or_else(|e| panic!("{safe_non_ccm:?} 是安全字符集,不该被 Gate 1 拒: {e}"));
            assert!(
                sk.contains("@ccm_sid"),
                "非 cc-* 安全名 {safe_non_ccm:?} 的 send-keys 命令必须核验远端 @ccm_sid: {sk}"
            );
        }
        // 对照组：cc-* 前缀命中 → 本地已判定"是我们的"，kill 命令不含 @ccm_sid 查询（覆盖 Gate 3
        // 仍然核验 windows，但不必再查 sid）；send-keys 更进一步，完全退化成今天的一行（零 Gate）。
        let kill_owned = build_kill_session_cmd("cc-abc12345").unwrap();
        assert!(
            !kill_owned.contains("@ccm_sid"),
            "cc-* 前缀命中不该再问远端 @ccm_sid: {kill_owned}"
        );
        assert!(
            kill_owned.contains("session_windows"),
            "kill 恒需要 Gate 3 的 windows 核验（即便 Gate 2 本地已过）: {kill_owned}"
        );
        let sk_owned = build_send_keys_remote_cmd("cc-abc12345", "/exit", true).unwrap();
        assert!(
            !sk_owned.contains("@ccm_sid") && !sk_owned.contains("display-message"),
            "cc-* 前缀命中的 send-keys 应退化成今天的一行、零额外 round trip: {sk_owned}"
        );
    }

    /// F04：Gate 3（仅 kill）——`windows` 门槛只出现在 kill 的命令构造里，send-keys 恒不含。
    #[test]
    fn gate3_only_applies_to_kill_not_send_keys() {
        let kill = build_kill_session_cmd("cc-abc12345").unwrap();
        assert!(kill.contains("windows"), "kill 必须核验 windows: {kill}");
        let sk = build_send_keys_remote_cmd("cc-abc12345", "/exit", true).unwrap();
        assert!(
            !sk.contains("windows"),
            "send-keys 不删东西，不该有 Gate 3: {sk}"
        );
    }

    /// F04 Phase D 审计发现并修：非前缀名的 send-keys（`need_sid=true, need_windows=false`）
    /// 之前的拒绝消息恒带 `windows=%s`——`$w` 只是 existence-marker、从未参与 guard 判断，混进
    /// 消息会让用户误以为 windows 数也影响了 send-keys 的拒绝判断。拒绝消息现在按
    /// `(need_sid, need_windows)` 精确匹配 guard 实际用到的字段。
    #[test]
    fn reject_message_only_reports_fields_actually_gated_on() {
        let kill_custom = build_kill_session_cmd("e2e-custom").unwrap();
        assert!(
            kill_custom.contains("CCM_GUARD_REJECTED sid=%s windows=%s"),
            "kill（need_sid+need_windows 都真）拒绝消息应同时报 sid 和 windows: {kill_custom}"
        );
        let sk_custom = build_send_keys_remote_cmd("e2e-custom", "/exit", true).unwrap();
        assert!(
            sk_custom.contains("CCM_GUARD_REJECTED sid=%s\\n") && !sk_custom.contains("windows=%s"),
            "send-keys（仅 need_sid 真）拒绝消息只该报 sid，不该混入未参与判断的 windows: {sk_custom}"
        );
    }

    /// A5+：send-keys 命令构造（补 R1）——enter=true 尾附 ` Enter`，false 不附；target/keys 经 shell_quote。
    /// F01：target 形态为 `'=<名>:'`（精确匹配，见 `exact_target`）。用 cc-* 名走零 Gate 的退化路径，
    /// 命令形状与今天逐字节相同（F04 对 100% 真实流量零改动的验证点）。
    #[test]
    fn send_keys_cmd_construction() {
        let with_enter = build_send_keys_remote_cmd("cc-abc12345", "/compact", true).unwrap();
        assert!(
            with_enter.contains("tmux send-keys -t '=cc-abc12345:' '/compact' Enter 2>&1"),
            "enter=true 应尾附 Enter: {with_enter}"
        );
        let no_enter = build_send_keys_remote_cmd("cc-abc12345", "Escape", false).unwrap();
        assert!(
            no_enter.contains("tmux send-keys -t '=cc-abc12345:' 'Escape' 2>&1"),
            "enter=false 不应附 Enter: {no_enter}"
        );
        assert!(
            !no_enter.contains(" Enter 2>&1"),
            "enter=false 命令里不得出现 Enter 键: {no_enter}"
        );
        // NO_TMUX 降级分支两者都在。
        assert!(
            with_enter.contains("printf 'NO_TMUX\\n'") && no_enter.contains("printf 'NO_TMUX\\n'")
        );
    }

    /// F01 回归：tmux `-t` 目标**必须**精确匹配（`'=<名>:'`），绝不留裸目标。
    ///
    /// 裸 `-t <名>` 是「精确 → 名字开头 → glob」三级解析。实测(tmux 3.6)只有 `sib-2` 存在时
    /// `kill-session -t sib` 杀掉 `sib-2` 且 **rc=0**、`send-keys -t sib` 投进 `sib-2`、
    /// `kill-session -t 'si*'` glob 命中。本仓必然踩（`pickFreshTmuxName` 造 `<sid8>-cc-2/-3`、
    /// 终端 `cct` 造 `<dir>_cc-2/-3`）。
    ///
    /// 删掉 `exact_target` 会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，
    /// 而 UI 报告「已重启」。**尾冒号不能省**：`send-keys`/`capture-pane` 收 target-pane，
    /// `=名`（无冒号）在那条路径上 rc=1 完全失效。
    #[test]
    fn tmux_targets_use_exact_match() {
        // **三个命令构造点全钉死**（D 审计：此前只钉了 send-keys，另两处改回裸目标测试仍全绿）。
        let sk = build_send_keys_remote_cmd("cc-abc12345", "/exit", true).unwrap();
        let cap = build_capture_pane_cmd("cc-abc12345").unwrap();
        let kill = build_kill_session_cmd("cc-abc12345").unwrap();
        for (label, cmd) in [
            ("send-keys", &sk),
            ("capture-pane", &cap),
            ("kill-session", &kill),
        ] {
            assert!(
                cmd.contains("-t '=cc-abc12345:'"),
                "{label} 目标必须是 '=<名>:' 精确形态: {cmd}"
            );
            assert!(
                !cmd.contains("-t 'cc-abc12345'"),
                "{label} 不得留裸目标（会前缀命中 cc-abc12345-2）: {cmd}"
            );
        }

        // exact_target 本身：`=` 与 `:` 都落在引号内，且不吃掉原名。
        assert_eq!(exact_target("cc-x").unwrap(), "'=cc-x:'");
        assert_eq!(exact_target("proj_cc-2").unwrap(), "'=proj_cc-2:'");
        // glob 名即便漏进来也被引号原样包住（不脱出成 shell glob）；名字层另有
        // `isValidTmuxName` 禁 `*`/`?` 作第二道防线。
        assert_eq!(exact_target("si*").unwrap(), "'=si*:'");
        // 含单引号的名字仍被正确转义（shell_quote 的 '\'' 形态）。
        assert!(exact_target("a'b").unwrap().starts_with("'=a"));
        assert!(exact_target("a'b").unwrap().ends_with("b:'"));
        // Gate 1：空 target 必须被拒——`=:` 会被 tmux 解析成「当前会话」，是唯一真正危险的默认值。
        assert!(exact_target("").is_err(), "空 target 必须被 Gate 1 拒绝");
    }

    #[test]
    fn parse_multi_session() {
        // 真 TAB 分隔(Rust "\t" = 0x09)。6 列,末列 @ccm_sid。
        let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t2\tsess-42\nweb\t/srv/web\tzsh\t0\t1\t\n";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "cc-abc12345");
        assert_eq!(s[0].path, "/home/pi/proj");
        assert_eq!(s[0].command, "claude");
        assert!(s[0].attached);
        assert_eq!(s[0].windows, 2);
        // @ccm_sid 有值 → Some;空串 → None(向后兼容老会话)。
        assert_eq!(s[0].sid.as_deref(), Some("sess-42"));
        assert!(!s[1].attached);
        assert_eq!(s[1].command, "zsh");
        assert_eq!(s[1].sid, None);
    }

    /// F10：一次性用量探针会话的识别——纯前缀匹配，不涉及新 tmux user-option（不碰
    /// `TMUX_LS_FMT` 双写点，见 `USAGE_PROBE_NAME_PREFIX` 头注）。
    #[test]
    fn usage_probe_session_name_prefix() {
        assert!(is_usage_probe_session("ccm-usage-z"));
        assert!(is_usage_probe_session("ccm-usage-z-2")); // 撞名重试的 -N 变体
        assert!(!is_usage_probe_session("cc-abc12345")); // 正牌会话前缀，不该被误判
        assert!(!is_usage_probe_session("web")); // 用户自己的会话
        assert!(!is_usage_probe_session("ccm-usage")); // 无尾随连字符，不是前缀本身
        assert!(!is_usage_probe_session("")); // 空
    }

    /// F10：`list_remote_tmux` 对探针会话的过滤逻辑——`parse_tmux_ls` 之后接一次
    /// `is_usage_probe_session` 过滤，验证两者组合后正牌会话保留、探针会话消失（不需要真的
    /// 发起 SSH 连接，`list_remote_tmux` 内部这段处理是纯数据变换，抽取同样的组合方式单测）。
    #[test]
    fn list_remote_tmux_filters_out_usage_probe_sessions() {
        let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t1\tsess-1\nccm-usage-z\t/home/z\tclaude\t1\t1\t\nweb\t/srv/web\tzsh\t0\t1\t\n";
        // 走**生产同一条**代码路径（`list_remote_tmux` 内联调的就是它）——此前这里把过滤表达式
        // 在测试体里抄了一遍，删掉生产侧的 filter 照样绿，是伪测试（F10 Phase D 审计发现）。
        let filtered = parse_visible_tmux_sessions(out);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|s| s.name == "cc-abc12345"));
        assert!(filtered.iter().any(|s| s.name == "web"));
        assert!(!filtered.iter().any(|s| s.name == "ccm-usage-z"));
    }

    #[test]
    fn parse_skips_malformed_and_handles_edges() {
        // 空输出 → 空。
        assert!(parse_tmux_ls("").is_empty());
        assert!(parse_tmux_ls("\n\n").is_empty());
        // 字段数不符(无 TAB / 少字段 / 旧 5 列)→ 跳过;name 空 → 跳过。
        let out = "no tabs here\nn\t/p\tsh\t0\n\t/p\tclaude\t1\t1\told5\t/p\tclaude\t1\t2\ngood\t/home/a b\tclaude\t1\t3\t";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 1, "只有最后一行(6 列)合法");
        assert_eq!(s[0].name, "good");
        // 路径含空格(非 TAB)保留。
        assert_eq!(s[0].path, "/home/a b");
        assert_eq!(s[0].windows, 3);
        // 末列空串 → sid None。
        assert_eq!(s[0].sid, None);
    }

    #[test]
    fn parse_windows_nonnumeric_falls_back_zero() {
        let s = parse_tmux_ls("n\t/p\tclaude\t1\tNaN\tsid-x");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].windows, 0);
        assert_eq!(s[0].sid.as_deref(), Some("sid-x"));
    }

    #[test]
    fn parse_sid_rejects_unexpanded_format_and_garbage() {
        // 极老 tmux 不展开 `#{@ccm_sid}` → 原样字面串(含 `#{}`)→ 当 None,否则 findClaudeTmux 的
        // anySidKnown 恒真、老 wrapper 用户永远走不到 cwd 回退(审计建议)。
        let s = parse_tmux_ls("n\t/p\tclaude\t1\t1\t#{@ccm_sid}");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sid, None, "未展开格式串不当 sid");
        // 合法 sid 字符集(字母数字 + - + _)照收。
        let s2 = parse_tmux_ls("n\t/p\tclaude\t1\t1\tab_c-12");
        assert_eq!(s2[0].sid.as_deref(), Some("ab_c-12"));
    }

    #[test]
    fn classify_capture_output_sentinels_and_text() {
        // 正常屏文本原样返回(含尾换行不误判)。
        assert_eq!(
            classify_capture_output("$ ls\nfoo bar\n").unwrap(),
            "$ ls\nfoo bar\n"
        );
        // 哨兵 → Err。
        assert!(classify_capture_output("NO_TMUX\n").is_err());
        assert!(classify_capture_output("NO_PANE\n").is_err());
        assert!(classify_capture_output("NO_TMUX").is_err()); // 无尾换行也判
                                                              // 屏内容里恰有一行 NO_PANE(但非唯一 trim 内容)→ 不误判,正常返回。
        assert!(classify_capture_output("foo\nNO_PANE\nbar\n").is_ok());
        // 空屏 → Ok(空/空白),非哨兵。
        assert!(classify_capture_output("").is_ok());
    }

    #[test]
    fn fmt_uses_real_tab_not_literal_backslash_t() {
        // 回归调研 03 §3.1 坑:格式串里必须是真 TAB 字节,不能是字面 \t。
        assert!(TMUX_LS_FMT.contains('\t'), "格式串须含真 TAB");
        assert!(!TMUX_LS_FMT.contains("\\t"), "格式串不得含字面反斜杠-t");
    }

    #[test]
    fn tmux_ls_fmt_double_write_point_stays_in_sync() {
        // F08a：TMUX_LS_FMT 双写点断言（红线 I8 的机器化护栏）。monitor(本 const) 与 daemon
        // (`remote-daemon-proto/src/observe/watcher.rs`) 分属两个独立 crate、不能共享 const，但两侧
        // `tmux ls -F` 格式串**必须逐字一致**（否则 daemon 推的列 monitor 解错位）。编译期
        // include_str! 读 daemon 源，把本 const 的真 TAB 折回源码里的 `\t` 转义再断言 daemon 源
        // 含该带引号字面量——**双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
        let daemon_src = include_str!(daemon_watcher_src!());
        let source_literal = TMUX_LS_FMT.replace('\t', "\\t");
        // 锚定到 const 定义行（非裸字面量）——否则该字面量若也出现在某条注释里，会掩盖真 const 漂移
        // （假阴性）。daemon 侧常量名同为 TMUX_LS_FMT（红线 I8 不许改），故按定义行精确比对。
        let expected_def = format!("const TMUX_LS_FMT: &str = \"{source_literal}\";");
        assert!(
            daemon_src.contains(&expected_def),
            "TMUX_LS_FMT 双写点漂移：daemon watcher.rs 不含与 monitor 侧一致的定义 {expected_def:?}\n\
             （改了 tmux ls 格式串就得两侧同步——红线 I8）"
        );
    }

    /// F04 真机验收的**输入源**（同 `e2e/tmux-target-emit.mts` 对 TS 侧的模式：从真 builder 取
    /// 生产命令串，不手搓等价命令）。`#[ignore]`——不在常规 `cargo test` 里跑，只由
    /// `e2e/tmux-guarded-acceptance.sh` 用 `cargo test --lib -- --ignored --nocapture` 触发，
    /// 把三道门的**真实**构造命令打到 stdout（`<key>\t<命令串>`），喂给隔离 `-L` socket 验证
    /// "这条命令在 tmux 上真的干了什么"——门禁只锁字符串形状不锁行为是 R1 的教训，本模块新增的
    /// 三道门原子命令构造有真实的 shell 语法复杂度（嵌套 if/then/else、`cut -f1/-f2`），必须过
    /// 真机而非只信 Rust 单测的字符串断言。
    #[test]
    #[ignore]
    fn emit_guarded_commands_for_e2e() {
        let emit = |key: &str, r: Result<String, String>| {
            println!("{key}\t{}", r.unwrap_or_else(|e| format!("ERR:{e}")));
        };
        // kill：cc-* 前缀命中（本地已过 Gate2）→ 只需远端 windows 核验（Gate 3）。
        emit("kill_owned", build_kill_session_cmd("cc-e2e-owned"));
        // kill：非前缀但字符安全 → 远端核验 @ccm_sid（Gate 2 远端半支）+ windows（Gate 3）。
        emit("kill_custom", build_kill_session_cmd("e2e-custom"));
        // send-keys：cc-* 前缀命中 → 退化成今天的零 Gate 一行。
        emit(
            "send_keys_owned",
            build_send_keys_remote_cmd("cc-e2e-owned", "CCMPROBE", true),
        );
        // send-keys：非前缀但字符安全 → 远端核验 @ccm_sid（无 Gate 3，不删东西）。
        emit(
            "send_keys_custom",
            build_send_keys_remote_cmd("e2e-custom", "CCMPROBE", true),
        );
    }
}
