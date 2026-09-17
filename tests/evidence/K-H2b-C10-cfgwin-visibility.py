#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-H2b 第十轮（C 第十轮）量具：`#[cfg(windows)]` 的函数体，**哪一类判据看得见它**。

住址：`evidence/K-H2b-C10-cfgwin-visibility.py`（本件本轮独占的名字，别的轮次别复用）。
被测对象**由 argv 给出**，脚本里没有硬编码任何一棵树 ——
    python3 evidence/K-H2b-C10-cfgwin-visibility.py <launch.rs 的绝对路径>
交回时必须连「喂进去的是哪棵树的哪一份文件」一起写。

# 它量什么

① 复刻 `guard_core::production_code`（剥 `#[cfg(test)] mod X { }` 段 + 剥整行 `//`
   + 剥行尾 `//`，最后一半是 `K-R3` 的 `2474dff` 才补上的）；
② 在剥完的文本里定位 `#[cfg(windows)] pub fn launch_powershell_window` 的**函数体**；
③ 对给定的几个构造，分别数「全文件几处 / 那个体内几处」。

# 它买不到什么（别读宽）

- 它是**文本**尺子，与「哪条测试真的断言了它」是两回事 —— 后者要人去读判据体，
  本脚本只给出「文本上看得见几处」这一半。
- `strip_trailing_comments` 有三种保守情形整行不切（raw/byte string 起头 · 跨行字符串
  内部 · 引号本行不配平）—— 复刻里一并照抄，因此本脚本与真尺子同宽同窄，
  **但两边同时错的可能性没有被排除** ⇒ 下面 `--selftest` 拿真判据钉住的三个数对账。

# 自检

