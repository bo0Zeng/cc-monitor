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
    // ★★ **锁不跨子进程**〔D 阶段补审 08-12〕。
    //
    // 第一版把 `MutexGuard` 一路握到 `Command::output()` 之后。那形状有两个后果：
    // ① 两次并发 resume 会**排队**，第二次白等第一次的一整个 `bash -lic`；
    // ② 真出现下面那种挂死时，锁**永远不释放** ⇒ 此后**每一次**本机 resume 都堵在这里，
    //    而 UI 上什么都不会响 —— 与 `local_backend` 那条「reap 不许握着锁等」同一个病。
    // ⇒ 读一次就放手；探完再拿一次写回去。竞态下最多多探一次（纯读、幂等），代价远小于排队。
    {
        let g = LOCAL_PROBE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, cached)) = g.as_ref() {
            if at.elapsed() < LOCAL_PROBE_TTL {
                return cached.clone();
            }
        }
    }
    let r = probe_local_ccm_uncached(LOCAL_PROBE_TIMEOUT);
    *LOCAL_PROBE_CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((std::time::Instant::now(), r.clone()));
    r
}

/// 探测的**上限**。
///
/// 本机实测一次 0.12–0.13s（`bash -lic` 那层要跑完用户的交互式 rc）。给到 5s
/// 是留给「rc 里有 nvm/conda 这类慢初始化」的机器，而不是留给「rc 会卡住」的机器 ——
/// 后者要的是**有个头**，不是等得更久。
#[cfg(not(windows))]
const LOCAL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 带超时地跑一次探测。**超时不是错误，是「当没装」** —— 调用方据此降级回旧路。
///
/// # 为什么非有超时不可〔D 阶段补审 08-12〕
///
/// 它跑的是 `bash -lic`，也就是**用户自己的交互式 rc**，内容不在本仓控制之下
/// （联网的 `nvm use`、等 agent 的 `ssh-add`、写错的 `read`……任何一条都能让它永不返回）。
/// 而调用链是 `resume_history_session`（**同步** `#[tauri::command]`）→ `launch_local`
/// ⇒ 没有上限的话，用户点一次「恢复」就是**永远转圈、且不再有任何本机会话拉得起来**。
///
/// `Command::output()` 没有超时形态（std 不提供 `wait_timeout`），所以这里自己拼：
/// stdout 交给一条读线程（不读会在管道写满时把子进程堵死），主线程按截止时间轮询 `try_wait`。
#[cfg(not(windows))]
fn probe_local_ccm_uncached(timeout: std::time::Duration) -> CcmProbeResult {
    probe_with(timeout, CCM_PROBE_CMD)
}

