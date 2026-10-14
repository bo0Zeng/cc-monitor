#!/usr/bin/env python3
# 手机冒烟主路（模拟器 ＋ 沙箱后端，替身 agent）：从清空的 App 起，一步一步走，每步截屏、记时，末尾一张表。
# 前提：rig.sh up（沙箱 sshd ＋ 后端）· emu.sh up（模拟器）· debug 包已装；环境照 emu.sh env。入口是同目录 smoke.sh。
# 第 4 步后两小步（4b 模型 chip · 4c ［+］面板）走聊天屏输入那一圈。
# 每一步只看手机屏上与台架记录里看得到的东西（不读 App 内部）；判不出的写「看不出」，不当过。
import json
import os
import re
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


def open_import():
    """钥匙页点「+」开「添一把钥匙」那一张、点「搬进来」。提示条收起时「+」在往回落，那一下可能点空：没开出来就再点，最多三回。"""
    for _ in range(3):
        tap("+")
        if wait(t("mobIdentity.add.import"), 4):
            break
    tap(t("mobIdentity.add.import"))
    time.sleep(1)


def add_host(name):
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
    adb("shell", "input", "swipe", "540", "1600", "540", "400", "400")
    time.sleep(1)
    tap("smoke")
    tap(t("mobHostEdit.page.save"))
    time.sleep(2)


def claude_models(cell="models"):
    """那台给 claude 的模型 · 权限档（session-new-facts 那一家的 `models` / `permissionModes`）。"""
    facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                      capture_output=True, text=True).stdout or "{}")
    agents = (facts.get("data") or {}).get("agents") or []
    got = next((a[cell] for a in agents if a.get("kind") == "claude"), [])
    if not got:
        raise Step.Fail(f"那台没给 claude 的 {cell}：{facts!r}"[:200])
    return got


def pane_has(name, text, secs):
    """那个 tmux 会话的屏上在 [secs] 秒内出现 [text]。"""
    end = time.time() + secs
    while True:
        if text in rig("tmux", "capture-pane", "-p", "-t", name or "").stdout:
            return True
        if time.time() >= end:
            return False
        time.sleep(1)


