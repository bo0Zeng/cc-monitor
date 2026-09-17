#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R43 量具：两条「找那个 daemon 二进制」的路各自问了哪几个问题。

住址（唯一）：`<代码仓工作树>/evidence/K-R43-resolution-duty-census.py`
被测对象：**跑它的时候所在的那棵树**（`--tree` 给根，缺省 = 本文件的上一级目录）。
  ⚠ 这一条是 `5k` 逼出来的：同名量具被改成指向另一棵树，输出长得一模一样。
  ⇒ 每次都把 `--tree` 的**绝对路径**印在抬头里。

它答的是什么
------------
`local_backend.rs` 模块头注逐字列了三个问题（`:80`）：
  ① 旁边有没有（`resolve_with` / `resolve_beside_this_exe`）
  ② 这份产物带没带（`native_embedded_daemon`）
  ③ 放不放得下来（`extraction_failure_reason`）
本量具在**两条生产路的函数体**里，逐个数这三问的锚点各出现几次。

分母怎么数的
------------
分母 = **两个函数体**，切法逐字照抄本仓自己那两份切法：
  · `local_backend.rs::tests::start_or_extract_body()` —— 找函数头 → 到**列 0 的那个 `}`** →
    **整行 `//` 注释剥掉**（不剥就恒判「多于一处」，那是它头注写下的理由）。
  · `local_daemon.rs::tests::body_of()` —— 同一套（找头 → 列 0 收尾 → 非空自检）。
本量具**另加**一步：先砍掉 `#[cfg(test)]` 之后的全部文本（相当于 `guard_core::production_code`
的那一半射程），免得函数头的**字符串字面量副本**（判据里写着 `"fn resolve_daemon_bin("`）被切中。

⚠ 射程（写死别读宽）
--------------------
· 它数的是**源码里的字面量**，不是运行时行为 —— 「问了」= 那个名字出现在体内，
  不等于「问对了」，更不等于「真机上跑过」。
· 它只盖**点名的这几个函数体**，不是「全仓所有找二进制的地方」。
· 空刀也是读数：某一格是 0 就是 0，`--expect` 那一格会把它标出来。

回测（`--selftest`）
--------------------
拿一个**已知样本**回测：本仓判据
`local_backend.rs::the_self_extract_path_really_asks_the_product_whether_it_carries_one`
用 `find_pinned` 断言「自释放那条路的体内 `native_embedded_daemon` **恰好一处**」。
⇒ 本量具在同一个体上数，数不出 1 就是尺子坏了，当场报 TOOL-BROKEN。

