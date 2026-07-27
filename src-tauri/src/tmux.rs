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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// 列远端 tmux 会话(通道 B,一次性 exec)。`command -v tmux` 门控:无 tmux → 哨兵 `NO_TMUX`
/// → 返 `None`(前端隐藏 attach 项);有 tmux 但无会话 → `Some(空)`。
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
    Ok(Some(parse_tmux_ls(&out)))
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
/// 本仓必然踩：`pickFreshTmuxName` 刻意造 `cc-<sid8>-2/-3`，终端 `cct` 造 `<dir>_cc-2/-3`。
///
/// **为什么是 `=name:` 而不是 `=name`**（别"简化"掉尾冒号）：`=` 前缀只在 target-**session**
/// 解析路径上被识别。`send-keys`/`capture-pane` 收的是 target-**pane**，`set-option`/`show-options`
/// 走 pane 解析后上溯——这些路径上 `=name` 直接 `can't find pane`、**rc=1 完全失效**（实测）。
/// 尾冒号把串强制成 `session:` 形态（当前 window、活动 pane），`=` 才落在会话名段上被正确识别。
/// `=name:` 是唯一在全部动词上都既通用又精确的形式。矩阵见 `.claude/planned-build/unify-launch/MASTERPLAN.md` §5.3。
///
/// 删掉它会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，而 UI 报告「已重启」。
fn exact_target(target: &str) -> String {
    ssh_source::shell_quote(&format!("={target}:"))
}

/// `capture-pane` 远端命令串（提纯以便单测——D 审计：内联 `format!` 让 3 个 `-t` 位点里 2 个
/// 无测试覆盖，把 `exact_target` 改回裸目标 `cargo test` 依旧全绿）。
fn build_capture_pane_cmd(target: &str) -> String {
    format!(
        "if command -v tmux >/dev/null 2>&1; then tmux capture-pane -p -t {} 2>/dev/null || printf 'NO_PANE\\n'; else printf 'NO_TMUX\\n'; fi",
        exact_target(target)
    )
}

/// `kill-session` 远端命令串（提纯以便单测，理由同上）。**破坏性动作**——正是「杀错会话」
/// 那条生产 bug 的一端，必须被回归测试钉死。
fn build_kill_session_cmd(target: &str) -> String {
    format!(
        "if command -v tmux >/dev/null 2>&1; then tmux kill-session -t {} 2>&1; else printf 'NO_TMUX\\n'; fi",
        exact_target(target)
    )
}

/// F60:抓一个远端 tmux 会话当前窗口/pane 的屏幕文本(**只读快照,非 attach**)。
/// `tmux capture-pane -p -t <target>`(`-p` 打 stdout、`-t` 选会话)。`command -v tmux` 门控
/// (无 → `NO_TMUX`);会话不存在 / 抓屏失败 → `NO_PANE`(`|| printf`)。target 经 `shell_quote`
/// (来自 `list_remote_tmux` 的真实会话名,仍防御转义)。通道 B 一次性 exec,不干扰前台终端。
#[tauri::command]
pub async fn capture_remote_pane(origin: String, target: String) -> Result<String, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let cmd = build_capture_pane_cmd(&target);
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

/// F79(#38)：杀死远端 tmux 会话（`tmux kill-session -t <target>`）。**破坏性操作**——前端二次确认后才调。
/// `target` 经 `shell_quote`（来自 `list_remote_tmux` 的真实会话名，仍防御转义）。杀完 tab 变灰由 #60-A
/// 的 tmux 存活对账兜（本命令不主动 archive，守 §24）。成功无输出；失败（会话不存在等）经 `2>&1` 捕获报错。
#[tauri::command]
pub async fn kill_remote_tmux(origin: String, target: String) -> Result<(), String> {
    // audit-fixes F02(I1)：与 `tmux_send_keys` 对称——只 kill **本工具的 cc-* 会话**。
    // 否则 F79 的 cwd 回退可能命中用户自己 `tmux new -s work` 里跑的 claude，
    // `tmux kill-session -t work` 会端掉该会话的**所有 window/pane**（误杀无关工作）。
    // cc-* 名（含 cwd 回退命中的自建会话）放行；非 cc-* 一律拒，让用户到那个 tmux 里自行处理。
    if !is_ccm_tmux_name(&target) {
        return Err(format!(
            "拒绝 kill：非本工具 tmux 会话名: {target:?}（避免误杀你自己的 tmux 会话——kill-session 会连它的其它 window 一起端掉；请到该 tmux 里自行处理）"
        ));
    }
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let cmd = build_kill_session_cmd(&target);
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
    // kill-session 成功无输出；非空 = stderr 里的失败信息（如 "can't find session"）。
    if !trimmed.is_empty() {
        return Err(format!("tmux kill-session: {trimmed}"));
    }
    Ok(())
}

