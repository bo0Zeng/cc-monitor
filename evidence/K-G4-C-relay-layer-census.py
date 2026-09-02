#!/usr/bin/env python3
"""K-G4 · C 实现拍：`relay/` 层间引用的**加护栏之前**的独立量具。

被测对象（写死住址，别让它跟着 cwd 漂）：
    /home/zbl/文档/claudecode-frontend/.claude/worktrees/k-g4/remote-daemon-proto/src

它量什么
--------
三个方向各数一次「今天违规几处」：

1. `relay/` 生产段里引到 `observe/` · `control/` · `plugin/` · `agents/`
2. `observe/` · `control/` · `plugin/` 生产段里引到 `relay/`
3. `relay/` <-> `plugin/` 互引（方向 1、2 的交集面，单列出来对齐 DoD 的第三支）

它是怎么量的（分母口径，逐条写出来）
------------------------------------
- **分母 = 该层目录树下全部 `.rs` 文件数**（递归），逐文件报。
- 每个文件先剥成「生产段」：抄 `guard_core::production_source` +
  `production_code` 的口径 —— 剥掉列 0 的 `#[cfg(test)] mod X { ... }` 整段、
  剥掉整行 `//` 注释、剥掉行尾 `//` 注释（raw/byte string 那三种保守情形整行不动）。
- 再抄 `layering_guard::refs_to_layer` 的口径找引用：
  `crate::<层>` / `super::super::<层>` 后面跟 `::` 算符号路径、跟别的算模块级引入，
  外加 `use crate::{...}` / `use super::super::{...}` 成组导入单独一路。

⚠ 诚实边界
----------
本量具是 Rust 判据的**独立第二只眼**，不是它的替身：两边各自实现一遍同一个口径，
读数对不上就说明其中一边错了。它同样只看 **import 图**，看不见反射式/字符串式耦合。
"""

import os
import re
import sys

SRC = os.path.join(
    "/home/zbl/文档/claudecode-frontend/.claude/worktrees/k-g4",
    "remote-daemon-proto",
    "src",
)


def test_module_ranges(src):
    """抄 `guard_core::test_module_ranges`：列 0 的 `#[cfg(test)] mod X {` ... `\\n}`。"""
    out = []
    i = 0
    while True:
        rel = src.find("\n#[cfg(", i)
        if rel < 0:
            return out
        j = rel
        attr_start = j + 1
        attr_end = src.find("\n", attr_start)
        if attr_end < 0:
            attr_end = len(src)
        mod_start = min(attr_end + 1, len(src))
        mod_end = src.find("\n", mod_start)
        if mod_end < 0:
            mod_end = len(src)
        mod_line = src[mod_start:mod_end].strip()
        attr = src[attr_start:attr_end]
        cfg_test_only = attr.replace(" ", "") == "#[cfg(test)]"
        if not (cfg_test_only and mod_line.startswith("mod ") and mod_line.endswith("{")):
            i = attr_end
            continue
        close = src.find("\n}", mod_end)
        end = len(src) if close < 0 else close + 2
        out.append((j, end))
        i = end


def production_source(src):
    out = []
    i = 0
    for start, end in test_module_ranges(src):
        out.append(src[i:start])
        i = end
    out.append(src[i:])
    return "".join(out)


def strip_trailing_comments(src):
    """抄 `guard_core::strip_trailing_comments`（含它那三条保守边界）。"""
    out = []
    in_str = False
    for raw in src.split("\n"):
        masked = re.sub(r"'(\\.|[^'\\])'", "''", raw)
        if 'r"' in masked or "r#" in masked or 'b"' in masked:
            out.append(raw)
            in_str = False
            continue
        mb = masked
        if in_str:
            k = 0
            while k < len(mb):
                if mb[k] == "\\":
                    k += 2
                    continue
                if mb[k] == '"':
                    in_str = False
                    break
                k += 1
            out.append(raw)
            continue
        k = 0
        cut = None
        while k < len(mb):
            if in_str:
                if mb[k] == "\\":
                    k += 2
                    continue
                if mb[k] == '"':
                    in_str = False
                k += 1
                continue
            if mb[k] == '"':
                in_str = True
                k += 1
                continue
            if mb[k] == "/" and k + 1 < len(mb) and mb[k + 1] == "/":
                cut = k
                break
            k += 1
        out.append(raw if cut is None else raw[:cut])
    return "\n".join(out)


def production_code(src):
    kept = "\n".join(
        l for l in production_source(src).split("\n") if not l.lstrip().startswith("//")
    )
    return strip_trailing_comments(kept)


def group_names_layer(group, layer):
    """抄 `layering_guard::group_names_layer`。"""
    frm = 0
    while True:
        i = group.find(layer, frm)
        if i < 0:
            return False
        frm = i + len(layer)
        before = group[:i][-1:] if i else ""
        before_ok = before in ("{", ",") or (before != "" and before.isspace())
        tail = group[frm:]
        after_ok = not (tail[:1].isalnum() or tail[:1] == "_")
        if before_ok and after_ok:
            return True


