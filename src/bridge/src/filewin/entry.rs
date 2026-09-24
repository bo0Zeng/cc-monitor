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
//! ⇒ 本命令**先真的把那个目录列出来**（远端走 [`super::source::list_remote`]，
//! 也就是共用那条池），列不出来就**带着原文报错返回**，webview 那侧照旧弹它的失败提示；
//! 列出来了才开窗，而且把**那一屏直接交给窗口**（[`super::shell::open_detached_seeded`]）
//! ⇒ 窗口一出来就是有内容的，也不会对同一个目录连打两次往返。
//!
//! ⚠ **它买不到「窗口真的出现在屏幕上」**。那一格要一个图形会话，本机
//! `XDG_SESSION_TYPE=tty`（`真相源/99 §一`）⇒ 本机永远量不到。
//! 能确定地量到的是「那个窗口进程起来了、而且没有在开窗预算内就退」，
//! 与「这个目录此刻列得出来」（回值那个行数）。**两件都不是「窗口在屏幕上」，别读宽。**
//!
//! 🔴〔第十三刀 2026-09-23〕**开窗 ＝ 起一个独立进程。** 上一版这一节的①逐字写着
//! 「走 [`super::source::list_remote`]，也就是**共用那条池**」——**那半句今天只对一半**：
//! 列这一趟仍然在 monitor 进程里走共用池（所以「列不出来就别开窗」一个字没变），
//! 但**窗口自己那一侧换了进程** ⇒ 它之后的每一次列目录走的是它自己那份池。
//! 代价与买到的东西逐条住 [`super::proc`] 头注 §二 / §四，别在这里读第二遍。
//!
//! # 二、为什么回一个行数，而不是 `()`
//!
//! 回 `()` 的话前端拿不到任何可说的东西，那条「点了之后出声」的链就断在包装层。
//! 回**这一趟列到的行数** ⇒ 前端那句提示里的数是**真读数**，不是文案。
//! ⚠ 刻意不回一个结构体：`设计` 那条纪律是「返回类型只在 TS 侧真消费字段时才生成」
//! （`tests/ipc/commands.vitest.ts` 头注逐字），一个 `usize` 不需要 `ts-rs`。
//!
//! # 🔴 三、签名为什么只吃 `RemoteConfig` —— 因为**只有一侧**
//!
//! 〔2026-09-23 本机侧退役〕这一节从前讲的是一处不对称：窗口的数据面两侧都通，
//! 而这条命令只开远端那一侧，本机侧靠窗口自己工具栏上那颗按钮走到。
//! **那处不对称不在了** —— 窗口的数据面今天只有远端一侧
//! （用户裁决与那条白名单原文住 `super::source` 头注那块墓碑）。
//!
//! ⇒ `parity_ledger` 那一行照旧签 `Side::Remote`，而它**从「签实况」变成了「签全部」**：
//! 本机那一侧没有另一条可达路径等着被记上来了。
//!
//! 🔴〔第七刀补记〕`path` 传空串现在是**合法调用**，意思是「开在远端 home」——
//! 于是顶栏那颗按钮接过来时**不必先自己解一趟路径**（那正是老面板今天在做的事）。
//! 逐条理由住 [`open_file_window`] 的 `# 🔴〔第七刀〕` 那一节。
//!
//! ⚠ 顶栏那个 SFTP 入口（`src/main.ts::openSftpFromTopbar`，0 台提示 / 1 台直开 / 多台选单）
//! **这一刀没碰** —— `src/main.ts` 不在本刀写区。要把原生窗口接到顶栏上，
//! 得连那颗按钮一起改，那是下一刀的事（而且那一刀正好是「旧面板退役」那一刀）。

use crate::ssh_source::RemoteConfig;

use super::proc::{open_in_new_process, OpenRequest};
use super::source::Source;

/// 这一趟要落在哪儿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// 直接进这个目录（老面板 `initialDir`，F78）。
    Dir(String),
    /// 进 `dir` 并高亮 `name` 那一行（老面板 `revealPath`，F54）。
    Reveal { dir: String, name: String },
    /// 都没说 ⇒ 问那台机器上的后端 `files-home`（第七刀立的这一支；〔F7a〕从 SFTP 换成了后端）。
    /// **只有这一支要多一趟 IO。**
    Home,
}

