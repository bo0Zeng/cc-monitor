//! 要求：「文件窗口成独立包 `src/frontend/filewin/`」· 「窗口是又一个前端：只说 `call` / `subscribe`」
//! 独立的包：库面 ＋ 本包的 `[[bin]] cc-monitor-filewin`（`src/main.rs`，一行转调 [`run`]）。
//!
//! 它依赖的只有契约类与前端宿主原语（`comms-inward` 通道 · `filewin-contract` 种子 · `copy-core` 文案 · `creds-core` 家目录规矩 ·
//! `relay-route-core` 暂存区路径 · `host-core` 原子写与窗口几何）＋ 自己的界面库；一个判定类 crate 都不链，也不链 monitor
//! （`tests/frontend/shell/contract_crate_guard_tests.rs` 钉）。monitor 那一侧（开窗入口 · 起进程 · 读就绪那一行）住壳里 `filewin/`。
//!
//! 一个窗口一个进程：winit 一个进程只许一个事件循环，「关掉就销毁」＋「还能再打开」只在独立进程下同时成立
//! （形状与买到 / 买不到住 [`proc`] 的头注）。
//!
//! 🔴 egui 命中测试平手时取后登记的那个（「In case of a tie, take the last one = the one on top」）：
//! 「整行可点」那块矩形后登记、横向拉满整行宽的话，**行上那颗按钮编得过、画得出、永远点不到**
//! ⇒ 整行那块矩形**让开按钮那一段**，判据两侧都钉（点按钮要回按钮、点空白处双击要回那一行）。

