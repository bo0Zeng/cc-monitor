//! 库 crate 根：模块声明 + Tauri 应用装配。
//!
//! `run()` 在 `tauri::Builder` 之前先 `logging::init`（tracing 全局 dispatcher 必须最先 init），
//! 然后注册 single-instance plugin（须为链上第一个）、`setup()` 里 spawn watcher / 各后台线程
//! 并 `app.manage` 所有 Arc-shared State，最后注册 `invoke_handler`（IPC 命令清单）。
//! State 注册矩阵见 doc/STATE-MATRIX.md；漏 `manage` 不会被 cargo check 抓住（INVARIANT § 8）。

mod account_usage; // F10：per-account Claude 订阅计划用量窗口%（一次性探针会话 + capture-pane）
mod accounts; // A2：多账号（cc-acct-iso）只读查询——账号=一个 CLAUDE_CONFIG_DIR
mod acct_iso_deploy; // F5：一键部署 vendored cc-acct-iso 到远端 + 存在性检测
mod adapter;
mod auto_launch;
mod bind;
mod bridge;
mod cc_bus; // B03：cc-bus 状态的纯解析层（脏数据防御，见 features/B03-dirty-data-samples.md）
mod codex_record; // Phase 2 · F2a：Codex rollout 记录防御式分类器（keystone 第一块）
mod config;
mod config_surface; // T02：配置面审计视图（遍历 tool_registry，只读、不轮询）
mod data_paths;
// U-CC1：数据面漂移记账 —— 把「CC 变了」从不可观测变成看一眼就知道。只记账，零行为变化。
mod drift_ledger;
mod event_replay;
mod fenced_block; // T04 第二步：围栏块配对判定（本机+远端 profile 共用最强那一档）
mod history;
mod hooks_diag; // B04：cc-bus 钩子在 settings.json 里的只读诊断 + 生成待贴文本（绝不写入）
                // U8a-2a：monitor 侧的入方向发送端（往那条长连接的写半边发命令 + 按 id 收应答）。
                // 「hello 之前不许写」在这里是类型上的事实：ParkedWriter 身上没有任何写方法。
mod backend; // P4a（§1.4b）：monitor 侧的后端边界 —— 读/控制两条能力线，宿主无关
mod inbound_client;
mod launch;
mod local_accounts; // L3a：本机多账号枚举（只读）——`accounts.rs` 的本地对侧
mod logging;
mod mcp; // F87（#50+#51）：MCP 管理（读跨 scope 展示 / 写只项目 .mcp.json，SS-14）
mod messages;
mod panorama;
mod parser;
mod paths;
mod port_forward;
mod profile_installer;
mod pubkey;
mod remote_branch; // G6：远端分叉（经 ssh 调 daemon `--fork-session`）——写面故与只读的 remote_history 分家
mod remote_history;
mod search;
mod session_map;
mod sftp_pool;
mod verified_write; // T01：统一的「备份→写→读回比对→回滚」；本机侧从长度比对升级为内容比对
                    // SS-D 统一 SFTP 写层（issue #29 自动部署 F08；后续 F11/F10 复用）。
mod sftp;
// SSH-remote Phase 0 (issue #15)：从 setup() 调用 —— 当 config.json 的
// `remote.enabled = true` 时，ssh_source::run 作为**附加**数据源与本地 jsonl-watcher
// 并行跑（aggregate：本地 + 远端 session 同时显示为 Tab），走相同的
// batch_to_payloads → replay.on_line_batch 出口；远端行带 origin=host 标签。
// remote off（默认）时本模块不被调用，本地路径 bit-for-bit 不变。
mod ccm_probe;
mod ssh_source;
// T01：结构性扫描的可复用形式（枚举+逐个断言+计数自检+钉死逃生口）。
// **只在测试期编译**——它的消费者全在 `#[cfg(test)]` 里（`sftp.rs` 的 tmux 目标守卫、
// `tool_registry.rs` 的字段纪律）。这是测试支撑模块，不是被闲置的生产代码；
// 加 `cfg(test)` 就是把这件事写进类型系统，顺带消掉 5 条 dead_code 警告。
/// U1a：`shared/ccm` 的强度契约（仅测试构建）。U9 迁移后由同一份 `measure()` 对拍新构造点。
///
/// ⚠ **插在这里、不要插在上面那条注释与 `#[cfg(test)]` 之间。** U1a 初版就插错了位置，
/// 把属性与 `mod structural_scan;` 的配对拆开 —— `structural_scan` 当场变成无条件编译，
/// 上面注释里逐字写着的「顺带消掉 5 条 dead_code 警告」被原样撤销，而 CI 的 `cargo build`
/// 没有 `-D warnings` ⇒ **不会红**。是 Phase D 审计数出「dead_code 正好 +5」才发现的。
#[cfg(test)]
mod ccm_cli_contract;
#[cfg(test)]
#[cfg(test)]
#[cfg(test)]
mod parity_ledger; // L5：本地/远端平价对账表（§40 的机制那半；内部整体 cfg(test)）
mod polling_registry; // U7-P：前端 + shared/ccm 的周期唤醒清账（daemon 那条零定时器护栏点名要「单独论证」的那半）
mod quote_singleton_guard; // U8c-2b-0：POSIX 单引号 quote 在 Rust 侧只许有一个实现（账本 S5）
#[cfg(test)]
mod session_name_registry; // U11 摸底：会话名产出点清账 + 递减棘轮（账本 S12 的落地形态）
#[cfg(test)]
mod shared_crate_registry; // U8c-1：新增共享 crate 时 CI 三样都要补 —— 从散文变机检
#[cfg(test)]
mod structural_scan;
mod subagent;
mod tasks;
mod tmux;
mod tmux_daemon_gate_guard; // U10 裁决：daemon 侧没有身份守卫之前，send-keys/kill 不许改走 daemon
mod tmux_reconcile;
mod tool_registry; // T01：受管工具声明（只声明，不改各工具行为）
mod usage;
mod utils;
mod watcher;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Listener, Manager};

/// issue #24：清掉从宿主 shell 继承的 claude 嵌套标记。
///
/// Windows 子进程默认继承全部环境。若 monitor 是从「Claude Code 会话内的 shell」
/// 启动的（开发者跑 `run.ps1 dev` 很常见），这些标记会沿 monitor → wt.exe →
/// powershell → `claude --resume` 一路传下去，resume 出的 claude 被嵌套检测判成
/// **子会话** → 不注册 `sessions/<PID>.json`、不写会话 jsonl（对话只活在内存、
/// 关窗即丢）→ monitor 永远不出 Tab。启动时单点清洗，之后 spawn 的一切子进程
/// 都干净。**保留 `CLAUDE_CONFIG_DIR`**（monitor 自己消费它解析数据目录）。
/// 正常启动路径这些变量本就不存在 → no-op 零回归。
///
/// ⚠ 勿把上面"子会话不注册 pidfile"泛化：CC 2.1.x 的 daemon **后台任务**
/// (--fork-session) 会写 pidfile（kind:"bg" + jobId）——那类由 session_map /
/// 远端 daemon 的 kind 交互性过滤处理（Batch6-F21），与本处嵌套环境清洗无关。
/// 完整排查：doc/DEVELOPMENT.md 常见问题节。
///
/// 返回实际清掉的 key（供 caller 在 logging 就绪后留痕——本函数必须在任何线程
/// spawn 之前调用，那时 logging 还没初始化、不能直接打 log）。
fn scrub_env_vars(keys: &[&str]) -> Vec<String> {
    let mut removed = Vec::new();
    for &k in keys {
        if std::env::var_os(k).is_some() {
            std::env::remove_var(k);
            removed.push(k.to_string());
        }
    }
    removed
}

// F-MA:CC 嵌套会话 env 清单移到 adapter/claude_code.rs（走 adapter.nested_env_to_scrub()）。

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Batch7-F23A：nudge skip 判定的纯函数对（单测钦定，见 MASTERPLAN §6）。
///
/// `pack_nudge_state`：终态物理尺寸 + fullscreen 位打包成一个可比较状态值。
/// fullscreen 占 bit 63（F11 borderless 全屏与 maximize 在"自动隐藏任务栏"下
/// inner 尺寸可能相同——状态位保证这类 #4095 高危过渡不被 skip）；宽度截 31 位
/// （物理像素远小于 2^31，不损失信息）。
pub(crate) fn pack_nudge_state(w: u32, h: u32, fullscreen: bool) -> u64 {
    ((fullscreen as u64) << 63) | (((w as u64) & 0x7FFF_FFFF) << 32) | h as u64
}

/// skip 当且仅当：曾经 nudge 过（last != 0）且 (尺寸+全屏态) 与上次执行完的
/// nudge 完全一致——典型即"最小化→恢复"。0 是安全哨兵：真实窗口尺寸非零，
/// pack 结果不可能为 0（0×0 在事件入口与 settle 双重滤除）。
pub(crate) fn nudge_should_skip(last_nudged: u64, packed: u64) -> bool {
    last_nudged != 0 && last_nudged == packed
}

