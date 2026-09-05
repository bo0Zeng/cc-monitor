#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-W1C · D1 的尺子：数「拉前缓存」这条链上有几条边，每条边有没有人看着。

住址（唯一）：`evidence/K-W1C-D1-edge-ruler.py`。被测对象由 `--root` 给（默认 = 本文件
所在工作树的仓根）⇒ 交回时连「量于哪棵树、哪个提交」一起写；同一住址下**不许**先后住过
两份被测对象不同的量具（风险 5k：那会给出一次静默的假读数）。

跑法
----
    python3 evidence/K-W1C-D1-edge-ruler.py                     # 全量，出边表
    python3 evidence/K-W1C-D1-edge-ruler.py --only src-tauri/src/utils.rs   # 空转自检
    python3 evidence/K-W1C-D1-edge-ruler.py --root /path/to/tree

退出码：0 = 边表出得来且登记表与盘上逐行对上；2 = 切不到东西（空转）；3 = 登记表与盘上对不上。

尺子怎么切（三个词都承重）
------------------------
一条**边** = 「一个关于会话的**事实**到达」→「一次**缓存写**」的配对。

* **缓存**（闭集，只有一个住址 = 下面的 `CACHE_TYPES`）：`拉前` 那两份 sid → 窗口绑定。
  别的同名东西一概不算 —— `EventReplay::forget`（忘的是回放缓冲）、
  `forget_tmux_raw`（忘的是 `tmux ls` 原文）、`clear_idle`（忘的是灰灯账本）都在人群外。
* **缓存写**：那两个类型的方法里，**真的动那张 `by_sid` 表**的那些（直接 `by_sid.write()`，
  或者调另一个这样的方法）。⚠ 这一栏**不是**写死的名单：本尺子从 Rust 源里
  按 `impl` 块把它们**推出来**（不动点迭代）⇒ 有人新加一个写方法，尺子自己会看见。
* **到达**：那次调用的**调用点**，而且必须是**从缓存外面**打进来的那一次 ——
  接收者必须**静态地**是那两个类型之一（`SidHwndCache::load` / `RemoteHwndCache::new` /
  `State<Arc<…>>` 参数，加上 `.clone()` 传播），**并且**调用点不在那两个 `impl` 块里面。
  🔴 **写方法内部再调一个写方法不是第二条边**（`apply_local_removal` 里那句 `self.forget(…)`、
  `try_bind_with_retry` 里那两句 `self.try_bind(…)`）—— 事实只到达了一次，
  那是同一条边的**内部转调**。第一版尺子没分这一格，把 11 条报了出来，真值 7 条。
  ⇒ 这就是「`.forget(` 出现几次」那把粗尺子**分不开**、而本尺子分得开的那一格。

⚠ **它买不到什么**（逐条写明，别读大）：
1. 按**文本**认接收者 —— 把缓存塞进结构体字段再从字段上调，本尺子看不见（今天零处）。
2. 「这条边**该不该**存在」不判；「判据**够不够**」不判 —— 只判「有没有一条点得出名字的」。
3. 判据那一栏是**人的答案**（`EDGES` 里逐行登记），机器只核三件事：
   ① 登记的判据名在源码里真有一条 `fn <名字>`；② 机检出来的边**全在**登记表里；
   ③ 登记表里的边**今天仍在**盘上。三条任一不成立 ⇒ 退 3，不许静默。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# ── 闭集：只有这一个住址。下面所有「几个 / 全部」的话都拿它现算，不写字面量 ──────
CACHE_TYPES = ("SidHwndCache", "RemoteHwndCache")

# 语料面（分母的第一层）：缓存写只可能出现在 Rust 侧。TS 侧单独量，见 `scan_ts`。
RUST_CORPUS = ("src-tauri/src",)
TS_CORPUS = ("src",)