def refs_to_layer(code, layer):
    """抄 `layering_guard::refs_to_layer`。"""
    hits = []
    for root in ("crate::", "super::super::"):
        anchor = root + layer
        frm = 0
        while True:
            i = code.find(anchor, frm)
            if i < 0:
                break
            frm = i + len(anchor)
            tail = code[frm:]
            if tail[:1].isalnum() or tail[:1] == "_":
                continue
            if tail.startswith("::"):
                rest = tail[2:]
                m = re.search(r"[^A-Za-z0-9_:]", rest)
                end = m.start() if m else len(rest)
                sym = "crate::%s::%s" % (layer, rest[:end])
                sym = sym.rstrip(":")
                if sym not in hits:
                    hits.append(sym)
            else:
                mark = anchor + "（模块级引入）"
                if mark not in hits:
                    hits.append(mark)
    for prefix in ("use crate::{", "use super::super::{"):
        frm = 0
        while True:
            i = code.find(prefix, frm)
            if i < 0:
                break
            frm = i + len(prefix)
            stmt = code[i:]
            e = stmt.find(";")
            group = stmt[: e + 1] if e >= 0 else stmt
            if group_names_layer(group, layer):
                mark = group.replace("\n", " ") + "（成组导入）"
                if mark not in hits:
                    hits.append(mark)
    return sorted(hits)


def layer_files(layer):
    root = os.path.join(SRC, layer)
    out = []
    for dirpath, _dirnames, filenames in os.walk(root):
        for fn in filenames:
            if fn.endswith(".rs"):
                p = os.path.join(dirpath, fn)
                rel = os.path.relpath(p, root).replace("\\", "/")
                with open(p, encoding="utf-8") as f:
                    out.append(("%s/%s" % (layer, rel), production_code(f.read())))
    return sorted(out)


def main():
    print("== 被测对象 ==")
    print("  " + SRC)
    print()

    layers = ["relay", "observe", "control", "plugin"]
    counts = {}
    for lay in layers:
        fs = layer_files(lay)
        counts[lay] = len(fs)
        print("== 分母 · %s/ 有 %d 个 .rs ==" % (lay, len(fs)))
        for name, _ in fs:
            print("   " + name)
    print()

    total = 0

    print("== 方向① relay/ 引到别层（不该引的：observe/control/plugin/agents） ==")
    relay = layer_files("relay")
    n = 0
    for name, code in relay:
        for other in ("observe", "control", "plugin", "agents"):
            for sym in refs_to_layer(code, other):
                print("   违规 %s -> %s" % (name, sym))
                n += 1
    print("   命中 %d 处（分母：relay/ %d 个 .rs 的生产段 × 4 个被禁层）" % (n, counts["relay"]))
    total += n
    print()

    print("== 方向② 别处反向引 relay/ 内部 ==")
    n2 = 0
    for lay in ("observe", "control", "plugin"):
        for name, code in layer_files(lay):
            for sym in refs_to_layer(code, "relay"):
                print("   违规 %s -> %s" % (name, sym))
                n2 += 1
    print(
        "   命中 %d 处（分母：observe/ %d + control/ %d + plugin/ %d = %d 个 .rs 的生产段）"
        % (
            n2,
            counts["observe"],
            counts["control"],
            counts["plugin"],
            counts["observe"] + counts["control"] + counts["plugin"],
        )
    )
    total += n2
    print()

    print("== 方向③ relay/ <-> plugin/ 互引 ==")
    n3 = 0
    for name, code in relay:
        for sym in refs_to_layer(code, "plugin"):
            print("   违规 relay->plugin %s -> %s" % (name, sym))
            n3 += 1
    for name, code in layer_files("plugin"):
        for sym in refs_to_layer(code, "relay"):
            print("   违规 plugin->relay %s -> %s" % (name, sym))
            n3 += 1
    print(
        "   命中 %d 处（分母：relay/ %d + plugin/ %d = %d 个 .rs 的生产段）"
        % (n3, counts["relay"], counts["plugin"], counts["relay"] + counts["plugin"])
    )
    print()

    print("== 非空对照：同一把尺子量一条**已知存在**的边（证明它不是没跑） ==")
    ctl = layer_files("control")
    ctl_hits = 0
    for name, code in ctl:
        for sym in refs_to_layer(code, "plugin"):
            print("   对照 %s -> %s" % (name, sym))
            ctl_hits += 1
    print("   对照命中 %d 处（分母：control/ %d 个 .rs 的生产段）" % (ctl_hits, counts["control"]))
    print()

    print("== 总计 ==")
    print("  方向①+② 违规 %d 处；方向③ 违规 %d 处" % (total, n3))
    if ctl_hits == 0:
        print("  🔴 对照面也是 0 —— 尺子本身可能没跑，上面的 0 不作数")
        return 2
    return 0 if total == 0 and n3 == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
