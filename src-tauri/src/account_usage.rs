//! F10（unify-launch，剩余账号 UX）：每账号 Claude 订阅计划用量窗口百分比（"plan 窗口%"）——
//! 不是 context window 用量（那是 `usage-hud.ts` 的事），不是本地 token 累计（那是
//! `usage.rs`/`views/usage-view.ts` 的事），是 Anthropic 服务端权威的 5h/周额度窗口剩余%，
//! 必须真的起一个已登录的 claude 会话跑 `/usage` 斜杠命令、capture-pane 抓屏解析。
//!
//! **本模块只负责编排一次性探针会话本身**（建/等/送键/抓屏/清理），完全不理解 `/usage`
//! 输出的语义——那是 TS 侧 `src/account-usage-parse.ts` 纯函数的职责。`launch_payload`
//! （`export CLAUDE_CONFIG_DIR=...; unset <嵌套env>; claude` 这一行）由 TS 侧
//! `buildUsageProbePayload` 构造好传入，本模块只管把它安全地敲进一个隐藏 tmux 会话。
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
/// 画面稳定轮询：每次间隔（秒）+ 最大轮询次数——用"连续两次抓屏内容一致"代替固定 sleep 猜测
/// 冷启动/网络查询耗时（真机耗时未知，格式无关、版本无关，见 F10 计划 §1 设计说明）。
const QUIESCENCE_POLL_INTERVAL_SECS: &str = "0.5";
const QUIESCENCE_MAX_POLLS: u32 = 14; // ≈7s 上限
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

    // 稳定轮询：抓两次屏，内容一致且非空就认为"画面稳定"，提前结束；否则轮询到上限。
    let quiescence_wait = format!(
        "prev=''; i=0; while [ $i -lt {QUIESCENCE_MAX_POLLS} ]; do \
sleep {QUIESCENCE_POLL_INTERVAL_SECS}; \
cur=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
if [ -n \"$cur\" ] && [ \"$cur\" = \"$prev\" ]; then break; fi; \
prev=\"$cur\"; i=$((i+1)); \
done"
    );

    Ok(format!(
        "if command -v tmux >/dev/null 2>&1; then \
tmux kill-session -t {t} >/dev/null 2>&1 || true; \
tmux new-session -d -s {session_q} -x {PROBE_COLS} -y {PROBE_ROWS}; \
setsid sh -c 'sleep {watchdog_timeout_secs}; tmux kill-session -t {t} >/dev/null 2>&1' </dev/null >/dev/null 2>&1 & \
tmux send-keys -t {t} {payload_q} Enter; \
{quiescence_wait}; \
tmux send-keys -t {t} '/usage' Enter; \
{quiescence_wait}; \
out=\"$(tmux capture-pane -p -t {t} 2>/dev/null || true)\"; \
tmux kill-session -t {t} >/dev/null 2>&1 || true; \
printf '%s' \"$out\"; \
else printf 'NO_TMUX\\n'; fi",
        session_q = ssh_source::shell_quote(&session),
    ))
}

/// F10：per-account 探测 Claude 订阅计划用量窗口%（"plan 窗口%"）。通道 B（一次性 headless
/// exec，不占用前台可见终端，同 `list_remote_tmux`/`capture_remote_pane` 既有分工）。
///
/// `account_name` 只用于探针会话名 slug + 错误文案，不参与鉴权（鉴权/账号存在性由 TS 侧调用
/// 前已经确认过，`launch_payload` 本身已经带着正确的 `CLAUDE_CONFIG_DIR`）。
#[tauri::command]
pub async fn account_usage(
    origin: String,
    account_name: String,
    launch_payload: String,
) -> Result<AccountUsageProbeResult, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let slug = slugify_account_name(&account_name);
    // 构造失败（Gate 1 拒绝）→ 诚实回报，不发起任何 SSH 连接。`slugify_account_name` 已保证
    // slug 恒非空，这里恒不触发；留着是因为闸门该由 `exact_target` 说了算，不是由"我确信调用方
    // 不会传空"说了算。
    let cmd = match build_usage_probe_cmd(&slug, &launch_payload, WATCHDOG_TIMEOUT_SECS) {
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
        const E2E_WATCHDOG_SECS: u32 = 3;
        // 三个场景各用**不同的 slug**（会话名互不相同）——同一 slug 跨场景复用会让上一个场景
        // 遗留的看门狗（3s 后才触发，独立于主流程是否已自然完成）在下一个场景刚建好同名会话
        // 时杀过来，制造纯测试脚本层面的竞态假象，跟被测代码本身无关。
        println!(
            "normal\t{}",
            build_usage_probe_cmd("z", "unset CLAUDECODE; FAKECLAUDE", E2E_WATCHDOG_SECS).unwrap()
        );
        println!(
            "collision\t{}",
            build_usage_probe_cmd(
                "collision",
                "unset CLAUDECODE; FAKECLAUDE",
                E2E_WATCHDOG_SECS
            )
            .unwrap()
        );
        println!(
            "watchdog\t{}",
            build_usage_probe_cmd(
                "watchdog",
                "unset CLAUDECODE; FAKECLAUDE",
                E2E_WATCHDOG_SECS
            )
            .unwrap()
        );
    }
}
