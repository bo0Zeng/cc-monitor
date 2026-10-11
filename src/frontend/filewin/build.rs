//! 本包只做一件构建期的事：Windows 进程清单编成资源、交给本包每一个链接产物（本包只有 lib ⇒ lib 的测试程序）。
//! 清单与编法只住 `src/common/win-manifest`，monitor 的 `build.rs` 调同一处。

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    win_manifest::embed_for_every_artifact();
}
