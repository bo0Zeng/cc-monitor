#!/usr/bin/env python3
# 手机冒烟主路（模拟器 ＋ 沙箱后端，替身 agent）：从清空的 App 起，一步一步走，每步截屏、记时，末尾一张表。
# 前提：rig.sh up（沙箱 sshd ＋ 后端）· emu.sh up（模拟器）· debug 包已装；环境照 emu.sh env。入口是同目录 smoke.sh。
# 每一步只看手机屏上与台架记录里看得到的东西（不读 App 内部）；判不出的写「看不出」，不当过。
import json
import os
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
import ui  # noqa: E402

# 屏上的字都照文案表取（App 与核心读的同一张表）：文案一改，冒烟跟着走，不靠抄一份字面。
_COPY = json.load(open(os.path.join(here, "..", "..", "src", "shared", "copy", "table.json"), encoding="utf-8"))["entries"]


def t(key, **args):
    s = _COPY[key]["zh"]
    for k, v in args.items():
        s = s.replace("{" + k + "}", str(v))
    return s


BOX = os.environ["CCM_SMOKE_BOX"]
OUT = os.environ.get("CCM_SMOKE_OUT", os.path.join(BOX, "..", "shots"))
PORT = os.environ.get("CCM_SMOKE_PORT", "22422")
USER = subprocess.run(["id", "-un"], capture_output=True, text=True).stdout.strip()
PKG = "com.ccmonitor.mobile"
os.makedirs(OUT, exist_ok=True)
results = []
# 这一趟的记号：台架目录跨趟留着，旧会话里也有同样的句子 ⇒ 每句带上它，认会话只认这一趟的。
T = "s" + str(int(time.time()))[-5:]


def rig(*a, **k):
    return subprocess.run(["bash", os.path.join(here, "rig.sh"), *a], capture_output=True, text=True, **k)


def sh(cmd):
    return rig("ssh", cmd).stdout


def adb(*a):
    return ui.adb(*a).stdout.decode("utf-8", "replace")


def tap(text, nth=0, must=True):
    n = ui.find(text, nth)
    if n is None:
        if must:
            raise Step.Fail(f"屏上找不到「{text}」")
        return False
    adb("shell", "input", "tap", *map(str, ui.center(n)))
    return True


def wait(text, secs=30):
    end = time.time() + secs
    while time.time() < end:
        if ui.find(text) is not None:
            return True
        time.sleep(1)
    return False


def typed(s):
    adb("shell", "input", "text", s.replace(" ", "%s"))


def hide_keyboard():
    if "mInputShown=true" in adb("shell", "dumpsys", "input_method"):
        adb("shell", "input", "keyevent", "4")
        time.sleep(0.5)


def shot(name):
    p = os.path.join(OUT, f"{name}.png")
    open(p, "wb").write(ui.adb("exec-out", "screencap", "-p").stdout)
    return p


def launch(fresh=False):
    adb("shell", "monkey", "-p", PKG, "-c", "android.intent.category.LAUNCHER", "1")
    if fresh:
        if not wait(t("mobDrawer.server.bare"), 20):
            raise Step.Fail("App 没起来")
        return
    time.sleep(3)
    to_chat()


def to_chat():
    """回到带抽屉按钮的那一屏（对话列表 · 终端屏没有顶栏，按返回回去）。"""
    for _ in range(4):
        hide_keyboard()
        if wait(t("mobDrawer.open.aria"), 3):
            return
        adb("shell", "input", "keyevent", "4")
        time.sleep(1)
    raise Step.Fail("回不到聊天屏（找不到抽屉按钮）")


def to_bottom():
    for _ in range(12):
        adb("shell", "input", "swipe", "540", "2000", "540", "500", "120")


def drawer(item):
    to_chat()
    tap(t("mobDrawer.open.aria"))
    time.sleep(1.5)
    tap(item)
    time.sleep(2)


