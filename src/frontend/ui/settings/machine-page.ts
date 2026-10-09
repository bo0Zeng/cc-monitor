/**
 * 一台机器的页（本机与远端同一个形状）：卡头 ＋「账号 · 轮换 · 别名与配置文件」三栏。
 *
 * 卡头：名字 · 状态点与状态词 · 一行地址与系统 · 掉线时的问题行；右上「连接设置」（只远端）与 ⋯。
 * 「连接设置」在卡头里就地展开；「这台上的 cc-monitor」是卡头里的一折（起停 · 退出时 · 恢复命令 · 输出）。
 * 状态由宿主照后端的回答喂进来（`setConnected`），本模块不判。
 */
import { button } from "../kit/button";
import { fold, setFoldSummary } from "../kit/fold";
import { icon } from "../kit/icon";
import { openMenu, type MenuItem } from "../kit/menu";
import { statusDot, setDot } from "../kit/status-dot";
import { copyText } from "../copy-table";
import { SettingsRouter } from "./router";
import { hostOs, type HostOs } from "./host-os";
import { paintProblem, type MachineFace, type MachineFix } from "./machine-state";
import { CONFIG_SHOWN_EVENT } from "./events";

const OS_NAME: Record<HostOs, string | null> = { linux: "Linux", windows: "Windows", macos: "macOS", unknown: null };

/** 后端报来的那台的两样事实（还没报 ⇒ `null`，那一段不写）。 */
export interface MachineFacts {
  os: string | null;
  /** 产品版本号（与这一版同一份字节时才有）。 */
  version: string | null;
  /** 开发构建（版本不可比）：版本那一段写「开发版」。 */
  dev?: boolean;
}

export const NO_FACTS: MachineFacts = { os: null, version: null };

/**
 * 一台的第二行：列表那一行 `地址 · 系统`；卡头 `地址:端口 · 系统 · 版本 x`。
 * `who` 为 `null` ＝ 本机（`这台电脑`，系统取这扇窗口所在的那台）。
 */
export function machineMeta(at: "list" | "head", who: string | null, facts: MachineFacts): string {
  const os = who === null ? (OS_NAME[hostOs()] ?? facts.os) : facts.os;
  const parts = [who ?? copyText("machinePage.meta.localBare")];
  if (os) parts.push(os);
  if (at === "head" && facts.version) parts.push(copyText("machinePage.meta.version", { ver: facts.version }));
  else if (at === "head" && facts.dev) parts.push(copyText("machineState.word.incomparable"));
  return parts.filter((p) => p !== "").join(copyText("kit.text.sep"));
}

/** 本机那一行 / 卡头的第二行。 */
export function localMeta(at: "list" | "head" = "head", facts: MachineFacts = NO_FACTS): string {
  return machineMeta(at, null, facts);
}

export type MachineSection = "conn" | "cc";

export interface MachinePageSpec {
  pageId: string;
  name: string;
  meta: string;
  /** 连接设置的内容（本机没有）。 */
  connection?: HTMLElement;
  /** 「这台上的 cc-monitor」里的几块。 */
  ccMonitor: HTMLElement[];
  menu: () => MenuItem[];
  /** 问题行上的修法按钮点了。 */
  onFix?: (fix: MachineFix) => void;
}

export interface MachinePage {
  element: HTMLElement;
  /** 三栏的落点：账号 · 轮换 · 别名与配置文件。 */
  slots: { acct: HTMLElement; rot: HTMLElement; config: HTMLElement };
  tabs: SettingsRouter;
  setTitle(name: string): void;
  setMeta(meta: string): void;
  setConnected(connected: boolean | null): void;
  /** 停用了连接：空心点 ＋「已停用」，问题行「连接已停用」（开关在连接设置里）。 */
  setDisabled(on: boolean): void;
  /** 照后端的状态成品画卡头（点 · 词 · 问题行 ＋ 修法）。 */
  setMachine(face: MachineFace): void;
  /** 「这台上的 cc-monitor」折叠头右侧那一句（`版本 x · 在跑`）。 */
  setCcSummary(text: string): void;
  open(section: MachineSection): void;
}

