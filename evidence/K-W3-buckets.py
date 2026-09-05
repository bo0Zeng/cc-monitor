#!/usr/bin/env python3
"""K-W3 `KW3D1` 三桶 + 「没判」桶 —— 按文件切（切法 ①），每条归桶规则带盘上住址。

只读。被测对象 = 本量具所在工作树（住址口径同 `evidence/K-W3-ruler.py`）。
三把尺由 `K-W3-ruler.py` 提供（双口径 p / k）；本量具只做**归桶**，不重新定义尺子。

## 为什么选切法 ①（按文件）—— 三条候选里选它的理由，读数在 `report()` 里现打

- 切法 ③（按 `#[tauri::command]` 调用面）的字面口径是「**只被 tauri 命令调用**、且 daemon
  侧不需要的 ⇒ UI 外壳」⇒ 它要一张**调用图**。本件是零代码摸底件，手上没有一把
  作用域核过的调用图量具 ⇒ 照字面做会产出一个下一轮复不出的数。
  退化成「文件里有没有命令」这一维时，现打：44 个有命令的文件占尺C-k 的 **71.6%**
  （16849 / 23529）⇒ 它把七成分母留在同一堆里，**买不到分桶**，而那正是切法 ① 被
  批评的那条病（切不动大文件）。
- 切法 ②（按顶层项）要 Rust AST。沙箱里 Python 的 `ast` 只吃 Python，无 rust parser ⇒ 同上。
- 切法 ①（按文件）**今天就能跑、可复跑、口径唯一**，而且 `§0-2` 钉住的判据面那一桶
  口径本来就是按文件（`*_registry.rs`）⇒ 三桶用同一粒度的尺，不混口径。
  它切不动的那几份**整份进「没判」桶**并逐条点名 —— `KW3D1` 逐字允许
  「差额单列一桶叫『没判』」，那一桶就是「下一件要按项切的活」的清单。

## 归桶规则（每条给住址；没有住址的一律进「没判」）

R1 桶3 判据面（`§0-2` 钉住的口径，不改）：文件名 `*_registry.rs`。
R1b 同形但**不在**钉住口径里的，**单列一行、不折进桶3**：
    `*_guard.rs` · `*_parity.rs` · `structural_scan.rs` · `parity_ledger.rs`。
R2 桶1 该进合并后的后端：
   R2a `src-tauri/src/backend/**` —— 它自己就是「宿主无关的后端那半」，
       由判据 `backend::tests::the_backend_layer_stays_host_agnostic` /
       `the_backend_half_stays_platform_agnostic` 钉着（住址 `src-tauri/src/backend/mod.rs`）。
   R2b 五对同名同职的**宿主侧**实现（`KG1` 成功标准① 点名的历史/用量/账号/监视/搜索）。
       ⚠ 名单比件文件 `§0-3` 那张表多一个 `usage.rs`，理由见 `report()` 的现打读数。
   R2c `local_read_surface_registry.rs` 的 `reader` 档点名的文件（今天 8 条）——
       那是「monitor 还在自己读 `~/.claude`」的机器读数，退役归属逐条写在表里。
R3 桶2 留宿主壳（两条来源，都指盘上的住址，扣掉已进桶1 的）：
   R3a 生产段命中 `src-tauri/src/backend/mod.rs::platform_needles()`（11 支平台针）
       或同文件 `FORBIDDEN`（7 支 GUI 宿主把手）—— **针集逐字取自那两个函数**，不是本件发明的。
       盘上自陈的理由（`backend/mod.rs:406-407` 逐字）：「另一半（`bind.rs` 的窗口把手 ·
       `launch.rs` 的开窗 · `session_map.rs` 的进程身份）是 **C9** 的活：
       『在用户桌面上开一个终端窗口』本身就是平台特定的」。
   R3b `local_read_surface_registry.rs` 里**逐字写着「切后端之后仍要在 / 不属退役范围」**
       那几档点名的文件：`hub` · `payload` · `fence` · `non-read` · `write`。
   🔴 **`remote` 档刻意不进 R3b** —— 把整条读完（铁律 13）：它逐字说的是
       「**根本不是本机读面** ⇒ 不属 F10」，那是**射程**声明，不是**归属**声明。
       拿它当「该留宿主壳」用是只引方便的那半。⇒ `ssh_source.rs` 因此进桶0，不进桶2。
R0 桶0 没判：以上都点不到的文件。

⚠ **按文件切的近似，逐条登记，不藏**：`local_read_surface_registry` 是**按处**分类的，
  同一个文件可以同时挂几档（`history.rs` 一个文件挂了 `no-counterpart` 2 · `fence` 2 ·
  `write` 4 · `payload` 5 四档共 11 处）⇒ 它进桶1 是**近似**，那 11 处要按项才切得开。
  `report()` 把这类文件与它们的处数逐条印出来。

用法：python3 evidence/K-W3-buckets.py
"""
from __future__ import annotations

