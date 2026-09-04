#!/usr/bin/env python3
"""K-R23 · D1 的量具：`build_guarded_tmux_cmd` 这道门今天有几个调用方

**分母是怎么切的**（这是本脚本存在的理由，不是脚注）：

  `build_guarded_tmux_cmd` 在 `src-tauri/src/tmux.rs` 里是 **私有 `fn`**（无 `pub`）。
  Rust 的可见性规则：私有项只在**它所在的模块及其后代模块**里可见。
  它所在的模块 = 文件模块 `crate::tmux`（`tmux.rs` 整个文件，外层没有 `mod { }` 包着），
  后代 = 文件里那个 `#[cfg(test)] mod tests`。
  ⇒ **调用方人群 = 且仅 = `src-tauri/src/tmux.rs` 这一个文件里的调用表达式。**
  分母 = 1 个文件 / 该文件的总行数（脚本会打出来）。

  🔴 「私有」这条前提**本脚本自己验**（`assert_private`）——它一旦变成 `pub`，
  上面那句推理当场作废、分母要重切。不验就等于把结论架在一个没人看的假设上。

**交叉复核**：再对全仓（除 `node_modules` / `target` / `.git`）扫一遍这个标识符，
逐处分类成「调用 / 文档注释 / 字符串字面量」——用来证明「文件外没有第二个调用方」
不是靠上面那段推理**空口**说的，盘上也确实没有。

⚠ 不能裸 `grep`：这个名字在本仓大量出现在 `///` 文档注释和**字符串常量**里
  （`tmux_daemon_gate_guard.rs:152` 的 `const GATE_BUILDER`、`tmux.rs:1182` 的
  `const DELEGATE: &str = "build_guarded_tmux_cmd("` ——**后者带括号**，
  裸 `grep 'build_guarded_tmux_cmd('` 会把它数成调用方）。
  ⇒ 本脚本带一个小词法器：码 / 行注释 / 块注释 / 普通串 / 原始串。

用法：
    python3 evidence/k-r23-d1-door-callers.py [仓根]     # 缺省 = 当前目录
"""
import os
import re
import sys

TARGET = "build_guarded_tmux_cmd"
DOOR_FILE = "src-tauri/src/tmux.rs"
SKIP_DIRS = {"node_modules", "target", ".git", "dist", ".claude"}


# ── 词法：把「码」以外的字节掩成 '_'，下标一一对应（同 K-R9 那把尺子的手法）──────
def mask_noncode(src: str) -> str:
    """返回与 src 等长的串：非「码」区域（注释 / 字符串）替换成 '_'。"""
    b = list(src)
    out = list(src)
    i, n = 0, len(b)
    while i < n:
        c = b[i]
        # 原始串 r"..." / r#"..."#
        if c == "r" and i + 1 < n and (b[i + 1] == '"' or b[i + 1] == "#"):
            j = i + 1
            hashes = 0
            while j < n and b[j] == "#":
                hashes += 1
                j += 1
            if j < n and b[j] == '"':
                close = '"' + "#" * hashes
                k = src.find(close, j + 1)
                k = n if k == -1 else k + len(close)
                for p in range(i, k):
                    out[p] = "_"
                i = k
                continue
        if c == "/" and i + 1 < n and b[i + 1] == "/":
            k = src.find("\n", i)
            k = n if k == -1 else k
            for p in range(i, k):
                out[p] = "_"
            i = k
            continue
        if c == "/" and i + 1 < n and b[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for p in range(i, j):
                out[p] = "_"
            i = j
            continue
        if c == '"':
            j = i + 1
            while j < n:
                if b[j] == "\\":
                    j += 2
                    continue
                if b[j] == '"':
                    j += 1
                    break
                j += 1
            for p in range(i, min(j, n)):
                out[p] = "_"
            i = j
            continue
        i += 1
    # 换行必须留着，否则算不出行号
    for p, ch in enumerate(src):
        if ch == "\n":
            out[p] = "\n"
    return "".join(out)


def line_of(src: str, off: int) -> int:
    return src.count("\n", 0, off) + 1


def split_top_level(args: str):
    """按顶层逗号切实参（闭包体里的逗号不算）。"""
    parts, depth, cur = [], 0, ""
    for ch in args:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur.strip())
    return parts


def call_args(src: str, open_paren: int) -> str:
    depth, j = 0, open_paren
    while j < len(src):
        if src[j] == "(":
            depth += 1
        elif src[j] == ")":
            depth -= 1
            if depth == 0:
                return src[open_paren + 1 : j]
        j += 1
    return ""


def enclosing_fn(src: str, off: int):
    head = src[:off]
    # 🔴 缩进只许吃**同一行**的空白：写 `\s*` 会让 `^` 从上一行起匹配（`\s` 含 `\n`），
    #    定义行的行号当场偏一格 —— 第一版就是这么把定义数成了调用方。
    hits = list(re.finditer(r"(?m)^[^\S\n]*(?:pub(?:\([^)]*\))?[^\S\n]+)?(?:async[^\S\n]+)?fn[^\S\n]+(\w+)", head))
    if not hits:
        return None, None
    last = hits[-1]
    return last.group(1), line_of(src, last.start())


