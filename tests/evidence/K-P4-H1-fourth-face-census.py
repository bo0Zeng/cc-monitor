#!/usr/bin/env python3
"""K-P4 握手帧拍（`KP4H`）的量具 —— **握手帧第四条面「我做得到什么」落地前后的现打读数**。

住址：`<工作树>/evidence/K-P4-H1-fourth-face-census.py`（默认工作树 `.claude/worktrees/k-p4a`）

它答的是 `D1` 那四问要的读数，外加 `D2` 落地后的自检：

  面①  握手帧今天有几条能力面、各自几条、serde 形态是什么（`skip_serializing_if`）
  面②  **正向表 vs 负向表**在 additive（空则省略）下的默认值真值表
        —— 这一格是 `D1①` 的论据：正向表**结构上**表达不了「我什么都做不到」
  面③  「做不到」的判定来源：`REGISTRY` 里哪些命令登记了 `no_tmux` / `not_installed`
        ⇒ 派生出来的负向表长什么样（证明它**不是手写的第二份真相**）
  面④  生产赋值点：`main.rs` 生产段里 `homes:` / `unavailable:` 各几处、逐字是什么
        ⇒ 「能填不真填」这句话的机器读数
  面⑤  本拍新加的判据在不在（按名字逐条查），以及它们各自钉的是哪一半

⚠ 尺子的射程（说清楚，别读成证明）：
  · 它数的是**源码文本**（正则切块 + 逐字子串），不编译、不跑测试。
    「判据真的会红」那一维由**死值验**给（读数写在件文件 `§8`），不由本脚本给。
  · 面②是**推演**不是测量：它把两种表在 additive 约束下的默认值列出来，
    结论（正向表表达不了「全不可用」）是从「空则省略」这条硬约束推出来的。
  · 面③只证「盘上这几条命令登记了这个 code」，不证「运行时真发得出来」。

跑法：
    python3 evidence/K-P4-H1-fourth-face-census.py
退出码：0 = 每一格都切到了东西；3 = 有格子空转（分母触地板）。
"""

import argparse
import re
import sys
from pathlib import Path

PROJ = Path("/home/zbl/文档/claudecode-frontend")
DEFAULT_WT = PROJ / ".claude" / "worktrees" / "k-p4a"

# ── 地板：低于它就说明尺子没切到东西，本脚本必须报空转而不是报 0 ──────────────
FACE_FLOOR = 3        # 握手帧的能力面至少几条
REG_FLOOR = 5         # REGISTRY 至少几条
GUARD_FLOOR = 3       # 本拍新加的判据至少几条


def read(p: Path) -> str:
    return p.read_text(encoding="utf-8")


def strip_tests(src: str) -> str:
    """粗略剥掉 `#[cfg(test)]` 之后的那个 `mod { ... }`（按大括号配平）。

    ⚠ 与 Rust 侧 `guard_support::production_code` **不是同一个实现**，只求够用：
    本脚本只拿它数「生产段里的赋值点」，而那几行离测块很远。
    """
    out = []
    i = 0
    while True:
        j = src.find("#[cfg(test)]", i)
        if j < 0:
            out.append(src[i:])
            break
        out.append(src[i:j])
        k = src.find("{", j)
        if k < 0:
            break
        depth, m = 0, k
        while m < len(src):
            if src[m] == "{":
                depth += 1
            elif src[m] == "}":
                depth -= 1
                if depth == 0:
                    break
            m += 1
        i = m + 1
    return "".join(out)


def const_items(src: str, name: str):
    """抠出 `const NAME: &[&str] = &[ ... ];` 里的字符串字面量。"""
    m = re.search(r"const\s+" + re.escape(name) + r"\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\];", src, re.S)
    if not m:
        return None
    return re.findall(r'"([^"]+)"', m.group(1))


