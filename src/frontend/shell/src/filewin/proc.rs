//! 进程形态：一个窗口一个进程（`§8.5`）。
//!
//! 窗口关掉就销毁，而 winit 一个进程只许一个事件循环（eframe 把建好的那个缓存在线程局部里）⇒ 同一个进程里第二次 `run_native` 必然失败。
//! 「关掉就销毁」与「关掉之后还能再打开」在同进程形态下只能二选一 ⇒ 每开一个窗口，起一个独立进程。
//! 顺带：进程退出 = 内核收走那一窗的内存（不必「常驻隐藏」）。窗口进程零 SFTP、零 SSH（SFTP 住本机常驻后端），所以多进程不多连接。
//!
//! # 形状：种子走 stdin，argv 上一个字都不带
//!
//! ```text
//! open_file_window（monitor 进程）
//! ├─ ① resolve_window_bin()      ← 环境变量 CCM_FILEWIN_BIN，或 exe 旁那份，或 monitor 自带那份（放到 ~/.cc-monitor/bin）
//! ├─ ② 起进程的唯一出口          ← `crate::spawn_managed` 那个五参数形态
//! │                                  （Hidden · Detached · Inherit，逐格理由住 [`spawn_window`]）
//! ├─ ③ 把种子写进它的 stdin 再关掉    ← 源 + cwd（缺 = 问那台 home）+ reveal + 交接件
//! ├─ ④ 读它 stdout 上的一行      ← 子进程拨回通道、问 home、列第一屏之后说「列到 N 行」或「列不出来：原话」
//! │                                  （列不出来 ⇒ 它不开窗就退，父进程带着原话回错）
//! ├─ ⑤ 等一个很短的预算，看它是不是当场就退了（[`early_failure`]，同一条轮询）
//! └─ ⑥ 把句柄交给一条收尸线程        ← Detached 不改父子关系 ⇒ 不 wait 就留僵尸；stdout 由它读到 EOF
//! ```
//!
//! 第一屏在窗口进程里列：它拨回通道之后自己问（[`first_screen`]，与之后每一次列目录同一条 `source::ask`），再在 stdout 上说一行（[`Ready`]）。
//!
//! 种子走 stdin，不走环境变量或 argv：① 环境变量装不下（一条 128 KiB，一屏 [`super::source::LS_LIMIT`] 条的 JSON 是兆字节级 ⇒ `E2BIG`）；
//! ② argv 是世界可读的（`/proc/<pid>/cmdline`），而种子里有主机名 · 用户名 · 私钥路径；③ stdin 关掉就是 EOF。
//! 写种子是阻塞写，子进程第一件事就是把 stdin 读到 EOF（[`child_main`]）⇒ 正常路径上不堵；子进程当场死掉时拿到 `EPIPE`，照原样报。
//!
//! # 买不到的
//!
//! - 订阅（生产上零条流，`chan/host.rs` 头注）· 断线重拨（`chan/client.rs` 头注：断了就断了，之后每一件都会出声说没走通）。
//!   拨回 monitor 的通道口拨不通就不开窗（`D11`：没有退路）。
//! - 「窗口真的出现在屏幕上」在本机量不到（`XDG_SESSION_TYPE=tty`）：这一侧买得到的是「那个进程起来了、而且没有当场退」，
//!   屏幕上那一维由 Xvfb 台架那一摞买。真窗口管理器（reparent 之后几何不同）· 真 Windows · release 档（`panic = "abort"`）都没读数。
//! - 发版包里有这个二进制：tauri bundler 把每个 `[[bin]]` 装到主程序旁（两向相等判据住 `K-R124-ruler.py` ⑭）；单文件的 monitor 内嵌它的字节
//!   （`build.rs::embed_native_filewin`），旁边没有时放到 `~/.cc-monitor/bin/` 再起。

use crate::copy_table::copy_text;
use std::path::{Path, PathBuf};

// 种子 · 就绪那一行 · 指到窗口二进制的环境变量：monitor 与窗口进程两边对上的形状住 `filewin-contract`。
pub use filewin_contract::{
    decode_ready, decode_request, encode_ready, encode_request, OpenRequest, Ready, BIN_ENV,
};

/// 窗口进程那份二进制的**文件名主干**。
///
/// ⚠ 与本包主二进制同住一个目录（`cargo` 的 `target/<档>/` 或安装包的 exe 旁）。
pub const BIN_STEM: &str = "cc-monitor-filewin";

