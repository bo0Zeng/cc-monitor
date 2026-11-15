//! 开发调度的编译节点：经连接收作业、在这台跑并管住、自己管好自己。

pub mod facts;
pub mod instance;
pub mod limits;
pub mod procs;
pub mod root;
pub mod sccache;
pub mod serve;

/// 节点入口；返回进程退出码。
pub fn main() -> i32 {
    serve::main_with(std::env::args().skip(1).collect())
}
