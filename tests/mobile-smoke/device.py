#!/usr/bin/env python3
# 真机代测（模拟器 ＋ 沙箱后端，替身 agent）：只核真机那几样，每项末尾一屏亮暗各一张，量出来的数进 results.json。
# 入口是同目录 device.sh（编后端 · 起台架 · 起模拟器 · 编并装正式包）；按键 · 找字 · 加机器这些零件借冒烟 smoke.py 的（不改它那十步）。
# 每项的判法只看屏上 · 台架记录（commands.log · tmux）· 系统自己报的（dumpsys）；判不出的写「没核」，不当过。
#   1 正式包 arm64 经 ARM 转译：系统报这个包跑在 arm64-v8a 上 · 进程里有转译层 · 连上 · 新建会话答得回来。
#   2 Doze / App Standby / am kill / force-stop：退后台 → 进那个状态 → 桌面那侧来几句 → 回前台；量「回前台 → 重连」「回前台 → 收齐」。
#   3 网：模拟器控制台的限速 · 延迟 · 断数据（先量它们管不管用）＋ 模拟器里的 netem（慢）· iptables（静默断）；量「网回来 → 看到新话」。
#     静默断网 15 秒 · 90 秒：网回来 CCM_DEVICE_REJOIN（默认 10）秒内要重连上，晚了就红（断 90 秒改前要 56 秒）。
#   4 通知：后台时那台来一条需手动 ⇒ 出通知 · 点它进那一条；一轮完成的通知点进去也得是那一条。
# CCM_DEVICE_BREAK=reconnect：回前台 / 网回来之前把台架 sshd（监听的与已连上的）停掉 ⇒ 2 · 3 必须红（看台架自己会红）。
import json
import os
import re
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import smoke  # noqa: E402  零件：tap · wait · drawer · add_host · open_import · tmux_name_of · rig · sh
import ui  # noqa: E402

t, tap, wait, adb, rig = smoke.t, smoke.tap, smoke.wait, smoke.adb, smoke.rig
BOX = smoke.BOX
OUT = smoke.OUT
PKG = smoke.PKG
ARM = os.environ.get("CCM_DEVICE_ARM") == "1"
BREAK = os.environ.get("CCM_DEVICE_BREAK", "")
ITEMS = os.environ.get("CCM_DEVICE_ITEMS", "1 2 3 4").split()
# 回前台 / 网回来之后多久还没收齐就判红（秒）。
WITHIN = float(os.environ.get("CCM_DEVICE_WITHIN", "60"))
# 静默断网之后网回来多久内要重连上（秒）：连接自己的保活判死、重拨的间隔都要短到网回来几秒内接上。
REJOIN = float(os.environ.get("CCM_DEVICE_REJOIN", "10"))
D = "d" + str(int(time.time()))[-5:]
rows = []  # (项, 结果 绿/红/没核, 说明)
nums = {}  # 量出来的数：名 → 秒（或说明）


def say(*a):
    print(*a, flush=True)


def shots(name):
    """这一屏亮暗各一张（截完回到亮色）。"""
    got = []
    for mode in ("no", "yes"):
        adb("shell", "cmd", "uimode", "night", mode)
        time.sleep(2)
        p = os.path.join(OUT, f"{name}-{'亮' if mode == 'no' else '暗'}.png")
        open(p, "wb").write(ui.adb("exec-out", "screencap", "-p").stdout)
        got.append(p)
    adb("shell", "cmd", "uimode", "night", "no")
    return got


class Red(Exception):
    pass


class Skip(Exception):
    pass


def item(name, fn):
    t0 = time.time()
    try:
        note = fn() or ""
        res = "绿"
    except Red as e:
        res, note = "红", str(e)
    except Skip as e:
        res, note = "没核", str(e)
    except smoke.Step.Fail as e:
        res, note = "红", str(e)
    try:
        p = shots(name)
    except Exception as e:  # 截不到图不改判
        p = [f"截图失败：{e}"]
    rows.append((name, res, note, f"{time.time() - t0:.0f}s", p))
    say(f"{res} {name}  {note}")


# ── 台架记录 ──