pub fn run() {
    // 启动 perf 测量起点
    let t0 = std::time::Instant::now();

    // issue #24：第一件事就是清嵌套标记——必须在任何线程 spawn 之前
    // （std::env::remove_var 修改进程级环境，单线程窗口内调用才稳妥；
    // 下面 logging::init 就会起 non_blocking writer 线程）。
    let scrubbed_env = scrub_env_vars(adapter::active().nested_env_to_scrub());

    // v2.0.0 (issue #4)：tracing 初始化提前到 Builder 之前 —— 一旦 init 全局
    // dispatcher 锁死，且我们要捕获 setup() 期间的所有 log。
    //
    // logging 模块内部把所有复杂度（rolling file appender / non_blocking writer /
    // ErrorEmitterLayer / EnvFilter reload）封死，对外只暴露 init + state。
    //
    // **monitor_data_dir 必须能解析**：这里用 dirs::home_dir 兜底，不依赖任何
    // 配置（避免 log 初始化跟 config 初始化循环依赖）。
    let monitor_data_dir = paths::resolve_monitor_data_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("cc-monitor-fallback"));
    let logging_state = logging::init(&monitor_data_dir);
    tracing::info!(
        "[perf] T+{}ms cc-monitor starting (data_dir={}, log_dir={})",
        t0.elapsed().as_millis(),
        monitor_data_dir.display(),
        logging_state.log_dir().display()
    );
    if !scrubbed_env.is_empty() {
        // issue #24：留痕——宿主 shell 带嵌套标记（从 claude 会话内启动的）。
        // 没这行，"清洗是否真的发生过"无法事后验证。
        tracing::info!(
            "scrubbed inherited claude nested-session env markers: {}",
            scrubbed_env.join(", ")
        );
    }

    // setup 闭包是 FnOnce + 'static，必须 move-capture。把 logging_state shadow 进闭包，
    // 闭包内同时 install_error_emitter（&self 借用）+ app.manage(clone)
    let logging_state = logging_state;

    // issue #9：single-instance lock。**必须是第一个 plugin**（Tauri 官方 plugin 要求）。
    // 第二个 cc-monitor 实例启动 → 触发本回调（在第一个实例里跑）→ 把主窗口
    // unminimize + show + set_focus → 第二个实例立即退出（plugin 内部处理）。
    // 详 doc/INVARIANTS.md § 16。
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    #[cfg(windows)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // 第二个实例若带 --background（cc auto-launch 竞态下偶发）→ 只 show 不抢焦点；
            // 普通双击拉起第二个实例则照常置前（用户显式想看）。
            let background = args.iter().any(|a| a == "--background");
            tracing::info!("second cc-monitor instance detected (background={background})");
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.unminimize();
                let _ = win.show();
                if !background {
                    let _ = win.set_focus();
                }
            }
        }));

        // WebView2 maximize / 全屏后内容错位修复（v2.14.0 引入，F12 加固重写）。
        // 根因是 WebView2 Runtime 内部（浏览器进程）在 maximize / restore / 全屏切换后
        // 丢失/挂起对宿主 bounds 更新的处理（WebView2Feedback #4095 族，微软未修）：
        // 宿主侧 put_Bounds 成功、容器 HWND 已是全尺寸，但合成层（"Intermediate D3D
        // Window"）停在旧尺寸 → 内容不铺满、周围留白。DOM 之下，前端 reflow 够不着。
        //
        // v2.14 的手段（±1px webview.set_size 抖动）机制上生效但对 Runtime 内部 bug
        // 不可靠（1px 差值可能被 Runtime 合并/丢弃），F12 升级为 controller 级三板斧
        // （with_webview 闭包内直接 COM 调用）：
        //   1. 双 rect SetBounds（h-1 → h）：让 Runtime 看到「变化后的 rect」重新 put_Bounds
        //   2. NotifyParentWindowPositionChanged：微软文档明示的宿主位置变化通知
        //   3. SetIsVisible(false→true) 翻转：强制重建/重挂合成 visual，对 #4095 族最有效；
        //      仅 maximize/fullscreen 时做（普通拖拽 resize 不翻，避免理论上的闪烁）
        //
        // ⚠ 最小化守卫（F12，修"restore 后数秒点不了"）：tao 0.35 在 WM_SIZE(SIZE_MINIMIZED)
        // 时发 Resized(0,0)（不过滤），而 wry 自己的 subclass 明确跳过 SIZE_MINIMIZED——
        // 最小化时把 controller bounds 打成 0×0 会让 renderer 视口归零、进入挂起态，
        // restore 后画面先回、输入 hit-test 层数秒才重建。入口按 0×0 早退 + 线程动作前
        // 二次守卫（去抖 60ms 期间可能又被最小化），对齐 wry 的保护语义。
        //
        // 去抖：resize 期间（含拖拽）每个事件 bump 一个 generation；后台线程等到连续 60ms
        // 没有新事件（= 过渡稳定）再动手。nudge_pending 保证一个突发 resize 只有一个
        // 去抖线程在飞。with_webview 的闭包由 tauri 派发到主线程执行——闭包内只做 COM
        // 调用、禁止 sleep（同一闭包内连续两次不同 rect 已满足重钉条件，v2.14 的 16ms
        // 间隔不再需要）。
        builder = builder.on_window_event({
            use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
            use std::sync::Arc;
            use std::time::Duration;
            let resize_gen = Arc::new(AtomicU64::new(0));
            let nudge_pending = Arc::new(AtomicBool::new(false));
            // Batch7-F23A：上次 nudge **闭包执行完毕**时的 (尺寸+全屏态) 打包值
            // （pack_nudge_state；0=从未）。最小化→恢复回到同状态时合成层没有错位
            // 理由（#4095 是 resize/maximize **过渡** bug），却会因 is_maximized()
            // 为 true 走 SetIsVisible 翻转 → 拆挂合成 visual 瞬间露白底（用户实测
            // 白闪）。同状态直接 skip 全部 COM 动作。两条取舍（审计 D 复核后留档）：
            // ① store 在 with_webview 闭包尾执行——"执行完"= 闭包跑完，单个 COM
            //   调用失败仍记录（COM 级失败不重试；派发失败才不记录）；
            // ② 拖拽一圈回到原尺寸的 settle 也会被 skip（终态==上次已修复态，
            //   wry 自身的 WM_SIZE 路径已实时跟踪中间态，残余风险接受）。
            // F11 全屏与 maximize 同 inner 尺寸的角例由打包值里的 fullscreen 位
            // 区分（状态变了照跑三板斧）。
            let last_nudged = Arc::new(AtomicU64::new(0));
            move |window, event| {
                let size = match event {
                    tauri::WindowEvent::Resized(s) => *s,
                    _ => return,
                };
                // 最小化：绝不动 webview bounds（见块头 ⚠），也不 bump gen
                if size.width == 0 && size.height == 0 {
                    return;
                }
                resize_gen.fetch_add(1, Ordering::SeqCst);
                // 已有一个去抖线程在飞 → 它会读到新的 gen 自行续等，不再 spawn
                if nudge_pending.swap(true, Ordering::SeqCst) {
                    return;
                }
                let window = window.clone();
                let resize_gen = resize_gen.clone();
                let nudge_pending = nudge_pending.clone();
                let last_nudged = last_nudged.clone();
                std::thread::spawn(move || {
                    let mut last = resize_gen.load(Ordering::SeqCst);
                    loop {
                        std::thread::sleep(Duration::from_millis(60));
                        let now = resize_gen.load(Ordering::SeqCst);
                        if now == last {
                            break;
                        }
                        last = now;
                    }
                    nudge_pending.store(false, Ordering::SeqCst);
                    // 二次守卫：去抖期间窗口可能又被最小化 / 尺寸归零
                    if window.is_minimized().unwrap_or(false) {
                        tracing::info!("nudge skip: window minimized during debounce");
                        return;
                    }
                    let target = match window.inner_size() {
                        Ok(t) if t.width > 0 && t.height > 0 => t,
                        _ => {
                            tracing::info!("nudge skip: zero/unknown inner_size");
                            return;
                        }
                    };
                    let maximized = window.is_maximized().unwrap_or(false);
                    let fullscreen = window.is_fullscreen().unwrap_or(false);
                    let flip = maximized || fullscreen;
                    // Batch7-F23A：同(尺寸+全屏态) skip（典型 = 最小化恢复）。
                    // 判定与打包是纯函数（单测见 nudge_skip_tests）。
                    let packed =
                        pack_nudge_state(target.width, target.height, fullscreen);
                    if nudge_should_skip(last_nudged.load(Ordering::SeqCst), packed) {
                        tracing::info!(
                            "nudge skip: size+state unchanged {}x{} fs={fullscreen} (restore-from-minimize path)",
                            target.width,
                            target.height
                        );
                        return;
                    }
                    let Some(webview) = window.webviews().into_iter().next() else {
                        tracing::warn!("nudge skip: no webview on window");
                        return;
                    };
                    tracing::info!(
                        "nudge settle: target={}x{} maximized={maximized} fullscreen={fullscreen} flip={flip}",
                        target.width,
                        target.height
                    );
                    let last_nudged_in = last_nudged.clone();
                    let res = webview.with_webview(move |pw| {
                        // RECT 必须来自 webview2-com 0.38 配对的 windows 0.61
                        // （windows-wv2 rename，见 Cargo.toml），0.56 的类型不互通
                        use windows_wv2::Win32::Foundation::RECT;
                        let controller = pw.controller();
                        let full = RECT {
                            left: 0,
                            top: 0,
                            right: target.width as i32,
                            bottom: target.height as i32,
                        };
                        let shrunk = RECT {
                            bottom: target.height.saturating_sub(1) as i32,
                            ..full
                        };
                        // 每个 COM 调用的失败单独 warn（不再静默）：理论上存在不对称失败
                        // ——如 SetIsVisible(false) 成功而 (true) 失败会让 webview 停在隐藏态，
                        // 无日志就无从取证。失败不中断后续调用（终态尽量推向可见+正确 bounds）。
                        unsafe {
                            if let Err(e) = controller.SetBounds(shrunk) {
                                tracing::warn!("nudge SetBounds(shrunk) failed: {e}");
                            }
                            if let Err(e) = controller.SetBounds(full) {
                                tracing::warn!("nudge SetBounds(full) failed: {e}");
                            }
                            if let Err(e) = controller.NotifyParentWindowPositionChanged() {
                                tracing::warn!("nudge NotifyParentWindowPositionChanged failed: {e}");
                            }
                            if flip {
                                if let Err(e) = controller.SetIsVisible(false) {
                                    tracing::warn!("nudge SetIsVisible(false) failed: {e}");
                                }
                                if let Err(e) = controller.SetIsVisible(true) {
                                    tracing::warn!("nudge SetIsVisible(true) failed: {e}");
                                }
                            }
                        }
                        // 闭包执行完毕才记录（with_webview 的 Ok 只代表"已派发到主
                        // 线程"——审计 D 修订：在这里 store 才是"执行完"的语义）
                        last_nudged_in.store(packed, Ordering::SeqCst);
                    });
                    if let Err(e) = res {
                        tracing::warn!("nudge with_webview failed: {e}");
                    }
                });
            }
        });
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            // Debug build 自动开 DevTools(CCM_NO_DEVTOOLS=1 抑制——远程实测/E2E 时省半屏)
            #[cfg(debug_assertions)]
            if std::env::var("CCM_NO_DEVTOOLS").is_err() {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            // 窗口 config `focus=false` → 创建时不激活、不抢前台（cc 集成 auto-launch 带
            // `--background` 启动时正好不打断当前终端）。但**手动**启动（双击 exe，无该参数）
            // 仍应置前，这里补一次 set_focus 还原默认体验。
            if !std::env::args().any(|a| a == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
            }

            // F-MA：agent 数据目录 + 会话源布局都走活跃适配器（Claude Code = 第一个实例；
            // data_root 仍是三级回退 用户配置 → CLAUDE_CONFIG_DIR → ~/.claude）。子目录名不再硬编码。
            let agent = adapter::active();
            let claude_dir = agent.data_root().ok_or("agent data dir not found")?;
            tracing::info!("monitor using agent [{}] data dir: {}", agent.id(), claude_dir.display());
            let projects_dir = adapter::records_dir(&claude_dir);
            let sessions_dir = adapter::liveness_dir(&claude_dir);
            // v2.3.0 issue #11：任务追踪文件根（CC = tasks）
            let tasks_dir =
                adapter::tasks_dir(&claude_dir).unwrap_or_else(|| claude_dir.join("tasks"));

            // monitor 自己的数据目录：~/.claude/claudecode-frontend
            let monitor_data_dir = paths::resolve_monitor_data_dir().ok_or("no data dir")?;
            tracing::info!("monitor_data_dir: {}", monitor_data_dir.display());

            // v2.0.0：把 AppHandle 注入给 ErrorEmitterLayer（之前一直是 None，
            // setup 期间的 ERROR 只写 log；从这里开始 ERROR 才会弹前端 toast）
            logging_state.install_error_emitter(app.handle().clone());

            // v1.7.1：把当前 exe 路径记到 auto-launch.json，让 cc function 能在用户启用
            // auto-launch 时主动启动 monitor（不硬编码安装路径）
            auto_launch::update_monitor_path_on_startup(&monitor_data_dir);

            // v1.7：BindRegistry 监听 ps-await/ → EnumWindows → 写 ps-registry/。
            // SidHwndCache 持久化 sid → 拉前所需信息（含复合指纹）。
            let bind_registry = bind::BindRegistry::spawn(monitor_data_dir.clone());
            let sid_hwnd_cache =
                bind::SidHwndCache::load(monitor_data_dir.join("sid-hwnd-cache.json"));

            // Feature ②（远端 Tab ↗ 拉前）：纯内存的 sid → hwnd 缓存。远端 session
            // 加入时扫本地窗口找 `ccm-rbind-<sid>` 标题（wrapper 在远端设的 OSC 标题，
            // 经 ssh 透传到本地 Windows Terminal）并绑定。bring_remote_terminal_to_front
            // IPC 取 State<Arc<RemoteHwndCache>>（INVARIANT § 8：必须 manage，见下方）。
            let remote_hwnd_cache = bind::RemoteHwndCache::new();

            // SessionMap = Claude Code 自己维护的 ~/.claude/sessions/<PID>.json
            let (session_map, session_changes) =
                session_map::SessionMap::load_with_changes(
                    sessions_dir,
                    load_show_bg_sessions(),
                );

            // Watcher: 只对活跃 session 的 jsonl emit
            let active_filter: watcher::ActiveFilter = {
                let map = session_map.clone();
                Arc::new(move |sid: &str| map.is_session_active(sid))
            };

            // v2.4 (修首次启动乱序)：watcher 直接调 on_line 回调（取代之前的 mpsc
            // 中间层）。回调内同步 parse + record() —— history buffer 在 watcher
            // 线程内同步落盘，初始全量扫完成 = history 完整 = frontend-ready 触发
            // replay 时 snapshot 一定完整。
            //
            // 旧设计：watcher tx → mpsc → tauri::async_runtime::spawn drain → record。
            // async drain 跟 frontend-ready 是竞态：drain 没追上时 snapshot 不完整，
            // 部分历史漏到 live emit 路径 → 跟 chunked replay 错位 → 首次启动乱序。
            // F5 因 backend 已稳定看不到 bug。详 watcher.rs::spawn_watcher 注释。
            let replay = Arc::new(event_replay::EventReplay::new());
            // v2.4.2 issue #2: watcher 改成一次 process_file 给一批 lines，
            // lib.rs 这里 parse 整批后一次 on_line_batch 给 EventReplay。
            // EventReplay 按 batch 大小分流（详 event_replay::on_line_batch 注释）。
            let on_batch: watcher::BatchHandler = {
                let replay = replay.clone();
                let handle = app.handle().clone();
                Arc::new(move |lines: Vec<watcher::JsonlLine>| {
                    // 本地行无 origin（None）；远端行由 ssh_source 传 Some(host)。
                    let payloads = batch_to_payloads(lines, None);
                    replay.on_line_batch(&handle, payloads);
                })
            };

            // 本地 jsonl-watcher：**始终** spawn（与 SSH-remote 引入前完全一致）。
            // 远端（如启用）是纯附加数据源（见下方 load_remote_configs 块），不影响这里。
            let watcher_handle = watcher::spawn_watcher(projects_dir, active_filter, on_batch);
            let force_rescan_tx = watcher_handle.force_rescan_tx;
            let initial_scan_done = watcher_handle.initial_scan_done;

            // session 集合变化 emitter（本地）：
            //   - added：通知 jsonl-watcher 主动重扫该 session（修 Bug 2-A 竞态）
            //             + 调 SidHwndCache.record 把 sid → hwnd 绑定持久化
            //   - removed：透传 session-ended 给前端，Tab 灰显归档
            //              + 调 SidHwndCache.forget 清理过期 sid
            {
                let handle = app.handle().clone();
                let session_map_for_emitter = session_map.clone();
                let bind_for_emitter = bind_registry.clone();
                let cache_for_emitter = sid_hwnd_cache.clone();
                let spawned = std::thread::Builder::new()
                    .name("session-changes-emitter".into())
                    .spawn(move || {
                        while let Ok(change) = session_changes.recv() {
                            for sid in &change.added {
                                tracing::info!("session added: {sid}, triggering jsonl rescan");
                                if let Err(e) = force_rescan_tx.send(sid.clone()) {
                                    tracing::warn!("force_rescan send failed for {sid}: {e}");
                                }
                                // 尝试绑定 sid → hwnd（通过 claude_pid 的 parent PS）
                                if let Some(info) = session_map_for_emitter.lookup(sid) {
                                    let _ =
                                        cache_for_emitter.record(sid, info.pid, &bind_for_emitter);
                                }
                                // 会话（重新）变活 → 通知前端复活已归档的本地 Tab（resume：
                                // 崩溃→灰显→/resume 后免 F5 即回 live）。**仅在 PID 真探活通过
                                // 时发**：崩溃残留的旧 sessions/<PID>.json 被后续文件事件重扫也
                                // 会进 added（心跳已从 by_id 删过它），无 liveness 门会误复活刚
                                // 归档的死会话。is_session_active 读 by_id（此刻 = 本次重扫的
                                // next）并 re-probe 进程，门住该竞态。session-ended 的对称补全。
                                if session_map_for_emitter.is_session_active(sid) {
                                    // Batch7-F24：带 pidfile 元信息——前端无 Tab 时建
                                    // 骨架（中途出现的 bg 会话需要 kind 才有 ⚙/树状）。
                                    let info = session_map_for_emitter.lookup(sid);
                                    let payload = bridge::SessionStartedPayload {
                                        session_id: sid.clone(),
                                        cwd: info.as_ref().map(|i| i.cwd.clone()),
                                        kind: info.as_ref().and_then(|i| i.kind.clone()),
                                        name: info.as_ref().and_then(|i| i.name.clone()),
                                    };
                                    if let Err(e) =
                                        handle.emit(bridge::events::SESSION_STARTED, &payload)
                                    {
                                        tracing::warn!("emit session-started failed: {e}");
                                    } else {
                                        tracing::info!("session started (revive): {sid}");
                                    }
                                }
                            }
                            // S0：本地路径没有 idle-tmux 灰点（`SESSION_IDLE` 是远端专有），
                            // cause 在这里无分支意义，取 sid 即可。
                            for removed in change.removed {
                                let sid = removed.sid;
                                cache_for_emitter.forget(&sid);
                                let payload = bridge::SessionEndedPayload {
                                    session_id: sid.clone(),
                                };
                                if let Err(e) = handle.emit(bridge::events::SESSION_ENDED, &payload)
                                {
                                    tracing::warn!("emit session-ended failed: {e}");
                                } else {
                                    tracing::info!("session ended: {sid}");
                                }
                            }
                            // issue #23：红绿灯——status/waitingFor 变了才会出现在这里
                            // （session_map 重扫时逐会话比对），透传给前端改灯色。
                            for act in change.status_changed {
                                let payload = bridge::SessionActivityPayload {
                                    session_id: act.session_id,
                                    status: act.status,
                                    waiting_for: act.waiting_for,
                                };
                                if let Err(e) =
                                    handle.emit(bridge::events::SESSION_ACTIVITY, &payload)
                                {
                                    tracing::warn!("emit session-activity failed: {e}");
                                }
                            }
                        }
                    });
                if let Err(e) = spawned {
                    tracing::error!(
                        "failed to spawn session-changes-emitter thread: {e}; \
                         session 增减事件将丢失，Tab 不会自动归档 / 新会话可能丢首屏"
                    );
                }
            }

            // issue #20：远端当前活跃 sid 集 —— session_map 的远端对应物，专供
            // frontend-ready 重放后对账（远端 sid 不在 session_map，#19 的本地对账
            // 覆盖不到）。唯一写者是下面的 remote-session-emitter（daemon 的
            // added/removed 与断连 flush 走同一 remote_tx 通道，集合恒等于"前端当前
            // 应视为 live 的远端 sid"）。无远端配置时恒空，对账自然 no-op。
            // 违反此约束见 doc/INVARIANTS.md § 24。
            let remote_active: Arc<parking_lot::Mutex<std::collections::HashSet<String>>> =
                Arc::new(parking_lot::Mutex::new(std::collections::HashSet::new()));

            // SSH-remote Phase 0 (issue #15)：远端是**纯附加**数据源。config.json 的
            // `remote.enabled = true` 且配置完整 → 在本地 watcher 之外**额外**起一条
            // ssh_source::run（aggregate：本地 + 远端 session 同时显示）。否则（默认 /
            // 无 remote 配置）此块不执行，本地路径与历史 bit-for-bit 一致。
            let remote_cfgs = load_remote_configs();
            if !remote_cfgs.is_empty() {
                tracing::info!(
                    "remote mode ENABLED (additive): {} SSH data source(s) (local jsonl-watcher still running)",
                    remote_cfgs.len()
                );

                // 远端独立 session 通道：ssh_source::run 持 sender；这里起一条**专用**的
                // 精简 emitter drain 它。
                //   - added（Feature ②）：远端 Tab 由 line 帧经 ensureTab 创建（不调
                //     force_rescan_tx，那是本地 jsonl 专用）。但要扫本地窗口找
                //     `ccm-rbind-<sid>` 标题绑定 hwnd，供 ↗ 拉前。wrapper 设标题经 ssh
                //     透传到本地 WT 有延迟（OSC 序列要等远端 shell 起来 + 透传），故
                //     +1500/3000/4500/6000ms 重试扫描，首次绑定成功即停。
                //   - removed：emit session-ended（远端 Tab 归档）+ forget 远端绑定。
                let (remote_tx, remote_rx) =
                    std::sync::mpsc::channel::<session_map::SessionChange>();
                {
                    let handle = app.handle().clone();
                    let remote_cache_for_emitter = remote_hwnd_cache.clone();
                    let remote_active_for_emitter = remote_active.clone();
                    let spawned = std::thread::Builder::new()
                        .name("remote-session-emitter".into())
                        .spawn(move || {
                            while let Ok(change) = remote_rx.recv() {
                                // issue #20：先维护远端活跃集（再做 emit/扫描等副作用）。
                                // 断连 flush 的 removed 也从这里清掉 → 断线期间集合为空，
                                // 与前端"全部已归档"的视图一致。
                                {
                                    let mut active = remote_active_for_emitter.lock();
                                    for sid in &change.added {
                                        active.insert(sid.clone());
                                    }
                                    for removed in &change.removed {
                                        active.remove(&removed.sid);
                                    }
                                }
                                // added 先处理：每个新 sid 起一条**独立** std::thread
                                // 做带 sleep 的重试扫描。
                                //
                                // 为何用 std::thread 而非 tauri::async_runtime::spawn：扫描
                                // 本体是同步 Win32（find_window_by_marker_substr，
                                // INVARIANT § 10 要求 Win32 同步调用不能压在 IPC/async
                                // 派发线程上），无任何 .await；用专用 std::thread + sleep
                                // 最简单且与 async runtime 是否就绪完全解耦（本块身处 std::thread
                                // 里，调 async_runtime::spawn 虽也可行但平添对全局 runtime 的
                                // 隐性依赖，无收益）。线程扫完即退，不长驻。
                                for sid in change.added {
                                    // audit-fixes F03.2：会话（重新）变活 → 清 idle 灰灯标记（resume/新会话）。
                                    ssh_source::clear_idle(&sid);
                                    let cache = remote_cache_for_emitter.clone();
                                    let spawn_res = std::thread::Builder::new()
                                        .name("remote-bind-scan".into())
                                        .spawn(move || {
                                            // 每 ~0.6s 扫一次、最多 ~9s，命中即停。比固定 4 次更稳健：
                                            // claude 启动时也会设标题（实测 ~once），wrapper 每 0.3s
                                            // 重刷整个 ~9s 窗口；多次扫描覆盖该窗口，大幅降低"恰好每次
                                            // 扫描都撞上 claude 标题而漏绑"的概率（EnumWindows 廉价，命中即停）。
                                            for _ in 0u32..15 {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(600),
                                                );
                                                if cache.try_bind(&sid) {
                                                    tracing::info!(
                                                        "remote bind: sid={sid} → hwnd bound"
                                                    );
                                                    break;
                                                }
                                            }
                                        });
                                    if let Err(e) = spawn_res {
                                        tracing::warn!(
                                            "failed to spawn remote-bind-scan thread: {e}; 远端 Tab ↗ 拉前将不可用"
                                        );
                                    }
                                }
                                for removed in change.removed {
                                    let sid = removed.sid;
                                    // audit-fixes F03.2（灰灯三态分流）：daemon-removed（claude 进程没了，权威）
                                    // 到达时，看该 sid 的 `@ccm_sid` 是否仍出现在某 origin 的 TmuxSessions 帧里：
                                    //   - Some(origin)=tmux 会话尚在（空 shell）→ **idle-tmux 灰灯**：mark_idle +
                                    //     emit SESSION_IDLE + **不 forget 绑定**（登录 shell 的 ssh 窗仍活、↗ 拉前有效）。
                                    //   - None=tmux 也没了 → **archived**（原逻辑）：clear_idle + forget + SESSION_ENDED。
                                    // 判据 command-agnostic（见 ssh_source::tmux_origin_for_sid）：帧 ≤8s 陈旧，退出
                                    // 瞬间 command 列可能仍是 claude，故用 daemon-removed 判"claude 死"、@ccm_sid
                                    // present 判"tmux 在"。**§24**：removed sid 已在上方从 remote_active 移出，idle 天然
                                    // 在集合外；idle 只写独立 REMOTE_IDLE（唯一写者=本 emitter），**不新增 remote_active 写点**。
                                    //
                                    // ★ S0：上面那段「查 @ccm_sid 还在不在」**只对 `Gone` 成立**。
                                    // `Superseded`（同 pidfile 原地换 sid，即 /branch）时旧 sid 的
                                    // tmux 格子确实还在、但已经改挂新 sid ⇒ 查快照必然误判成灰点，
                                    // 且那份快照在 P5 删掉 ticker 后没有任何事件路径会刷新它
                                    // ⇒ 永久灰点、按旧 sid 也 attach 不上（用户实测「杀不掉」）。
                                    // 故 cause 先于快照裁决，见 `classify_removed` 的文档注释。
                                    match ssh_source::classify_removed(
                                        ssh_source::find_tmux_origin_for_sid(&sid),
                                        removed.cause,
                                    ) {
                                        ssh_source::RemovedDisposition::Idle { origin } => {
                                            ssh_source::mark_idle(&origin, &sid);
                                            let payload = bridge::SessionIdlePayload {
                                                session_id: sid.clone(),
                                            };
                                            if let Err(e) =
                                                handle.emit(bridge::events::SESSION_IDLE, &payload)
                                            {
                                                tracing::warn!("emit remote session-idle failed: {e}");
                                            } else {
                                                tracing::info!("remote session idle-tmux: {sid}");
                                            }
                                        }
                                        ssh_source::RemovedDisposition::Archive => {
                                            ssh_source::clear_idle(&sid);
                                            remote_cache_for_emitter.forget(&sid);
                                            let payload = bridge::SessionEndedPayload {
                                                session_id: sid.clone(),
                                            };
                                            if let Err(e) =
                                                handle.emit(bridge::events::SESSION_ENDED, &payload)
                                            {
                                                tracing::warn!("emit remote session-ended failed: {e}");
                                            } else {
                                                tracing::info!("remote session ended: {sid}");
                                            }
                                        }
                                    }
                                }
                                // Batch9-F27：远端红绿灯——daemon session_status 帧/
                                // 宣告初始值经 status_changed 透传（与本地 emitter
                                // 同形状，前端 sid-keyed 零改动）。
                                for act in change.status_changed {
                                    let payload = bridge::SessionActivityPayload {
                                        session_id: act.session_id,
                                        status: act.status,
                                        waiting_for: act.waiting_for,
                                    };
                                    if let Err(e) =
                                        handle.emit(bridge::events::SESSION_ACTIVITY, &payload)
                                    {
                                        tracing::warn!("emit remote session-activity failed: {e}");
                                    }
                                }
                            }
                        });
                    if let Err(e) = spawned {
                        tracing::error!(
                            "failed to spawn remote-session-emitter thread: {e}; 远端 Tab 不会自动归档"
                        );
                    }
                }

                // 每台远端各起一条 ssh_source::run（多机 #30），与本地 watcher 走相同出口
                // （batch_to_payloads → on_line_batch）；session 变化共享 remote_tx → 上面那
                // 唯一的 remote-session-emitter（session 变化 host 无关，按 sid 维护）。
                // `connected` 是 connection-healthy signal（每台一份）：stream_loop 收到 daemon
                // hello 时置 true，run() 的重连循环据此判定本次是否连上过（连上过→下次立即快速
                // 重连，否则指数退避）。远端**不**门控 frontend-ready（本地 watcher 的
                // initial_scan_done 才门控 replay；远端是实时流，无"初始扫完成"概念）。
                for cfg in remote_cfgs {
                    tracing::info!(
                        "  remote host [{}]: {}@{}:{}",
                        cfg.origin_label(),
                        cfg.user,
                        cfg.host,
                        cfg.port
                    );
                    let replay_for_ssh = replay.clone();
                    let app_for_ssh = app.handle().clone();
                    let tx_for_ssh = remote_tx.clone();
                    let connected = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    tauri::async_runtime::spawn(async move {
                        let label = cfg.origin_label();
                        if let Err(e) =
                            ssh_source::run(cfg, replay_for_ssh, app_for_ssh, tx_for_ssh, connected)
                                .await
                        {
                            // S8/S9 会把"connection dropped"做成显眼的前端提示；先大声 log。
                            tracing::error!("ssh_source::run [{label}] exited: {e}");
                        }
                    });
                }
                // audit-fixes F03.2：tmux 存活对账**从 8s poller 改为收帧驱动**（甲-evented，零轮询）——
                // 收割器现落在 `ssh_source::stream_loop` 的 `TmuxSessions` 帧臂（daemon 每 ~8s 推帧即算），
                // 复用 `tmux_reconcile::reconcile_step`。故此处不再 spawn poller（`run_tmux_reconcile_poller` 已删）。
            }

            // 焦点同步功能已移除：Windows 11 默认 WT 是单进程多窗口架构，
            // GetForegroundWindow 永远返回 WT 主进程 PID，OS 无法区分 tab/window。
            // 旧 focus.rs / lookup_by_foreground_pid / focus-switch IPC 都已删。
            // Tab 切换走手动点击或 Ctrl+Tab 快捷键。

            // v2.3.0 issue #11：监听 task 文件变更，per-session 重读后 emit 给前端。
            // 不依赖 SessionMap，独立 watcher。tasks_dir 不存在时函数内部 no-op。
            tasks::spawn_task_watcher(tasks_dir.clone(), app.handle().clone());

            // issue #6：历史全文搜索索引。后台线程扫 projects/**/*.jsonl 建内存索引。
            // 延迟 1.5s 启动 —— 让首屏 replay 先跑完，不抢磁盘 / CPU；索引就绪前
            // search_history 返回 status="indexing"，前端显示"索引中"。
            let search_index = Arc::new(search::SearchIndex::new());
            {
                let idx = search_index.clone();
                let claude_dir_for_index = claude_dir.clone();
                let spawned = std::thread::Builder::new()
                    .name("search-index-build".into())
                    .spawn(move || {
                        idx.build_blocking(
                            &claude_dir_for_index,
                            std::time::Duration::from_millis(1500),
                        );
                    });
                if let Err(e) = spawned {
                    tracing::error!("failed to spawn search-index-build thread: {e}; 全文搜索不可用");
                }
            }

            // 前端 ready 事件 → 等 watcher 初始扫完成 → replay all。
            //
            // v2.4 修首次启动乱序：之前 listener 直接调 replay()，但 watcher 是
            // 异步全量扫，snapshot 时 history 不完整 → 部分历史漏到 live emit 路径
            // → 跟 chunked replay 错位。现在 listener 在 async task 里 spin-wait
            // `initial_scan_done`，扫完才 snapshot，保证 chunked replay 包含全部历史。
            //
            // 等待用 10ms 间隔 poll，整体 timeout 10s（防 watcher 死锁卡死整个 UI
            // 永远看不到内容）。timeout 到也会强行 replay，degraded but unblocked。
            {
                let replay = replay.clone();
                let handle = app.handle().clone();
                let initial_scan_done = initial_scan_done.clone();
                let session_map = session_map.clone();
                let remote_active = remote_active.clone();
                let t0_capture = t0;
                app.listen(bridge::events::FRONTEND_READY, move |event| {
                    // Batch5-F19：payload 携带用户上次所在 tab（localStorage 记忆），
                    // replay 按 session 分组、该 tab 的块先发。缺省/解析失败 → None
                    // （行为同 F19 前；viewer 等旧调用方不带 payload 也安全）。
                    // 契约定义在 bridge.rs（单一来源，G 验收纠偏）。
                    let priority_sid =
                        serde_json::from_str::<bridge::FrontendReadyPayload>(event.payload())
                            .ok()
                            .and_then(|p| p.priority_sid);
                    let replay = replay.clone();
                    let handle = handle.clone();
                    let initial_scan_done = initial_scan_done.clone();
                    let session_map = session_map.clone();
                    let remote_active = remote_active.clone();
                    let listen_recv_at = t0_capture.elapsed().as_millis();
                    tauri::async_runtime::spawn(async move {
                        tracing::info!(
                            "[perf] T+{}ms frontend-ready received, waiting for watcher initial scan",
                            listen_recv_at
                        );
                        let wait_started = std::time::Instant::now();
                        const WAIT_TIMEOUT: std::time::Duration =
                            std::time::Duration::from_secs(10);
                        while !initial_scan_done
                            .load(std::sync::atomic::Ordering::Acquire)
                        {
                            if wait_started.elapsed() > WAIT_TIMEOUT {
                                tracing::warn!(
                                    "watcher initial scan timed out after 10s; replay with partial history"
                                );
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        }
                        tracing::info!(
                            "[perf] T+{}ms watcher initial scan done (+{}ms wait), starting replay",
                            t0_capture.elapsed().as_millis(),
                            wait_started.elapsed().as_millis()
                        );
                        // Batch9-F28：replay 之前先重发全部已宣告远端会话（骨架+
                        // 初始灯）——remote-session-added 不进 replay buffer，F5 后
                        // 无行骨架/bg ⚙ 元数据/远端 lastActive 焦点全靠这次重发
                        // （Batch5 I-1 缺口）。宣告先于行的契约由这里的顺序保证。
                        ssh_source::reannounce_all(&handle);
                        replay
                            .replay_and_mark_ready(&handle, priority_sid.as_deref())
                            .await;

                        // issue #19：前端是纯事件增量模型——Tab 见行即建 live，只有一次性的
                        // session-ended 能归档。F5/HMR 重载后 replay 把 buffer 里**已结束**
                        // 会话的行也重放成 live Tab，而归档信号不在 buffer、不会重发 → 僵尸
                        // live Tab（还因 closeTab 门控 archived 而关不掉）。这里按当前活跃集
                        // 对账：对已不活跃的**本地** sid 补发 session-ended，复用前端
                        // archiveTab（幂等）。本段仅本地：session_map 只认本地，远端 sid
                        // 不在其中（远端对账见紧随其后的 issue #20 块）。
                        let stale: Vec<String> = replay
                            .buffered_local_session_ids()
                            .into_iter()
                            .filter(|sid| !session_map.is_session_active(sid))
                            .collect();
                        // issue #20：#19 的远端版。远端 sid 不在 session_map，活跃集由
                        // remote-session-emitter 维护（daemon added/removed + 断连 flush
                        // 同一通道）。断连窗口期 F5 会把其实还活着的远端会话一并归档——
                        // 重连后 daemon 重发 session-added + 重放行，前端 un-archive
                        // （tabs.ts ensureTab，仅远端）复活，自愈闭环。
                        //
                        // ⚠ 配套前提：前端把 session-ended 与行事件**同序**处理（events.ts
                        // 的 queue，#20 一并改）。否则这里补发的 ended 会抢在积压重放行
                        // 之前执行，归档随即被后续远端行 un-archive 翻回 live，补发等于无效。
                        // audit-fixes F03.2：idle-tmux sid 不在 remote_active（变 idle 时已移出），若不排除
                        // 会被当"死"补 SESSION_ENDED、F5 后灰灯塌成 archived。故排除 idle sid + 下面重发 SESSION_IDLE。
                        let idle_all: std::collections::HashSet<String> =
                            ssh_source::snapshot_idle_by_origin()
                                .into_values()
                                .flatten()
                                .collect();
                        let remote_stale: Vec<String> = {
                            let active = remote_active.lock();
                            replay
                                .buffered_remote_session_ids()
                                .into_iter()
                                .filter(|sid| !active.contains(sid) && !idle_all.contains(sid))
                                .collect()
                        };
                        // F03.2：F5 后把 idle sid 的灰灯盖回（行重放会把其 tab 建成 live，这次重发再变灰）。
                        for sid in &idle_all {
                            let _ = handle.emit(
                                bridge::events::SESSION_IDLE,
                                &bridge::SessionIdlePayload {
                                    session_id: sid.clone(),
                                },
                            );
                        }
                        for sid in stale.iter().chain(remote_stale.iter()) {
                            if let Err(e) = handle.emit(
                                bridge::events::SESSION_ENDED,
                                &bridge::SessionEndedPayload {
                                    session_id: sid.clone(),
                                },
                            ) {
                                tracing::warn!(
                                    "reconcile emit session-ended failed for {sid}: {e}"
                                );
                            }
                        }
                        if !stale.is_empty() || !remote_stale.is_empty() {
                            tracing::info!(
                                "replay 对账：补发 session-ended 归档已结束 Tab（本地 {} 个 + 远端 {} 个）",
                                stale.len(),
                                remote_stale.len()
                            );
                        }
                    });
                });
            }

            // 给 Tauri 命令暴露 state。
            //
            // **v1.7.4 修回归**：v1.6.7 撤 bring_terminal_to_front 时把
            // `app.manage(session_map.clone())` 也删了，但 history 命令
            // （list_history_projects / list_history_sessions_in_project）也接
            // `State<Arc<SessionMap>>`，导致历史浏览器打不开，报"state not managed
            // for field `map`"。这里补回去。
            app.manage(session_map.clone());
            app.manage(replay.clone());
            app.manage(bind_registry.clone());
            app.manage(sid_hwnd_cache.clone());
            // Feature ②：远端 sid → hwnd 缓存。bring_remote_terminal_to_front 取此 State。
            app.manage(remote_hwnd_cache.clone());
            // v2.0.0 (issue #4)：logging state 也要 manage，IPC handler 才能拿到
            app.manage(logging_state.clone());
            // issue #6：全文搜索索引 State（search_history / rebuild / status IPC 用）
            app.manage(search_index.clone());

            tracing::info!(
                "[perf] T+{}ms setup() completed (watchers spawned, state managed)",
                t0.elapsed().as_millis()
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::load_config,
            config::save_config,
            // F87(#50+#51): MCP 管理——读跨 scope 展示 / 写只项目 .mcp.json（SS-14）
            // B03 批一：cc-bus 驾驶舱（只读，按需 SSH cat，无轮询）
            cc_bus::read_cc_bus_state,
            cc_bus::check_cc_bus_agent_online,
            cc_bus::read_cc_bus_inbox,
            cc_bus::cc_bus_send,
            cc_bus::cc_bus_spawn,
            // B04：钩子只读诊断（本机 + 远端）。**没有任何写命令**——用户定调不改 settings.json
            config_surface::config_surface_report,
            drift_ledger::drift_ledger_report,
            backend::control::daemon_launch::daemon_send_into,
            backend::control::launch_wire::render_ccm_launch,
            backend::control::launch_wire::render_launch_payload,
            hooks_diag::diagnose_local_cc_bus_hooks,
            hooks_diag::diagnose_remote_cc_bus_hooks,
            mcp::read_mcp_servers,
            mcp::read_remote_mcp_servers,
            mcp::list_remote_mcp_origins,
            mcp::list_remote_mcp_project_dirs,
            mcp::read_remote_project_mcp,
            mcp::write_remote_mcp_server,
            mcp::remove_remote_mcp_server,
            mcp::list_mcp_project_dirs,
            mcp::write_project_mcp_server,
            mcp::remove_project_mcp_server,
            subagent::load_subagent,
            forget_session,
            // issue #10: 独立只读窗口（多窗口 / 双屏）
            open_session_in_new_window,
            replay_session_to_window,
            // F82a(#56+#47): 设置独立窗口
            open_settings_window,
            bring_terminal_to_front,
            // Feature ②: 远端 Tab ↗ 拉前对应本地终端窗口（ccm wrapper 设标题绑定）
            bring_remote_terminal_to_front,
            // issue #23: 红绿灯快照（启动/F5 初始收敛；增量走 session-activity 事件）
            list_session_activity,
            list_active_sessions,
            // v2.4 issue #2: 用户在终端输入时可选拉前 monitor 自身
            bring_monitor_to_front,
            cc_integration_status,
            cc_integration_preview,
            cc_integration_scan_path,
            cc_integration_install,
            cc_integration_uninstall,
            cc_get_auto_launch,
            cc_set_auto_launch,
            // v2.0.0 (issue #4): 诊断 / log
            frontend_perf_log,
            get_diagnostics_config,
            set_diagnostics_config,
            get_log_file_info,
            open_log_file,
            open_log_dir,
            history::list_history_projects,
            history::stream_history_sessions_in_project,
            history::stream_read_session_jsonl,
            remote_branch::create_remote_branch_session, // G6
            remote_history::list_remote_history_projects,
            remote_history::stream_remote_history_sessions,
            remote_history::stream_read_remote_session,
            // F11：远端历史删除（SFTP 写，SS-G 用户数据写豁免）
            remote_history::delete_remote_history_session,
            // F10：一键装 / 卸远端 ccm 助手到 ~/.bashrc（SFTP 写 profile，SS-H）
            sftp::install_remote_ccm_helper,
            sftp::uninstall_remote_ccm_helper,
            // F08c：手动安装 / 卸载远端 daemon（SFTP 写 ~/.cc-monitor/bin，SS-G 部署写豁免）
            sftp::deploy_remote_daemon,
            sftp::uninstall_remote_daemon,
            acct_iso_deploy::deploy_remote_acct_iso,
            acct_iso_deploy::check_remote_acct_iso,
            acct_iso_deploy::remote_acct_iso_shellinit,
            history::delete_history_session,
            history::create_branch_session,
            history::update_history_metadata,
            history::list_last_accounts,
            history::resume_history_session,
            history::new_local_session,
            usage::aggregate_usage_all,
            remote_history::aggregate_remote_usage_all, // F88a-remote：远端 daemon 用量 fan-out
            // A2：多账号只读查询（账号=一个 CLAUDE_CONFIG_DIR）。旧 daemon/daemonless
            // 台一律回 available:false，前端降级隐藏账号功能而不是弹错。
            accounts::list_remote_accounts,
            local_accounts::list_local_accounts,
            local_accounts::list_local_session_accounts, // E79：本机版「某会话属于哪个账号」
            accounts::list_remote_session_accounts,
            accounts::check_account_trust,
            launch::launch_remote_terminal,
            sftp_pool::sftp_realpath,
            sftp_pool::sftp_list_dir,
            sftp_pool::sftp_stat,
            sftp_pool::sftp_download,
            sftp_pool::sftp_upload,
            sftp_pool::sftp_cancel_transfer,
            sftp_pool::sftp_mkdir,
            sftp_pool::sftp_rename,
            sftp_pool::sftp_delete,
            sftp_pool::sftp_read_text_for_edit,
            sftp_pool::sftp_write_text,
            pubkey::push_public_key,
            tmux::list_remote_tmux,
            tmux::capture_remote_pane,
            tmux::kill_remote_tmux,
            tmux::tmux_send_keys,
            account_usage::account_usage,
            ccm_probe::probe_ccm_cli,
            // Batch15-P1：code-picture 代码全景后端命令族（per-repo Engine 池,只读查询）
            panorama::panorama_index,
            panorama::panorama_reindex,
            panorama::panorama_status,
            panorama::panorama_overview,
            panorama::panorama_node,
            panorama::panorama_subgraph,
            panorama::panorama_callers,
            panorama::panorama_callees,
            panorama::panorama_impact,
            panorama::panorama_search,
            panorama::panorama_docs_for,
            panorama::panorama_touching,
            panorama::panorama_symbols_in_file,
            panorama::panorama_drift,
            panorama::panorama_add_annotation,
            panorama::panorama_propose_annotation,
            panorama::panorama_approve_annotation,
            panorama::panorama_remove_annotation,
            panorama::panorama_list_annotations,
            panorama::panorama_write_doc_link,
            panorama::panorama_remove_doc_link,
            port_forward::start_forward,
            port_forward::stop_forward,
            port_forward::list_forwards,
            // issue #6: 历史全文搜索
            search::search_history,
            search::get_search_index_status,
            search::rebuild_search_index,
            // v2.3.0 issue #11: task 面板初次拉
            tasks::get_session_tasks,
            // v2.3.0 issue #3 (A 透明化): 设置面板「数据」区列出所有持久路径
            data_paths::get_data_paths,
            // issue #15 Tier 1: SSH 连接 UX —— ~/.ssh/config 导入 + 测试连接 + 指纹固化
            ssh_source::list_ssh_host_aliases,
            ssh_source::resolve_ssh_host,
            ssh_source::import_ssh_hosts,
            ssh_source::test_remote_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn extract_cwd(rec: &messages::JsonlRecord) -> Option<String> {
    match rec {
        messages::JsonlRecord::User { cwd, .. } => cwd.clone(),
        _ => None,
    }
}

/// Batch7-F24：读 config.json 顶层 `showBgSessions`（默认 true）。**OnceLock 缓存
/// 首读**——本地 scan 过滤与远端 exec 参数（含每次重连）拿到同一个值，双端统一
/// "重启生效"语义（审计 D：不缓存则远端在重连时活切换、与本地/文案不一致）。
pub(crate) fn load_show_bg_sessions() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        config::load_config()
            .ok()
            .and_then(|v| v.get("showBgSessions").and_then(|b| b.as_bool()))
            .unwrap_or(true)
    })
}

