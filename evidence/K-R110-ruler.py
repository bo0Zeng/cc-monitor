#!/usr/bin/env python3
"""`K-R110` 的尺子。**只读**，不改任何文件。

住址：本文件住在 `<本工作树>/evidence/`；`ROOT` = **它所在的那棵树的仓根**
（`brief` 第 12 条：量具住址要唯一定位到那一份被测对象 —— 不许拿 `cwd` 猜）。
⚠ 与 `K-R106-ruler.py` / `K-R101-cut.py` 是**不同的量具**，被测对象也不同，别互相照搬读数。

三把尺子，各自的分母都写在自己的输出里：

  A `strip`    —— `guard_core::test_module_ranges` 的**列 0 右大括号**收尾针，
                  在全树每一份 `.rs` 上「现行（裸文本）」与「词法掩码后」两口径的**对拍**。
                  差一处 = 一份被**提前收尾**（或反过来）的文件。逐份给漏了几行。
  B `backtest` —— 尺子 A 的**回测**（纪律 ⑳）：已知样本 `remote-daemon-proto/src/plugin/mod.rs`
                  必须被 A 数到；另造两份合成样本（一份该命中、一份不该命中）各判一次。
  C `census`   —— `K-R103` 那个「匹配单位是行」的人群：`.lines()` 在
                  `remote-daemon-proto/src/*.rs`（**顶层，不递归**）＋ `platform/*.rs` 里逐处，
                  连同它所在的函数与上下文一起印，供人工三档定性。

用法：`python3 evidence/K-R110-ruler.py [strip|backtest|census|probe|all]`

⚠ 尺子 A 的词法掩码是 `guard_core::try_strip_block_comments` 那台状态机的**逐条 Python 移植**
（块注释 ＋ 原始串/字节串/普通串 ＋ 行注释 ＋ 字符字面量掩码 ＋ 同一条兜底）。
移植不是权威源 —— **权威源是 Rust 那一份**；本尺子只用来出读数，判红归门禁。
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `guard_core::assert_tree_strips_clean` 今天的三个调用根（住址在下面三处）：
#   remote-daemon-proto/src/guard_support.rs:61 · src-tauri/src/structural_scan.rs:611 / 667
TREE_ROOTS = [
    "remote-daemon-proto/src",
    "src-tauri/src",
]


# ── 词法掩码：`try_strip_block_comments` 的逐条移植，另加「串内容也抹掉」──────────
def _ident(c: int) -> bool:
    return (48 <= c <= 57) or (65 <= c <= 90) or (97 <= c <= 122) or c == 0x5F


def _utf8_len(lead: int) -> int:
    if lead < 0x80:
        return 1
    if lead >= 0xF0:
        return 4
    if lead >= 0xE0:
        return 3
    return 2


def mask_char_literals(line: bytes) -> bytes:
    b = bytearray(line)
    i = 0
    n = len(line)
    while i < n:
        if line[i] == 0x27:  # '
            if i + 3 < n and line[i + 1] == 0x5C:  # backslash
                k = line.find(b"'", i + 3)
                if k != -1:
                    for j in range(i, k + 1):
                        b[j] = 0x5F
                    i = k + 1
                    continue
            if i + 1 < n:
                cl = _utf8_len(line[i + 1])
                c0 = line[i + 1]
                if c0 != 0x27 and c0 != 0x5C and i + 1 + cl < n and line[i + 1 + cl] == 0x27:
                    for j in range(i, i + 2 + cl):
                        b[j] = 0x5F
                    i = i + 2 + cl
                    continue
        i += 1
    return bytes(b)


def raw_string_open(sb: bytes, i: int):
    """`r"…"` / `r#"…"#` / `b"…"` / `br#"…"#` 的开头在不在 `sb[i]`。→ (井号数, 消耗字节数)"""
    if i > 0 and _ident(sb[i - 1]):
        return None
    k = i
    if k < len(sb) and sb[k] == 0x62:  # b
        k += 1
        if k < len(sb) and sb[k] == 0x22:  # "
            return (0, k + 1 - i)
    if k >= len(sb) or sb[k] != 0x72:  # r
        return None
    k += 1
    hash_start = k
    while k < len(sb) and sb[k] == 0x23:  # #
        k += 1
    if k >= len(sb) or sb[k] != 0x22:
        return None
    return (k - hash_start, k + 1 - i)


def mask_literals_and_comments(src: bytes):
    """块注释 ＋ 字符串字面量的内容一律抹成等长空格。词法对不上 ⇒ `None`（同 Rust 兜底）。"""
    out = bytearray()
    depth = 0
    in_str = False
    raw_hashes = None
    lines = src.split(b"\n")
    for li, raw in enumerate(lines):
        if li > 0:
            out.append(0x0A)
        if depth == 0 and not in_str and raw_hashes is None:
            scan = mask_char_literals(raw)
        else:
            scan = raw
        line = bytearray(raw)
        i = 0
        n = len(scan)
        while i < n:
            if depth > 0:
                if scan[i] == 0x2F and i + 1 < n and scan[i + 1] == 0x2A:  # /*
                    depth += 1
                    line[i] = 0x20
                    line[i + 1] = 0x20
                    i += 2
                    continue
                if scan[i] == 0x2A and i + 1 < n and scan[i + 1] == 0x2F:  # */
                    depth -= 1
                    line[i] = 0x20
                    line[i + 1] = 0x20
                    i += 2
                    continue
                line[i] = 0x20
                i += 1
                continue
            if raw_hashes is not None:
                h = raw_hashes
                if scan[i] == 0x22 and n >= i + 1 + h and all(
                    scan[j] == 0x23 for j in range(i + 1, i + 1 + h)
                ):
                    raw_hashes = None
                    for j in range(i, i + 1 + h):
                        line[j] = 0x20
                    i += 1 + h
                    continue
                line[i] = 0x20
                i += 1
                continue
            if in_str:
                if scan[i] == 0x5C:
                    line[i] = 0x20
                    if i + 1 < n:
                        line[i + 1] = 0x20
                    i += 2
                    continue
                if scan[i] == 0x22:
                    in_str = False
                line[i] = 0x20
                i += 1
                continue
            # ── 码状态 ──
            ro = raw_string_open(scan, i)
            if ro is not None:
                h, consumed = ro
                if h == 0 and scan[i] == 0x62:
                    in_str = True
                else:
                    raw_hashes = h
                for j in range(i, min(i + consumed, len(line))):
                    line[j] = 0x20
                i += consumed
                continue
            if scan[i] == 0x22:
                in_str = True
                line[i] = 0x20
                i += 1
                continue
            if scan[i] == 0x2F and i + 1 < n and scan[i + 1] == 0x2F:  # //
                break
            if scan[i] == 0x2F and i + 1 < n and scan[i + 1] == 0x2A:  # /*
                depth += 1
                line[i] = 0x20
                line[i + 1] = 0x20
                i += 2
                continue
            i += 1
        out.extend(line)
    if depth != 0 or in_str or raw_hashes is not None:
        return None
    return bytes(out)


# ── `guard_core` 那三个判定的移植 ──────────────────────────────────────────────
def cfg_is_test_only(attr: bytes) -> bool:
    k = attr.find(b"test")
    while k != -1:
        before_ok = k == 0 or not (_ident(attr[k - 1]) or attr[k - 1] == 0x2D)
        after = k + 4
        after_ok = after >= len(attr) or not (_ident(attr[after]) or attr[after] == 0x2D)
        if before_ok and after_ok:
            return True
        k = attr.find(b"test", k + 1)
    return False


def strip_visibility(line: bytes) -> bytes:
    if not line.startswith(b"pub"):
        return line
    rest = line[3:]
    after = rest.lstrip()
    if len(after) == len(rest) and not rest.startswith(b"("):
        return line
    if not after.startswith(b"("):
        return after
    inner = after[1:]
    depth = 1
    for k in range(len(inner)):
        c = inner[k]
        if c == 0x28:
            depth += 1
        elif c == 0x29:
            depth -= 1
            if depth == 0:
                return inner[k + 1:].lstrip()
    return line


def test_module_ranges(src: bytes, hay: bytes):
    """`hay` = 拿来找锚点的那份文本（现行口径 = `src` 本身；修后口径 = 掩码后）。"""
    OPEN = b"\n#[cfg("
    CLOSE = b"\n}"
    out = []
    i = 0
    while True:
        rel = hay.find(OPEN, i)
        if rel == -1:
            return out
        j = rel

        def line_end(frm: int) -> int:
            k = src.find(b"\n", frm)
            return len(src) if k == -1 else k

        attr_start = j + 1
        attr_end = line_end(attr_start)
        mod_start = min(attr_end + 1, len(src))
        mod_end = line_end(mod_start)
        mod_line = src[mod_start:mod_end].strip()
        is_test_mod = (
            cfg_is_test_only(src[attr_start:attr_end])
            and strip_visibility(mod_line).startswith(b"mod ")
            and mod_line.endswith(b"{")
        )
        if not is_test_mod:
            i = attr_end
            continue
        k = hay.find(CLOSE, j)
        if k == -1:
            out.append((j, len(src)))
            return out
        end = k + len(CLOSE)
        out.append((j, end))
        i = end


def lineno(src: bytes, off: int) -> int:
    return src.count(b"\n", 0, off) + 1


def tracked_rs():
    o = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "*.rs"], capture_output=True, text=True, check=True
    )
    return [p for p in o.stdout.splitlines() if p]


def in_tree_roots(rel: str) -> bool:
    return any(rel.startswith(r + "/") for r in TREE_ROOTS)


# ── 尺子 A ────────────────────────────────────────────────────────────────────
def scan_one(path: Path):
    src = path.read_bytes()
    masked = mask_literals_and_comments(src)
    fell_back = masked is None
    cur = test_module_ranges(src, src)
    fix = test_module_ranges(src, src if fell_back else masked)
    return src, cur, fix, fell_back


def strip_ruler(verbose=True):
    files = tracked_rs()
    diffs = []
    fallbacks = []
    n_with_mod = 0
    for rel in files:
        p = ROOT / rel
        if not p.exists():
            continue
        src, cur, fix, fell_back = scan_one(p)
        if fell_back:
            fallbacks.append(rel)
        if cur:
            n_with_mod += 1
        if cur != fix:
            diffs.append((rel, src, cur, fix))
    if verbose:
        print("── 尺子 A `strip`：列 0 收尾针「裸文本 vs 词法掩码」对拍 ──")
        print(f"分母：`git ls-files '*.rs'` 现打 **{len(files)}** 份")
        inr = [f for f in files if in_tree_roots(f)]
        print(
            f"      其中落在 `assert_tree_strips_clean` 三个调用根之下的 **{len(inr)}** 份"
            f"（根：{', '.join(TREE_ROOTS)}），射程外 **{len(files) - len(inr)}** 份"
        )
        print(f"      现行口径下**切出至少一个测试模块区间**的 **{n_with_mod}** 份")
        print(f"      词法掩码走兜底（模型对不上、一个字不抹）的 **{len(fallbacks)}** 份 {fallbacks}")
        print(f"两口径**不一致**的：**{len(diffs)}** 份")
        for rel, src, cur, fix in diffs:
            print(f"\n  · {rel}（`wc -l` 口径 {src.count(chr(10).encode())} 行，"
                  f"{'在' if in_tree_roots(rel) else '**不在**'}树遍历射程内）")
            print(f"      现行区间 {[(lineno(src, a), lineno(src, b)) for a, b in cur]}")
            print(f"      掩码区间 {[(lineno(src, a), lineno(src, b)) for a, b in fix]}")
            for (a1, b1), (a2, b2) in zip(cur, fix):
                if b1 != b2:
                    lost = lineno(src, b2) - lineno(src, b1)
                    print(
                        f"      ⇒ 提前收尾于第 {lineno(src, b1)} 行"
                        f"（逐字 `{src[b1 - 1:src.find(chr(10).encode(), b1)].decode(errors='replace')}`），"
                        f"真收尾在第 {lineno(src, b2)} 行 ⇒ **{lost} 行**测试代码漏进生产段、"
                        f"同样这 {lost} 行从测试段里少掉"
                    )
    return diffs


# ── 尺子 B：回测 ──────────────────────────────────────────────────────────────
CLEAN_SAMPLE = b"""fn keep() {}