def commands_after(t0):
    """commands.log 里 t0（本机时刻）之后进来的命令：[(时刻, 命令)]。"""
    out = []
    day = time.strftime("%Y-%m-%d ")
    try:
        lines = open(os.path.join(BOX, "logs", "commands.log"), encoding="utf-8").read().splitlines()
    except OSError:
        return out
    for line in lines[-400:]:
        stamp, _, cmd = line.partition(" ")
        try:
            hms, frac = stamp.split(".")
            at = time.mktime(time.strptime(day + hms, "%Y-%m-%d %H:%M:%S")) + float("0." + frac)
        except ValueError:
            continue
        if at >= t0:
            out.append((at, cmd))
    return out


def sshd_pid():
    try:
        return int(open(os.path.join(BOX, "sshd", "sshd.pid")).read().strip())
    except (OSError, ValueError):
        return None


def children_of(pid):
    """pid 的子孙（读 /proc 的 ppid，不按名字找）。"""
    kids = {}
    for d in os.listdir("/proc"):
        if d.isdigit():
            try:
                ppid = int(open(f"/proc/{d}/stat").read().rsplit(")", 1)[1].split()[1])
            except (OSError, IndexError, ValueError):
                continue
            kids.setdefault(ppid, []).append(int(d))
    out, todo = [], [pid]
    while todo:
        p = todo.pop()
        for c in kids.get(p, []):
            out.append(c)
            todo.append(c)
    return out


def break_reconnect():
    """CCM_DEVICE_BREAK=reconnect：停掉台架 sshd 的监听进程与已连上的那几条（只按 pidfile 与它的子孙停）。"""
    pid = sshd_pid()
    if pid is None:
        return
    for p in children_of(pid) + [pid]:
        try:
            os.kill(p, 15)
        except OSError:
            pass
    say(f"  [故意弄坏] 停了台架 sshd（pid {pid} 与已连上的）")


def mend_reconnect():
    if BREAK == "reconnect":
        rig("up")


def send(name, text):
    rig("tmux", "send-keys", "-t", name, text, "Enter")


# ── 屏上 ──

# 翻屏只在聊天区中段划（输入框 · 键盘在下半屏：从那儿起划，键盘的滑行输入会把划痕当字敲进输入框）。
def fling_down():
    adb("shell", "input", "swipe", "540", "1500", "540", "500", "100")


def find_up(text, tries=16):
    """往上慢慢翻着找 [text]（看早一点那几句还在不在）：慢划不带惯性，一次只挪半屏，不会一下越过去。"""
    smoke.hide_keyboard()
    for _ in range(tries):
        if ui.find(text) is not None:
            return True
        adb("shell", "input", "swipe", "540", "700", "540", "1200", "600")
        time.sleep(0.5)
    return False


def till_visible(text, t0, secs):
    """[text] 上屏的那一刻距 t0 多少秒；secs 内没上屏 ⇒ None。每圈往底翻一下。"""
    end = t0 + secs
    smoke.hide_keyboard()
    while time.time() < end:
        if ui.find(text) is not None:
            return time.time() - t0
        fling_down()
        time.sleep(0.3)
    return None


def to_session(title):
    """经抽屉进 [title] 那一条会话。"""
    smoke.drawer(t("mobDrawer.item.chats"))
    if not wait(title, 30):
        raise Red(f"对话列表里没有「{title}」")
    tap(title)
    time.sleep(2)


def foreground():
    adb("shell", "input", "keyevent", "224")  # 亮屏
    adb("shell", "wm", "dismiss-keyguard")
    adb("shell", "monkey", "-p", PKG, "-c", "android.intent.category.LAUNCHER", "1")


def home():
    adb("shell", "input", "keyevent", "3")
    time.sleep(2)


# ── 准备：钥匙 ＋ 机器 ＋ 一条会话 ──

state = {}