class Step:
    class Fail(Exception):
        pass

    def __init__(self, name):
        self.name = name

    def __enter__(self):
        self.t0 = time.time()
        return self

    def __exit__(self, kind, err, tb):
        took = time.time() - self.t0
        p = shot(self.name)
        ok = kind is None
        results.append((self.name, "过" if ok else f"没过：{err}", f"{took:.1f}s", p))
        print(f"{'ok ' if ok else 'RED'} {self.name}  {took:.1f}s  {'' if ok else err}  {p}", flush=True)
        return kind is Step.Fail


def tmux_name_of(text):
    """记录里有这一句的那条会话在台架 tmux 里叫什么。"""
    for line in rig("tmux", "list-panes", "-a", "-F", "#{session_name} #{@ccm_sid}").stdout.splitlines():
        name, _, sid = line.partition(" ")
        if sid and text in sh(f"cat ~/.claude/projects/*/{sid}.jsonl 2>/dev/null; true"):
            return name
    return None


def host_alias():
    """模拟器里够到宿主机的那个地址 ＝ 模拟器的网关（问模拟器自己，不写死）。"""
    route = adb("shell", "ip", "route", "get", "8.8.8.8").split()
    if "via" not in route:
        raise Step.Fail(f"问不出模拟器的网关：{' '.join(route)}")
    return route[route.index("via") + 1]


def fab_under_notice(fab_text, notice_text):
    """同一帧里：悬浮按钮（含 [fab_text] 的那个可点的块）与提示条（含 [notice_text] 的那一条）的矩形有没有交叠。"""
    ns = ui.nodes()

    def first(text):
        return next((n for n in ns if text in (n.get("text"), n.get("content-desc"))), None)

    def area(b):
        return (b[2] - b[0]) * (b[3] - b[1])

    def inside(outer, inner):
        return outer[0] <= inner[0] and outer[1] <= inner[1] and outer[2] >= inner[2] and outer[3] >= inner[3]

    f, c = first(fab_text), first(notice_text)
    if c is None:
        raise Step.Fail(f"这一帧里没有提示「{notice_text}」")
    if f is None:
        return True  # 按钮整个被盖住时，无障碍树里连它的字都不出

    fb, cb = ui.bounds(f), ui.bounds(c)
    fab = min((ui.bounds(n) for n in ns if n.get("clickable") == "true" and inside(ui.bounds(n), fb)), key=area, default=fb)
    # 提示条：装得下那颗按钮字、又比它宽出一大截的最小那一块（按钮本身不算）。
    bars = [ui.bounds(n) for n in ns if inside(ui.bounds(n), cb) and ui.bounds(n)[2] - ui.bounds(n)[0] > 3 * (cb[2] - cb[0])]
    bar = min(bars, key=area, default=cb)
    return not (fab[2] <= bar[0] or bar[2] <= fab[0] or fab[3] <= bar[1] or bar[3] <= fab[1])


