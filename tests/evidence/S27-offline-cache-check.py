#!/usr/bin/env python3
"""S27 判据 —— **断网门禁要的那些 `.crate`，今天在不在本机缓存里**。

# 它治的病

门禁断网跑（`.claude/devbox/gate` 默认 `--network none`，缓存是具名卷
`ccmon-cargo-registry`，从宿主 `~/.cargo/registry` 播种）。缓存里缺一份 `.crate`，
`cargo` 退 **101**，输出是「attempting to make an HTTP request, but --offline was
specified」—— **门禁红在「下不到包」这条与代码对不对毫无关系的诊断上**。
本仓刚踩过同形的坑（一把尺子崩在 `substring not found`，而病在搬树）。

⇒ 本判据**不等门禁去红**，它自己先点名：**谁缺了**。

# 两个人群（都必须非空 —— 扫到 0 条当成失败，不是「没问题」）

- **P1「今天」**：`src/bridge/Cargo.lock` ＋ `src/backend/Cargo.lock` ＋ `src/panorama-engine/Cargo.lock`（〔RM1c〕）里
  **source 指向 crates.io** 的全部 `(name, version)` 去重并集。
  这是**今天**门禁断网构建要的全集。现打 672 条（2026-09-19）。
  ⚠ `path` 依赖（本仓自己那 19 个块）不在人群里 —— 它们不走 registry 缓存。
- **P2「23a 的载荷」**：`tests/evidence/S27-cache-manifest.md` 那张表里的 26 条 ——
  `russh-sftp 3.0.0` 落地会新增 / 顶版的 `(name, version)`。
  **它们今天还没进任何一份 lock**，所以 P1 盖不到，必须单列。

# 它买不到什么（诚实段）

- **只看 `.crate` 文件在不在**，不校验内容、不校验 checksum（`cargo` 自己会校）。
- **不证「编得过」**。它只证「解析 + 取包这一步断网过得去」。
- **不看沙箱那个具名卷**，看的是**宿主的** `~/.cargo/registry`（或 `$CARGO_HOME`）——
  那是卷的**种子**。卷若已存在且比种子旧，本判据看不见那条差。
  ⇒ 卷是派生物，种子是真相源；要确保两者一致就把卷删掉让它重播种。
- **index 那一半只在跑真 cargo 时才验**。离线解析除了 `.crate` 还要 registry index
  的本地缓存（`~/.cargo/registry/index/*/.cache/`）。本脚本不解析 index 的二进制缓存格式；
  真正把这一格买下来的是 `S27-prove-offline.py`（`bwrap --unshare-net` 跑真 `cargo fetch`）。

# 跑法

    python3 tests/evidence/S27-offline-cache-check.py            # 人读
    python3 tests/evidence/S27-offline-cache-check.py --json     # 机读

退出码：0 = 齐；1 = 缺（缺谁逐条印在 stderr）；2 = 人群空 / 环境不对（同样算失败）。

被测对象 = **本脚本所在的那棵工作树**（按 `__file__` 往上两级定位仓根，不读环境变量）。
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# 〔RM1c · 第四波〕第三份：只装全景引擎的独立小程序（`src/panorama-engine`）。
# 它的 lock 由 monitor 那份播种后剪枝（同一批版本）⇒ 今天不新增任何 `(name, version)`，
# 但它**是**一份断网构建要解析的 lock，不进人群的话哪天它自己升一个版本就没人点名。
LOCKS = [
    ROOT / "src" / "bridge" / "Cargo.lock",
    ROOT / "src" / "backend" / "Cargo.lock",
    ROOT / "src" / "panorama-engine" / "Cargo.lock",
]
MANIFEST = ROOT / "tests" / "evidence" / "S27-cache-manifest.md"

# 反空真的地板。现打（2026-09-19）P1 = 672、P2 = 26。
# 地板刻意**不等于**现打值：等号会让任何一次正常的依赖增减都红成假阳性；
# 但也刻意**不设成 1**：那样「lock 被清空」这种病就漏过去了。
P1_FLOOR = 500
P2_EXACT = 26

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def cargo_cache_dirs() -> list[Path]:
    """本机 cargo 的 `.crate` 缓存目录。

    `$CARGO_HOME` 优先（沙箱镜像里是 `/opt/rust/cargo`，**不是** `~/.cargo`），
    回落 `~/.cargo`。目录名形如 `index.crates.io-<hash>`（sparse）或 `github.com-<hash>`（git 索引）；
    一台机器上可能同时有好几个（换过 cargo 大版本就会多一个），**全都算数**。
    """
    home = os.environ.get("CARGO_HOME") or str(Path.home() / ".cargo")
    base = Path(home) / "registry" / "cache"
    if not base.is_dir():
        return []
    return sorted(p for p in base.iterdir() if p.is_dir())


def lock_pairs(path: Path) -> list[tuple[str, str]]:
    """把一份 Cargo.lock 切成 crates.io 来源的 `(name, version)`。

    切法与 `K-P6b-r3-lock-prime.py` 同源：以 `[[package]]` 为界，只认块内三行，
    **不解析 TOML**（不引第三方依赖 —— 本仓的判据一律不许为了好看去联网装包）。
    没有 `source` 的块 = `path` 依赖（本仓自己的 crate），不进人群。
    """
    out: list[tuple[str, str]] = []
    text = path.read_text(encoding="utf-8")
    for blk in text.split("[[package]]")[1:]:
        name = re.search(r'^name = "(.*)"', blk, re.M)
        ver = re.search(r'^version = "(.*)"', blk, re.M)
        src = re.search(r'^source = "(.*)"', blk, re.M)
        if not (name and ver and src):
            continue
        if not src.group(1).startswith(CRATES_IO):
            continue
        out.append((name.group(1), ver.group(1)))
    return out


ROW = re.compile(r"^\|\s*`([A-Za-z0-9_.+-]+)`\s*\|\s*([0-9][A-Za-z0-9_.+-]*)\s*\|")


def manifest_pairs(path: Path) -> list[tuple[str, str]]:
    """解析 `S27-cache-manifest.md` 那张表。表头/分隔行天然不匹配（第一格不是反引号包住的名字）。"""
    return [(m.group(1), m.group(2)) for m in (ROW.match(l) for l in path.read_text(encoding="utf-8").splitlines()) if m]


def missing(pairs, dirs: list[Path]) -> list[tuple[str, str]]:
    gone = []
    for name, ver in pairs:
        f = f"{name}-{ver}.crate"
        if not any((d / f).is_file() for d in dirs):
            gone.append((name, ver))
    return gone


def main() -> int:
    as_json = "--json" in sys.argv
    fail: list[str] = []

    dirs = cargo_cache_dirs()
    for p in LOCKS + [MANIFEST]:
        if not p.is_file():
            print(f"S27: 读不到 {p} —— 判不了，当失败", file=sys.stderr)
            return 2

    p1 = sorted({pair for lk in LOCKS for pair in lock_pairs(lk)})
    p2 = manifest_pairs(MANIFEST)

    # ── 反空真：人群本身先立住，再谈缺不缺 ──────────────────────────────
    if len(p1) < P1_FLOOR:
        fail.append(f"P1 只扫到 {len(p1)} 条（地板 {P1_FLOOR}）—— 几份 lock 或解析口径坏了，不是「没问题」")
    if len(p2) != P2_EXACT:
        fail.append(f"P2 扫到 {len(p2)} 条，表里应当恰好 {P2_EXACT} 条 —— manifest 被改过或解析口径坏了")
    if not dirs:
        fail.append("本机找不到任何 cargo registry 缓存目录（$CARGO_HOME/registry/cache 或 ~/.cargo/registry/cache）")

    m1 = missing(p1, dirs) if dirs else p1
    m2 = missing(p2, dirs) if dirs else p2

    report = {
        "cache_dirs": [str(d) for d in dirs],
        "p1_total": len(p1),
        "p1_missing": [f"{n} {v}" for n, v in m1],
        "p2_total": len(p2),
        "p2_missing": [f"{n} {v}" for n, v in m2],
        # 🔴 `list(fail)` 不是洁癖：下面还会往 `fail` 里追「缺了几份」那两条，
        # 而 `structural_failures` 要的是**人群/环境本身坏没坏**这一类。
        # 直接放引用的话两类会混在一起 —— 调用方（`tests/offline-cargo-cache.vitest.ts`）
        # 先断言这一项为空，于是「缺谁」那条**具名**断言永远轮不到开口，
        # 报出来的就成了一句不点名的话。那正是本件要治的那种病。
        "structural_failures": list(fail),
    }

    if m1:
        fail.append(f"P1（今天那几份 lock）缺 {len(m1)} 份 `.crate` —— 断网门禁会红在「下不到包」上")
    if m2:
        fail.append(f"P2（23a 的 26 条）缺 {len(m2)} 份 `.crate` —— `russh-sftp` 一落地门禁当场断网失败")

    report["ok"] = not fail

    if as_json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        print(f"S27 断网缓存判据 · 缓存目录 {', '.join(map(str, dirs)) or '（无）'}")
        print(f"  P1 今天那几份 lock 的 crates.io 条目 : {len(p1) - len(m1)}/{len(p1)} 齐")
        print(f"  P2 23a 要新增的条目（manifest）    : {len(p2) - len(m2)}/{len(p2)} 齐")
        for n, v in m1:
            print(f"  🔴 P1 缺：{n} {v}", file=sys.stderr)
        for n, v in m2:
            print(f"  🔴 P2 缺：{n} {v}", file=sys.stderr)
        for f in fail:
            print(f"  🔴 {f}", file=sys.stderr)
        print("S27: OK" if not fail else "S27: FAIL")

    return 0 if not fail else 1


if __name__ == "__main__":
    sys.exit(main())