def prepare():
    key = open(os.path.join(BOX, "sshd", "client_ed25519")).read().splitlines()
    smoke.launch(fresh=True)
    tap(t("mobIdentity.page.title")); time.sleep(1)
    smoke.open_import()
    tap(t("mobIdentity.import.name")); smoke.typed("smoke")
    tap(t("mobIdentity.import.pem")); time.sleep(0.5)
    for line in key:
        smoke.typed(line)
        adb("shell", "input", "keyevent", "66")
    tap(t("mobIdentity.import.ok"))
    if not wait("SHA256:", 10):
        raise Red("钥匙没导进来")
    tap(t("mobKit.act.back")); time.sleep(1)
    smoke.add_host("devbox")
    smoke.drawer(t("mobDrawer.item.chats"))
    if not (wait(t("history.section.today"), 40) or wait(t("history.list.empty"), 5)):
        raise Red("对话列表没出来（没连上）")
    smoke.drawer(t("mobDrawer.item.newChat"))
    tap(t("mobChat.input.placeholder")); smoke.typed(f"{D} start")
    smoke.hide_keyboard()
    tap(t("mobChat.input.send"))
    if not wait(f"替身收到：{D} start", 60):
        raise Red("新建的会话答不回来")
    state["name"] = smoke.tmux_name_of(f"{D} start")
    state["title"] = f"{D} start"
    if not state["name"]:
        raise Red("台架 tmux 里找不到这条会话")


def mapped_exec(maps, entry):
    """包里那个 .so（不解压、直接从 base.apk 映射）有没有映进进程：照它在 APK 里的偏移对 maps 的文件偏移。
    经转译跑的 arm64 代码由转译层读、不在宿主上直接执行 ⇒ 映射是 r--p / rw-p，不看 x 位。"""
    import zipfile
    apk = os.environ.get("CCM_DEVICE_APK", "")
    try:
        with zipfile.ZipFile(apk) as z:
            info = z.getinfo(entry)
            with open(apk, "rb") as f:
                f.seek(info.header_offset + 26)
                n, m = int.from_bytes(f.read(2), "little"), int.from_bytes(f.read(2), "little")
            start = info.header_offset + 30 + n + m
            end = start + info.file_size
    except (OSError, KeyError, zipfile.BadZipFile):
        return entry.rsplit("/", 1)[1] in maps  # 解压装的：maps 里直接是文件名
    for line in maps.splitlines():
        f = line.split()
        if len(f) >= 6 and f[5].endswith("/base.apk") and f[1].endswith("p"):
            lo, hi = (int(x, 16) for x in f[0].split("-"))
            off = int(f[2], 16)
            if off < end and off + (hi - lo) > start:
                return True
    return False


def item1():
    if not ARM:
        raise Skip("镜像没有 ARM 转译（ABI 表里没有 arm64-v8a）：装的是 x86_64 混淆包，这一项不算")
    abi = re.search(r"primaryCpuAbi=(\S+)", adb("shell", "dumpsys", "package", PKG))
    if not abi or abi.group(1) != "arm64-v8a":
        raise Red(f"系统报这个包的 ABI 是 {abi.group(1) if abi else '（读不出）'}，不是 arm64-v8a")
    t0 = time.time()
    prepare()
    nums["正式包：开 App → 新会话答回（含导钥匙 · 加机器）"] = round(time.time() - t0, 1)
    pid = adb("shell", "pidof", PKG).strip()
    maps = adb("shell", "cat", f"/proc/{pid}/maps") if pid else ""
    if not maps.strip():
        raise Red("读不到这个进程的 maps（adbd 不是 root？）")
    if "libndk_translation" not in maps:
        raise Red("进程里看不到转译层 libndk_translation（不是经 ARM 转译跑的？）")
    if not mapped_exec(maps, "lib/arm64-v8a/libmobcore.so"):
        raise Red("进程里没映射 arm64 的 libmobcore.so（Rust 核心没起来）")
    return f"包 ABI arm64-v8a · 进程里有 libndk_translation、arm64 的 libmobcore.so 映进来了 · 新会话答回 {nums['正式包：开 App → 新会话答回（含导钥匙 · 加机器）']}s"


# ── 2 后台的几种状态 ──

