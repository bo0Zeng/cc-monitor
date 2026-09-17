#!/usr/bin/env python3
"""K-R36 · 「按判据切块」的**普查量具**（09-06 实现方现打）。

# 它回答什么

`scanning_guard_registry::every_registry_guard_keeps_its_reverse_half` 的名字说的是
**「每条登记表判据都留着反向那半」**，而它（本件之前）按**整份文件**的测试段文本判。
本量具把 `K-R36` `D1` 的四问量出来：

1. **①** 今天的人群里，每份文件各有几条判据、其中几条自带登记表、几条自带反向那半；
2. **②** 换成按判据判之后会红多少条 —— **两个口径各给一个数**：
   · **窄**（落地的那个）：表**声明在这条判据体内**才算它带表；
   · **宽**（实测之后否决的）：再算上「模块级前言里的表被这条判据**按名字**用着」；
3. **③** 那些会红的**逐条列出**（判词由人写，量具只负责把住址与体量摆出来）；
4. **④** `K-R31` 那条（`local_backend.rs::nothing_in_the_production_path_…`）会不会红。

顺带量一格 `guard_fn_item` 的**已知漏判面**：`#[test]` 之后**不是** `fn` 的那几处
（折行属性 / 夹了文档注释），以及其中**带登记表的有几处**。

# 🔴 被测对象是谁：由 `--root` 给，默认 = 本文件所在工作树的仓根

⚠ 报读数时**连 `--root` 与 `--at` 一起写**：同一条命令在不同工作树 / 不同提交上跑，
答案不同，而两次输出长得**一模一样**（`brief` 12：量具住址要能唯一定位到那一份 +
它的被测对象指向哪棵树）。

⚠ **`--at` 不是可选的讲究**：`K-R36` 往 `scanning_guard_registry.rs` 里加了几行
**长得像声明的合成夹具串**（`const REGISTERED:` / `const SITES:` 写在字符串里）。
那一份**按构造被 `scan_tree!` 摘除**、本量具也跟着摘，所以不影响人群；
但拿本量具去数「全树有几条 `const X:`」时会比立件那一刻多几条，而输出格式一模一样。
（★ 这正是本模块治的那一族：量具的语料里装着量具自己。）

# 跑法

    python3 evidence/K-R36-per-guard-census.py                  # 量工作树此刻的盘面
    python3 evidence/K-R36-per-guard-census.py --at 0b8bad7     # 量立件那一刻（D1 的分母就该这么钉）
    python3 evidence/K-R36-per-guard-census.py --list-blocks    # 逐条打印判据体（给 D1③ 判词用）

# 口径与 Rust 那一份的对应关系（改一边先看另一边）

| 本量具 | `scanning_guard_registry.rs` |
|---|---|
| `test_source` | `guard_core::test_source`（`#[cfg(test)] mod X { … }` 的**非重叠**区间） |
| `test_attr_chunks` | `guard_core::test_attr_chunks`（整行 trim 后逐字 `#[test]`） |
| `guard_fn_item` | 同名函数（跳过块首空行/属性/注释 ⇒ `fn`；收尾靠**同缩进**的 `}`） |
| `CLOSED_SET` / `REVERSE` | `TABLE_DECLS` / `REVERSE` |

🔴 `CLOSED_SET` 与 `REVERSE` 在这里是**第二份字面量** —— 本量具是一次性普查工具，
唯一的住址仍是 `scanning_guard_registry.rs` 里那两个 const。拿本文件的数说事之前，
先对一眼那边有没有变（`brief` 13b）。
"""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

CLOSED_SET = ("const REGISTERED:", "const SITES:", "const SCHEDULING_SITES:", "const FORMS:")
REVERSE = ("assert_eq!(", "已经不在了", "已经没有")
TABLE_NAMES = tuple(d[len("const "):-1] for d in CLOSED_SET)

# 那条元判据**真正**扫的那一棵树（`scan_tree!` 的实参逐字），以及它按构造摘掉的那一份。
GUARD_SUBTREE = "src-tauri/src"
GUARD_SELF = "src-tauri/src/scanning_guard_registry.rs"

FN_LINE = re.compile(r"^(\s*)fn\s+([A-Za-z0-9_]+)")


