#!/usr/bin/env python3
"""K-R48 现打对拍：同一套 argv 喂给**旧 bash `shared/ccm`** 与**新原生命令**（后端二进制），
逐字节比 stdout + stderr + 退出码。

# 它买的是什么

PM 09-11 的问题是「那四套 e2e（356 条）删掉之后，(b) 那类『一条起会话命令长什么样』
的契约还剩多少」。这份读数给的是**另一个答案**：那四套多半**不必重写，
只要把 `CCM=$REPO/shared/ccm` 换成那个二进制** —— 两边输出本来就一样。

# 09-11 现打读数：**SAME=27 · DIFF=2**

两条 DIFF 都是**有意的**：`--version` 由 `ccm 4` 变 `ccm 5`、`--ccm-probe` 的同一个
`version=` 字段（其余四行逐字相同）。实现换了、版本号跟着走，是 `KCY4` 那条既有纪律。

# ⚠ 射程（写死，别读宽）

只比 **`--print` 与报错出口**这两面 —— 那是本 CLI 的**平价预言机**。
真起会话 / 真 attach / 真 tmux 那一面**不在这份读数里**：那要真 tmux，本轮没跑。

# 为什么是 .py 不是 .sh

头一版是 `.sh`，`shell_lint_registry::every_shell_script_is_either_linted_or_registered_as_exempt`
当场把它逮住了（仓里每个 shell 脚本要么进 CI 的 shellcheck 表达式、要么登记豁免）。
那条判据是对的，而给一份一次性的对拍台架去改 CI 的 lint 清单不划算 ⇒ 换语言。

跑法（沙箱内，工作树根）：
    cd remote-daemon-proto && cargo build --bin cc-monitor-remote && cd ..
    python3 evidence/K-R48-native-vs-bash-parity.py
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = pathlib.Path(
    os.environ.get("CARGO_TARGET_DIR", str(ROOT / "remote-daemon-proto" / "target"))
) / "debug" / "cc-monitor-remote"
CCM = ROOT / "shared" / "ccm"

# (标签, argv…)
CASES = [
    ("零修饰", ["--cwd", "/p", "--print"]),
    ("resume 位置形", ["resume", "abc-123", "--cwd", "/p", "--launcher", "claude", "--print"]),
    ("--resume 旗标形", ["--resume", "abc-123", "--cwd", "/p", "--launcher", "claude", "--print"]),
    ("--resume= 等号形", ["--resume=abc-123", "--cwd", "/p", "--launcher", "claude", "--print"]),
    ("--base", ["--cwd", "/p", "--base", "--print"]),
    ("--model", ["--cwd", "/p", "--model", "opus", "--print"]),
    ("--agent codex", ["--cwd", "/p", "--agent", "codex", "--print"]),
    ("--launcher 覆盖", ["--cwd", "/p", "--launcher", "/x/y", "--print"]),
    ("-- 透传", ["--cwd", "/p", "--print", "--", "-p", "hi there"]),
    ("--account z", ["--cwd", "/p", "--account", "z", "--print"]),
    ("裸终端落默认号", ["--cwd", "/p", "--print"]),
    ("attach", ["attach", "cc-p1", "--print"]),
    ("容器路 --tmux=名", ["--tmux=cc-p1", "--cwd", "/tmp", "--ccm-sid", "p1", "--base", "--print"]),
    ("容器路 派生名", ["--tmux", "--cwd", "/home/pi/my proj", "--print"]),
    ("容器路 --detach+尺寸", ["--tmux=n1", "--cwd", "/p", "--detach", "--tmux-size", "220x50", "--print"]),
    ("容器路 resume+model", ["resume", "p1", "--tmux=cc-p1", "--cwd", "/tmp", "--model", "opus",
                             "--ccm-sid", "p1", "--base", "--print"]),
    ("err 未知选项", ["--nope"]),
    ("err 未知 agent", ["--agent", "gemini"]),
    ("err account/base 互斥", ["--account", "z", "--base"]),
    ("err resume 缺 sid", ["resume", "--tmux"]),
    ("err attach 缺名", ["attach", "--tmux"]),
    ("err detach 无 tmux", ["--detach"]),
    ("err 非法尺寸", ["--tmux", "--tmux-size", "x50"]),
    ("err 多余位置参数", ["foo"]),
    ("err codex 不支持 resume", ["resume", "s", "--agent", "codex"]),
    ("err bus-note 无登记", ["--bus-note", "x"]),
    ("err tmux 名互斥", ["--tmux=a", "--tmux-base", "b"]),
    ("--version", ["--version"]),
    ("--ccm-probe", ["--ccm-probe"]),
]

FAKE_DAEMON = """#!/bin/sh
# 只答 --list-accounts：bash 那侧要跨进程问它，二进制那侧自己读 manifest。
case "$1" in
  --list-accounts)
    d=""; while [ $# -gt 0 ]; do [ "$1" = --accts-dir ] && d="$2"; shift; done
    printf '{"kind":"accounts-meta"}\\n'
    [ -f "$d/accounts.json" ] && python3 -c '
import json,sys
for a in json.load(open(sys.argv[1]))["accounts"]:
    print(json.dumps({"name":a["name"],"configDir":a.get("configDir"),
                      "isDefault":a.get("isDefault",False)},separators=(",",":")))
' "$d/accounts.json" ;;
esac
exit 0
"""


def main():
    if not BIN.is_file():
        print(f"CRASH：二进制不在 {BIN} —— 先 `cargo build --bin cc-monitor-remote`", file=sys.stderr)
        return 2
    if not CCM.is_file():
        print(f"CRASH：`shared/ccm` 不在 {CCM} —— 这份对拍要两边都在才跑得了", file=sys.stderr)
        return 2
    w = pathlib.Path(tempfile.mkdtemp(prefix="ccm-parity-"))
    try:
        (w / "bin").mkdir()
        (w / "z").mkdir()
        (w / "p").mkdir()
        shutil.copy2(BIN, w / "bin" / "ccm")
        (w / "bin" / "ccm").chmod(0o755)
        (w / "bin" / "faked").write_text(FAKE_DAEMON)
        (w / "bin" / "faked").chmod(0o755)
        (w / "accounts.json").write_text(json.dumps({"accounts": [
            {"name": "z", "configDir": str(w / "z"), "isDefault": True},
            {"name": "b", "configDir": str(w / "z")},
        ]}))
        env = dict(os.environ)
        for k in ("TMUX", "CLAUDE_CONFIG_DIR"):
            env.pop(k, None)
        env.update({
            "CCM_SELF": "/usr/local/bin/ccm",
            "CCM_CONFIG": "/nonexistent",
            "CCM_ACCTS_MANIFEST": str(w / "accounts.json"),
            "CCM_DAEMON_BIN": str(w / "bin" / "faked"),
            "HOME": str(w),
        })

        def run(argv):
            p = subprocess.run(argv, env=env, capture_output=True, text=True, cwd=str(w))
            return p.stdout + p.stderr + f"rc={p.returncode}\n"

        same = diff = 0
        for label, argv in CASES:
            a = run(["bash", str(CCM), *argv])
            b = run([str(w / "bin" / "ccm"), *argv])
            if a == b:
                same += 1
                print(f"SAME | {label}")
            else:
                diff += 1
                print(f"DIFF | {label}\n  bash: {a!r}\n  bin : {b!r}")
        print(f"===== 合计 SAME={same} DIFF={diff} =====")
        return 0
    finally:
        shutil.rmtree(w, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
