//! **用户可达的入口** —— 那唯一一条 `#[tauri::command]`。
//!
//! 别处只由代码调 [`super::shell::window::open_detached_seeded`] 开这个窗口；界面上点得开它的只这一格。
//!
//! # 🔴 一、为什么先列一趟目录、再开窗（这一条是承重的）
//!
//! 最省事的写法是「`spawn` 一条线程就返回 `Ok(())`」。**那是一条静默成功**：
//! 开窗失败（没有图形会话 / DISPLAY 不对）与远端断开，在 webview 那侧
//! 长得跟成功一模一样 —— 用户点了按钮，什么都没发生，也没有任何一句话。
//!
//! ⇒ 本命令**先等那个目录真的列出来**，列不出来就**带着原文报错返回**，webview 那侧照旧弹它的失败提示；
//! 列出来了才开窗，而且那一屏**直接交给窗口**（[`super::shell::window::open_detached_seeded`]）
//! ⇒ 窗口一出来就是有内容的，也不会对同一个目录连打两次往返。
//!
//! ⚠ **它买不到「窗口真的出现在屏幕上」**。那一格要一个图形会话，本机
//! `XDG_SESSION_TYPE=tty`⇒ 本机永远量不到。
//! 能确定地量到的是「那个窗口进程起来了、而且没有在开窗预算内就退」，
//! 与「这个目录此刻列得出来」（回值那个行数）。**两件都不是「窗口在屏幕上」，别读宽。**
//!
//! 🔴**开窗 ＝ 起一个独立进程。** 上一版这一节的①逐字写着
//! 「走 [`super::source::list_remote`]，也就是**共用那条池**」。
//! 〔审计 F 🔴-3 订正〕今天两侧都不走池；〔09-28 裁 3〕开窗前那一屏也由窗口进程经通道 `call` 那台后端
//! `files-home` / `files-ls`（「列不出来就别开窗」一个字没变），与它之后的每一次列目录同一条路，一行 SFTP 都不碰。代价与买到的东西逐条住 [`super::proc`] 头注 §二 / §四。
//!
//! # 二、为什么回一个行数，而不是 `()`
//!
//! 回 `()` 的话前端拿不到任何可说的东西，那条「点了之后出声」的链就断在包装层。
//! 回**这一趟列到的行数** ⇒ 前端那句提示里的数是**真读数**，不是文案。
//! ⚠ 刻意不回一个结构体：`设计` 那条纪律是「返回类型只在 TS 侧真消费字段时才生成」
//! （`tests/frontend/ui/ipc/commands.vitest.ts` 头注逐字），一个 `usize` 不需要 `ts-rs`。
//!
//! # 🔴 三、签名为什么只吃一台远端的名字 —— 因为**只有一侧**
//!
//! 窗口的数据面只有远端一侧（那条白名单住 `super::source` 头注「只有远端一侧」那一节）
//! ⇒ `parity_ledger` 那一行签 `Side::Remote`，签的就是全部：本机那一侧没有另一条可达路径。
//!
//! 🔴 `path` 传空串是**合法调用**，意思是「开在远端 home」（由窗口进程去问）——
//! 顶栏那个入口（`src/frontend/ui/sftp-host-picker.ts`，0 台提示 / 1 台直开 / 多台选单）接过来时**不必先自己解一趟路径**。
//! 逐条理由住 [`open_file_window`] 的 `# 🔴` 那一节。

use crate::detail::Shell as _;
use copy_core::copy_text;
use copy_core::said::Said;

use super::proc::{open_in_new_process, OpenRequest, ProcFail, Unopened};

/// 这一趟要落在哪儿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// 直接进这个目录（远端会话「打开工作目录」，F78）。
    Dir(String),
    /// 进 `dir` 并高亮 `name` 那一行（工具卡上的文件链接「定位文件」，F54）。
    Reveal { dir: String, name: String },
    /// 都没说 ⇒ 问那台机器上的后端 `files-home`（由窗口进程自己问：种子里 `cwd` 缺席）。
    /// **只有这一支要多一趟 IO。**
    Home,
}

