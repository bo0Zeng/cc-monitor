#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R17 实现方的量具：源码里「地址」这一族今天有多少、判得了多少、判不了多少。

用法（**被测对象是命令行给的那棵树，不写死**）：
    python3 evidence/K-R17-C-staleref.py <树的绝对路径> [--dump]

住址与被测对象（brief 12 「量具住址要能唯一定位到那一份」）
------------------------------------------------------------
· 本文件住 `<树>/evidence/K-R17-C-staleref.py`（名字里带件号 + 角色 `C`，
  与 PM 那把 `K-R17-PM-粗尺子.py` **不同名**，不会互相覆盖）。
· 被测对象**由 argv[1] 指定**，本文件里没有任何写死的树路径 ——
  PM 那把尺子写死了主树 `cc-monitor`，照它的住址在工作树上复跑会量到另一棵树。

尺子作用域（先写出来 —— 本仓最高频的一类错就是这个没写）
------------------------------------------------------------
· 分母 = `git -C <树> ls-files '*.rs'`，排 `vendor/` 与 `code-picture-core`。
  ⇒ **只数 git 跟踪的**：磁盘上的生成物 / target 不在分母里。
· 两个人群，形状不同，**分别数**：
    人群甲 `文件.rs:数字`（可带 `-数字` 的区间）—— 本件的正题。
    人群乙 `文件.rs::符号`         —— 08-25 处方要人改成的那个形状。
· `.sh:N` / `.ts:N` / `.md:N` **不在本尺子里**（`capability_registry.rs` 那 6 处
  `.sh:N` 归 `K-R5`）。

★ 与 PM 那把粗尺子的差别（`§3` 手核出它假阳 2/4，这里逐条订正）
------------------------------------------------------------
1. **形 A（引文属于引用方自己）**：粗尺子把地址前后 2 行内**任何**一段
   「…」/ `…` 都当成「对被引行的断言」。`bind.rs:674` 就是这么被误判的
   —— 旁边那个 `ccm-rbind-<sid>` 是引用方自己的话。
   ⇒ 本尺子**不再从散文里猜锚点**。只认**声明式锚点**（见下），猜不到就写「判不了」。
2. **形 B（故意留着的历史反例 / 订正段 / 墓碑）**：粗尺子会去红「记录了这个病的那段话」
   本身（`byte_cap_registry.rs:418` 就是）。
   ⇒ 本尺子只认**显式标记** `〔行号墓碑〕`，不靠关键词猜。
   〔为什么不用关键词：`ratchet_guard.rs` 头注逐字量过同一格 —— 「试跑限定词规则
    **误红 7 处全是合法文本**，把限定词表调到全绿就是曲线拟合」。〕
3. **消歧**：粗尺子按裸文件名找候选，`main.rs` / `lib.rs` / `mod.rs` 各有多个 ⇒ 它报
   「认不出被引文件 23」。本尺子按三步消歧：路径后缀 → 同 crate 优先 → 行号在范围内。
   （实例：`src-tauri/src/main.rs` 只有 8 行 ⇒ 一切 `main.rs:N`（N>8）只能是 daemon 那份。）

声明式锚点的形状（本件立的，人群乙之外唯一能机判的一档）
------------------------------------------------------------
    `路径/文件.rs:123`〔锚 `一段会跟着一起变的原文`〕

只有写成这个形状的地址才进「判得了」这一档：把〔锚 …〕里的原文拿去与被引行 ±3 行比。
**没有这个标记的裸地址一律记「判不了」** —— 不许拿一条 grep 换一个「绿」。

三值口径（brief 17：判不了就明说判不了，不许写成通过）
------------------------------------------------------------
    真馊 / 判得了且不馊 / 判不了 —— 三者相加 = 总命中。
