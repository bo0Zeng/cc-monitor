//! `24e` 第二刀：**用户可达的入口** —— 那唯一一条 `#[tauri::command]`。
//!
//! 第一刀落地之后这个窗口只能由代码调 [`super::shell::open_detached`]
//! ⇒ 界面上没有任何地方点得开它。本模块补的就是那一格。
//!
//! # 🔴 一、为什么先列一趟目录、再开窗（这一条是承重的）
//!
//! 最省事的写法是「`spawn` 一条线程就返回 `Ok(())`」。**那是一条静默成功**：
//! 开窗失败（没有图形会话 / DISPLAY 不对）与远端连不上，在 webview 那侧
//! 长得跟成功一模一样 —— 用户点了按钮，什么都没发生，也没有任何一句话。
//!
//! ⇒ 本命令**先等那个目录真的列出来**，列不出来就**带着原文报错返回**，webview 那侧照旧弹它的失败提示；
//! 列出来了才开窗，而且那一屏**直接交给窗口**（[`super::shell::open_detached_seeded`]）
//! ⇒ 窗口一出来就是有内容的，也不会对同一个目录连打两次往返。
//!
//! 🔴**谁去列换了：从 monitor 换成窗口进程自己。** 上一版这里由 monitor 经宿主注入的句柄
//! 问那台后端 `files-home` / `files-ls`（`host_ask` · `ask_home` · `list_first_screen`〔散文墓碑〕），列好的一屏放进种子 ——
//! 那是 monitor 替窗口问后端（待迁那一行）。今天窗口进程拨回通道后自己问（`proc::first_screen`），
//! 在 stdout 上说一行「列到 N 行」或「列不出来：原话」；本命令只起进程、等那一行（[`super::proc::open_in_new_process`]）。
//! 「列不出来就不开窗、带原话」一个字没变，变的是它在哪个进程里成立。
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
//! # 🔴 三、签名为什么只吃 `RemoteConfig` —— 因为**只有一侧**
//!
//! 〔2026-09-23 本机侧退役〕这一节从前讲的是一处不对称：窗口的数据面两侧都通，
//! 而这条命令只开远端那一侧，本机侧靠窗口自己工具栏上那颗按钮走到。
//! **那处不对称不在了** —— 窗口的数据面今天只有远端一侧
//! （那条白名单原文住 `super::source` 头注那块墓碑）。
//!
//! ⇒ `parity_ledger` 那一行照旧签 `Side::Remote`，而它**从「签实况」变成了「签全部」**：
//! 本机那一侧没有另一条可达路径等着被记上来了。
//!
//! 🔴〔第七刀补记〕`path` 传空串现在是**合法调用**，意思是「开在远端 home」（〔09-28 裁 3〕由窗口进程去问）——
//! 于是顶栏那颗按钮接过来时**不必先自己解一趟路径**（那正是老面板今天在做的事）。
//! 逐条理由住 [`open_file_window`] 的 `# 🔴` 那一节。
//!
//! ⚠ 顶栏那个 SFTP 入口（`src/frontend/ui/sftp-host-picker.ts::openSftpFromTopbar`，0 台提示 / 1 台直开 / 多台选单）
//! **这一刀没碰** —— `src/frontend/ui/main.ts` 不在本刀写区。要把原生窗口接到顶栏上，
//! 得连那颗按钮一起改，那是下一刀的事（而且那一刀正好是「旧面板退役」那一刀）。

use crate::copy_table::copy_text;
use crate::detail::Said;
use crate::stream_source::RemoteConfig;

use super::proc::{open_in_new_process, OpenRequest, ProcFail, Unopened};

/// 这一趟要落在哪儿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// 直接进这个目录（老面板 `initialDir`，F78）。
    Dir(String),
    /// 进 `dir` 并高亮 `name` 那一行（老面板 `revealPath`，F54）。
    Reveal { dir: String, name: String },
    /// 都没说 ⇒ 问那台机器上的后端 `files-home`（第七刀立的这一支；从 SFTP 换成了后端；
    /// 〔09-28 裁 3〕由窗口进程自己问：种子里 `cwd` 缺席）。**只有这一支要多一趟 IO。**
    Home,
}

