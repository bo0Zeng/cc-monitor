#!/usr/bin/env python3
"""RT1 · 在 Linux 上交叉编出 Win11 虚拟机真机测试要的全部 Windows 字节（本机侧跑）。

守的要求：「能，用虚拟机」——「Win11 虚拟机可以当真机测试资源」。
本脚本只产字节，不碰虚拟机（拷过去、起、收是 `RT1-vm.py` 的事）。

配方与 `tests/scripts/re-embed.sh::do_native` 同一条（后端 / 全景小程序 `--release --locked`，
铺进 `src/frontend/shell/native-backend/` 并旁挂 `.target`），差别只有一处：re-embed 取「本机 host triple」，
这里钉死 `x86_64-pc-windows-gnu`（本机是 Linux）。

⚠ monitor 本体两处与 `cargo build` 裸跑不同，都照发版那一条补上：
  · `--features tauri/custom-protocol`：`cargo tauri build` 替你加的那一格；不加 ⇒ exe 去连 `devUrl`
    （`localhost:24174`），窗口里是一张「拒绝连接」页（2026-09-25 现打过一次）。
  · `[lib] crate-type` 从前要在这里**临时**收成 `["rlib"]`（`-gnu` 交叉链接 `monitor_lib.dll`
    报 `export ordinal too large`，RT1 F1）；WIN1 已把它在 `Cargo.toml` 里收成 `["rlib"]`（全仓没有移动端，
    `cdylib` / `staticlib` 零消费者），门禁 `winlink` 那一格真链接 ⇒ 这里只**核**它还是 `["rlib"]`，不再改文件。

产物（全在 .build/ 与 src/frontend/shell/native-backend/ 下，两处都被 gitignore）：
  .build/shell/x86_64-pc-windows-gnu/release/{cc-monitor.exe,cc-monitor-filewin.exe,WebView2Loader.dll}
  .build/backend/x86_64-pc-windows-gnu/release/cc-monitor-backend.exe
  .build/panorama/x86_64-pc-windows-gnu/release/cc-monitor-panorama.exe
  以及两个替身（不是 cargo target，rustc 直编）：claude.exe · rt1-bench.exe → .build/rt1/
"""
import os
import shutil
import subprocess

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
T = "x86_64-pc-windows-gnu"
NATIVE = os.path.join(ROOT, "src", "frontend", "shell", "native-backend")


def run(cmd, cwd):
    print("==>", " ".join(cmd), f"（{os.path.relpath(cwd, ROOT)}）", flush=True)
    subprocess.run(cmd, cwd=cwd, check=True)


def main():
    run(["npm", "run", "build"], ROOT)
    run(["cargo", "build", "--release", "--locked", "--target", T], os.path.join(ROOT, "src", "backend"))
    run(["cargo", "build", "--release", "--locked", "--target", T], os.path.join(ROOT, "src", "panorama-engine"))
    os.makedirs(NATIVE, exist_ok=True)
    for name, src in (("cc-monitor-native", f".build/backend/{T}/release/cc-monitor-backend.exe"),
                      ("cc-monitor-panorama", f".build/panorama/{T}/release/cc-monitor-panorama.exe")):
        shutil.copyfile(os.path.join(ROOT, src), os.path.join(NATIVE, name))
        with open(os.path.join(NATIVE, name + ".target"), "w") as f:
            f.write(T + "\n")
    toml = open(os.path.join(ROOT, "src", "frontend", "shell", "Cargo.toml"), "rb").read()
    assert toml.count(b'crate-type = ["rlib"]') == 1, \
        "crate-type 不再是 [\"rlib\"] —— cdylib 回来了 ⇒ -gnu 交叉链接会红在 export ordinal too large（RT1 F1）"
    try:
        run(["cargo", "build", "--release", "--locked", "-p", "monitor", "--target", T,
             "--features", "tauri/custom-protocol"], os.path.join(ROOT, "src", "frontend", "shell"))
    finally:
        # 铺进去的是 **Windows** 字节（`.target` = windows-gnu）：留着 ⇒ 本机 Linux 构建在
        # `build.rs::embed_native_backend` 的 ① 号校验上当场 panic（2026-09-25 现打过一次）。用完即撤。
        shutil.rmtree(NATIVE, ignore_errors=True)
    out = os.path.join(ROOT, ".build", "rt1")
    os.makedirs(out, exist_ok=True)
    for src, exe in (("RT1-fake-claude.rs", "claude.exe"), ("RT1-relay-bench.rs", "rt1-bench.exe")):
        run(["rustc", "-O", "--edition", "2021", "--target", T, "-o", os.path.join(out, exe),
             os.path.join(ROOT, "tests", "evidence", src)], ROOT)


if __name__ == "__main__":
    main()