/// 🔴**三者的优先级**：`path`（非空）> `reveal_file` > home。
///
/// # 为什么抽成纯函数
///
/// 它埋在那条 `async fn` 里的时候**判不了**：那条路第一步就要连远端
/// （本仓红线不许起真连接）⇒ 优先级选错了哪一支，在失败路径上**看不出来**
/// （两支回的是同一句池错误）。
/// 抽出来之后它零 IO、三支都判得到。
/// ⇒ 同 `source::home_from_reply` / `row_from_ls_entry` 那条方法学。
///
/// # 为什么是这个优先级
///
/// 前端那几条调用点（`file-window.vitest.ts` 点名的那几处）就是按它写的：给了目录就进目录，
/// 只给了文件就定位到它，都没给才开在 home。换一个优先级就得同时改那几处。
///
/// # 🔴 父目录与尾段**在这一侧算**
///
/// 前端不许自己切远端路径：窗口这条路只有 [`super::source::parent_dir`] / [`super::source::remote_basename`]
/// 这**一对**。多一份就多一种「Windows 上 `\` 被当分隔符」的机会。
///
/// # Errors
///
/// `reveal_file` 给了但切不出名字（比如它就是 `"/"`）⇒ 报错。
/// **不静默退回 home** —— 那样用户点了「跳到这个文件」，窗口开在别处，而且没有一句话。
pub fn plan_target(path: &str, reveal_file: Option<&str>) -> Result<Target, String> {
    if !path.trim().is_empty() {
        return Ok(Target::Dir(path.to_string()));
    }
    let Some(f) = reveal_file.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(Target::Home);
    };
    let name = filewin_contract::remote_basename(f).to_string();
    if name.is_empty() {
        return Err(copy_text(
            "rsFilewinEntry.plan.noTarget",
            &[("target", &f.to_string())],
        ));
    }
    // 🔴 `parent_dir` 只吃一条字符串（窗口只有远端一侧，切法只有一种）⇒ 这里不用合成任何占位配置。
    let dir = filewin_contract::parent_dir(f);
    Ok(Target::Reveal { dir, name })
}

/// 在**原生窗口**里打开远端 `path` 这个目录。
///
/// 回值 = 这一趟列到的行数。⚠ 它是「开窗那一刻那个目录有多少项」，
/// 不是「窗口里现在有多少行」（窗口自己会刷新、会换目录）。
///
/// # 🔴`path` 是空的 ⇒ **去问远端 home**
///
/// **不拿 `.` 兜底、不在这里猜一个默认值**（远端的 `.` 归谁解释不是这一侧的事，猜就是把两处的规矩写成两份）——
/// 去问那个唯一权威：那台机器上的后端 `files-home`，由窗口进程的 `proc::first_screen` 问，
/// 与列第一屏那一趟**同一条路**。⇒ monitor 这一侧开窗一个 SFTP 都不拨。
///
/// ## 为什么必须有这一格
///
/// 顶栏入口与「开在 home」都靠它：没有这一格，窗口就得先由调用方解出一条绝对路径才开得了。
///
/// ## ⚠ 那条性质一个字都没松
///
/// 「说不出要看哪儿就别开一个空窗」**照旧成立**：问不到 home（断开 / 对面把 `.`
/// 解成空串或相对路径）⇒ 带着原文回错，**一个窗口都不开**。
/// 变的只是「说不出」的判定从「入参是空的」换成「入参是空的**而且**问不到」。
///
/// # Errors
///
/// - `path` 是空的、且远端 home 问不出来 ⇒ 带着原文回错，不开窗。
/// - 目录列不出来（断开 / 没权限 / 不是目录）⇒ 把窗口进程说的那句原文带回去。
/// 「复制到另一台」下拉里的机器：开窗那一刻问本机后端一次（`machine-read`：它机器表里的每台，「连接这台」关着的那几台不列）。
/// 问不到 ⇒ 下拉是空的（落到本机走「下载」，不经这张表）。
async fn machine_names() -> Vec<String> {
    let deadline = crate::frame_query::Deadline::within(std::time::Duration::from_secs(5));
    let got = crate::frame_query::ask(
        &crate::origin::Origin::local(),
        &cmd_wire::machine::MachineReadReq {},
        None,
        deadline,
    )
    .await;
    match got {
        Ok(r) => r
            .machines
            .into_iter()
            .filter(|(_, m)| m.state != cmd_wire::machine::MachineStateKind::Disabled)
            .map(|(name, _)| name)
            .collect(),
        Err(f) => {
            tracing::info!(
                "filewin: machine-read not answered, cross-copy list left empty: {}",
                f.said
            );
            Vec::new()
        }
    }
}