// 🔴**大文件模式**：只排视口内的行。
pub mod batch;
pub mod bigfile;
// 帧日志：`CCM_FILEWIN_FRAME_LOG=<文件>` 打开时每帧记一行（为什么重画 · 画面与布局变没变），收集「一闪一闪」的证据；不设就什么都不做。
pub mod frame_log;
// 🔴**书签** —— 存 monitor 自己的状态文件
//    （不是用户文件），两个进程的写都走同一个「上锁 → 现读 → 改 → 原子换」。逐条理由住那份头注。
pub mod bookmarks;
// **上传的块形**：SFTP 起始目录不是后端 home 时经后端链路分块写。
pub mod chunk_upload;
// 窗口的框：工具条 · 命令栏 · 左栏 · 状态栏（作用于焦点那一栏）。
pub mod chrome;
pub mod copy;
// 合成语料（判据用，采结构不采内容）：只在测试里编，发版那份二进制里没有它。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/corpus.rs"]
pub mod corpus;
// **复制到另一台机器**（§7 第 9 条 Q2）：A 下到本机暂存 → 传到 B → B 提交，一条进度、可撤、半路清暂存。
pub mod cross_copy;
// 🔴**新建空文件**（`files-create`，经通道）。
pub mod create;
// 🔴**往外拖** —— `sftp_download` 在窗口上的落点
//    （那条欠账逐字「往外拖（下载）没做」）。
pub mod download;
// 🔴**改一份远端文本** —— 把「那个上限该多少 ·
//    超了怎么办」两个问题指名留给了 `24e`，答复逐条住 `editor.rs` 头注。
pub mod editor;
// **解压到这里**：后端 `files-extract` 的窗口那一侧（发 · 撞名就问 · 摆一句话）。
pub mod extract;
// 🔴**进程形态** —— 一个窗口一个进程。
//    头注里写着它买到的三条（关窗不再可能带走整个 app · 第二趟开窗成功 · 165 MiB 真还回去）
//    与它买不到的三条（后端通道够不着 · 第二条 SSH 连接 · 装机那份还没有这个二进制）。
pub mod proc;
// 🔴 `files-read` 这一族在**客户端侧**的命令面与消费面 ——
//    搜索＋ 新鲜度那一行（`§3.5.3` 那条 ⬜）＋ `§3.5.2a` 那条节奏判据。
pub mod find;
pub mod fonts;
// 按内容搜（`files-grep`）在窗口上的命令面与消费面。
pub mod grep;
// 文件种类：图标与「类型」那一列的字、按类型怎么排（怎么画的事，只在这里判）。
// **图片查看页**：图片开成当前这一栏里的一个标签（适应窗口 · 原尺寸 · 缩放 · 拖动 · 翻同目录）。
pub mod imgview;
pub mod kind;
pub mod kit;
// **有损名下载**：远端按字节就地拷进暂存区再走传输，本机按原始字节 / 有损形落名。
pub mod lossy_pull;
// **预览**：右侧一块只读面板（已有的 `files-read-text`，自己的上限，最多一趟在飞）。
pub mod preview;
// 窗口底部那张「进度」表：一趟一行（上传 · 下载 · 复制 · 解压 · 算大小 · 删除），所有标签页共用一份。
pub mod progress;
// 右键「属性」：问那台 `files-stat`，摆一个框。
pub mod props;
// **原生选文件框**：上传 / 存到哪儿那两问的「浏览…」。
pub mod picker;
// **一张图读一次、解一次**：预览 · 「看一眼」· 查看页共用那一份（`files-read-chunk` 逐块读，窗口解码）。
pub mod picture;
// 本包的平台层（平台 cfg 只许住这里）。
pub mod platform;
// **计划反查**：目录落在某一片的仓库里 ⇒「格」一列 · 预览头「归属」· 右键「在计划里看」（后端 `plan-files` 判，这里只排版）。
pub mod plan;
// 渲染器挑哪块显卡：真显卡在前、软件渲染垫底（规则只住这一处；Windows 上接 wgpu 的那几行住 `platform.rs`）。
pub mod render;
pub mod rows;
// 秤 F1（首屏 · 滚动帧 · 内存，判据用）：只在测试里编，发版那份二进制里没有它。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/scale.rs"]
pub mod scale;
// 🔴**选中态 · 键位 · 右键菜单那张表**。
//    三件问的是同一个问题（「选中了哪几行、能对它们做什么」），纯的一份住这儿；
//    接到窗口上的那几跳住 `shell/keys.rs`。逐条理由住那份文件的头注。
pub mod select;
pub mod shell;
// **算大小**：后端 `files-size` 的窗口那一侧（发 · 解 · 摆一句话）。
pub mod size;
pub mod source;
// 窗口的样子：开窗种子里那一套主题 → egui 的配色 · 字号 · 间距（色值只住这一处换算）。
pub mod theme;
pub mod transfer;
// **一趟**：各看板共用的那一格（在做哪一项 · 落地 · 结局）与落地时只判一次的结局（进度表与标签页收场读同一份）。
pub mod trip;
// 工具栏「上传」那一问：选完走拖入那一条。
pub mod upload;
// 窗口的最外一层：**标签页 ＋ 双栏 ＋ 复制到另一栏**。
//    `FileWindow` 不拆（它就是一个标签页），`eframe::App` 挪到这里。逐条理由住那份头注。
pub mod workspace;
// 🔴：那四条**写**命令在窗口上的落点
//    （新建目录 · 删除 · 改名 · 改权限）。取消那一条住 [`transfer`]。
pub mod writeops;

// 与 app 那条边界的判据（`boundary_tests`）挂在 monitor 那一侧（壳里 `filewin/mod.rs`）：它判的是 monitor 那一侧够不够得着这里。

/// 窗口进程的入口：本包的 `[[bin]] cc-monitor-filewin`（`src/main.rs`）只转调这一个函数。
pub fn run() -> i32 {
    proc::child_main()
}

// Windows 进程清单进了本包每一个链接产物（`build.rs` 调 `win_manifest::embed_for_every_artifact`），测试程序读回自己那一份。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/app_manifest_tests.rs"]
mod app_manifest_tests;
// 出口扫描判据的 Rust 那一半（壳与文件窗口两棵生产树一起扫）。
#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/exit_scan_tests.rs"]
mod exit_scan_tests;
// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本 crate 的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;
