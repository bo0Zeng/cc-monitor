#!/usr/bin/env python3
# K-R14 · C 拍量具 —— 「反空转自检」的阈值余量表 + 同形自检普查
#
# 住址（唯一）：<工作树>/evidence/K-R14-C-ratio-margins.py
# 被测对象：由 --root 指定，**默认不猜** —— 必须显式给工作树绝对路径。
#           （brief 第 12 条：量具住址要能唯一定位到那一份，且要写明被测对象指向哪棵树。）
#
# 用法：
#   python3 evidence/K-R14-C-ratio-margins.py --root <工作树绝对路径> [--json]
#
# 它做三件事：
# ★★ 时点：`margins` / `ratio_table` / `census` 里写死的**行号与判据字面**都是
#    **量于主干尖 `018b134`（K-R14 动手之前）**的快照 —— 本件正是要改掉其中两条，
#    改完之后行号会漂。要复跑请先 `git checkout 018b134 -- src-tauri/src/polling_registry.rs`
#    到一份副本上，或只信这里的**函数名**（`the_identity_poller_is_gone_for_good` /
#    `the_scan_actually_reads_the_frontend_and_ccm`），别信行号。
#
#   1) margins  : 把 guard-core 的两个剥法在 Python 里逐字复刻，量 shared/ccm 上
#                 那两条比例判据的**当前余量**（字节），并给「再加几个汉字就红」。
#                 ⚠ 复刻已对过真 Rust：`strip_hash_comment_lines` 22849 == 真 Rust 22849，
#                 `production_code(tmux_reconcile.rs)` 1213 == 真 Rust 1213（两次都由
#                 「把阈值改瞎、读 panic 里印出的真数」取得，不是自证）。
#   2) census   : 全仓普查「同形自检」，两种切法各自给分母与逐条判定。
#   3) simulate : 模拟往一份 ccm 副本里追加注释，打印判据翻红的那一刻。
#
# ⚠ 本量具**只读**，不写被测树的任何文件。

import argparse
import json
import os
import re
import sys

# ─────────────────────────────────────────────────────────────────────────────
# 一 · 把 Rust 侧的原语逐字复刻（口径要对得上，否则量的是另一件事）
# ─────────────────────────────────────────────────────────────────────────────

# Rust `str::lines()`：按 '\n' 切；每段末尾的 '\r' 去掉；末尾那个空段不产出。
def rust_lines(s: str):
    parts = s.split("\n")
    if parts and parts[-1] == "":
        parts.pop()
    return [p[:-1] if p.endswith("\r") else p for p in parts]


# Rust `str::trim_start()` 去的是 Unicode 空白。Python 的 str.lstrip() 口径足够近，
# 但显式列出常见的几个，免得在全角空格上口径分岔。
_WS = " \t\r\n\x0b\x0c                 　"


def trim_start(s: str) -> str:
    return s.lstrip(_WS)


# guard_core::strip_hash_comment_lines —— filter：整行**丢掉**（连同它的 join 分隔符）
def strip_hash_comment_lines(src: str) -> str:
    return "\n".join(l for l in rust_lines(src) if not trim_start(l).startswith("#"))


# guard_core::strip_comment_lines —— map：换成空串，**行数不变**
def strip_comment_lines(src: str) -> str:
    out = []
    for l in rust_lines(src):
        t = trim_start(l)
        out.append("" if (t.startswith("//") or t.startswith("*") or t.startswith("/*")) else l)
    return "\n".join(out)


def blen(s: str) -> int:
    """Rust `String::len()` 是 **UTF-8 字节数**，不是字符数。"""
    return len(s.encode("utf-8"))


# guard_core::cfg_is_test_only —— `test` 作为独立标识符出现过
def _cfg_is_test_only(attr: str) -> bool:
    b = attr.encode("utf-8")

    def ident(c):
        return (48 <= c <= 57) or (65 <= c <= 90) or (97 <= c <= 122) or c in (95, 45)

    k = 0
    while True:
        k = b.find(b"test", k)
        if k < 0:
            return False
        before_ok = k == 0 or not ident(b[k - 1])
        after = k + 4
        after_ok = after >= len(b) or not ident(b[after])
        if before_ok and after_ok:
            return True
        k += 1