/// SSH-remote（issue #15 / 多机 #30）：从 monitor 的 config.json 读 `remote` 段，构造
/// **0..N 个** [`ssh_source::RemoteConfig`]。**空 Vec = 本地模式**（与历史 bit-for-bit
/// 一致）：config.json 不存在 / 解析失败 / 无 `remote` 键 / `enabled != true` / 无任何
/// 合法 host → 空 Vec。
///
/// config.rs 是 schema-agnostic（只透传 serde_json::Value），所以这里直接读
/// `paths::resolve_config_path()` 的文件，自己取 `remote` 子对象。读法对齐
/// `paths.rs::read_user_override`（同一个 config.json，同样的 best-effort 容错）。
///
/// remote 段 schema（S6/S7 的设置 UI 负责写）：
/// ```json
/// "remote": {
///   "enabled": true,
///   "hosts": [
///     { "label": "pi", "host": "raspberrypi.local", "port": 22, "user": "pi",
///       "keyPath": "C:\\Users\\me\\.ssh\\id_ed25519",
///       "daemonPath": "/home/pi/cc-monitor-remote",
///       "hostKeyFingerprint": "SHA256:..." }
///   ]
/// }
/// ```
/// **向后兼容**：旧单对象形态 `"remote": { "enabled": true, "host": …, … }`（无 `hosts`
/// 键）归一成 1 元素列表（`label` 默认 = host）。每台缺必填字段(host/user/daemonPath)
/// 则跳过 + warn；`label` 重复则后缀化 ` (#2)`（保证 by-label 选台 key 唯一）。
pub(crate) fn load_remote_configs() -> Vec<ssh_source::RemoteConfig> {
    let Some(cfg_path) = paths::resolve_config_path() else {
        return Vec::new();
    };
    if !cfg_path.exists() {
        return Vec::new();
    }
    let Ok(raw) = std::fs::read_to_string(&cfg_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Vec::new();
    };
    let Some(remote) = value.get("remote").and_then(|v| v.as_object()) else {
        return Vec::new();
    };

    // 全局 enabled 门控：未显式 true → 关闭（默认本地）。
    if remote.get("enabled").and_then(|v| v.as_bool()) != Some(true) {
        return Vec::new();
    }

    parse_remote_hosts(remote)
}

