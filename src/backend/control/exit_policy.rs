//! 〔B2 · 条 66 · `设计/01 §3.3b`〕**「退出行为」那个值的唯一住址** —— `~/.cc-monitor/backend.json`。
//!
//! # 它从哪搬来、为什么非搬不可
//!
//! 搬家前它住 monitor 自己的 `config.json`（键 `backendPolicy`，**按 origin 分键**），
//! 启动时与改动时由前端**推**进 monitor 进程内的一张表，退出臂读那张表。
//! `§3.3b ①` 逐字给过它真会犯的错：A 把 B 设成「独立」→ A 没干净退出 → C 连上 B
//! ⇒ C 拿**自己那份**默认值把 B 的行为悄悄改掉，而没有任何人做过这个决定。
//! ⇒ **一台机器一个值**，住在后端所在的那台机器上，由后端读写。
//!
//! # 四件事，各一句（对照 `§3.3b` 的编号）
//!
//! - ② **住址**：`<家目录>/.cc-monitor/backend.json`（[`DIR_NAME`] / [`FILE_NAME`]）。
//!   `~/.cc-monitor/` 本来就是后端在每台机器上的家（`bin/ccm` 住那里），不另起门户。
//! - ③ **谁写**：只有本模块的 [`answer_set`]，而它只从 `inbound.rs` 那一条登记进来
//!   （帧面 `exit-policy-set` ＋ 派生的 CLI 面）。前端要改它就发那条命令，**前端从不碰这个文件**。
//!   单写者 ⇒ 前端「读—改—写整份」拿陈旧副本盖掉别人那一形**在构造上不存在**。
//! - ④ **什么时候读**：**决定那一刻现读**（[`read_now`]），不缓存、不在启动时读一次存内存。
//!   本模块生产段一个 `static` / `OnceLock` / `Mutex` 都没有 —— 判据
//!   `tests::the_decision_path_has_exactly_the_registered_cache_points`（E2）按**相等**钉那个 0。
//! - ⑤ **读不出来**：按缺省（[`DEFAULT_KILL_ON_EXIT`] = 不结束）办，**并且说出口** ——
//!   [`Read::Unreadable`] 与 [`Read::Absent`]（没人选过）是两个不同的状态，线上也分开报
//!   （`state` 一格），界面据此说不同的话。
//!
//! # ⚠ `§3.3b ⑦` 的 `lingerMs` **本路没做**（如实登记，交主会话拍板）
//!
//! 「归零后等 `lingerMs` 再决定」= 一个会自己醒来的一次性构件，而后端零定时器铁律
//! （`no_timer_guard::backend_production_code_has_no_periodic_wakeups`，按**调用形态**禁 `sleep(` 等八个名字、
//! **没有登记口**）不放行它。`§3.3b ⑦` 论证过「通信层零期限常量那条不受影响」，**没论证过后端 P6**。
//! ⇒ 本路不给 P6 开口子，也**不把 `lingerMs` 写进文件当摆设**（一个写得进去、没人读的值比没有更坏）：
//! 归零即现读即决定。
//!
//! # 它**不**决定什么
//!
//! 本模块只答「这台机器的值是什么」。**谁在什么时候按它动手**住两处，都是「最后一个客户走了」那一刻：
//! 常驻那条载体的流结束（`main.rs::serve_listening`）· monitor 退出臂现问一次（它收自己起的那两个子进程）。
//! 逐条登记在 `tests::DECISION_SITES`。

use std::path::{Path, PathBuf};

/// 后端在每台机器上的家目录名（相对用户家目录）。
pub const DIR_NAME: &str = ".cc-monitor";

/// 那个值住的文件名。**全仓只有这一处写这个字面量**（E1 的判据按它找写者）。
pub const FILE_NAME: &str = "backend.json";

/// 文件里那一格的键。键名与线上 `data.killOnExit` 同一个词，免得两个名字说一件事。
pub const KEY_KILL_ON_EXIT: &str = "killOnExit";