# ── 人的答案：每条边的「事实」与「判据」。机器核它，不替它答 ─────────────────────
# 键 = (文件相对路径, 写方法名, 锚点子串)；锚点子串必须**逐字**出现在那一行里
# （13c 的校验位：指住址不给裸行号）。
#
# `acceptor` = 今天真的盖着这条边的判据名（空 = 0）。
# `pending`  = 「等某一步接线之后才盖得住」的判据名 + 那一步是什么。
#   ⚠ 本波的写区**不含 `lib.rs`** ⇒ 那两句 `forget` 换成新入口那一步归 PM 落。
#     在它落之前，这两条边的判据栏如实写 **0**，不许写成「已经盖住了」。
EDGES = {
    (
        "src-tauri/src/lib.rs",
        "record",
        "cache_for_emitter.record(sid, info.pid, &bind_for_emitter)",
    ): {
        "kind": "建",
        "fact": "本机 diff 报 added（新会话出现）",
        "cache": "SidHwndCache",
        "acceptor": (),
        "pending": None,
    },
    (
        "src-tauri/src/lib.rs",
        "apply_local_removal",
        "cache_for_emitter.apply_local_removal(&removed)",
    ): {
        "kind": "忘",
        "fact": "本机 diff 报 removed（Gone 或 Superseded）",
        "cache": "SidHwndCache",
        "acceptor": (
            "a_local_session_that_is_gone_gets_forgotten_in_memory_and_on_disk",
            "a_local_session_that_was_superseded_gets_forgotten_too",
        ),
        "pending": None,
    },
    (
        "src-tauri/src/lib.rs",
        "try_bind",
        "if cache.try_bind(&sid)",
    ): {
        "kind": "建",
        "fact": "远端 daemon 帧报 session_added",
        "cache": "RemoteHwndCache",
        "acceptor": (),
        "pending": None,
    },
    (
        "src-tauri/src/lib.rs",
        "apply_remote_disposition",
        "remote_cache_for_emitter.apply_remote_disposition(&sid, &disposition)",
    ): {
        "kind": "忘 / 不忘（按分流裁决）",
        # ⚠ 这条边**盖住两种归宿**：`Archive` ⇒ 忘 · `Idle` ⇒ 不忘。`lib.rs` 这一句是
        # **无条件**调用的，「哪种要忘」整个住在 `apply_remote_disposition` 里 ——
        # 那正是本波要买的东西（判断只有一个住址，且那个住址测得动）。
        "fact": "远端 daemon 帧报 removed，classify_removed 的裁决到达",
        "cache": "RemoteHwndCache",
        "acceptor": (
            "a_remote_session_classified_as_archive_gets_forgotten",
            "a_remote_session_that_only_went_idle_keeps_its_binding",
        ),
        "pending": None,
    },
    (
        "src-tauri/src/lib.rs",
        "forget",
        "cache.forget(&session_id)",
    ): {
        "kind": "忘",
        "fact": "用户点 ↗（Tauri 命令）且旧绑定 verify 失败",
        "cache": "RemoteHwndCache",
        "acceptor": (),
        "pending": None,
    },
    (
        "src-tauri/src/lib.rs",
        "try_bind_with_retry",
        "cache.try_bind_with_retry(",
    ): {
        "kind": "建",
        # ⚠ 这一行**盖住两个调用点**（`bring_remote_terminal_to_front` 里的
        # cache-miss 路与 verify-fail 重绑路，两处逐字同形）⇒ 下面「登记 N 行 ↔ 机检 M 条」
        # 两个数**不等**是正常的，别把它读成漏登记。
        "fact": "用户点 ↗（Tauri 命令）且缓存没命中 / 旧绑定失效",
        "cache": "RemoteHwndCache",
        "acceptor": (),
        "pending": None,
    },
}

# 写方法里**今天生产段零调用点**的那些：必须逐条登记为「入口尚未接线」，
# 否则一个白抽出来、谁也不调的入口会在读数里静默消失。
#
# ⚠ **09-04 接线那一拍清空了它**：`apply_local_removal` / `apply_remote_disposition`
# 两个入口的 `lib.rs` 那两行已经落地（PM 裁定后由实现方落）⇒ 两条登记按本表自己那条
# 反向对账（「已经接上线了 ⇒ 删掉这一行」）删掉。**空不是「没查」，是「今天一个都没有」**：
# 真出现一个零调用点的写方法，下面那道对账会当场退 3。
PENDING_ENTRIES: dict[str, str] = {}


def anchored(text: str, anchor: str) -> bool:
    """锚点是否**在标识符边界上**出现在 `text` 里。

    🔴 **不许用裸 `in`。** 本尺子自己栽过一次：锚点
    `cache_for_emitter.forget(&sid)` 是 `remote_cache_for_emitter.forget(&sid);`
    的子串 ⇒ 远端那条边被认成了本机那条，事实栏当场印出一句**自信的错答案**
    （不是「找不到」，是「找错了」）。本仓那族「子串陷阱」的又一形。
    """
    at = text.find(anchor)
    while at >= 0:
        before = text[at - 1] if at > 0 else " "
        if not (before.isalnum() or before == "_"):
            return True
        at = text.find(anchor, at + 1)
    return False