/// 🔴**三者的优先级** —— 与老面板逐字相同：
/// `path`（非空）> `reveal_file` > home。
///
/// # 为什么抽成纯函数
///
/// 它埋在那条 `async fn` 里的时候**判不了**：那条路第一步就要连远端
/// （本仓红线不许起真连接）⇒ 优先级选错了哪一支，在失败路径上**看不出来**
/// （两支回的是同一句池错误 —— 第七刀那条判据栽过同一形）。
/// 抽出来之后它零 IO、三支都判得到。
/// ⇒ 同 `source::home_from_reply` / `row_from_ls_entry` 那条方法学。
///
/// # 为什么照抄老面板那个优先级
///
/// **不是省事** —— 前端那几条调用点今天就是按它写的
/// （`src/sftp/panel.ts::open` 的 `initialDir > revealPath > home`）。
/// 换一个优先级就得同时改那几处，而那是另一件活。
///
/// # 🔴 父目录与尾段**在这一侧算**
///
/// 前端不许自己切远端路径：老面板那侧是 TS 的 `parentPath` / `basename` 各一份，
/// 而窗口这条路只有 [`super::source::parent_dir`] / [`super::source::remote_basename`]
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
    // 🔴〔2026-09-23 本机侧退役〕**这里少了一个占位配置，那是买到的东西之一。**
    //    从前 `parent_dir` 的签名吃 `&Source`（两侧不是同一个切法），于是这一行
    //    得合成一份「一个字段都不会被读」的 `RemoteConfig` 喂给它（`synthetic_remote`）。
    //    本机那一侧退役之后 `parent_dir` 只吃一条字符串 ⇒ 那个占位整个不需要了。
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
/// 上一版这里逐字写着「路径是空的 ⇒ 立刻回错（**不拿 `.` 兜底**：远端的 `.`
/// 归谁解释是 `sftp_realpath` 的事，在这里猜一个默认值就是把两处的规矩写成两份）」。
///
/// **那条理由是对的，而它现在被满足了，不是被推翻了** —— 我们没有在这里猜一个默认值，
/// 我们去问了那个唯一权威（那台机器上的后端 `files-home`；〔09-28 裁 3〕窗口进程的 `proc::first_screen` 问）。
/// 「在这里猜」与「去问那个说得上话的」是两件事，上一版只有前者可选，所以它选了回错。
///
/// 🔴**问的对象换了：从 SFTP 换成后端。** 第七刀那一版问的是
/// SFTP 的 `realpath(".")`（monitor 为这一问单拨一条 SFTP）；现在问后端 `files-home`，
/// 与列第一屏那一趟**同一条路**（〔09-28 裁 3〕都在窗口进程里）。⇒ monitor 这一侧开窗一个 SFTP 都不拨了。
///
/// ## 为什么必须有这一格
///
/// 在这之前，这条命令吃的绝对路径**唯一来源**是老面板 `src/sftp/panel.ts` 那处
/// `sftp_realpath(cfg, ".")` ⇒ **窗口连「自己开在远端 home」都做不到**，
/// 而 `P3`（老面板退役）因此排不动 —— 退役那一刀会把窗口的起点一起带走。
///
/// ## ⚠ 那条性质一个字都没松
///
/// 「说不出要看哪儿就别开一个空窗」**照旧成立**：问不到 home（连不上 / 对面把 `.`
/// 解成空串或相对路径）⇒ 带着原文回错，**一个窗口都不开**。
/// 变的只是「说不出」的判定从「入参是空的」换成「入参是空的**而且**问不到」。
///
/// # Errors
///
/// - `path` 是空的、且远端 home 问不出来 ⇒ 带着原文回错，不开窗。
/// - 目录列不出来（连不上 / 没权限 / 不是目录）⇒ 把窗口进程说的那句原文带回去。
/// 「复制到另一台」下拉里的机器：本机（`<local>`）＋ 已配的远端（`origin_label`，与通道寻址同一个名字）。
/// 来自已有的配置读口，不新建数据源。
fn machine_names() -> Vec<String> {
    std::iter::once(crate::inbound_client::LOCAL_ORIGIN.to_string())
        .chain(
            crate::load_remote_configs()
                .iter()
                .map(RemoteConfig::origin_label),
        )
        .collect()
}

#[tauri::command]
pub async fn open_file_window(
    app: tauri::AppHandle,
    cfg: RemoteConfig,
    path: String,
    reveal_file: Option<String>,
    theme: std::collections::BTreeMap<String, String>,
) -> Result<usize, Said> {
    let r: Result<usize, Said> = async move {
        use tauri::{Emitter, Manager};
        // 窗口的样子：主界面此刻 `:root` 上的那一套（含用户改过的）解成数。解不出来就不开（不替它补一套）。
        let theme = filewin_contract::Theme::from_tokens(&theme)?;
        // 主窗所在那台显示器的工作区（窗口进程开出来第一拍夹进它）。
        let work_area = app
            .get_webview_window(crate::MAIN_WINDOW_LABEL)
            .as_ref()
            .and_then(crate::work_area_of);
        // 开出来之后又不体面地退了 ⇒ 经远端健康那条通道出声（同一个 toast 出口）。
        let origin = cfg.origin_label();
        let late: super::proc::LateExit = Box::new(move |f| {
            let said = process_said(f);
            let payload = crate::ui_contract::RemoteHealthPayload {
                origin,
                kind: FILEWIN_EXIT_KIND.to_string(),
                message: said.said,
                detail: said.detail,
            };
            if let Err(e) = app.emit(crate::ui_contract::events::REMOTE_HEALTH, payload) {
                tracing::warn!("文件窗口没了那一条没有发出去：{e}");
            }
        });
        Ok(open_with(cfg, path, reveal_file, work_area, theme, late).await?)
    }
    .await;
    r.map_err(|s| s.named("open_file_window"))
}