export function buildMachinePage(spec: MachinePageSpec): MachinePage {
  const root = document.createElement("div");
  root.className = "machine-page";

  const head = document.createElement("div");
  head.className = "machine-head";
  root.appendChild(head);

  const top = document.createElement("div");
  top.className = "machine-head-top";
  const title = document.createElement("span");
  title.className = "machine-head-name";
  title.textContent = spec.name;
  const dot = statusDot("unknown", copyText("settingsNav.dot.unknown", { machine: spec.name }), "compact");
  const word = document.createElement("span");
  word.className = "machine-head-state";
  const sp = document.createElement("span");
  sp.className = "machine-head-sp";
  top.append(title, dot, word, sp);

  let connFold: { open(): void; toggle(): void; el: HTMLElement } | null = null;
  if (spec.connection) {
    const body = document.createElement("div");
    body.className = "machine-conn";
    body.hidden = true;
    const t = document.createElement("div");
    t.className = "machine-conn-title";
    t.textContent = copyText("machinePage.conn.how");
    body.append(t, spec.connection);
    const toggleBtn = button({
      label: copyText("machinePage.conn.toggle"),
      icon: "caretDown",
      iconAfter: true,
      onClick: () => setConn(body.hidden),
    });
    toggleBtn.classList.add("machine-conn-toggle");
    toggleBtn.setAttribute("aria-expanded", "false");
    const setConn = (open: boolean): void => {
      body.hidden = !open;
      toggleBtn.setAttribute("aria-expanded", String(open));
      if (open) body.querySelector<HTMLElement>("input, textarea, select, button")?.focus();
    };
    connFold = { open: () => setConn(true), toggle: () => setConn(body.hidden), el: body };
    top.appendChild(toggleBtn);
  }
  const more = button({ label: copyText("machinePage.head.more"), kind: "icon", icon: "more", hint: copyText("machinePage.head.more") });
  more.addEventListener("click", () => {
    openMenu({ el: more, align: "end" }, spec.menu());
  });
  top.appendChild(more);
  head.appendChild(top);

  const meta = document.createElement("div");
  meta.className = "machine-head-meta";
  meta.textContent = spec.meta;
  head.appendChild(meta);

  const problem = document.createElement("div");
  problem.className = "machine-problem";
  problem.hidden = true;
  head.appendChild(problem);

  if (connFold) head.appendChild(connFold.el);

  const ccBody = document.createElement("div");
  ccBody.className = "machine-monitor";
  for (const b of spec.ccMonitor) ccBody.appendChild(b);
  const ccFold = fold({ title: copyText("machinePage.cc.title"), open: false, body: ccBody });
  ccFold.classList.add("machine-monitor-fold");
  head.appendChild(ccFold);

  const tabs = new SettingsRouter({ landingId: `${spec.pageId}#acct`, orientation: "horizontal", hidePageHeader: true });
  const acct = document.createElement("div");
  const rot = document.createElement("div");
  const config = document.createElement("div");
  tabs.addRoute({ id: `${spec.pageId}#acct`, title: copyText("machinePage.tab.accounts"), element: acct });
  tabs.addRoute({ id: `${spec.pageId}#rot`, title: copyText("machinePage.tab.rot"), element: rot });
  tabs.addRoute({ id: `${spec.pageId}#config`, title: copyText("machinePage.tab.config"), element: config });
  root.appendChild(tabs.element);
  // 切到「别名与配置文件」那一栏 ⇒ 告诉那一栏里的每一块（第一次露出来才问那台，之后每次露出来重读）。
  tabs.onNavigate((id) => {
    if (id !== `${spec.pageId}#config`) return;
    for (const c of config.querySelectorAll("[data-config-shown]")) c.dispatchEvent(new CustomEvent(CONFIG_SHOWN_EVENT));
  });

  let name = spec.name;
  let connected: boolean | null = null;
  let disabled = false;
  const paint = (): void => {
    if (disabled) {
      setDot(dot, "exited", copyText("settingsNav.dot.disabled", { machine: name }));
      word.textContent = copyText("machinePage.state.disabled");
      const text = document.createElement("span");
      text.textContent = copyText("machinePage.problem.disabled");
      problem.replaceChildren(text);
      if (connFold) problem.appendChild(button({ label: copyText("machinePage.conn.toggle"), size: "compact", onClick: () => connFold!.open() }));
      problem.hidden = false;
      return;
    }
    if (connected === true) {
      setDot(dot, "up", copyText("settingsNav.dot.up", { machine: name }));
      word.textContent = copyText("machinePage.state.up");
    } else if (connected === false) {
      setDot(dot, "failed", copyText("settingsNav.dot.down", { machine: name }));
      word.textContent = copyText("machinePage.state.down");
    } else {
      setDot(dot, "unknown", copyText("settingsNav.dot.unknown", { machine: name }));
      word.textContent = "";
    }
    problem.replaceChildren();
    problem.hidden = connected !== false;
    if (connected === false) {
      const err = icon("error", "compact");
      const text = document.createElement("span");
      text.textContent = copyText("machinePage.problem.offline", { machine: name });
      problem.append(err, text);
      if (connFold) {
        const fix = button({ label: copyText("machinePage.conn.toggle"), size: "compact", onClick: () => connFold!.open() });
        problem.appendChild(fix);
      }
    }
  };
  paint();

  return {
    element: root,
    slots: { acct, rot, config },
    tabs,
    setTitle(n) {
      name = n;
      title.textContent = n;
      paint();
    },
    setMeta(m) {
      meta.textContent = m;
    },
    setConnected(c) {
      connected = c;
      paint();
    },
    setMachine(f) {
      setDot(dot, f.dot, f.word || copyText("settingsNav.dot.up", { machine: name }));
      word.textContent = f.word;
      paintProblem(problem, f, (fix) => (fix === "conn_settings" && connFold ? connFold.open() : spec.onFix?.(fix)));
    },
    setCcSummary(text) {
      setFoldSummary(ccFold, text);
    },
    setDisabled(on) {
      if (disabled === on) return;
      disabled = on;
      paint();
    },
    open(section) {
      if (section === "conn") {
        connFold?.open();
        connFold?.el.scrollIntoView?.({ block: "nearest" });
        return;
      }
      const h = ccFold.querySelector<HTMLButtonElement>("button[aria-expanded]");
      if (h?.getAttribute("aria-expanded") === "false") h.click();
      ccFold.scrollIntoView?.({ block: "nearest" });
    },
  };
}