def _row_of(e: dict) -> dict | None:
    """一条机检出来的边 → `EDGES` 里对应那一行（按「文件 + 写方法 + 逐字锚点」认）。"""
    for k, v in EDGES.items():
        if k[0] == e["file"] and k[1] == e["meth"] and anchored(e["text"], k[2]):
            return v
    return None


def strip_tests(src: str) -> str:
    """把 `#[cfg(test)] mod tests { … }` 之后整段砍掉（本仓 `.rs` 的测试段都在文件尾）。

    ⚠ 它比 `guard_core::production_code` 弱：不剥注释。**故意的** —— 本尺子要认
    「调用点」，而注释里的调用点写法与真调用点长得一样，剥掉注释反而看不见
    「注释里写着一条今天不存在的边」这种病。注释里的命中在输出里单独标 `注释`。
    """
    at = src.find("#[cfg(test)]\nmod tests")
    return src if at < 0 else src[:at]


def impl_blocks(src: str, ty: str) -> str:
    """取 `impl <ty> { … }` 那一块的原文（按大括号配平）。找不到返空串。"""
    m = re.search(r"impl\s+" + re.escape(ty) + r"\s*\{", src)
    if not m:
        return ""
    i = m.end() - 1
    depth = 0
    for j in range(i, len(src)):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[i : j + 1]
    return ""


def methods_of(block: str) -> dict[str, str]:
    """`impl` 块里 `fn <名字>` → 它的函数体原文（到下一个 `fn ` 或块尾）。

    🔴 **同名的多份 cfg 变体要拼起来，不许覆盖。** `try_bind` 有两份
    （`#[cfg(windows)]` 真写 + `#[cfg(not(windows))]` 返 `false`）——
    第一版这里写的是 `out[name] = body`，于是**非 Windows 那份把真写的那份盖掉**，
    `try_bind` 当场从写方法里消失、两条建边静默漏掉。**那正是尺子自己的假读数。**
    """
    out: dict[str, str] = {}
    hits = list(re.finditer(r"\n    (?:pub )?(?:pub\(crate\) )?fn (\w+)", block))
    for k, m in enumerate(hits):
        end = hits[k + 1].start() if k + 1 < len(hits) else len(block)
        out[m.group(1)] = out.get(m.group(1), "") + block[m.start() : end]
    return out


def writers(src: str) -> dict[str, set[str]]:
    """推出每个缓存类型的**写方法**集合：直接动 `by_sid.write()` 的 + 调它们的（不动点）。"""
    out: dict[str, set[str]] = {}
    for ty in CACHE_TYPES:
        ms = methods_of(impl_blocks(src, ty))
        w = {n for n, body in ms.items() if "by_sid.write()" in body}
        changed = True
        while changed:
            changed = False
            for n, body in ms.items():
                if n in w:
                    continue
                if any(f"self.{k}(" in body for k in w):
                    w.add(n)
                    changed = True
        out[ty] = w
    return out


def receivers(src: str) -> dict[str, str]:
    """识别符 → 它静态地是哪个缓存类型。按声明处的类型名认，`.clone()` 传播。"""
    out: dict[str, str] = {}
    for ty in CACHE_TYPES:
        # ① `let x = …SidHwndCache…`（含 `::load(` / `::new()` / `State<Arc<bind::X>>`）
        for m in re.finditer(r"let (?:mut )?(\w+)\s*=\s*[^;]*" + re.escape(ty), src, re.S):
            out[m.group(1)] = ty
        # ② 函数参数 `cache: tauri::State<'_, Arc<bind::X>>`
        for m in re.finditer(r"(\w+)\s*:\s*[^,)]*" + re.escape(ty), src):
            out[m.group(1)] = ty
    # ③ `.clone()` 传播，跑到不动点（`let cache_for_emitter = sid_hwnd_cache.clone();`）
    changed = True
    while changed:
        changed = False
        for m in re.finditer(r"let (?:mut )?(\w+)\s*=\s*(\w+)\.clone\(\)", src):
            dst, srcname = m.group(1), m.group(2)
            if srcname in out and out.get(dst) != out[srcname]:
                out[dst] = out[srcname]
                changed = True
    return out