"""

import os
import re
import subprocess
import sys
from pathlib import Path

# 人群甲：`文件.rs:数字[-数字]`
REF_LINE = re.compile(r"([A-Za-z0-9_/.\-]+\.rs):(\d+)(?:-:?(\d+))?")
# 人群乙：`文件.rs::符号`
REF_SYM = re.compile(r"([A-Za-z0-9_/.\-]+\.rs)::([A-Za-z_][A-Za-z0-9_]*)")
# 声明式锚点：`…:123`〔锚 `原文`〕 —— 只认紧跟在地址之后的那一个
ANCHOR = re.compile(r"〔锚\s*[`「]([^`」\n]+)[`」]\s*〕")
# 形 B 的显式标记
TOMB = "〔行号墓碑〕"

# 第三方 crate 的目录形状（`名字-1.2.3/`）—— 它们本来就不在本仓，判不了不是病
VENDORISH = re.compile(r"(^|/)[A-Za-z0-9_.\-]+-\d+\.\d+(\.\d+)?/")

KW = ("fn", "struct", "enum", "const", "static", "trait", "mod", "type")

# 人群乙的**声明式例外**，与 Rust 侧那条判据
# （`structural_scan.rs::every_symbol_address_in_the_sources_still_resolves` 的 `EXCEPTIONS`）
# 一一对应。两处口径必须一致 —— 不一致时以 Rust 那条为准（它才是会在门禁里红的那一份）。
SYM_EXCEPTIONS = {
    "Hello": "`Frame` 的枚举变体，不是一处 item 声明",
    "resolve_claude_dir": "历史句：那句话逐字写的就是「从它原样搬来」",
    "setup": "tauri 的 `.setup(…)` 钩子闭包，不是一处声明",
    "symbol": "示例占位符（讲「地址长什么样」时写的假地址）",
    "foo": "示例占位符（同上）",
}


def tracked_rs(root: Path):
    out = subprocess.run(
        ["git", "-C", str(root), "ls-files", "*.rs"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [f for f in out if not f.startswith("vendor/") and "code-picture-core" not in f]


def read(root: Path, rel: str):
    return (root / rel).read_text(encoding="utf-8", errors="replace")


def strip_comment_lines(src: str) -> str:
    """与 `guard_core::strip_comment_lines` 同口径：`trim_start()` 之后以
    `//` / `*` / `/*` 打头的**整行**换成空行。

    ⚠ 诚实边界（共享原语头注逐字）：它**只剥整行**，块注释里不以这三者打头的内层行
    原样留着。本尺子只拿它数「声明」，多留几行注释里的 `fn foo` 会让判据更宽（更不易误红），
    不会让它漏红。
    """
    out = []
    for ln in src.splitlines():
        t = ln.lstrip()
        out.append("" if t.startswith("//") or t.startswith("*") or t.startswith("/*") else ln)
    return "\n".join(out)


def collect_decls(root: Path, files):
    """符号名 → 它出现在哪些**文件名**里。口径逐字抄
    `doc_claim_registry.rs::every_code_symbol_named_in_the_docs_still_resolves`
    （那条是本仓已有的、同族的、只扫 `doc/` 的判据）。"""
    decl = {}
    srcs = list(files)
    n_extra = 0
    br = root / "src-tauri/build.rs"
    if br.exists():
        srcs.append("src-tauri/build.rs")
        n_extra += 1
    for rel in srcs:
        fname = os.path.basename(rel)
        for line in strip_comment_lines(read(root, rel)).splitlines():
            toks = line.split()
            for i, tok in enumerate(toks[:-1]):
                if tok not in KW:
                    continue
                ident = ""
                for c in toks[i + 1]:
                    if c.isascii() and (c.isalnum() or c == "_"):
                        ident += c
                    else:
                        break
                if ident:
                    decl.setdefault(ident, set()).add(fname)
    return decl, n_extra


def resolve(cited: str, citing: str, bybase, root: Path, want_line=None):
    """三步消歧。返回 (路径, 说明) 或 (None, 为什么解析不到)。"""
    base = os.path.basename(cited)
    cands = bybase.get(base, [])
    if not cands:
        if VENDORISH.search(cited):
            return None, "第三方 crate 路径（不在本仓，本尺子管不到）"
        return None, "被引文件不在树里"
    # ① 路径后缀
    c = [x for x in cands if x.endswith(cited)]
    if len(c) == 1:
        return c[0], "路径后缀唯一"
    c = c or cands
    # ② 同 crate 优先
    crate = citing.split("/src/")[0] if "/src/" in citing else citing.split("/")[0]
    sib = [x for x in c if x.startswith(crate + "/")]
    if len(sib) == 1:
        return sib[0], "同 crate 唯一"
    c2 = sib or c
    # ③ 行号在范围内
    if want_line is not None:
        fit = [x for x in c2 if want_line <= len(read(root, x).splitlines())]
        if len(fit) == 1:
            return fit[0], "只有这一份长到有这一行"
        c2 = fit or c2
    if len(c2) == 1:
        return c2[0], "唯一候选"
    return None, f"{len(c2)} 个同名候选，消歧不了：{c2}"


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    root = Path(sys.argv[1]).resolve()
    dump = "--dump" in sys.argv[2:]
    files = tracked_rs(root)
    bybase = {}
    for f in files:
        bybase.setdefault(os.path.basename(f), []).append(f)
    head = subprocess.run(["git", "-C", str(root), "rev-parse", "--short", "HEAD"],
                          capture_output=True, text=True).stdout.strip()
    branch = subprocess.run(["git", "-C", str(root), "rev-parse", "--abbrev-ref", "HEAD"],
                            capture_output=True, text=True).stdout.strip()
    print(f"# 被测对象：{root}（分支 {branch} @ {head}）")
    print(f"# 分母：{len(files)} 份 git 跟踪的 .rs（排 vendor/ 与 code-picture-core）")

    # ───────────────── 人群甲：`文件.rs:数字`
    stale, ok, unjudged, tomb = [], [], [], []
    total_a = 0
    for f in files:
        lines = read(root, f).splitlines()
        for i, ln in enumerate(lines):
            for m in REF_LINE.finditer(ln):
                total_a += 1
                cited, n = m.group(1), int(m.group(2))
                end = int(m.group(3)) if m.group(3) else n
                where = f"{f}:{i + 1}  引 {cited}:{m.group(2)}"
                # 形 B：显式墓碑标记（同行或前 2 行内）
                blk = "\n".join(lines[max(0, i - 2):i + 1])
                if TOMB in blk:
                    tomb.append((where, "带 " + TOMB + " 标记：故意留着的历史反例，不判"))
                    continue
                tgt, why = resolve(cited, f, bybase, root, want_line=n)
                if tgt is None:
                    unjudged.append((where, why))
                    continue
                tl = read(root, tgt).splitlines()
                if n > len(tl):
                    stale.append((where, f"越界：{tgt} 只有 {len(tl)} 行"))
                    continue
                # 锚点必须**紧跟在地址之后**：同一行的余下部分，或紧接着的两行。
                # （不往前找、也不放宽到 ±2 行 —— 那正是形 A 假阳的来源。）
                tail = ln[m.end():] + "\n" + "\n".join(lines[i + 1:i + 3])
                am = ANCHOR.search(tail)
                if not am:
                    unjudged.append((where, "没有声明式锚点〔锚 `…`〕⇒ 这句引文说的还是不是被引行那件事，读不出来"))
                    continue
                q = am.group(1).strip()
                win = "\n".join(tl[max(0, n - 4):end + 3])
                if q in win:
                    ok.append((where, f"锚点 `{q[:30]}` 在 {tgt}:{n} ±3 内"))
                else:
                    stale.append((where, f"锚点 `{q[:40]}` 不在 {tgt}:{n} ±3 内"))

    # ───────────────── 人群乙：`文件.rs::符号`
    decl, n_extra = collect_decls(root, files)
    sym_total, sym_ok, sym_bad, sym_unres, sym_exc = 0, 0, [], [], []
    for f in files:
        for i, ln in enumerate(read(root, f).splitlines()):
            for m in REF_SYM.finditer(ln):
                sym_total += 1
                cited, sym = m.group(1), m.group(2)
                base = os.path.basename(cited)
                where = f"{f}:{i + 1}  引 {base}::{sym}"
                if base not in bybase:
                    sym_unres.append((where, "被引文件不在树里（示例占位符 / 第三方）"))
                    continue
                if sym in SYM_EXCEPTIONS:
                    sym_exc.append((where, SYM_EXCEPTIONS[sym]))
                    continue
                if sym.endswith("_"):
                    # 前缀形（通配 `build_local_*_command` / 行折 `emit_daemon_`+换行）：
                    # 降级成「那个文件里有某个以它打头的声明」。
                    if any(k.startswith(sym) and base in v for k, v in decl.items()):
                        sym_ok += 1
                    else:
                        sym_bad.append((where, f"前缀 `{sym}` 在 {base} 里找不到任何以它打头的声明"))
                    continue
                homes = decl.get(sym)
                if homes is None:
                    sym_bad.append((where, "**全仓找不到这个符号**（改名或删了）"))
                elif base not in homes:
                    sym_bad.append((where, f"符号还在，但**搬家了**：现住 {sorted(homes)}"))
                else:
                    sym_ok += 1

    # 抽取器自检（要件 3：扫到 0 处 = 尺子失效，不是盘干净了）
    assert len(decl) > 2000, f"只抽到 {len(decl)} 个声明符号 —— 遍历或剥法坏了"
    assert total_a > 0 and sym_total > 0, "两个人群都零命中 —— 正则或分母坏了"

    print()
    print("## 人群甲 `文件.rs:数字`")
    print(f"总命中 {total_a}  =  真馊 {len(stale)}  +  判得了且不馊 {len(ok)}  "
          f"+  形 B 墓碑（不判）{len(tomb)}  +  判不了 {len(unjudged)}")
    print()
    print("## 人群乙 `文件.rs::符号`（08-25 处方要人改成的形状）")
    print(f"总命中 {sym_total}  =  解析得到 {sym_ok}  +  对不上 {len(sym_bad)}  "
          f"+  声明式例外 {len(sym_exc)}  +  被引文件不在树里 {len(sym_unres)}")
    print(f"   （声明面：{len(files)} 份 .rs + build.rs {n_extra} 份 ⇒ 抽到 {len(decl)} 个声明符号）")

    if dump:
        for title, rows in (("真馊（人群甲）", stale), ("判得了且不馊（人群甲）", ok),
                            ("形 B 墓碑，按标记跳过", tomb), ("判不了（人群甲）", unjudged),
                            ("对不上（人群乙）", sym_bad), ("声明式例外（人群乙）", sym_exc),
                            ("被引文件不在树里（人群乙）", sym_unres)):
            print(f"\n### {title}：{len(rows)}")
            for w, why in rows:
                print(f"  {w}  —— {why}")


if __name__ == "__main__":
    main()