# guard_core::test_module_ranges —— 全程按**字节**下标走（Rust 的 str 下标就是字节）
def _test_module_ranges(src_b: bytes):
    open_m = b"\n#[cfg("
    close_m = b"\n}"
    out = []
    i = 0
    while True:
        rel = src_b.find(open_m, i)
        if rel < 0:
            return out
        j = rel

        def line_end(frm):
            k = src_b.find(b"\n", frm)
            return k if k >= 0 else len(src_b)

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src_b))
        mod_end = line_end(mod_start)
        mod_line = src_b[mod_start:mod_end].decode("utf-8", "replace").strip()
        is_test_mod = (
            _cfg_is_test_only(src_b[attr_start:attr_end].decode("utf-8", "replace"))
            and mod_line.startswith("mod ")
            and mod_line.endswith("{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        rel_end = src_b.find(close_m, j)
        if rel_end < 0:
            out.append((j, len(src_b)))
            return out
        end = rel_end + len(close_m)
        out.append((j, end))
        i = end


def production_source(src: str) -> str:
    b = src.encode("utf-8")
    out = bytearray()
    i = 0
    for s, e in _test_module_ranges(b):
        out += b[i:s]
        i = e
    out += b[i:]
    return out.decode("utf-8", "replace")


def production_code(src: str) -> str:
    return "\n".join(
        l for l in rust_lines(production_source(src)) if not trim_start(l).startswith("//")
    )


# ─────────────────────────────────────────────────────────────────────────────
# 二 · 余量
# ─────────────────────────────────────────────────────────────────────────────


def margins(root: str):
    ccm_path = os.path.join(root, "shared", "ccm")
    with open(ccm_path, "rb") as f:
        raw_b = f.read()
    raw = raw_b.decode("utf-8")
    raw_len = len(raw_b)

    rows = []

    # ① polling_registry.rs:736  `prod.len() * 4 > raw.len()`
    prod4 = strip_hash_comment_lines(raw)
    lhs4 = blen(prod4) * 4
    rows.append(
        {
            "住址": "src-tauri/src/polling_registry.rs:736",
            "所在测试": "the_identity_poller_is_gone_for_good",
            "判据": "strip_hash_comment_lines(raw).len() * 4 > raw.len()",
            "剥法": "strip_hash_comment_lines",
            "prod字节": blen(prod4),
            "raw字节": raw_len,
            "左边(prod*N)": lhs4,
            "余量字节": lhs4 - raw_len,
            "报文": "剥注释后只剩 {} / {} 字节 —— 剥法坏了，本条在空转",
        }
    )

    # ② polling_registry.rs:456  `strip_comment_lines(ccm).len() * 2 > ccm.len()`
    prod2 = strip_comment_lines(raw)
    lhs2 = blen(prod2) * 2
    rows.append(
        {
            "住址": "src-tauri/src/polling_registry.rs:456",
            "所在测试": "the_scan_actually_reads_the_frontend_and_ccm",
            "判据": "strip_comment_lines(ccm).len() * 2 > ccm.len()",
            "剥法": "strip_comment_lines",
            "prod字节": blen(prod2),
            "raw字节": raw_len,
            "左边(prod*N)": lhs2,
            "余量字节": lhs2 - raw_len,
            "报文": "剥注释后 shared/ccm 只剩不到一半 —— 剥法太狠",
        }
    )

    # ③ polling_registry.rs:450  `read(shared/ccm).len() > 10_000`（地板，非比例）
    rows.append(
        {
            "住址": "src-tauri/src/polling_registry.rs:450",
            "所在测试": "the_scan_actually_reads_the_frontend_and_ccm",
            "判据": "read_to_string(shared/ccm).len() > 10_000",
            "剥法": "（不剥）",
            "prod字节": raw_len,
            "raw字节": 10_000,
            "左边(prod*N)": raw_len,
            "余量字节": raw_len - 10_000,
            "报文": "shared/ccm 读不到或太短 —— 路径变了？",
        }
    )
    return rows


def added_comment_bytes_to_flip(root: str) -> dict:
    """往 ccm 里追加**纯 `#` 注释行**，第几个字节让 `prod*4 > raw` 翻红。

    形状：追加 `#` + <正文> + `\n`。该行被 filter 丢掉 ⇒ prod 不变、raw 涨 (1+len+1)。
    ⇒ 每加一行涨的 raw = 2 + 正文字节数。余量按 `prod*4 - raw` 逐字扣。
    """
    ccm_path = os.path.join(root, "shared", "ccm")
    raw = open(ccm_path, "rb").read().decode("utf-8")
    prod = strip_hash_comment_lines(raw)
    margin = blen(prod) * 4 - len(raw.encode("utf-8"))
    # 一行注释 `#` + N 个汉字 + `\n` ⇒ raw += 2 + 3N（汉字 UTF-8 三字节）
    # 翻红条件：margin - (2 + 3N) <= 0  ⇒  N >= (margin - 2) / 3
    import math

    n_han = math.ceil((margin - 2) / 3)
    return {"当前余量": margin, "一行注释里放几个汉字就翻红": n_han}


def simulate(root: str, han_per_line: int = 30, max_lines: int = 8):
    """真的在内存里追加注释，逐行重算判据 —— 不写盘、不碰 shared/ccm。

    ★ 追加形状必须是 `"# …\\n"`（ccm 末字节已是 `\\n`），**不是** `"\\n# …"`。
      后者会在文件里多插一个**空行**，而空行不以 `#` 开头 ⇒ 它被 `strip_hash_comment_lines`
      **留下**，`prod` 因此 +1 字节（22849 -> 22850），`prod*4` 就多算 4，余量从 97 变成 101。
      〔K-R14 C 拍 09-01〕**PM 派工单里的 101 正是这个口径差**；`K-G3` 报的 97 才与真 Rust
      对得上（实测：真 Rust 的失败报文印出 `prod = 22849`，见交回里的夹逼实验）。
    """
    ccm_path = os.path.join(root, "shared", "ccm")
    raw = open(ccm_path, "rb").read().decode("utf-8")
    out = []
    cur = raw
    for i in range(max_lines + 1):
        prod = strip_hash_comment_lines(cur)
        ok4 = blen(prod) * 4 > blen(cur)
        p2 = strip_comment_lines(cur)
        ok2 = blen(p2) * 2 > blen(cur)
        out.append(
            {
                "追加注释行数": i,
                "raw": blen(cur),
                "prod(hash剥)": blen(prod),
                "prod*4": blen(prod) * 4,
                "第736条(*4)": "绿" if ok4 else "红",
                "第456条(*2)": "绿" if ok2 else "红",
            }
        )
        cur = cur + "# " + ("说" * han_per_line) + "\n"
    return out


# ─────────────────────────────────────────────────────────────────────────────
# 三 · 普查：盘上还有几条同形的自检
# ─────────────────────────────────────────────────────────────────────────────

SKIP_DIRS = {".git", "target", "node_modules", "dist", "build", ".venv"}

ASSERT_RE = re.compile(r"\bassert(?:_eq|_ne)?!\s*\(")

# 切法 A：**字面同形** —— `<x>.len() * <整数> <比较> <y>.len()`
SHAPE_A = re.compile(r"\.len\(\)\s*\*\s*[0-9_]+\s*(?:>=|<=|>|<)|[<>]=?\s*[A-Za-z_][\w.]*\.len\(\)\s*\*\s*[0-9_]+")

# 切法 B 的机检特征：判据里有「长度/计数 与 阈值」的比较
THRESH_RE = re.compile(
    r"(?:\.len\(\)|\.count\(\))\s*(?:>=|<=|>|<)\s*[0-9_]+"
    r"|[0-9_]+\s*(?:>=|<=|>|<)\s*(?:\w+)?\.?(?:len\(\)|count\(\))"
    r"|\.len\(\)\s*\*\s*[0-9_]+"
    r"|(?:\.len\(\)|\.count\(\))\s*(?:>=|<=|>|<)\s*[A-Za-z_][\w.]*\.(?:len|count)\(\)"
)

# 剥法 / 抽取器的名字：判「这个量是不是从一个抽取器出来的」
EXTRACTOR_RE = re.compile(
    r"strip_hash_comment_lines|strip_comment_lines|production_code|production_source"
    r"|test_source|assert_no_test_code|assert_tree_strips_clean|\bscan\s*\(|\.split\s*\("
)

# 反空转报文词表（只做**辅助**标注，不当判据 —— 措辞会漂）
VACUOUS_WORDS = ["空转", "剥法", "抽取器", "零命中", "扫不到", "抽错", "遍历器", "太短", "剥空", "没扫到", "抽不到"]


def iter_rs(root: str):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in filenames:
            if fn.endswith(".rs"):
                yield os.path.join(dirpath, fn)


def macro_body(text: str, open_paren_idx: int):
    """从 `(` 起做括号配平，返回 (body, end_idx)。粗略跳过字符串与字符字面量。"""
    depth = 0
    i = open_paren_idx
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            i += 1
            while i < n:
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == '"':
                    break
                i += 1
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return text[open_paren_idx + 1 : i], i
        i += 1
    return text[open_paren_idx + 1 :], n


def enclosing_fn(text: str, idx: int):
    """往上找最近的 `fn <name>(`。"""
    head = text[:idx]
    hits = list(re.finditer(r"\bfn\s+([A-Za-z_]\w*)\s*[(<]", head))
    return hits[-1].group(1) if hits else "<未知>"


def census(root: str):
    total_files = 0
    total_asserts = 0
    shape_a = []
    shape_b = []
    for path in iter_rs(root):
        total_files += 1
        try:
            text = open(path, encoding="utf-8").read()
        except (UnicodeDecodeError, OSError):
            continue
        for m in ASSERT_RE.finditer(text):
            total_asserts += 1
            op = m.end() - 1
            body, _ = macro_body(text, op)
            line = text[: m.start()].count("\n") + 1
            rel = os.path.relpath(path, root)
            fname = enclosing_fn(text, m.start())
            # 判据部分 = 第一个顶层逗号之前
            pred = body.split(",")[0]
            rec = {
                "住址": f"{rel}:{line}",
                "所在函数": fname,
                "判据(截断 160)": " ".join(pred.split())[:160],
                "报文里含反空转词": [w for w in VACUOUS_WORDS if w in body],
                "同函数里有抽取器": bool(EXTRACTOR_RE.search(text[max(0, m.start() - 2500) : m.start()])),
            }
            if SHAPE_A.search(body):
                shape_a.append(rec)
            if THRESH_RE.search(body):
                shape_b.append(rec)
    return {
        "分母·扫了几个 .rs 文件": total_files,
        "分母·扫到几个 assert 宏调用": total_asserts,
        "切法A·比例同形(len()*N 与另一长度比)": shape_a,
        "切法A·条数": len(shape_a),
        "切法B·任何长度/计数阈值断言": shape_b,
        "切法B·条数": len(shape_b),
    }


def emit_fixture(root: str, out_path: str, n_lines: int, han: int) -> dict:
    """把 `shared/ccm` 拷成一份**副本**并在末尾追加 `n_lines` 行纯注释。

    ★ 只写 `out_path`，**绝不碰被测树里的 `shared/ccm`**（K-R14 §4④：本件不动它本身）。
    追加形状刻意是「文件末尾直接接 `# …\\n`」——因为 ccm 末字节已是 `\\n`，
    这样**不会**多出一个空行 ⇒ `prod` 逐字节不变，翻红的唯一原因就是 `raw` 涨了。
    """
    import hashlib

    src = os.path.join(root, "shared", "ccm")
    raw = open(src, "rb").read().decode("utf-8")
    if han < 0:
        # han < 0 ⇒ 「精确 N 字节」模式：N = -han，用纯 ASCII 补到**恰好** N 字节。
        # 用途：把余量**夹逼**出来（N-1 绿 / N 红 ⇒ 余量恰是 N-1），不靠复刻的算术自证。
        total = -han
        body = total - 3  # "# " + body + "\n"
        add = "# " + ("x" * body) + "\n"
    else:
        add = "".join("# " + ("说" * han) + "\n" for _ in range(n_lines))
    new = raw + add
    os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
    with open(out_path, "wb") as f:
        f.write(new.encode("utf-8"))
    prod = strip_hash_comment_lines(new)
    prod0 = strip_hash_comment_lines(raw)
    return {
        "副本路径": out_path,
        "副本 sha256": hashlib.sha256(new.encode("utf-8")).hexdigest(),
        "原件 sha256": hashlib.sha256(raw.encode("utf-8")).hexdigest(),
        "追加行数": n_lines,
        "每行汉字数": han,
        "raw(原)": blen(raw),
        "raw(副本)": blen(new),
        "prod(原)": blen(prod0),
        "prod(副本)": blen(prod),
        "prod 是否逐字节不变": prod == prod0,
        "判据 prod*4>raw（副本）": blen(prod) * 4 > blen(new),
    }


# ─────────────────────────────────────────────────────────────────────────────
# 三之二 · 切法 C：**比例式剥法自检**全表（本件真正的病族）
#
# 切法逐字：一个 `assert`，其判据把「**一个剥法/抽取器的输出长度**」与「**同一份原文
# （或同一份语料的另一种派生）的长度**」按一个有理系数比较（写成 `* N` / `/ N` / 系数 1）。
# ⇒ 它的通过与否随**语料的注释、测试段占比**漂移，而那是日常写作会动的量。
#
# 排除：判据右侧是**常数地板**（`x.len() > 3000`）的那一族 —— 它们不随注释占比漂移，
#       只随文件整体规模漂移，是另一族病（本仓已有多处头注在管它，例：
#       `polling_registry.rs:431` 的「地板 170」那段）。
# ─────────────────────────────────────────────────────────────────────────────

# 逐条住址由上面的 grep 确定，这里只算余量。`kind` 决定余量怎么算。
RATIO_SITES = [
    # (住址, 文件, 剥法, 系数分子, 系数分母, 方向, 说明)
    # 方向 "lower"：prod*num/den > raw  ⇒ 注释**多**会红（假红风险在这一侧）
    # 方向 "upper"：prod*num/den < raw  ⇒ 注释**少**会红
    ("src-tauri/src/polling_registry.rs:736", "shared/ccm", "hash", 4, 1, "lower"),
    ("src-tauri/src/polling_registry.rs:456", "shared/ccm", "slash", 2, 1, "lower"),
    ("src-tauri/src/rust_timer_registry.rs:364", "src-tauri/src/rust_timer_registry.rs", "prodcode", 2, 1, "upper"),
    (
        "src-tauri/src/local_read_surface_registry.rs:565",
        "src-tauri/src/local_read_surface_registry.rs",
        "prodcode",
        2,
        1,
        "upper",
    ),
    ("src-tauri/src/tmux_reconcile.rs:421", "src-tauri/src/tmux_reconcile.rs", "prodcode", 1, 1, "upper"),
    ("remote-daemon-proto/src/observe/watcher.rs:2584", "remote-daemon-proto/src/observe/watcher.rs", "prodcode", 1, 1, "upper"),
    ("remote-daemon-proto/src/observe/watcher.rs:2608", "remote-daemon-proto/src/observe/watcher.rs", "prodcode", 1, 1, "upper"),
    ("remote-daemon-proto/src/observe/watcher.rs:2692", "remote-daemon-proto/src/observe/watcher.rs", "prodcode", 1, 1, "upper"),
    ("remote-daemon-proto/src/observe/accounts_query.rs:1311", "remote-daemon-proto/src/main.rs", "prodcode", 1, 1, "upper"),
    # ssh_source 那条两侧都是派生量，单列（见下）
]


def _strip(kind: str, s: str) -> str:
    if kind == "hash":
        return strip_hash_comment_lines(s)
    if kind == "slash":
        return strip_comment_lines(s)
    if kind == "prodcode":
        return production_code(s)
    raise ValueError(kind)


def ratio_table(root: str):
    rows = []
    for addr, rel, kind, num, den, direction in RATIO_SITES:
        path = os.path.join(root, rel)
        try:
            raw = open(path, "rb").read().decode("utf-8")
        except OSError as e:
            rows.append({"住址": addr, "错误": str(e)})
            continue
        prod = _strip(kind, raw)
        lhs = blen(prod) * num // den
        rhs = blen(raw)
        if direction == "lower":
            # 判据 lhs > rhs；余量 = 还能让 rhs 涨多少（= 再写多少字节注释）
            margin = lhs - rhs
            note = f"再往语料里加 {max(margin - 1, 0)} 字节**注释**就翻红"
        else:
            # 判据 lhs < rhs；余量 = rhs - lhs（还能让 prod 涨多少 / 注释缩多少）
            margin = rhs - lhs
            note = f"再往语料里加 {max(margin - 1, 0)} 字节**生产代码**（或删同等注释）才翻红"
        rows.append(
            {
                "住址": addr,
                "语料": rel,
                "剥法": kind,
                "判据": f"prod*{num}/{den} {'>' if direction=='lower' else '<'} raw",
                "prod字节": blen(prod),
                "raw字节": rhs,
                "方向": direction,
                "余量字节": margin,
                "读法": note,
            }
        )
    # ssh_source.rs:3204 —— 两侧都是派生量：good = production_code(self)，cheap = 第一个 cfg(test) 之前
    p = os.path.join(root, "src-tauri/src/ssh_source.rs")
    me = open(p, "rb").read().decode("utf-8")
    good = production_code(me)
    cheap = me.split("\n#[cfg(test)]")[0]
    rows.append(
        {
            "住址": "src-tauri/src/ssh_source.rs:3204",
            "语料": "src-tauri/src/ssh_source.rs（自身）",
            "剥法": "prodcode vs 「第一个 cfg(test) 之前」的便宜近似",
            "判据": "good > cheap*2",
            "prod字节": blen(good),
            "raw字节": blen(cheap) * 2,
            "方向": "lower",
            "余量字节": blen(good) - blen(cheap) * 2,
            "读法": "cheap 每涨 1 字节，余量掉 2 —— 在**第一个 `#[cfg(test)]` 之前**写字才动它",
        }
    )
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, help="被测工作树的绝对路径（不猜，必须给）")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--only", choices=["margins", "census", "simulate"], default=None)
    ap.add_argument("--emit-fixture", metavar="PATH", default=None, help="生成一份跨线的 ccm 副本")
    ap.add_argument("--fixture-lines", type=int, default=1)
    ap.add_argument("--fixture-han", type=int, default=40)
    a = ap.parse_args()
    root = os.path.abspath(a.root)
    if not os.path.isdir(os.path.join(root, "src-tauri")):
        print(f"不像工作树（没有 src-tauri/）：{root}", file=sys.stderr)
        return 3

    result = {"被测树": root}
    if a.emit_fixture:
        result["夹具"] = emit_fixture(root, os.path.abspath(a.emit_fixture), a.fixture_lines, a.fixture_han)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0
    if a.only in (None, "margins"):
        result["余量表"] = margins(root)
        result["翻红门槛"] = added_comment_bytes_to_flip(root)
    if a.only in (None, "simulate"):
        result["追加注释模拟"] = simulate(root)
    if a.only in (None, "census"):
        result["普查"] = census(root)

    if a.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