def scan_rust(root: Path, files: list[Path]):
    """返回（边表, 内部转调表, 每类型的写方法集, 接收者个数）。"""
    all_src = "\n".join(p.read_text(encoding="utf-8") for p in files)
    # 写方法只可能定义在缓存自己住的那个文件里，但推导用全量拼串更稳（impl 块唯一）。
    w = writers(all_src)
    edges: list[dict] = []
    inner: list[dict] = []
    recv_total = 0
    for p in files:
        raw = p.read_text(encoding="utf-8")
        prod = strip_tests(raw)
        recv = receivers(prod)
        recv_total += len(recv)
        lines = prod.splitlines()
        # `impl` 块内部：接收者是 `self`；同时记住「这一行属于哪个方法」——
        # 判「是不是内部转调」要的正是后者。
        self_ty: dict[int, str] = {}
        for ty in CACHE_TYPES:
            blk = impl_blocks(prod, ty)
            if not blk:
                continue
            start = prod[: prod.find(blk)].count("\n")
            for k in range(start, start + blk.count("\n") + 1):
                self_ty[k] = ty
        # 🔴 **按整份文本扫，不按单行扫。**
        # 第一版按行扫，于是 `receiver` 与 `.method(` **换行分开**的那一种写法整个看不见
        # （本仓长调用被折行是常态）。接线那一拍现打逮到活体：
        # `remote_cache_for_emitter\n    .apply_remote_disposition(&sid, &disposition);`
        # 被漏成一条边，而 `apply_remote_disposition` 当场被误报成「入口尚未接线」——
        # 那是一句**自信的错答案**，不是「判不了」。⇒ `\s*` 跨得过换行。
        for m in re.finditer(r"(\w+)\s*\.\s*(\w+)\(", prod):
            who, meth = m.group(1), m.group(2)
            i = prod.count("\n", 0, m.start())  # 接收者所在行（0 基）
            enclosing = self_ty.get(i)
            ty = enclosing if who == "self" else recv.get(who)
            if ty is None or meth not in w[ty]:
                continue
            # 逐字校验位：把这次调用横跨的那几行**折成一行**（多余空白压成一个空格），
            # 折行写法与单行写法于是给出同一个校验位。
            j = prod.count("\n", 0, m.end())
            text = " ".join(" ".join(lines[i : j + 1]).split())
            # 折行处的 `接收者 . 方法(` 再压掉那个空格 ⇒ 校验位读起来就是一行真代码。
            text = re.sub(r"\s*\.\s*", ".", text)
            row = {
                "file": str(p.relative_to(root)),
                "line": i + 1,
                "text": text,
                "meth": meth,
                "cache": ty,
                "comment": lines[i].lstrip().startswith("//"),
            }
            # 🔴 调用点落在这份缓存自己的 `impl` 块里 ⇒ 事实没有在这里到达，
            #    它是同一条边的**内部转调**（第一版尺子把这几处算成了边）。
            (inner if enclosing == ty else edges).append(row)
    return edges, inner, w, recv_total