/// 文件窗口开出来之后又退了那一形在 `remote-health` 上的 `kind`（界面 `remote-health.ts` 按它选标题）。
pub(crate) const FILEWIN_EXIT_KIND: &str = "filewin-exit";

/// [`open_file_window`] 去掉「问 Tauri」那一步之后的全部（判据从这里进：判据进程里没有 `AppHandle`）。
pub(crate) async fn open_with(
    cfg: RemoteConfig,
    path: String,
    reveal_file: Option<String>,
    work_area: Option<host_core::WorkArea>,
    theme: filewin_contract::Theme,
    late: super::proc::LateExit,
) -> Result<usize, Said> {
    // 窗口进程只拿那台的名字（寻址用，`origin_label` 口径）；它不认识 monitor 的配置类型。
    let origin = cfg.origin_label();
    // 书签文件住 monitor 自己的数据目录（不是用户文件），路径在这一侧算好交过去（名字只住 `data_paths`）。
    let bookmarks = crate::config::resolve_monitor_data_dir()
        .map(|d| d.join(crate::data_paths::FILEWIN_BOOKMARKS_FILE));
    let view = crate::config::resolve_monitor_data_dir()
        .map(|d| d.join(crate::data_paths::FILEWIN_VIEW_FILE));
    // ⓪ 三者优先级 —— 那一段是**纯函数**（[`plan_target`]），理由见它的头注。
    //    〔09-28 裁 3〕home 那一支不在这里问了：`cwd` 缺席交给窗口进程（`proc::first_screen`）。
    let (cwd, reveal) = match plan_target(&path, reveal_file.as_deref()).map_err(command)? {
        Target::Dir(d) => (Some(d), None),
        Target::Reveal { dir, name } => (Some(dir), Some(name)),
        Target::Home => (None, None),
    };
    // 通道就是窗口进程的 stdin / stdout（`chan::host::serve_window`）：帧长上限随种子交过去，两端同一个数。
    let req = OpenRequest {
        origin,
        cwd,
        reveal,
        frame: crate::chan::host::FRAME_MAX_BYTES,
        bookmarks,
        view,
        machines: machine_names(),
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
        .map_err(|e| {
            Said::new(
                copy_text("rsFilewinEntry.open.crashed", &[]),
                OPEN_COMMAND,
                Some(&e.to_string()),
            )
        })?;
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
    if machine == crate::inbound_client::LOCAL_ORIGIN {
        return Err((
            "local_has_no_file_window",
            copy_text("rsFilewinEntry.other.localNone", &[]),
        ));
    }
    let Some(theme) = filewin_contract::filewin_open_theme(args) else {
        return Err(("bad_args", copy_text("rsFilewinEntry.other.badArgs", &[])));
    };
    let Some(cfg) = crate::load_remote_config_by_label(machine) else {
        return Err((
            "no_such_origin",
            copy_text(
                "rsFilewinEntry.other.noConfig",
                &[("machine", &machine.to_string())],
            ),
        ));
    };
    let late: super::proc::LateExit =
        Box::new(|f| tracing::warn!("另开的文件窗口退了：{} {:?} {:?}", f.said, f.code, f.raw));
    open_with(cfg, String::new(), None, None, theme, late)
        .await
        .map(|_| ())
        .map_err(|why| ("open_failed", why.said))
}

/// 开窗那条命令的名字（复制详情的「命令」那一项）。
const OPEN_COMMAND: &str = "open_file_window";

/// 开窗那条路上只有一句话的失败 ⇒ 带命令名的那一形。
fn command(said: String) -> Said {
    Said::new(said, OPEN_COMMAND, None)
}

/// 进程这一层没成 ⇒ 那一句 ＋ 复制详情（命令 · 退出状态 · stderr 末几行）。
fn process_said(f: ProcFail) -> Said {
    Said::coded(f.said, OPEN_COMMAND, f.code.as_deref(), f.raw.as_deref())
}

/// 开窗没成 ⇒ 给 webview 的那一句。窗口进程列不出来时说的那句**原话原样**交出去（与上一版 monitor 自己列不出来时回的是同一句）；
/// 进程这一层的错套上「文件窗口没起来」，退出状态与 stderr 进复制详情。
fn unopened_said(u: Unopened) -> Said {
    match u {
        Unopened::Said(said) => command(said),
        Unopened::Process(f) => process_said(ProcFail {
            said: copy_text("rsFilewinEntry.open.failed", &[("why", &f.said)]),
            ..f
        }),
    }
}

// 〔09-28 裁 3〕`list_first_screen` · `ask_home` · `host_ask`〔散文墓碑〕与开窗前那两问的期限退役：
//   monitor 这一侧不再替窗口问后端；那两问进了窗口进程（`proc::first_screen`，期限 `proc::FIRST_SCREEN_BUDGET`）。

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/filewin/entry_tests.rs"]
mod tests;