/// 缺省：**不结束**（`C8③` 的前半句，`§3.3b ⑤` 逐字「本条不推翻」）。
pub const DEFAULT_KILL_ON_EXIT: bool = false;

// 〔S5 · 第四波 · V105 清账〕这里原来有 `pub const SHELL: &str = "standalone"`，
//   随线上 `shell` 那一格一起删了：「折进前端进程」那一档已放弃（`99 §1` V105），壳只剩独立进程，
//   这一格恒为同一个值、唯一的读者是界面那条永远走不到的「不适用」臂（E4，同拍删）。
//   线上形状由 `tests::the_wire_shape_is_exactly_the_four_registered_fields` 按键集相等钉住。

/// 现读一次的结果。**三态，不许合并**（`§3.3b ⑤`：「读不出来」与「用户选了默认」不是一回事）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// 文件在、读得懂、那一格是个布尔 —— **有人选过**。
    Chosen(bool),
    /// 文件不在 —— **没人选过**，按缺省办（今天每台没碰过这个开关的机器都是这一态）。
    Absent,
    /// 文件在但读不出来（权限 / 不是 JSON / 那一格缺了或不是布尔 / 家目录解析不出来）。
    /// 按缺省办，**并带上读不出来的原因**。
    Unreadable(String),
}

impl Read {
    /// 生效值：选过就用选的，否则缺省。
    pub fn kill_on_exit(&self) -> bool {
        match self {
            Read::Chosen(k) => *k,
            Read::Absent | Read::Unreadable(_) => DEFAULT_KILL_ON_EXIT,
        }
    }

    /// 线上 `state` 那一格。三个词与 TS 那侧 `ExitPolicyState` 逐字对齐。
    pub fn state(&self) -> &'static str {
        match self {
            Read::Chosen(_) => "chosen",
            Read::Absent => "absent",
            Read::Unreadable(_) => "unreadable",
        }
    }
}

/// 这台机器上那个文件的路径。家目录：`HOME`，没有再退 `USERPROFILE`（Windows）。
/// 两个都没有 ⇒ `None`（调用方把它说成「读不出来」，**不猜一个路径**）。
pub fn policy_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|h| !h.is_empty()))?;
    Some(PathBuf::from(home).join(DIR_NAME).join(FILE_NAME))
}

/// 读一次 `path`。**每次调用都真去读盘**，没有任何记忆。
pub fn read_at(path: &Path) -> Read {
    let body = match std::fs::read_to_string(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        Err(e) => return Read::Unreadable(format!("读 {} 失败：{e}", path.display())),
    };
    let v: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return Read::Unreadable(format!("{} 不是 JSON：{e}", path.display())),
    };
    match v.get(KEY_KILL_ON_EXIT) {
        Some(serde_json::Value::Bool(k)) => Read::Chosen(*k),
        Some(other) => Read::Unreadable(format!(
            "{} 里 `{KEY_KILL_ON_EXIT}` 不是布尔（实得 {other}）",
            path.display()
        )),
        None => Read::Unreadable(format!(
            "{} 里没有 `{KEY_KILL_ON_EXIT}` 这一格",
            path.display()
        )),
    }
}

/// **决定那一刻**调它：现解路径、现读盘。
pub fn read_now() -> Read {
    match policy_path() {
        Some(p) => read_at(&p),
        None => Read::Unreadable("家目录解析不出来（HOME / USERPROFILE 都没有）".into()),
    }
}

