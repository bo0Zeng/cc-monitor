#!/usr/bin/env python3
# 手机冒烟台架的替身 agent（不起真 claude / codex）。rig.sh 在台架的 PATH 里放 `claude` 与 `codex` 两段小壳
# （`exec -a <名> python3 本文件`，CCM_SMOKE_KIND 说扮哪一家）：argv[0] 与进程名都是那一家的名字，tmux 的 pane_current_command 读得到。
#
# 只按那一家的原始记录格式写盘，不碰界面、不判任何东西：
#   claude：<CLAUDE_CONFIG_DIR 或 $HOME/.claude>/sessions/<pid>.json pidfile（idle / busy）＋
#           projects/<编码后的 cwd>/<sid>.jsonl（sid 认 --session-id / --resume，没给就随机）。
#           第一句进来才建记录文件（与真的一样）。每句：user → thinking → tool_use → tool_result → text(end_turn)，
#           逐条隔一小会儿落盘（像真的那样一条一条出），句里带 long ⇒ 答一大段（看往上翻），
#           带 ask ⇒ 停在批准框上（pidfile waiting · permission prompt，看需手动），终端里再敲一行才往下走；
#           带 quiz ⇒ 停在提问框（AskUserQuestion），带 plan ⇒ 停在批计划（ExitPlanMode）。起的时候给了答题 hook
#           （`--settings` 里的 PermissionRequest，ccm 起会话时垫的）⇒ 同真的那样把请求交给它，hook 先给了决定就照它往下走
#           （决定原样落进那一步的结果），终端里先敲了一行也算答（谁先算谁的）。
#           敲进来 `/model <值>` ⇒ 不答，只落一条本地命令输出 ``Set model to `<值>` ``（那一家换模型的读回）。
#   codex： <CODEX_HOME 或 $HOME/.codex>/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl，在世期间一直开着写
#           （那一家「开着即活」）；第一句进来才建文件；每句 task_started … task_complete。
# 终端那一侧：读终端里敲进来的行（规范模式，送字 ＋ 回车就到），每句回显一行，桌面 / 手机两侧打字都进同一处。
# 安全：家目录必须在 CCM_SMOKE_BOX 底下（台架给），不在就拒绝写 —— 宁可起不来，也不往真家目录写。
import datetime
import json
import os
import sys
import time
import uuid

KIND = os.environ.get("CCM_SMOKE_KIND") or os.path.basename(sys.argv[0])
try:  # 进程名（/proc/<pid>/comm）也叫那一家的名字；argv[0] 由 rig.sh 装的那一小段 exec -a 给
    import ctypes
    ctypes.CDLL(None).prctl(15, KIND.encode(), 0, 0, 0)  # PR_SET_NAME
except (OSError, AttributeError):
    pass
BOX = os.environ.get("CCM_SMOKE_BOX", "")
STEP = float(os.environ.get("CCM_SMOKE_STEP", "0.6"))
# 起到能收第一句之前那一段（真的那一家要一两秒：载入 · 信任框）。0 ⇒ 一起来就说闲着（看核心那边起会话交第一句的竞态用）。
BOOT = float(os.environ.get("CCM_SMOKE_BOOT", "2"))


def now_iso():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


def refuse_outside_box(path):
    real = os.path.realpath(path)
    if not BOX or not real.startswith(os.path.realpath(BOX) + os.sep):
        sys.stderr.write(f"fake-agent：{real} 不在台架目录 CCM_SMOKE_BOX 底下 —— 拒绝写\n")
        sys.exit(2)
    return real


def arg_after(flag):
    a = sys.argv[1:]
    for i, x in enumerate(a):
        if x == flag and i + 1 < len(a):
            return a[i + 1]
        if x.startswith(flag + "="):
            return x.split("=", 1)[1]
    return None


def hook_command():
    """起会话时垫的答题 hook（`--settings` 里 hooks.PermissionRequest 的第一条 command 形）；没有 ⇒ None。"""
    raw = arg_after("--settings")
    try:
        for group in (json.loads(raw or "{}").get("hooks") or {}).get("PermissionRequest") or []:
            for h in group.get("hooks") or []:
                if h.get("type") == "command" and h.get("command"):
                    return [h["command"], *(h.get("args") or [])]
    except ValueError:
        pass
    return None


# 三种框：哪个词触发 · 工具名 · 工具参数 · hook 那一份里的建议。
ASK_TOOLS = (
    ("quiz", "AskUserQuestion", {"questions": [
        {"question": "Which color?", "header": "Color", "multiSelect": False,
         "options": [{"label": "Red", "description": "warm"}, {"label": "Blue", "description": ""}]},
        {"question": "Which fruits?", "header": "Fruit", "multiSelect": True,
         "options": [{"label": "Apple", "description": ""}, {"label": "Pear", "description": ""}]}]}, []),
    ("plan", "ExitPlanMode", {"plan": "1. 建 planB.txt，写入 hi\n2. 跑一遍测试"}, []),
    ("ask", "Bash", {"command": "touch probe1.txt\necho ok > probe1.txt", "description": "建探针文件"},
     [{"type": "addDirectories", "directories": ["/w"], "destination": "session"}]),
)