#[cfg(test)]
mod tests {
    #[test]
    fn t() {
        let s = "no col-0 brace here";
        assert!(!s.is_empty());
    }
}
"""

DIRTY_SAMPLE = (
    b"fn keep() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n"
    b"        let s = r#\"\nfn x() {\n}\"#;\n        assert!(!s.is_empty());\n    }\n}\n"
)


def backtest():
    print("\n── 尺子 B `backtest`：拿已知答案回测尺子 A（纪律 ⑳）──")
    known = "remote-daemon-proto/src/plugin/mod.rs"
    diffs = strip_ruler(verbose=False)
    hit = [d[0] for d in diffs]
    ok1 = known in hit
    print(f"  ① 已知阳性样本 `{known}`：尺子 A {'**数到了**' if ok1 else '**没数到 ⇒ 读数作废**'}")
    for name, sample, want in (
        ("合成阴性（测试模块里没有列 0 `}`）", CLEAN_SAMPLE, False),
        ("合成阳性（`r#\"…\"#` 内容含列 0 `}`）", DIRTY_SAMPLE, True),
    ):
        masked = mask_literals_and_comments(sample)
        cur = test_module_ranges(sample, sample)
        fix = test_module_ranges(sample, sample if masked is None else masked)
        got = cur != fix
        print(
            f"  {'②' if want is False else '③'} {name}：现行 "
            f"{[(lineno(sample, a), lineno(sample, b)) for a, b in cur]} · 掩码 "
            f"{[(lineno(sample, a), lineno(sample, b)) for a, b in fix]} ⇒ "
            f"{'判为提前收尾' if got else '判为一致'}，期望 "
            f"{'提前收尾' if want else '一致'} ⇒ {'OK' if got == want else '**尺子坏了**'}"
        )
    return ok1


# ── 尺子 C：`.lines()` 人群 ───────────────────────────────────────────────────
def census_population():
    """`K-R103` 逐字的人群：`remote-daemon-proto/src/*.rs`（顶层）＋ `platform/*.rs`。"""
    base = ROOT / "remote-daemon-proto/src"
    files = sorted(p for p in base.glob("*.rs"))
    files += sorted(p for p in (base / "platform").glob("*.rs"))
    return files


def census():
    print("\n── 尺子 C `census`：「匹配单位是行」的人群逐处 ──")
    files = census_population()
    total = 0
    excl = 0
    rows = []
    for p in files:
        rel = str(p.relative_to(ROOT))
        src = p.read_text(encoding="utf8")
        for i, line in enumerate(src.splitlines(), 1):
            c = line.count(".lines()")
            if not c:
                continue
            total += c
            if p.name == "no_timer_guard.rs":
                excl += c
                continue
            rows.append((rel, i, line.strip(), c))
    print(
        f"分母：`.lines()` 的**文本出现次数**，人群 = `remote-daemon-proto/src/*.rs`"
        f"（顶层，不递归）＋ `platform/*.rs`，现打 **{len(files)}** 份文件"
    )
    print(f"  全人群 **{total}** 处 · 其中 `no_timer_guard.rs` **{excl}** 处（`K-R103` 已治，扣掉）"
          f" ⇒ **{total - excl}** 处待查")
    for rel, i, text, c in rows:
        print(f"  {rel}:{i}{'  ×' + str(c) if c > 1 else ''}\n      {text}")
    return rows


# ── 尺子 D：三档定性的**回测台**（纪律 ⑳）────────────────────────────────────
#
# 每一条都是那处判据谓词的**逐条 Python 复刻**（复刻不是权威源，权威源是 Rust 那一份）。
# 每条喂**两个**样本：
#   ① **已知会命中**的单行形 —— 它不命中 ⇒ 复刻写错了，这一行的读数作废；
#   ② **同一个事实**换成 rustfmt / 人手会写出来的跨行形 —— 它不命中 ⇒ 「够不着」成立。
# 「今天零实例」这一档只说明**盘上现在没有②那种写法**，不说明判据够得着它。

def _lines(sample):
    return [l for l in sample.split("\n")]


def _fs_verb(line):
    """逐条照抄 `every_fs_call_in_daemon_production_is_read_only` 的动词抽法（只在本行上找）。"""
    l = line.strip()
    if l.startswith("//"):
        return ""
    k = l.find("fs::")
    if k < 0:
        return ""
    rest = l[k + 4:]
    out = []
    for c in rest:
        if c.isalnum() or c == "_":
            out.append(c)
        else:
            break
    return "".join(out)


def _cfgless_tail(ls):
    """逐条照抄 `cfgless_guard::fabricated_success`：先 `trim` 再剥外层花括号，再取最后一个非空行。"""
    inner = "\n".join(ls).strip().lstrip("{").rstrip("}")
    body = [l.strip() for l in inner.split("\n") if l.strip()]
    return (body[-1] if body else "").rstrip(",;")


PROBES = [
    (
        "9",
        "inbound.rs:804",
        "`l.contains(\"tokio::main\") && (含 current_thread 或 worker_threads = 1)`",
        lambda ls: any(
            ("tokio::main" in l)
            and ("current_thread" in l or "worker_threads = 1" in l)
            for l in ls
        ),
        '#[tokio::main(flavor = "current_thread")]',
        '#[tokio::main(\n    flavor = "current_thread"\n)]',
    ),
    (
        "11",
        "inbound.rs:1393",
        "`(含 id. / id_for_task. / id_sup.) && (含 .parse/.split/…)`",
        lambda ls: any(
            ("id." in l or "id_for_task." in l or "id_sup." in l)
            and any(b in l for b in (".parse", ".split", ".strip_prefix", ".strip_suffix", ".starts_with", ".ends_with"))
            for l in ls
        ),
        "let n = id.parse::<u64>().unwrap();",
        "let n = id_for_task\n    .strip_prefix(\"req-\")\n    .unwrap();",
    ),
    (
        "20",
        "panorama_locus_guard.rs:284",
        "`declares_engine_dep`：行首那个词 == 引擎名（按 `=` 切）",
        lambda ls: any(
            (not l.strip().startswith("#"))
            and ("=" in l)
            and l.strip().split("=", 1)[0].strip().strip('"') in ("code-picture-core", "code_picture_core")
            for l in ls
        ),
        'code-picture-core = { path = "../x" }',
        '[dependencies.code-picture-core]\npath = "../x"',
    ),
    (
        "24",
        "protocol_doc_guard.rs:301",
        "`声明 = 左花括号之前那一段的**最后一行**`",
        lambda ls: [l for l in ls if l.strip()][-1].strip().startswith("pub struct"),
        "pub struct Hello",
        "pub struct Hello<\n    T,\n>",
    ),
    (
        "25",
        "protocol_doc_guard.rs:478",
        "`l.starts_with(\"#[serde(\") && l.contains(\"rename\")`",
        lambda ls: any(l.strip().startswith("#[serde(") and "rename" in l for l in ls),
        '#[serde(rename = "sessionId")]',
        '#[serde(\n    rename = "sessionId"\n)]',
    ),
    (
        "30",
        "readonly_guard.rs:648",
        "`is_hatch`：同一行既 `use ` 开头、又含 `std::fs`/`tokio::fs` 且其后不是 `;`",
        lambda ls: any(
            l.strip().startswith("use ")
            and any(
                (l.find(b) >= 0 and not l[l.find(b) + len(b):].lstrip().startswith(";"))
                for b in ("std::fs", "tokio::fs")
            )
            for l in ls
        ),
        "use std::fs::write;",
        "use std::{\n    fs::write,\n    io,\n};",
    ),
    (
        "31",
        "readonly_guard.rs:676",
        "`每处 fs:: / File:: 之后紧跟的那个动词`（动词只在**同一行**上找；取空就 `continue`）",
        lambda ls: any(_fs_verb(l) not in ("", "read_to_string", "metadata") for l in ls),
        'std::fs::write(p, b"x");',
        'std::fs::\n    write(p, b"x");',
    ),
    (
        "34",
        "readonly_guard.rs:2680",
        "`dep_entries`：节名必须逐字以 `dependencies]` 收尾，条目按 `key = value` 逐行取",
        lambda ls: _dep_entries_has(ls, "code-picture-core"),
        '[dependencies]\ncode-picture-core = { path = "../x" }',
        '[dependencies.code-picture-core]\npath = "../x"',
    ),
    (
        "44",
        "platform/cfgless_guard.rs:809",
        "`fabricated_success`：**先剥掉外层花括号**，再看块体最后一个非空行是否以 `Some(` / `Ok(` 打头",
        lambda ls: _cfgless_tail(ls).startswith(("Some(", "Ok(")),
        "{\n    Ok(Status::Alive)\n}",
        "{\n    Ok(\n        Status::Alive,\n    )\n}",
    ),
    (
        "45/46",
        "platform/fallback_guard.rs:284 / :423",
        "`block_tail`：块的值 = **最后一个非空、非 `//`、非裸花括号**的那一行",
        lambda ls: (
            [l for l in ls if l.strip() and not l.strip().startswith("//") and l.strip() not in ("{", "}")][-1]
            .strip()
            .rstrip(";}")
            .strip()
            .startswith(("unimplemented!(", "todo!("))
        ),
        "{\n    unimplemented!(\"未实现\")\n}",
        "{\n    unimplemented!(\n        \"未实现\"\n    )\n}",
    ),
]


def _dep_entries_has(ls, name):
    section = ""
    for line in ls:
        e = line.strip()
        if e.startswith("["):
            section = e if e.endswith("dependencies]") else ""
            continue
        if not section or "=" not in e:
            continue
        if e.split("=", 1)[0].strip() == name:
            return True
    return False


def probe():
    print("\n── 尺子 D `probe`：三档定性的回测台（每条两个样本）──")
    print("分母 = 下面这 %d 条（= 普查里判「够不着」的全部处数；判「够得着」的 38 处不进这张表）" % len(PROBES))
    bad = 0
    for pid, addr, pred_txt, pred, single, multi in PROBES:
        a = pred(_lines(single))
        b = pred(_lines(multi))
        verdict = "OK" if (a and not b) else "**回测不成立**"
        if not (a and not b):
            bad += 1
        print(f"\n  [{pid}] {addr}\n      谓词：{pred_txt}")
        print(f"      ① 已知会命中的单行形 {single!r} ⇒ {'命中' if a else '**没命中**'}")
        print(f"      ② 同一事实的跨行形   {multi!r} ⇒ {'命中' if b else '不命中（够不着成立）'}")
        print(f"      ⇒ {verdict}")
    print(f"\n  合计：{len(PROBES)} 条里回测成立 {len(PROBES) - bad} 条、不成立 {bad} 条")
    return bad == 0


# ── 尺子 E：改动面 —— 逐个顶层 item 的 md5（Rust 没有 `ast`，按列 0 花括号切）────
#
# 切法：用尺子 A 那把**词法掩码**找列 0 的 `fn` / `mod` / `impl` 头与它的列 0 收尾 `}`。
# ⚠ 它答的是「**哪几个顶层 item 的字节变了**」，不答「语义变没变」。
# 用法：`python3 evidence/K-R110-ruler.py funcs <文件相对路径> [<git-ref>]`
#       给了 ref 就同时打那个 ref 上的同名 item，逐个对比。

import hashlib
import re


def top_items(src: bytes):
    """→ [(名字, 起偏移, 止偏移)]，按**词法掩码**上的列 0 花括号收尾切。"""
    mk = mask_literals_and_comments(src)
    hay = src if mk is None else mk
    out = []
    head = re.compile(
        rb"^((?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?"
        rb"(fn|mod|impl|struct|enum|trait|const|static)\s+[A-Za-z_][A-Za-z0-9_]*)",
        re.M,
    )
    for m in head.finditer(hay):
        start = m.start()
        brace = hay.find(b"{", start)
        semi = hay.find(b";", start)
        if brace < 0 or (0 <= semi < brace):
            continue  # 无块体的声明
        end = hay.find(b"\n}", brace)
        end = len(src) if end < 0 else end + 2
        name = m.group(1).decode("utf8", "replace").split()[-1]
        kind = m.group(2).decode()
        out.append((f"{kind} {name}", start, end))
    return out


def funcs(rel: str, ref: str | None = None):
    src = (ROOT / rel).read_bytes()
    now = {n: hashlib.md5(src[a:b]).hexdigest()[:10] for n, a, b in top_items(src)}
    print(f"── 尺子 E `funcs`：{rel} 的顶层 item md5（现打 {len(now)} 个）──")
    if ref:
        old_src = subprocess.run(
            ["git", "-C", str(ROOT), "show", f"{ref}:{rel}"],
            capture_output=True, check=True,
        ).stdout
        was = {n: hashlib.md5(old_src[a:b]).hexdigest()[:10] for n, a, b in top_items(old_src)}
        names = sorted(set(now) | set(was))
        changed = [n for n in names if now.get(n) != was.get(n)]
        print(f"   对照 `{ref}`：那一版 {len(was)} 个 · 两版并集 {len(names)} 个 · **变了 {len(changed)} 个**")
        for n in changed:
            print(f"     · {n}\n         {ref}: {was.get(n, '（不存在）')}   →   工作树: {now.get(n, '（已删）')}")
        same = [n for n in names if n in now and n in was and now[n] == was[n]]
        print(f"   逐字未变的 {len(same)} 个（分母 = 两版并集 {len(names)}）")
    else:
        for n in sorted(now):
            print(f"     {now[n]}  {n}")


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    if which in ("strip", "all"):
        strip_ruler()
    if which in ("backtest", "all"):
        backtest()
    if which in ("census", "all"):
        census()
    if which in ("probe", "all"):
        probe()
    if which == "funcs":
        funcs(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None)
