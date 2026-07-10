//! F51 tmux 反查(控制类命令走通道 B = russh exec,不干扰前台 PowerShell 终端)。
//!
//! tab 右键菜单打开时按需查询远端 tmux 会话列表,前端按 `pane_current_path==cwd +
//! pane_current_command==claude` 反查该 tab 的 Claude 正跑在哪个 tmux 会话,命中则一键
//! `ssh -t … tmux attach -t <名>`(交互走通道 A = PowerShell)。
//!
//! **最隐蔽的重写坑(调研 03 档 §3.1)**:`tmux ls -F` 的格式串**不解释**字面 `\t`——给什么
//! 字节原样输出。所以分隔符必须是**真 TAB 字节(0x09)**。Rust 里 `"\t"` 是真 TAB(勿写
//! `\\t`),`parse_tmux_ls` 按真 TAB `split`。F52/F58 的 kill/rename/capture 续挂本模块。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

/// `tmux ls -F` 的格式串。字段以**真 TAB**分隔(见模块注释):
/// name ⇥ pane_current_path ⇥ pane_current_command ⇥ attached(1/0) ⇥ windows。
const TMUX_LS_FMT: &str = "#{session_name}\t#{pane_current_path}\t#{pane_current_command}\t#{?session_attached,1,0}\t#{session_windows}";

/// 一个远端 tmux 会话(反查 + 未来管理用)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxSession {
    pub name: String,
    pub path: String,
    pub command: String,
    pub attached: bool,
    pub windows: u32,
}

/// 解析 `tmux ls -F '<TMUX_LS_FMT>'` 输出(真 TAB 分列)。字段数不符 / name 空的行跳过
/// (半截行、非法行不进结果);windows 非数字回退 0。
pub fn parse_tmux_ls(output: &str) -> Vec<TmuxSession> {
    output
        .lines()
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 5 || f[0].is_empty() {
                return None;
            }
            Some(TmuxSession {
                name: f[0].to_string(),
                path: f[1].to_string(),
                command: f[2].to_string(),
                attached: f[3] == "1",
                windows: f[4].parse().unwrap_or(0),
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
    let mut out = String::new();
    reader
        .read_to_string(&mut out)
        .await
        .map_err(|e| format!("读 tmux 列表失败: {e}"))?;
    if out.trim() == "NO_TMUX" {
        return Ok(None);
    }
    Ok(Some(parse_tmux_ls(&out)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multi_session() {
        // 真 TAB 分隔(Rust "\t" = 0x09)。
        let out = "cc-abc12345\t/home/pi/proj\tclaude\t1\t2\nweb\t/srv/web\tzsh\t0\t1\n";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].name, "cc-abc12345");
        assert_eq!(s[0].path, "/home/pi/proj");
        assert_eq!(s[0].command, "claude");
        assert!(s[0].attached);
        assert_eq!(s[0].windows, 2);
        assert!(!s[1].attached);
        assert_eq!(s[1].command, "zsh");
    }

    #[test]
    fn parse_skips_malformed_and_handles_edges() {
        // 空输出 → 空。
        assert!(parse_tmux_ls("").is_empty());
        assert!(parse_tmux_ls("\n\n").is_empty());
        // 字段数不符(无 TAB / 少字段)→ 跳过;name 空 → 跳过。
        let out = "no tabs here\nn\t/p\tsh\t0\n\t/p\tclaude\t1\t1\ngood\t/home/a b\tclaude\t1\t3";
        let s = parse_tmux_ls(out);
        assert_eq!(s.len(), 1, "只有最后一行合法");
        assert_eq!(s[0].name, "good");
        // 路径含空格(非 TAB)保留。
        assert_eq!(s[0].path, "/home/a b");
        assert_eq!(s[0].windows, 3);
    }

    #[test]
    fn parse_windows_nonnumeric_falls_back_zero() {
        let s = parse_tmux_ls("n\t/p\tclaude\t1\tNaN");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].windows, 0);
    }

    #[test]
    fn fmt_uses_real_tab_not_literal_backslash_t() {
        // 回归调研 03 §3.1 坑:格式串里必须是真 TAB 字节,不能是字面 \t。
        assert!(TMUX_LS_FMT.contains('\t'), "格式串须含真 TAB");
        assert!(!TMUX_LS_FMT.contains("\\t"), "格式串不得含字面反斜杠-t");
    }
}