def ask_tool(said):
    return next((x for x in ASK_TOOLS if x[0] in said), None)


def wait_answer(hook, request, readline):
    """框挂着：hook 先给了决定 ⇒ 它（dict）；终端里先敲了一行 ⇒ None。hook 空手退了（改到终端里答 · 别处关了）⇒ 等终端。"""
    import select
    import subprocess
    proc = None
    if hook:
        proc = subprocess.Popen(hook, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        proc.stdin.write(json.dumps(request).encode())
        proc.stdin.close()
    try:
        while True:
            fds = [sys.stdin] + ([proc.stdout] if proc and proc.stdout else [])
            ready, _, _ = select.select(fds, [], [])
            if sys.stdin in ready:
                readline()
                return None
            out = proc.stdout.read()
            proc.wait()
            proc.stdout = None
            if out.strip():
                return json.loads(out)
    finally:
        if proc and proc.poll() is None:
            proc.kill()


def answer_text(said):
    if "long" in said:
        return "\n\n".join(f"第 {i} 段：替身的长回答，用来看往上翻。" * 3 for i in range(1, 41))
    return f"替身收到：{said}"


class Claude:
    def __init__(self):
        home = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(os.environ["HOME"], ".claude")
        self.home = refuse_outside_box(home)
        self.sid = arg_after("--session-id") or arg_after("--resume") or str(uuid.uuid4())  # 带初始提示词起：见 initial_prompt
        self.cwd = os.getcwd()
        self.parent = None
        self.pidfile = os.path.join(self.home, "sessions", f"{os.getpid()}.json")
        os.makedirs(os.path.dirname(self.pidfile), exist_ok=True)
        enc = "".join(c if c.isalnum() else "-" for c in self.cwd)
        self.log = os.path.join(self.home, "projects", enc, f"{self.sid}.jsonl")
        if os.path.exists(self.log):
            with open(self.log) as f:
                for line in f:
                    try:
                        self.parent = json.loads(line).get("uuid") or self.parent
                    except ValueError:
                        pass
        with open(f"/proc/{os.getpid()}/stat") as f:
            self.proc_start = f.read().rsplit(")", 1)[1].split()[19]
        self.started = int(time.time() * 1000)
        time.sleep(BOOT)
        self.status("idle")

    def status(self, s, waiting_for=None):
        tmp = self.pidfile + ".tmp"
        rec = {"pid": os.getpid(), "sessionId": self.sid, "cwd": self.cwd,
               "startedAt": self.started, "procStart": self.proc_start,
               "kind": "interactive", "entrypoint": "cli", "status": s,
               "statusUpdatedAt": int(time.time() * 1000)}
        if waiting_for:
            rec["waitingFor"] = waiting_for
        with open(tmp, "w") as f:
            json.dump(rec, f)
        os.replace(tmp, self.pidfile)

    def put(self, rec):
        u = str(uuid.uuid4())
        rec.update({"uuid": u, "parentUuid": self.parent, "timestamp": now_iso(), "sessionId": self.sid,
                    "cwd": self.cwd, "isSidechain": False, "userType": "external", "version": "0.0.0-smoke"})
        os.makedirs(os.path.dirname(self.log), exist_ok=True)
        with open(self.log, "a") as f:
            f.write(json.dumps(rec, ensure_ascii=False) + "\n")
        self.parent = u
        time.sleep(STEP)

    def assistant(self, mid, block, stop=None):
        self.put({"type": "assistant", "message": {"id": mid, "type": "message", "role": "assistant",
                  "model": "smoke-model", "content": [block], "stop_reason": stop,
                  "usage": {"input_tokens": 10, "output_tokens": 5}}})

    def local_model(self, value):
        """`/model <值>`：那一家当场换、只落一条本地命令输出（不是一轮）。"""
        self.put({"type": "system", "subtype": "local_command", "level": "info",
                  "content": f"<local-command-stdout>Set model to `{value}`</local-command-stdout>"})

    def turn(self, said, ask):
        self.status("busy")
        self.put({"type": "user", "message": {"role": "user", "content": said}})
        mid = "msg_" + uuid.uuid4().hex[:20]
        self.assistant(mid, {"type": "thinking", "thinking": "替身在想。", "signature": "AAAA"})
        call = "toolu_" + uuid.uuid4().hex[:20]
        box = ask_tool(said)
        tool, tinput, suggest = (box[1], box[2], box[3]) if box else ("Bash", {"command": "ls", "description": "看看目录"}, [])
        self.assistant(mid, {"type": "tool_use", "id": call, "name": tool, "input": tinput}, stop="tool_use")
        result = "a.txt\nb.txt"
        if box:  # 停在框上（需手动）：答题 hook 先答 · 终端里先敲一行，谁先算谁的
            self.status("waiting", "permission prompt")
            print(f"[替身] 要批准：{tool}（敲一行放行）", flush=True)
            got = wait_answer(hook_command(), {
                "hook_event_name": "PermissionRequest", "session_id": self.sid, "cwd": self.cwd,
                "tool_name": tool, "tool_input": tinput, "permission_suggestions": suggest}, ask)
            if got is not None:
                decision = (got.get("hookSpecificOutput") or {}).get("decision") or {}
                result = "答题口：" + json.dumps(decision, ensure_ascii=False, sort_keys=True)
                print(f"[替身] {result}", flush=True)
            self.status("busy")
        self.put({"type": "user", "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": call, "content": result, "is_error": False}]},
            "toolUseResult": {"stdout": result, "stderr": "", "interrupted": False}})
        mid = "msg_" + uuid.uuid4().hex[:20]
        self.assistant(mid, {"type": "text", "text": answer_text(said)}, stop="end_turn")
        self.status("idle")


