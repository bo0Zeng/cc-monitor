#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R28 `D5①` / 交回「改动面」那一栏的量具：**按函数取体、逐函数 md5**。

⚠ 本文件是 **K-R28 自己的量具**，被测树写死在 `WT`（见下）。〔风险 5k〕

── 口径 ───────────────────────────────────────────────────────────
  · 取体的办法**逐字照 `sftp.rs::both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker`
    那一份**：从签名首次出现处起，切到**下一处 `\\n}\\n`**（列 0 的右大括号）为止。
    ⇒ 它与本件新加的那条函数体判据看的是**同一段字节**，两边不会各看一半。
  · md5 打在**原样字节**上（含注释与空白）——「逐字不变」就是逐字。
  · 分母 = 命令行给的那几个签名；**每个签名在文件里必须恰好命中 1 次**，
    命中 0 次或多次一律报错退非零（不许静默取第一处）。

用法：
  python3 evidence/K-R28-fn-body-md5.py            # 打本件钉住的那几处
  python3 evidence/K-R28-fn-body-md5.py <文件相对路径> <签名>   # 打指定的一处
"""
import hashlib
import os
import sys

WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r28"

# 本件钉住的那几处：(标签, 文件相对路径, 签名)
PINS = [
    (
        "D5① extract_embedded_to（.partial / rename / 带 pid 那三样，一个字不许动）",
        "src-tauri/src/backend/control/local_backend.rs",
        "pub fn extract_embedded_to(\n    dir: &Path,",
    ),
    (
        "D3 落点① supervise_with_stdio",
        "src-tauri/src/backend/control/local_backend.rs",
        "pub fn supervise_with_stdio(\n    bin: PathBuf,",
    ),
    (
        "D3 落点② spawn_detached（Linux 那一支）",
        "src-tauri/src/local_daemon.rs",
        "fn spawn_detached(\n    bin: &std::path::Path,\n    port: u16,",
    ),
    (
        "D5② SuperviseEvent 事件枚举（上线契约面）",
        "src-tauri/src/backend/control/local_backend.rs",
        "pub enum SuperviseEvent {",
    ),
    (
        "D5① 的邻居：resolve_daemon_bin（本件不许动的那一跳）",
        "src-tauri/src/local_daemon.rs",
        "fn resolve_daemon_bin(\n    extract_dir: &std::path::Path,",
    ),
]


def body(rel: str, sig: str):
    path = os.path.join(WT, rel)
    src = open(path, encoding="utf-8", errors="replace").read()
    n = src.count(sig)
    if n != 1:
        return None, n, None
    i = src.index(sig)
    k = src.find("\n}\n", i)
    j = k if k >= 0 else len(src)
    seg = src[i:j]
    line = src[:i].count("\n") + 1
    return seg, 1, line


def main() -> int:
    pins = PINS
    if len(sys.argv) == 3:
        pins = [("<命令行给的>", sys.argv[1], sys.argv[2])]
    rc = 0
    for label, rel, sig in pins:
        seg, n, line = body(rel, sig)
        if seg is None:
            print(f"🔴 {label}\n   {rel}  签名命中 {n} 次（要求恰好 1）—— 判不了，别当成「没变」")
            rc = 1
            continue
        h = hashlib.md5(seg.encode("utf-8")).hexdigest()
        print(f"{h}  {len(seg):6d}B  {len(seg.splitlines()):4d}行  {rel}:{line}")
        print(f"    └ {label}")
        print(f"    └ 校验位（签名那一行逐字）：{sig.splitlines()[0]}")
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
