#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R3-C1 · 把写区外那条判据 `the_exit_path_really_stops_the_local_backend`
逐条重打一遍（Python 复刻），用来**快速筛**反例；**判决仍以 cargo 的真实输出为准**。

住址（唯一）：<工作树>/evidence/K-R3-C1-verdict-sim.py
被测对象：由 --lib 指定的那一份 `src-tauri/src/lib.rs`（默认取本文件所在工作树的那份）
量于：调用时打印 `git rev-parse HEAD` + 被测文件 md5，别把读数与提交脱钩。

复刻的是 `src-tauri/src/backend/control/local_backend.rs:2170` 那条测试今天的形状
（量于 018b134）。它一共断 **7 处**（PM 派工单 §3 写的是「四样」—— 少了第 7 处，
而第 7 处正是反例要钻的那一处）：

  A1  prod 里三个 needle 都在：RunEvent::Exit · LOCAL_BACKEND · .stop()
  A2  从 RunEvent::Exit 起按花括号配平切出退出臂的体；40 < len < 4000
  A3  find_pinned(body, ".stop()")        —— 恰好一处干净命中
  A4  find_pinned(body, "kill_on_exit(")  —— 恰好一处干净命中
  A5  policy_at < stop_at
  A6  从策略那一行抠得出绑定名（`let <名> = …`），且非空
  A7  body[policy_at..stop_at] 里含 `if <绑定名>`   ← 今天绑定名就是 `kill`
                                                     ⇒ 行为上等价于 contains("if kill")

用法：
  python3 evidence/K-R3-C1-verdict-sim.py                       # 打盘上现状
  python3 evidence/K-R3-C1-verdict-sim.py --mutate M1 --dry     # 只看变异后的判决
  python3 evidence/K-R3-C1-verdict-sim.py --mutate M1 --apply   # 真的写进 lib.rs
  python3 evidence/K-R3-C1-verdict-sim.py --revert              # git checkout 还原 lib.rs