class Codex:
    def __init__(self):
        home = os.environ.get("CODEX_HOME") or os.path.join(os.environ["HOME"], ".codex")
        self.home = refuse_outside_box(home)
        self.sid = None
        self.f = None
        self.cwd = os.getcwd()
        self.n = 0

    def put(self, typ, payload):
        self.f.write(json.dumps({"timestamp": now_iso(), "type": typ, "payload": payload}, ensure_ascii=False) + "\n")
        self.f.flush()
        time.sleep(STEP)

    def open(self):
        self.sid = str(uuid.uuid4())
        d = datetime.datetime.now()
        path = os.path.join(self.home, "sessions", d.strftime("%Y/%m/%d"),
                            f"rollout-{d.strftime('%Y-%m-%dT%H-%M-%S')}-{self.sid}.jsonl")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        self.f = open(path, "a")  # 在世期间一直开着（那一家「开着即活」）
        self.put("session_meta", {"id": self.sid, "timestamp": now_iso(), "cwd": self.cwd,
                                  "originator": "codex_cli_rs", "cli_version": "0.0.0-smoke", "instructions": None})

    def turn(self, said, ask):
        if self.f is None:
            self.open()
        self.n += 1
        t = f"t{self.n}"
        self.put("turn_context", {"turn_id": t, "cwd": self.cwd, "approval_policy": "on-request", "model": "smoke-model"})
        self.put("event_msg", {"type": "task_started", "turn_id": t})
        self.put("response_item", {"type": "message", "role": "user", "content": [{"type": "input_text", "text": said}]})
        self.put("event_msg", {"type": "user_message", "message": said})
        self.put("response_item", {"type": "reasoning", "summary": [{"type": "summary_text", "text": "替身在想。"}],
                                   "encrypted_content": "AAAA"})
        call = "call_" + uuid.uuid4().hex[:12]
        self.put("response_item", {"type": "function_call", "name": "shell", "call_id": call,
                                   "arguments": json.dumps({"command": ["ls"]})})
        self.put("response_item", {"type": "function_call_output", "call_id": call,
                                   "output": json.dumps({"output": "a.txt\nb.txt", "metadata": {"exit_code": 0}})})
        text = answer_text(said)
        self.put("response_item", {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]})
        self.put("event_msg", {"type": "agent_message", "message": text})
        self.put("event_msg", {"type": "task_complete", "turn_id": t, "last_agent_message": text})


def initial_prompt():
    """起的时候带的第一句：`--` 之后那一段（那一家「带初始提示词起」的形状）。"""
    a = sys.argv[1:]
    return " ".join(a[a.index("--") + 1:]).strip() if "--" in a else ""


def main():
    agent = Codex() if KIND == "codex" else Claude()
    print(f"[替身 {KIND}] 就绪，敲一句回车。", flush=True)
    first = initial_prompt()
    if first:
        print(f"> {first}", flush=True)
        agent.turn(first, sys.stdin.readline)
        print("[替身] 答完。", flush=True)
    for line in sys.stdin:
        said = line.strip()
        if not said:
            continue
        print(f"> {said}", flush=True)
        if said.startswith("/model ") and hasattr(agent, "local_model"):
            agent.local_model(said[len("/model "):].strip())
            continue
        agent.turn(said, sys.stdin.readline)
        print("[替身] 答完。", flush=True)


if __name__ == "__main__":
    main()