import importlib.util
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("kw3ruler", os.path.join(HERE, "K-W3-ruler.py"))
R = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(R)

# R2b —— 五对同名同职的宿主侧实现。`usage.rs` 是本件加上去的，理由见 report()。
PAIRS = {
    "历史": (["src-tauri/src/history.rs"],
             ["remote-daemon-proto/src/observe/history_query.rs"]),
    "用量": (["src-tauri/src/usage.rs", "src-tauri/src/account_usage.rs"],
             ["remote-daemon-proto/src/observe/usage_query.rs"]),
    "账号": (["src-tauri/src/accounts.rs", "src-tauri/src/local_accounts.rs"],
             ["remote-daemon-proto/src/observe/accounts_query.rs"]),
    "监视": (["src-tauri/src/watcher.rs"],
             ["remote-daemon-proto/src/observe/watcher.rs"]),
    "搜索": (["src-tauri/src/search.rs"],
             ["remote-daemon-proto/src/observe/search_query.rs"]),
}

# 走盘针（`needle` 子命令同一批）：用来判「这一侧还有没有自己一份遍历实现」。
# ⚠ 射程：数的是含子串的行，注释提到也算 ⇒ 它证「还提这件事」，不证「还实现着」。
DISK_NEEDLES = ["read_dir", "WalkDir", "File::open", "BufReader", "read_to_string"]

REG_PATH = "src-tauri/src/local_read_surface_registry.rs"
REG_ROW = re.compile(
    r'\(\s*\n\s*"([^"]+)",\s*\n\s*'
    r'"(hub|reader|payload|remote|fence|write|non-read|no-counterpart)",\s*\n\s*(\d+),', re.M)

# R3b：只收「逐字写着切后端之后仍要在 / 不属退役范围」那几档。`remote` 档见模块头注。
STAY_KINDS = {"hub", "payload", "fence", "non-read", "write"}

# R3a：针集**逐字**取自 `src-tauri/src/backend/mod.rs` 的 `platform_needles()` 与 `FORBIDDEN`。
# ⚠ 那两个函数为了不命中自己是**运行时拼串**的 ⇒ 这里只能抄一份。抄来的那份会漂
#    ⇒ `check_needle_provenance()` 每次跑都回去核「这 18 支在那个文件里逐字都还在」。
PLATFORM_NEEDLES = [
    "#[cfg(windows)", "#[cfg(unix)", "#[cfg(not(windows)", "#[cfg(not(unix)",
    'target_os = "', "target_family", "libc::", "std::os::unix", "std::os::windows",
    "windows_sys::", "winapi::",
]
GUI_NEEDLES = ["AppHandle", "tauri::Window", "WebviewWindow", "State<",
               ".emit(", "Emitter", "Manager"]
# 那两个函数里针是拼出来的，核的时候要按拼法找它们的**片段**。
NEEDLE_PROVENANCE = [
    ("src-tauri/src/backend/mod.rs", 'format!("#[{cfg}(windows)")'),
    ("src-tauri/src/backend/mod.rs", 'format!("{}_os = \\"", "target")'),
    ("src-tauri/src/backend/mod.rs", '"libc::".to_string()'),
    ("src-tauri/src/backend/mod.rs", 'format!("std::os::{}", "unix")'),
    ("src-tauri/src/backend/mod.rs", '"winapi::".to_string()'),
    ("src-tauri/src/backend/mod.rs", '"WebviewWindow"'),
    ("src-tauri/src/backend/mod.rs", '"AppHandle"'),
]
JUDGE_EXTRA_SUFFIX = ("_guard.rs", "_parity.rs")
JUDGE_EXTRA_NAMES = ("structural_scan.rs", "parity_ledger.rs")


def agg(per, sel):
    t = {"files": 0, "a": 0, "cp": 0, "ck": 0, "bk": 0}
    for f in sel:
        t["files"] += 1
        t["a"] += per[f]["p"]["a"]
        t["cp"] += per[f]["p"]["c"]
        t["ck"] += per[f]["k"]["c"]
        t["bk"] += per[f]["k"]["b"]
    return t