/// **最后一个客户走了**的那一刻问一次：这台机器要不要跟着退（`§3.3b ④⑥`）。
///
/// 只被常驻那条载体的流结束那一臂调（`main.rs::serve_listening`）。今天那条载体**单流**
/// （第二条流回 `stream-busy`，`single_stream_guard.rs` 看着）⇒ 连接计数恒 ∈ {0, 1}，
/// 「归零」＝「那一条流断了」。只读 hello 就走的连接**不是客户**，不进这一问。
///
/// 读不出来 ⇒ 按缺省（不结束）办，**并出声**（`warn`）—— 那不等于有人这么选过。
pub fn last_client_left() -> bool {
    let r = read_now();
    match &r {
        Read::Unreadable(why) => tracing::warn!(
            "最后一个客户走了；退出策略读不出来（{why}）⇒ 按缺省（不结束）办 —— 这不等于有人这么选过"
        ),
        other => tracing::info!(
            "最后一个客户走了；退出策略 state={} killOnExit={}",
            other.state(),
            other.kill_on_exit()
        ),
    }
    r.kill_on_exit()
}

/// 文件的全部内容。**只有一格**，写的时候整份写（单写者 ⇒ 不需要读—改—写）。
fn render(kill: bool) -> String {
    format!("{{\"{KEY_KILL_ON_EXIT}\":{kill}}}\n")
}

/// **全仓唯一的写者**（E1）。
///
/// 做法：`O_EXCL` 新建一份按 pid 命名的临时文件 → 写满 → `sync` → 原子 `rename` 盖到目标上。
/// ⇒ 读者（[`read_at`]）要么看到旧的整份、要么看到新的整份，看不到写了一半的。
/// 目录不在就建**这一层**（`~/.cc-monitor` 本身；它的父目录是家目录，本来就在）。
/// 失败时把临时文件删掉，**不留垃圾、不静默**。
fn write_at(path: &Path, kill: bool) -> Result<(), String> {
    use std::io::Write as _;
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} 没有父目录", path.display()))?;
    if let Err(e) = std::fs::create_dir(dir) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(format!("建 {} 失败：{e}", dir.display()));
        }
    }
    // 〔HX2〕第四层同一条规矩：写之前拿那个目录的跨进程锁（`platform/lock.rs`）。这一份没有读—改—写（整份一格），
    //   锁在这里只为「每一份第四层写口都在锁里写」这条规矩没有例外（`readonly_guard` 第四层 ⑥）。
    let _lock = crate::platform::lock::hold(dir)?;
    let tmp = dir.join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| format!("建临时文件 {} 失败：{e}", tmp.display()))?;
        f.write_all(render(kill).as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(|e| format!("写临时文件 {} 失败：{e}", tmp.display()))?;
        drop(f);
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("把 {} 挪到 {} 失败：{e}", tmp.display(), path.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 一次读数的线上形状（`exit-policy-read` 与 `exit-policy-set` 共用）。
fn wire(r: &Read, path: Option<&Path>) -> serde_json::Value {
    serde_json::json!({
        "state": r.state(),
        "killOnExit": r.kill_on_exit(),
        "reason": match r { Read::Unreadable(why) => Some(why.clone()), _ => None },
        "path": path.map(|p| p.display().to_string()),
    })
}

/// `exit-policy-read`：现读一次，原样报三态。**从不报错** —— 「读不出来」是一个状态，不是一次失败。
pub fn answer_read() -> serde_json::Value {
    let path = policy_path();
    wire(&read_now(), path.as_deref())
}

/// `exit-policy-set`：写 `args.killOnExit`，**写完再读一遍**，回读到的那份
/// （回的是盘上的事实，不是「我以为写进去了」）。
pub fn answer_set(args: &serde_json::Value) -> Result<serde_json::Value, (&'static str, String)> {
    let kill = args
        .get(KEY_KILL_ON_EXIT)
        .and_then(|v| v.as_bool())
        .ok_or((
            "bad_args",
            format!("少了 `{KEY_KILL_ON_EXIT}`，或它不是布尔"),
        ))?;
    let path = policy_path().ok_or((
        "io_failed",
        "家目录解析不出来（HOME / USERPROFILE 都没有）—— 不猜一个路径去写".to_string(),
    ))?;
    write_at(&path, kill).map_err(|e| ("io_failed", e))?;
    Ok(wire(&read_at(&path), Some(&path)))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/exit_policy_tests.rs"]
mod tests;