"""
from __future__ import annotations

import argparse
import hashlib
import pathlib
import re
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve()
WT = HERE.parent.parent


# ── guard_core 的三个原语，逐条复刻（口径抄 src-tauri/crates/guard-core/src/lib.rs）──

def cfg_is_test_only(attr: str) -> bool:
    def ident(c: str) -> bool:
        return c.isascii() and (c.isalnum() or c in "_-")

    for m in re.finditer("test", attr):
        k = m.start()
        before_ok = k == 0 or not ident(attr[k - 1])
        after = k + 4
        after_ok = after >= len(attr) or not ident(attr[after])
        if before_ok and after_ok:
            return True
    return False


def test_module_ranges(src: str):
    open_, close = "\n#[cfg(", "\n}"
    out, i = [], 0
    while True:
        rel = src.find(open_, i)
        if rel < 0:
            return out
        j = rel

        def line_end(frm: int) -> int:
            k = src.find("\n", frm)
            return len(src) if k < 0 else k

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_end = line_end(mod_start)
        mod_line = src[mod_start:mod_end].strip()
        if not (cfg_is_test_only(src[attr_start:attr_end])
                and mod_line.startswith("mod ") and mod_line.endswith("{")):
            i = attr_end
            continue
        rel_end = src.find(close, j)
        if rel_end < 0:
            out.append((j, len(src)))
            return out
        end = rel_end + len(close)
        out.append((j, end))
        i = end


def production_source(src: str) -> str:
    out, i = [], 0
    for s, e in test_module_ranges(src):
        out.append(src[i:s])
        i = e
    out.append(src[i:])
    return "".join(out)


def production_code(src: str) -> str:
    """今天盘上那一份（只剔**整行**注释）。"""
    return "\n".join(l for l in production_source(src).splitlines()
                     if not l.lstrip().startswith("//"))


def _ident_char(c: str) -> bool:
    return c.isalnum() or c in "_-"


def find_pinned(hay: str, needle: str):
    """返回 (ok, 偏移或诊断)。口径抄 guard_core::find_pinned。"""
    if not needle:
        return False, "needle 为空"
    first_is_ident = _ident_char(needle[0])
    last_is_ident = _ident_char(needle[-1])
    clean, stretched, frm = [], [], 0
    while True:
        at = hay.find(needle, frm)
        if at < 0:
            break
        end = at + len(needle)
        before_ok = (not first_is_ident) or at == 0 or not _ident_char(hay[at - 1])
        after_ok = (not last_is_ident) or end >= len(hay) or not _ident_char(hay[end])
        if before_ok and after_ok:
            clean.append(at)
        else:
            stretched.append(hay[max(0, at - 10):end + 12].replace("\n", "\\n"))
        frm = at + 1
    if len(clean) == 1:
        return True, clean[0]
    if not clean and not stretched:
        return False, f"`{needle}` 一处都找不到"
    if not clean:
        return False, f"`{needle}` 只出现在被撑大的位置：{stretched}"
    return False, f"`{needle}` 命中 {len(clean)} 处（偏移 {clean}）"


# ── 判据本体，逐条 ──────────────────────────────────────────────────────

def verdict(src: str):
    """返回 (通过?, [逐条读数])。"""
    rows = []
    prod = production_code(src)
    ok = True
    for needle in ("RunEvent::Exit", "LOCAL_BACKEND", ".stop()"):
        hit = needle in prod
        rows.append(("A1 needle " + needle, "在" if hit else "🔴 不在"))
        ok &= hit
    if not ok:
        return False, rows

    arm_at = prod.find("RunEvent::Exit")
    open_ = prod.find("{", arm_at)
    if open_ < 0:
        rows.append(("A2 切臂体", "🔴 找不到块起点"))
        return False, rows
    depth, end = 0, len(prod)
    for i in range(open_, len(prod)):
        if prod[i] == "{":
            depth += 1
        elif prod[i] == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    body = prod[open_:end]
    len_ok = 40 < len(body) < 4000
    rows.append(("A2 臂体字节数", f"{len(body)}" + ("" if len_ok else " 🔴 出界")))
    ok &= len_ok

    got, stop_at = find_pinned(body, ".stop()")
    rows.append(("A3 find_pinned(body, `.stop()`)", stop_at if got else f"🔴 {stop_at}"))
    ok &= got
    got2, policy_at = find_pinned(body, "kill_on_exit(")
    rows.append(("A4 find_pinned(body, `kill_on_exit(`)", policy_at if got2 else f"🔴 {policy_at}"))
    ok &= got2
    if not (got and got2):
        return False, rows

    order_ok = policy_at < stop_at
    rows.append(("A5 policy_at < stop_at", f"{policy_at} < {stop_at} = {order_ok}"
                 + ("" if order_ok else " 🔴")))
    ok &= order_ok

    line = body[:policy_at].splitlines()[-1]
    t = line.lstrip()
    if not t.startswith("let "):
        rows.append(("A6 绑定名", f"🔴 策略那一行不是 `let <名> = …`：{t!r}"))
        return False, rows
    rest = t[4:]
    bind = ""
    for c in rest:
        if c.isalnum() or c == "_":
            bind += c
        else:
            break
    rows.append(("A6 绑定名", repr(bind) + ("" if bind else " 🔴 空")))
    ok &= bool(bind)

    between = body[policy_at:stop_at]
    has = f"if {bind}" in between
    rows.append((f"A7 between 含 `if {bind}`", str(has) + ("" if has else " 🔴")))
    ok &= has
    return ok, rows


# ── 变异 ────────────────────────────────────────────────────────────────
#
# 🔴 每一刀都先断言锚点**恰好命中 1 次**再改（铁律 7），命中数打在输出里。

MUTATIONS = {
    # 题面旧文那把刀，09-01 现打是不是还过得去
    "M1": (
        "                        if kill {\n"
        "                            h.stop();\n"
        "                        }\n",
        "                        if kill && !kill {\n"
        "                            h.stop();\n"
        "                        }\n",
        "旧反例：`if kill && !kill { h.stop(); }`（题面说它已馊，本刀现打）",
    ),
    # 「第三条起法收进缝」那一刀：把 .stop() 从臂里抽走（别处留一处）
    "M2": (
        "                        if kill {\n"
        "                            h.stop();\n"
        "                        }\n",
        "                        if kill {\n"
        "                            stop_supervised_on_exit(h);\n"
        "                        }\n",
        "把被监护那条的收口抽成具名函数（= 收进缝的最小形），臂里不再有 `.stop()`",
    ),
    # 行尾注释那个洞，长在**本件自己**这条判据上
    "M3": (
        "                        if kill {\n"
        "                            h.stop();\n"
        "                        }\n",
        "                        if kill {\n"
        "                            let _ = h.current_pid(); // h.stop();\n"
        "                        }\n",
        "行尾注释喂饱：生产上**不再收**被监护那条，而 `.stop()` 活在行尾注释里",
    ),
    # D4③ 的非空对照：一条**正当的**行尾注释
    "M4": (
        "                        if kill {\n"
        "                            h.stop();\n"
        "                        }\n",
        "                        if kill {\n"
        "                            h.stop(); // 勾了才收；缺省不收见 C8③\n"
        "                        }\n",
        "非空对照：真调用还在，只是尾巴上挂了一条正当注释 ⇒ 必须**不红**",
    ),
}


def lib_path() -> pathlib.Path:
    return WT / "src-tauri" / "src" / "lib.rs"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--lib", default=str(lib_path()))
    ap.add_argument("--mutate", choices=sorted(MUTATIONS))
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--revert", action="store_true")
    a = ap.parse_args()

    p = pathlib.Path(a.lib)
    head = subprocess.run(["git", "-C", str(WT), "rev-parse", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    if a.revert:
        subprocess.run(["git", "-C", str(WT), "checkout", "--", str(p)], check=True)
        src = p.read_text(encoding="utf-8")
        print(f"已还原 {p}；md5={hashlib.md5(src.encode()).hexdigest()}")
        return 0

    src = p.read_text(encoding="utf-8")
    print(f"# 工作树 HEAD = {head}")
    print(f"# 被测 = {p}")
    print(f"# md5(原) = {hashlib.md5(src.encode()).hexdigest()}")

    if a.mutate:
        old, new, why = MUTATIONS[a.mutate]
        n = src.count(old)
        print(f"# 变异 {a.mutate}：{why}")
        print(f"# 锚点命中 = {n} 次（铁律 7：不是 1 就不许切）")
        if n != 1:
            print("🔴 锚点不是恰好一次 —— 拒绝落刀")
            return 3
        src = src.replace(old, new)
        print(f"# md5(变异后) = {hashlib.md5(src.encode()).hexdigest()}")
        if a.mutate == "M2":
            anchor = "pub fn run() {\n"
            assert src.count(anchor) == 1, f"M2 第二处锚点命中 {src.count(anchor)} 次"
            helper = (
                "fn stop_supervised_on_exit(h: &backend::control::local_backend::SuperviseHandle) {\n"
                "    h.stop();\n"
                "}\n\n"
            )
            src = src.replace(anchor, helper + anchor)
            print("# M2 第二处（补具名函数）锚点命中 = 1 次，已落地")
        if a.apply:
            p.write_text(src, encoding="utf-8")
            print("# 变异已落地（写进了工作树）")
        else:
            print("# 变异**未**落地（--dry）")

    ok, rows = verdict(src)
    print()
    for k, v in rows:
        print(f"  {k:44s} {v}")
    print()
    print(f"判决（Python 复刻）= {'绿' if ok else '红'}")
    print("⚠ 这是复刻，不是真值。真值以 `cargo test … the_exit_path_really_stops_the_local_backend` 为准。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