/// 窗口那份二进制在 `dir` 里的落点。**纯函数**（判据要在临时目录上喂它）。
pub fn window_bin_in(dir: &Path) -> PathBuf {
    dir.join(window_bin_name())
}

/// 窗口那份二进制的文件名（exe 旁边与 `~/.cc-monitor/bin` 里是同一个名字）。
pub fn window_bin_name() -> String {
    format!("{BIN_STEM}{}", crate::platform::proc::EXE_SUFFIX)
}

/// 那份二进制到底在哪 —— 环境变量 → exe 旁边 → monitor 自带那份（放到 `~/.cc-monitor/bin/` 再用）。三处之外没有别的路，也没有回落：
/// 找不到 / 放不下来都是一条响亮的失败，说清看过哪几处、或往哪儿写失败了。
///
/// - 环境变量 [`BIN_ENV`]：开发时指一份别处的。
/// - exe 旁边：安装包把它装在主程序旁；开发树 `target/<档>/` 里两个也挨着。名字里永远不带 target triple（与 `local_backend::local_backend_candidates` 不同）。
/// - monitor 自带那份（`byte_table::native_filewin`，发版时内嵌）：落点、上位、Windows 上旧的那份正在跑时怎么换，都与本机后端同一套
///   （`local_backend::place_local_program`）；盘上那份逐字节相等才直接用，不同就换成这一版 monitor 带的那份（种子与就绪那一行是编进两侧的契约）。
///
/// # Errors
///
/// 三处都给不出一个存在的文件；或自带的那份放不下来。
pub fn resolve_window_bin() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| copy_text("rsFilewinProc.selfPath.failed", &[("e", &e.to_string())]))?;
    resolve_window_bin_in(
        std::env::var_os(BIN_ENV).map(PathBuf::from),
        exe.parent().unwrap_or(Path::new(".")),
        crate::byte_table::native_filewin(),
        landing_dir().as_deref(),
        &crate::platform::fs::make_executable,
        &crate::platform::fs::ensure_private_dir,
    )
}

/// 自带那份放到哪：本机后端落点所在的那个目录（`~/.cc-monitor/bin`）。家目录问不到 ⇒ `None`（不发明一个目录）。
fn landing_dir() -> Option<PathBuf> {
    let rel = crate::profile_installer::ccm_bin_dir_rel()?;
    creds_core::store::home_dir().map(|h| rel.split('/').fold(h, |p, seg| p.join(seg)))
}

/// [`resolve_window_bin`] 的注入形：环境变量的值 · exe 所在目录 · 自带的字节 · 落点目录 · 两样宿主知识都由调用方给
/// （判据在临时目录上喂它，不碰真 `~/.cc-monitor`）。
///
/// # Errors
///
/// 同 [`resolve_window_bin`]。
pub fn resolve_window_bin_in(
    env: Option<PathBuf>,
    exe_dir: &Path,
    embedded: Option<&[u8]>,
    landing: Option<&Path>,
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
    ensure_dir: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let mut looked: Vec<PathBuf> = Vec::new();
    if let Some(p) = env {
        if p.is_file() {
            return Ok(p);
        }
        looked.push(p);
    }
    let beside = window_bin_in(exe_dir);
    if beside.is_file() {
        return Ok(beside);
    }
    looked.push(beside);
    let Some(bytes) = embedded else {
        return Err(copy_text(
            "rsFilewinProc.bin.notFound",
            &[
                ("binStem", &BIN_STEM.to_string()),
                ("looked", &format!("{:?}", looked)),
                ("binEnv", &BIN_ENV.to_string()),
            ],
        ));
    };
    let Some(dir) = landing else {
        return Err(copy_text("rsFilewinProc.bin.noHome", &[]));
    };
    crate::local_backend::place_local_program(
        dir,
        &window_bin_name(),
        bytes,
        make_executable,
        ensure_dir,
    )
    .map_err(|e| {
        copy_text(
            "rsFilewinProc.bin.placeFailed",
            &[("dir", &dir.display().to_string()), ("e", &e)],
        )
    })
}

