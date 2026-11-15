//! 后端二进制：全局分配器 ＋ 两形进程的分岔：托管进程那一形（`-- --host-run …`）交 [`backend_host::managed::run::process`]
//! （单线程运行时、阻塞池按需），其余是后端那一形 [`cc_monitor_backend::run`]（身子住库里的 `stream/entry.rs`）。

// 远端 musl 版换分配器：musl 自带的那个放掉的内存不还给系统（理由与读数住 `Cargo.toml` 那一段）。
// 只在本二进制定，不在库面定：链这个库的（判据那几份测试二进制）不跟着换。判据 `allocator_guard`。
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> std::process::ExitCode {
    #[expect(
        clippy::disallowed_methods,
        reason = "这里只看是不是托管进程那一形：是 ⇒ 交它；不是 ⇒ 后端那一形的入口自己再取一次（两形只走一形）"
    )]
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Some(rest) = backend_control::ccm::host_run_args(&argv) {
        // 一个托管进程只托管一个终端：不起后端那个按核数开工作线程的运行时。
        #[expect(
            clippy::disallowed_methods,
            reason = "托管进程那一形写自家状态（数据目录底下 `hosts/<id>/`）的句柄：只在这个入口造一次、递下去"
        )]
        let own = backend_platform::Own::issue();
        // 起子进程之前：各家「我是哪个会话」的变量一律不往下传（同后端那一形的入口；托管的子进程不许认错父）。
        backend_agents::install_child_env_filter();
        // 退出码交回 `main` 退（不在这里 `process::exit`：退出口只在后端那一形的入口与收场闸，判据 x15）；进程照 POSIX 只留低 8 位。
        let code = backend_host::managed::run::process(rest, &own);
        return std::process::ExitCode::from(code.to_le_bytes()[0]);
    }
    backend();
    std::process::ExitCode::SUCCESS
}

/// 后端那一形：多工作线程（`Disposition::SpawnBlocking` 那一档的论证压在这上面，判据 `the_backend_runtime_keeps_more_than_one_worker`）。
#[tokio::main]
async fn backend() {
    cc_monitor_backend::run().await;
}