/// send-keys 远端命令串（提纯以便单测——补 R1「命令构造测缺」）。`enter=true` 时尾附 `Enter` 键
/// （如 `/compact`、`/exit` 这类要回车提交的）；`enter=false` 只发裸键（如 `Escape` 打断当前回合，
/// **不能**带尾回车，否则可能误提交输入框里的队列文本）。target/keys 均经 `shell_quote`。
fn build_send_keys_remote_cmd(target: &str, keys: &str, enter: bool) -> String {
    let tail = if enter { " Enter" } else { "" };
    format!(
        "if command -v tmux >/dev/null 2>&1; then tmux send-keys -t {} {}{} 2>&1; else printf 'NO_TMUX\\n'; fi",
        exact_target(target),
        ssh_source::shell_quote(keys),
        tail,
    )
}

/// A5：向远端 tmux 会话发按键（headless ssh，如换号重启前在旧号上 send `/compact`、或优雅退出的
/// `Escape`/`/exit`）。**只发按键、不杀不建**，走一次性 ssh、**daemon 不参与**（守只读边界）。
/// `keys` 是字面串或 tmux 键名（`/compact` / `/exit` / `Escape`）；`enter`（可选，**默认 true** 向后兼容
/// A5 旧调用）决定是否尾附 `Enter`——优雅退出的 `Escape` 传 `enter=false`。
/// 安全：`target` 限**本工具建的 `cc-*` 会话名**（`is_ccm_tmux_name`），防误发到用户别的 tmux；
/// keys 经 `shell_quote`。成功无输出；失败（会话不存在等）经 `2>&1` 捕获报错。
#[tauri::command]
pub async fn tmux_send_keys(
    origin: String,
    target: String,
    keys: String,
    enter: Option<bool>,
) -> Result<(), String> {
    if !is_ccm_tmux_name(&target) {
        return Err(format!("拒绝 send-keys：非本工具 tmux 会话名: {target:?}"));
    }
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    // 缺省（前端旧调用不传）→ true，与 A5 原行为逐字节等价。
    let cmd = build_send_keys_remote_cmd(&target, &keys, enter.unwrap_or(true));
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
    if !trimmed.is_empty() {
        return Err(format!("tmux send-keys: {trimmed}"));
    }
    Ok(())
}

