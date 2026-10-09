//! 文件窗口进程的**躯体**（原住壳里 `filewin/proc.rs` 的「子进程那一侧」，逐字搬来）：读种子 → 拨回通道 → 列第一屏、在 stdout 上说那一行 → 开窗。
//!
//! 进程形态（一窗一进程 · 种子走 stdin · 为什么）与 monitor 那一侧（起进程 · 写种子 · 读就绪那一行 · 收尸）住壳里
//! `src/frontend/shell/src/filewin/proc.rs` 的头注；两边对上的形状住 `filewin-contract`。

use copy_core::copy_text;

use super::source::{Listed, Source};
use filewin_contract::{decode_request, encode_ready, Ready};

/// 窗口进程拨回 monitor 那个通道口、出示钥匙，换一条线。**拨不通就是错**（`D11`）。
///
/// 抽成具名函数是为了让它可判：[`child_main`] 要读 stdin、要开真窗口，判据跑不动它；
/// 而「拿着交接件拨不拨得通、拨不通说什么」这一段不需要窗口。
/// 期限是这里给的（说法归调用方）。
///
/// # Errors
///
/// 连不上 / 钥匙不对 / 期限内没答 —— 带着通道那一层的分层原因。
pub async fn dial_back(
    h: &comms_inward::chan::handoff::Handoff,
) -> Result<super::source::Line, String> {
    use comms_inward::chan::wire::{Budget, CancelToken};
    let budget = Budget {
        until: std::time::Instant::now() + DIAL_BUDGET,
        cancel: CancelToken::new(),
    };
    comms_inward::chan::dial::dial(h, budget)
        .await
        .map_err(|e| copy_text("rsFilewinProc.dialBack.failed", &[("e", &e.to_string())]))
}

/// 拨回 monitor 那个通道口（回环）的期限。回环上连一次 ＋ 一来一回的认证，给得很宽。
pub const DIAL_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// 开窗前那两问（home · 第一屏）各自的往返上限（调用方给的期限；〔09-28 裁 3〕随那两问从 `entry.rs` 搬来）。
pub const FIRST_SCREEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

/// 〔09-28 裁 3〕**开窗前那一屏，窗口进程自己问**：`cwd` 缺席 ⇒ 先问那台 `files-home`；再列那个目录。
/// 回 `(起点, 那一屏)`。与窗口里之后每一次列目录同一条路（[`super::source::list_dir`] → [`super::source::ask`]）。
///
/// # Errors
///
/// home 问不到 / 解不出起点 · 目录列不出来 —— 带那一跳的原话（[`super::source::said`] 翻过的）。
pub async fn first_screen(
    line: &super::source::Line,
    source: &Source,
    cwd: Option<String>,
) -> Result<(String, Vec<Listed>), String> {
    let cwd = match cwd {
        Some(d) => d,
        None => {
            let d = super::source::ask(
                line,
                &source.origin(),
                super::source::CMD_HOME,
                &serde_json::json!({}),
                FIRST_SCREEN_BUDGET,
            )
            .await?;
            super::source::home_from_reply(&d)?
        }
    };
    let (rows, _truncated) =
        super::source::list_dir(line, source, &cwd, super::source::SortBy::default()).await?;
    Ok((cwd, rows))
}

/// 在 stdout 上说那一行。父进程已经不在了（管子断了）⇒ 只在 stderr 上记一句，不当成窗口的错。
fn say(r: &Ready) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Err(e) = out
        .write_all(encode_ready(r).as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("就绪那一行没送出去（{e}）");
    }
}

/// 说「列不出来」并回那个退出码（窗口不开）。
fn refuse(said: String, code: i32) -> i32 {
    eprintln!("{said}");
    say(&Ready::Failed(said));
    code
}

/// 种子解不出来时的退出码。
pub const EXIT_BAD_SEED: i32 = 2;
/// 窗口没立起来时的退出码。
pub const EXIT_WINDOW_FAILED: i32 = 1;
/// 〔09-28 裁 3〕第一屏列不出来（home 问不到 / 目录列不出来）时的退出码 —— 窗口没开。
pub const EXIT_NOT_LISTED: i32 = 3;

/// **窗口进程的躯体。** `[[bin]]` 那个入口只有一行，调的就是它。
///
/// 回值 = 进程退出码。四档刻意分开（`D7`：失败要显式、归因要准确）：
/// `0` 窗口开过又关了 · [`EXIT_WINDOW_FAILED`] 窗口立不起来 ·
/// [`EXIT_BAD_SEED`] 种子读不动（那是 monitor 与它之间的契约漂了，不是显示问题）·
/// [`EXIT_NOT_LISTED`] 第一屏列不出来（窗口没开）。
/// 〔09-28 裁 3〕开窗之前的每一种失败都**也**在 stdout 上说一行 [`Ready::Failed`]（父进程据此带原话回错）。
///
/// ⚠ **它把原因印在 stderr 上**，而那根 stderr 是继承来的（见 [`spawn_window`]）
/// ⇒ 从终端里起的 monitor 上看得见。装机那份 GUI app 没有 stderr 控制台
/// ⇒ 那句话今天会丢。**如实登记**：把它接进 monitor 的滚动日志要 `StderrSink::ToLog`，
/// 而那一格要一条泵、而且会把「窗口的话」与「后端的话」灌进同一个文件 —— 没顺手做。
pub fn child_main() -> i32 {
    let mut raw = String::new();
    if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw) {
        return refuse(
            copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]),
            EXIT_BAD_SEED,
        );
    }
    let req = match decode_request(&raw) {
        Ok(r) => r,
        Err(e) => return refuse(e, EXIT_BAD_SEED),
    };
    super::source::set_local_line(&req.local_line);
    // 🔴 这个进程里要有一个 tokio 运行时 —— 窗口那一侧的每一次列目录 / 传输 / 搜索
    //    都是 `h.spawn(async …)`。**不给它就等于开一个什么都做不了的窗口**
    //    （`FileWindow` 在 `rt: None` 时会把「这个窗口没拿到运行时」画出来，
    //     那一形是判据夹具的样子，不是生产的）。
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            return refuse(
                copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]),
                EXIT_WINDOW_FAILED,
            )
        }
    };
    // 🔴**先拨通道，拨不通就别开窗**（`D11`：没有退路 ——
    //    不许「连不上就退回 SFTP 自己列」）。
    let line = match rt.block_on(dial_back(&req.handoff)) {
        Ok(c) => c,
        Err(e) => return refuse(e, EXIT_WINDOW_FAILED),
    };
    // 🔴〔09-28 裁 3〕**列不出来就别开窗**（那条纪律从 monitor 那一侧搬到这里，一个字没动）：
    //    先列第一屏，说一行给父进程；列不出来 ⇒ 说原话、退，窗口一个都不开。
    let source = Source::remote(req.origin.clone());
    let (cwd, rows) = match rt.block_on(first_screen(&line, &source, req.cwd.clone())) {
        Ok(first) => first,
        Err(e) => return refuse(e, EXIT_NOT_LISTED),
    };
    say(&Ready::Listed(rows.len()));
    let h = super::shell::open_detached_seeded(
        source,
        cwd,
        Some(rt.handle().clone()),
        Some(line),
        rows,
        req.reveal,
        req.bookmarks,
        req.view,
        req.machines,
        req.work_area,
        Some(req.theme),
    );
    match h.join() {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            eprintln!("{e}");
            EXIT_WINDOW_FAILED
        }
        Err(_) => {
            eprintln!("开窗那条线程炸了（panic）—— 原因在它自己上面那几行");
            EXIT_WINDOW_FAILED
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/proc_tests.rs"]
mod tests;
