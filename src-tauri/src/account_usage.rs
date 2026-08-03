//! F10（unify-launch，剩余账号 UX）：每账号 Claude 订阅计划用量窗口百分比（"plan 窗口%"）——
//! 不是 context window 用量（那是 `usage-hud.ts` 的事），不是本地 token 累计（那是
//! `usage.rs`/`views/usage-view.ts` 的事），是 Anthropic 服务端权威的 5h/周额度窗口剩余%，
//! 必须真的起一个已登录的 claude 会话跑 `/usage` 斜杠命令、capture-pane 抓屏解析。
//!
//! **本模块只负责编排一次性探针会话本身**（建/等/送键/抓屏/清理），完全不理解 `/usage`
//! 输出的语义——那是 TS 侧 `src/account-usage-parse.ts` 纯函数的职责。
//!
//! ⚠ **U8c-2a 起载荷不再由 TS 传进来** —— IPC 收的是**结构化账号表态**
//! （`config_dir: Option<String>`），载荷由 `launch_core::usage_probe_payload` 编译
//! （见 [`probe_payload_for`]）。TS 的 `buildUsageProbePayload` 已删除。
//! **Z03 起它有两种形态**，账号维度必定显式表态、不存在裸载荷：
//!   - 具名账号：`export CLAUDE_CONFIG_DIR=...; unset <嵌套env>; claude`
//!   - **账号 0**：`unset CLAUDE_CONFIG_DIR; unset <嵌套env>; claude`
//!     （**不能省成裸载荷**——远端 rc 里那句 `export CLAUDE_CONFIG_DIR=<默认账号>` 会让
//!     探针探到别的号，而 UI 会把结果标成账号 0 的用量 = 静默串号）
//!
//! **命名与识别**：探针会话固定命名 `ccm-usage-<slug>`（`slug` 是账号名的安全化版本）——
//! 这个前缀（`tmux.rs::USAGE_PROBE_NAME_PREFIX`）专属本功能，不会被其它任何路径创建，因此
//! 名字本身就是所有权证明：每次探测前先无条件清掉同名残留（不需要额外的 tmux user-option
//! 打标去区分"是不是自己的"，见 `tmux.rs::is_usage_probe_session` 头注——那条注释解释了为什么
//! **不**用新 tag 列，改走名字前缀）。
//!
//! **孤儿防护**：自毁看门狗（`setsid`+`sleep 30`+`kill-session`）独立于 SSH 通道是否存活——
//! 即便本次 exec 因网络中断"跑不完"，这个已脱离本次会话的远端后台进程仍会在 30s 内把探针会话
//! 杀掉。**不用 `disown`**（Phase D 后端审计指出）：`setsid` 已经把它放进一个全新 session，
//! shell 退出时的 SIGHUP 只发给控制终端的前台进程组，根本到不了它——`disown` 是多余的；而它是
//! bash/zsh builtin、POSIX sh 没有，远端登录 shell 若是 dash 会报 `disown: not found`。本探针
//! 命令其余部分（`command -v`/`[ -n "$cur" ]`/`printf`）都是严格 POSIX，不该只有它一个例外。
//! **刻意不做**"扫描 tmux 列表、启发式判定孤儿、弹确认批量清理"这类通用机制——这个
//! 仓库已经做过又主动砍掉过这个模式（见 `.claude/planned-build/audit-fixes/features/
//! 05-cleanup-orphans.md` 落地、`.claude/planned-build/auto-e2e/features/
//! remove-orphan-cleanup.md` 因"UX 审计 footgun：把别窗口/实例正跑的活会话误列孤儿劝杀"而
//! 删除），本模块的探针生命周期管理是自包含、确定性的，不重蹈覆辙。
//!
//! **不新增轮询**：探针只在前端按需调用时触发（面板"查看用量"按钮/chip 菜单展开），没有
//! 任何 `setInterval`/定时任务。

use crate::ssh_source;
use tokio::io::{AsyncReadExt, BufReader};