def background_case(label, enter, leave, lines=3, gap=4.0, hold=20.0):
    """退后台 → enter() → 桌面那侧隔 gap 秒来一句、共 lines 句 → 再等 hold 秒 → leave() → 回前台。
    回 (重连秒或 None, 收齐秒或 None, 说明)。"""
    name = state["name"]
    home()
    before_pid = adb("shell", "pidof", PKG).strip()
    note = enter() or ""
    said = [f"{D} {label} {i}" for i in range(1, lines + 1)]
    for s in said:
        send(name, s)
        time.sleep(gap)
    time.sleep(hold)
    leave()
    after_pid = adb("shell", "pidof", PKG).strip()
    if BREAK == "reconnect":
        break_reconnect()
    t0 = time.time()
    foreground()
    last = f"替身收到：{said[-1]}"
    got = till_visible(last, t0, 12)
    nav = ""
    if got is None:
        # 12 秒没上屏：先记屏上是不是还是那一条（有这一趟的句子），再经抽屉进去看（这段时间也算进收齐）。
        same = any(D in (n.get("text") or "") for n in ui.nodes())
        open(os.path.join(OUT, f"2-{label}-回前台12秒.png"), "wb").write(ui.adb("exec-out", "screencap", "-p").stdout)
        smoke.to_chat()
        to_session(state["title"])
        got = till_visible(last, t0, WITHIN)
        nav = ("（回前台 12 秒屏上还是那一条、新话没来，重进那一条才有）" if same
               else "（回前台 12 秒落在别处，经抽屉进的那一条）")
    cmds = [c for c in commands_after(t0) if "--resident" in c[1] or "ccm" in c[1]]
    re_at = round(cmds[0][0] - t0, 1) if cmds else None
    missing = [s for s in said[:-1] if not find_up(f"替身收到：{s}")]
    killed = "进程被杀" if before_pid and before_pid != after_pid else "进程还在"
    detail = f"{note}{killed}{nav}"
    if got is None:
        return re_at, None, f"回前台 {WITHIN:.0f}s 内没看到「{said[-1]}」的回答 · {detail}"
    if missing:
        # 留证：那一刻屏上有哪几句（往底翻回去再记），与一张图。
        for _ in range(4):
            fling_down()
        seen = [n.get("text") for n in ui.nodes() if D in (n.get("text") or "")]
        open(os.path.join(OUT, f"2-{label}-缺.png"), "wb").write(ui.adb("exec-out", "screencap", "-p").stdout)
        return re_at, round(got, 1), f"收不齐：缺 {missing}（屏底那几句 {seen[-6:]}）· {detail}"
    return re_at, round(got, 1), detail


def item2():
    reds = []
    cases = []

    def doze_in():
        adb("shell", "dumpsys", "battery", "unplug")
        adb("shell", "input", "keyevent", "223")
        out = adb("shell", "dumpsys", "deviceidle", "force-idle")
        if "deep idle" not in out:
            raise Red(f"进不了 Doze：{out.strip()[:120]}")
        return ""

    def doze_out():
        adb("shell", "dumpsys", "deviceidle", "unforce")
        adb("shell", "dumpsys", "battery", "reset")

    def rare_in():
        adb("shell", "am", "set-standby-bucket", PKG, "rare")
        b = adb("shell", "am", "get-standby-bucket", PKG).strip()
        if b != "40":
            raise Red(f"standby bucket 设不成 rare（读回 {b}）")
        return ""

    def kill_in():
        adb("shell", "am", "kill", PKG)
        time.sleep(1)
        return ""

    def stop_in():
        adb("shell", "am", "force-stop", PKG)
        time.sleep(1)
        if adb("shell", "pidof", PKG).strip():
            raise Red("force-stop 之后进程还在")
        return ""

    nothing = lambda: None  # noqa: E731
    for label, enter, leave in (("doze", doze_in, doze_out), ("rare", rare_in, nothing),
                                ("amkill", kill_in, nothing), ("forcestop", stop_in, nothing)):
        try:
            re_at, got, detail = background_case(label, enter, leave)
        except (Red, smoke.Step.Fail) as e:
            re_at, got, detail = None, None, str(e)
        finally:
            mend_reconnect()
        nums[f"{label}：回前台 → 重连"] = re_at if re_at is not None else "没有新连进来的命令（连着没断或没连上）"
        nums[f"{label}：回前台 → 收齐"] = got if got is not None else "没收齐"
        cases.append(f"{label} 重连 {re_at}s · 收齐 {got}s（{detail}）")
        say(f"  {cases[-1]}")
        if got is None or detail.startswith("收不齐"):
            reds.append(label)
        smoke.to_chat()
    if reds:
        raise Red(f"{' · '.join(reds)} 没收齐；" + "；".join(cases))
    return "；".join(cases)


# ── 3 网 ──

