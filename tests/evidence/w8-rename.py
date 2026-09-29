#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""步 8（`设计/99 §4`）改名一刀的**量具与执行器**。

它做三件事，**一件一批**，不混：

  1. `--census <族>`    现打一族名字今天还剩多少处（按文件、按拼写）
  2. `--rename <族>`    按该族的映射表机械替换（带词边界 ＋ **明写的保护名单**）
  3. `--files <族>`     按该族的文件/目录改名表 `git mv`

🔴 三条纪律写进代码，不靠人记着：

  · **保护名单明写**（`PROTECTED`）—— 每一条都带一句「为什么不许动」。
    保护靠**先替换成占位符、最后还原**实现，不靠正则回看 ——
    回看在「`remote-daemon-proto` 里也有 `daemon`」这种嵌套上会漏。
  · **`ccm` / `monitor` 明写为排除**：它们是对外命令名与产品名（`设计/90 §1.1` 两行 ⚠）。
    本脚本**从不**把它们放进任何映射表；`--census` 每次跑都把它们的数印出来当哨兵，
    跑完对不上就是有人碰了它们。
  · **射程明写**：`tests/evidence/` · `CHANGELOG.md` · `vendor/` · 锁文件**不改** ——
    那是记录与第三方，改了等于把历史读数改成假话。
