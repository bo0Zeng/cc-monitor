//! 开发调度两端之间的帧与连接：一条连接上一行一帧的 JSON。调度端与编译节点链这同一份。

pub mod conn;
pub mod frame;
pub mod line;
pub mod shape;
pub mod stop;

pub use frame::*;
pub use line::{decode, encode, write_frame, FrameReader, WireError, MAX_LINE};
pub use shape::{digest, proto};
pub use stop::StopWatch;

/// 只当兜底的等待期限（测试里等帧 · 等进程退 · 等条件，到了就当红；生产里没有别的兜底）：开发调度里只这一个数，各处都引用它。
pub const WALL_CEILING: std::time::Duration = std::time::Duration::from_secs(300);

/// 节点根下 `toolchains/` 里不是「照工具清单装进来的工具」的那几个目录（Rust 工具链 · 手机 SDK）：
/// 调度端的工具清单不许用这几个名字，节点的作业 PATH 也不把它们当工具接进来。两头只认这一份。
pub const TOOLCHAINS_NOT_TOOLS: &[&str] = &["cargo", "rustup", "android-sdk"];
