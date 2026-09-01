#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""K-R12 · Bx 量具之三：**有没有任何判据看得见这条静默失效** —— 现打分母。

住址（唯一）：<树根>/evidence/k-r12-bx-judgement-denominator.py
被测对象：本量具所在那棵树的**全部** `.rs` 测试函数 + 门禁脚本 + 判据登记表。

分母怎么数的（逐条写死，别让读的人猜）：
  D1 = 全树 `.rs` 里 `#[test]` / `#[tokio::test]` 属性的条数（含 `#[ignore]`）。
  D2 = D1 中**函数体**里出现 `tmux`（不分大小写）的条数。
  D3 = D2 中会**真起一个 tmux 进程**的条数（体内出现 `Command::new("tmux")`
       或 `new-session` 或 `"tmux"` 作为 argv 首项）。
  D4 = D3 中**解析多字段分隔输出**的条数（体内同时出现 `split('\\t')` 或 `\\t` 分隔的
       格式串）—— 这一格才是「能看见 `_`」的必要条件。
  D5 = D1 中提到 `query_tmux_server` / `server_pid` / `socket_path` 的条数。
  E1 = 失效路径上有几条日志会打（人肉点名 + 现打核对）。

⚠ 口径边界：这把尺子按**文本**认，不按语义认；它会漏掉「靠 helper 间接起 tmux」的写法。
   所以 D3/D4 是**下界**，报的时候要连这句一起报。