class Tree:
    """语料来源：工作树上的盘面（`at=None`），或某个提交（`at=<ref>`）。"""

    def __init__(self, root: pathlib.Path, at: str | None):
        self.root, self.at = root, at
        self._names: list[str] = []
        if at:
            r = subprocess.run(
                ["git", "-C", str(root), "ls-tree", "-r", "--name-only", at],
                capture_output=True, text=True, check=True,
            )
            self._names = [x for x in r.stdout.split("\n") if x.endswith(".rs")]

    def label(self) -> str:
        return f"{self.root}  @ {self.at or '工作树盘面（未提交的改动也算）'}"

    def list_rs(self, sub: str) -> list[str]:
        if self.at:
            return sorted(n for n in self._names if n.startswith(sub + "/"))
        d = self.root / sub
        return sorted(p.relative_to(self.root).as_posix() for p in d.rglob("*.rs"))

    def read(self, rel: str) -> str:
        if self.at:
            return subprocess.run(
                ["git", "-C", str(self.root), "show", f"{self.at}:{rel}"],
                capture_output=True, text=True, check=True,
            ).stdout
        return (self.root / rel).read_text(encoding="utf-8", errors="replace")


def test_source(src: str) -> str:
    """`guard_core::test_module_ranges` 的同形实现 —— **非重叠**。

    ⚠ 与 `scanning_guard_registry.rs` 里那个**私有的** `test_regions` 刻意不同：
    后者按任意一处 `#[cfg(test)]` 起、到下一处行首 `}` 止，**区间会重叠**
    ⇒ 一份多次出现 `#[cfg(test)]` 的文件，它拼出来的文本里同一段会出现好几遍
    （09-06 现打的活体：`plugin_class_registry.rs` 盘上 `const REGISTERED:` **只有 1 处**
    （模块级），而那份拼接文本里出现 **3** 次 —— 1 次落在前言（对的），
    **2 次都落进同一条判据** `the_arm_extractor_takes_one_arm_not_the_whole_case` 的块
    ⇒ 按块判凭空多 **1 条**假人群（两次重复命中同一块，所以是 1 条不是 2 条）。
    判「存在性」时看不出差别，**按块切时差别是致命的** —— 所以判据级那一层走这一份。
    """
    out, i = [], 0
    while True:
        rel = src.find("\n#[cfg(", i)
        if rel < 0:
            break
        attr_start = rel + 1
        attr_end = src.find("\n", attr_start)
        attr_end = len(src) if attr_end < 0 else attr_end
        mod_start = min(attr_end + 1, len(src))
        mod_end = src.find("\n", mod_start)
        mod_end = len(src) if mod_end < 0 else mod_end
        mod_line = src[mod_start:mod_end].strip()
        if not (src[attr_start:attr_end].strip() == "#[cfg(test)]"
                and mod_line.startswith("mod ") and mod_line.endswith("{")):
            i = attr_end
            continue
        e = src.find("\n}", rel)
        if e < 0:
            out.append(src[rel:])
            break
        out.append(src[rel:e + len("\n}")])
        i = e + len("\n}")
    return "".join(out)


def test_attr_chunks(text: str) -> list[list[str]]:
    """`guard_core::test_attr_chunks` 的同形实现（按行认，边界不进任何一块）。"""
    out: list[list[str]] = []
    cur: list[str] = []
    for line in text.split("\n"):
        if line.strip() == "#[test]":
            if cur:
                out.append(cur)
                cur = []
            continue
        cur.append(line)
    out.append(cur)
    return out


SKIP = ("#", "//")


def guard_fn_item(chunk: list[str]):
    """块里那个 fn 项本身 —— 返回 (判据名, 起, 止)；不是一条判据就返回 None。

    `止` 之后的那几行是**跟在这条判据后面的模块级代码**（辅助函数 / 常量 /
    下一条判据的文档注释），它们归模块级，不算这条判据自带。
    """
    head = None
    for i, l in enumerate(chunk):
        t = l.lstrip()
        if t == "" or t.startswith(SKIP):
            continue
        head = i
        break
    if head is None:
        return None
    m = FN_LINE.match(chunk[head])
    if not m:
        return None
    close = m.group(1) + "}"
    end = len(chunk)
    for j in range(head, len(chunk)):
        if chunk[j] == close:
            end = j + 1
            break
    return m.group(2), head, end


def used_by_name(item: str, name: str) -> bool:
    """宽口径：这条判据体里**按名字**出现了那张模块级表（含字符串与注释里的提及）。"""
    return re.search(r"(?<![A-Za-z0-9_])" + name + r"(?![A-Za-z0-9_])", item) is not None


