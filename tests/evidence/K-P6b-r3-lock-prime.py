#!/usr/bin/env python3
"""K-P6b 第三轮：把 `russh` 那棵子树从 monitor 那份 lock **播种**进 daemon 那份 lock。

# 它存在的理由（不是省事，是一条 cargo 的硬行为）

`russh 0.61.1` 无条件依赖 `crypto-bigint ^0.7.1`。现打（量法住 `§读数` 那一节）：
`crypto-bigint` 的 `0.7.0 … 0.7.4` **在 crates.io 上全部 yanked**，只有 `0.7.5` 没被 yank；
而门禁那个断网沙箱的 crate 缓存里**只有** `crypto-bigint-0.7.3.crate` 这一份。
⇒ `--offline` 把候选集限制成「已下载的那几份」⇒ 唯一候选是 yanked 的那份
⇒ **`cargo` 在「解析」这一步拒绝选 yanked 版本**（`=0.7.3` 硬钉也拒），实打退 101。

而 **`cargo` 只在解析时拒 yanked，不在「lock 里已经写着」时拒** ——
`../src-tauri/Cargo.lock` 今天就锁着 `crypto-bigint 0.7.3`（它是在 yank 之前锁进去的），
monitor 侧因此断网构建得动。⇒ 让 daemon 那份 lock 处在**同一处境**即可：
把 monitor 那份 lock 里 `russh` 可达的那些 `[[package]]` 块**原样搬过去**。

🔴 **它是一次性的引导（bootstrap），不是一份要长期手改的生成物。**
播种之后第一次 `cargo` 命令会把这份 lock 按自己的格式重写一遍 ——
**盘上最终那份 lock 是 cargo 写的**，本脚本只负责让解析器有东西可锁。

# 它买不到的（别读大）

- **不保证编得过**。它只让「解析」这一步过得去；能不能编、musl 交叉编译过不过，
  是另外两件事，本脚本一个字都不证。
- **不保证与 monitor 侧同版**。名字在 daemon 那份 lock 里**已经有**的包（`serde` / `tokio` …），
  本脚本**不覆盖**（见 `SKIPPED` 那一节的输出）⇒ 两侧那几个包可能是不同版本，
  这一格由 `cargo` 自己去解析，本脚本不替它决定。

# 跑法

    python3 evidence/K-P6b-r3-lock-prime.py            # 播种（写 remote-daemon-proto/Cargo.lock）
    python3 evidence/K-P6b-r3-lock-prime.py --dry-run  # 只报，不写

被测对象 = **本脚本所在的那棵工作树**（按 `__file__` 往上两级定位仓根，不读任何环境变量）。
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC_LOCK = ROOT / "src-tauri" / "Cargo.lock"
DST_LOCK = ROOT / "remote-daemon-proto" / "Cargo.lock"
ROOT_OF_SUBTREE = "russh"


def parse_packages(text):
    """把一份 Cargo.lock 切成 [(name, version, 整块原文)]，顺序保持原样。

    切法：以 `[[package]]` 行为界。**不解析 TOML** —— 只认块边界与块内
    `name = "…"` / `version = "…"` 两行，够用且不引第三方依赖。
    """
    blocks = []
    cur = None
    for line in text.splitlines(keepends=True):
        if line.strip() == "[[package]]":
            if cur is not None:
                blocks.append(cur)
            cur = [line]
        elif cur is not None:
            # `[metadata]` 这类尾部表结束包块
            if line.startswith("[") and line.strip() != "[[package]]":
                blocks.append(cur)
                cur = None
            else:
                cur.append(line)
    if cur is not None:
        blocks.append(cur)

    out = []
    for b in blocks:
        body = "".join(b)
        n = re.search(r'^name = "(.*)"$', body, re.M)
        v = re.search(r'^version = "(.*)"$', body, re.M)
        if n and v:
            out.append((n.group(1), v.group(1), body))
    return out


def deps_of(body):
    """块里 `dependencies = [ … ]` 的成员名（去掉版本后缀，如 `sha2 0.11.0` → `sha2`）。"""
    m = re.search(r"^dependencies = \[\n(.*?)^\]\n", body, re.M | re.S)
    if not m:
        return []
    names = []
    for line in m.group(1).splitlines():
        line = line.strip().strip(",").strip('"')
        if line:
            names.append(line.split()[0])
    return names


def main():
    dry = "--dry-run" in sys.argv
    src = parse_packages(SRC_LOCK.read_text(encoding="utf-8"))
    dst_text = DST_LOCK.read_text(encoding="utf-8")
    dst = parse_packages(dst_text)

    by_name = {}
    for name, ver, body in src:
        by_name.setdefault(name, []).append((ver, body))

    # 传递闭包：从 russh 出发按 `dependencies` 走。
    # ⚠ 同名多版本时全都带上（`sha2 0.10` 与 `sha2 0.11` 并存是常态）。
    seen = set()
    stack = [ROOT_OF_SUBTREE]
    while stack:
        n = stack.pop()
        if n in seen or n not in by_name:
            continue
        seen.add(n)
        for _ver, body in by_name[n]:
            stack.extend(deps_of(body))

    have = {name for name, _v, _b in dst}
    added, skipped = [], []
    new_blocks = []
    for name in sorted(seen):
        if name in have:
            skipped.append(name)
            continue
        for ver, body in by_name[name]:
            # path 依赖（本仓自己的 crate）不搬 —— 它们在 daemon 那份 lock 里另有住址。
            if 'source = "registry' not in body:
                skipped.append(f"{name}(非 registry)")
                continue
            added.append(f"{name} {ver}")
            new_blocks.append(body if body.endswith("\n") else body + "\n")

    print(f"闭包大小（russh 可达、含 russh 自己）: {len(seen)}")
    print(f"daemon lock 里已有、**不覆盖**: {len(skipped)}")
    for s in sorted(skipped):
        print(f"  SKIP {s}")
    print(f"要播种的包块: {len(added)}")
    for a in added:
        print(f"  ADD  {a}")

    if dry:
        print("--dry-run：没写盘")
        return 0

    # 追加在末尾。cargo 不要求 `[[package]]` 有序；它自己会重排。
    out = dst_text
    if not out.endswith("\n"):
        out += "\n"
    out += "".join(new_blocks)
    DST_LOCK.write_text(out, encoding="utf-8")
    print(f"已写 {DST_LOCK}（+{len(added)} 个包块）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
