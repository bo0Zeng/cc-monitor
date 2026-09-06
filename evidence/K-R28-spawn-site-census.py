#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R28 `D1②` 的量具：**生产段里「起一个进程」的落点**有几处，其中几处 exec 的是**我们自己刚写出来的**文件。

⚠ 本文件是 **K-R28 自己的量具**，被测对象**写死指向 `k-r28` 这棵树**（见 `WT`）。
   〔风险 5k：临时目录同名量具被覆盖过 —— 所以名字里带件号，且住址与被测树一起写在这里〕
   同名前身 `evidence/K-R24-D7-exec-after-write-census.py` 的 `WT` 指的是 **`k-r24c`**，
   口径也不同（它数的是「加执行位」的候选面）。**两者不可互换，读数也不同源。**

── 口径（`D1②` 要的「把口径写死」）───────────────────────────────────
  本件问的是 **落点**，不是「人群」：
    **落点** = 生产段里，本进程**直接把一个可执行文件起起来**的那一处代码
              （`Command::new(` 所在的那个函数）。
    ⚠ 与 `K-R24` 待办里那个「4 处人群」**不同源**：那一条数的是
       `evidence/K-R24-D7-exec-after-write-census.py` 的**候选面**口径
       （「给一个路径加执行位」的处数，机器数出 10 处），**不是起进程的落点**。
       两个数不同分母、不同被数物，**本件不去凑那个 4**。

  两级分母，分开报：
    · 分母① = 三个 Rust 根下的 `.rs` 文件数（去 `vendor/` 与 `target/`）。
    · 分母② = 分母① 的**生产段**里 `Command::new(` 的处数（按「文件::外层函数」去重）。
              ⇒ 这一格与 `write_site_registry::tests::SPAWNS` 那张申报表**同口径**
                （同一个针、同一个 `enclosing_fn` 回溯法），可以互相对拍。
    · **本件的落点** = 分母② 里同时满足下面两条的那几处（这一半**是逐处读代码判的，
      不是机器判的** —— 别把它读成机检读数）：
        ⓐ 被 exec 的那个二进制**是本进程在同一条路上刚写出来的**
          （`local_backend::extract_embedded_to` 释放内嵌 daemon 那条路）；
        ⓑ 它**真的被 exec**（只 `stat` / 只查 `PATH` 的不算 —— 那条路不经过 execve）。

  🔴 剥生产段的口径：**逐块剥掉带花括号体的 `#[cfg(…test…)] mod X { … }`**（收尾判据 =
     列 0 的右大括号），再去整行 `//` 与行尾 `//` —— 逐条照 `guard_core::production_source`
     / `production_code` 的头注移植，**同形但不是同一份实现**（跨语言够不着）。
     ⇒ 本脚本印出的分母② 若与 cargo 那张申报表对不上，**先怀疑这份剥法**，别先改申报表。

     ⚠ **初版就是在这里错的，登记下来**：第一版写的是「在**第一个** `#[cfg(test)]` 处一刀切」。
     那一刀把 `lib.rs::open_with_os` 与 `ssh_source.rs::resolve_ssh_host` 两处**真生产落点**
     当成测试段丢掉（它们排在文件里第一个测试模块之后），同时把 `session_map.rs` 的
     `#[cfg(all(test, target_os = "linux"))] mod linux_liveness` **整段当成生产段**收进来
     （那个属性不逐字等于 `#[cfg(test)]`）⇒ 分母② 印出 **18**，其中 1 处是测试、2 处漏掉。
     `cfg_is_test_only` 那半（`test` 要是**独立标识符**）与「逐块剥、不一刀切」那半，
     缺哪一半都会这样。

用法：
  python3 evidence/K-R28-spawn-site-census.py            # 印两级分母 + 逐处落点
  python3 evidence/K-R28-spawn-site-census.py --ctx 6    # 每处多印几行上下文