def line(name, t, denom_ck):
    pct = 100.0 * t["ck"] / denom_ck if denom_ck else 0.0
    return (f"{name}: 文件 {t['files']:3d} · 尺A {t['a']:6d}"
            f" · 尺C-p {t['cp']:6d} · 尺C-k {t['ck']:6d}（占分母 {pct:5.1f}%）")


def prod_hits(root, rel, needles):
    text = R.read_text(root, rel, None)
    lines = R.split_lines(text)
    is_test = R.mark_test(lines, "k")
    prod = [l for i, l in enumerate(lines) if not is_test[i]]
    return {n: sum(1 for l in prod if n in l) for n in needles}


def check_needle_provenance(root) -> int:
    """核「抄来的那 18 支针」在它的家里逐字都还在。抄的东西会漂 ⇒ 每次跑都回去核一遍。"""
    bad = 0
    for rel, frag in NEEDLE_PROVENANCE:
        src = R.read_text(root, rel, None)
        ok = frag in src
        if not ok:
            bad += 1
        print(f"  {'OK ' if ok else 'BAD'} {rel} 里 {frag!r}")
    print(f"  [针来历] BAD={bad} · 分母 = 抽查 {len(NEEDLE_PROVENANCE)} 条"
          f"（**不是** 18 支全核；给不出全核的分母就只写抽查了这几条）")
    return bad