def tap_in_row(label, button):
    """点 [label] 那一行上的 [button]（同一行：纵向范围重叠）。"""
    ns = ui.nodes()
    row = next((n for n in ns if n.get("text") == label), None)
    if row is None:
        raise Step.Fail(f"屏上找不到「{label}」那一行")
    y = ui.center(row)[1]
    for n in ns:
        if n.get("text") == button:
            x1, y1, x2, y2 = ui.bounds(n)
            if y1 - 40 <= y <= y2 + 40:
                adb("shell", "input", "tap", str((x1 + x2) // 2), str((y1 + y2) // 2))
                return
    raise Step.Fail(f"「{label}」那一行上没有「{button}」")


def add_host(name, codex=False):
    if ui.find(t("mobDrawer.open.aria")) is not None:
        drawer(t("mobDrawer.server.named", name=""))
    tap("+")
    time.sleep(1.5)
    tap(t("mobHostEdit.field.name")); typed(name)
    tap(t("mobHostEdit.field.host")); typed(host_alias())
    tap("22")
    for k in ("123",) + ("67",) * 4:
        adb("shell", "input", "keyevent", k)
    typed(PORT)
    tap(t("mobHostEdit.field.user")); typed(USER)
    hide_keyboard()
    if codex:
        tap("Codex")
    adb("shell", "input", "swipe", "540", "1600", "540", "400", "400")
    time.sleep(1)
    tap("smoke")
    tap(t("mobHostEdit.page.save"))
    time.sleep(2)


def main():
    adb("shell", "pm", "clear", PKG)
    adb("shell", "pm", "grant", PKG, "android.permission.POST_NOTIFICATIONS")
    key = open(os.path.join(BOX, "sshd", "client_ed25519")).read().splitlines()

    with Step("1-加机器连上"):
        launch(fresh=True)
        tap(t("mobIdentity.page.title")); time.sleep(1)
        tap("+"); time.sleep(1)
        tap(t("mobIdentity.add.import")); time.sleep(1)
        tap(t("mobIdentity.import.name")); typed("smoke")
        tap(t("mobIdentity.import.pem")); time.sleep(0.5)
        typed(key[0])  # 只贴一行：要当场拒
        tap(t("mobIdentity.import.ok")); time.sleep(2)
        if ui.find("imported") is not None:
            raise Step.Fail("只贴进一行的钥匙照样存下了")
        # 拒的那一句是条长提示（带「复制详情」）：它在的时候「+」要让开、不被盖住，接着点得到。
        if not wait(t("detail.act.copy"), 5):
            raise Step.Fail("拒了却没出那条提示")
        if fab_under_notice("+", t("detail.act.copy")):
            raise Step.Fail("提示盖住了「+」（按钮没让开）")
        tap("+"); time.sleep(1)
        tap(t("mobIdentity.add.import")); time.sleep(1)
        tap(t("mobIdentity.import.name")); typed("smoke")
        tap(t("mobIdentity.import.pem")); time.sleep(0.5)
        for line in key:
            typed(line)
            adb("shell", "input", "keyevent", "66")
        tap(t("mobIdentity.import.ok"))
        if not wait("SHA256:", 10):
            raise Step.Fail("整把钥匙没导进来")
        tap(t("mobKit.act.back")); time.sleep(1)
        add_host("smokebox")
        drawer(t("mobDrawer.item.chats"))
        if not (wait(t("history.section.today"), 30) or wait(t("history.list.empty"), 5)):
            raise Step.Fail("对话列表没出来（没连上）")

    with Step("2-对话列表"):
        sh(f'tmux new -d -s seed{T} -c ~/work claude; sleep 3; tmux send-keys -t seed{T} "{T} seed" Enter; '
           f'tmux new -d -s needs{T} -c ~/work claude; sleep 3; tmux send-keys -t needs{T} "ask {T} needs" Enter')
        if not wait(f"{T} seed", 30):
            raise Step.Fail("历史清单没出新会话")
        if not wait(t("needs.bar.unknown"), 20):
            raise Step.Fail("需手动没出来")
        # 台架跨趟留着旧会话（上一趟中途停下的那条也还在等），它们按分钟走、几秒里不变：
        # 看的是「有没有一行在走表」，这一趟刚起的那条按秒走，一定在里面。
        def waits():
            return [n.get("text") for n in ui.nodes() if t("needs.bar.subBare") in (n.get("text") or "")]

        first = waits()
        time.sleep(4)
        later = waits()
        if not first or first == later:
            raise Step.Fail(f"已等没走表（{first!r} → {later!r}）")
        rig("tmux", "send-keys", "-t", f"needs{T}", "y", "Enter")

    with Step("3-新建会话"):
        drawer(t("mobDrawer.item.newChat"))
        tap(t("mobChat.input.placeholder")); typed(f"{T} new session")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} new session", 40):
            raise Step.Fail("答不回来")

    with Step("4-聊天屏"):
        tap(t("mobChat.input.placeholder")); typed(f"{T} long")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(t("cards.thinking.title"), 20):
            raise Step.Fail("过程没出来")
        time.sleep(4)
        for _ in range(12):
            adb("shell", "input", "swipe", "540", "700", "540", "2000", "150")
        if not wait(f"{T} new session", 5):
            raise Step.Fail("往上翻翻不到第一句")
        tap(t("cards.thinking.title"))
        if not wait("替身在想。", 5):
            raise Step.Fail("思考展不开")

    with Step("5-一轮完成的通知"):
        name = tmux_name_of(f"{T} new session")
        adb("shell", "input", "keyevent", "3")
        time.sleep(2)
        rig("tmux", "send-keys", "-t", name or "", f"{T} notify", "Enter")
        end = time.time() + 20
        while time.time() < end and t("turnNotify.finish.title", tab="").rstrip(" ·") not in adb("shell", "dumpsys", "notification", "--noredact"):
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("没出完成通知")
        launch()

    with Step("6-断网再连"):
        name = tmux_name_of(f"{T} new session")
        adb("shell", "cmd", "connectivity", "airplane-mode", "enable")
        flashed = []
        for _ in range(3):
            time.sleep(2)
            flashed += [n.get("text") for n in ui.nodes() if "离线" in (n.get("text") or "")]
        rig("tmux", "send-keys", "-t", name or "", f"{T} offline", "Enter")
        time.sleep(6)
        adb("shell", "cmd", "connectivity", "airplane-mode", "disable")
        time.sleep(10)
        to_bottom()
        if not wait(f"替身收到：{T} offline", 60):
            raise Step.Fail("网回来之后没补上断开那段")
        to_bottom()
        missing = [t for t in (f"替身收到：{T} notify", f"替身收到：{T} offline") if ui.find(t) is None]
        if missing:
            raise Step.Fail(f"漏了：{missing}")
        if not adb("shell", "pidof", PKG).strip():
            raise Step.Fail("App 没了（断网闪退）")
        if flashed:
            raise Step.Fail(f"闪了「离线」：{flashed[:1]}")

    with Step("7-终端屏"):
        name = tmux_name_of(f"{T} new session")
        drawer(t("mobDrawer.item.terminal"))
        time.sleep(2)
        hide_keyboard()
        tap(t("mobDefaultButtons.label.tmux")); time.sleep(3)
        names = [n.get("text") for n in ui.nodes()]
        if name not in names:
            raise Step.Fail(f"tmux 会话列表里没有 {name}")
        tap_in_row(name, t("mobTmux.row.attach"))
        time.sleep(3)
        typed(f"{T} from phone terminal")
        adb("shell", "input", "keyevent", "66")
        time.sleep(1)
        rig("tmux", "send-keys", "-t", name, f"{T} from desk", "Enter")
        time.sleep(6)
        pane = rig("tmux", "capture-pane", "-p", "-t", name).stdout
        if f"> {T} from phone terminal" not in pane or f"> {T} from desk" not in pane:
            raise Step.Fail("两侧打的字没都进那个会话")
        adb("shell", "input", "keyevent", "4")

    with Step("8-Codex"):
        launch()
        add_host("smokecodex", codex=True)
        hide_keyboard()
        tap(t("mobChat.input.placeholder")); typed(f"{T} codex")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} codex", 40):
            raise Step.Fail("Codex 答不回来")

    print()
    print("| 步 | 结果 | 用时 | 截图 |")
    print("|---|---|---|---|")
    for name, res, took, p in results:
        print(f"| {name} | {res} | {took} | {p} |")
    json.dump(results, open(os.path.join(OUT, "results.json"), "w"), ensure_ascii=False, indent=1)
    sys.exit(0 if all(r[1] == "过" for r in results) else 1)


if __name__ == "__main__":
    main()