/// 本工具建的 tmux 会话名判定：`cc-` 前缀 + 只含 `[A-Za-z0-9_-]`（`cc-<sid8>[-N]` 恒满足）。
/// 用于 send-keys 目标白名单——绝不向用户自己的其它 tmux 会话发按键。
fn is_ccm_tmux_name(name: &str) -> bool {
    name.starts_with("cc-")
        && name.len() > 3
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A5：send-keys 目标白名单——只认本工具的 cc-* 会话名，拒用户别的 tmux。
    #[test]
    fn ccm_tmux_name_whitelist() {
        assert!(is_ccm_tmux_name("cc-abc12345"));
        assert!(is_ccm_tmux_name("cc-abc12345-2")); // pickFreshTmuxName 的 -N 变体
        assert!(!is_ccm_tmux_name("cc-")); // 只前缀无体
        assert!(!is_ccm_tmux_name("web")); // 用户自己的会话
        assert!(!is_ccm_tmux_name("mycc-x")); // 非前缀
        assert!(!is_ccm_tmux_name("cc-a b")); // 空格（注入面）
        assert!(!is_ccm_tmux_name("cc-a;rm")); // 分号
        assert!(!is_ccm_tmux_name("cc-a$x")); // 元字符
    }

    /// audit-fixes F02(I1)：kill_remote_tmux 必须与 send-keys 对称,拒非 cc-* 名——
    /// 防 F79 cwd 回退误杀用户自己 `tmux new -s work` 的整个会话。变异锚点:去掉 guard → 此测红
    /// （非 cc- 名会往下走到 SSH，不再早退 Err「拒绝」）。cc-* 名不在此拦(会继续到 SSH,不在此验)。
    #[tokio::test]
    async fn kill_remote_tmux_rejects_non_ccm_name() {
        for bad in ["work", "web", "0", "my-session", "cc-a b"] {
            let r = kill_remote_tmux("aya".to_string(), bad.to_string()).await;
            assert!(r.is_err(), "非 cc-* 名 {bad:?} 应被拒");
            assert!(
                r.unwrap_err().contains("拒绝"),
                "{bad:?} 应是白名单拒绝(而非 SSH/配置错)"
            );
        }
    }

    /// A5+：send-keys 命令构造（补 R1）——enter=true 尾附 ` Enter`，false 不附；target/keys 经 shell_quote。
    /// F01：target 形态为 `'=<名>:'`（精确匹配，见 `exact_target`）。
    #[test]
    fn send_keys_cmd_construction() {
        let with_enter = build_send_keys_remote_cmd("cc-abc12345", "/compact", true);
        assert!(
            with_enter.contains("tmux send-keys -t '=cc-abc12345:' '/compact' Enter 2>&1"),
            "enter=true 应尾附 Enter: {with_enter}"
        );
        let no_enter = build_send_keys_remote_cmd("cc-abc12345", "Escape", false);
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
    /// `kill-session -t 'si*'` glob 命中。本仓必然踩（`pickFreshTmuxName` 造 `cc-<sid8>-2/-3`、
    /// 终端 `cct` 造 `<dir>_cc-2/-3`）。
    ///
    /// 删掉 `exact_target` 会让换号重启把 `/exit` 敲进**兄弟会话里还活着的 claude** 并 kill 它，
    /// 而 UI 报告「已重启」。**尾冒号不能省**：`send-keys`/`capture-pane` 收 target-pane，
    /// `=名`（无冒号）在那条路径上 rc=1 完全失效。
    #[test]
    fn tmux_targets_use_exact_match() {
        // **三个命令构造点全钉死**（D 审计：此前只钉了 send-keys，另两处改回裸目标测试仍全绿）。
        let sk = build_send_keys_remote_cmd("cc-abc12345", "/exit", true);
        let cap = build_capture_pane_cmd("cc-abc12345");
        let kill = build_kill_session_cmd("cc-abc12345");
        for (label, cmd) in [("send-keys", &sk), ("capture-pane", &cap), ("kill-session", &kill)] {
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
        assert_eq!(exact_target("cc-x"), "'=cc-x:'");
        assert_eq!(exact_target("proj_cc-2"), "'=proj_cc-2:'");
        // glob 名即便漏进来也被引号原样包住（不脱出成 shell glob）；名字层另有
        // `isValidTmuxName` 禁 `*`/`?` 作第二道防线。
        assert_eq!(exact_target("si*"), "'=si*:'");
        // 含单引号的名字仍被正确转义（shell_quote 的 '\'' 形态）。
        assert!(exact_target("a'b").starts_with("'=a"));
        assert!(exact_target("a'b").ends_with("b:'"));
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
        // (`remote-daemon-proto/src/watcher.rs`) 分属两个独立 crate、不能共享 const，但两侧
        // `tmux ls -F` 格式串**必须逐字一致**（否则 daemon 推的列 monitor 解错位）。编译期
        // include_str! 读 daemon 源，把本 const 的真 TAB 折回源码里的 `\t` 转义再断言 daemon 源
        // 含该带引号字面量——**双向**：改 monitor 或 daemon 任一侧忘同步，本测即红。
        let daemon_src = include_str!("../../remote-daemon-proto/src/watcher.rs");
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
}