def switch_model(name):
    """终端屏的命令面板里换模型：列的是那台给的那一家的模型（session-new-facts），点一个 ⇒ session-model。
    替身读到 `/model <值>` 就落那一家的读回 ⇒ 结局「换成了」，那台写好的那一句上屏（Toast，经无障碍事件看）；
    回包解不开 · 没换成 ⇒ 提示条「发送失败」。"""
    facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                      capture_output=True, text=True).stdout or "{}")
    agents = (facts.get("data") or {}).get("agents") or []
    models = next((a["models"] for a in agents if a.get("kind") == "claude"), [])
    if not models:
        raise Step.Fail(f"那台没给 claude 的模型：{facts!r}"[:200])
    pick = models[-1]
    hide_keyboard()
    tap(t("mobDefaultButtons.label.command"))
    time.sleep(1.5)
    row = ui.find(pick["text"])
    if row is None:
        raise Step.Fail(f"命令面板里没有「{pick['text']}」")
    said = t("beSessionModel.set.saved", model=pick["value"])
    # 无障碍事件流和 `uiautomator dump` 抢同一个口：看事件的这几秒里只点 · 只读台架，不 dump 屏。
    events = subprocess.Popen(["adb", "shell", "uiautomator", "events"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    try:
        time.sleep(1)
        adb("shell", "input", "tap", *map(str, ui.center(row)))
        end = time.time() + 20
        while time.time() < end and f"> /model {pick['value']}" not in rig("tmux", "capture-pane", "-p", "-t", name).stdout:
            time.sleep(1)
        got = time.time() < end
        time.sleep(4)
    finally:
        events.terminate()
    seen = events.communicate(timeout=5)[0]
    if not got:
        raise Step.Fail(f"点了「{pick['text']}」，那个会话里没收到 /model {pick['value']}")
    if t("mobTerm.model.sendFailed") in seen:
        raise Step.Fail("换模型那一问上屏的是「发送失败」（回包没解开或没换成）")
    if said not in seen:
        raise Step.Fail(f"没看到那台写好的那一句「{said}」")


# 轮换屏那一步用的两份策略（合成的：照号牌顺序用；一格「权重容差」）。写进台架家里的策略目录（不是真家目录）。
SMOKE_STRATEGY = """// 照号牌顺序用（冒烟台架）。
/*--- rotation
{
  "format": 5,
  "tz": "UTC",
  "settings": { "accounts": [{ "id": "work" }, { "id": "team" }, { "id": "lab" }], "close": 25 },
  "form": [
    { "key": "close", "kind": "percent", "scope": "all", "min": 0, "max": 100, "default": 25,
      "label": { "zh": "权重容差" }, "help": { "zh": "冒烟台架的一格" } }
  ]
}
---*/
fn decide(ctx) {
  let order = [];
  let why = #{};
  for r in ctx.settings.accounts { order.push(r.id); why[r.id] = "照号牌顺序"; }
  #{ order: order, why: why, hold_until: (), wake: #{ at: (), when: [] }, memo: () }
}
"""


def frames(cmd, args, within=20000):
    out = subprocess.run([sys.executable, os.path.join(here, "frames.py"), cmd, json.dumps(args), str(within)],
                         capture_output=True, text=True).stdout
    return json.loads(out or "{}")


def seed_rotation():
    """台架里的账号库（work · team · lab）与两份策略；跨趟留着，再跑一遍不出错就行。"""
    frames("accounts-init", {"name": "work"})
    for n in ("team", "lab"):
        frames("accounts-add", {"name": n, "kind": "subscription"})
    import base64
    b = base64.b64encode(SMOKE_STRATEGY.encode()).decode()
    sh(f"mkdir -p ~/.cc-monitor/strategies && for n in smoke-a smoke-b; do echo {b} | base64 -d > ~/.cc-monitor/strategies/$n.rhai; done")


def lane_head(text):
    """时间轴里号名那一列的 [text]（同名的号牌在上面，取最靠下、靠左的那一个）。"""
    ns = [n for n in ui.nodes() if n.get("text") == text and ui.center(n)[0] < 300]
    return max(ns, key=lambda n: ui.center(n)[1], default=None)


def main():
    # 深浅：CCM_SMOKE_NIGHT=yes|no 时先把模拟器切到那一套（截图两套各走一趟用）；不设不动。
    if os.environ.get("CCM_SMOKE_NIGHT") in ("yes", "no"):
        adb("shell", "cmd", "uimode", "night", os.environ["CCM_SMOKE_NIGHT"])
    adb("shell", "pm", "clear", PKG)
    adb("shell", "pm", "grant", PKG, "android.permission.POST_NOTIFICATIONS")
    key = open(os.path.join(BOX, "sshd", "client_ed25519")).read().splitlines()

    with Step("1-加机器连上"):
        launch(fresh=True)
        tap(t("mobIdentity.page.title")); time.sleep(1)
        open_import()
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
        open_import()
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
        # 新建会话打开就是填好的（目录 ＝ 那台最近用过的第一个）；点目录 chip → 浏览… → 进一层 → 选这个目录，起在那一层。
        sh(f"mkdir -p ~/work/{T}dir")
        drawer(t("mobDrawer.item.newChat"))
        if not wait("~/work", 20):
            raise Step.Fail("目录 chip 没填上那台最近的目录")
        tap("~/work")
        if not wait(t("mobNewSession.dir.browse"), 10):
            raise Step.Fail("目录抽屉没出来")
        tap(t("mobNewSession.dir.browse"))
        if not wait(f"{T}dir", 20):
            raise Step.Fail("浏览屏里没列出那一层的文件夹")
        tap(f"{T}dir")
        time.sleep(2)
        tap(t("mobNewSession.dir.pick"))
        if not wait(f"~/work/{T}dir", 10):
            raise Step.Fail("选了目录，chip 没换成它")
        tap(t("mobChat.input.placeholder")); typed(f"{T} new session")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} new session", 40):
            raise Step.Fail("答不回来")
        if f"{T}dir" not in sh("ls ~/.claude/projects/"):
            raise Step.Fail("那条会话没起在选的目录里")

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

    with Step("4b-换模型 chip"):
        # 聊天屏输入框下那颗模型 chip：抽屉里列那台给的模型，点一个 ⇒ session-model；读回 done 才换字（替身落的读回是那个值）。
        name = tmux_name_of(f"{T} new session")
        pick = claude_models()[-1]
        hide_keyboard()
        tap(t("session.model.title"))
        if not wait(pick["text"], 10):
            raise Step.Fail(f"模型抽屉里没有「{pick['text']}」")
        shot("4b-模型抽屉")
        tap(pick["text"])
        if not pane_has(name, f"> /model {pick['value']}", 20):
            raise Step.Fail(f"点了「{pick['text']}」，那个会话里没收到 /model {pick['value']}")
        end = time.time() + 20
        while time.time() < end and ui.find(t("session.model.title")) is not None:
            time.sleep(1)
        if ui.find(t("session.model.unsure")) is not None:
            raise Step.Fail("chip 上方说换模型结果未知（没读回）")
        if ui.find(pick["value"]) is None:
            raise Step.Fail(f"chip 没换成读回的字「{pick['value']}」")
        # 权限档 chip：抽屉里列那台给的档（session-new-facts）。替身不画档位那一行、也不认切档键（按下去的键会混进它读的下一行），这里只开抽屉不切。
        tap(t("session.permission.title"))
        modes = claude_models("permissionModes")
        if not wait(modes[0]["text"], 10):
            raise Step.Fail(f"权限档抽屉里没有「{modes[0]['text']}」")
        shot("4b-权限档抽屉")
        adb("shell", "input", "keyevent", "4")
        time.sleep(1)

    with Step("4c-加号面板"):
        # ［+］：命令清单是那台给这条会话的（session-commands），点一条只填进输入框；压缩上下文不确认、经 session-say 送；清空会话先确认，取消就不送。
        name = tmux_name_of(f"{T} new session")
        hide_keyboard()
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.commands"), 15):
            raise Step.Fail("面板里没有「命令」那一行")
        shot("4c-面板")
        tap(t("mobPlus.row.commands"))
        if not wait("/init", 10):
            raise Step.Fail("命令清单里没有 /init")
        shot("4c-命令清单")
        tap("/init")
        time.sleep(1.5)
        if ui.find("/init ") is None:
            raise Step.Fail("点了 /init 没填进输入框")
        if pane_has(name, "> /init", 2):
            raise Step.Fail("点了命令就发出去了（应只填进输入框）")
        tap("/init ")
        for _ in range(8):
            adb("shell", "input", "keyevent", "67")
        hide_keyboard()
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.clear"), 15):
            raise Step.Fail("面板里没有「清空会话」")
        tap(t("mobPlus.row.clear"))
        if not wait(t("session.clear.body"), 5):
            raise Step.Fail("清空会话没先确认")
        shot("4c-清空先确认")
        tap(t("kit.dialog.cancel"))
        time.sleep(2)
        if pane_has(name, "> /clear", 2):
            raise Step.Fail("取消了照样送了 /clear")
        tap(t("mobPlus.sheet.title"))
        if not wait(t("mobPlus.row.compact"), 15):
            raise Step.Fail("面板里没有「压缩上下文」")
        tap(t("mobPlus.row.compact"))
        if not pane_has(name, "> /compact", 20):
            raise Step.Fail("压缩上下文没送进那个会话")

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

    with Step("7-换网"):
        # 换一张网（Wi-Fi 关掉 ⇒ 落到移动数据，再开回来）：SSH 那层重连换一条，常驻流在新的那条上当场重接；断着那段补上。
        name = tmux_name_of(f"{T} new session")
        adb("shell", "svc", "wifi", "disable")
        time.sleep(4)
        rig("tmux", "send-keys", "-t", name or "", f"{T} switched", "Enter")
        time.sleep(4)
        adb("shell", "svc", "wifi", "enable")
        time.sleep(6)
        to_bottom()
        if not wait(f"替身收到：{T} switched", 60):
            raise Step.Fail("换网之后没补上那一段")
        if not adb("shell", "pidof", PKG).strip():
            raise Step.Fail("App 没了（换网闪退）")

    with Step("8-杀进程"):
        # 进程整个没了（停掉，连前台服务一起）：内存里的续点都没了；重开 ⇒ 核心重接、先取尾巴，被杀那段的话照样在。
        name = tmux_name_of(f"{T} new session")
        adb("shell", "am", "force-stop", PKG)
        time.sleep(2)
        if adb("shell", "pidof", PKG).strip():
            raise Step.Fail("进程没停掉")
        rig("tmux", "send-keys", "-t", name or "", f"{T} while dead", "Enter")
        time.sleep(4)
        launch()
        drawer(t("mobDrawer.item.chats"))
        if not wait(f"{T} new session", 30):
            raise Step.Fail("重开之后对话列表里没有那条会话")
        tap(f"{T} new session")
        time.sleep(3)
        to_bottom()
        if not wait(f"替身收到：{T} while dead", 60):
            raise Step.Fail("重开之后没有被杀那段的话")

    with Step("9-终端屏"):
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
        switch_model(name)
        adb("shell", "input", "keyevent", "4")

    with Step("10-Codex"):
        launch()
        # 哪几家是那台给的（session-new-facts）：新建会话顶栏下那一颗 agent chip 列得出，手机自己不认识 Codex。
        facts = json.loads(subprocess.run([sys.executable, os.path.join(here, "frames.py"), "session-new-facts", "{}"],
                                          capture_output=True, text=True).stdout or "{}")
        names = {a.get("kind"): a.get("text") for a in (facts.get("data") or {}).get("agents") or []}
        if "codex" not in names or "claude" not in names:
            raise Step.Fail(f"那台没给两家：{names!r}")
        drawer(t("mobDrawer.item.newChat"))
        if not wait(names["claude"], 20):
            raise Step.Fail("新建会话没画 agent chip")
        tap(names["claude"])
        if not wait(names["codex"], 10):
            raise Step.Fail("agent 抽屉里列不出那台能起的几家")
        tap(names["codex"])
        time.sleep(1)
        hide_keyboard()
        tap(t("mobChat.input.placeholder")); typed(f"{T} codex")
        hide_keyboard()
        tap(t("mobChat.input.send"))
        if not wait(f"替身收到：{T} codex", 40):
            raise Step.Fail("Codex 答不回来")

    with Step("11-轮换屏"):
        # 从会话进轮换屏（聊天屏「⋯」→ 轮换）· 换策略 · 长按号名拖到最上那条成临时条目 · 改一格 · 存；每一样都核那台的记录。
        seed_rotation()
        started = frames("session-new", {"agent": "claude", "cwd": "~/work", "place": "tmux", "local": False, "ticket": f"{T}rot",
                                         "prompt": f"{T} rotation", "rotation": {"strategy": "smoke-a"}}, 45000)
        sid = (started.get("data") or {}).get("sid")
        if not sid:
            raise Step.Fail(f"台架起不了带轮换的会话：{started!r}"[:300])
        launch()
        drawer(t("mobDrawer.item.chats"))
        if not wait(f"{T} rotation", 40):
            raise Step.Fail("对话列表里没有那条带轮换的会话")
        tap(f"{T} rotation")
        time.sleep(3)
        tap(t("history.row.more"))
        time.sleep(1)
        tap(t("mobChat.more.rotation"))
        if not wait("smoke-a", 20):
            raise Step.Fail("轮换屏没画出那条会话用的策略")
        if not wait(t("rot.h.cards"), 10) or ui.find(t("mobRot.cards.hint")) is None:
            raise Step.Fail("轮换屏没画出轮换账号那一块")
        shot("11a-轮换屏")
        tap("smoke-a")
        if not wait("smoke-b", 10):
            raise Step.Fail("策略抽屉里没有 smoke-b")
        tap("smoke-b")
        end = time.time() + 20
        while time.time() < end and ((frames("rotation-view", {"target": {"sid": sid}}).get("data") or {}).get("strategy") or {}).get("id") != "smoke-b":
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("选了 smoke-b，那台记的那条会话没换过去")
        if not wait("smoke-b", 10):
            raise Step.Fail("换了策略，屏上没跟着换")
        to_bottom()
        time.sleep(1)
        head = lane_head("team")
        top = next((n for n in ui.nodes() if n.get("content-desc") == t("rot.h.sim")), None)
        if head is None or top is None:
            raise Step.Fail("模拟里找不到 team 那一行或最上那条")
        (x1, y1), (x2, y2) = ui.center(head), ui.center(top)
        adb("shell", "input", "draganddrop", str(x1), str(y1), str(x2), str(y2), "1500")
        end = time.time() + 20
        while time.time() < end and "team" not in (((frames("rotation-view", {"target": {"sid": sid}}).get("data") or {}).get("decision") or {}).get("temp") or []):
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("拖了 team 到最上那条，那台没记成临时条目")
        if not wait(t("rot.sim.manual", n=1), 10):
            raise Step.Fail("排了临时条目，模拟的小标题没出「手动 1 段」")
        shot("11b-临时条目")
        adb("shell", "input", "swipe", "540", "700", "540", "1600", "200")
        time.sleep(1)
        box = ui.find("25")
        if box is None:
            raise Step.Fail("找不到「权重容差」那一格的数")
        adb("shell", "input", "tap", *map(str, ui.center(box)))
        time.sleep(0.5)
        adb("shell", "input", "keyevent", "123")
        for _ in range(4):
            adb("shell", "input", "keyevent", "67")
        typed("30")
        adb("shell", "input", "keyevent", "66")
        if not wait(t("rot.top.save"), 15):
            raise Step.Fail("改了一格，底部没出没存条")
        shot("11c-没存")
        tap(t("rot.top.save"))
        end = time.time() + 20
        while time.time() < end and not re.search(r'"close":\s*30(\.0)?\b', sh("cat ~/.cc-monitor/strategies/smoke-b.rhai")):
            time.sleep(1)
        if time.time() >= end:
            raise Step.Fail("点了存，smoke-b 那份文件里没写进 30")
        hide_keyboard()
        if wait(t("rot.top.save"), 3):
            raise Step.Fail("存完没存条还在")
        # 另一个进口：设置 → 机器与账号 → 轮换（这台新会话用的那份），点得到。
        adb("shell", "input", "keyevent", "4")
        time.sleep(1)
        drawer(t("mobDrawer.settings.aria"))
        if not wait(t("mobSettings.entry.rotation"), 10):
            raise Step.Fail("设置里没有「机器与账号 → 轮换」")
        tap(t("mobSettings.entry.rotation"))
        if not wait(t("rot.top.here", machine="smokebox"), 15):
            raise Step.Fail("设置进的轮换屏顶栏不是「设置 · 那台」")

    print()
    print("| 步 | 结果 | 用时 | 截图 |")
    print("|---|---|---|---|")
    for name, res, took, p in results:
        print(f"| {name} | {res} | {took} | {p} |")
    json.dump(results, open(os.path.join(OUT, "results.json"), "w"), ensure_ascii=False, indent=1)
    sys.exit(0 if all(r[1] == "过" for r in results) else 1)


if __name__ == "__main__":
    main()
