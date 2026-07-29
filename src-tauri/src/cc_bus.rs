//! B03：cc-bus 状态的**纯解析层**（无 I/O，可单测）。
//!
//! 为什么单独一层：`~/.cc-bus/` 里的状态文件**实测是脏的**，且脏得比计划预估严重。
//! 2026-07-28 直接读开发机上那份真实数据，结论见
//! `.claude/planned-build/unify-launch/features/B03-dirty-data-samples.md`：
//!   · `spawned.tsv` **15 行里 8 行是坏的（53%）**——目录名与任务文本里含 `\n`，
//!     把一条记录劈成多行。**坏行是多数派**，所以解析器绝不能"发现坏行就整体报错"，
//!     那样 7 条好记录也一起没了。
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
pub struct CcBusAgent {
    pub id: String,
    pub pane: String,
    pub registered_at: String,
}

/// `spawned.tsv` 的一行：id / 工作目录 / spawn 时间 / 初始任务。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CcBusSpawned {
    pub id: String,
    pub dir: String,
    pub spawned_at: String,
    pub task: String,
}

/// 一次读回的完整状态。`skipped` = 两个文件里被跳过的坏行总数。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct CcBusState {
    pub agents: Vec<CcBusAgent>,
    pub spawned: Vec<CcBusSpawned>,
    pub skipped: usize,
}

/// 判一行是否该被跳过：字段数不足、或 id 非法。
/// 空行**不计入 skipped**——文件尾部的空行是正常的，把它算成"无法解析"会让 UI 虚报。
fn row_fields(line: &str, want: usize) -> Option<Vec<&str>> {
    if line.trim().is_empty() {
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
            task: f[3].to_string(),
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
    fn bad_lines_being_majority_still_yields_good_rows() {
        // 盘面实况：15 行里 8 行坏（53%）。坏行是多数派也不能整体失败。
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
}
