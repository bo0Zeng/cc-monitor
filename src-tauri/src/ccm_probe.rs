//! F03（unify-launch）：探测远端是否已装 `ccm`（F02 统一启动 CLI）及其能力集，供前端
//! `src/ccm-probe.ts` 决定走 CLI 渲染器还是兜底渲染器。一次性 headless SSH exec，照
//! `tmux.rs::capture_remote_pane` 的范式（通道 B，不干扰前台终端、不涉及 daemon）。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct CcmProbeResult {
    pub installed: bool,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
}

/// 解析 `ccm --ccm-probe` 的输出。首行非字面 `name=ccm` → 判定未装/不兼容——防止 PATH 里
/// 已有同名但无关的自定义 `ccm`（用户自己的脚本）被误判为本工具的 CLI。
fn parse_probe_output(out: &str) -> CcmProbeResult {
    let mut lines = out.lines();
    if lines.next() != Some("name=ccm") {
        return CcmProbeResult {
            installed: false,
            version: None,
            capabilities: vec![],
        };
    }
    let (mut version, mut capabilities) = (None, vec![]);
    for line in lines {
        if let Some(v) = line.strip_prefix("version=") {
            version = Some(v.to_string());
        } else if let Some(c) = line.strip_prefix("capabilities=") {
            capabilities = c
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    CcmProbeResult {
        installed: true,
        version,
        capabilities,
    }
}

/// 探测远端 `ccm` 是否已装 + 能力集。`command -v` 找不到 → 走 `NO_CCM` 哨兵分支，不报错
/// （未装是正常状态之一，不是异常）。
#[tauri::command]
pub async fn probe_ccm_cli(origin: String) -> Result<CcmProbeResult, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let cmd = "command -v ccm >/dev/null 2>&1 && ccm --ccm-probe || printf 'NO_CCM\\n'";
    let stream = ssh_source::connect_and_exec_cmd(&cfg, cmd).await?;
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("探测 ccm 失败: {e}"))?;
    Ok(parse_probe_output(&String::from_utf8_lossy(&buf)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_probe_output() {
        let out = "name=ccm\nversion=1\nself=/x/ccm\ncapabilities=new,resume,attach,tmux,account,cwd,agent,launcher,ccm-sid,print\nagents=claude,codex\n";
        let r = parse_probe_output(out);
        assert!(r.installed);
        assert_eq!(r.version.as_deref(), Some("1"));
        assert!(r.capabilities.contains(&"tmux".to_string()));
        assert!(r.capabilities.contains(&"ccm-sid".to_string()));
    }

    #[test]
    fn no_ccm_sentinel_not_installed() {
        let r = parse_probe_output("NO_CCM\n");
        assert!(!r.installed);
        assert_eq!(r.capabilities.len(), 0);
    }

    #[test]
    fn unrelated_same_name_binary_not_installed() {
        // PATH 上恰好有个不相关的同名 `ccm` 脚本——首行不是 "name=ccm" 就必须判定未装，
        // 不能因为 rc=0 就当已装（防止把用户自己的脚本误当成本工具的 CLI）。
        let r = parse_probe_output("some unrelated program output\n");
        assert!(!r.installed);
    }

    #[test]
    fn empty_output_not_installed() {
        let r = parse_probe_output("");
        assert!(!r.installed);
    }
}