/// 🔴〔第十刀〕**三者的优先级** —— 与老面板逐字相同：
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
    let name = super::source::remote_basename(f).to_string();
    if name.is_empty() {
        return Err(format!("`{f}` 看不出要高亮哪个文件"));
    }
    // 🔴〔2026-09-23 本机侧退役〕**这里少了一个占位配置，那是买到的东西之一。**
    //    从前 `parent_dir` 的签名吃 `&Source`（两侧不是同一个切法），于是这一行
    //    得合成一份「一个字段都不会被读」的 `RemoteConfig` 喂给它（`synthetic_remote`）。
    //    本机那一侧退役之后 `parent_dir` 只吃一条字符串 ⇒ 那个占位整个不需要了。
    let dir = super::source::parent_dir(f);
    Ok(Target::Reveal { dir, name })
}

/// 在**原生窗口**里打开远端 `path` 这个目录。
///
/// 回值 = 这一趟列到的行数。⚠ 它是「开窗那一刻那个目录有多少项」，
/// 不是「窗口里现在有多少行」（窗口自己会刷新、会换目录）。
///
/// # 🔴〔第七刀 2026-09-21〕`path` 是空的 ⇒ **去问远端 home**
///
/// 上一版这里逐字写着「路径是空的 ⇒ 立刻回错（**不拿 `.` 兜底**：远端的 `.`
/// 归谁解释是 `sftp_realpath` 的事，在这里猜一个默认值就是把两处的规矩写成两份）」。
///
/// **那条理由是对的，而它现在被满足了，不是被推翻了** —— 我们没有在这里猜一个默认值，
/// 我们去问了那个唯一权威（[`ask_home`] → 那台机器上的后端 `files-home`）。
/// 「在这里猜」与「去问那个说得上话的」是两件事，上一版只有前者可选，所以它选了回错。
///
/// 🔴〔F7a · 第三波 2026-09-24〕**问的对象换了：从 SFTP 换成后端。** 第七刀那一版问的是
/// SFTP 的 `realpath(".")`（monitor 为这一问单拨一条 SFTP）；现在经通道宿主的同一个句柄
/// 问后端 `files-home`，与下面列第一屏那一趟**同一条路**。⇒ monitor 这一侧开窗一个 SFTP 都不拨了。
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
/// - 目录列不出来（连不上 / 没权限 / 不是目录）⇒ 把 `sftp_pool` 那边的原文带回去。
#[tauri::command]
pub async fn open_file_window(
    cfg: RemoteConfig,
    path: String,
    reveal_file: Option<String>,
) -> Result<usize, String> {
    let source = Source::remote(cfg);
    // ⓪ 三者优先级 —— 那一段是**纯函数**（[`plan_target`]），理由见它的头注。
    let (path, reveal) = match plan_target(&path, reveal_file.as_deref())? {
        Target::Dir(d) => (d, None),
        Target::Reveal { dir, name } => (dir, Some(name)),
        // 只有这一支要 IO ⇒ 它留在 async 这一侧。〔F7a〕问的是后端，不是 SFTP。
        Target::Home => (ask_home(&source).await?, None),
    };
    // 🔴〔F2 · 2026-09-24〕通道没起来 ⇒ 开不了窗（`D11`：窗口只有这一条路够后端）。
    let handoff = crate::chan::host::handoff()
        .ok_or_else(|| "主程序的通道口没起来，文件窗口够不着后端".to_string())?;
    // ① 先真的列一趟 —— 〔F2〕问那台机器上的后端，**与窗口里那一次走同一条路**：
    //    通道宿主注入给路由器的那个句柄（`chan::host::InboundBackends`）。列不出来就别开窗。
    let rows = list_first_screen(&source, &path).await?;
    let n = rows.len();
    // ② 🔴〔第十三刀 2026-09-23〕**拿着这一屏起一个独立进程。**
    //
    //    逐条理由住 `proc` 头注（用户「窗口生命周期就是销毁」那条裁决 ＋
    //    「winit 一个进程只许一个事件循环」那条现打事实 ⇒ 同进程形态下
    //    「关掉就销毁」与「还能再打开」不可同时成立）。
    //
    //    ⚠ 这里**不等窗口关闭**，理由一个字没变：那会把这条 Tauri 命令挂在一个
    //      窗口的寿命上。变的是它在等什么 —— 从「那条线程」变成「那个进程」。
    //    🔴 `D11`：一条退路都没有。起不了独立进程就是错，照实报（`proc` 里那几档
    //      各自带着自己的原因），**不许**退回同进程开一个。
    let pid = open_in_new_process(&OpenRequest {
        source,
        cwd: path,
        rows,
        reveal,
        handoff,
        // 〔FW34〕书签文件住 monitor 自己的数据目录（不是用户文件），路径在这一侧算好交过去。
        bookmarks: crate::paths::resolve_monitor_data_dir().map(|d| super::bookmarks::file_in(&d)),
    })
    .map_err(|why| format!("窗口没起来：{why}"))?;
    tracing::info!("文件窗口起在进程 {pid} 上（{n} 行已经交给它了）");
    Ok(n)
}