def scan_ts(root: Path) -> list[str]:
    """TS 侧：缓存写 0 处（缓存整个住后端）⇒ 只报「事实到达口」，即那两个 invoke。"""
    out = []
    for sub in TS_CORPUS:
        d = root / sub
        if not d.is_dir():
            continue
        for p in sorted(d.rglob("*.ts")):
            for i, line in enumerate(p.read_text(encoding="utf-8").splitlines()):
                if re.search(r'invoke<[^>]*>\("bring_(remote_)?terminal_to_front"', line):
                    out.append(f"{p.relative_to(root)}:{i + 1}  {line.strip()}")
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=None, help="仓根（默认：本文件所在工作树）")
    ap.add_argument("--only", default=None, help="只量这一份文件（空转自检用）")
    ap.add_argument("--no-registry", action="store_true", help="只出机检边表，不核登记表")
    a = ap.parse_args()

    root = Path(a.root).resolve() if a.root else Path(__file__).resolve().parents[1]
    if a.only:
        files = [root / a.only]
    else:
        files = []
        for sub in RUST_CORPUS:
            files.extend(sorted((root / sub).rglob("*.rs")))
    files = [p for p in files if p.is_file()]

    print(f"# 量于 {root}")
    print(f"# 缓存闭集（{len(CACHE_TYPES)} 个）：{', '.join(CACHE_TYPES)}")
    print(f"# 语料面：{len(files)} 份 .rs（分母第一层）")

    edges, inner, w, recv_total = scan_rust(root, files)
    all_writers: set[str] = set()
    for ty in CACHE_TYPES:
        got = sorted(w[ty])
        all_writers |= w[ty]
        print(f"# 写方法（推出来的，{len(got)} 个）· {ty}：{', '.join(got) if got else '（无）'}")
    print(f"# 静态认得出的接收者：{recv_total} 处（分母第二层）")

    # ★ 空转自检：切不到东西要退非零，不许静默给 0。
    if not edges:
        print("\n切不到任何一条边 —— 这不是「答案是 0」，是**这把尺子在这份语料上没东西可切**。")
        print("（缓存写只可能出现在那两个类型的 impl 与它们的调用点上；换一份语料再量。）")
        return 2

    prod = [e for e in edges if not e["comment"]]
    print(f"\n## 边表：{len(prod)} 条（注释里的写法 {len(edges) - len(prod)} 处，不算边）\n")
    print("| # | 类 | 缓存 | 写方法 | 住址（带逐字校验位） | 事实 | 今天有判据吗 |")
    print("|---|---|---|---|---|---|---|")
    unmatched: list[str] = []
    for n, e in enumerate(prod, 1):
        meta = _row_of(e)
        if meta is None:
            unmatched.append(f'{e["file"]}:{e["line"]}  {e["text"]}  （方法 {e["meth"]}）')
            fact, acc, kind = "**未登记**", "**未登记**", "?"
        else:
            kind = meta["kind"]
            fact = meta["fact"]
            if meta["acceptor"]:
                acc = "<br>".join(meta["acceptor"])
            elif meta["pending"]:
                step, names = meta["pending"]
                acc = f'**0** —— 接线那一步（{step}）落地后，这条边由 {"、".join(names)} 盖住'
            else:
                acc = "**0**"
        print(
            f'| {n} | {kind} | {e["cache"]} | `{e["meth"]}` | '
            f'`{e["file"]}:{e["line"]}` 逐字 `{e["text"]}` | {fact} | {acc} |'
        )

    covered = sum(1 for e in prod if (_row_of(e) or {}).get("acceptor"))
    print(
        f"\n机检边 {len(prod)} 条 ↔ 登记 {len(EDGES)} 行"
        f"（不等是因为有一行盖住两个逐字同形的调用点，见 EDGES 里那条注释）"
    )
    print(f"**今天**有判据的边：{covered} / {len(prod)}；判据栏写 0 的：{len(prod) - covered}")

    print(f"\n## 内部转调：{len(inner)} 处（写方法里再调一个写方法 —— 事实没在这里到达，不算边）")
    for e in inner:
        print(f'  {e["file"]}:{e["line"]}  {e["text"]}  （在 {e["cache"]} 的 impl 里）')

    called = {e["meth"] for e in prod} | {e["meth"] for e in inner}
    idle = sorted(all_writers - called)
    print(f"\n## 入口尚未接线：{len(idle)} 个写方法在生产段零调用点")
    for name in idle:
        print(f"  {name} —— {PENDING_ENTRIES.get(name, '**未登记**')}")

    inv = scan_ts(root)
    print(f"\n## TS 侧：缓存写 0 处（两份缓存整个住 Rust）；事实到达口 {len(inv)} 处")
    for s in inv:
        print(f"  {s}")

    if a.no_registry:
        return 0

    # ── 登记表对账四条 ────────────────────────────────────────────────
    bad: list[str] = []
    bad += [f"机检到一条边，登记表里没有：{s}" for s in unmatched]
    src_all = "\n".join(p.read_text(encoding="utf-8") for p in files)
    for k, v in EDGES.items():
        hit = any(
            e["file"] == k[0] and e["meth"] == k[1] and anchored(e["text"], k[2]) for e in prod
        )
        if not hit:
            bad.append(f"登记表里这条边今天在盘上找不到了：{k}")
        names = tuple(v["acceptor"] or ()) + tuple((v["pending"] or (None, ()))[1])
        for name in names:
            if f"fn {name}" not in src_all:
                bad.append(f"登记的判据 `{name}` 在源码里没有一条 `fn` —— 名字烂了")
    for name in idle:
        if name not in PENDING_ENTRIES:
            bad.append(f"写方法 `{name}` 生产段零调用点，而 `PENDING_ENTRIES` 里没登记它")
    for name in PENDING_ENTRIES:
        if name in called:
            bad.append(f"`{name}` 已经接上线了 —— 从 `PENDING_ENTRIES` 里删掉这一行，并把对应那条边的判据栏从 0 改成它")
    if bad:
        print("\n🔴 登记表与盘上对不上：")
        for s in bad:
            print(f"  - {s}")
        return 3
    print("\n登记表与盘上逐行对上（机检边 == 登记边，判据名全部解析得到）。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