"""

import argparse
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# ── 射程 ────────────────────────────────────────────────────────────────
EXTS = {
    ".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".json", ".toml", ".yml", ".yaml",
    ".sh", ".md", ".css", ".html", ".py", ".ps1", ".txt", ".service", ".conf",
}
# 无扩展名但要扫的脚本
EXTRA_FILES = {
    "src/shared/cc-bus/scripts/cc-send",
    "src/shared/cc-bus/scripts/cc-recv",
    "src/shared/cc-bus/scripts/cc-list",
    "src/shared/cc-bus/scripts/cc-spawn",
    "src/shared/cc-bus/scripts/cc-kill",
    "src/shared/cc-bus/scripts/cc-busd",
    "src/shared/cc-bus/scripts/cc-broadcast",
    "tests/e2e/fake-claude",
}

# 🔴 射程外，逐条带理由
SKIP_PREFIXES = [
    (".git/", "版本库内部"),
    ("node_modules/", "依赖"),
    ("target/", "构建产物"),
    (".build/", "构建产物"),
    ("tests/evidence/", "量具与记录 —— 那是**过去量到的读数**，改了等于把历史改成假话"),
    ("src/vendor/", "第三方 vendored 代码，不是我们的名字"),
    ("src/panorama-engine/vendor/", "第三方 vendored 代码，不是我们的名字"),
    ("src/frontend/shell/icons/", "二进制"),
]
SKIP_FILES = [
    ("CHANGELOG.md", "发版历史 —— 每一条都是当时的原话"),
    ("package-lock.json", "锁文件，由 npm 生成"),
    ("src/frontend/shell/Cargo.lock", "锁文件（`--files` 那一批单独处理包名）"),
    ("src/backend/Cargo.lock", "锁文件（同上）"),
]

# ── 哨兵：这两个名字一格都不许动 ──────────────────────────────────────
SENTINELS = ["ccm", "monitor"]

# ── 🔴 第三档哨兵：**已经写在用户 shell rc 文件里的三条围栏**〔步 8 · 2026-09-19〕
#
# 它们不在那六个名字里，但两条路会误伤：
#   ① `cc-monitor-remote` 的替换 —— `CCM_PROFILE_BEGIN` 里是 `cc-monitor remote ccm`
#      （**空格不是连字符**），正则写松了会命中。⇒ 本脚本的映射表**钉连字符**。
#   ② `relay` / `proto` / `sidecar` 不带词边界时会扫到附近的注释。
#
# 一改 ⇒ 盘上那块**永远对不上**：界面说「未安装」· 卸载按钮消失 · 残留无人提。
# 🔴 而本仓**今天没有任何东西认得上一版围栏** ⇒ 改完之后判据侧会跟着改、**全绿**。
# ⇒ 逐字节钉在这里，`--fences` 每趟现打核一遍。
FENCES = [
    ("# === cc-monitor aliases BEGIN v1 ===",
     "`account_aliases.rs::RC_BEGIN` —— 别名块的起围栏"),
    ("# === cc-monitor BEGIN",
     "`profile_installer.rs::BEGIN_MARKER` —— 本机 profile 块的起围栏"),
    ("# === cc-monitor remote ccm BEGIN ===",
     "`sftp.rs::CCM_PROFILE_BEGIN` —— 远端 ccm 块的起围栏"),
]


def in_scope(rel: str) -> bool:
    for p, _ in SKIP_PREFIXES:
        if rel.startswith(p):
            return False
    for f, _ in SKIP_FILES:
        if rel == f:
            return False
    if rel in EXTRA_FILES:
        return True
    return os.path.splitext(rel)[1] in EXTS


def walk():
    out = []
    for dirpath, dirnames, filenames in os.walk(REPO):
        dirnames[:] = [d for d in dirnames if d not in (".git", "node_modules", "target", ".build")]
        for fn in filenames:
            ap = os.path.join(dirpath, fn)
            rel = os.path.relpath(ap, REPO).replace(os.sep, "/")
            if in_scope(rel) and not os.path.islink(ap):
                out.append((rel, ap))
    return sorted(out)


# ═══════════════════════════════════════════════════════════════════════
# 族定义
# ═══════════════════════════════════════════════════════════════════════
#
# 每一族：
#   needle    —— `--census` 用的大小写不敏感针
#   protected —— [(字面量, 为什么不许动)]，**先冻结后还原**
#   subs      —— [(正则, 替换)]，**按顺序**施加
#   files     —— [(旧相对路径, 新相对路径)]

FAMILIES = {}

# ── daemon ──────────────────────────────────────────────────────────────
FAMILIES["daemon"] = {
    "needle": "daemon",
    "protected": [
        ("remote-daemon-proto",
         "旧 crate **目录名**（住址，不是名字）。目录本身早已改成 `src/backend/`，"
         "留在散文里的是病史。`tool_registry::old_name_counts` 第一步就把这个拼写整个剥掉。"),
        ("remote_daemon_proto",
         "同上，它的 Rust 模块拼写。"),
        ("remote-daemon",
         "**旧的闭集 id**，逐字引用在订正段 / 墓碑 / 病史里。"
         "`tool_registry::Why::OldId` 逐字：「一句话写下时真、后来被别的裁定推翻，"
         "那是历史，不是错误」⇒ **没有解锁条件，它本来就不该被改**。"),
        ("daemonless",
         "🔴 **已落盘的用户配置键**（`remote-config.ts::LEGACY_NO_BACKEND_KEY = \"daemonless\"`）。"
         "那一档虽然 `K-R59` 退役了，但键名还要用来读用户机器上既存的配置；"
         "改了 = 静默丢掉他们存过的那一格。散文里的引用也全部在逐字说这个键。"),
    ],
    "subs": [
        # 中文散文里的「daemon」＝「后端」（`设计/90 §0.5`①「现在没有 daemon，只有 backend」）
        (r"(?<=[一-鿿])\s*daemons?\s*(?=[一-鿿])", "后端"),
        (r"(?<=[一-鿿])\s*daemons?\s*(?=[，。、：；！？「」（）】])", "后端"),
        (r"(?<=[一-鿿])\s+daemons?(?=\s*$)", "后端"),
        # 其余一律机械改名，大小写三形
        (r"daemon", "backend"),
        (r"Daemon", "Backend"),
        (r"DAEMON", "BACKEND"),
    ],
    "files": [
        ("src/daemon-policy.ts", "src/frontend/ui/backend-policy.ts"),
        ("src/frontend/ui/settings/daemon-section.ts", "src/frontend/ui/settings/backend-section.ts"),
        ("tests/daemon-policy.vitest.ts", "tests/frontend/ui/backend-policy.vitest.ts"),
        ("tests/send-into-daemon.vitest.ts", "tests/frontend/ui/send-into-backend.vitest.ts"),
        ("tests/frontend/ui/settings/daemon-section.vitest.ts", "tests/frontend/ui/settings/backend-section.vitest.ts"),
        ("src/frontend/shell/src/daemon_control.rs", "src/frontend/shell/src/backend_control.rs"),
        ("src/frontend/shell/src/daemon_policy.rs", "src/frontend/shell/src/backend_policy.rs"),
        ("src/frontend/shell/src/local_daemon.rs", "src/frontend/shell/src/local_backend.rs"),
        ("src/frontend/shell/src/tmux_daemon_gate_guard.rs", "src/frontend/shell/src/tmux_backend_gate_guard.rs"),
        ("src/frontend/shell/src/backend/control/daemon_kill.rs", "src/frontend/shell/src/backend/control/backend_kill.rs"),
        ("src/frontend/shell/src/backend/control/daemon_launch.rs", "src/frontend/shell/src/backend/control/backend_launch.rs"),
        ("src/frontend/shell/src/backend/control/daemon_route.rs", "src/comms/inward/backend_route.rs"),
        ("src/frontend/shell/src/backend/control/daemon_send_keys.rs", "src/frontend/shell/src/backend/control/backend_send_keys.rs"),
        ("tests/frontend/shell/daemon_control_tests.rs", "tests/frontend/shell/backend_control_tests.rs"),
        ("tests/frontend/shell/daemon_policy_tests.rs", "tests/frontend/shell/backend_policy_tests.rs"),
        ("tests/frontend/shell/local_daemon_tests.rs", "tests/frontend/shell/local_backend_tests.rs"),
        ("tests/frontend/shell/tmux_daemon_gate_guard_tests.rs", "tests/frontend/shell/tmux_backend_gate_guard_tests.rs"),
        ("tests/frontend/shell/doc_claim_registry_daemon_wording_registry.rs", "tests/frontend/shell/doc_claim_registry_backend_wording_registry.rs"),
        ("tests/frontend/shell/doc_claim_registry_frozen_daemon_census.rs", "tests/frontend/shell/doc_claim_registry_frozen_backend_census.rs"),
        ("tests/frontend/shell/backend/control/daemon_kill_tests.rs", "tests/frontend/shell/backend/control/backend_kill_tests.rs"),
        ("tests/frontend/shell/backend/control/daemon_kill_creation_detect.rs", "tests/frontend/shell/backend/control/backend_kill_creation_detect.rs"),
        ("tests/frontend/shell/backend/control/daemon_launch_tests.rs", "tests/frontend/shell/backend/control/backend_launch_tests.rs"),
        ("tests/frontend/shell/backend/control/daemon_route_tests.rs", "tests/comms/inward/backend_route_tests.rs"),
        ("tests/frontend/shell/backend/control/daemon_send_keys_tests.rs", "tests/frontend/shell/backend/control/backend_send_keys_tests.rs"),
        ("tests/e2e/daemon-cc-bus.sh", "tests/e2e/backend-cc-bus.sh"),
        ("tests/e2e/daemon-fork-session.sh", "tests/e2e/backend-fork-session.sh"),
        ("tests/e2e/daemon-gate2-acceptance.sh", "tests/e2e/backend-gate2-acceptance.sh"),
        ("tests/e2e/daemon-sessions-rewatch.sh", "tests/e2e/backend-sessions-rewatch.sh"),
        ("tests/e2e/daemon-tmux-late-server.sh", "tests/e2e/backend-tmux-late-server.sh"),
        ("tests/e2e/daemon-wrapper.sh", "tests/e2e/backend-wrapper.sh"),
        ("tests/e2e/fake-daemon.sh", "tests/e2e/fake-backend.sh"),
        ("tests/e2e/graylight-daemon-frames.sh", "tests/e2e/graylight-backend-frames.sh"),
        ("tests/e2e/inbound-daemon-frames.sh", "tests/e2e/inbound-backend-frames.sh"),
        ("tests/e2e/reap-orphan-daemons.sh", "tests/e2e/reap-orphan-backends.sh"),
        ("tests/e2e/resume-daemon-frames.sh", "tests/e2e/resume-backend-frames.sh"),
        ("tests/e2e/restart-daemon-frames.sh", "tests/e2e/restart-backend-frames.sh"),
    ],
}

# ── cc-monitor-remote ───────────────────────────────────────────────────
FAMILIES["cc-monitor-remote"] = {
    "needle": "cc-monitor-remote",
    "protected": [],
    "subs": [
        (r"cc-monitor-remote", "cc-monitor-backend"),
        (r"cc_monitor_remote", "cc_monitor_backend"),
    ],
    "files": [],
}

# 同一份代码三个身份（`设计/90 §1.2` 的 `K-R70`）：本机释放物与内嵌物归一
FAMILIES["local-native"] = {
    "needle": "cc-monitor-local-",
    "protected": [],
    "subs": [
        (r"cc-monitor-local-", "cc-monitor-backend-"),
        (r"cc-monitor-native-", "cc-monitor-backend-native-"),
    ],
    "files": [],
}

# ── sidecar ─────────────────────────────────────────────────────────────
#
# `设计/90 §0.5.2b`：这个词今天套在两样东西上 ——
#   (a) Tauri `externalBin` 打进 `monitor` 的**本机后端** ⇒ 它就是本机后端，不是挎斗；
#   (b) 后端 `sidecars/` 那套「HTTP 按需拉外部二进制」⇒ **整棵已删**（条 67）。
# ⇒ (b) 没了，这个词就该从我们的词表里消失；只留 Tauri 自己那份词汇。
FAMILIES["sidecar"] = {
    "needle": "sidecar",
    "protected": [
        ("tauri.sidecar.conf.json",
         "🔴 **Tauri 自己的词**：这份配置对应 `externalBin`，而 Tauri 的文档把 `externalBin` "
         "打进去的那个二进制就叫 sidecar。那是**外部世界的事实**，不是我们对自己那一半的称呼 —— "
         "同 `DaemonTransport`（aterm 的类型名）那一档。"),
        ("externalBin", "Tauri 的字段名。"),
        ("code-picture-sidecar", "vendored 内核自己的进程名（`vendor/code-picture-core`）。"),
        ("sidecars/", "已整棵删除的 `src/backend/sidecars/`（条 67，2 008 行）。散文里是墓碑与病史。"),
        ("sidecars", "同上，不带斜杠的那些引用。"),
    ],
    "subs": [
        (r"(?<=[\u4e00-\u9fff])\s*sidecars?\s*(?=[\u4e00-\u9fff])", "本机后端"),
        (r"(?<=[\u4e00-\u9fff])\s*sidecars?\s*(?=[，。、：；！？「」（）】])", "本机后端"),
        (r"SIDECAR", "LOCAL_BACKEND"),
        (r"Sidecar", "LocalBackend"),
        (r"sidecar", "local_backend"),
    ],
    "files": [],
}


def compile_family(fam):
    prot = []
    for i, (lit, _why) in enumerate(fam["protected"]):
        prot.append((lit, "\x00W8P%dP\x00" % i))
    subs = [(re.compile(p), r) for p, r in fam["subs"]]
    return prot, subs


def apply_text(text, prot, subs):
    for lit, ph in prot:
        text = text.replace(lit, ph)
    for rx, rep in subs:
        text = rx.sub(rep, text)
    for lit, ph in prot:
        text = text.replace(ph, lit)
    return text


def cmd_census(name):
    fam = FAMILIES[name]
    needle = fam["needle"]
    rx = re.compile(re.escape(needle), re.I)
    per_file = Counter()
    per_ext = Counter()
    spell = Counter()
    tokrx = re.compile(r"[A-Za-z0-9_.\-]*" + re.escape(needle) + r"[A-Za-z0-9_.\-]*", re.I)
    for rel, ap in walk():
        try:
            t = open(ap, encoding="utf-8").read()
        except (UnicodeDecodeError, OSError):
            continue
        n = len(rx.findall(t))
        if n:
            per_file[rel] = n
            per_ext[os.path.splitext(rel)[1] or "(none)"] += n
            spell.update(tokrx.findall(t))
    print("== 族 `%s` 现打 ==" % name)
    print("总处数 %d（%d 份文件）" % (sum(per_file.values()), len(per_file)))
    for e, n in per_ext.most_common():
        print("  %-8s %d" % (e, n))
    print("-- 前 25 个拼写 --")
    for s, n in spell.most_common(25):
        print("  %5d  %s" % (n, s))
    print("-- 前 20 份文件 --")
    for f, n in per_file.most_common(20):
        print("  %5d  %s" % (n, f))
    return per_file


def sentinel_counts():
    out = {}
    for s in SENTINELS:
        rx = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(s) + r"(?![A-Za-z0-9_])")
        tot = 0
        for rel, ap in walk():
            try:
                tot += len(rx.findall(open(ap, encoding="utf-8").read()))
            except (UnicodeDecodeError, OSError):
                pass
        out[s] = tot
    return out


def cmd_rename(name, dry):
    fam = FAMILIES[name]
    if not fam["subs"]:
        print("族 `%s` 没有机械映射表 —— 它是逐处判的那一种。" % name)
        return
    prot, subs = compile_family(fam)
    before = sentinel_counts()
    changed = 0
    hits = 0
    for rel, ap in walk():
        try:
            t = open(ap, encoding="utf-8").read()
        except (UnicodeDecodeError, OSError):
            continue
        n = apply_text(t, prot, subs)
        if n != t:
            changed += 1
            hits += sum(1 for a, b in zip(t, n) if a != b) and 1
            if not dry:
                open(ap, "w", encoding="utf-8").write(n)
    print("族 `%s`：%d 份文件%s" % (name, changed, "（dry-run）" if dry else " 已改写"))
    if not dry:
        after = sentinel_counts()
        for s in SENTINELS:
            mark = "✅" if before[s] == after[s] else "🔴 动了！"
            print("  哨兵 `%s`：%d → %d  %s" % (s, before[s], after[s], mark))


def cmd_files(name, dry):
    fam = FAMILIES[name]
    for old, new in fam["files"]:
        op = os.path.join(REPO, old)
        if not os.path.exists(op):
            print("  跳过（不在盘上）：%s" % old)
            continue
        print("  git mv %s %s" % (old, new))
        if not dry:
            os.makedirs(os.path.dirname(os.path.join(REPO, new)), exist_ok=True)
            subprocess.check_call(["git", "mv", old, new], cwd=REPO)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--census")
    ap.add_argument("--rename")
    ap.add_argument("--files")
    ap.add_argument("--sentinels", action="store_true")
    ap.add_argument("--fences", action="store_true", help="现打三条围栏的逐字节处数")
    ap.add_argument("--apply", action="store_true", help="真写盘（默认 dry-run）")
    a = ap.parse_args()
    if a.census:
        cmd_census(a.census)
    if a.sentinels:
        for s, n in sentinel_counts().items():
            print("哨兵 `%s`：%d" % (s, n))
    if a.fences:
        for lit, why in FENCES:
            n = 0
            for rel, ap2 in walk():
                try:
                    n += open(ap2, encoding="utf-8").read().count(lit)
                except (UnicodeDecodeError, OSError):
                    pass
            print("围栏 %r：%d 处  —— %s" % (lit, n, why))
    if a.files:
        cmd_files(a.files, not a.apply)
    if a.rename:
        cmd_rename(a.rename, not a.apply)


if __name__ == "__main__":
    sys.exit(main())