def nc_ms():
    """模拟器里连一次台架 sshd、读到它的招呼行要多少毫秒（只连 TCP，不登录）。"""
    out = adb("shell", f'a=$(date +%s%N); n=$(echo | timeout 20 nc -w 5 {smoke.host_alias()} {smoke.PORT} 2>/dev/null | head -c 8 | wc -c); '
                       f'b=$(date +%s%N); echo $n $(( (b-a)/1000000 ))')
    try:
        n, ms = out.strip().splitlines()[-1].split()
        return int(ms) if int(n) > 0 else None  # 没读到招呼行 ⇒ 没连通
    except (ValueError, IndexError):
        return None


def ifaces():
    up = {line.split()[0] for line in adb("shell", "ip", "-br", "link").splitlines() if line.split()}
    return [i for i in ("eth0", "wlan0") if i in up]


def netem(rule):
    """两张网卡上都打（或撤）一条 netem（模拟器里，不碰宿主网络）。rule=None ⇒ 撤。打不上 ⇒ 红（不当成网况就是这样）。"""
    names = ifaces()
    if rule and not names:
        raise Red("模拟器里找不到 eth0 / wlan0，netem 没处打")
    for i in names:
        adb("shell", "tc", "qdisc", "del", "dev", i, "root")
        if rule:
            adb("shell", "tc", "qdisc", "add", "dev", i, "root", "netem", *rule.split())
            if "netem" not in adb("shell", "tc", "qdisc", "show", "dev", i):
                raise Red(f"{i} 上 netem 没打上（{rule}）")


def cut_host(on):
    """静默断网：模拟器里两个方向都丢掉与宿主（台架 sshd 所在）之间的包，网卡还在、连接不收 RST——最像进了没信号的地方。
    netem 只管出方向（进来的照样收得到），断网用 iptables。"""
    host = smoke.host_alias()
    for chain, flag in (("INPUT", "-s"), ("OUTPUT", "-d")):
        for _ in range(5):  # 撤干净（叠过几条就撤几条）；撤不动了 iptables 说 Bad rule
            if "Bad rule" in adb("shell", f"iptables -D {chain} {flag} {host} -j DROP 2>&1"):
                break
        if on:
            adb("shell", "iptables", "-I", chain, flag, host, "-j", "DROP")


def chat_round(text, secs):
    """前台、在那条会话里：桌面那侧来一句，量到它的回答上屏要多久。"""
    t0 = time.time()
    send(state["name"], text)
    return till_visible(f"替身收到：{text}", t0, secs)