/// 把 `remote` 对象解析成 host 列表（抽出供单测直接喂 JSON 对象）。优先读 `hosts`
/// 数组；无 `hosts` 但有 `host`（旧单对象）→ 当 1 台。重复 label 后缀化去重。
fn parse_remote_hosts(
    remote: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ssh_source::RemoteConfig> {
    let host_objs: Vec<&serde_json::Map<String, serde_json::Value>> =
        match remote.get("hosts").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_object()).collect(),
            None => vec![remote], // 向后兼容：旧单对象
        };

    let mut out: Vec<ssh_source::RemoteConfig> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for obj in host_objs {
        let Some(mut cfg) = parse_host_obj(obj) else {
            continue; // parse_host_obj 已 warn
        };
        // label 去重：重复则后缀化 " (#2)"、" (#3)"…，保证 by_label 选台唯一。
        if !seen.insert(cfg.label.clone()) {
            let base = cfg.label.clone();
            let mut n = 2u32;
            let unique = loop {
                let cand = format!("{base} (#{n})");
                if seen.insert(cand.clone()) {
                    break cand;
                }
                n += 1;
            };
            tracing::warn!("remote label 重复，'{base}' 改为 '{unique}'");
            cfg.label = unique;
        }
        out.push(cfg);
    }
    out
}

