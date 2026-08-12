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

/// 探测命令本身 —— **本机与远端逐字同一条**。
///
/// P3t-Y2：这就是 §40「本地 = 不走 ssh 的远端」在探测这一跳的落点 ——
/// 同一个命令串，远端包进 ssh，本机直接交给 `bash -lic`。
/// 抽成常量不是为了省字，是为了让「两侧探的是不是同一件事」这个问题**不必靠读两遍确认**
/// （`the_local_and_remote_probe_ask_the_same_question` 钉住它只有一处定义）。
const CCM_PROBE_CMD: &str = "command -v ccm >/dev/null 2>&1 && ccm --ccm-probe || printf 'NO_CCM\\n'";

/// 本机探测结果的缓存。TTL 与前端 `ccm-probe.ts::CCM_PROBE_TTL_MS` 同为 5 分钟 ——
/// 用户装完 ccm 不必重启 app，但也不必每次拉起都付一次 `bash -lic` 的钱。
///
/// ⚠ 与前端那个缓存**不是同一个** —— 那个缓存的是远端 origin 的探测结果（走 IPC），
/// 这个缓存的是本机的（不走 IPC）。同一个 TTL 是刻意的，不是它们共用了什么。
static LOCAL_PROBE_CACHE: std::sync::Mutex<Option<(std::time::Instant, CcmProbeResult)>> =
    std::sync::Mutex::new(None);
const LOCAL_PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// P3t-Y2：**本机**的 ccm 探测 —— 同步、带缓存。
///
/// # 为什么本机这条是同步的，而远端那条是 async
///
/// 调用方是 `history.rs::launch_local`（`resume_history_session` / `new_local_session`
/// 两个 `#[tauri::command]` 的同步调用链）。远端那条是 async 因为 ssh 本来就要 await；
/// 本机没有那一跳，`bash -lic` 直接跑 —— 为了它把整条链改成 async 是纯粹的传染。
///
/// # 为什么必须真跑一次，而不是渲染完让 shell 自己探
///
/// 旧路（`build_local_posix_command`）用的是 shell 里的 `if command -v cc; then …; else …; fi`
/// —— 那种探法只答得了「在不在」，答不了**能力集**。而渲染器要按 caps 决定
/// 「这条修饰说不说得出来」（§35/§37）：把 caps 猜成「全都有」，遇到老 ccm 会渲染出一条
/// 带未知 flag 的命令，而那时 `else` 分支**已经不在了**，没有回落可走 ⇒ 是个 fail-open。
///
/// # 未装不是错误
///
/// 与远端同款：`command -v` 找不到 → `NO_CCM` 哨兵 → `installed: false`，
/// 调用方据此诚实降级回旧路。`bash` 本身跑不起来（罕见）也走这条 —— 探不到就当没装，
/// 绝不让「探测失败」升级成「拉不起来」。
#[cfg(not(windows))]
pub(crate) fn probe_local_ccm() -> CcmProbeResult {
    let mut g = LOCAL_PROBE_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((at, cached)) = g.as_ref() {
        if at.elapsed() < LOCAL_PROBE_TTL {
            return cached.clone();
        }
    }
    let out = std::process::Command::new("bash")
        .args(["-lic", CCM_PROBE_CMD])
        .output();
    let r = match out {
        Ok(o) => parse_probe_output(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => parse_probe_output(""),
    };
    *g = Some((std::time::Instant::now(), r.clone()));
    r
}

/// 探测远端 `ccm` 是否已装 + 能力集。`command -v` 找不到 → 走 `NO_CCM` 哨兵分支，不报错
/// （未装是正常状态之一，不是异常）。
#[tauri::command]
pub async fn probe_ccm_cli(origin: String) -> Result<CcmProbeResult, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, CCM_PROBE_CMD).await?;
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
