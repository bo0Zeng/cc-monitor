#!/usr/bin/env python3
# ruff: noqa: E501
"""K-R1 · B1 尺子 —— 「per-row auth_style」与「Base 加 path」这两格的**静态读数**。

用法（**被测对象必须显式给**，不许有默认值）：

    python3 evidence/K-R1-B1-auth-style-and-base-path-census.py <代码仓根的绝对路径>

⚠ **为什么不许有默认值**〔`brief` 12 的 `5k`〕：临时目录与 evidence/ 都是几个 agent 共用的，
一份量具被同名覆盖成「被测对象指向另一棵树」的版本之后，照原用法重跑**跑出来的是另一棵树上的数，
而输出长得一模一样** —— 那是一次静默的假读数。⇒ 树由命令行给，并且**印在输出第一行**。

# 它答什么

七格，每格 `(锚点, 期望, 实得)`。**锚点对不上就 `exit 1`** —— 不许在一把瞎了的尺子上读数。
〔上一拍那份 `K-R1-Z1-…` 的尺子自己先红过一次：裸 `find("//")` 把 `"https://…"` 里的 `//`
当成注释起点，要数的第一条假设**恒 0 命中**。本尺子的注释剥法逐字节跟引号状态，见 `strip_comments`。〕

# 它**不**答什么（射程如实写，别读大一格）

- 它是**文本**尺子：数的是「源码里有几处这么写」，不是「跑起来会怎样」。
  行为那一半住 cargo 判据（逐条名字见下面每格的 `judge` 栏），本尺子只做**处数对账**。
- 「供应商名零命中」这一格的分母是**我列出的这几个名字**，不是「所有供应商」——
  那个分母没人给得出。它能证「我列的这几个确实没进代码」，不能证「没有任何供应商名进了代码」。
- 它不看 `#[cfg(test)]`：判据段里出现供应商名是**合法**的（夹具要一个像样的名字）。
  ⇒ 剥测试段那一步走**保守**的括号配平；切不出来就 `exit 1`，不静默放过。
"""

import re
import sys
from pathlib import Path

# ── 分母：本尺子看哪几棵子树 ────────────────────────────────────────────────
DAEMON_SRC = "remote-daemon-proto/src"
MONITOR_SRC = "src-tauri/src"
CREDS_CORE_SRC = "src-tauri/crates/creds-core/src"

# ⚠ 这几个名字是**我列出的分母**，不是「所有供应商」。全部小写比对。
VENDOR_NAMES = [
    "deepseek",
    "kimi",
    "moonshot",
    "qwen",
    "dashscope",
    "ollama",
    "vllm",
    "lmstudio",
    "openrouter",
    "zhipu",
    "glm",
    "minimax",
]