/// 探针会话固定几何尺寸——较宽的列数减少 `/usage` 表格换行/裁切风险（真机验证前的保守选择，
/// 见 F10 计划 §7 真机验证清单第 5 条）。
const PROBE_COLS: u32 = 200;
const PROBE_ROWS: u32 = 50;
/// 看门狗自毁超时（秒）——正常路径下探针几秒内就该跑完+清理，这是"万一跑不完"的保险丝。
const WATCHDOG_TIMEOUT_SECS: u32 = 30;
/// 画面稳定轮询：用"抓屏内容连续多久没变"代替固定 sleep 猜测冷启动/网络查询耗时
/// （真机耗时未知，且这个判据格式无关、版本无关，见 F10 计划 §1 设计说明）。
const QUIESCENCE_POLL_INTERVAL_MS: u32 = 500;
/// 判定"画面稳定"所需的**连续无变化次数**（× 间隔 = 静止时长）。6 × 0.5s = 3s。
///
/// ★ **这是 E42 的真正修复点**，且是唯一有实测支撑的那一半
/// （`e2e/usage-probe-acceptance.sh` 场景 4：把它调回 1 就红，其余场景全绿）。
/// 之所以是"静止时长"而不是"画面变过没有"：`send-keys '/usage'` 打进去的字符**会被终端
/// 回显**，屏幕在毫秒级就变了 —— 任何"变过就算数"的判据都会被回显自己满足
/// （我第一版修法就是这么错的，已在真 tmux 上证伪）。**能区分"渲染完了"和"还在等"的
/// 只有：静止得够久。**
///
/// 3s 是**预算，不是测量值**：本仓库不允许起真实已认证的 claude 去测真实渲染耗时
/// （消耗真实订阅额度、且与用户当前会话交互不可控）。取 3s 的依据是它显著大于回显与
/// TUI 重绘的时间尺度（~10ms 级），又装得进下面的时间预算。**残余风险如实说**：真 claude
/// 若在渲染途中静止超过 3s（如网络请求卡顿），仍会抓早 —— 那种情况下解析器返回
/// `unrecognized` 并把原始屏带回 UI（"复制诊断文本"），是**可见失败，不是静默错值**。
const QUIESCENCE_STILL_POLLS: u32 = 6;
/// 两段等待的轮询上限**分开给**——它们等的不是一回事，预算也不该平摊。
///
/// 第一段等 claude 的 REPL 起来（本地进程启动，快）；第二段等 `/usage` 的面板渲染出来
/// （要拉一次用量数据，可能走网络，慢）。总和受 [`EXEC_TIMEOUT_SECS`] 约束，
/// 由 `time_budget_ordering_holds` 钉住。
const STARTUP_MAX_POLLS: u32 = 12; // 6s
const RENDER_MAX_POLLS: u32 = 20; // 10s
/// Rust 侧整条 exec 的硬超时——防 SSH 通道本身卡死导致 `account_usage` 永久挂起（比现有
/// `tmux.rs` 几个近乎瞬时往返的命令更谨慎，因为这次故意要在远端阻塞较久）。
const EXEC_TIMEOUT_SECS: u64 = 25;

#[derive(serde::Serialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageProbeResult {
    /// true = 拿到了屏幕文本（不代表内容可解析——解析是 TS 侧纯函数 `parseUsageCapture` 的职责）。
    pub captured: bool,
    /// `captured=true` 时的 capture-pane 原始文本。
    pub raw: Option<String>,
    /// `captured=false` 时的人话原因（无 tmux / 连接失败 / 超时）。
    pub error: Option<String>,
}

/// 把账号名安全化成可以嵌进 tmux 会话名的 slug——只留 `[A-Za-z0-9_-]`，其余字符丢弃；
/// 结果为空（如账号名全是非常规字符,理论上 `validateAcctName` 早已在创建时拒绝这类名字,
/// 这里是纵深防御,不信任调用方）时兜底成 `x`,保证候选名恒非空、恒安全。
fn slugify_account_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if s.is_empty() {
        "x".to_string()
    } else {
        s
    }
}

/// 每轮 `sleep` 的秒数字面量（由 [`QUIESCENCE_POLL_INTERVAL_MS`] 推导，不双写）。
fn poll_interval_secs() -> String {
    format!(
        "{}.{:03}",
        QUIESCENCE_POLL_INTERVAL_MS / 1000,
        QUIESCENCE_POLL_INTERVAL_MS % 1000
    )
}