用法：python3 evidence/k-r12-bx-judgement-denominator.py
"""

import hashlib
import os
import re
import subprocess
import sys

SELF = os.path.abspath(__file__)
SELF_DIR = os.path.dirname(SELF)
SUBJECT_REL = "remote-daemon-proto/src/observe/watcher.rs"


def run(argv):
    return subprocess.run(argv, capture_output=True)


def preamble():
    root = run(["git", "-C", SELF_DIR, "rev-parse", "--show-toplevel"]).stdout.decode().strip()
    head = run(["git", "-C", root, "rev-parse", "HEAD"]).stdout.decode().strip()
    subj = os.path.join(root, SUBJECT_REL)
    md5 = hashlib.md5(open(subj, "rb").read()).hexdigest()
    st = run(["git", "-C", root, "status", "--porcelain"]).stdout.decode("utf-8", "replace")
    dirty = len([x for x in st.splitlines() if x.strip()])
    print("=" * 78)
    print("K-R12 · Bx 量具之三（判据分母）")
    print("  量具住址      : %s" % SELF)
    print("  树根          : %s" % root)
    print("  HEAD          : %s" % head)
    print("  被测文件 md5  : %s  (%s)" % (md5, SUBJECT_REL))
    print("  未提交改动处数: %d" % dirty)
    print("=" * 78)
    return root


def rs_files(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames
                       if d not in (".git", "target", "node_modules", "dist")]
        for fn in filenames:
            if fn.endswith(".rs"):
                out.append(os.path.join(dirpath, fn))
    return sorted(out)


TEST_ATTR = re.compile(r"#\[(?:tokio::)?test\]")


def mask_rust(src):
    """把注释 / 字符串 / 字符字面量的**内容**换成同长空格，只留结构性的 `{}`。

    🔴 为什么必须有这一步（第一版没有，读数是错的）：
       `format!("... {} ... {{")` 里那对 `{{` 会被裸的括号计数当成两个开括号 ⇒
       函数体的终点被算飞，把后面几百行**别人的代码**吞进"这个测试的体"里。
       实测后果：`tmux_reprobe_triggers_on_sid_drift_not_on_every_json_event`
       （一条纯 `include_str!` 扫源码的测试，一个 tmux 进程都不起）被误判成
       「真起进程 + 体内含 `#{pid}` / `#{socket_path}`」⇒ D4 假读出 1。
    """
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            j = src.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
        elif c == "/" and i + 1 < n and src[i + 1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src[j:j + 2] == "/*":
                    depth += 1
                    j += 2
                elif src[j:j + 2] == "*/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            for k in range(i, min(j, n)):
                if out[k] != "\n":
                    out[k] = " "
            i = j
        elif c == "r" and i + 1 < n and src[i + 1] in '"#':
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                e = src.find(close, j + 1)
                e = n if e < 0 else e + len(close)
                for k in range(i, e):
                    if out[k] != "\n":
                        out[k] = " "
                i = e
            else:
                i += 1
        elif c == '"':
            j = i + 1
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    j += 1
                    break
                j += 1
            for k in range(i, min(j, n)):
                if out[k] != "\n":
                    out[k] = " "
            i = j
        elif c == "'":
            # 字符字面量 vs 生命周期：只有形如 'x' / '\n' / '\u{..}' 才是字面量
            m = re.match(r"'(?:\\u\{[0-9a-fA-F]+\}|\\.|[^\\'])'", src[i:])
            if m:
                for k in range(i, i + m.end()):
                    out[k] = " "
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def fn_bodies(src):
    """返回 [(fn_name, body)]：每个 #[test]/#[tokio::test] 之后那个 fn 的体。

    括号配对走 `mask_rust` 的掩码副本，取回的 body 切自**原文**。
    """
    masked = mask_rust(src)
    out = []
    for m in TEST_ATTR.finditer(masked):
        i = masked.find("fn ", m.end())
        if i < 0:
            continue
        j = masked.find("(", i)
        name = src[i + 3:j].strip() if j > 0 else "?"
        k = masked.find("{", j if j > 0 else i)
        if k < 0:
            continue
        depth, p = 0, k
        while p < len(masked):
            if masked[p] == "{":
                depth += 1
            elif masked[p] == "}":
                depth -= 1
                if depth == 0:
                    break
            p += 1
        out.append((name, src[k:p + 1]))
    return out


def main():
    root = preamble()
    files = rs_files(root)
    print("\n### 扫描面（分母的分母）")
    print("  扫的是: 全树 *.rs（排除 .git / target / node_modules / dist）")
    print("  文件数: %d" % len(files))

    d1 = []
    for f in files:
        try:
            src = open(f, encoding="utf-8").read()
        except UnicodeDecodeError:
            continue
        for name, body in fn_bodies(src):
            d1.append((os.path.relpath(f, root), name, body))

    # 自检：函数体里不该再套着另一个 `#[test]` —— 套了就说明括号配对又算飞了。
    # ⚠ 自检也要用**掩码后**的体：原文里那 5 条命中全在字符串字面量里（登记表测试把
    #   `#[test]` 当数据写着），拿原文自检等于给自己报一次假警。
    overlap = [x for x in d1 if TEST_ATTR.search(mask_rust(x[2]))]
    longest = max(d1, key=lambda x: len(x[2])) if d1 else None
    print("\n### 尺子自检（第一版就是在这里静默出错的）")
    print("  函数体里又套到 #[test] 的条数（该是 0）: %d %s"
          % (len(overlap), [x[1] for x in overlap[:5]]))
    print("  最长函数体: %s (%s) = %d 字符"
          % (longest[1], longest[0], len(longest[2])) if longest else "  （无）")

    d2 = [x for x in d1 if "tmux" in x[2].lower()]
    d3 = [x for x in d2 if ('Command::new("tmux")' in x[2]
                            or "new-session" in x[2]
                            or '"tmux",' in x[2])]
    d4 = [x for x in d3 if ("split('\\t')" in x[2] or "\\t#{" in x[2]
                            or "}\\t" in x[2])]
    d5 = [x for x in d1 if re.search(r"query_tmux_server|server_pid|socket_path", x[2])]

    print("\n### 逐格读数")
    print("  D1 全树 #[test]/#[tokio::test] 函数数            : %d" % len(d1))
    print("  D2   其中函数体提到 tmux 的                      : %d" % len(d2))
    print("  D3     其中会真起 tmux 进程的（文本认，下界）    : %d" % len(d3))
    print("  D4       其中解析多字段 TAB 分隔输出的           : %d" % len(d4))
    print("  D5 全树提到 query_tmux_server/server_pid/socket_path 的: %d" % len(d5))

    print("\n### D3 逐条点名（能看见真 tmux 输出的全部候选）")
    for f, n, b in d3:
        real = 'Command::new("tmux")' in b
        seps = sorted(set(re.findall(r"#\{[a-z_@?][^}]*\}", b)))
        print("  · %-46s %s" % (n, f))
        print("      真起进程=%s  体内格式串=%s" % (real, seps if seps else "（无）"))

    print("\n### D4 逐条点名（**这一格是空的就等于没人看得见**）")
    if not d4:
        print("  （空）")
    for f, n, b in d4:
        print("  · %-46s %s" % (n, f))

    print("\n### D5 逐条点名")
    for f, n, b in d5:
        print("  · %-46s %s" % (n, f))

    print("\n### E1 失效路径上会打的日志（点名，不写「一整套」）")
    subj = os.path.join(root, SUBJECT_REL)
    src = open(subj, encoding="utf-8").read()
    lines = src.splitlines()

    def seg(a, b, label):
        body = "\n".join(lines[a - 1:b])
        logs = re.findall(r"tracing::(trace|debug|info|warn|error)!", body)
        print("  · %-42s 行 %d-%d  日志条数=%d %s"
              % (label, a, b, len(logs), sorted(set(logs))))
        return len(logs)

    # query_tmux_server 函数体
    i = src.find("fn query_tmux_server()")
    la = src[:i].count("\n") + 1
    j = src.find("\n}\n", i)
    lb = src[:j].count("\n") + 1
    seg(la, lb, "query_tmux_server 全体")
    # 解析那 4 行（从 let text 到 (pid, sock)）
    k = src.find("let text = String::from_utf8_lossy(&out.stdout);", i)
    ka = src[:k].count("\n") + 1
    seg(ka, lb, "  其中「解析两段」那一段")
    # `_ => {}` 那条臂
    m = re.search(r"None if matches!\(obs, TmuxObservation::NoServer\).*?_ => \{\}", src, re.S)
    if m:
        ma = src[:m.start()].count("\n") + 1
        mb = src[:m.end()].count("\n") + 1
        seg(ma, mb, "match probe.server_pid 的 None/_ 两臂")
        arm = src[m.end() - 8:m.end()]
        print("      `_` 臂逐字 = %r" % arm)
    # socket inotify 那一段
    n2 = src.find("if let Some(sock) = probe.socket_path.as_ref()")
    if n2 > 0:
        na = src[:n2].count("\n") + 1
        seg(na, na + 22, "socket_path 为 None 时**整段被跳过**")
        print("      ⇒ 跳过时 0 条日志（`if let` 没有 else 臂）")

    print("\n### 门禁 / 判据登记面（另一族分母）")
    for rel in ("scripts/gate.sh", "src-tauri/src/exec_site_registry.rs",
                "remote-daemon-proto/src/readonly_guard.rs"):
        p = os.path.join(root, rel)
        if not os.path.exists(p):
            print("  · %-46s （不存在）" % rel)
            continue
        t = open(p, encoding="utf-8", errors="replace").read()
        hits = len(re.findall(r"display-message|socket_path|#\{pid\}", t))
        print("  · %-46s 提到 display-message/socket_path/#{pid} 的处数 = %d"
              % (rel, hits))


if __name__ == "__main__":
    main()