def literal_mask(src: str) -> list[bool]:
    """逐字节标出「这个位置在字符串 / 字符字面量里吗」。**注释剥法与括号配平共用这一份。**

    # ⚠ 三个坑，每个都真栽过或差点栽

    1. **`//` 在字符串里**：`const D: &str = "https://api.example.com";` ——
       裸 `find("//")` 会把这一行整个截短，要数的东西恒 0 命中。
       〔上一拍那份 `K-R1-Z1-…` 尺子第一版就是这么红的。〕
    2. **单引号不一定是字面量**：Rust 的**生命周期** `&'static str` 里那个 `'` 没有配对的
       另一半。按「见 `'` 就进字面量」扫，会从那里一路吞到下一个 `'`
       ⇒ 中间真的注释与真的括号全被吞掉。
       ⇒ 这里只把**看起来像字符字面量**的那一形算字面量（`'x'` / `'\\n'`），别的当普通字符。
    3. **字面量里的括号**：`b'{'` 与 `format!("{k}: {v}")` 里的括号**不许**参与配平 ——
       本尺子第一版就是在这里当场 `exit 1` 的（`b'{'` 把深度算歪了）。

    ⚠ 射程：不认 raw string（`r#"…"#`）的转义规则；本仓那几处里没有会踩到的形状，
    如实记为射程外。认不出时**宁可算成「不在字面量里」**（人群偏大 ⇒ 宁可多报，不静默漏报）。
    """
    n = len(src)
    mask = [False] * n
    i = 0
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            while i < n and src[i] != "\n":
                i += 1
            continue
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            i += 2
            while i + 1 < n and not (src[i] == "*" and src[i + 1] == "/"):
                i += 1
            i += 2
            continue
        # **raw string**（`r"…"` / `r#"…"#` / `r##"…"##`）。
        # ⚠ 这一支是实测逼出来的：没有它，`r#""kind":"hello""#` 里那个紧跟着的 `"`
        #   会被当成字符串的结尾 ⇒ 后面 raw string 里那些 `{` `}` 全没被遮住
        #   ⇒ 括号配平当场炸（现打：`remote_branch.rs` 与 `ssh_source.rs` 两份）。
        if c == "r" and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            k = i + 1
            while k < n and src[k] == "#":
                k += 1
            hashes = k - i - 1
            if k < n and src[k] == '"':
                close = '"' + "#" * hashes
                end = src.find(close, k + 1)
                stop = (end + len(close)) if end >= 0 else n
                for q in range(i, stop):
                    mask[q] = True
                i = stop
                continue
        if c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            for k in range(i, min(j + 1, n)):
                mask[k] = True
            i = j + 1
            continue
        if c == "'":
            # 只认 `'x'` 与 `'\x'` 这两形；别的（生命周期）当普通字符。
            if i + 2 < n and src[i + 1] != "\\" and src[i + 2] == "'":
                mask[i] = mask[i + 1] = mask[i + 2] = True
                i += 3
                continue
            if i + 3 < n and src[i + 1] == "\\" and src[i + 3] == "'":
                for k in range(i, i + 4):
                    mask[k] = True
                i += 4
                continue
            i += 1
            continue
        i += 1
    return mask


def strip_comments(src: str) -> str:
    """把 `//` 行注释与 `/* */` 块注释剥掉（换行保留 ⇒ 行号不动）。"""
    mask = literal_mask(src)
    out = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if not mask[i] and c == "/" and i + 1 < n and src[i + 1] == "/":
            while i < n and src[i] != "\n":
                i += 1
            continue
        if not mask[i] and c == "/" and i + 1 < n and src[i + 1] == "*":
            i += 2
            while i + 1 < n and not (src[i] == "*" and src[i + 1] == "/"):
                if src[i] == "\n":
                    out.append("\n")
                i += 1
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def strip_test_mods(src: str) -> str:
    """剥掉 `#[cfg(test)]` 之后那一个 `mod … { … }` 块（括号配平，**字面量里的不算**）。

    **切不出来就 raise** —— 剥法坏了的时候人群会偏大，而偏大的人群会报一堆假红。
    """
    out = src
    while True:
        at = out.find("#[cfg(test)]")
        if at < 0:
            return out
        mask = literal_mask(out)
        brace = -1
        for k in range(at, len(out)):
            if out[k] == "{" and not mask[k]:
                brace = k
                break
        if brace < 0:
            eol = out.find("\n", at)
            out = out[:at] + out[eol if eol > 0 else len(out) :]
            continue
        depth, i = 0, brace
        while i < len(out):
            if not mask[i]:
                if out[i] == "{":
                    depth += 1
                elif out[i] == "}":
                    depth -= 1
                    if depth == 0:
                        break
            i += 1
        if depth != 0:
            raise SystemExit("剥 `#[cfg(test)]` 时括号没配平 —— 剥法坏了，本尺子按红处理")
        out = out[:at] + out[i + 1 :]


def rs_files(root: Path, rel: str) -> list[Path]:
    d = root / rel
    if not d.is_dir():
        raise SystemExit(f"扫不到 {d} —— 被测对象给错了棵树？本尺子按红处理")
    return sorted(p for p in d.rglob("*.rs"))


