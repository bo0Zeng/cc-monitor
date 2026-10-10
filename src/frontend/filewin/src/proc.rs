//! 文件窗口进程的**躯体**（原住壳里 `filewin/proc.rs` 的「子进程那一侧」，逐字搬来）：读种子（stdin 第一行）→ 在自己的 stdin / stdout 上起通道 → 列第一屏、在 stderr 上说那一行 → 开窗。
//!
//! 进程形态（一窗一进程 · 种子走 stdin · 为什么）与 monitor 那一侧（起进程 · 写种子 · 读就绪那一行 · 收尸）住壳里
//! `src/frontend/shell/src/filewin/proc.rs` 的头注；两边对上的形状住 `filewin-contract`。

use copy_core::copy_text;

use super::source::{Listed, Source};
use filewin_contract::{decode_request, encode_ready, Ready};

/// 在自己的 stdin / stdout 上起通道的客户端，换一条线：起它的 monitor 在那对管子的另一头（父子管道，没有钥匙）。
/// 要在 tokio 运行时里调。种子那一行已经从 stdin 读走（[`read_seed_line`]）。
pub fn line_over_stdio(frame: usize) -> super::source::Line {
    comms_inward::chan::client::Client::over(tokio::io::stdin(), tokio::io::stdout(), frame)
}

/// 从 stdin 读种子那**一行**（不读到 EOF：之后同一根管子就是通道）。逐字节读，一个字节都不多拿 ——
/// 多读进缓冲的字节就不在通道那一侧了。
///
/// # Errors
///
/// 读失败 · 一行都没有就 EOF。
pub fn read_seed_line(r: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut out = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match r.read(&mut one) {
            Ok(0) => break,
            Ok(_) if one[0] == b'\n' => break,
            Ok(_) => out.push(one[0]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

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

/// 在 stderr 上说那一行（stdout 是通道）。父进程已经不在了（管子断了）⇒ 没人收，不当成窗口的错。
fn say(r: &Ready) {
    use std::io::Write;
    let mut err = std::io::stderr().lock();
    let _ = err
        .write_all(encode_ready(r).as_bytes())
        .and_then(|()| err.flush());
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
/// 〔09-28 裁 3〕开窗之前的每一种失败都**也**在 stderr 上说一行 [`Ready::Failed`]（父进程据此带原话回错）。
///
/// 它的 stderr 由 monitor 接走（就绪那一行之外的逐行进 monitor 的日志，壳里 `filewin/proc.rs::StderrTail`）。
pub fn child_main() -> i32 {
    let raw = match read_seed_line(&mut std::io::stdin()) {
        Ok(r) => r,
        Err(e) => {
            return refuse(
                copy_text("rsFilewinProc.child.noRuntime", &[("e", &e.to_string())]),
                EXIT_BAD_SEED,
            )
        }
    };
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
    // 通道就是自己的 stdin / stdout（`D11`：没有退路 —— 不许「连不上就退回 SFTP 自己列」）。
    let line = {
        let _in_rt = rt.enter();
        line_over_stdio(req.frame)
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