/// 起一个窗口进程并把种子交给它。回**那个句柄**（还没做早失败判断）。
///
/// 🔴 **起进程走的是全仓那个唯一出口**（`crate::spawn_managed`），三条策略逐格的理由：
///
/// - `ConsolePolicy::Hidden` —— Windows 上不给它开控制台黑框。monitor 是
///   `windows_subsystem = "windows"` 的 GUI app，不带这个 flag 时系统会**替子进程
///   新开一个控制台窗口**，而那个框是**可关的**：用户一关，`CTRL_CLOSE_EVENT`
///   打到窗口进程 ⇒ 文件窗口被杀。egui 那个窗口自己会出来，它不需要控制台。
/// - `Lifetime::Detached` —— **绝不能是 `JobKillOnClose`**。理由与
///   `lib.rs::open_with_os` 那一处逐字同形：这条命令一返回，句柄就丢了
///   ⇒ Job 句柄一关，刚开出来的窗口当场被收掉。
///   ⚠ 顺带一格**刻意的**：用户关掉 monitor 时，已经开着的文件窗口**不跟着走** ——
///   那正是「像系统自己的文件管理器一样」那句裁决的样子。
/// - `StderrSink::Captured` —— 它的 stderr 上是「窗口为什么没立起来」那几句（例如缺 OpenGL 2.0）：
///   双击起的 monitor 没有 stderr 可跟，跟过去就等于丢了 ⇒ 接出来，逐行记进 monitor 的日志，
///   最后几行留给开窗没成 / 开了又退那一句话（[`StderrTail`]）。
///
/// # Errors
///
/// 二进制找不到 · `spawn` 失败 · 种子写不进去（含子进程当场死掉那一形的 `EPIPE`）。
pub fn spawn_window(
    req: &OpenRequest,
) -> Result<(crate::spawn_managed::ManagedChild, StderrTail), String> {
    use crate::spawn_managed::{ConsolePolicy, Lifetime, StderrSink};
    let bin = resolve_window_bin()?;
    let seed = encode_request(req)?;
    // argv 上一个字都没有（理由住头注）；接 stdin（种子）与 stdout（就绪那一行）两根。stdout 一直有人读：
    // [`open_in_new_process`] 读那一行，之后 [`reap_later`] 把它读到 EOF（没人读的管子写满就会卡住窗口）。
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    let mut child = crate::spawn_managed::spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Captured,
    )
    .map_err(|e| {
        copy_text(
            "rsFilewinProc.spawn.failed",
            &[("bin", &(bin.display()).to_string()), ("e", &e.to_string())],
        )
    })?;
    let tail = StderrTail::drain(child.stderr.take(), child.id());
    write_seed(&mut child, &seed)?;
    Ok((child, tail))
}

/// 窗口进程 stderr 留几行给那一句话。
const STDERR_TAIL_LINES: usize = 4;

/// 窗口进程的 stderr：一条线程读到 EOF，逐行记进 monitor 的日志、留最后几行。
pub struct StderrTail {
    reader: Option<std::thread::JoinHandle<Vec<String>>>,
}

impl StderrTail {
    fn drain(err: Option<std::process::ChildStderr>, pid: u32) -> Self {
        let reader = err.map(|err| {
            std::thread::spawn(move || {
                use std::io::{BufRead, Read};
                let mut r = std::io::BufReader::new(err);
                let mut tail: std::collections::VecDeque<String> = Default::default();
                let mut buf = Vec::new();
                loop {
                    buf.clear();
                    match (&mut r)
                        .take(crate::local_backend::STDERR_MAX_LINE_BYTES)
                        .read_until(b'\n', &mut buf)
                    {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    tracing::warn!("文件窗口进程（pid {pid}）stderr：{line}");
                    if tail.len() == STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line);
                }
                tail.into_iter().collect()
            })
        });
        StderrTail { reader }
    }

    /// 进程已经退了之后调：等那条线程读到 EOF，交最后几行（没接上 / 读线程没了 ⇒ 空）。
    fn finish(self) -> Vec<String> {
        self.reader.and_then(|h| h.join().ok()).unwrap_or_default()
    }
}

/// 开窗没成 / 开了又退那一句：`why`（退出码那一句）＋ 它 stderr 最后几行说的原因。
/// 认得出的原因说人话（缺 OpenGL 2.0）；认不出照原话带最后几行；一行都没有 ⇒ 说它没留下原因。
pub fn exit_said(why: &str, tail: &[String]) -> String {
    let all = tail.join("\n");
    let lower = all.to_ascii_lowercase();
    if lower.contains("opengl") && lower.contains("2.0") {
        return copy_text("rsFilewinProc.cause.noOpenGl", &[("why", why)]);
    }
    if tail.is_empty() {
        copy_text("rsFilewinProc.open.noStderr", &[("why", why)])
    } else {
        copy_text(
            "rsFilewinProc.open.stderrTail",
            &[("why", why), ("tail", &all)],
        )
    }
}

