//! 后端二进制：全局分配器 ＋ 一行 [`cc_monitor_backend::run`]（身子住库里的 `stream/entry.rs`）。

// 远端 musl 版换分配器：musl 自带的那个放掉的内存不还给系统（理由与读数住 `Cargo.toml` 那一段）。
// 只在本二进制定，不在库面定：本机 GUI 进程把后端当库链进去，库里定会连壳一起换。判据 `allocator_guard`。
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() {
    cc_monitor_backend::run().await;
}