/// 稳定轮询片段：抓屏直到「**相对基线变过** 且 **连续静止够久**」，或到 `max_polls` 上限。
///
/// ★ 2026-07-31（E42）重做。原判据是「连续两次一致且非空」，间隔 0.5s ⇒ send-keys
/// `/usage` 之后 t=0.5s / t=1.0s 抓到的都还是渲染前的画面，两次相等 ⇒ 立刻 break ⇒
/// 抓回去的屏上根本没有 /usage 面板。用户实测症状正是「抓到了屏幕但认不出格式」。
///
/// 判据两半，**证据强度不同，别当成一回事**：
///
/// 1. 连续 [`QUIESCENCE_STILL_POLLS`] 次无变化 —— **修复 E42 的就是这一半**，
///    有 e2e 场景 4（慢速 stand-in）红/绿两向实测。
/// 2. `cur != base` —— 排除"什么都没发生"（键没送到 / 会话没起来）。
///    **这一半没有实测支撑，是推理**：拿掉它 e2e 仍 9/9 全绿（实测过）。留着的理由是它守
///    第 1 半守不住的那个形态 —— 「渲染前的画面本身就静止 ≥3s」，典型是 claude 启动慢时
///    第一段等待停在静止的 shell 提示符上，于是 `/usage` 在 TUI 的输入框就绪前就被送出去、
///    被真 claude 丢掉。e2e 复现不了它，因为 stand-in 是个 shell：**tty 会把早到的按键缓冲
///    住**，等程序开始 read 时照样交付，而真 TUI 不会。代价有界（最多多等到上限，且届时
///    抓到的内容与提前 break 时相同），所以按 fail-safe 留下，但不谎称它被验证过。
///
/// `$base` 由调用点在每次 send-keys **之前**取；只取一次不行（第二段会拿"claude 已起来"
/// 的屏当基线，第 2 半退化成恒真）。
fn quiescence_wait(t: &str, max_polls: u32) -> String {
    let interval = poll_interval_secs();
    let still = QUIESCENCE_STILL_POLLS;
    format!(
        "prev=''; same=0; i=0; while [ $i -lt {max_polls} ]; do \
sleep {interval}; \
cur=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
if [ -n \"$cur\" ] && [ \"$cur\" = \"$prev\" ]; then same=$((same+1)); else same=0; fi; \
prev=\"$cur\"; \
if [ -n \"$cur\" ] && [ \"$cur\" != \"$base\" ] && [ $same -ge {still} ]; then break; fi; \
i=$((i+1)); \
done"
    )
}

/// 构造整条探针远端脚本（纯函数，可单测——同 `build_capture_pane_cmd`/`build_kill_session_cmd`
/// 既有惯例：编排逻辑与"怎么发起 SSH exec"分离）。
///
/// 算法：清掉同名残留 → 建会话（固定几何尺寸）→ 挂自毁看门狗 → send-keys 启动 payload →
/// 稳定轮询 → send-keys `/usage` → 稳定轮询 → capture-pane → kill-session → 输出。
/// `command -v tmux` 门控：无 tmux → 哨兵 `NO_TMUX`（同仓库其余 tmux 命令的既有惯例）。
///
/// `watchdog_timeout_secs` 独立于 `WATCHDOG_TIMEOUT_SECS` 常量传入——生产恒传该常量
/// （30s）；真机 e2e（`e2e/usage-probe-acceptance.sh`）验证看门狗本身时需要一个短得多的值
/// 才能在合理时间内跑完测试，不代表生产行为可配置。
///
/// **fallible**（F10 Phase D 后端审计）：`-t` 目标一律经 `tmux::exact_target` 产出，不再手抄
/// `shell_quote(&format!("={session}:"))`——那条手抄绕过了 Gate 1（空 target 恒拒），而
/// `exact_target` 的头注恰恰声称"任何未来新增的 tmux 命令构造点都结构性不可能绕过它"。
/// 走真函数就要接它的 `Result`，这个"麻烦"正是结构保证本身。
fn build_usage_probe_cmd(
    account_slug: &str,
    launch_payload: &str,
    watchdog_timeout_secs: u32,
) -> Result<String, String> {
    let session = format!("ccm-usage-{account_slug}");
    let t = crate::tmux::exact_target(&session)?;
    let payload_q = ssh_source::shell_quote(launch_payload);
    let startup_wait = quiescence_wait(&t, STARTUP_MAX_POLLS);
    let render_wait = quiescence_wait(&t, RENDER_MAX_POLLS);

    Ok(format!(
        "if command -v tmux >/dev/null 2>&1; then \
tmux kill-session -t {t} >/dev/null 2>&1 || true; \
tmux new-session -d -s {session_q} -x {PROBE_COLS} -y {PROBE_ROWS}; \
setsid sh -c 'sleep {watchdog_timeout_secs}; tmux kill-session -t {t} >/dev/null 2>&1' </dev/null >/dev/null 2>&1 & \
base=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
tmux send-keys -t {t} {payload_q} Enter; \
{startup_wait}; \
base=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
tmux send-keys -t {t} '/usage' Enter; \
{render_wait}; \
out=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
tmux kill-session -t {t} >/dev/null 2>&1 || true; \
printf '%s' \"$out\"; \
else printf 'NO_TMUX\\n'; fi",
        session_q = ssh_source::shell_quote(&session),
    ))
}

/// **结构化账号表态 → 整条远端探针命令**（U8c-2a：`account_usage` 的构造那一段整体抽成纯函数）。
///
/// # 为什么要抽出来（代码审计 R1–R4）
///
/// 抽之前，「载荷编译 + 命令编排」两段都长在 `account_usage` 这个 **async tauri 命令**里，
/// 于是它们**没有任何单测能到达**。审计实测四个变异在 729 条 Rust + 1168 条 TS 全绿下存活：
/// 恒当账号 0 · 探写死的别的号 · 只清一个嵌套 env 键 · 换掉启动器。
/// 前两个正是这套设计从头到尾要防的形态 —— **探到别的号、UI 标成本账号 = 静默串号**。
///
/// 抽成纯函数之后，后两个（键表 / 启动器）由下面的逐字节断言杀掉，
/// 前两个的接缝缩成 `account_usage` 里**一行、一个 token**（`config_dir.as_deref()`）。
///
/// ⚠ **诚实边界（登记在案，不假装做完了）**：那一行本身**仍然没有判据**。
/// 代码审计实测：把它改成恒传 `None`（恒当账号 0）或写死别的号，
/// **729 条 Rust + 1168 条 TS 全绿**。它正是这套设计要防的形态 —— 探到别的号、
/// UI 标成本账号 = **静默串号**。
/// 审计试过的两种纯函数写法都杀不掉它（接缝只是换了位置）；要真钉住，得让
/// `emit_usage_probe_cmd_for_e2e` 改由**真接线**驱动（`Some(dir)`/`None` 两态各发一条场景），
/// 让 usage-probe 那 9 条 e2e 覆盖到。⇒ **U8c-2b 或独立一件。**
fn probe_command_for(
    slug: &str,
    config_dir: Option<&str>,
    watchdog_timeout_secs: u32,
) -> Result<String, String> {
    build_usage_probe_cmd(slug, &probe_payload_for(config_dir)?, watchdog_timeout_secs)
}

/// 只产**载荷**那一段（不含外层 tmux 编排）。
///
/// 与 [`probe_command_for`] 分开是为了**可断言** —— 载荷进整条命令时会被 `shell_quote`
/// 包一层，在命令串上做逐字节断言等于顺带在断言引号算法，噪音盖过信号。
fn probe_payload_for(config_dir: Option<&str>) -> Result<String, String> {
    // 键表与启动器都走活跃适配器 —— 它们各自已有 TS↔Rust 对拍守卫。
    let agent = crate::adapter::active();
    launch_core::usage_probe_payload(
        config_dir,
        agent.nested_env_to_scrub(),
        agent.default_launcher(),
    )
}

/// F10：per-account 探测 Claude 订阅计划用量窗口%（"plan 窗口%"）。通道 B（一次性 headless
/// exec，不占用前台可见终端，同 `list_remote_tmux`/`capture_remote_pane` 既有分工）。
///
/// `account_name` 只用于探针会话名 slug + 错误文案，不参与鉴权（鉴权/账号存在性由 TS 侧调用
/// 前已经确认过）。
///
/// # U8c-2a：**收结构化账号表态，不再收渲染好的串**
///
/// 此前这里收的是 `launch_payload: String` —— TS 的 `buildUsageProbePayload` 渲染好递进来，
/// 本模块「只透传不校验」。那是账本 S28 里六个载荷产出点的第 ②。现在它退役了：
/// 前端只报「哪个账号」，载荷由 `launch_core::usage_probe_payload` 编译。
///
/// `config_dir` **两态，没有第三态**（探针恒是 per-account）：
/// `Some(路径)` = 具名账号 · `None` = **账号 0**（产出 `unset CLAUDE_CONFIG_DIR; `，
/// 绝不是「什么都不加」——那会让探针落到远端 rc 的默认号上 = 静默串号）·
/// `Some("")` = 坏数据 ⇒ 诚实回报 probe-failed。
#[tauri::command]
pub async fn account_usage(
    origin: String,
    account_name: String,
    config_dir: Option<String>,
) -> Result<AccountUsageProbeResult, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let slug = slugify_account_name(&account_name);
    // 载荷由内核编译（`launch-core`）：账号前缀 + 嵌套 env 清理 + 启动器，无 cd。
    // 构造失败（载荷非法 / Gate 1 拒绝）→ 诚实回报，**不发起任何 SSH 连接**。
    let cmd = match probe_command_for(&slug, config_dir.as_deref(), WATCHDOG_TIMEOUT_SECS) {
        Ok(c) => c,
        Err(e) => {
            return Ok(AccountUsageProbeResult {
                captured: false,
                raw: None,
                error: Some(e),
            });
        }
    };

    let exec = async {
        let stream = ssh_source::connect_and_exec_cmd(&cfg, &cmd).await?;
        let mut reader = BufReader::new(stream);
        let mut buf: Vec<u8> = Vec::new();
        reader
            .read_to_end(&mut buf)
            .await
            .map_err(|e| format!("读用量探针输出失败: {e}"))?;
        Ok::<Vec<u8>, String>(buf)
    };

    let buf =
        match tokio::time::timeout(std::time::Duration::from_secs(EXEC_TIMEOUT_SECS), exec).await {
            Ok(Ok(buf)) => buf,
            Ok(Err(e)) => {
                return Ok(AccountUsageProbeResult {
                    captured: false,
                    raw: None,
                    error: Some(e),
                });
            }
            Err(_) => {
                return Ok(AccountUsageProbeResult {
                    captured: false,
                    raw: None,
                    error: Some(format!(
                        "探测超时（{EXEC_TIMEOUT_SECS}s）——远端连接可能卡住，稍后重试"
                    )),
                });
            }
        };

    let out = String::from_utf8_lossy(&buf);
    if out.trim() == "NO_TMUX" {
        return Ok(AccountUsageProbeResult {
            captured: false,
            raw: None,
            error: Some("远端未安装 tmux".to_string()),
        });
    }
    Ok(AccountUsageProbeResult {
        captured: true,
        raw: Some(out.to_string()),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_only_safe_chars() {
        assert_eq!(slugify_account_name("z"), "z");
        assert_eq!(slugify_account_name("my-account_2"), "my-account_2");
        assert_eq!(slugify_account_name("a b;rm -rf"), "abrm-rf");
        assert_eq!(slugify_account_name("日本語"), "x"); // 全非 ASCII 字母数字 → 兜底
        assert_eq!(slugify_account_name(""), "x");
        // 长度截断：防止一个异常长的账号名把远端命令串撑得过长。
        let long = "a".repeat(100);
        assert_eq!(slugify_account_name(&long).len(), 32);
    }

    /// ★ U8c-2a-fix：**把 `account_usage` 那一行接线钉住**（上一轮如实登记为「仍未做完」的债）。
    ///
    /// 上一轮抽出纯函数只把接缝从「一整段」缩成一行一个 token —— 代码审计实测，把
    /// `config_dir.as_deref()` 改成 `None`（恒当账号 0）或写死别的号，
    /// **731 条 Rust + 1168 条 TS 全绿**。那正是这套设计从头到尾要防的形态：
    /// **探到别的号、UI 标成本账号 = 静默串号**。
    ///
    /// # 为什么是源码守卫，以及它证明不了什么
    ///
    /// 「这个 async tauri 命令有没有把它收到的参数转发下去」**在 Rust 类型里表达不了**
    /// —— 换成任何一层包装，变异只会跟着下移一层（审计试过两种写法，都杀不掉）。
    /// 真正的行为判据要让 `usage-probe` 的 e2e 由真接线驱动，而那条路今天走不通：
    /// e2e 用的是 `FAKECLAUDE` stand-in，而 `probe_payload_for` 里的启动器来自
    /// `agent.default_launcher()`（恒 `claude`）—— 沙箱里没有真 claude。
    ///
    /// ⇒ 退而求其次：**钉住那一行的源码形态**。它是**约定不是事实**（同
    /// `protocol_doc_guard` 的 `doc_anchor`、`TS_HALF` 那一族），能挡住的是
    /// 「顺手把参数换成常量」这一类改动，挡不住「换个名字继续错」。**比没有强，但别读成证明。**
    #[test]
    fn account_usage_actually_forwards_the_config_dir_it_received() {
        let src = guard_core::production_code(include_str!("account_usage.rs"));
        // 只数**调用点** —— `fn probe_command_for(` 那个定义也含同一串，不能算进来。
        //
        // ⚠ **括号要配平**（2026-08-03 复盘 P3 实测修的）：初版取到**第一个** `)` 为止，
        // 于是 `probe_command_for(&slug, None, watchdog_for(config_dir.as_deref()))`
        // 这种形态里，`config_dir` 从**第三个实参**漏进窗口 ⇒ 第二个实参明明是 `None`
        // （恒当账号 0 = 静默串号，正是本条要防的），守卫却 16 passed 全绿。
        // 现在按括号深度取完整实参表，再按**顶层逗号**切开，只看**第二个**实参。
        let calls: Vec<Vec<String>> = src
            .match_indices("probe_command_for(")
            .filter(|(i, _)| !src[..*i].ends_with("fn "))
            .map(|(i, _)| {
                let rest = &src[i + "probe_command_for(".len()..];
                let mut depth = 0usize;
                let mut args: Vec<String> = vec![String::new()];
                for c in rest.chars() {
                    match c {
                        '(' | '[' => depth += 1,
                        ')' | ']' if depth == 0 => break,
                        ')' | ']' => depth -= 1,
                        ',' if depth == 0 => {
                            args.push(String::new());
                            continue;
                        }
                        _ => {}
                    }
                    args.last_mut().unwrap().push(c);
                }
                args.into_iter().map(|a| a.trim().to_string()).collect()
            })
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "生产段里 `probe_command_for` 的调用点不是恰好一个（实得 {}）—— \
             多一个就说明有第二条路绕过了这条判据",
            calls.len()
        );
        // `probe_command_for(slug, config_dir, watchdog)` —— 三个实参，配平后必须切出三段。
        assert_eq!(
            calls[0].len(),
            3,
            "实参切成了 {} 段（应为 3）：{:?} —— 括号配平/切分坏了，下面那条会看错格子",
            calls[0].len(),
            calls[0]
        );
        assert!(
            calls[0][1].contains("config_dir"),
            "`account_usage` 没有把它收到的 `config_dir` 转发下去（第二个实参：`{}`；\
             整个实参表：{:?}）—— 恒当账号 0 或写死别的号 = 静默串号，\
             而其余全部判据都会保持绿",
            calls[0][1],
            calls[0]
        );
    }

    /// ★ U8c-2a（代码审计 R1–R4 的收口）：**两态各自的载荷逐字节钉住**。
    ///
    /// 这条杀掉的是「载荷编译搬进 Rust 之后接线没人管」那一类：审计实测
    /// 「只清一个嵌套 env 键」「换掉启动器」两个变异在全绿门禁下存活过。
    ///
    /// 它同时接住了搬家前那条 TS 测试（`launchPayload` 逐字节）钉的三件事：
    /// ① 账号隔离真的通过 `CLAUDE_CONFIG_DIR` 生效（**不是裸 claude** —— 那会探到错账号
    /// 的用量且看起来完全正常）· ② 嵌套 env 被清掉 · ③ 引号形态。
    ///
    /// ⚠ **键序与 TS `AGENT_PROFILE.nestedEnvVars` 同序是刻意的**（见
    /// `adapter/claude_code.rs::CLAUDE_NESTED_ENV` 头注）：搬家前后送到远端的字节**完全相同**。
    #[test]
    fn probe_payload_is_byte_exact_for_both_account_states() {
        const NESTED: &str =
            "unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; ";
        // 这两串**逐字节等于搬家前 TS `buildUsageProbePayload` 的产出**（键序刻意同 TS）。
        assert_eq!(
            probe_payload_for(Some("/h/.claude-accts/z")).unwrap(),
            format!("export CLAUDE_CONFIG_DIR='/h/.claude-accts/z'; {NESTED}claude")
        );
        let zero = probe_payload_for(None).unwrap();
        assert_eq!(zero, format!("unset CLAUDE_CONFIG_DIR; {NESTED}claude"));
        // ★ 最要紧的一条：账号 0 **绝不**退化成裸载荷（那会继承远端 rc 里的默认号 = 静默串号）。
        assert!(
            !zero.contains("export CLAUDE_CONFIG_DIR="),
            "账号 0 竟带上了 export：\n{zero}"
        );
        // 接缝判据：那条载荷真的被塞进了整条命令，中间这一步不是摆设。
        let named = probe_payload_for(Some("/h/.claude-accts/z")).unwrap();
        let cmd = probe_command_for("z", Some("/h/.claude-accts/z"), 30).unwrap();
        assert!(
            cmd.contains(&ssh_source::shell_quote(&named)),
            "命令里找不到那条载荷：\n{cmd}"
        );
    }

    /// 空 configDir 是坏数据 ⇒ 命令根本构造不出来（**不发起 SSH**）。
    #[test]
    fn empty_config_dir_never_produces_a_probe_command() {
        assert!(probe_command_for("z", Some(""), 30).is_err());
        assert!(probe_command_for("z", Some("/h/a;rm -rf /"), 30).is_err());
        // 反向自检：合法输入必须构造得出来，否则上面两条是空转。
        assert!(probe_command_for("z", Some("/h/.claude-accts/z"), 30).is_ok());
    }

    #[test]
    fn probe_cmd_uses_dedicated_prefix_and_exact_target() {
        let cmd =
            build_usage_probe_cmd("z", "export FOO=1; claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        assert!(cmd.contains("ccm-usage-z"), "会话名须含专属前缀+slug");
        // exact_target 惯例（=name:）——同仓库其余 tmux 命令一致，防前缀/glob 误命中。
        assert!(
            cmd.contains("'=ccm-usage-z:'"),
            "target 须是 =name: 精确形式"
        );
        // Phase D 后端审计：这个串必须由 `tmux::exact_target` **真的产出**，不是手抄一个长得像
        // 的字符串。对拍真函数的返回值——手抄版一旦与 `exact_target` 的规则漂移（比如它日后
        // 改了引号策略），这条会红。
        assert!(
            cmd.contains(&crate::tmux::exact_target("ccm-usage-z").unwrap()),
            "target 须由 tmux::exact_target 产出，不得手抄"
        );
    }

    /// Phase D 后端审计的**订正**：最初这条测试断言"空 slug → Gate 1 拒绝"，实测红了——
    /// 因为会话名是 `ccm-usage-{slug}`，常量前缀让它**恒非空**，Gate 1（只拒空 target）
    /// 在本构造点结构上就不可达。所以走 `exact_target` 的真实收益**不是**空值防护，而是
    /// `=name:` 引号规则的单一事实来源：它日后若改引号策略，本模块自动跟随、不会漂移。
    /// 这条测试锁的就是这件事——顺带钉死"前缀保证非空"这个让 Gate 1 不可达的前提，
    /// 万一有人把前缀改成可空的，这里会红。
    #[test]
    fn probe_cmd_target_tracks_exact_target_and_prefix_keeps_gate1_unreachable() {
        for slug in ["z", "", "collision"] {
            let cmd = build_usage_probe_cmd(slug, "claude", WATCHDOG_TIMEOUT_SECS)
                .expect("前缀恒非空 → Gate 1 不可达，构造不该失败");
            let expected = crate::tmux::exact_target(&format!("ccm-usage-{slug}")).unwrap();
            assert!(
                cmd.contains(&expected),
                "slug={slug:?} 的 target 须与 exact_target 产出逐字一致"
            );
        }
    }

    #[test]
    fn probe_cmd_kills_stale_session_before_creating() {
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        let kill_pos = cmd.find("tmux kill-session").expect("应先清场");
        let new_pos = cmd.find("tmux new-session").expect("应再建会话");
        assert!(
            kill_pos < new_pos,
            "清场必须先于建会话（同名探针名字前缀专属，可安全无条件清）"
        );
    }

    #[test]
    fn probe_cmd_watchdog_is_setsid_detached_and_posix_only() {
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        assert!(
            cmd.contains("setsid"),
            "看门狗须用 setsid 放进新 session，独立于本次 SSH exec 通道存活"
        );
        assert!(
            cmd.contains(&format!("sleep {WATCHDOG_TIMEOUT_SECS}")),
            "看门狗超时须用配置的常量,不能是魔法数字"
        );
        // Phase D 后端审计（重要）：整条探针命令由远端 sshd 用**用户的登录 shell** 执行
        // （russh `channel.exec`），那可能是 dash。`disown` 是 bash/zsh builtin、POSIX sh 没有，
        // 且在 `setsid` 之后毫无作用（新 session 收不到控制终端的 SIGHUP）。这条断言防它被
        // "顺手加回来"——不是风格洁癖，是真会在 dash 登录 shell 上打出 `disown: not found`。
        assert!(
            !cmd.contains("disown"),
            "不得使用 disown（非 POSIX，且 setsid 之后是多余的）"
        );
        // 同理：看门狗内层用 `sh -c` 不用 `bash -c`——远端不保证装了 bash。
        assert!(
            !cmd.contains("bash -c"),
            "看门狗内层不得写死 bash，远端不保证有"
        );
    }

    #[test]
    fn probe_cmd_sends_payload_then_usage_with_quiescence_waits_between() {
        let cmd = build_usage_probe_cmd("z", "export X=1; claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        let payload_pos = cmd.find("export X=1").expect("须发送启动 payload");
        let usage_pos = cmd.find("'/usage'").expect("须发送 /usage 斜杠命令");
        assert!(
            payload_pos < usage_pos,
            "先起 claude 再发 /usage，顺序不能反"
        );
        // 两次 send-keys 之间必须有稳定轮询（不是固定 sleep），断言轮询逻辑的关键片段在两次
        // send-keys 之间各出现一次。
        let between = &cmd[payload_pos..usage_pos];
        assert!(
            between.contains("capture-pane"),
            "两次 send-keys 之间应有抓屏轮询,不是纯 sleep"
        );
    }

    #[test]
    fn probe_quiescence_requires_the_screen_to_change_before_accepting_stability() {
        // ★ E42 回归钉（2026-07-31 用户实测：「抓到了屏幕但认不出格式」）。
        //
        // 这条只钉**结构**（两半判据都在、基线取的位置对）。判据的**行为**由
        // `e2e/usage-probe-acceptance.sh` 场景 4（慢速 stand-in）钉——那才是能证伪它的地方，
        // 秒回的 stand-in 无论判据多松都会绿。两处分工写在 `quiescence_wait` 的文档注释里。
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        assert!(
            cmd.contains(r#"[ "$cur" != "$base" ]"#),
            "稳定判据须含「相对基线变过」这一半（fail-safe，理由见 quiescence_wait 注释）：{cmd}"
        );
        assert!(
            cmd.contains(&format!("[ $same -ge {QUIESCENCE_STILL_POLLS} ]")),
            "稳定判据须含「连续静止够久」这一半——**修复 E42 的正是它**：{cmd}"
        );

        // 「变过」只有配上「每次 send-keys 前重取基线」才成立。基线若只取一次，第二段等待会
        // 拿第一段结束时的旧屏当基线——那时 claude 已经起来了，判据退化回原来的坏行为。
        let sends: Vec<usize> = cmd
            .match_indices("tmux send-keys")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            sends.len(),
            2,
            "应恰好两次 send-keys（payload + /usage）：{cmd}"
        );
        for (n, &pos) in sends.iter().enumerate() {
            let before = &cmd[..pos];
            let base_pos = before
                .rfind("base=\"$(tmux capture-pane")
                .unwrap_or_else(|| panic!("第 {} 次 send-keys 之前没有取基线：{cmd}", n + 1));
            assert!(
                !before[base_pos..].contains("while ["),
                "第 {} 次 send-keys 的基线必须紧邻它之前取，中间不能夹一轮等待",
                n + 1
            );
        }
    }

    #[test]
    fn time_budget_ordering_holds() {
        // 三层超时必须**严格套娃**，否则外层会把内层掐死、让被保护的逻辑根本跑不完。
        // ★ 这条测试是被真事逼出来的：E42 之前 `emit_usage_probe_cmd_for_e2e` 给三个场景
        // 统一发 3s 看门狗，旧判据下自然完成约 2s 侥幸躲过；判据一改成"静止 3s"，
        // 正常路径的会话就在轮询跑完前被自己的看门狗杀掉，抓回来一句
        // "no server running"。**当时没有任何东西钉住这个关系。**
        let interval_ms = u64::from(QUIESCENCE_POLL_INTERVAL_MS);
        let poll_budget_ms =
            interval_ms * u64::from(STARTUP_MAX_POLLS) + interval_ms * u64::from(RENDER_MAX_POLLS);
        let exec_ms = EXEC_TIMEOUT_SECS * 1000;
        let watchdog_ms = u64::from(WATCHDOG_TIMEOUT_SECS) * 1000;

        // ① 两段轮询跑满也要装得进 exec 硬超时，且留出余量给 SSH 往返 + 建/清会话。
        //    余量取轮询预算的 1/4——远端链路慢起来不是几十毫秒的事。
        assert!(
            poll_budget_ms + poll_budget_ms / 4 < exec_ms,
            "轮询预算 {poll_budget_ms}ms(+25% 余量) 撑破了 exec 硬超时 {exec_ms}ms：\
远端稍慢就会被 exec 先掐断，探针永远拿不到面板"
        );
        // ② exec 先放弃，看门狗后收尸——反过来会留下无人清理的探针会话。
        assert!(
            exec_ms < watchdog_ms,
            "exec 超时 {exec_ms}ms 必须早于看门狗 {watchdog_ms}ms，否则会话会被提前杀掉"
        );
        // ③ 判定"静止"所需的时长必须显著短于**单段**上限，否则该判据永远无法满足，
        //    每段都会空跑到上限——功能上还对，但每次探测都白等满预算。
        let still_ms = interval_ms * u64::from(QUIESCENCE_STILL_POLLS);
        for (name, cap) in [("startup", STARTUP_MAX_POLLS), ("render", RENDER_MAX_POLLS)] {
            let cap_ms = interval_ms * u64::from(cap);
            assert!(
                still_ms * 2 <= cap_ms,
                "{name} 段上限 {cap_ms}ms 容不下两倍静止时长 {still_ms}ms：判据几乎必然打不中，等于退化成固定 sleep"
            );
        }
    }

    #[test]
    fn poll_interval_string_is_derived_not_double_written() {
        // 反向自检：改 MS 常量，sleep 的字面量必须跟着变（否则就是又一个双写点）。
        assert_eq!(poll_interval_secs(), "0.500");
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        assert!(cmd.contains(&format!("sleep {}", poll_interval_secs())));
    }

    #[test]
    fn probe_cmd_cleans_up_session_after_capture() {
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        let capture_pos = cmd.rfind("capture-pane").expect("须抓屏");
        let final_kill_pos = cmd.rfind("tmux kill-session").expect("须清理");
        assert!(
            final_kill_pos > capture_pos,
            "抓屏之后必须清理会话，不能残留"
        );
    }

    #[test]
    fn probe_cmd_falls_back_to_no_tmux_sentinel() {
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        assert!(cmd.contains("NO_TMUX"), "无 tmux 时须走既有哨兵惯例");
    }

    /// F10 Phase D 审计（后端架构，重要）：此前这条测试是伪验证——只断言 `cmd.contains("send-keys")`，
    /// 跟"payload 有没有被正确转义"毫无关系，含单引号的攻击性 payload 照样能通过。改成两层真验证：
    /// ①`cmd` 里必须**原样**包含 `shell_quote` 对同一输入的产出（证明确实调过同一套转义规则，
    /// 不是自己另写了一套或漏调）；②把转义后的字符串真的丢给 `/bin/sh` 解析，确认 shell 眼里
    /// 看到的就是原始 payload 一字不差（纵深防御——即使①字符串匹配通过，也不能排除转义规则
    /// 本身有漏洞；这一步验证的是"shell 怎么理解它"而不是"我们怎么拼它"）。
    #[test]
    fn probe_payload_and_target_are_shell_quoted() {
        let adversarial_payloads = [
            "export X='a'\"'\"'b'; claude",
            "$(rm -rf /tmp/should-not-run) `whoami`; echo done",
            "line1\nline2\twith\ttabs and 'quotes'",
        ];
        for payload in adversarial_payloads {
            let cmd = build_usage_probe_cmd("z", payload, WATCHDOG_TIMEOUT_SECS).unwrap();
            let payload_q = ssh_source::shell_quote(payload);
            assert!(
                cmd.contains(&payload_q),
                "payload 未按 shell_quote 规则原样嵌入：{payload:?}"
            );
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf '%s' {payload_q}"))
                .output()
                .expect("sh 应该可用（本仓库 e2e 套件同样依赖 sh/bash 标配）");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                payload,
                "shell_quote 未能安全往返（shell 解析出来的内容跟原始 payload 不一致）：{payload:?}"
            );
        }

        // exact-target（`={session}:`）同理：账号名经 `slugify_account_name` 清洗后只剩
        // `[A-Za-z0-9_-]`，这里直接验证 quoting 机制本身对合法 slug 没坏——不是重复验证
        // slugify（那是它自己的测试职责）。
        let cmd = build_usage_probe_cmd("z", "claude", WATCHDOG_TIMEOUT_SECS).unwrap();
        let target_q = ssh_source::shell_quote("=ccm-usage-z:");
        assert!(
            cmd.contains(&target_q),
            "exact-target 未按 shell_quote 规则原样嵌入"
        );
    }

    /// F10 真机验收的**输入源**（同 F04 `emit_guarded_commands_for_e2e` 的既有惯例）：打印真实
    /// `build_usage_probe_cmd` 产出的命令串，供 `e2e/usage-probe-acceptance.sh` 提取、在隔离
    /// tmux socket 上验证真实行为——不手搓等价命令。看门狗超时故意传短值（真机 e2e 要能在合理
    /// 时间内跑完，不代表生产的 30s 可配置）。`#[ignore]`——只由该脚本用
    /// `cargo test --lib -- --ignored --nocapture emit_usage_probe_cmd_for_e2e` 触发。
    #[test]
    #[ignore]
    fn emit_usage_probe_cmd_for_e2e() {
        // 看门狗**按场景分开**：正常路径必须用生产值，短看门狗只给专门测看门狗的那个场景。
        //
        // ★ 2026-07-31 修：原先三个场景**统一用 3s**。那在旧的稳定判据下勉强成立（自然完成
        // 约 2s，刚好赶在自毁前），E42 把判据改成"静止 3s"后自然完成变成 ~7s ⇒ 会话在轮询
        // 跑完前就被自己的看门狗杀掉，正常路径场景整个失去意义（实测：抓回来的是
        // "no server running"）。**根子上就不该让正常路径的看门狗短于自然完成时间**
        // ——那让一个本该测编排的场景变成在测竞态。
        const E2E_SHORT_WATCHDOG_SECS: u32 = 3;
        for (name, payload, watchdog) in [
            (
                "normal",
                "unset CLAUDECODE; FAKECLAUDE",
                WATCHDOG_TIMEOUT_SECS,
            ),
            (
                "collision",
                "unset CLAUDECODE; FAKECLAUDE",
                WATCHDOG_TIMEOUT_SECS,
            ),
            // 慢速 stand-in：收到 /usage 后先停几秒再吐面板，复现 E42 的真实失败形态。
            (
                "slow",
                "unset CLAUDECODE; SLOWCLAUDE",
                WATCHDOG_TIMEOUT_SECS,
            ),
            (
                "watchdog",
                "unset CLAUDECODE; FAKECLAUDE",
                E2E_SHORT_WATCHDOG_SECS,
            ),
        ] {
            // 各场景用**不同 slug**（会话名互不相同）——同一 slug 跨场景复用会让上一个场景
            // 遗留的看门狗在下一个场景刚建好同名会话时杀过来，制造纯脚本层面的竞态假象。
            println!(
                "{name}\t{}",
                build_usage_probe_cmd(name, payload, watchdog).unwrap()
            );
        }
    }
}