def item3():
    smoke.to_chat()
    to_session(state["title"])
    notes, reds = [], []
    # ① 控制台那几条管不管用：改前 / 改后各量一次连上要多久（照 weak-net 台架「成对读数」的规矩）。
    base = nc_ms()
    adb("emu", "network", "delay", "1000")
    adb("emu", "network", "speed", "gsm")
    shaped = nc_ms()
    adb("emu", "network", "delay", "none")
    adb("emu", "network", "speed", "full")
    nums["控制台 network delay 1000：改前 / 改后连上（ms）"] = f"{base} / {shaped}"
    console_ok = base is not None and shaped is not None and shaped - base >= 800
    notes.append(f"控制台 delay 1000 改前 {base}ms 改后 {shaped}ms（{'管用' if console_ok else '不管用'}）")
    adb("emu", "gsm", "data", "off")
    time.sleep(6)
    cut = nc_ms()
    adb("emu", "gsm", "data", "on")
    nums["控制台 gsm data off 之后连上（ms）"] = cut
    notes.append(f"控制台 gsm data off 之后照样连得上（{cut}ms）" if cut is not None and cut < 5000 else "控制台 gsm data off 断得了")
    # ② 模拟器里 netem：基线 · 慢网 · 静默断网（网卡还在、包全丢，最像进了没信号的地方）。
    netem(None)
    b = chat_round(f"{D} net base", WITHIN)
    nums["基线：来一句 → 回答上屏"] = round(b, 1) if b is not None else "没上屏"
    netem("delay 400ms 100ms rate 64kbit")
    slow_ms = nc_ms()
    if base is None or slow_ms is None or slow_ms - base < 300:
        netem(None)
        raise Red(f"netem 没造出慢网（改前 {base}ms · 改后 {slow_ms}ms）")
    s = chat_round(f"{D} net slow", WITHIN)
    netem(None)
    nums["慢网（netem 400±100ms · 64kbit）：来一句 → 回答上屏"] = round(s, 1) if s is not None else "没上屏"
    nums["慢网：连上（ms）"] = slow_ms
    if s is None:
        reds.append("慢网下回答没上屏")
    for secs in (15, 90):
        label = f"cut{secs}"
        cut_host(True)
        time.sleep(2)
        if nc_ms() is not None:
            cut_host(False)
            raise Red("断了之后模拟器照样连得通台架（没断成）")
        send(state["name"], f"{D} {label}")
        time.sleep(secs)
        if ui.find(f"替身收到：{D} {label}") is not None:
            cut_host(False)
            raise Red(f"断着网，「{D} {label}」的回答却上屏了（没断成）")
        if BREAK == "reconnect":
            break_reconnect()
        cut_host(False)
        t0 = time.time()
        got = till_visible(f"替身收到：{D} {label}", t0, WITHIN)
        cmds = [c for c in commands_after(t0) if "ccm" in c[1]]
        re_at = round(cmds[0][0] - t0, 1) if cmds else None
        mend_reconnect()
        nums[f"静默断网 {secs}s：网回来 → 重连"] = re_at if re_at is not None else "没有新连进来的命令"
        nums[f"静默断网 {secs}s：网回来 → 看到断开时来的那句"] = round(got, 1) if got is not None else "没看到"
        notes.append(f"静默断网 {secs}s：重连 {re_at}s · 看到 {None if got is None else round(got, 1)}s")
        if got is None:
            reds.append(f"静默断网 {secs}s 之后 {WITHIN:.0f}s 内没补上")
        elif re_at is None or re_at > REJOIN:
            reds.append(f"静默断网 {secs}s：网回来 {re_at}s 才重连（要 ≤ {REJOIN:.0f}s）")
        say(f"  {notes[-1]}")
    if not adb("shell", "pidof", PKG).strip():
        reds.append("App 没了")
    if reds:
        raise Red("；".join(reds + notes))
    return "；".join(notes)


# ── 4 通知 ──

def notifications():
    """这个包现在挂着的通知：[(标题, 正文)]（照 dumpsys notification 的 extras）。"""
    out = adb("shell", "dumpsys", "notification", "--noredact")
    got = []
    for block in out.split("NotificationRecord(")[1:]:
        if f"pkg={PKG}" not in block[:400]:
            continue
        title = re.search(r"android\.title=String \((.*?)\)\n", block)
        text = re.search(r"android\.text=String \((.*?)\)\n", block)
        got.append((title.group(1) if title else "", text.group(1) if text else ""))
    return got


def tap_notification(word):
    """拉下通知栏、点含 [word] 的那条；点不到 ⇒ False。"""
    adb("shell", "cmd", "statusbar", "expand-notifications")
    time.sleep(2)
    ok = tap(word, must=False)
    if not ok:
        adb("shell", "cmd", "statusbar", "collapse")
    time.sleep(3)
    return ok


def on_session(marker, other):
    """屏上是那一条：看得到 [marker]、看不到 [other]（另一条会话独有的那句）。"""
    for _ in range(6):
        if ui.find(marker) is not None:
            return ui.find(other) is None
        fling_down()
        time.sleep(0.5)
    return False