/// 解析单个 host JSON 对象 → RemoteConfig；缺必填字段(host/user/daemonPath) → None+warn。
fn parse_host_obj(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<ssh_source::RemoteConfig> {
    let str_field = |k: &str| {
        obj.get(k)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    };

    let (host, user, daemon_path) = match (
        str_field("host"),
        str_field("user"),
        str_field("daemonPath"),
    ) {
        (Some(h), Some(u), Some(d)) => (h.to_string(), u.to_string(), d.to_string()),
        _ => {
            tracing::warn!("remote host 缺必填字段(host/user/daemonPath)，跳过该台");
            return None;
        }
    };

    let label = str_field("label")
        .map(str::to_string)
        .unwrap_or_else(|| host.clone());
    let port = obj
        .get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(22);
    let key_path = str_field("keyPath").map(str::to_string);
    let host_key_fingerprint = str_field("hostKeyFingerprint").map(str::to_string);
    // Batch14-F56：跳板 label（指向另一台已配置主机的 origin_label）。
    let jump = str_field("jump").map(str::to_string);
    // Batch14-F59：daemonless 降级读取开关（per-host，缺省 false = 走 daemon 流路径）。
    let daemonless = obj
        .get("daemonless")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // Batch14-F45：备用地址。前端下发数组（addresses: string[]）；也容忍换行文本（历史/手填）。
    let addresses: Vec<String> = match obj.get("addresses") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    };

    Some(ssh_source::RemoteConfig {
        label,
        host,
        port,
        user,
        key_path,
        daemon_path,
        host_key_fingerprint,
        addresses,
        jump,
        daemonless,
    })
}