#[tauri::command]
pub async fn open_file_window(
    app: tauri::AppHandle,
    origin: String,
    path: String,
    reveal_file: Option<String>,
    theme: std::collections::BTreeMap<String, String>,
) -> Result<usize, cmd_wire::failed::Failed> {
    let r: Result<usize, Said> = async move {
        use tauri::Manager;
        // 窗口的样子：主界面此刻 `:root` 上的那一套（含用户改过的）解成数。解不出来就不开（不替它补一套）。
        let theme = filewin_contract::Theme::from_tokens(&theme).map_err(Said::from)?;
        // 主窗所在那台显示器的工作区（窗口进程开出来第一拍夹进它）。
        let work_area = app
            .get_webview_window(crate::MAIN_WINDOW_LABEL)
            .as_ref()
            .and_then(crate::work_area_of);
        // 开出来之后又不体面地退了 ⇒ 告诉用户（出错 toast 那一个出口）；退出状态与错误输出进复制详情。
        let late: super::proc::LateExit = Box::new(|f| {
            crate::ui_error::tell(crate::ui_error::UiError::FileWindowExited {
                code: f.code,
                raw: f.raw,
            })
        });
        Ok(open_with(origin, path, reveal_file, work_area, theme, late).await?)
    }
    .await;
    r.map_err(|s| s.named("open_file_window"))
}

/// [`open_file_window`] 去掉「问 Tauri」那一步之后的全部（判据从这里进：判据进程里没有 `AppHandle`）。
pub(crate) async fn open_with(
    origin: String,
    path: String,
    reveal_file: Option<String>,
    work_area: Option<filewin_contract::WorkArea>,
    theme: filewin_contract::Theme,
    late: super::proc::LateExit,
) -> Result<usize, Said> {
    // 窗口进程只拿那台的名字（寻址用，本机后端机器表里的那个名字）。
    // 视图文件住 monitor 自己的数据目录（不是用户文件），路径在这一侧算好交过去（名字只住 `data_paths`）。
    //   书签不在这里：它归那台后端，窗口按机器名自己问（`bookmarks-read` / `-edit`）。
    let view = crate::config::resolve_monitor_data_dir()
        .map(|d| d.join(crate::data_paths::FILEWIN_VIEW_FILE));
    // ⓪ 三者优先级 —— 那一段是**纯函数**（[`plan_target`]），理由见它的头注。
    //    〔09-28 裁 3〕home 那一支不在这里问了：`cwd` 缺席交给窗口进程（`proc::first_screen`）。
    let (cwd, reveal) = match plan_target(&path, reveal_file.as_deref()).map_err(Said::from)? {
        Target::Dir(d) => (Some(d), None),
        Target::Reveal { dir, name } => (Some(dir), Some(name)),
        Target::Home => (None, None),
    };
    // 通道就是窗口进程的 stdin / stdout（`chan::host::serve_window`）：帧长上限随种子交过去，两端同一个数。
    let req = OpenRequest {
        origin,
        cwd,
        reveal,
        frame: crate::chan::host::FRAME_MAX_BYTES.as_len(),
        view,
        machines: machine_names().await,
        work_area,
        theme,
        local_line: crate::detail::local_line(),
    };
    // 🔴**起一个独立进程**；〔09-28 裁 3〕它先列第一屏、说一行，再开窗。
    //
    //    逐条理由住 `proc` 头注（用户「窗口生命周期就是销毁」那条裁决 ＋
    //    「winit 一个进程只许一个事件循环」那条现打事实 ⇒ 同进程形态下
    //    「关掉就销毁」与「还能再打开」不可同时成立）。
    //
    //    ⚠ 这里**不等窗口关闭**：那会把这条 Tauri 命令挂在一个窗口的寿命上。等的只是就绪那一行
    //      （阻塞读 ⇒ 放在 `spawn_blocking` 上，不占 async worker）。
    //    🔴 `D11`：一条退路都没有。起不了独立进程就是错，照实报（`proc` 里那几档
    //      各自带着自己的原因），**不许**退回同进程开一个。
    let opened = tokio::task::spawn_blocking(move || open_in_new_process(&req, late))
        .await
        .map_err(|e| Said::with_raw(copy_text("rsFilewinEntry.open.crashed", &[]), e))?;
    let (pid, n) = opened.map_err(unopened_said)?;
    tracing::info!("文件窗口起在进程 {pid} 上（第一屏 {n} 行，它自己列的）");
    Ok(n)
}