/// 把种子写进那条 stdin 并关掉它（关掉 = EOF = 「给完了」）。抽成具名函数让 `EPIPE` 那一形判得到：「种子送不进去」不是「窗口画不出来」。
fn write_seed(child: &mut crate::spawn_managed::ManagedChild, seed: &str) -> Result<(), String> {
    use std::io::Write;
    let mut pipe = child
        .stdin
        .take()
        .ok_or_else(|| copy_text("rsFilewinProc.spawn.noStdin", &[]))?;
    pipe.write_all(seed.as_bytes())
        .and_then(|()| pipe.flush())
        .map_err(|e| copy_text("rsFilewinProc.seed.sendFailed", &[("e", &e.to_string())]))
}

/// 开窗没成的两种：前一种是窗口进程列不出来时说的原话，入口原样交出去；后一种是进程这一层的事（起不来 · 当场退了 · 一句话都没说），
/// 入口给它套上「文件窗口没起来」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unopened {
    /// 窗口进程说的「列不出来」那一行（home 问不到 / 目录列不出来 / 拨不回通道 / 种子读不动）。
    Said(String),
    /// 进程这一层没成。
    Process(String),
}

/// 开一个窗口 —— 生产那条路的入口。回 `(那个进程的 pid, 第一屏列到的行数)`。没有退路：起不了独立进程就是错，照实报。
///
/// 先读就绪那一行：列不出来 ⇒ 窗口进程不开窗就退，这里带着那句原话回 [`Unopened::Said`]；一句话都没说就 EOF ⇒ 进程这一层的错。
/// 这一读是阻塞的，上界由窗口进程自己那几段期限定（拨回 [`DIAL_BUDGET`] ＋ 两问各 [`FIRST_SCREEN_BUDGET`]）⇒ 入口把整件放在 `spawn_blocking` 上。
///
/// 早失败那一跳走 [`early_failure`] 同一条轮询（登记在 `rust_timer_registry` 里）：起不来那一形在说完就绪之后毫秒级就退，起来了那一形要占着
/// 进程直到用户关窗 ⇒ 预算内必然还在跑。买不到「窗口真的出现在屏幕上」与「起了循环之后才炸」的慢失败。
///
/// # Errors
///
/// [`spawn_window`] 的任何一档 · 窗口进程说列不出来 · 一句话没说就退了 · 说了就绪却在开窗预算内就退了。
/// 判成功之后窗口进程不体面地退了（退出码非零 / 被信号杀）⇒ 收尸线程把这句话交给 `late`、由调用方出声。
pub type LateExit = Box<dyn FnOnce(String) + Send + 'static>;

pub fn open_in_new_process(req: &OpenRequest, late: LateExit) -> Result<(u32, usize), Unopened> {
    let (mut child, tail) = spawn_window(req).map_err(Unopened::Process)?;
    let pid = child.id();
    let Some(out) = child.stdout.take() else {
        if let Err(e) = child.kill() {
            tracing::warn!(
                "收不掉那个窗口进程（{e}）—— 它说的话对不上约定，可能还会开出一个没人认的窗口"
            );
        }
        reap_later(child, None, tail, None);
        return Err(Unopened::Process(copy_text(
            "rsFilewinProc.spawn.noStdout",
            &[],
        )));
    };
    let mut out = std::io::BufReader::new(out);
    let n = match read_ready(&mut out) {
        Ok(Some(Ready::Listed(n))) => n,
        Ok(Some(Ready::Failed(said))) => {
            reap_later(child, Some(out), tail, None);
            return Err(Unopened::Said(said));
        }
        Ok(None) => {
            let st = child.wait_for_status();
            let why = match st {
                Ok(st) => copy_text("rsFilewinProc.open.exited", &[("st", &st.to_string())]),
                Err(e) => copy_text("rsFilewinProc.open.failedWhy", &[("e", &e.to_string())]),
            };
            return Err(Unopened::Process(exit_said(&why, &tail.finish())));
        }
        Err(garbled) => {
            // 说了一句不是约定形状的话 ⇒ 两端契约漂了；它接下来会不会开窗说不准 ⇒ 收掉它，不留一个没人认的窗口。
            if let Err(e) = child.kill() {
                tracing::warn!(
                    "收不掉那个窗口进程（{e}）—— 它说的话对不上约定，可能还会开出一个没人认的窗口"
                );
            }
            reap_later(child, Some(out), tail, None);
            return Err(Unopened::Process(garbled));
        }
    };
    if early_failure(
        || matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
        EARLY_FAILURE_BUDGET,
    ) {
        let why = match child.try_wait() {
            Ok(Some(st)) => copy_text("rsFilewinProc.open.exited", &[("st", &st.to_string())]),
            Ok(None) => copy_text("rsFilewinProc.open.failed", &[]),
            Err(e) => copy_text("rsFilewinProc.open.failedWhy", &[("e", &e.to_string())]),
        };
        // 已经退了（预算内结束）⇒ 等它的 stderr 读完、带着原因说。
        let said = if matches!(child.try_wait(), Ok(Some(_))) {
            let s = exit_said(&why, &tail.finish());
            reap_later(child, Some(out), StderrTail { reader: None }, None);
            s
        } else {
            reap_later(child, Some(out), tail, None);
            exit_said(&why, &[])
        };
        return Err(Unopened::Process(said));
    }
    reap_later(child, Some(out), tail, Some(late));
    Ok((pid, n))
}

