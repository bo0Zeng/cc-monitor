/**
 * 一台机器的页（本机与远端同一个形状）：卡头 ＋「账号 · 轮换 · 别名与配置文件」三栏。
 *
 * 卡头：名字 · 状态点与状态词 · 一行地址与系统 · 怎么连过去的那一行（有才出）· 掉线时的问题行；右上「连接设置」（只远端）与 ⋯（成品有复制详情 ⇒ 末尾「复制详情」）。
 * 「连接设置」在卡头里就地展开；「这台上的 cc-monitor」是卡头里的一折（起停 · 退出时 · 恢复命令 · 输出）。
 * 状态由宿主喂进来，本模块不判：点 · 词 · 问题行只由一份样子写（`setMachine`：那台状态，本机后端没起来时是壳那一格，都没有 ⇒ [`UNKNOWN_FACE`]）。
 */
import { button } from "../kit/button";
import { fold, setFoldSummary } from "../kit/fold";
import { openMenu, type MenuItem } from "../kit/menu";
import { statusDot, setDot } from "../kit/status-dot";
import { copyText } from "../copy-table";
import { SettingsRouter } from "./router";
import { hostOs, type HostOs } from "./host-os";
import { paintProblem, paintRoute, type MachineFace, type MachineFix } from "./machine-state";
import { writeClipboard } from "../clipboard";
import { failToast, toast } from "../kit/toast";
import { CONFIG_SHOWN_EVENT } from "./events";

const OS_NAME: Record<HostOs, string | null> = { linux: "Linux", windows: "Windows", macos: "macOS", unknown: null };

/** 后端报来的那台的两样事实（还没报 ⇒ `null`，那一段不写）。 */
export interface MachineFacts {
  os: string | null;
  /** 产品版本号（与这一版同一份字节时才有）。 */
  version: string | null;
}

export const NO_FACTS: MachineFacts = { os: null, version: null };

/** 那台还没有状态（成品没到 · 壳也没说本机后端没起来）：「说不清」那一形 —— 没问到的点，没有词与问题行。 */
export const UNKNOWN_FACE: MachineFace = { dot: "unknown", word: "", problem: "", tone: "warn", bar: false, fixes: [], detail: "", next: "", route: "" };

/** 那台的点的读屏名（导航 · 列表那一行 · 卡头同一句）：有词 ⇒ `名字 · 词`；「说不清」那一形 ⇒ 状态不明；别的没词 ⇒ 在线。 */
export function dotLabel(name: string, face: MachineFace): string {
  if (face.word) return `${name} · ${face.word}`;
  return face === UNKNOWN_FACE ? copyText("settingsNav.dot.unknown", { machine: name }) : copyText("settingsNav.dot.up", { machine: name });
}

/**
 * 一台的第二行：列表那一行 `地址 · 系统`；卡头 `地址:端口 · 系统 · 版本 x`。
 * `who` 为 `null` ＝ 本机（`这台电脑`，系统取这扇窗口所在的那台）。
 */
export function machineMeta(at: "list" | "head", who: string | null, facts: MachineFacts): string {
  const os = who === null ? (OS_NAME[hostOs()] ?? facts.os) : facts.os;
  const parts = [who ?? copyText("detail.label.local")];
  if (os) parts.push(os);
  if (at === "head" && facts.version) parts.push(copyText("machinePage.meta.version", { ver: facts.version }));
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
  /** 照那一份样子画卡头（点 · 词 · 问题行 ＋ 修法 · 怎么连过去）。卡头这几格只有这一个写口。 */
  setMachine(face: MachineFace): void;
  /** 「这台上的 cc-monitor」折叠头右侧那一句（与后端那一行状态格同一句）。 */
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
  const more = button({ label: copyText("history.row.more"), kind: "icon", icon: "more", hint: copyText("history.row.more") });
  /** 成品的复制详情（连着时是每条连接那一段；空 ⇒ ⋯ 里不出「复制详情」）。 */
  let detail = "";
  const copyItem: MenuItem = { label: copyText("detail.act.copy"), onClick: () => void copyDetail(detail) };
  more.addEventListener("click", () => {
    openMenu({ el: more, align: "end" }, detail === "" ? spec.menu() : [...spec.menu(), copyItem]);
  });
  top.appendChild(more);
  head.appendChild(top);

  const meta = document.createElement("div");
  meta.className = "machine-head-meta";
  meta.textContent = spec.meta;
  head.appendChild(meta);

  // 怎么连过去的（成品写好的那一句：经跳板 · 经代理）；直连不经代理 ⇒ 不出。每条连接的读数在 ⋯ ›「复制详情」里。
  const route = document.createElement("div");
  route.className = "machine-route";
  route.hidden = true;
  head.appendChild(route);

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
  tabs.addRoute({ id: `${spec.pageId}#acct`, title: copyText("beProfile.slot.account"), element: acct });
  tabs.addRoute({ id: `${spec.pageId}#rot`, title: copyText("machinePage.tab.rot"), element: rot });
  tabs.addRoute({ id: `${spec.pageId}#config`, title: copyText("machinePage.tab.config"), element: config });
  root.appendChild(tabs.element);
  // 切到「别名与配置文件」那一栏 ⇒ 告诉那一栏里的每一块（第一次露出来才问那台，之后每次露出来重读）。
  tabs.onNavigate((id) => {
    if (id !== `${spec.pageId}#config`) return;
    for (const c of config.querySelectorAll("[data-config-shown]")) c.dispatchEvent(new CustomEvent(CONFIG_SHOWN_EVENT));
  });

  let name = spec.name;
  let face = UNKNOWN_FACE;

  return {
    element: root,
    slots: { acct, rot, config },
    tabs,
    setTitle(n) {
      name = n;
      title.textContent = n;
      setDot(dot, face.dot, dotLabel(name, face));
    },
    setMeta(m) {
      meta.textContent = m;
    },
    setMachine(f) {
      face = f;
      setDot(dot, f.dot, dotLabel(name, f));
      word.textContent = f.word;
      paintProblem(problem, f, (fix) => (fix === "conn_settings" && connFold ? connFold.open() : spec.onFix?.(fix)));
      paintRoute(route, f);
      detail = f.detail;
    },
    setCcSummary(text) {
      setFoldSummary(ccFold, text);
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

/** ⋯ ›「复制详情」：成品那一份原样写进剪贴板；写成说一句已复制，写不进照壳那一句报。 */
async function copyDetail(text: string): Promise<void> {
  try {
    await writeClipboard(text);
    toast(copyText("detail.act.copied"), "", { level: "success" });
  } catch (e) {
    failToast(copyText("detail.act.failed"), e);
  }
}