`--selftest` 会把 ① 的产物喂进 `launch.rs` 那条真判据
`every_terminal_window_backend_opens_carries_the_daemon_path` 钉着的三个等号
（`Command::new(` == 4 · `.env(k, v)` == 3 · `daemon_bin_env_for_window(` == 2）。
对不上 ⇒ 复刻坏了，**上面所有读数一律作废**，不许拿去写结论。
"""

import sys


def mask_char_literals(line: str) -> str:
    """把字符字面量换成等长的 `_`（复刻 guard_core::mask_char_literals）。"""
    b = list(line)
    out = list(line)
    i = 0
    n = len(b)
    while i < n:
        if b[i] == "'":
            if i + 3 < n and b[i + 1] == "\\":
                k = None
                for j in range(i + 3, n):
                    if b[j] == "'":
                        k = j
                        break
                if k is not None:
                    for j in range(i, k + 1):
                        out[j] = "_"
                    i = k + 1
                    continue
            if i + 1 < n:
                c = b[i + 1]
                if c != "'" and c != "\\" and i + 2 < n and b[i + 2] == "'":
                    for j in range(i, i + 3):
                        out[j] = "_"
                    i = i + 3
                    continue
        i += 1
    return "".join(out)


def cfg_is_test_only(attr: str) -> bool:
    """`#[cfg(test)]` / `#[cfg(all(test, …))]` 这一族。保守：只认含 `test` 且不含 `not(`。"""
    s = attr.strip()
    return s.startswith("#[cfg(") and "test" in s and "not(" not in s


def test_module_ranges(src: str):
    """复刻 guard_core::test_module_ranges（字节口径改成字符口径，本仓源码里等价用法）。"""
    open_pat = "\n#[cfg("
    close_pat = "\n}"
    out = []
    i = 0
    while True:
        rel = src.find(open_pat, i)
        if rel < 0:
            return out
        j = rel

        def line_end(frm: int) -> int:
            k = src.find("\n", frm)
            return k if k >= 0 else len(src)

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_end = line_end(mod_start)
        mod_line = src[mod_start:mod_end].strip()
        is_test_mod = (
            cfg_is_test_only(src[attr_start:attr_end])
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        rel_end = src.find(close_pat, j)
        if rel_end < 0:
            out.append((j, len(src)))
            return out
        end = rel_end + len(close_pat)
        out.append((j, end))
        i = end


def production_source(src: str) -> str:
    out = []
    i = 0
    for start, end in test_module_ranges(src):
        out.append(src[i:start])
        i = end
    out.append(src[i:])
    return "".join(out)


def strip_trailing_comments(src: str) -> str:
    """复刻 guard_core::strip_trailing_comments（含它三条刻意留着的保守边界）。"""
    out = []
    in_str = False
    for raw in src.split("\n"):
        masked = mask_char_literals(raw)
        if 'r"' in masked or "r#" in masked or 'b"' in masked:
            out.append(raw)
            in_str = False
            continue
        mb = masked
        if in_str:
            i = 0
            while i < len(mb):
                if mb[i] == "\\":
                    i += 2
                    continue
                if mb[i] == '"':
                    in_str = False
                    break
                i += 1
            out.append(raw)
            continue
        i = 0
        cut = None
        while i < len(mb):
            if in_str:
                if mb[i] == "\\":
                    i += 2
                    continue
                if mb[i] == '"':
                    in_str = False
                i += 1
                continue
            if mb[i] == '"':
                in_str = True
                i += 1
                continue
            if mb[i] == "/" and i + 1 < len(mb) and mb[i + 1] == "/":
                cut = i
                break
            i += 1
        out.append(raw[:cut] if cut is not None else raw)
    return "\n".join(out)


def production_code(src: str) -> str:
    kept = "\n".join(
        l for l in production_source(src).split("\n") if not l.lstrip().startswith("//")
    )
    return strip_trailing_comments(kept)


def body_of(prod: str, sig: str):
    """从签名行切到**列 0 的 `}`** —— 顶层 item 的收尾。找不到返回 None。"""
    lines = prod.split("\n")
    start = None
    for i, l in enumerate(lines):
        if sig in l:
            start = i
            break
    if start is None:
        return None
    for j in range(start + 1, len(lines)):
        if lines[j] == "}":
            return "\n".join(lines[start : j + 1])
    return None


# 被数的构造 = `launch.rs` 那条真判据钉着的那三样。
NEEDLES = ["Command::new(", ".env(k, v)", "daemon_bin_env_for_window("]
# 真判据 `every_terminal_window_backend_opens_carries_the_daemon_path` 里的三个等号。
PINNED = {"Command::new(": 4, ".env(k, v)": 3, "daemon_bin_env_for_window(": 2}
WIN_SIG = "pub fn launch_powershell_window(ps_command: &str, local_cwd: Option<&str>)"


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--selftest"]
    selftest = "--selftest" in sys.argv[1:]
    if len(args) != 1:
        print(__doc__)
        return 2
    path = args[0]
    with open(path, encoding="utf-8") as f:
        src = f.read()
    prod = production_code(src)

    print(f"被测对象：{path}")
    print(f"原文 {len(src)} 字节 / {len(src.splitlines())} 行；")
    print(f"production_code 后 {len(prod)} 字节 / {len(prod.splitlines())} 行")

    ok = True
    if selftest:
        print("\n— 自检：复刻的尺子对不对得上真判据钉的三个等号 —")
        for k, want in PINNED.items():
            got = prod.count(k)
            flag = "ok " if got == want else "❌ "
            if got != want:
                ok = False
            print(f"  {flag}{k!r:32} 复刻数 {got}  真判据钉的 {want}")
        if not ok:
            print("\n❌ 复刻与真尺子对不上 ⇒ 下面的读数一律作废。")
            return 1

    body = body_of(prod, WIN_SIG)
    if body is None:
        print(f"\n❌ 在 production_code 文本里找不到 {WIN_SIG!r} 的函数体")
        return 1
    print(f"\n`#[cfg(windows)] launch_powershell_window` 的体（剥完之后）："
          f"{len(body.splitlines())} 行 / {len(body)} 字节")
    print("\n| 构造 | 剥完全文件几处 | 其中在这个 `#[cfg(windows)]` 体内几处 |")
    print("|---|---|---|")
    for k in NEEDLES:
        print(f"| `{k}` | {prod.count(k)} | {body.count(k)} |")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