/// 按 label 选台（`remote_history` 的历史查询据此选连哪台）。无匹配 → None。
pub(crate) fn load_remote_config_by_label(label: &str) -> Option<ssh_source::RemoteConfig> {
    load_remote_configs()
        .into_iter()
        .find(|c| c.origin_label() == label)
}

/// 把 watcher 读出的一批 `JsonlLine` parse 成可 emit 的 `JsonlLinePayload`。
///
/// v2.4.2 issue #2 抽出的最小 seam：watcher 回调和后续（SSH-remote）数据源都调
/// 这一个自由函数，保持 parse → is_displayable 过滤 → extract_cwd → 组 payload
/// 的行为唯一。过滤次序、解析错误 warn-then-continue、`seq` 透传都必须与历史一致。
///
/// `origin`：数据来源标签。`None` = 本地（前端 Tab 标题不加前缀，与历史一致）；
/// `Some(host)` = 远端（issue #15，前端 Tab 标题加 `[host]` 前缀以区分本地/远端）。
/// 透传到每条 payload，让前端按 sid 分流时知道该 Tab 是本地还是哪台远端主机。
pub(crate) fn batch_to_payloads(
    lines: Vec<watcher::JsonlLine>,
    origin: Option<String>,
) -> Vec<bridge::JsonlLinePayload> {
    let mut payloads = Vec::with_capacity(lines.len());
    for line in lines {
        match parser::parse_line(&line.raw) {
            Ok(Some(record)) if record.is_displayable() => {
                let cwd = extract_cwd(&record);
                payloads.push(bridge::JsonlLinePayload {
                    session_id: line.session_id.clone(),
                    cwd,
                    path: line.path.to_string_lossy().into_owned(),
                    // P5.1：watcher 给每行单调编号；前端按 seq 排到 timeline
                    seq: line.seq,
                    origin: origin.clone(),
                    message: record,
                });
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("parse line failed in {}: {e}", line.path.display());
            }
        }
    }
    payloads
}

/// 前端关闭 archived Tab 时调用：从 event_replay 历史里抹掉这个 session，
/// 防止下次 F5 刷新它原地复活。
#[tauri::command]
fn forget_session(
    session_id: String,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), String> {
    replay.forget(&session_id);
    Ok(())
}

/// issue #10：把某 session 在一个独立 WebviewWindow（`viewer-<sid>`）里打开，
/// 加载 `index.html?viewer=<sid>` —— 前端检测到 `viewer` 参数走精简只读 bootstrap。
/// 窗口已存在则前置聚焦（不重复开）。双屏 / 并排查看用。
///
/// **必须 `async`**：Tauri 2 同步 `fn` 命令在**主线程**执行，而
/// `WebviewWindowBuilder::build()` 要把窗口创建派发到主线程并阻塞等待 —— 同步命令
/// 就是在主线程里等主线程 → 死锁（表现：新窗口白屏 + 整个 app 卡死连 X 都点不了）。
/// async 命令跑在 async runtime（非主线程）→ build() 派发给空闲主线程 → 正常建窗。
///
/// 拖拽撕离（tear-off）：`x` / `y` 为可选的**逻辑屏幕坐标**（CSS px），来自前端
/// mouseup 的 `e.screenX/screenY`。两者皆 `Some` 时新窗口在该落点打开（双屏拖出体验）；
/// 任一为 `None`（右键菜单 / Ctrl+Shift+N 老调用方）则维持默认居中行为，不破坏旧路径。
#[tauri::command]
async fn open_session_in_new_window(
    app: tauri::AppHandle,
    session_id: String,
    title: String,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    use tauri::Manager;
    let label = format!("viewer-{session_id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let url = tauri::WebviewUrl::App(format!("index.html?viewer={session_id}").into());
    let mut builder = tauri::WebviewWindowBuilder::new(&app, &label, url)
        .title(if title.is_empty() {
            "cc-monitor"
        } else {
            &title
        })
        .inner_size(900.0, 720.0)
        // Batch7-F23B：与主窗口 backgroundColor 一致——合成间隙露底为主题深色
        // 而非 WebView2 默认白（tauri.conf.json 主窗口同款 #2b2a27）
        .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff));
    // 落点定位：仅当 x/y 都给出时按逻辑坐标摆放（Tauri 2 builder 取 LogicalPosition）。
    if let (Some(x), Some(y)) = (x, y) {
        builder = builder.position(x, y);
    }
    builder
        .build()
        .map_err(|e| format!("create viewer window failed: {e}"))?;
    Ok(())
}

/// F82a（#56+#47）：把「设置」开进独立窗口（SS-3 终态：设置搬独立窗）。单例 `settings` 窗，
/// 已存在则前置聚焦。**必须 `async`**（同 `open_session_in_new_window`：同步命令建窗死锁，见其
/// doc + `viewer-window-investigation.md` 五坑之一）。设置窗加载 `?settings=1` → 前端 `bootstrapSettings`
/// 精简挂载 SettingsPanel（windowMode）。设置项经既有 config 命令读写（窗口无关），无需 replay/事件流；
/// 保存时前端广播 `settings-applied`，主窗口 listen 后重读并应用主题/行为（跨窗同步）。
#[tauri::command]
async fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let label = "settings";
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let url = tauri::WebviewUrl::App("index.html?settings=1".into());
    tauri::WebviewWindowBuilder::new(&app, label, url)
        .title("cc-monitor 设置")
        .inner_size(760.0, 820.0)
        // 与主窗口 backgroundColor 一致，合成间隙露底为主题深色而非 WebView2 默认白（同 viewer）
        .background_color(tauri::window::Color(0x2b, 0x2a, 0x27, 0xff))
        .build()
        .map_err(|e| format!("create settings window failed: {e}"))?;
    Ok(())
}

/// issue #10：独立 viewer 窗口加载后调用 —— 后端把该 sid 的历史定向 emit 给本窗口。
/// `window` 由 Tauri 注入 = 发起调用的窗口（即那个 viewer 窗口）。
#[tauri::command]
fn replay_session_to_window(
    session_id: String,
    window: tauri::WebviewWindow,
    replay: tauri::State<'_, Arc<event_replay::EventReplay>>,
) -> Result<(), String> {
    replay.replay_session_to_window(&window, &session_id);
    Ok(())
}

