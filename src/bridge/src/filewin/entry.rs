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
//! 能确定地量到的是「开窗请求发出去了」（[`super::shell::open_requested`]），
//! 与「这个目录此刻列得出来」（回值那个行数）。**两件都不是「窗口在屏幕上」，别读宽。**
//!
//! # 二、为什么回一个行数，而不是 `()`
//!
//! 回 `()` 的话前端拿不到任何可说的东西，那条「点了之后出声」的链就断在包装层。
//! 回**这一趟列到的行数** ⇒ 前端那句提示里的数是**真读数**，不是文案。
//! ⚠ 刻意不回一个结构体：`设计` 那条纪律是「返回类型只在 TS 侧真消费字段时才生成」
//! （`tests/ipc/commands.vitest.ts` 头注逐字），一个 `usize` 不需要 `ts-rs`。
//!
//! # 🔴 三、签名为什么只吃 `RemoteConfig`（而窗口自己两侧都会）
//!
//! 窗口的数据面两侧都通（`Source::Local` / `Source::Remote`），
//! 但**这条命令只开远端那一侧**，因为界面上点得到它的地方只有一个 ——
//! 旧 SFTP 面板的表头（见 `src/sftp/panel.ts`），而那块面板本来就是远端专用的。
//!
//! ⇒ 于是 `parity_ledger` 那一行签的是 `Side::Remote`，**签的是实况不是愿望**。
//! 本机那一侧今天的可达路径是**窗口自己那颗「本机」按钮**
//! （[`super::shell::FileWindow::go_local`]）—— 它不是第二条 Tauri 命令，
//! 所以它不进那张表；如实记在这儿，别以为本机侧没人走得到。
//!
//! 🔴〔第七刀补记〕`path` 传空串现在是**合法调用**，意思是「开在远端 home」——
//! 于是顶栏那颗按钮接过来时**不必先自己解一趟路径**（那正是老面板今天在做的事）。
//! 逐条理由住 [`open_file_window`] 的 `# 🔴〔第七刀〕` 那一节。
//!
//! ⚠ 顶栏那个 SFTP 入口（`src/main.ts::openSftpFromTopbar`，0 台提示 / 1 台直开 / 多台选单）
//! **这一刀没碰** —— `src/main.ts` 不在本刀写区。要把原生窗口接到顶栏上，
//! 得连那颗按钮一起改，那是下一刀的事（而且那一刀正好是「旧面板退役」那一刀）。

use crate::ssh_source::RemoteConfig;

use super::shell::open_detached_seeded;
use super::source::{list_remote, Source};

/// 这一趟要落在哪儿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// 直接进这个目录（老面板 `initialDir`，F78）。
    Dir(String),
    /// 进 `dir` 并高亮 `name` 那一行（老面板 `revealPath`，F54）。
    Reveal { dir: String, name: String },
    /// 都没说 ⇒ 问远端 `realpath('.')`（第七刀）。**只有这一支要 IO。**
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
/// ⇒ 同 `source::row_from_sftp_entry` / `start_dir_from_realpath` 那条方法学。
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
    // ⚠ `parent_dir` 的远端那一支一个 cfg 字段都不看 ⇒ 这里不需要真配置。
    let dir = super::source::parent_dir(&Source::Remote(Box::new(synthetic_remote())), f);
    Ok(Target::Reveal { dir, name })
}

/// [`plan_target`] 里那一处占位配置。
///
/// 🔴 它存在只因为 [`super::source::parent_dir`] 的签名吃 `&Source`，
/// 而**远端那一支一个 cfg 字段都不看**（它只用 `Source::Remote` 这个判别式选算法）。
/// ⚠ 刻意不把 `parent_dir` 改成吃一个 `bool` —— 那个签名今天挡住了
/// 「拿 `std::path` 切远端路径」那一形（`parent_dir` 头注逐字），
/// 换成 `bool` 之后调用方就能随手传错。⇒ 宁可在这儿多一个占位。
fn synthetic_remote() -> RemoteConfig {
    RemoteConfig {
        host: String::new(),
        label: String::new(),
        port: 0,
        user: String::new(),
        key_path: None,
        backend_path: String::new(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
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
/// 我们去问了那个唯一权威（[`super::source::resolve_remote_home`] → `sftp_realpath`）。
/// 「在这里猜」与「去问那个说得上话的」是两件事，上一版只有前者可选，所以它选了回错。
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
    // ⓪ 三者优先级 —— 那一段是**纯函数**（[`plan_target`]），理由见它的头注。
    let (path, reveal) = match plan_target(&path, reveal_file.as_deref())? {
        Target::Dir(d) => (d, None),
        Target::Reveal { dir, name } => (dir, Some(name)),
        // 只有这一支要 IO ⇒ 它留在 async 这一侧。
        Target::Home => (super::source::resolve_remote_home(&cfg).await?, None),
    };
    // ① 先真的列一趟 —— 走共用那条池（同进程、无 IPC）。列不出来就别开窗。
    let rows = list_remote(&cfg, &path).await?;
    let n = rows.len();
    // ② 拿着这一屏开窗。⚠ 不 `join` 那个句柄：`run_native` 要阻塞到窗口关闭，
    //    等它就等于把 Tauri 的 async 运行时挂在一个窗口的寿命上。
    let h = open_detached_seeded(
        Source::Remote(Box::new(cfg)),
        path,
        tokio::runtime::Handle::try_current().ok(),
        rows,
        reveal,
    );
    // ③ 🔴〔第十一刀〕**那条线程当场就死了的话，别报成功。**
    //
    //    上一版这里是 `let _ = open_detached_seeded(…)`，句柄直接丢掉
    //    ⇒ 窗口起没起来，webview 那侧看到的都是 `Ok(行数)`。
    //    而有一条路**今天必然走到那里**：同一个进程里第二次开窗必然失败
    //    （winit 的进程级事件循环标志，逐条理由住 `shell::early_failure`）
    //    ⇒ 用户关掉窗口再点一次，屏幕上什么都没有、界面却说「成功」。
    //
    //    ⚠ 这一跳**不等窗口关闭**（那会把这条命令挂在一个窗口的寿命上，
    //      正是上面②那条注释说的那件事）—— 它只等一个很短的预算，
    //      分开「早失败」与「起来了」。买不到什么逐条写在 `early_failure` 头注里。
    if super::shell::early_failure(&h, super::shell::EARLY_FAILURE_BUDGET) {
        let why = match h.join() {
            Ok(Err(e)) => e,
            Ok(Ok(())) => "那条线程回了成功，但它在开窗预算内就结束了 —— \
                           窗口没能立起来（这一形此前是静默的）"
                .to_string(),
            Err(_) => "开窗那条线程炸了（panic），原因只落在它自己的 stderr 上".to_string(),
        };
        return Err(format!("窗口没起来：{why}"));
    }
    Ok(n)
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/entry_tests.rs"]
mod tests;
