#!/usr/bin/env python3
"""K-W2E 的量具：**插件轴那九跳，今天每一跳有几条判据在驱动它、其中几条真起过进程。**

住址（唯一）：`evidence/K-W2E-hop-census.py`。
被测对象**由 `--root` 指定，默认 = 本脚本所在的那棵树**（脚本在 `<树>/evidence/` 下）——
交回时连「量于哪棵树、哪个提交」一起写。⚠ 别把这份读数当成主干的读数：
两棵树的 `plugin/` 与 `control/cc_bus.rs` 可以不同。

# 它答什么、不答什么

答（三个数，各自带分母）：
  ① 每个语料文件里有几条 `#[test]`（分母 = 该文件里 `#[test]` 属性的处数）；
  ② 每条判据**触到哪几跳**（分母 = 下面 `HOP_NEEDLES` 那张按文件给的针表）——
     「没有一条判据同时驱动 ≥2 跳」这句话的读数就是这一列；
  ③ 每条判据**有没有真起进程**（分母 = 两根针：经通用调用口起 / 直接 `Command::new`）。

不答：
  · 「这条判据判得对不对」——本脚本只看它**碰到了哪些符号**；
  · 「生产路上有没有人走这一跳」——那一维要读生产段的调用点，`--callsites` 单独给；
  · 跳①③⑨ 的执行读数：它们要一个真 daemon 收发帧，不在本 crate 的判据里（见件文件 `KW2E6`）。

# ⚠ 抽取面的三条已知局限（写在这里，别把读数读大一格）

1. **测试体的切法是近似的**：从一条 `#[test]` 切到下一条 `#[test]`（或测试模块末尾）。
   判据之间的辅助函数会被算进**它前面那一条**。⇒ 「触到哪几跳」那一列可能**偏大**，
   而本脚本的结论（「零串联」）是**偏大方向上仍然成立**才算数。
   ★ 反方向也真的漂过一次（09-04 现打，本脚本第一版）：**第一条 `#[test]` 之前**的
   辅助函数一份都不算 ⇒ 走全流程那条判据的跳数被数**偏小**（它的六跳全在 driver 里）。
   ⇒ 现在加了一条 [`DRIVERS`] 的**传递归因**：判据调了 driver ⇒ 它继承 driver 触到的跳
   与 driver 的「起没起进程」。传递来的那几跳在输出里带「经…」标出来，别与直接命中混读。
2. **针是按文件给的**（`use super::*` ⇒ 测试里写的是裸符号名）。换文件就要换针表 ——
   新文件不在 `HOP_NEEDLES` 里时本脚本**报出来**（不静默跳过）。
3. **剥生产段的规则是 `guard_core::production_code` 的近似**：只剥
   `#[cfg(test)] mod X {` … 列 0 右大括号。与 Rust 侧那份不是同一份实现 ⇒
   两者的数对不上时**以 Rust 那份为准**，本脚本只用来出「谁碰了什么」这张表。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# ── 语料：本 crate 里与插件轴有关的判据都住这几处 ──────────────────────────────
CORPUS = [
    "remote-daemon-proto/src/plugin/discover.rs",
    "remote-daemon-proto/src/plugin/probe.rs",
    "remote-daemon-proto/src/plugin/invoke.rs",
    "remote-daemon-proto/src/plugin/mod.rs",
    "remote-daemon-proto/src/control/cc_bus.rs",
    "remote-daemon-proto/src/plugin_walk_fixture.rs",
]

# ── 九跳的针，**按文件给**（测试里 `use super::*` ⇒ 写的是裸符号名）───────────
# 值 = {跳号: [针…]}；跳号用件文件 `§0b` 的编号。
_DISCOVER = {"4": ["find(", "not_installed_message(", "is_executable(", "on_path("]}
_PROBE = {"5": ["parse(", "negotiate(", "require(", "declared_capabilities("]}
_INVOKE = {
    "6": ["argv_for(", "invoke::run(", "deadline_bin("],
    "7": ["Done {", "timed_out()", "diagnosis()", "first_line("],
}
_CC_BUS = {
    "4": ["fixed_candidates(", "not_installed_message(", "discover::find("],
    "6": ["invoke::run("],
    "7": ["out.code", "diagnosis()", "timed_out()"],
    "8": ["classify_send("],
}
_FIXTURE = {
    "2": ["unavailable_from_plugin(", "wire::Unavailable"],
    "4": ["discover::find(", "not_installed_message(", "is_executable("],
    "5": ["probe::negotiate("],
    "6": ["invoke::run(", "argv_for("],
    "7": ["refused_code", "refused_diag", "timed_out()", "sealed_code"],
    "8": ["code_word(", "real_plugin_word(", "refused_word", "sealed_word"],
}
HOP_NEEDLES = {
    "plugin/discover.rs": _DISCOVER,
    "plugin/probe.rs": _PROBE,
    "plugin/invoke.rs": _INVOKE,
    "plugin/mod.rs": {},  # 层边界判据，不驱动任何一跳（刻意留空，不是漏了）
    "control/cc_bus.rs": _CC_BUS,
    "plugin_walk_fixture.rs": _FIXTURE,
}

# ── 「真起进程」的两根针 ───────────────────────────────────────────────────────
SPAWN_NEEDLES = [
    ("invoke::run(", "经通用调用口起进程（`E5` 裁定复用的那一处口）"),
    ("Command::new(", "绕过通用口直接起进程"),
]

# ── 传递归因：`{文件: {调用针: 那个辅助函数的名字}}` ──────────────────────────
# 判据调了它 ⇒ 继承它触到的跳与它的「起没起进程」。理由见模块头注局限 1 的后半段。
DRIVERS = {
    "plugin_walk_fixture.rs": {"walk(&": "walk"},
}


def rel_key(rel: str) -> str:
    """把语料路径压成针表的键（去掉 crate 前缀）。"""
    return rel.split("remote-daemon-proto/src/", 1)[-1]


def strip_test_modules(src: str) -> tuple[str, str]:
    """切成 `(生产段, 测试段)` —— 规则是 `guard_core::production_code` 的近似。

    只认「`#[cfg(...)]` 里带 `test` 这个独立标识符」＋下一行以 `{` 收尾的模块声明，
    收尾判据 = **列 0 的右大括号**。两段都返回，免得调用方各切一份（会漂）。
    """
    lines = src.splitlines(keepends=True)
    prod: list[str] = []
    test: list[str] = []
    i = 0
    n = len(lines)
    while i < n:
        line = lines[i]
        is_attr = line.lstrip().startswith("#[cfg(") and re.search(
            r"(?<![A-Za-z0-9_-])test(?![A-Za-z0-9_-])", line
        )
        nxt = lines[i + 1] if i + 1 < n else ""
        if is_attr and nxt.rstrip().endswith("{"):
            test.append(line)
            test.append(nxt)
            j = i + 2
            while j < n and not lines[j].startswith("}"):
                test.append(lines[j])
                j += 1
            if j < n:
                test.append(lines[j])
            i = j + 1
            continue
        prod.append(line)
        i += 1
    return "".join(prod), "".join(test)


TEST_ATTR = re.compile(r"^\s*#\[test\]\s*$", re.M)
FN_NAME = re.compile(r"\bfn\s+([a-z0-9_]+)")


def split_tests(test_src: str) -> list[tuple[str, str]]:
    """把测试段切成 `[(判据名, 判据体)]`。

    ⚠ 近似（见模块头注局限 1）：从一条 `#[test]` 切到下一条。
    """
    marks = [m.start() for m in TEST_ATTR.finditer(test_src)]
    out: list[tuple[str, str]] = []
    for k, start in enumerate(marks):
        end = marks[k + 1] if k + 1 < len(marks) else len(test_src)
        body = test_src[start:end]
        m = FN_NAME.search(body)
        out.append((m.group(1) if m else "<认不出名字>", body))
    return out


def hops_of(body: str, needles: dict[str, list[str]]) -> list[str]:
    hit = []
    for hop, ns in sorted(needles.items()):
        if any(x in body for x in ns):
            hit.append(hop)
    return hit


def spawns_of(body: str) -> list[str]:
    return [why for n, why in SPAWN_NEEDLES if n in body]


def fn_body(src: str, name: str) -> str:
    """抠出测试段里一个**顶层辅助函数**的函数体（4 空格缩进的 `    }` 收尾）。

    ⚠ 近似，与 [`strip_test_modules`] 同一族：靠缩进而不是配对大括号。
    抠不到就返回空串，而调用方**要把这件事报出来**（静默空串 = 传递归因失效，
    而失效之后读数看起来和没失效一模一样）。
    """
    at = src.find(f"fn {name}(")
    if at < 0:
        return ""
    end = src.find("\n    }\n", at)
    return src[at : end + len("\n    }\n")] if end > 0 else src[at:]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
        help="被测那棵树的根（默认 = 本脚本所在的那棵树）",
    )
    ap.add_argument(
        "--callsites",
        action="store_true",
        help="另外印一份「生产段里谁在调这九跳」——那是三维读数的第㈡维",
    )
    args = ap.parse_args()
    root = Path(args.root).resolve()
    print(f"# 被测的那棵树：{root}")
    print("# ⚠ 这一行就是本读数的分母边界：换一棵树，下面每个数都可能不同。\n")

    total_tests = 0
    total_spawning = 0
    multi_hop: list[str] = []
    per_file: list[tuple[str, int]] = []

    for rel in CORPUS:
        p = root / rel
        key = rel_key(rel)
        if not p.exists():
            print(f"!! 语料不在盘上：{rel} —— 本次读数**缺一块**，不许当成 0")
            continue
        if key not in HOP_NEEDLES:
            print(f"!! 语料 {key} 不在针表里 —— 加针表或改语料清单，别静默跳过")
            continue
        src = p.read_text(encoding="utf-8")
        prod, test = strip_test_modules(src)
        tests = split_tests(test)
        per_file.append((key, len(tests)))
        print(f"## {key} —— {len(tests)} 条判据"
              f"（生产段 {len(prod)} 字节 · 测试段 {len(test)} 字节）")
        needles = HOP_NEEDLES[key]
        # 传递归因：先把这个文件里的 driver 各自触到的跳算出来。
        drivers: dict[str, tuple[list[str], list[str]]] = {}
        for call_needle, fname in DRIVERS.get(key, {}).items():
            fb = fn_body(test, fname)
            if not fb:
                print(f"!! 抠不到 driver `{fname}` 的函数体 —— 传递归因此刻失效，不许当成 0")
                continue
            drivers[call_needle] = (hops_of(fb, needles), spawns_of(fb))
        for name, body in tests:
            direct = hops_of(body, needles)
            spawn = spawns_of(body)
            via: list[str] = []
            for call_needle, (dhops, dspawn) in drivers.items():
                if call_needle in body:
                    via.extend(h for h in dhops if h not in direct)
                    spawn = spawn + [f"经 driver：{w}" for w in dspawn]
            via = sorted(set(via))
            hops = sorted(set(direct) | set(via))
            total_tests += 1
            if len(hops) >= 2:
                multi_hop.append(f"{key}::{name} → 跳{'/'.join(hops)}")
            if spawn:
                total_spawning += 1
            tag = "跳" + "/".join(direct) if direct else "无直接命中"
            if via:
                tag += f"（＋经 driver 传递：跳{'/'.join(via)}）"
            mark = "  ★真起进程" if spawn else ""
            print(f"   - {name}: {tag}{mark}")
        print()

    print("## 合计（每个数的分母写在括号里）")
    print(f"   判据总条数：{total_tests}"
          f"（分母 = 上面 {len(per_file)} 个语料文件里 `#[test]` 的处数）")
    for k, c in per_file:
        print(f"     · {k} = {c}")
    print(f"   同时驱动 ≥2 跳的判据：{len(multi_hop)}"
          f"（分母 = 同一个 {total_tests}）")
    for m in multi_hop:
        print(f"     · {m}")
    print(f"   真起进程的判据：{total_spawning}"
          f"（分母 = 同一个 {total_tests}；针 = {[n for n, _ in SPAWN_NEEDLES]}）")

    if args.callsites:
        print("\n## 第㈡维：生产段里谁在调这九跳（`invoke::run` / `discover::find` / `probe::`）")
        hits = 0
        for f in sorted((root / "remote-daemon-proto/src").rglob("*.rs")):
            prod, _ = strip_test_modules(f.read_text(encoding="utf-8"))
            for no, line in enumerate(prod.splitlines(), 1):
                for n in ("plugin::invoke::run(", "plugin::discover::find(",
                          "plugin::probe::", "plugin::invoke::argv_for("):
                    if n in line:
                        hits += 1
                        print(f"   {f.relative_to(root)}:{no}: {line.strip()}")
                        break
        print(f"   生产调用点合计：{hits}（分母 = daemon `src/` 下全部 `.rs` 的生产段）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