/// v2.4 (issue #2)：把 monitor 自己的主窗口拉到最前 + unminimize + 抢焦点。
///
/// 用途：用户在终端敲键时，前端 user-active 信号路径下，若用户开了「拉前
/// monitor 窗口」toggle 就 invoke 这个 IPC 让 monitor 主动浮上来。
///
/// **核心问题**：用户敲终端时前台是 PS/WT，**monitor 不是前台进程** →
/// `SetForegroundWindow` 直接调被 OS 拒绝（只闪任务栏图标）。这是 Windows
/// 对前台抢焦的设计限制（防恶意软件偷焦点）。
///
/// **解法 = AttachThreadInput hack**：临时把当前线程附加到前台线程的输入
/// 队列，OS 把它俩视作"同输入上下文" → 借用前台线程的拉前权限 →
/// SetForegroundWindow 通过 → 立刻 detach。广泛使用的可靠 hack
/// （Visual Studio / 各 IDE 都用），不被 OS 视为恶意。
///
/// v2.4.0 直接用 win.set_focus()（内部就是 SetForegroundWindow）必败，
/// v2.4.1 hotfix 改这版。
///
/// Tauri 内部用 windows crate 0.61（HWND.0 = *mut c_void），我们 0.56
/// （HWND.0 = isize）；用 `as isize` cast 跨版本兼容。
#[cfg(windows)]
#[tauri::command]
async fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOMOVE,
        SWP_NOSIZE, SW_RESTORE, SW_SHOW,
    };

    tracing::info!("bring_monitor_to_front: invoked");

    // HWND 必须在 webview 所属线程里取（Tauri 内部约束），随即 cast 成
    // isize 跨线程，INVARIANTS § 19 跨 windows crate 版本约定。
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let tauri_hwnd = win.hwnd().map_err(|e| format!("hwnd: {e}"))?;
    let hwnd_value = tauri_hwnd.0 as isize;

    // INVARIANTS § 10：Win32 同步调用必须 spawn_blocking，否则慢路径会让 Tauri
    // IPC 派发线程排队（v2.4 起 autoFollowUserActive 高频触发该 IPC）。
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        unsafe {
            let h = HWND(hwnd_value);
            tracing::info!("bring_monitor_to_front: monitor hwnd = {:#x}", hwnd_value);

            // === 三层 hack 突破 Win10/11 前台抢焦限制 ===
            // 详 ARCHITECTURE.md § 5「bring_monitor_to_front 三层 hack」。
            // attach/detach + Alt down/up 同闭包内必须配对，整段在同一 blocking
            // 线程内串行，安全。

            // 1. ShowWindow 先做：可能 minimize 状态
            if IsIconic(h).as_bool() {
                tracing::info!("bring_monitor_to_front: window iconic, SW_RESTORE");
                let _ = ShowWindow(h, SW_RESTORE);
            } else {
                let _ = ShowWindow(h, SW_SHOW);
            }

            // 2. 模拟 Alt 按键（down 阶段，up 在末尾）
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_EXTENDEDKEY, 0);

            // 3. AttachThreadInput
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, None);
            let cur_thread = GetCurrentThreadId();
            tracing::info!(
                "bring_monitor_to_front: fg_hwnd={:#x} fg_thread={} cur_thread={}",
                fg.0,
                fg_thread,
                cur_thread
            );
            let attached = fg_thread != 0
                && fg_thread != cur_thread
                && AttachThreadInput(fg_thread, cur_thread, true).as_bool();

            // 4. TOPMOST 强制 Z 序拉顶 + BringWindowToTop
            let _ = SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            let _ = BringWindowToTop(h);

            // 5. SetForegroundWindow 真正抢焦
            let ok = SetForegroundWindow(h).as_bool();
            tracing::info!(
                "bring_monitor_to_front: attached={} SetForegroundWindow={}",
                attached,
                ok
            );

            // 6. detach + Alt 释放
            if attached {
                let _ = AttachThreadInput(fg_thread, cur_thread, false);
            }
            keybd_event(
                VK_MENU.0 as u8,
                0,
                KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP,
                0,
            );

            if ok {
                Ok(())
            } else {
                // 拉前真失败也 Z 序已被推顶，视觉上窗口浮起来了
                // （只是焦点没抢到）。给前端 warn 但不视为 fatal。
                tracing::warn!("bring_monitor_to_front: SetForegroundWindow rejected (window Z-order raised but no focus)");
                Err("SetForegroundWindow rejected (window raised but not focused)".into())
            }
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[cfg(not(windows))]
#[tauri::command]
async fn bring_monitor_to_front(app: tauri::AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let _ = win.unminimize();
    let _ = win.show();
    win.set_focus().map_err(|e| format!("set_focus: {e}"))
}

/// issue #23：当前全部本地活跃会话的红绿灯快照。前端启动/F5 后调一次做初始收敛
/// （session-activity 是稀疏事件、不进 replay buffer，刷新会丢——同 get_session_tasks
/// 的「快照 + 事件增量」双路收敛模式）。纯内存读（RwLock clone），无需 spawn_blocking。
#[tauri::command]
fn list_session_activity(
    map: tauri::State<'_, Arc<session_map::SessionMap>>,
) -> Vec<bridge::SessionActivityPayload> {
    map.snapshot_activity()
        .into_iter()
        .map(|a| bridge::SessionActivityPayload {
            session_id: a.session_id,
            status: a.status,
            waiting_for: a.waiting_for,
        })
        .collect()
}

/// Batch5-F18：本地活跃会话清单（sid + cwd）——前端启动时（frontend-ready 之前）
/// 调一次，先建全部骨架 Tab。纯内存读（RwLock clone），无需 spawn_blocking。
#[tauri::command]
fn list_active_sessions(
    map: tauri::State<'_, Arc<session_map::SessionMap>>,
) -> Vec<bridge::ActiveSessionPayload> {
    map.snapshot_active()
        .into_iter()
        .map(|e| bridge::ActiveSessionPayload {
            session_id: e.session_id,
            cwd: e.cwd,
            kind: e.kind,
            name: e.name,
        })
        .collect()
}