def fn_units(text: str) -> dict[str, str]:
    """一份源码里每个 `fn <名>` 项的**原样字节** md5（切法与 `guard_fn_item` 同：同缩进收尾 `}`）。

    ⚠ 与 `evidence/K-R30-fn-body-md5.py` **刻意不同**：那一份的分母是**生产段**的列 0 函数，
    而本模块**整个住在 `#[cfg(test)]` 里** ⇒ 拿那把尺子来量，分母是空的、结论恒是「全同」。
    这里量的是**测试段里的每一个 `fn`**（含 `#[test]` 与辅助函数）。
    """
    lines = text.split("\n")
    out: dict[str, str] = {}
    for i, l in enumerate(lines):
        m = FN_LINE.match(l)
        if not m:
            continue
        close = m.group(1) + "}"
        end = len(lines)
        for j in range(i + 1, len(lines)):
            if lines[j] == close:
                end = j + 1
                break
        body = "\n".join(lines[i:end]).encode("utf-8")
        out[m.group(2)] = hashlib.md5(body).hexdigest()[:12]
    return out


def fn_md5_diff(root: pathlib.Path, rev_a: str, rev_b: str, rel: str) -> None:
    def at(rev: str) -> str:
        return subprocess.run(["git", "-C", str(root), "show", f"{rev}:{rel}"],
                              capture_output=True, text=True, check=True).stdout

    ta, tb = at(rev_a), at(rev_b)
    print(f"# 改动面 · 逐函数 md5：{rel}")
    print(f"#   改前 {rev_a} · 改后 {rev_b} · 仓根 {root}")
    # 🔴 非空对照（`brief` 12）：整份文件的 md5 必须**不同**，否则我根本没比到东西。
    ma = hashlib.md5(ta.encode()).hexdigest()[:12]
    mb = hashlib.md5(tb.encode()).hexdigest()[:12]
    print(f"  〔非空对照〕整份文件 md5：{ma} -> {mb}   {'不同 ✓' if ma != mb else '🔴 相同 —— 没比到东西'}")
    fa, fb = fn_units(ta), fn_units(tb)
    both = sorted(set(fa) & set(fb))
    print(f"  分母只装**两边都有**的 {len(both)} 个 fn（`brief` 15a）；只有一边有的单列在下面")
    for n in both:
        mark = "逐字未动" if fa[n] == fb[n] else "**变了**"
        print(f"    {n:58s} {fa[n]} -> {fb[n]}  {mark}")
    for n in sorted(set(fa) - set(fb)):
        print(f"    〔只在改前〕{n}  {fa[n]}")
    for n in sorted(set(fb) - set(fa)):
        print(f"    〔只在改后 = 本件新增〕{n}  {fb[n]}")