def report() -> int:
    root = R.repo_root(None)
    rev = None
    head = os.popen(f"git -C {root!r} rev-parse --short HEAD").read().strip()
    files = R.list_rs(root, "src-tauri/src", rev)
    per = {}
    for f in files:
        t = R.read_text(root, f, rev)
        per[f] = {"p": R.measure(t, "p"), "k": R.measure(t, "k")}
    all_t = agg(per, files)
    denom = all_t["ck"]

    print(f"===== K-W3 KW3D1 三桶 · 量于 {head} · 分母 = src-tauri/src 全部 .rs =====")
    print(line("分母（全体）", all_t, denom))
    print(f"  （同一分母的钉住口径 尺C-p = {all_t['cp']}；本件订正口径 尺C-k = {all_t['ck']}）")
    print()

    # ---- 桶3
    b3 = [f for f in files if os.path.basename(f).endswith("_registry.rs")]
    b3x = [f for f in files
           if os.path.basename(f).endswith(JUDGE_EXTRA_SUFFIX)
           or os.path.basename(f) in JUDGE_EXTRA_NAMES]
    # ---- 桶1
    r2a = [f for f in files if f.startswith("src-tauri/src/backend/") and f not in b3 + b3x]
    r2b = [h for _k, (hs, _d) in PAIRS.items() for h in hs]
    reg_src = R.read_text(root, REG_PATH, rev)
    rows = REG_ROW.findall(reg_src)
    reader_files = sorted({"src-tauri/" + p for p, k, _n in rows if k == "reader"})
    stay_files = sorted({"src-tauri/" + p for p, k, _n in rows if k in STAY_KINDS})
    r2c = [f for f in reader_files if f in files]
    b1 = sorted(set(r2a) | {f for f in r2b if f in files} | set(r2c))
    # ---- 桶2
    r3a = sorted(f for f in files
                 if any(prod_hits(root, f, PLATFORM_NEEDLES + GUI_NEEDLES).values()))
    r3b = sorted(f for f in stay_files if f in files)
    b2 = sorted((set(r3a) | set(r3b)) - set(b1) - set(b3) - set(b3x))
    # ---- 桶0
    b0 = [f for f in files if f not in set(b1) | set(b2) | set(b3) | set(b3x)]

    print(line("桶1 该进合并后的后端 (R2a+R2b+R2c)", agg(per, b1), denom))
    print(line("  ├ R2a src-tauri/src/backend/**   ", agg(per, r2a), denom))
    print(line("  ├ R2b 五对·宿主侧(含 usage.rs)   ", agg(per, [f for f in r2b if f in files]), denom))
    print(line("  └ R2c reader 档点名(去重后新增)   ",
               agg(per, [f for f in r2c if f not in r2a and f not in r2b]), denom))
    print(line("桶2 留宿主壳 (R3a+R3b)            ", agg(per, b2), denom))
    print(line("  ├ R3a 平台针/GUI 把手(去桶1 后) ",
               agg(per, [f for f in r3a if f in b2]), denom))
    print(line("  └ R3b registry「仍要在」档新增   ",
               agg(per, [f for f in r3b if f in b2 and f not in r3a]), denom))
    print(line("桶3 判据面 *_registry.rs (R1·钉住)", agg(per, b3), denom))
    print(line("桶0 没判 (R0)                     ", agg(per, b0), denom))
    s = agg(per, b1 + b2 + b3 + b3x + b0)
    print(f"四桶+R1b 合计减全体（应全为 0）: "
          + " · ".join(f"{k}={s[k] - all_t[k]}" for k in ("files", "a", "cp", "ck")))
    print()
    print(line("[R1b 单列·同形但不在钉住口径里]   ", agg(per, b3x), denom))
    for f in sorted(b3x):
        print(f"      尺C-k {per[f]['k']['c']:5d} · 尺A {per[f]['p']['a']:5d}  {f}")
    print()

    print("----- 桶1 逐文件（尺C-k 降序）")
    for f in sorted(b1, key=lambda x: -per[x]["k"]["c"]):
        src = []
        if f in r2a:
            src.append("R2a")
        if f in r2b:
            src.append("R2b")
        if f in r2c:
            src.append("R2c")
        print(f"   {per[f]['k']['c']:6d} {per[f]['p']['a']:6d}  {'+'.join(src):11s} {f}")
    print()
    print("----- 桶2 逐文件（尺C-k 降序；档来自 local_read_surface_registry）")
    kinds = {}
    for p, k, n in rows:
        kinds.setdefault("src-tauri/" + p, []).append(f"{k}:{n}")
    for f in sorted(b2, key=lambda x: -per[x]["k"]["c"]):
        print(f"   {per[f]['k']['c']:6d} {per[f]['p']['a']:6d}  {','.join(kinds.get(f, []))}  {f}")
    print()
    print("----- 桶0「没判」逐文件（尺C-k 降序）—— 这就是「下一件要按项切」的清单")
    for f in sorted(b0, key=lambda x: -per[x]["k"]["c"]):
        print(f"   {per[f]['k']['c']:6d} {per[f]['p']['a']:6d}  {f}")
    print()

    print("----- 针来历自核（抄来的针会漂）")
    check_needle_provenance(root)
    print()

    print("----- 按文件切的近似①：进了桶1、但生产段也带平台针/GUI 把手的文件")
    print("      （= 这一份里既有该进后端的实现、又有只能留在宿主壳的那几行 ⇒ 按项才切得开）")
    for f in sorted(b1, key=lambda x: -per[x]["k"]["c"]):
        h = {k: v for k, v in prod_hits(root, f, PLATFORM_NEEDLES + GUI_NEEDLES).items() if v}
        if h:
            print(f"   尺C-k {per[f]['k']['c']:5d}  {f}  ← {h}")
    print()

    print("----- 按文件切的近似②，逐条登记：同一文件挂多档的")
    multi = {f: ks for f, ks in kinds.items() if len({k.split(':')[0] for k in ks}) > 1}
    for f, ks in sorted(multi.items()):
        b = "桶1" if f in b1 else ("桶2" if f in b2 else "桶0/桶3")
        n = sum(int(k.split(':')[1]) for k in ks)
        print(f"   {b} {f}: {','.join(ks)} —— 共 {n} 处，按项才切得开")
    print()

    print("----- 切法 ③ 的现打代价（选切法 ① 的第一条理由）")
    withcmd = [f for f in files if "#[tauri::command]" in R.read_text(root, f, rev)]
    tw = agg(per, withcmd)
    print(f"   有 #[tauri::command] 的文件 {tw['files']} · 尺C-k {tw['ck']}"
          f" ⇒ 占分母 {100.0 * tw['ck'] / denom:.1f}%（那一堆里就是还要再切的）")
    print()

    print("----- 五对今天各自在哪一档（走盘针只数生产段；射程见 DISK_NEEDLES 注释）")
    for name, (hs, ds) in PAIRS.items():
        hh = sum(sum(prod_hits(root, h, DISK_NEEDLES).values()) for h in hs if h in files)
        dd = sum(sum(prod_hits(root, d, DISK_NEEDLES).values()) for d in ds)
        hck = sum(per[h]["k"]["c"] for h in hs if h in files)
        dck = 0
        for d in ds:
            dck += R.measure(R.read_text(root, d, rev), "k")["c"]
        verdict = "宿主侧已无走盘 ⇒ 这一对已消（宿主只剩包装）" if hh == 0 else "两侧各一份 ⇒ 真重复"
        print(f"   {name}: 宿主 {len(hs)} 文件 尺C-k {hck} 走盘针 {hh}"
              f" | daemon 尺C-k {dck} 走盘针 {dd}  ⇒ {verdict}")
    return 0


if __name__ == "__main__":
    sys.exit(report())