/// [`probe_local_ccm_uncached`] 的内层。**命令是参数只为了让判据能喂一个「一定挂住」的串**；
/// 生产侧唯一的实参是 [`CCM_PROBE_CMD`]（由 `the_only_production_probe_command_is_the_constant` 钉）。
#[cfg(not(windows))]
fn probe_with(timeout: std::time::Duration, cmd: &str) -> CcmProbeResult {
    use std::io::Read;
    let spawned = std::process::Command::new("bash")
        .args(["-lic", cmd])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return parse_probe_output("");
    };
    // ⚠ 读线程必须在**等之前**起：管道缓冲写满时子进程会阻塞在 write 上，
    // 那时再怎么等都等不到它退出 —— 「等它退出再读」是个会自锁的顺序。
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = std::time::Instant::now() + timeout;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Err(_) => break false,
            Ok(None) => {}
        }
        if std::time::Instant::now() >= deadline {
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if timed_out {
        // 杀掉它，读线程随管道关闭而结束 —— 否则那条线程会跟着挂死的子进程一起留下。
        let _ = child.kill();
        let _ = child.wait();
        tracing::debug!("ccm 本机探测超时（{timeout:?}）—— 当作未装，降级回旧路");
    }
    let buf = reader.join().unwrap_or_default();
    if timed_out {
        // 超时那次**不采信半截输出**：`parse_probe_output` 只看首行，
        // 半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 会被读成「装了但没能力」，
        // 那是个比「没装」更难查的假象。
        return parse_probe_output("");
    }
    parse_probe_output(&String::from_utf8_lossy(&buf))
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

    /// ★★ **超时那条路真的会返回**〔D 阶段补审 08-12〕。
    ///
    /// 这条**必须真起一个挂住的子进程**，不能拿构造好的字符串喂 `parse_probe_output` ——
    /// 本件要防的东西恰恰不在解析里，在「等」这一步：没有上限时用户点一次「恢复」
    /// 就是永远转圈，而那个形态**任何纯函数判据都看不见**。
    ///
    /// 用 50ms 的上限去等一个睡 30s 的 `bash`：
    /// ① 必须在远早于 30s 的时间内返回（否则超时根本没生效）；
    /// ② 结论必须是 `installed: false`（超时当没装 ⇒ 调用方降级回旧路，而不是拿半截当真）。
    ///
    /// 射程：够得到「会返回、且返回什么」；**够不到**「子进程真被收干净了」——
    /// 那要看 `/proc`，而本条不去做那件事（`kill` + `wait` 已在生产段，收尸由它负责）。
    /// ★ 生产段喂给 [`probe_with`] 的命令**只有那个常量**。
    ///
    /// 上一条判据为了能测「挂住」把命令做成了参数。那就开了一个口：
    /// 谁都能从生产段传别的串进去，而**行为判据看不见这件事**（传什么它都照跑）。
    /// ⇒ 这条按源码钉：生产段（剥掉测试段）里 `probe_with(` 只许出现一次，且实参是 `CCM_PROBE_CMD`。
    #[test]
    fn the_only_production_probe_command_is_the_constant() {
        let prod = guard_core::production_code(include_str!("ccm_probe.rs"));
        // ⚠ 排掉**定义行**：`fn probe_with(` 自己也含这个串。
        // 「判据被自己要钉的那个名字命中」本会话第五次 —— 它不是偶发，是按名字取样的固有形态。
        // ⚠ 取的是**整行**，不是「从命中处到行尾」——第一版取后者，于是定义行切出来的是
        // `probe_with(timeout: …)`，`fn ` 被切在了前面，滤不掉。
        // 「判据被自己要钉的那个名字命中」本会话第五次，而这次连**补的那道滤网也切错了**。
        let calls: Vec<&str> = prod
            .lines()
            .filter(|l| l.contains("probe_with(") && !l.contains("fn probe_with("))
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "生产段里 `probe_with(` 出现 {} 次 —— 不是恰好一次，那个「命令是参数」的口就管不住了：{calls:?}",
            calls.len()
        );
        assert!(
            calls[0].contains("CCM_PROBE_CMD"),
            "生产段调 `probe_with` 时喂的不是 `CCM_PROBE_CMD` —— \n\
             那个参数只为判据而开（要一个「一定挂住」的串），生产侧不许借它跑别的东西。实得：{}",
            calls[0]
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn a_hanging_shell_does_not_hang_the_resume_button() {
        let t0 = std::time::Instant::now();
        // ⚠ 上限要**跨过 rc 加载**（本机 `bash -lic` 实测 ~130ms）：给 50ms 的话子进程
        // 还没来得及吐字就被杀了，「半截输出」那一格永远构造不出来 —— 第三版夹具卡在这儿。
        // 800ms 足够它吐完首行 + 填充，又远小于下面 5s 那道断言。
        let r = probe_local_ccm_uncached_for_test_sleep(
            std::time::Duration::from_millis(800),
        );
        let took = t0.elapsed();
        assert!(
            took < std::time::Duration::from_secs(5),
            "等了 {took:?} —— 超时没生效。生产上这意味着「恢复」按钮永远转圈，\n\
             而且锁一握不放之后**每一次**本机 resume 都堵在这里（D 阶段补审那两条的合体）"
        );
        assert!(
            !r.installed,
            "超时之后回了 `installed: true` —— 那会拿一段没读完的输出当能力集，\n\
             半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 被读成「装了但没能力」，\n\
             那是个比「没装」更难查的假象"
        );
    }

    /// 上一条的夹具：**先吐半截、再挂住**，其余与生产逐字同一条路径。
    ///
    /// ⚠⚠ 第一版这里是光秃秃的 `sleep 30`（零输出），于是「超时那次不采信半截输出」
    /// 那条分支**判据根本够不到** —— 变异「超时后照样解析」当场**绿**。
    /// 零输出时「采信」与「不采信」的结果恰好相同，那不是它被守住了，是它没被测。
    /// ⇒ 夹具必须真吐出 `name=ccm` 再挂住：那正是生产上最难查的那一格，
    ///   首行成立而 `capabilities=` 还没来 ⇒ 被读成「装了但一个能力都没有」。
    #[cfg(not(windows))]
    fn probe_local_ccm_uncached_for_test_sleep(timeout: std::time::Duration) -> CcmProbeResult {
        // ⚠ 光 `printf` 冲不出来：`bash` 的 stdout 是管道 ⇒ **块缓冲**，
        // 杀掉时那半截还在它自己的缓冲区里，读到的是空 —— 第二版夹具就是在这儿又绿了一次。
        // 补一段够大的填充把缓冲挤过去，首行才真的到得了读端。
        probe_with(
            timeout,
            "printf 'name=ccm\\n'; head -c 200000 /dev/zero | tr '\\0' x; sleep 30",
        )
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