/// 文件窗口左栏「其他机器」点了一台 ⇒ 窗口经通道 `call` 那一条（`filewin_contract::FILEWIN_OPEN_OP`，寻址 ＝ 要开的那台），
/// monitor 在这里接：按名字取那台的配置，照开窗入口同一条路（[`open_with`]）起一个新的窗口进程，开在那台的 home。
/// 样子沿用发起那扇窗开窗时的那一套（参数里带来）；工作区不夹（这一跳手上没有主窗）。
pub(crate) async fn open_from_window(
    machine: &str,
    args: &serde_json::Value,
) -> Result<(), (&'static str, String)> {
    // 本机没有文件窗口（窗口只开在远端上）⇒ 说真实原因，不让下面那句「没有叫 <local> 的远端配置」出声。
    if machine == cmd_wire::LOCAL {
        return Err((
            "local_has_no_file_window",
            copy_text("rsFilewinEntry.other.localNone", &[]),
        ));
    }
    let Some(theme) = filewin_contract::filewin_open_theme(args) else {
        return Err(("bad_args", copy_text("rsFilewinEntry.other.badArgs", &[])));
    };
    // 表里有没有那台由本机后端答（窗口第一屏那一问回它那一码）。
    let late: super::proc::LateExit =
        Box::new(|f| tracing::warn!("另开的文件窗口退了：{} {:?} {:?}", f.said, f.code, f.raw));
    open_with(machine.to_string(), String::new(), None, None, theme, late)
        .await
        .map(|_| ())
        .map_err(|why| ("open_failed", why.said))
}

/// 进程这一层没成 ⇒ 那一句 ＋ 码（退出状态）· 原话（stderr 末几行）；命令名由开窗那条命令投出去时补。
fn process_said(f: ProcFail) -> Said {
    let s = Said::from(f.said).and_raw(f.raw.as_deref());
    match f.code.as_deref() {
        Some(code) => s.coded(code),
        None => s,
    }
}

/// 开窗没成 ⇒ 给 webview 的那一句。窗口进程列不出来时说的那句**原话原样**交出去（与上一版 monitor 自己列不出来时回的是同一句）；
/// 进程这一层的错套上「文件窗口没起来」，退出状态与 stderr 进复制详情。
fn unopened_said(u: Unopened) -> Said {
    match u {
        Unopened::Said(said) => Said::from(said),
        Unopened::Process(f) => process_said(ProcFail {
            said: copy_text("rsFilewinEntry.open.failed", &[("why", &f.said)]),
            ..f
        }),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/entry_tests.rs"]
mod tests;