/// v1.7：拉对应终端窗口。
///
/// 流程：sid → 查 SidHwndCache → 校验复合指纹（IsWindow + owner_pid + procStart）
/// → activate_window。
///
/// **必须 async + spawn_blocking** 隔离 Win32 sync 调用（v1.6.5 的教训）。
#[tauri::command]
async fn bring_terminal_to_front(
    session_id: String,
    cache: tauri::State<'_, Arc<bind::SidHwndCache>>,
) -> Result<(), String> {
    let cache = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        let binding = cache.lookup(&session_id).ok_or_else(|| {
            format!(
                "session {session_id} 未绑定窗口。用 cc 命令启动 claude 才能拉前；\
                 cc 集成的安装见设置面板。"
            )
        })?;
        bind::verify_binding(&binding)?;
        bind::activate(binding.hwnd)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Feature ②：拉对应**远端** Tab 的本地终端窗口（远端会话需在远端启用 ccm wrapper，
/// 由它设 `ccm-rbind-<sid>` 窗口标题让 monitor 绑定本地 HWND）。
///
/// 流程：sid → 查 RemoteHwndCache（纯内存，session_added 时扫窗口绑定）→ 校验复合
/// 指纹（verify_binding：IsWindow + owner_pid + procStart）→ activate。镜像
/// `bring_terminal_to_front`，但走远端缓存。**必须 async + spawn_blocking** 隔离
/// Win32 sync 调用（INVARIANT § 10）。
#[tauri::command]
async fn bring_remote_terminal_to_front(
    session_id: String,
    cache: tauri::State<'_, Arc<bind::RemoteHwndCache>>,
) -> Result<(), String> {
    let cache = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        // 先查缓存；没命中就**点击时现扫一次**（marker 还挂在窗口标题上就能即时绑）。
        // 覆盖：① eager 扫描时机错过；② 用户 /resume 切到别的 sid——wrapper 会把
        // marker 重刷成当前 sid，这里现扫即可绑上。try_bind 是同步 Win32，已在
        // spawn_blocking 里（INVARIANT § 10）。
        let mut binding = match cache.lookup(&session_id) {
            Some(b) => b,
            None => {
                cache.try_bind_with_retry(
                    &session_id,
                    bind::ON_DEMAND_BIND_ATTEMPTS,
                    bind::ON_DEMAND_BIND_STEP_MS,
                );
                cache
                    .lookup(&session_id)
                    .ok_or_else(|| "未绑定窗口（远端会话需在远端启用 ccm wrapper）".to_string())?
            }
        };
        // 缓存命中但校验失败（终端已关 / HWND 易主）→ forget + 现扫重绑一次再试。
        // 场景：关掉终端后重新 ssh + tmux attach——新终端标题仍带 marker（tmux 会话级
        // set-titles 持久，attach 时重推 #T），死缓存不失效重扫的话 ↗ 就永远失灵。
        if bind::verify_binding(&binding).is_err() {
            cache.forget(&session_id);
            // #41(残):verify-fail 重绑路原是**单发** try_bind——F75 只给上面 cache-miss 路加了重试,
            // 这条(重新 attach、旧绑定失效)漏了。镜像兄弟路用 try_bind_with_retry,覆盖"刚 attach、
            // 新窗口 ccm-rbind 标题还没四跳传过来"的窗口期(否则重绑单扫落空 → 弹"未扫到新窗口")。
            cache.try_bind_with_retry(
                &session_id,
                bind::ON_DEMAND_BIND_ATTEMPTS,
                bind::ON_DEMAND_BIND_STEP_MS,
            );
            binding = cache.lookup(&session_id).ok_or_else(|| {
                "原绑定终端已关闭，且未扫到新的 ccm-rbind 窗口（请确认已重新 attach 且终端标题带 marker）"
                    .to_string()
            })?;
            bind::verify_binding(&binding)?;
        }
        bind::activate(binding.hwnd)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// === v1.7：PowerShell profile cc 集成 IPC ===

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
struct CcStatusResponse {
    profiles: Vec<profile_installer::ProfileScan>,
    active_registrations: u32,
    default_command_name: &'static str,
    /// v1.7.0-1.7.1 错把 cc 块装到 profile.ps1（CurrentUserAllHosts，PS 不自动加载）
    /// 的遗留文件列表。v1.7.2 起改装到 Microsoft.PowerShell_profile.ps1（默认 $PROFILE）。
    /// UI 检测到非空时显示警告，引导用户清理。
    legacy_profile_paths_with_block: Vec<LegacyProfileEntry>,
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
struct LegacyProfileEntry {
    kind: profile_installer::ProfileKind,
    path: String,
}

/// Batch13-F40:前端 perf 仪表落盘。webview 无 devtools(生产/CCM_NO_DEVTOOLS)时
/// console 取证不能,前端把启动管线 timeline/建卡计数经此写进 monitor 日志。
#[tauri::command]
fn frontend_perf_log(lines: String) {
    for line in lines.lines().take(40) {
        // 行数 + 单行长度双封顶(任意前端字符串进日志,防日志膨胀)
        let capped: String = line.chars().take(2000).collect();
        tracing::info!(target: "fe_perf", "{capped}");
    }
}

/// 扫描两个 PS profile + 报告当前活跃注册数。前端打开设置面板时调用。
#[tauri::command]
async fn cc_integration_status(
    command_name: Option<String>,
    bind_state: tauri::State<'_, Arc<bind::BindRegistry>>,
) -> Result<CcStatusResponse, String> {
    let cmd = command_name.unwrap_or_else(|| "cc".to_string());
    let bind_state = bind_state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let profiles: Vec<_> = profile_installer::discover_profiles()
            .into_iter()
            .map(|(kind, path)| profile_installer::scan_profile(kind, &path, &cmd))
            .collect();
        let legacy = profile_installer::scan_legacy_profiles()
            .into_iter()
            .map(|(kind, path)| LegacyProfileEntry { kind, path })
            .collect();
        Ok(CcStatusResponse {
            profiles,
            active_registrations: bind_state.registration_count() as u32,
            default_command_name: "cc",
            legacy_profile_paths_with_block: legacy,
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
struct CcPreviewResponse {
    code: String,
}

/// 返回将要写入 profile 的代码（含 BEGIN/END marker）。前端预览 modal 显示。
///
/// `include_cc_function` 控制是否生成完整 cc function（true）还是只装 helper（false）。
#[tauri::command]
fn cc_integration_preview(
    command_name: String,
    include_cc_function: bool,
) -> Result<CcPreviewResponse, String> {
    Ok(CcPreviewResponse {
        code: profile_installer::render_cc_code(&command_name, include_cc_function),
    })
}

/// 安装 cc function 到指定 path（前端自己组装路径——版本下拉 + 可编辑覆盖）。
/// idempotent；已有 ccm 块则原地替换。
///
/// `include_cc_function = false` 时只装 `__ccm_bind` helper，避免覆盖用户已有的 cc。
#[tauri::command]
async fn cc_integration_install(
    path: String,
    command_name: String,
    include_cc_function: bool,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        profile_installer::install_to_profile(&p, &command_name, include_cc_function)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 扫单个 path 的安装状态（前端用户改了路径后调）。
#[tauri::command]
async fn cc_integration_scan_path(
    path: String,
    command_name: String,
) -> Result<profile_installer::ProfileScan, String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        Ok(profile_installer::scan_path(&p, &command_name))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 读 auto-launch.json：UI 显示当前 toggle 状态 + 记录的 exe 路径。
#[tauri::command]
fn cc_get_auto_launch() -> Result<auto_launch::AutoLaunchConfig, String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    Ok(auto_launch::get_config(&dir))
}

/// UI toggle 改变时调：写 auto_launch_enabled。
#[tauri::command]
fn cc_set_auto_launch(enabled: bool) -> Result<(), String> {
    let dir = auto_launch::data_dir().ok_or("no data dir")?;
    auto_launch::set_enabled(&dir, enabled)
}

/// 卸载 cc function（删除 BEGIN/END 块；用户其他内容不动）。
#[tauri::command]
async fn cc_integration_uninstall(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = PathBuf::from(path);
        profile_installer::uninstall_from_profile(&p)
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

// ===== v2.0.0 (issue #4): 诊断 / log IPC =====

/// 读当前 diagnostics 配置。设置面板打开时调一次。
#[tauri::command]
fn get_diagnostics_config(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::DiagnosticsConfig, String> {
    Ok(state.config())
}

/// 应用新 diagnostics 配置。日志级别 + error_toast 立即生效；
/// log_enabled / max_files 改了返回 `NeedsRestart` 让前端提示用户重启。
#[tauri::command]
fn set_diagnostics_config(
    cfg: logging::DiagnosticsConfig,
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::RestartHint, String> {
    state.update_config(cfg)
}

/// 返回 log 目录 + 当前 log 文件 + 全部 .log 文件列表（path / size / mtime）。
/// 设置面板用来显示路径 + 文件大小，让用户一眼看到 log 状态。
#[tauri::command]
fn get_log_file_info(
    state: tauri::State<'_, Arc<logging::LoggingState>>,
) -> Result<logging::LogFileInfo, String> {
    Ok(state.log_file_info())
}

/// 用系统默认编辑器打开当前 log 文件（rolling::daily 写入的 mtime 最新那个）。
/// 失败常见原因：log_enabled=false 还没生成过 log 文件 → Err 让前端 alert 提示。
#[tauri::command]
async fn open_log_file(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
    let path = state
        .current_log_file()
        .ok_or_else(|| "没有 log 文件（log 文件未启用或还没产生）".to_string())?;
    let path_str = path.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&path_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 用资源管理器打开 log 目录。
#[tauri::command]
async fn open_log_dir(state: tauri::State<'_, Arc<logging::LoggingState>>) -> Result<(), String> {
    let dir = state.log_dir();
    // 目录可能还不存在（log_enabled=false 时不创建）
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("create log dir: {e}"))?;
    }
    let dir_str = dir.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || open_with_os(&dir_str))
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// 跨平台调系统默认 opener。Windows 用 `cmd /C start ""` 兜 path 中的空格。
/// 复用 tauri-plugin-opener 也行（前端就是走它），但这里在 Rust 端直接调更直接。
fn open_with_os(path_or_dir: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path_or_dir])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("start failed: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path_or_dir)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path_or_dir)
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod nudge_skip_tests {
    use super::{nudge_should_skip, pack_nudge_state};

    #[test]
    fn same_size_same_state_skips() {
        let a = pack_nudge_state(2560, 1400, false);
        assert!(nudge_should_skip(a, pack_nudge_state(2560, 1400, false)));
    }

    #[test]
    fn first_nudge_never_skips() {
        assert!(!nudge_should_skip(0, pack_nudge_state(800, 600, false)));
    }

    #[test]
    fn size_change_runs() {
        let a = pack_nudge_state(2560, 1400, false);
        assert!(!nudge_should_skip(a, pack_nudge_state(2560, 1399, false)));
        assert!(!nudge_should_skip(a, pack_nudge_state(2559, 1400, false)));
    }

    #[test]
    fn fullscreen_bit_distinguishes_same_inner_size() {
        // F11 无边框全屏与 maximize 在自动隐藏任务栏下 inner 尺寸可能相同——
        // 全屏态翻转必须照跑三板斧（#4095 高危过渡）
        let maxed = pack_nudge_state(2560, 1440, false);
        let fs = pack_nudge_state(2560, 1440, true);
        assert_ne!(maxed, fs);
        assert!(!nudge_should_skip(maxed, fs));
        assert!(!nudge_should_skip(fs, maxed));
    }

    #[test]
    fn pack_no_collision_between_dimensions() {
        // w/h 位域独立：宽高互换、进位不串位
        assert_ne!(pack_nudge_state(1, 2, false), pack_nudge_state(2, 1, false));
        assert_ne!(
            pack_nudge_state(0x10000, 0, false),
            pack_nudge_state(0, 0x10000, false)
        );
    }
}

#[cfg(test)]
mod env_scrub_tests {
    use super::scrub_env_vars;

    /// issue #24：清掉存在的、跳过不存在的、不碰未列出的。
    /// 用本测试专属的假变量名——cargo test 多线程跑，进程级 env 是共享的，
    /// 绝不能在测试里 set/remove 真实的 CLAUDE_* 变量（会干扰并发测试与宿主环境）。
    #[test]
    fn removes_present_keeps_unlisted_skips_absent() {
        // 正常启动路径：变量全不存在 → 严格 no-op（"零回归"声明的直接对应物）
        assert!(scrub_env_vars(&["CCM_TEST_SCRUB_NOOP"]).is_empty());

        std::env::set_var("CCM_TEST_SCRUB_A", "1");
        std::env::set_var("CCM_TEST_SCRUB_KEEP", "keep");
        let removed = scrub_env_vars(&["CCM_TEST_SCRUB_A", "CCM_TEST_SCRUB_ABSENT"]);
        assert_eq!(removed, vec!["CCM_TEST_SCRUB_A".to_string()]);
        assert!(std::env::var_os("CCM_TEST_SCRUB_A").is_none());
        assert_eq!(
            std::env::var("CCM_TEST_SCRUB_KEEP").as_deref(),
            Ok("keep"),
            "未列出的变量必须原样保留（对应真实场景的 CLAUDE_CONFIG_DIR）"
        );
        std::env::remove_var("CCM_TEST_SCRUB_KEEP");
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use std::path::PathBuf;

    fn jline(session_id: &str, seq: u64, raw: &str) -> watcher::JsonlLine {
        watcher::JsonlLine {
            session_id: session_id.to_string(),
            path: PathBuf::from("/tmp/projects/proj/s-abc.jsonl"),
            seq,
            raw: raw.to_string(),
        }
    }

    /// batch_to_payloads 必须：只保留 displayable 行、透传 seq/session_id、
    /// 在遇到 malformed 行时 warn-then-continue 不 panic，并对非 displayable 行静默丢弃。
    #[test]
    fn filters_non_displayable_and_survives_malformed() {
        // (a) 真实 displayable user 行（copy 自 messages.rs golden sample），seq 5
        let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/me/proj"
        }"#;
        // (b) 非 displayable 记录：permission-mode 的 is_displayable() 返回 false
        let non_displayable = r#"{"type":"permission-mode"}"#;
        // (c) 无法解析的行 → parse_line 返回 Err，必须被 warn 后跳过、不 panic
        let malformed = "not json at all";

        let lines = vec![
            jline("s-abc", 5, displayable_user),
            jline("s-abc", 6, non_displayable),
            jline("s-abc", 7, malformed),
        ];

        let payloads = batch_to_payloads(lines, None);

        // 只有 displayable user 行进 payload
        assert_eq!(payloads.len(), 1, "只应保留 1 条 displayable 记录");
        assert_eq!(payloads[0].seq, 5, "seq 必须原样透传");
        assert_eq!(payloads[0].session_id, "s-abc", "session_id 必须原样透传");
        assert_eq!(
            payloads[0].cwd.as_deref(),
            Some("/home/me/proj"),
            "extract_cwd 应取出 user.cwd"
        );
        // 本地（origin=None）行不带 origin。
        assert_eq!(payloads[0].origin, None, "本地行 origin 应为 None");
    }

    /// origin=Some(host) 时每条 payload 都带上该标签（远端 Tab 标题前缀用）。
    #[test]
    fn origin_is_propagated_to_payloads() {
        let displayable_user = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"},
            "cwd":"/home/pi/proj"
        }"#;
        let lines = vec![jline("s-remote", 0, displayable_user)];
        let payloads = batch_to_payloads(lines, Some("pi".to_string()));
        assert_eq!(payloads.len(), 1);
        assert_eq!(
            payloads[0].origin.as_deref(),
            Some("pi"),
            "远端行 origin 必须透传 host 标签"
        );
    }
}

#[cfg(test)]
mod remote_config_tests {
    use super::parse_remote_hosts;
    use serde_json::json;

    fn remote_obj(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        v.as_object().expect("test json 必须是对象").clone()
    }

    /// 向后兼容：旧单对象（无 hosts 键）→ 1 台，label 默认 = host，port 默认 22。
    #[test]
    fn legacy_single_object_one_host() {
        let remote = remote_obj(json!({
            "host": "pi.local", "user": "pi", "daemonPath": "/x"
        }));
        let cfgs = parse_remote_hosts(&remote);
        assert_eq!(cfgs.len(), 1);
        assert_eq!(cfgs[0].host, "pi.local");
        assert_eq!(cfgs[0].label, "pi.local", "label 默认 = host");
        assert_eq!(cfgs[0].port, 22, "port 默认 22");
    }

    /// F56：jump 字段解析——有值 → Some;缺省/空串 → None（str_field 过滤空）。
    #[test]
    fn jump_field_parsed() {
        let remote = remote_obj(json!({
            "hosts": [
                {"host": "internal", "user": "u", "daemonPath": "/x", "jump": "bastion"},
                {"host": "direct", "user": "u", "daemonPath": "/y"},
                {"host": "empty", "user": "u", "daemonPath": "/z", "jump": ""}
            ]
        }));
        let cfgs = parse_remote_hosts(&remote);
        assert_eq!(cfgs.len(), 3);
        assert_eq!(cfgs[0].jump.as_deref(), Some("bastion"));
        assert_eq!(cfgs[1].jump, None, "缺省 → None");
        assert_eq!(cfgs[2].jump, None, "空串 → None");
    }

    /// hosts 数组多台；缺 label 的台 label 回退 host；port 透传。
    #[test]
    fn hosts_array_multi() {
        let remote = remote_obj(json!({
            "hosts": [
                {"label": "pi", "host": "pi.local", "user": "pi", "daemonPath": "/x"},
                {"host": "nano.local", "user": "u", "daemonPath": "/y", "port": 2222}
            ]
        }));
        let cfgs = parse_remote_hosts(&remote);
        assert_eq!(cfgs.len(), 2);
        assert_eq!(cfgs[0].label, "pi");
        assert_eq!(cfgs[1].label, "nano.local", "缺 label → 默认 host");
        assert_eq!(cfgs[1].port, 2222);
    }

    /// 缺必填字段(daemonPath)的台被跳过，不影响其他台。
    #[test]
    fn missing_required_field_skipped() {
        let remote = remote_obj(json!({
            "hosts": [
                {"host": "ok.local", "user": "u", "daemonPath": "/x"},
                {"host": "bad.local", "user": "u"}
            ]
        }));
        let cfgs = parse_remote_hosts(&remote);
        assert_eq!(cfgs.len(), 1);
        assert_eq!(cfgs[0].host, "ok.local");
    }

    /// label 重复 → 第二台后缀化，保证 by_label 选台唯一。
    #[test]
    fn duplicate_label_suffixed() {
        let remote = remote_obj(json!({
            "hosts": [
                {"label": "box", "host": "a", "user": "u", "daemonPath": "/x"},
                {"label": "box", "host": "b", "user": "u", "daemonPath": "/y"}
            ]
        }));
        let cfgs = parse_remote_hosts(&remote);
        assert_eq!(cfgs.len(), 2);
        assert_eq!(cfgs[0].label, "box");
        assert_eq!(cfgs[1].label, "box (#2)");
    }

    /// 空 hosts 数组 → 空（无可用远端，等同本地）。
    #[test]
    fn empty_hosts_array_is_empty() {
        let remote = remote_obj(json!({ "hosts": [] }));
        assert!(parse_remote_hosts(&remote).is_empty());
    }
}