def main() -> int:
    here = pathlib.Path(__file__).resolve()
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(here.parent.parent), help="仓根（默认 = 本文件所在的那棵工作树）")
    ap.add_argument("--at", default=None, help="量哪个提交（不给 = 量工作树此刻的盘面）")
    ap.add_argument("--list-blocks", action="store_true", help="逐条打印会红的判据体（D1③ 的输入）")
    ap.add_argument("--fn-md5", nargs=2, metavar=("改前rev", "改后rev"),
                    help="改动面：`src-tauri/src/scanning_guard_registry.rs` 逐函数 md5 对拍")
    a = ap.parse_args()

    if a.fn_md5:
        fn_md5_diff(pathlib.Path(a.root).resolve(), a.fn_md5[0], a.fn_md5[1],
                    "src-tauri/src/scanning_guard_registry.rs")
        return 0

    tree = Tree(pathlib.Path(a.root).resolve(), a.at)
    print(f"# 被测对象：{tree.label()}")
    print(f"# 扫描面：`{GUARD_SUBTREE}` 一棵树，摘掉 `{GUARD_SELF}`（`scan_tree!` 按 `file!()` 摘）")
    print(f"# 本趟用的闭集（{len(CLOSED_SET)} 个名字）：{' · '.join(CLOSED_SET)}")
    print(f"# 本趟用的 REVERSE（{len(REVERSE)} 形）：{' · '.join(REVERSE)}")

    rels = [r for r in tree.list_rs(GUARD_SUBTREE) if r != GUARD_SELF]
    print(f"\n## 一 · 文件级人群（今天这条元判据采到的那一档）· 分母 = 扫描面 {len(rels)} 份 `.rs`")
    print(f"{'文件':52s} {'判据':>4s} {'带表(窄)':>8s} {'带表(宽)':>8s} {'红(窄)':>7s} {'红(宽)':>7s}")

    n_pop_files = 0
    narrow_pop, narrow_red, wide_pop, wide_red = [], [], [], []
    orphan_chunks: list[str] = []      # `#[test]` 之后不是 `fn` 的那几块
    orphan_with_table = 0
    blocks_for_listing: dict[str, list[str]] = {}

    for rel in rels:
        seg = test_source(tree.read(rel))
        if not any(d in seg for d in CLOSED_SET):
            continue
        n_pop_files += 1
        chunks = test_attr_chunks(seg)
        guards, modlevel = [], []
        for k, c in enumerate(chunks):
            got = guard_fn_item(c)
            if got is None:
                modlevel += c
                if k > 0:
                    head = next((l for l in c if l.strip()), "")
                    orphan_chunks.append(f"{rel}  首行=<{head.strip()[:70]}>")
                    if any(d in "\n".join(c) for d in CLOSED_SET):
                        orphan_with_table += 1
                continue
            name, head_i, end_i = got
            guards.append((name, "\n".join(c[head_i:end_i])))
            modlevel += c[:head_i] + c[end_i:]
        modtext = "\n".join(modlevel)
        modtabs = [n for d, n in zip(CLOSED_SET, TABLE_NAMES) if d in modtext]

        nw = nwr = ww = wwr = 0
        for name, item in guards:
            own = any(d in item for d in CLOSED_SET)
            used = [n for n in modtabs if used_by_name(item, n)]
            has_rev = any(m in item for m in REVERSE)
            addr = f"{rel}::{name}"
            if own:
                nw += 1
                narrow_pop.append(addr)
                if not has_rev:
                    nwr += 1
                    narrow_red.append(addr)
                    blocks_for_listing[addr] = item.split("\n")
            if own or used:
                ww += 1
                wide_pop.append(addr)
                if not has_rev:
                    wwr += 1
                    wide_red.append((addr, "体内" if own else "模块级:" + ",".join(used)))
                    blocks_for_listing.setdefault(addr, item.split("\n"))
        print(f"{rel:52s} {len(guards):4d} {nw:8d} {ww:8d} {nwr:7d} {wwr:7d}")

    print(f"\n## 二 · ②「换成按判据判之后会红多少条」—— 两个口径各一个数")
    print(f"  文件级人群（分母）           ：{n_pop_files} 份")
    print(f"  **窄**口径（落地的那个）人群 ：{len(narrow_pop)} 条判据 · 红 **{len(narrow_red)}** 条")
    print(f"  **宽**口径（实测后否决）人群 ：{len(wide_pop)} 条判据 · 红 **{len(wide_red)}** 条")

    print(f"\n## 三 · ③ 会红的逐条（判词由人写；量具只摆住址与归属来源）")
    print("  〔窄〕")
    for x in narrow_red:
        print(f"    {x}")
    print("  〔宽 —— 比窄多出来的那几条〕")
    for addr, why in wide_red:
        if addr not in narrow_red:
            print(f"    {addr}   〔表来自 {why}〕")

    k_r31 = "src-tauri/src/backend/control/local_backend.rs::" \
            "nothing_in_the_production_path_runs_code_between_fork_and_exec"
    print(f"\n## 四 · ④ `K-R31` 那条在新口径下会不会红")
    print(f"  进窄口径人群：{'是' if k_r31 in narrow_pop else '否'}"
          f" · 会红：{'是' if k_r31 in narrow_red else '否'}")

    print(f"\n## 五 · `guard_fn_item` 的已知漏判面（`#[test]` 之后认不出 `fn` 的块）")
    print("  🔴 分母**只取人群那几份文件**，而这是够的：一块里若真有登记表声明，"
          "那份文件的测试段就含闭集里的名字 ⇒ 它必定在人群里。")
    print("  ⚠ 单行的 `#[ignore = \"…\"]` / `#[cfg(…)]` **不是**漏判面 —— 块首的属性行会被跳过。")
    print(f"  共 {len(orphan_chunks)} 块，其中**块里带登记表的** {orphan_with_table} 块")
    for x in orphan_chunks:
        print(f"    {x}")

    if a.list_blocks:
        print("\n## 六 · 会红的判据体逐字（D1③ 判词的输入）")
        for addr, body in blocks_for_listing.items():
            print(f"\n----- {addr}  （{len(body)} 行）-----")
            print("\n".join(body))
    return 0


if __name__ == "__main__":
    sys.exit(main())