"""
import os
import re
import sys

WT = "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-r28"
ROOTS = ["src-tauri/src", "src-tauri/crates", "remote-daemon-proto/src"]

SPAWN_NEEDLE = "Command::" + "new("           # 运行时拼，免得本文件自己被别的针命中
FN_RE = re.compile(r"(?:^|\s)fn\s+([A-Za-z0-9_]+)")


def rs_files():
    out = []
    for root in ROOTS:
        for dirpath, _, names in os.walk(os.path.join(WT, root)):
            p = dirpath.replace(os.sep, "/")
            if "/vendor/" in p or "/target/" in p:
                continue
            for n in sorted(names):
                if n.endswith(".rs"):
                    out.append(os.path.join(dirpath, n))
    return sorted(out)


def _cfg_is_test_only(attr: str) -> bool:
    """属性里出现 `test` 这个**独立标识符**（照 `guard_core::cfg_is_test_only` 移植）。

    ⇒ `cfg(test)` / `cfg(all(test, …))` / `cfg(any(test, …))` 都算，
      而 `cfg(all(unix, not(target_os = "linux")))` 与 `cfg(feature = "test-utils")` 不算。
    """

    def ident(c: str) -> bool:
        return c.isalnum() or c in "_-"

    k = attr.find("test")
    while k >= 0:
        before_ok = k == 0 or not ident(attr[k - 1])
        after = k + 4
        after_ok = after >= len(attr) or not ident(attr[after])
        if before_ok and after_ok:
            return True
        k = attr.find("test", k + 1)
    return False


def _test_module_ranges(src: str):
    """每个「带花括号体的 `#[cfg(…test…)] mod X { … }`」在 `src` 里的字节区间。

    照 `guard_core::test_module_ranges` 移植，含它那条**承重的**前置检查：
    属性下面那一行必须是 `mod X {`（以左大括号收尾）才是模块体。
    没有它，`#[cfg(test)] mod x;` 这种**无花括号体的声明**会让「列 0 的右大括号」
    一路吞到下一个顶层 item 的收尾，把中间全部生产代码当测试段丢掉。
    """
    open_, close = "\n#[cfg(", "\n}"
    out = []
    i = 0
    while True:
        j = src.find(open_, i)
        if j < 0:
            return out

        def line_end(frm: int) -> int:
            k = src.find("\n", frm)
            return len(src) if k < 0 else k

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_line = src[mod_start:line_end(mod_start)].strip()
        is_test_mod = (
            _cfg_is_test_only(src[attr_start:attr_end])
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        k = src.find(close, j)
        if k < 0:
            out.append((j, len(src)))
            return out
        end = k + len(close)
        out.append((j, end))
        i = end


def production_code(src: str) -> str:
    """与 `guard_core::production_code` 同形的剥法（剥测试模块块 + 去注释）。

    🔴 **保行号**：被剥掉的区间换成等量空行，不删行 —— 否则印出来的行号指不回原文件，
       而「指进本树的行号要带校验位」那条纪律就没东西可核。
    """
    kept_chars = []
    i = 0
    for start, end in _test_module_ranges(src):
        kept_chars.append(src[i:start])
        kept_chars.append("\n" * src[start:end].count("\n"))
        i = end
    kept_chars.append(src[i:])
    stripped = "".join(kept_chars)

    out = []
    for line in stripped.split("\n"):
        if line.lstrip().startswith("//"):
            out.append("")
            continue
        k = line.find("//")
        out.append(line[:k] if k >= 0 else line)
    return "\n".join(out)


def enclosing_fn(lines, at: int) -> str:
    """那一行所在的函数名（往回找最近的 `fn`）—— 逐字照 `write_site_registry::enclosing_fn` 移植：
    先试 `" fn "` 之后那一段，再试整行以 `"fn "` 打头，取到的第一段标识符字符。"""
    for line in reversed(lines[: at + 1]):
        rest = None
        parts = line.split(" fn ")
        if len(parts) > 1:
            rest = parts[1]
        elif line.startswith("fn "):
            rest = line[3:]
        if rest is None:
            continue
        n = ""
        for c in rest:
            if c.isalnum() or c == "_":
                n += c
            else:
                break
        if n:
            return n
    return "<找不到外层函数>"


def spawns_table_keys():
    """`write_site_registry::tests::SPAWNS` 那张申报表里的「文件::函数」（用于对拍）。

    ⚠ 它是**另一份口径**：语料只有 monitor 的 `src/` **加 `build.rs`**，
      不含 `remote-daemon-proto`（daemon 侧同类表在 `readonly_guard`）。
      对拍时要按这个作用域裁，别拿两个分母直接相减。
    """
    path = os.path.join(WT, "src-tauri/src/write_site_registry.rs")
    src = open(path, encoding="utf-8", errors="replace").read()
    body = src.split("const SPAWNS:", 1)
    if len(body) < 2:
        return None
    # 🔴 **必须收在 `];` 那一行**：同一个文件里 `SPAWNS` 后面还跟着 `WRITE_SITES`，
    #    不收口就会把写盘那张表（28 条）一起吞进来 —— 初版正是这样对拍出「只有申报表有的 28 条」。
    tail = body[1].split("\n    ];", 1)[0]
    entries = re.findall(r'\(\s*"([^"]+\.rs)"\s*,\s*"([A-Za-z0-9_]+)"', tail)
    return sorted(set(entries))


def main() -> int:
    ctx = 3
    if "--ctx" in sys.argv:
        ctx = int(sys.argv[sys.argv.index("--ctx") + 1])

    files = rs_files()
    print(f"被测树：{WT}")
    print(f"分母① 三个根下的 `.rs` 文件（去 vendor/target）= {len(files)}")

    hits = []
    for path in files:
        raw = open(path, encoding="utf-8", errors="replace").read()
        prod = production_code(raw)
        plines = prod.split("\n")
        rlines = raw.split("\n")
        for i, line in enumerate(plines):
            if SPAWN_NEEDLE in line:
                hits.append((path, i + 1, enclosing_fn(plines, i), rlines))

    keyed = sorted({(os.path.relpath(p, WT), fn) for p, _, fn, _ in hits})
    print(f"分母② 生产段里起进程的落点（按「文件::外层函数」去重）= {len(keyed)}")
    for f, fn in keyed:
        print(f"    {f}::{fn}")
    print()

    # ── 反向自检：拿 cargo 那张申报表对拍，证明这份剥法没画歪扫描面 ──────────
    table = spawns_table_keys()
    if table is None:
        print("⚠ 对拍不了：`write_site_registry.rs` 里找不到 `const SPAWNS:` —— 表搬走或改名了")
    else:
        mine = {
            (os.path.basename(f), fn)
            for f, fn in keyed
            if f.startswith("src-tauri/src/")
        }
        theirs = {(f, fn) for f, fn in table if f != "build.rs"}
        print(f"【对拍】`SPAWNS` 申报表（去掉 build.rs 那 2 条，它不在本脚本的三个根里）= {len(theirs)}")
        print(f"【对拍】本脚本在 `src-tauri/src/` 下数出的 = {len(mine)}")
        only_mine = sorted(mine - theirs)
        only_theirs = sorted(theirs - mine)
        print(f"【对拍】只有本脚本有的 = {only_mine}")
        print(f"【对拍】只有申报表有的 = {only_theirs}")
        print(
            "【对拍】结论："
            + ("两侧逐条相同 ⇒ 这份剥法与 cargo 那张表同口径" if not only_mine and not only_theirs
               else "🔴 两侧不同 —— 先查剥法与 `enclosing_fn`，别先改申报表")
        )
    print()

    for path, ln, fn, rlines in hits:
        rel = os.path.relpath(path, WT)
        print(f"── {rel}:{ln}  fn {fn} ──")
        for j in range(max(0, ln - 1 - ctx), min(len(rlines), ln + ctx)):
            mark = ">>" if j == ln - 1 else "  "
            print(f" {mark} {j + 1}: {rlines[j].rstrip()}")
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