/// 开窗之前那一屏 —— 问后端 `files-ls`，经通道宿主的生产句柄（monitor 进程里）。
///
/// 🔴 **为什么不是自己拨一次通道**：这一段就住在 monitor 进程里，路由器身后那个句柄
/// （`InboundBackends`）就在手边；绕一圈回环口只多一跳、零收益。它与窗口里那一次
/// **落在同一个句柄上**（`inbound_client` → 那台机器的后端），不是两条路。
/// ⚠ 失败那句话走 [`super::source::said`] —— 与窗口里那一次**同一个翻译**。
async fn list_first_screen(
    source: &Source,
    dir: &str,
) -> Result<Vec<super::source::Listed>, String> {
    let args = serde_json::json!({ "path": dir, "limit": super::source::LS_LIMIT });
    let d = host_ask(source, super::source::CMD_LS, &args).await?;
    super::source::rows_from_ls_data(&d, super::source::SortBy::default()).map(|(rows, _)| rows)
}

/// 〔F7a · 第三波 2026-09-24〕开窗前「那台机器的 home 在哪」—— 问后端 `files-home`。
///
/// 与 [`list_first_screen`] **同一条路**（[`host_ask`]）；有逻辑的那一段（解字节 ＋ 判能不能当起点）
/// 住 [`super::source::home_from_reply`]，本函数自己没有逻辑。
async fn ask_home(source: &Source) -> Result<String, String> {
    let d = host_ask(source, super::source::CMD_HOME, &serde_json::json!({})).await?;
    super::source::home_from_reply(&d)
}

/// monitor 这一侧问后端的**唯一一处**：经通道宿主注入给路由器的那个生产句柄。
///
/// 🔴 **为什么不是自己拨一次通道**：这一段就住在 monitor 进程里，路由器身后那个句柄
/// （`InboundBackends`）就在手边；绕一圈回环口只多一跳、零收益。
/// ⚠ 失败那句话走 [`super::source::said`] —— 与窗口里那一次**同一个翻译**（话里带着命令名，
/// 于是「是问 home 那一跳还是列目录那一跳没走通」在报错上分得开）。
async fn host_ask(
    source: &Source,
    cmd: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::chan::router::Backends as _;
    use crate::chan::wire::{Body, CancelToken, Op};
    let payload =
        Body(serde_json::to_vec(args).map_err(|e| format!("`{cmd}` 的参数拼不出来：{e}"))?);
    let body = crate::chan::host::InboundBackends
        .call(
            source.origin(),
            Op(cmd.to_string()),
            payload,
            FIRST_SCREEN_BUDGET,
            CancelToken::new(),
        )
        .await
        .map_err(|e| super::source::said(cmd, &e))?;
    serde_json::from_slice(&body.0).map_err(|e| format!("`{cmd}` 的应答读不动：{e}"))
}

/// 开窗前那两问（home · 第一屏）各自的往返上限（调用方给的期限）。
const FIRST_SCREEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(20);

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/entry_tests.rs"]
mod tests;