🔴 **那个「体」是谁，本量具不写死** —— 它现打去 `local_backend.rs` 里读那条判据的切法
（`shared_resolution_body()` / 历史上叫 `start_or_extract_body()` 里那个 `.find("pub fn …(")`）。
理由是实打出来的：`K-R43` 把那三问从 `start_or_extract` 抽进 `resolve_or_extract`，
写死锚点的那一版当场报 TOOL-BROKEN —— **那一次它是对的**（尺子确实指错了地方），
可要是把锚点手动挪过去就成了「改尺子迁就读数」。⇒ 让尺子跟着判据走，谁都不用挪。
"""
import argparse
import pathlib
import re
import sys

# 三问的锚点。`local_backend.rs:80` 那三句话的落点，逐字。
ASKS = [
    ("① 旁边有没有", "resolve_beside_this_exe("),
    ("② 产物带没带", "native_embedded_daemon"),
    ("③ 放不放得下来", "extraction_failure_reason("),
    ("· 真去释放", "extract_embedded_to("),
    ("· 走共用那份", "resolve_or_extract("),
]

# 被测的两条生产路：(人话, 相对路径, 函数头)
LANES = [
    ("常驻这条 local_daemon::resolve_daemon_bin",
     "src-tauri/src/local_daemon.rs", "fn resolve_daemon_bin("),
    ("今天那条 local_backend::start_or_extract",
     "src-tauri/src/backend/control/local_backend.rs", "pub fn start_or_extract("),
    ("共用那份 local_backend::resolve_or_extract",
     "src-tauri/src/backend/control/local_backend.rs", "pub fn resolve_or_extract("),
]

# K-R42 从 `start_or_extract` 里删掉的那一句，逐字（取自 `git show fca80e4` 的 `-` 行）。
THE_INSEPARABLE_SENTENCE = "exe 旁无 sidecar，且释放内嵌 daemon 失败"


def production_half(src: str) -> str:
    """砍掉第一个 `#[cfg(test)]` 之后的全部文本。"""
    at = src.find("\n#[cfg(test)]")
    return src if at < 0 else src[: at + 1]


def cut_body(src: str, head: str):
    """照 `body_of` / `start_or_extract_body` 那套切：找头 → 列 0 的 `}` → 剥整行 `//`。

    回 (body, 诊断)；切不出来回 (None, 为什么)。
    """
    prod = production_half(src)
    hits = prod.count(head)
    if hits == 0:
        return None, f"函数头 `{head}` 在生产段里一处都没有（这一条此刻是空转的）"
    if hits > 1:
        return None, f"函数头 `{head}` 在生产段里 {hits} 处 —— 切法说不清切的是哪一处"
    rest = prod[prod.index(head):].split("\n")[1:]
    try:
        end = rest.index("}")
    except ValueError:
        return None, f"`{head}` 一路切到生产段末尾都没遇上列 0 那一行收尾 —— 窗口无界"
    body = "\n".join(rest[:end])
    if not any(l.strip() for l in body.split("\n")):
        return None, f"`{head}` 切出来的体里一行代码都没有"
    stripped = "\n".join(l for l in body.split("\n") if not l.lstrip().startswith("//"))
    return stripped, None


def judged_head(tree: pathlib.Path):
    """那条判据今天切的是哪个函数体 —— 现打，不写死。

    读的是 `local_backend.rs` 测试段里那个切体助手（`…_body()`）体内的
    `.find("pub fn …(")`。改名 / 换靶都跟得上；读不出来就回 `None`（当场 TOOL-BROKEN）。
    """
    p = tree / "src-tauri" / "src" / "backend" / "control" / "local_backend.rs"
    if not p.is_file():
        return None
    src = p.read_text(encoding="utf-8")
    at = src.find("the_self_extract_path_really_asks_the_product_whether_it_carries_one()")
    if at < 0:
        return None
    # 判据体里那一行 `let body = <助手>();`
    m = re.search(r"let body = (\w+)\(\);", src[at:])
    if not m:
        return None
    hat = src.find(f"fn {m.group(1)}() -> String {{")
    if hat < 0:
        return None
    m2 = re.search(r'\.find\("(pub fn [^"]+)"\)', src[hat:])
    return m2.group(1) if m2 else None


def census(tree: pathlib.Path):
    rows = []
    for who, rel, head in LANES:
        p = tree / rel
        if not p.is_file():
            rows.append((who, rel, head, None, f"文件不在：{p}", {}))
            continue
        body, why = cut_body(p.read_text(encoding="utf-8"), head)
        if body is None:
            rows.append((who, rel, head, None, why, {}))
            continue
        counts = {label: body.count(needle) for label, needle in ASKS}
        rows.append((who, rel, head, len(body), None, counts))
    return rows


def main():
    ap = argparse.ArgumentParser()
    here = pathlib.Path(__file__).resolve().parent
    ap.add_argument("--tree", default=str(here.parent),
                    help="被测对象那棵树的根（缺省 = 本量具的上一级目录）")
    ap.add_argument("--selftest", action="store_true",
                    help="拿已知样本回测这把尺子（见模块头注）")
    a = ap.parse_args()
    tree = pathlib.Path(a.tree).resolve()

    print(f"[K-R43 量具] 被测对象那棵树 = {tree}")
    print(f"[K-R43 量具] 量具自己住 = {pathlib.Path(__file__).resolve()}")
    print()

    rows = census(tree)

    if a.selftest:
        # 已知样本那个体是谁 —— **现打去判据那边读**，不写死（见模块头注）。
        want_head = judged_head(tree)
        if want_head is None:
            print("TOOL-BROKEN 读不出那条判据切的是哪个函数体 —— 切法改了，尺子先别信")
            return 2
        print(f"回测锚点（现打自 local_backend.rs 那条判据的切法）：`{want_head}`")
        for who, rel, head, size, why, counts in rows:
            if head != want_head:
                continue
            if why is not None:
                print(f"TOOL-BROKEN 回测取不到那个已知样本的体：{why}")
                return 2
            got = counts["② 产物带没带"]
            if got != 1:
                print(f"TOOL-BROKEN 已知样本回测没过：`{want_head}` 体内 "
                      f"`native_embedded_daemon` 本仓判据断的是**恰好一处**，本尺子数出 {got}。\n"
                      "  ⇒ 尺子逮不到手里这个已知样本，下面每一个数都不许信。")
                return 2
            print(f"回测过了：已知样本 `{want_head}` 体内 `native_embedded_daemon` = 1 "
                  "（与 `find_pinned` 那条判据同一个数）\n")
            break
        else:
            print("TOOL-BROKEN 回测那一条根本不在被测人群里 —— 人群画错了")
            return 2

    w = max(len(lbl) for lbl, _ in ASKS)
    for who, rel, head, size, why, counts in rows:
        print(f"── {who}")
        print(f"   住 {rel} · 函数头 `{head}`")
        if why is not None:
            print(f"   【切不出】{why}")
            print()
            continue
        print(f"   体 {size} 字节（剥掉整行 `//` 之后）")
        for lbl, needle in ASKS:
            n = counts[lbl]
            mark = "  ← 一处都没有" if n == 0 else ""
            print(f"   {lbl.ljust(w)}  `{needle}` × {n}{mark}")
        print()

    # 那一句「分不开的话」今天还住在哪儿 —— 全树现打，分母 = `src-tauri/src` 下全部 `.rs`
    print(f"── `K-R42` 从 `start_or_extract` 删掉的那一句今天还住在哪儿")
    print(f"   逐字：{THE_INSEPARABLE_SENTENCE}")
    root = tree / "src-tauri" / "src"
    files = sorted(root.rglob("*.rs"))
    hits = []
    for p in files:
        for i, l in enumerate(p.read_text(encoding="utf-8").split("\n"), 1):
            if THE_INSEPARABLE_SENTENCE in l:
                kind = "注释" if l.lstrip().startswith("//") else "生产串"
                hits.append((p.relative_to(tree), i, kind, l.strip()))
    print(f"   分母 = {root} 下 {len(files)} 个 `.rs`（现打，含测试段）")
    if not hits:
        print("   零命中")
    for rel, ln, kind, text in hits:
        print(f"   {rel}:{ln} [{kind}] {text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