def item4():
    reds = []
    other = f"needs{D}"
    # 另起一条（桌面那侧起），让「进对的那一条」有得分：手机此刻开着的是 state["title"] 那一条。
    smoke.sh(f"tmux new -d -s {other} -c ~/work claude; sleep 3; tmux send-keys -t {other} '{D} other' Enter")
    time.sleep(8)
    smoke.to_chat()
    to_session(state["title"])
    home()
    before = notifications()
    send(other, f"ask {D} needs")
    t0 = time.time()
    seen = None
    while time.time() - t0 < 30:
        new = [n for n in notifications() if n not in before]
        hit = [n for n in new if t("needs.bar.unknown") in " ".join(n) or "等批准" in " ".join(n)]
        if hit:
            seen = hit[0]
            break
        time.sleep(1)
    nums["后台需手动：来了 → 出通知"] = round(time.time() - t0, 1) if seen else "没出"
    if not seen:
        reds.append(f"后台时那台来了需手动，30s 内没出通知（挂着的：{notifications()}）")
    else:
        if not tap_notification(seen[0]):
            reds.append(f"通知栏里点不到「{seen[0]}」")
        elif not on_session(f"{D} needs", f"{D} start"):
            reds.append("点需手动的通知没进那一条会话")
    # 放行，让那一轮走完：一轮完成的通知（这个有）点进去也得是那一条。
    home()
    rig("tmux", "send-keys", "-t", other, "y", "Enter")
    t1 = time.time()
    done = None
    finish = t("turnNotify.finish.title", tab="").rstrip(" ·")
    while time.time() - t1 < 30:
        hit = [n for n in notifications() if finish in n[0] and n not in before]
        if hit:
            done = hit[0]
            break
        time.sleep(1)
    nums["后台一轮完成：答完 → 出通知"] = round(time.time() - t1, 1) if done else "没出"
    if not done:
        reds.append("后台时那一轮完成了，30s 内没出完成通知")
    else:
        if not tap_notification(finish):
            reds.append("通知栏里点不到完成通知")
        elif not on_session(f"替身收到：ask {D} needs", f"替身收到：{D} start"):
            reds.append(f"点完成通知（{done[0]}）没进那一条会话，落在了别处")
    if reds:
        raise Red("；".join(reds))
    return f"需手动通知 {nums['后台需手动：来了 → 出通知']}s · 完成通知 {nums['后台一轮完成：答完 → 出通知']}s，点进去都是那一条"


def as_root():
    """读别的进程的 maps（第 1 项）· 打 tc / iptables（第 3 项）都要 adbd 是 root（google_apis 镜像给；Play 商店镜像不给）。"""
    if adb("shell", "id", "-u").strip() == "0":
        return True
    adb("root")
    time.sleep(2)
    adb("wait-for-device")
    for _ in range(20):
        if adb("shell", "getprop", "sys.boot_completed").strip() == "1":
            break
        time.sleep(1)
    return adb("shell", "id", "-u").strip() == "0"


def main():
    if not as_root():
        say("adbd 成不了 root：第 1 项读不到进程映射、第 3 项打不了 netem / iptables，这两项会红")
    adb("shell", "cmd", "uimode", "night", "no")
    adb("shell", "pm", "clear", PKG)
    adb("shell", "pm", "grant", PKG, "android.permission.POST_NOTIFICATIONS")
    adb("shell", "svc", "wifi", "enable")
    netem(None)
    cut_host(False)
    if "1" in ITEMS:
        item("1-正式包经ARM转译", item1)
    for attempt in (1, 2):  # 准备那一步是冒烟走熟了的主路，偶尔点空一下：清数据重来一回，再不成才算
        if "name" in state:
            break
        try:
            prepare()
        except (Red, smoke.Step.Fail) as e:
            say(f"准备没成（第 {attempt} 回）：{e}")
            adb("shell", "pm", "clear", PKG)
            adb("shell", "pm", "grant", PKG, "android.permission.POST_NOTIFICATIONS")
    if "name" in state:
        for k, name, fn in (("2", "2-Doze待机杀后台", item2), ("3", "3-断网限速", item3), ("4", "4-通知", item4)):
            if k in ITEMS:
                item(name, fn)
    else:
        for k, name in (("2", "2-Doze待机杀后台"), ("3", "3-断网限速"), ("4", "4-通知")):
            if k in ITEMS:
                rows.append((name, "红", "准备那一步没成（没连通 / 新会话答不回）", "0s", []))
    netem(None)
    print()
    print("| 项 | 结果 | 说明 | 用时 |")
    print("|---|---|---|---|")
    for name, res, note, took, _ in rows:
        print(f"| {name} | {res} | {note} | {took} |")
    print()
    for k, v in nums.items():
        print(f"- {k}：{v}")
    json.dump({"break": BREAK, "rows": rows, "nums": nums}, open(os.path.join(OUT, "device-results.json"), "w"),
              ensure_ascii=False, indent=1)
    sys.exit(0 if all(r[1] == "绿" for r in rows) else 1)


if __name__ == "__main__":
    main()
