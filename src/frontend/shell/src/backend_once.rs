//! **问我们自己放下去的那一份后端一次**（帧命令的 CLI 面，不经 shell）与它用的那段等法：起它 · 带上限地等 · 读两根管子。
//! 本机后端自举（`place-verdict`）走它；同步命令里问后端也走它（同步命令里不 `block_on` 连常驻后端）。

/// 收全的一次：退出码（被信号杀 ⇒ `None`）· 两根管子。
pub(crate) struct Captured {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

/// 没收全的三形（互不合并：下一步各不相同）。
pub(crate) enum CaptureFail {
    /// 起不来（原话）。
    Spawn(String),
    /// 上限内没退出（已杀掉、已收尸）。
    TimedOut,
    /// 读它的输出出错（原话）。
    Read(String),
}

/// 起它（`input` ⇒ 从 stdin 交这一段、交完就关）、等它（带上限）、读两根管子（没接的那根是空）。
/// [`ask_once`] 与本机后端引导那一问共用这一段等法 —— 两份手写的 wait/read 之间只会漂。
pub(crate) fn capture_full(
    timeout: std::time::Duration,
    spawn: &dyn Fn() -> std::io::Result<crate::spawn_managed::ManagedChild>,
    input: Option<Vec<u8>>,
) -> Result<Captured, CaptureFail> {
    use std::io::{Read, Write};
    let mut child = spawn().map_err(|e| CaptureFail::Spawn(e.to_string()))?;
    // 入参另起一条线程写：对面不读 stdin 就先退出时，写端会断，这里不许因此挂住（写错不采信，等它的退出码说话）。
    let writer = match (child.stdin.take(), input) {
        (Some(mut w), Some(bytes)) => Some(std::thread::spawn(move || {
            let _ = w.write_all(&bytes);
        })),
        _ => None,
    };
    // ⚠ 读线程必须在**等之前**起：管道缓冲写满时子进程会阻塞在 write 上，
    // 那时再怎么等都等不到它退出 —— 「等它退出再读」是个会自锁的顺序。
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            // 〔E 吞错普查点名〕读错**带回来**：原先 `let _ =` 吞掉，读坏了与「没输出」长得一样（都判成没装）。
            let read = match pipe {
                Some(mut s) => s.read_to_end(&mut buf).err().map(|e| e.to_string()),
                None => None,
            };
            (buf, read)
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = std::time::Instant::now() + timeout;
    let (timed_out, code) = loop {
        match child.try_wait() {
            Ok(Some(st)) => break (false, st.code()),
            Err(_) => break (false, None),
            Ok(None) => {}
        }
        if std::time::Instant::now() >= deadline {
            break (true, None);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if timed_out {
        // 杀掉它，读线程随管道关闭而结束 —— 否则那条线程会跟着挂死的子进程一起留下。
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(w) = writer {
        let _ = w.join();
    }
    let (stdout, out_err) = out.join().unwrap_or_default();
    let (stderr, err_err) = err.join().unwrap_or_default();
    if timed_out {
        // 超时那次**不采信半截输出**：半截的 JSON 也可能恰好成形，读成一个没说完的答。
        return Err(CaptureFail::TimedOut);
    }
    // 读坏了的那一截同超时那一形：**不采信半截输出**。
    if let Some(e) = out_err.or(err_err) {
        return Err(CaptureFail::Read(e));
    }
    Ok(Captured {
        code,
        stdout,
        stderr,
    })
}

/// 〔判定只在后端〕[`ask_once`] 没问成的几形。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OnceErr {
    /// 起不来（原话）。
    Spawn(String),
    /// 上限内没答完（已杀掉）。
    TimedOut,
    /// 它答了「不」：CLI 面的错信封 `{code, message}`（`message` 是后端对人说的那一句）。
    Refused { code: String, message: String },
    /// 答话不成形：退出码与两根管子上的原话（截短）。
    Unreadable(String),
}

/// **问我们自己放下去的那一份后端一次**：帧命令的 CLI 面 `<bin> -- --<cmd>`（`stream/cli.rs`：入参 JSON 走 stdin、
/// 读到 EOF；exit 0 ⇒ stdout 一行 JSON · exit 2 ⇒ stderr 一行 `{code, message}`），不经 shell。
/// 同步命令里问后端就走这一条（`INVARIANTS §10`：同步命令里不 `block_on` 连常驻后端）；本机后端自举（[`ask_place_verdict`]）也走它。
pub(crate) fn ask_once(
    bin: &std::path::Path,
    cmd: &str,
    args: &serde_json::Value,
    timeout: std::time::Duration,
) -> Result<serde_json::Value, OnceErr> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let flag = format!("--{cmd}");
    let got = capture_full(
        timeout,
        &|| {
            let mut c = std::process::Command::new(bin);
            // 后端认的 argv 写在 `--` 右边（`control::ccm::route`）。
            c.args(["--", flag.as_str()])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped());
            // 三条策略：不闪窗口（Windows 上问的是控制台子系统的 `ccm.exe`）· 超时收整棵 · stderr 是答话的一半（错信封）⇒ 接出来。
            spawn_managed_cmd(
                &mut c,
                ConsolePolicy::Hidden,
                Lifetime::JobKillOnClose,
                StderrSink::Captured,
            )
        },
        Some(args.to_string().into_bytes()),
    )
    .map_err(|f| match f {
        CaptureFail::Spawn(e) => OnceErr::Spawn(e),
        CaptureFail::TimedOut => OnceErr::TimedOut,
        CaptureFail::Read(e) => OnceErr::Unreadable(e),
    })?;
    let line = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    let (out, err) = (line(&got.stdout), line(&got.stderr));
    let unreadable = || {
        let said: String = format!("exit={:?} stdout={out:?} stderr={err:?}", got.code)
            .chars()
            .take(400)
            .collect();
        OnceErr::Unreadable(said)
    };
    match got.code {
        Some(0) => serde_json::from_str(&out).map_err(|_| unreadable()),
        Some(2) => {
            let v: serde_json::Value = serde_json::from_str(&err).map_err(|_| unreadable())?;
            match (v["code"].as_str(), v["message"].as_str()) {
                (Some(c), Some(m)) => Err(OnceErr::Refused {
                    code: c.to_string(),
                    message: m.to_string(),
                }),
                _ => Err(unreadable()),
            }
        }
        _ => Err(unreadable()),
    }
}

/// 本机后端自举那一问的上限：一个后端进程起来、读一遍落点那个文件（约 10 MB）就答；Windows 上第一次跑一份新写的 exe
/// 杀毒软件会先扫一遍（几秒）⇒ 放宽到 20 秒。本机后端引导持锁等它（`local_backend_host::start_local_backend` 头注的代价那一段）。
pub(crate) const PLACE_ASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 〔形状 A〕宿主交给 `local_backend::extract_embedded_to` 的问话口（`local_backend::PlaceAsk`）：跑暂存件的 `--place-verdict` 问一次。
pub(crate) fn ask_place_verdict(
    staged: &std::path::Path,
    args: &serde_json::Value,
) -> Result<serde_json::Value, OnceErr> {
    ask_once(staged, "place-verdict", args, PLACE_ASK_TIMEOUT)
}
