#!/usr/bin/env python3
"""K-R126 量具 —— 在**沙箱里**反复跑 `relay::server::tests` 那一族，数红。

住址：`<本仓>/evidence/K-R126-ruler.py`（跟着工作树走）。
被测对象：**本文件所在的那棵工作树**（由 `__file__` 现算，见 `WT`）——
刻意不接受「树」参数，免得同一个住址先后指向两棵树、跑出一张静默的假读数表
（`brief` 硬规则 12 逐字点名的那一形）。

🔴 K31：一切 cargo 动作都在 `ccmon-devbox:latest` 容器里跑，宿主上一条都不跑。
   挂载与环境**逐项抄自** `.claude/devbox/gate`（PROJ / cargo registry 卷 / HOME / CARGO_TARGET_DIR）。
   与 `gate` 的差别只有一处、且是刻意的：它跑 `scripts/gate.sh` 全量门禁，
   本量具跑**指名的那几条测试**并循环 N 趟 —— `KR126D2` 刀③ 要「正常一趟跑很多遍」，
   全量门禁跑不了那个次数。收官那一趟仍然跑真 `gate`。

用法：
  K-R126-ruler.py build                      # 只编，不跑
  K-R126-ruler.py run  --runs=N [--threads=K] [--filter=F ...] [--cpuset=C]
  K-R126-ruler.py family --runs=N            # 整族（relay::server::tests::）
退出码：0 = 全绿；1 = 有红；3 = 环境不对。
"""
import argparse
import json
import os
import pathlib
import re
import subprocess
import sys

WT = pathlib.Path(__file__).resolve().parent.parent          # 被测对象 = 这棵树
PROJ = pathlib.Path("/home/zbl/文档/claudecode-frontend")
TARGETS = PROJ / ".claude" / "pm-targets"
TAG = WT.name
CACHE_VOL = "ccmon-cargo-registry"
IMAGE = "ccmon-devbox:latest"
CRATE_DIR = WT / "remote-daemon-proto"

# 那一族的前缀。**只有一个住址**，下面所有命令都取它。
FAMILY = "relay::server::tests::"


def _docker(inner: str, cpuset: str | None = None, timeout: int = 3600):
    cmd = ["docker", "run", "--rm", "--network", "none"]
    if cpuset:
        cmd += ["--cpuset-cpus", cpuset]
    cmd += [
        "-v", f"{PROJ}:{PROJ}",
        "-v", f"{CACHE_VOL}:/opt/rust/cargo/registry",
        "-e", f"CARGO_TARGET_DIR={TARGETS / TAG}",
        "-e", "HOME=/home/zbl",
        "-w", str(CRATE_DIR),
        IMAGE,
        "bash", "-o", "pipefail", "-c", inner,
    ]
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def preflight():
    if not (WT / "src-tauri").is_dir():
        sys.exit(f"❌ 不像工作树（没有 src-tauri/）：{WT}")
    if subprocess.run(["docker", "image", "inspect", IMAGE],
                      capture_output=True).returncode != 0:
        sys.exit(f"❌ 镜像 {IMAGE} 不在 —— 不许退回宿主跑（K31）")
    (TARGETS / TAG).mkdir(parents=True, exist_ok=True)


def build() -> str:
    """编出测试二进制，返回它在容器里的绝对路径。"""
    r = _docker("cargo test --no-run 2>&1 | tail -40")
    m = re.search(r"Executable unittests \S+ \((\S+)\)", r.stdout)
    if not m:
        print(r.stdout[-4000:], file=sys.stderr)
        sys.exit("❌ 没能从 `cargo test --no-run` 的输出里认出测试二进制")
    return m.group(1)


def run(binpath: str, runs: int, threads: int, filters: list[str],
        cpuset: str | None, keep_red: pathlib.Path | None):
    """跑 runs 趟，返回 (red, total, 第一趟红的全文)。"""
    filt = " ".join(f"'{f}'" for f in filters)
    inner = (
        f'red=0; first=""; '
        f'for i in $(seq 1 {runs}); do '
        f'  if ! {binpath} --test-threads={threads} {filt} > /tmp/kr126.$$.out 2>&1; then '
        f'    red=$((red+1)); '
        f'    if [ -z "$first" ]; then first=1; cp /tmp/kr126.$$.out /tmp/kr126.firstred; fi; '
        f'  fi; '
        f'done; '
        f'echo "__RED__=$red"; '
        f'if [ -f /tmp/kr126.firstred ]; then echo "__FIRSTRED__"; cat /tmp/kr126.firstred; fi'
    )
    r = _docker(inner, cpuset=cpuset)
    out = r.stdout
    m = re.search(r"__RED__=(\d+)", out)
    if not m:
        print(out[-4000:], file=sys.stderr)
        print(r.stderr[-2000:], file=sys.stderr)
        sys.exit("❌ 量具自己没跑成 —— 按 CRASH 记，不许读成「新红 0」")
    red = int(m.group(1))
    body = out.split("__FIRSTRED__", 1)[1] if "__FIRSTRED__" in out else ""
    if keep_red and body:
        keep_red.write_text(body, encoding="utf-8")
    return red, runs, body


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["build", "run", "family"])
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--threads", type=int, default=1)
    ap.add_argument("--filter", action="append", default=[])
    ap.add_argument("--cpuset", default=None)
    ap.add_argument("--keep-red", default=None)
    a = ap.parse_args()

    preflight()
    binpath = build()
    print(f"· 被测树：{WT}")
    print(f"· 测试二进制：{binpath}")
    if a.mode == "build":
        return 0

    filters = a.filter or ([FAMILY] if a.mode == "family" else [])
    if not filters:
        sys.exit("❌ run 模式要给 --filter（给不出就用 family）")
    keep = pathlib.Path(a.keep_red).resolve() if a.keep_red else None
    red, total, body = run(binpath, a.runs, a.threads, filters, a.cpuset, keep)
    print(f"· 筛子：{filters} · --test-threads={a.threads} · cpuset={a.cpuset}")
    print(f"RED={red} / {total}")
    if body:
        print("---- 头一趟红的全文（截断到 120 行）----")
        print("\n".join(body.splitlines()[:120]))
    return 1 if red else 0


if __name__ == "__main__":
    sys.exit(main())