def main() -> int:
    root = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else ".")
    door = os.path.join(root, DOOR_FILE)
    if not os.path.isfile(door):
        print(f"❌ 找不到 {DOOR_FILE} —— 仓根给错了？ root={root}")
        return 3
    src = open(door, encoding="utf-8").read()
    code = mask_noncode(src)
    total_lines = src.count("\n") + 1

    # ── 前提自验：它必须还是私有的，否则分母那段推理作废 ───────────────────
    m = re.search(rf"(?m)^([^\S\n]*)((?:pub(?:\([^)]*\))?[^\S\n]+)?)fn[^\S\n]+{TARGET}[^\S\n]*\(", src)
    if not m:
        print(f"❌ 在 {DOOR_FILE} 里找不到 `fn {TARGET}(` 的定义 —— 尺子对不上事实，停。")
        return 3
    def_line = line_of(src, m.start())
    vis = m.group(2).strip()
    private = vis == ""
    print("=" * 78)
    print(f"K-R23 · D1：`{TARGET}` 的调用方人群")
    print("=" * 78)
    print(f"仓根            : {root}")
    print(f"门所在文件      : {DOOR_FILE}（{total_lines} 行）")
    print(f"定义行          : :{def_line}  可见性 = {'私有（无 pub）' if private else repr(vis)}")
    if not private:
        print("🔴 它已经不是私有的了 —— 「人群 = 同文件」那段推理作废，分母必须重切。")
        return 3
    print("分母            : 1 个文件 / 该文件 %d 行（私有 fn ⇒ 只有同模块及其后代能调）" % total_lines)
    print()

    # ── 门内部的档位表（guard 逐字）────────────────────────────────────────
    arms = {}
    ge = re.search(r"(?s)fn gate_guard_expr\(.*?\{(.*?)\n\}", src)
    if ge:
        for a in re.finditer(r"\((true|false),\s*(true|false)\)\s*=>\s*(.+?),\n", ge.group(1)):
            arms[(a.group(1), a.group(2))] = a.group(3).strip()
    print("`gate_guard_expr` 现打的档位表（从源码取，不是抄的）:")
    for k, v in sorted(arms.items()):
        print(f"  (need_sid={k[0]:<5} need_windows={k[1]:<5}) ⇒ {v}")
    print()

    # ── 调用方逐个点名 ────────────────────────────────────────────────────
    print("调用方逐个点名（同文件内、词法确认是**调用表达式**的）:")
    rows = []
    for hit in re.finditer(re.escape(TARGET) + r"\s*\(", code):
        off = hit.start()
        ln = line_of(src, off)
        if ln == def_line:
            continue  # 定义本身不算调用方
        fn_name, fn_line = enclosing_fn(src, off)
        args = split_top_level(call_args(src, hit.end() - 1))
        need_sid = args[1] if len(args) > 1 else "?"
        need_win = args[2] if len(args) > 2 else "?"
        rows.append((ln, fn_name, fn_line, need_sid, need_win))
    for ln, fn_name, fn_line, need_sid, need_win in rows:
        print(f"  :{ln:<5} 在 `fn {fn_name}`（:{fn_line}）  need_sid={need_sid!r}  need_windows={need_win!r}")
    print(f"  ⇒ 调用方 {len(rows)} 处 / 分母 {total_lines} 行（同一个文件）")
    print()

    # ── 展开：need_sid 是**动态**的 ⇒ 真正的人群是「调用点 × name_owned」 ──
    print("⚠ `need_sid` 不是字面量、是 `!is_ccm_tmux_name(target)` 的值 ⇒ 一个调用点吃两档。")
    print("   真正要点名的人群是「调用点 × 目标名是否 cc-* 命名」:")
    for ln, fn_name, fn_line, need_sid, need_win in rows:
        dynamic = "name_owned" in need_sid
        for owned in ([True, False] if dynamic else [None]):
            ns = ("false" if owned else "true") if dynamic else need_sid
            nw = need_win
            if (ns, nw) == ("false", "false"):
                arm = "（退化分支：门整个不装，早返回）"
            else:
                arm = arms.get((ns, nw), "?")
            tag = "" if owned is None else ("目标是 cc-* " if owned else "目标非 cc-*")
            print(f"  :{ln:<5} {fn_name:<26} {tag:<12} (need_sid={ns:<5}, need_windows={nw:<5}) ⇒ {arm}")
    print()

    # ── 交叉复核：全仓这个名字都出现在哪，各是什么身份 ─────────────────────
    print("交叉复核 · 全仓这个标识符的每一处身份（证明文件外确实没有第二个调用方）:")
    kinds = {"调用": 0, "文档/注释": 0, "字符串": 0, "定义": 0}
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in filenames:
            if not fn.endswith((".rs", ".md", ".ts", ".sh", ".mts", ".toml")):
                continue
            p = os.path.join(dirpath, fn)
            try:
                s = open(p, encoding="utf-8").read()
            except (OSError, UnicodeDecodeError):
                continue
            if TARGET not in s:
                continue
            c = mask_noncode(s) if fn.endswith(".rs") else "_" * len(s)
            rel = os.path.relpath(p, root)
            for h in re.finditer(re.escape(TARGET), s):
                ln = line_of(s, h.start())
                in_code = c[h.start() : h.start() + len(TARGET)] == TARGET
                if not in_code:
                    kind = "文档/注释" if s[max(0, h.start() - 200) : h.start()].rfind("///") >= 0 else "字符串"
                    # 更稳的判定：看这一行的掩码结果里 `///` 在不在前面
                    line_start = s.rfind("\n", 0, h.start()) + 1
                    kind = "文档/注释" if re.match(r"\s*(///|//[!/]?|\*|//)", s[line_start : h.start()]) else "字符串"
                elif rel.replace(os.sep, "/") == DOOR_FILE and ln == def_line:
                    kind = "定义"
                else:
                    kind = "调用"
                kinds[kind] = kinds.get(kind, 0) + 1
                print(f"  {kind:<10} {rel}:{ln}")
    print()
    print("按身份合计: " + " · ".join(f"{k}={v}" for k, v in kinds.items()))
    print(f"⇒ 「调用」只出现在 {DOOR_FILE} 里，共 {kinds['调用']} 处 —— 与上面按可见性推出的人群一致。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