// 原住窗口那一侧 `shell.rs`：判的是 monitor 起的那个进程，随「起进程那一侧」留在这里。
/// 那件事是不是当场就失败了 —— 是（＝预算内就结束了）回 `true`。「当场就死了」不许被报成成功。
/// 「结束了没有」是入参：线程那一侧问 `is_finished()`、进程那一侧问 `try_wait()`，轮询只此一份（`rust_timer_registry` 里登记的那个 `wait-for-condition`）。
///
/// 够用：起不来那一形在毫秒级就结束（二进制不对 · 没有图形会话 · 种子解不出），起来了的那一条会一直占着直到窗口关闭。
/// 买不到「窗口真的出现在屏幕上」与慢失败。预算刻意短（300ms：加在用户那一次点击上的延迟）；不注入假时钟（换不掉任何 IO）。
pub fn early_failure(mut finished: impl FnMut() -> bool, budget: std::time::Duration) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < budget {
        if finished() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// 入口那条命令用的预算。**独立常量**，因为判据要按它喂合成输入。
pub const EARLY_FAILURE_BUDGET: std::time::Duration = std::time::Duration::from_millis(300);

/// 读窗口进程 stdout 上的**第一行**。`Ok(None)` = 一句没说就 EOF（它退了）；`Err` = 说了但不是约定形状。
///
/// # Errors
///
/// 读失败 · 那一行解不出 [`Ready`]。
pub fn read_ready(r: &mut impl std::io::BufRead) -> Result<Option<Ready>, String> {
    let mut line = String::new();
    let n = r
        .read_line(&mut line)
        .map_err(|e| copy_text("rsFilewinProc.ready.readFailed", &[("e", &e.to_string())]))?;
    if n == 0 {
        return Ok(None);
    }
    decode_ready(&line).map(Some)
}

/// 收尸：`Lifetime::Detached` 不改变父子关系 ⇒ 不 `wait` 就留僵尸。
/// 一条阻塞在 `waitpid` 上的线程是零 CPU、零唤醒的等法（等用户关窗没有上界）；它不持有窗口的任何东西。monitor 先退出的话，那个进程被 init 接管、由 init 收。
/// 它也把那根 stdout 读到 EOF（`out`）：就绪那一行之后不该再有输出，但万一某个库往 stdout 打，读掉扔了。
/// `late` 只在「判成功」那一支给（别的几支当场已经回过错）。
fn reap_later(
    child: crate::spawn_managed::ManagedChild,
    out: Option<std::io::BufReader<std::process::ChildStdout>>,
    tail: StderrTail,
    late: Option<LateExit>,
) {
    std::thread::spawn(move || {
        if let Some(mut out) = out {
            if let Err(e) = std::io::copy(&mut out, &mut std::io::sink()) {
                tracing::debug!("窗口进程的 stdout 读到一半断了（{e}）—— 只影响排空，不影响收尸");
            }
        }
        match child.wait_for_status() {
            Ok(st) if !st.success() => {
                let tail = tail.finish();
                tracing::warn!("文件窗口开出来之后又退出了：{st}");
                if let Some(say) = late {
                    let why = copy_text("rsFilewinProc.late.exited", &[("st", &st.to_string())]);
                    say(exit_said(&why, &tail));
                }
            }
            Ok(_) => drop(tail.finish()),
            Err(e) => tracing::warn!("收不掉那个窗口进程（{e}）—— 内核里会留一条僵尸记录"),
        }
    });
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/proc_tests.rs"]
mod tests;