def hello_faces(wire: str):
    """握手帧里每个 `Vec<_>` 面：(字段名, 元素类型, 有没有 skip_serializing_if)。"""
    m = re.search(r"Hello\s*\{(.*?)\n    \},", wire, re.S)
    if not m:
        return []
    body = m.group(1)
    out = []
    for fm in re.finditer(r"(\w+)\s*:\s*Vec<(\w+)>\s*,", body):
        head = body[: fm.start()]
        skip = "skip_serializing_if" in head.rsplit("///", 1)[-1] or "skip_serializing_if" in head[-400:]
        out.append((fm.group(1), fm.group(2), skip))
    return out


def registry_codes(inbound: str):
    """REGISTRY 里每条命令登记的 codes：name -> [code...]。"""
    m = re.search(r"REGISTRY:\s*&\[CommandSpec\]\s*=\s*&\[(.*?)\n\];", inbound, re.S)
    if not m:
        return {}
    out = {}
    for spec in re.finditer(r"CommandSpec\s*\{(.*?)\n    \}", m.group(1), re.S):
        body = spec.group(1)
        nm = re.search(r'name:\s*"([^"]+)"', body)
        cm = re.search(r"codes:\s*&\[(.*?)\]", body, re.S)
        if nm:
            out[nm.group(1)] = re.findall(r'"([^"]+)"', cm.group(1)) if cm else []
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--wt", default=str(DEFAULT_WT))
    a = ap.parse_args()
    wt = Path(a.wt)
    src = wt / "remote-daemon-proto" / "src"
    wire = read(src / "wire.rs")
    main_rs = read(src / "main.rs")
    inbound = read(src / "inbound.rs")
    doc = read(wt / "doc" / "IPC-PROTOCOL.md")

    bad = []
    print(f"# K-P4 `KP4H` 第四条面 · 现打（工作树 {wt.name}）\n")

    # ── 面① 握手帧的能力面 ────────────────────────────────────────────────
    faces = hello_faces(wire)
    print("## 面① hello 帧的能力面（字段 · 元素类型 · 空则省略）")
    for f, ty, skip in faces:
        print(f"  · {f:<12} Vec<{ty}>  skip_if_empty={skip}")
    caps = const_items(main_rs, "CAPABILITIES") or []
    emits = const_items(main_rs, "EMITS") or []
    cmds = re.findall(r'"([^"]+)"', re.search(r"COMMANDS:\s*&\[&str\]\s*=\s*&\[(.*?)\];", inbound, re.S).group(1))
    print(f"  数：capabilities {len(caps)} · emits {len(emits)} · commands {len(cmds)} · 面总数 {len(faces)}")
    print(f"  capabilities={caps}")
    print(f"  commands={cmds}")
    if len(faces) < FACE_FLOOR:
        bad.append(f"面① 只切到 {len(faces)} 条面（地板 {FACE_FLOOR}）")

    # ── 面② 正向表 vs 负向表的默认值真值表（D1① 的论据）──────────────────
    print("\n## 面② additive（空则省略）下，两种表的默认值各自读成什么")
    print("  | 表的方向 | 空/缺读成 | 「我什么都做不到」表达得出吗 | 旧 daemon 落哪一格 |")
    print("  |---|---|---|---|")
    print("  | 正向（我做得到 X） | 只能是「全都做得到」，否则旧 daemon 全体功能消失 | **表达不出** —— 空集与"
          "「缺字段」逐字节同形 | 与「全做得到」同形 |")
    print("  | 负向（我做不到 X） | 「我没有任何做不到的把握」 | 表达得出（列满即可） | 「没有把握」——"
          "逐字就是今天的语义 |")

    # ── 面③ 判定来源：谁登记了 no_tmux / not_installed ────────────────────
    codes = registry_codes(inbound)
    print(f"\n## 面③ REGISTRY {len(codes)} 条命令各自登记的「这里做不到」类 code")
    if len(codes) < REG_FLOOR:
        bad.append(f"面③ 只切到 {len(codes)} 条命令（地板 {REG_FLOOR}）")
    derived = {}
    for name, cs in sorted(codes.items()):
        marks = [c for c in cs if c in ("no_tmux", "not_installed")]
        print(f"  · {name:<10} codes={len(cs):>2}  这里做不到类={marks}")
        for c in marks:
            derived.setdefault(c, []).append(name)
    print("  ⇒ 从 `codes` **派生**出来的负向表（不是手写的）：")
    for c, names in sorted(derived.items()):
        print(f"      {c}: {names}")
    print("  ⚠ 本拍只接了 `no_tmux` 一件设施：`not_installed` 的调用时判法住在 "
          "`control/cc_bus.rs::find`（私有），本拍写区够不着 ⇒ 自己再写一份就是第二份真相。")

    # ── 面④ 生产赋值点：能填不真填 ────────────────────────────────────────
    prod = strip_tests(main_rs)
    print("\n## 面④ `main.rs` 生产段里的赋值点（能填不真填的机器读数）")
    for field in ("homes", "unavailable"):
        sites = [l.strip() for l in prod.splitlines() if l.strip().startswith(field + ":")]
        print(f"  · {field:<12} {len(sites)} 处：{sites}")
        if len(sites) != 1:
            bad.append(f"面④ `{field}` 赋值点 {len(sites)} 处（应恰好 1 处）")
    for fn in ("fn unavailable_here", "fn unavailable_from", "fn tmux_in"):
        print(f"  · 产出侧 {fn:<24} 在不在生产段：{fn in prod}")

    # ── 面⑤ 本拍的判据 ────────────────────────────────────────────────────
    guards = {
        "production_hello_leaves_unavailable_empty_so_the_wire_bytes_stay_frozen":
            ("main.rs", "生产给的是空表 ⇒ 线上字节不变（真填那天故意变红）"),
        "the_answer_is_a_function_of_the_machine_not_of_the_build":
            ("main.rs", "**不是编译期常量** —— 判定半 + 读世界半各证一半"),
        "the_declared_code_is_one_the_registry_already_declares":
            ("main.rs", "事前说的词 = 事后回的词；REGISTRY 改名会红"),
        "hello_unavailable_is_additive_present_and_absent":
            ("wire.rs", "省略时字节等价旧形 + present 形的精确字节（aterm fixture 真值）"),
    }
    print("\n## 面⑤ 本拍的判据（名字 · 住址 · 各自钉哪一半）")
    hit = 0
    for g, (where, what) in guards.items():
        body = main_rs if where == "main.rs" else wire
        ok = f"fn {g}(" in body
        hit += ok
        print(f"  {'ok ' if ok else 'MISS'} {where:<8} {g}\n            └ {what}")
        if not ok:
            bad.append(f"面⑤ 判据 `{g}` 在 {where} 里找不到")
    if hit < GUARD_FLOOR:
        bad.append(f"面⑤ 只找到 {hit} 条判据（地板 {GUARD_FLOOR}）")

    # ── 文档那一格（护栏要求：字段名要出现在 §10 的代码跨度里）────────────
    s = doc.find("## 10. 远端 daemon wire 协议")
    e = doc.find("\n## ", s)
    sec = doc[s: e if e > 0 else len(doc)]
    idents = set()
    for span in re.findall(r"`([^`]*)`", sec):
        idents.update(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", span))
    print("\n## 面⑥ `doc/IPC-PROTOCOL.md` §10 里有没有这三个名字（`protocol_doc_guard` 要的）")
    for w in ("unavailable", "command", "code"):
        print(f"  · {w:<12} {w in idents}")
        if w not in idents:
            bad.append(f"面⑥ `{w}` 不在 §10 的代码跨度里 ⇒ protocol_doc_guard 会红")

    print("\n" + ("=" * 60))
    if bad:
        print("空转/异常，逐条：")
        for b in bad:
            print("  ✗ " + b)
        return 3
    print("每一格都切到了东西（不代表结论对，只代表尺子没空转）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