def production(root: Path, rel: str) -> list[tuple[str, str]]:
    """`(仓根相对路径, 剥掉注释与测试段之后的源码)`。"""
    out = []
    for p in rs_files(root, rel):
        raw = p.read_text(encoding="utf-8")
        # ⚠⚠ **次序是承重的：先剥注释，再剥测试段。**
        #    反过来的话，一句**注释里**提到 `#[cfg(test)]` 的话（本仓 `readonly_guard` /
        #    `creds_guard` 的头注逐字就有）会被当成一个真的测试段起点，
        #    从那里一路吃到下一个配平的 `}` ⇒ 把一大片**生产段**连带剥掉，
        #    而症状是「要数的东西恰好 0 命中」——本尺子上一版就是这么错的。
        out.append((str(p.relative_to(root)), strip_test_mods(strip_comments(raw))))
    return out


def count(files: list[tuple[str, str]], needle: str) -> list[str]:
    """逐行找一根针，回 `路径:行号`。行号是**剥后**的行号，只当线索用。"""
    hits = []
    for path, src in files:
        for no, line in enumerate(src.splitlines(), 1):
            if needle in line:
                hits.append(f"{path}:{no}")
    return hits


# ── 七格 ────────────────────────────────────────────────────────────────────
def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    root = Path(sys.argv[1]).resolve()
    print(f"# 被测对象（量于哪棵树）: {root}")
    print(f"# 量具住址: evidence/{Path(__file__).name}")
    print()

    daemon = production(root, DAEMON_SRC)
    monitor = production(root, MONITOR_SRC)
    core = production(root, CREDS_CORE_SRC)
    all_prod = daemon + monitor + core

    # 采集面自检：三棵子树都真的扫到了东西（人群塌了的话下面全是空真）。
    for label, files, floor in (
        ("daemon", daemon, 30),
        ("monitor", monitor, 60),
        ("creds-core", core, 3),
    ):
        if len(files) < floor:
            print(f"FAIL 采集面自检 {label}: 只扫到 {len(files)} 个 .rs（下界 {floor}）—— 取法坏了")
            return 1

    rows: list[tuple[str, str, object, object]] = []

    # ① 明文出口恰好一处 —— 本件最要紧的那条不变量。
    #    judge: creds_guard::the_plaintext_leaves_the_type_at_exactly_one_place_in_this_crate
    h = count(daemon, "expose_for_auth_header(")
    rows.append(("① 明文换头出口（daemon 生产段）", "恰好 1", 1, (len(h), h)))

    # ② daemon 里一处落盘出口都没有（`K-H2a` 裁四：daemon 只读）。
    h = count(daemon, "expose_for_persisting(")
    rows.append(("② 明文落盘出口（daemon 生产段）", "恰好 0", 0, (len(h), h)))

    # ③ 上游与 key（`K-R1` 之后还加上鉴权头形状）焊在一起的地方恰好一处。
    #    judge: table_guard::the_only_place_that_welds_an_upstream_to_a_key_is_inside_the_sealed_module
    h = count(daemon, "Row { base")
    rows.append(("③ 焊接点（daemon 生产段）", "恰好 1", 1, (len(h), h)))

    # ④ 🔴 **不许为每家供应商各写一条路** —— 供应商名在**语句**里零命中。
    #    分母 = VENDOR_NAMES 那几个名字（不是「所有供应商」）。
    vend: list[str] = []
    for path, src in all_prod:
        low = src.lower()
        for name in VENDOR_NAMES:
            if name in low:
                vend.append(f"{path}: {name}")
    rows.append(
        (
            f"④ 供应商名进语句（分母 = 我列的 {len(VENDOR_NAMES)} 个名字）",
            "恰好 0",
            0,
            (len(vend), vend),
        )
    )

    # ⑤ 文件格式里那个词（`"bearer"`）的字面量只有一个住址：`AuthStyle::field_value`。
    #    多一处就是闭集有了第二份字面量（`brief` 13b）。
    h = count(core, '"bearer"')
    rows.append(("⑤ 格式词 \"bearer\" 字面量（creds-core 生产段）", "恰好 1", 1, (len(h), h)))

    # ⑥ 那三个格式词**没有**被抄进中转的日志行（日志走现算）。
    #    judge: creds.rs 里那行 `auth_style must be one of:` 拿 `AuthStyle::ALL` 现算
    creds_rs = [(p, s) for p, s in daemon if p.endswith("relay/creds.rs")]
    if len(creds_rs) != 1:
        print(f"FAIL 锚点: relay/creds.rs 不是恰好一份（实得 {len(creds_rs)}）")
        return 1
    computed = count(creds_rs, "AuthStyle::ALL")
    rows.append(("⑥ 中转日志里那份合法值清单是现算的", "恰好 1 处 AuthStyle::ALL", 1, (len(computed), computed)))

    # ⑦ `Base::parse("字面量")` 的调用点，以及其中**带路径**的有几条。
    #
    # 🔴🔴 **这一格有两个分母，它们的数不一样，混用就是一次假读数**〔本工作区最高频那族病〕：
    #   · **生产段**（剥了 `#[cfg(test)]`）—— 今天是 0，而且改前也是 0：
    #     生产段唯一那处 `Base::parse` 收的是**变量**（`resolve_config` 里那个
    #     `upstream_env.unwrap_or(DEFAULT_UPSTREAM)`），不是字面量 ⇒ 正则本来就采不到它。
    #   · **含测试段**（只剥注释）—— 这才是 PM 上一拍那个「全仓 6 处、带路径 0 条」的分母。
    # ⇒ 两个都印，各自标清楚。**别拿生产段那个 0 去对 PM 的 6。**
    #
    # ⚠ 还有第三格诚实边界：这根针是 `Base::parse("…")` —— **只认紧跟着字面量的那一形**。
    #   本轮新加的判据大量用 `for u in [ … ]` 把一串字面量喂进去（那样一条 `Base::parse(u)`
    #   顶好几形）⇒ 那些形状**一条都不进这个数**。⇒ 这个数是「带路径的形状被问过」的**下界**，
    #   不是「一共验了几形」。后者的住址是那几条判据自己写下的分母。
    lit = re.compile(r'Base::parse\(\s*"([^"]*)"')

    def parse_sites(files: list[tuple[str, str]]) -> tuple[list[str], int]:
        sites: list[str] = []
        with_path = 0
        for path, src in files:
            for m in lit.finditer(src):
                arg = m.group(1)
                after = arg.split("://", 1)[-1] if "://" in arg else arg
                has = "/" in after
                sites.append(f"{path}: {arg}{'   ← 带路径' if has else ''}")
                if has:
                    with_path += 1
        return sites, with_path

    prod_sites, prod_with_path = parse_sites(all_prod)
    rows.append(('⑦ Base::parse("字面量") 处数〔分母 = **生产段**〕', "读数（无期望）", None, (len(prod_sites), prod_sites)))
    rows.append(("⑦b 其中带路径的〔生产段〕", "读数（无期望）", None, prod_with_path))

    # 含测试段的那个分母：只剥注释，不剥 `#[cfg(test)]`。
    withtests: list[tuple[str, str]] = []
    for rel in (DAEMON_SRC, MONITOR_SRC, CREDS_CORE_SRC):
        for p in rs_files(root, rel):
            withtests.append(
                (str(p.relative_to(root)), strip_comments(p.read_text(encoding="utf-8")))
            )
    all_sites, all_with_path = parse_sites(withtests)
    rows.append(('⑦c Base::parse("字面量") 处数〔分母 = **含测试段**，PM 那个 6 用的是这个〕', "读数（无期望）", None, (len(all_sites), all_sites)))
    rows.append(("⑦d 其中带路径的〔含测试段；**改前是 0 条**〕", "读数（无期望）", None, all_with_path))

    bad = 0
    for label, want_text, want, got in rows:
        if isinstance(got, tuple):
            n, detail = got
        else:
            n, detail = got, None
        ok = "  " if want is None else ("OK" if n == want else "🔴")
        if want is not None and n != want:
            bad += 1
        print(f"{ok} {label}: 期望 {want_text} · 实得 {n}")
        # 读数格（`want is None`）**总是**把明细印出来 —— 一个不带明细的读数下一轮没人复得出。
        if detail and (want is None or n != want):
            for d in detail if isinstance(detail, list) else [detail]:
                print(f"      {d}")
    print()
    print(f"# 不合期望的格数: {bad}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
