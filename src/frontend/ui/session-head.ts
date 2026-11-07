/**
 * **会话头**（主区顶上 40px）：状态点 · 标题全名 · 「机器 · 目录」· 状态一句；右边一排按钮 ＋ 最右「⋯」（＝这个标签页的右键菜单）。
 * 按状态多一颗、永远在最左：已结束［恢复 ▾］。
 *
 * 右边那一排由用户自己排（默认：终端 · 在会话里找）：存 `config.json` 的 `sessionHead.buttons`（这扇界面自己的偏好；删掉这个键 ＝ 恢复默认）。
 * 入口：按钮那一排右键（调整… · 从会话头移走「x」）·「⋯」最后一项 · 命令面板；编辑浮层见 `session-head-edit.ts`。
 * 本会话用不上的（远端无目录）不出、不留空位；「终端」一直在，做不了灰着、悬停写那台写好的那一句。窄时先藏目录、再藏状态一句与换模型 · 切权限档那两颗（CSS 容器查询）；
 * 还放不下 ⇒ 从排在最后的那颗起收进「⋯」最上面一组（不改设置，变宽就回来）。
 *
 * 「终端」点了做什么照脸上那一格（`face-can.ts::terminalWay`：拉它的外部窗口 · 在这里开它的终端，宿主那一条判）。
 * 两个流开关（过程默认展开 · 显示系统注入）只住「⋯」最后一组。
 * 在新窗口打开 · 账号 · 固定 · 分组 · 恢复 · 关闭标签页那几颗借「⋯」那份菜单的同一项（字、能不能点、点了做什么都是那一项）。
 *
 * 标题后面按会话多几颗小标：被隐藏的 `已隐藏`（悬停是后端那一句）· 它是别的会话起的 `由 X 启动`（点了切回主会话）·
 * 它起过子会话 `子会话 N`（点开列子会话：名字 · 状态点 · 已隐藏标，点一行切过去）。血缘两格读会话事实（`history-facts`），界面不判。
 * 状态一句之后是换模型 · 切权限档那两颗（`session-switch.ts`：字 · 清单 · 能不能换 · 结局那一句都照那台）。
 *
 * 只排版：一句话怎么写从 `session-face.ts` 取；做事的都交宿主（`TabManager` 那几条）。
 */
import type { Tab } from "./tab-model";
import { isRemoteOrigin } from "./ipc/origin";
import { machineName } from "./ipc/chan-caller";
import { canResume, terminalWay } from "./face-can";
import { dispatcher, KeybindingDispatcher } from "./keybindings/registry";
import { copyText } from "./copy-table";
import { icon, type IconName } from "./kit/icon";
import { statusDot, setDot } from "./kit/status-dot";
import { attachTooltip } from "./kit/tooltip";
import { button } from "./kit/button";
import { dotOf, stateLine, stateWord } from "./session-face";
import { tag } from "./kit/badge";
import { openMenu, type MenuItem } from "./kit/menu";
import { closePopover, openPopover, popoverOpenOn } from "./kit/popover";
import { undoToast } from "./kit/toast";
import { loadConfig, patchConfig, removeAt, setAt } from "./config";
import { DEFAULT_HEAD_BUTTONS, headButtonsThatFit, sanitizeHeadButtons, type HeadButtonId } from "./session-head-buttons";
import { HEAD_BUTTON_FACE, HeadEditor } from "./session-head-edit";
import { SwitchChips, type SwitchHost } from "./session-switch";
import s from "./session-head.module.css";

/** `config.json` 里这扇界面自己那一格（登记在 `config.ts::CONFIG_KEY_OWNERS`）。 */
const KEY = "sessionHead";
/** 放不下时标题至少留这么宽（再窄就收按钮）。 */
const TITLE_MIN_PX = 160;

/** 借「⋯」那份菜单的哪一项（宿主照这个会话现拼）。 */
export type HeadMenuId = "popOut" | "account" | "pin" | "group" | "resume" | "close";

/** 交给「⋯」的那几组：最上面 ＝ 放不下收进来的；最后 ＝ 两个流开关 ＋「调整会话头按钮…」。 */
export interface HeadMoreExtra {
  top: MenuItem[];
  bottom: MenuItem[];
}

export interface SessionHeadHost {
  active(): Tab | null;
  openCwd(sid: string): void;
  /** 「终端」那一颗（拉窗还是在这里开照脸上那一格）。 */
  terminal(sid: string): void;
  find(): void;
  more(anchor: HTMLElement, sid: string, extra: HeadMoreExtra): void;
  resume(anchor: HTMLElement, sid: string): void;
  /** 这个会话是计划里哪一块的接手 ⇒ 那一枚标的字与点了去哪；不是 ⇒ `null`。 */
  planMark(tab: Tab): { slice: string; label: string; open: () => void } | null;
  /** 「⋯」那份菜单里的这一项（这个会话此刻没有这一项 ⇒ `null`）。 */
  menuItem(sid: string, id: HeadMenuId): MenuItem | null;
  /** 两个流开关（「⋯」最后一组）。 */
  displayItems(): MenuItem[];
  /** 这个会话在不在栏上（标签页或「已隐藏」那一行里）：血缘那两颗只对在的切得过去。 */
  tabOf(sid: string): Tab | null;
  /** 切过去（同点标签页；在「已隐藏」里的那一行跟着展开）。 */
  switchTo(sid: string): void;
  /** 换模型 · 切权限档那两颗要的（清单 · 交命令 · 握手说做不到的那一句 · toast）。 */
  switching: SwitchHost;
}

/** 这个会话此刻「终端」那一颗点不点得了（会话头与消息流里的报错卡同一道）：本机标签页挑 near、远端挑 far。 */
export function canTerminal(tab: Tab): boolean {
  const way = terminalWay(tab.face, isRemoteOrigin(tab.origin));
  return way !== null && way.go !== "no";
}

type Chorded = Parameters<typeof dispatcher.effectiveChord>[0];

/** 此刻的键位（没绑 ⇒ `undefined`）。 */
function chordOf(action: Chorded | null): string | undefined {
  const chord = action === null ? null : dispatcher.effectiveChord(action);
  return chord ? KeybindingDispatcher.prettyChord(chord) : undefined;
}

/** 悬停「名字 · 当前键位」（按 `config.json.keybindings` 现拼）。 */
function keyed(name: string, action: Chorded | null): () => string {
  return () => {
    const k = chordOf(action);
    return k ? `${name} · ${k}` : name;
  };
}

/** 一颗画出来的按钮 ＋ 收进「⋯」时它那一项。 */
interface Built {
  id: HeadButtonId;
  el: HTMLButtonElement;
  item: MenuItem;
}

export class SessionHead {
  readonly el: HTMLElement;
  private readonly dot: HTMLSpanElement;
  private readonly title: HTMLSpanElement;
  private readonly where: HTMLSpanElement;
  private readonly dir: HTMLSpanElement;
  private readonly state: HTMLSpanElement;
  private readonly plan: HTMLSpanElement;
  /** 已隐藏 · 由 X 启动 · 子会话 N 那几颗。 */
  private readonly kin: HTMLSpanElement;
  private readonly extra: HTMLSpanElement;
  private readonly acts: HTMLSpanElement;
  /** 换模型 · 切权限档那两颗（状态一句之后、按状态那一颗之前；不进自排清单、不收进「⋯」）。 */
  private readonly chips: SwitchChips;
  private drawn: string | null = null;
  /** 盘上那一格（`null` ＝ 没设过 ⇒ 默认）。 */
  private saved: HeadButtonId[] | null = null;
  /** 此刻画在会话头上的那几颗（含收进「⋯」的）与「⋯」本身。 */
  private built: Built[] = [];
  /** 最右「⋯」：一直是这一颗（编辑浮层锚在它上面，换会话、改排法都不换它）。 */
  private readonly moreBtn: HTMLButtonElement;
  /** 上次画出去的那一排（一样就不换 DOM：悬停提示、↗ 的结果浮层都锚在这些按钮上）。 */
  private actsDrawn = "";
  /** 收进「⋯」的那几颗（从后往前收，这里按会话头顺序）。 */
  private folded: Built[] = [];
  private editor: HeadEditor | null = null;

  /** 宿主拿 `el` 顶掉 `index.html` 里那一格占位（`#session-head`，认领 `#app` 网格的 `head` 区）。 */
  constructor(private readonly host: SessionHeadHost) {
    this.el = document.createElement("div");
    this.el.id = "session-head";
    this.el.className = s.shHead;
    this.dot = statusDot("running", copyText("backend.status.connected"), "compact");
    this.title = document.createElement("span");
    this.title.className = s.shTitle;
    this.title.dataset.headTitle = "";
    this.where = document.createElement("span");
    this.where.className = s.shWhere;
    this.dir = document.createElement("span");
    this.dir.className = s.shDir;
    this.state = document.createElement("span");
    this.state.className = s.shState;
    this.plan = document.createElement("span");
    this.plan.className = s.shPlanSlot;
    this.kin = document.createElement("span");
    this.kin.className = s.shPlanSlot;
    const sp = document.createElement("span");
    sp.className = s.shSp;
    this.extra = document.createElement("span");
    this.extra.className = s.shExtra;
    this.acts = document.createElement("span");
    this.acts.className = s.shActs;
    this.acts.addEventListener("contextmenu", (e) => this.onActsMenu(e));
    this.moreBtn = this.iconButton("more", copyText("history.row.more"), () => copyText("history.row.more"), (b) => {
      const t = this.host.active();
      if (t) this.host.more(b, t.sessionId, this.moreExtra());
    });
    this.moreBtn.dataset.head = "more";
    this.chips = new SwitchChips(host.switching);
    this.el.append(this.dot, this.title, this.where, this.dir, this.state, this.plan, this.kin, sp, this.chips.el, this.extra, this.acts);
    this.el.style.display = "none";
    // 窗口宽窄变了 ⇒ 重新看放不放得下（收进去的变宽就回来）。
    if (typeof ResizeObserver !== "undefined") new ResizeObserver(() => this.fit()).observe(this.el);
  }

  /** 会话头上此刻排的那几颗（没设过 ⇒ 默认那一排）。 */
  buttons(): readonly HeadButtonId[] {
    return this.saved ?? DEFAULT_HEAD_BUTTONS;
  }

  /** 读盘上那一格（启动时一次）。认不出的 id 跳过。 */
  async loadButtons(): Promise<void> {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const v = cfg[KEY];
    this.saved = sanitizeHeadButtons(v && typeof v === "object" ? (v as Record<string, unknown>).buttons : undefined);
    this.repaint();
  }

  /** 改成这一排：当场重画、落盘（按键补丁，只动 `sessionHead.buttons`）。 */
  private setButtons(next: HeadButtonId[] | null): void {
    this.saved = next === null ? null : [...next];
    this.repaint();
    void patchConfig([next === null ? removeAt([KEY]) : setAt([KEY, "buttons"], [...next])]).catch((e: unknown) => console.warn("[session-head] 写会话头按钮失败：", e));
  }

  /** 设置变了 / 开关变了：整个重画（按钮排与浮层清单）。 */
  private repaint(): void {
    this.drawn = null;
    this.actsDrawn = "";
    this.render();
    this.editor?.update();
  }

  /** 编辑浮层（锚在「⋯」下方；已开着 ⇒ 关）。命令面板「调整会话头按钮…」也走这里。 */
  openEditor(): void {
    const anchor = this.moreBtn;
    if (!anchor.isConnected) return;
    if (popoverOpenOn(anchor)) {
      closePopover();
      return;
    }
    const ed = new HeadEditor({
      buttons: () => this.buttons(),
      set: (next) => this.setButtons(next),
      reset: () => this.resetButtons(),
    });
    if (!openPopover(anchor, ed.el, { label: copyText("sessionHead.edit.title"), onClose: () => (this.editor = null) })) return;
    this.editor = ed;
    ed.focusFirst();
  }

  /** ［恢复默认］：删掉这个键，toast 可撤（撤 ＝ 写回原来那一排）。 */
  private resetButtons(): void {
    const was = this.saved;
    this.setButtons(null);
    undoToast(copyText("sessionHead.edit.resetDone"), () => this.setButtons(was), () => {});
  }

  /** 按钮那一排右键：调整… · 指着哪颗（能移走的）就多一项「从会话头移走「x」」。 */
  private onActsMenu(e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    const at = (e.target as HTMLElement | null)?.closest<HTMLElement>("[data-head]")?.dataset.head;
    const hit = this.built.find((b) => b.id === at);
    const items: MenuItem[] = [{ icon: "sliders", label: copyText("sessionHead.edit.open"), onClick: () => this.openEditor() }];
    if (hit) {
      const name = HEAD_BUTTON_FACE[hit.id].name();
      items.push({
        icon: "minimize",
        label: copyText("sessionHead.edit.remove", { name }),
        onClick: () => {
          const was = this.saved;
          this.setButtons(this.buttons().filter((x) => x !== hit.id));
          undoToast(copyText("sessionHead.edit.removed", { name }), () => this.setButtons(was), () => {});
        },
      });
    }
    openMenu({ x: e.clientX, y: e.clientY }, items);
  }

  private iconButton(name: IconName, label: string, hint: () => string, onClick: (b: HTMLButtonElement) => void): HTMLButtonElement {
    const b = document.createElement("button");
    b.type = "button";
    b.className = s.shIb;
    b.setAttribute("aria-label", label);
    b.appendChild(icon(name));
    attachTooltip(b, hint);
    b.addEventListener("click", () => onClick(b));
    return b;
  }

  /** 借「⋯」那一项画一颗：字 · 能不能点（灰着的悬停说为什么）· 有子菜单 ⇒ 锚在按钮上开。 */
  private fromItem(id: HeadButtonId, it: MenuItem, label: string, action: Chorded | null, said: string = label): Built {
    const off = it.enabled === false;
    const hint = off && it.why ? () => `${label} · ${it.why}` : keyed(said, action);
    const el = this.iconButton(HEAD_BUTTON_FACE[id].icon, label, hint, (b) => {
      if (off) return;
      if (it.submenu) openMenu({ el: b, align: "end" }, it.submenu);
      else it.onClick?.();
    });
    if (off) el.setAttribute("aria-disabled", "true");
    // 收进「⋯」时与菜单里原来那一项同在一份菜单里 ⇒ 摘掉 id（菜单按 id 就地换项，不能撞）。
    return { id, el, item: { ...it, id: undefined, icon: HEAD_BUTTON_FACE[id].icon } };
  }

  /** 两个流开关（点了重画：「⋯」里的勾跟着变）。 */
  private displayItems(): MenuItem[] {
    return this.host.displayItems().map((it) => ({ ...it, onClick: () => (it.onClick?.(), this.repaint()) }));
  }

  /** 这一颗在这个会话上画成什么样；本会话用不上 ⇒ `null`（不出、不留空位）。 */
  private build(id: HeadButtonId, tab: Tab, extra: string): Built | null {
    const sid = tab.sessionId;
    const face = HEAD_BUTTON_FACE[id];
    const plain = (label: string, action: Chorded | null, run: (b: HTMLButtonElement) => void, menuLabel = label): Built => ({
      id,
      el: this.iconButton(face.icon, label, keyed(label, action), run),
      item: { icon: face.icon, label: menuLabel, detail: chordOf(action), onClick: () => run(this.moreBtn) },
    });
    switch (id) {
      case "cwd":
        return tab.projectDir ? plain(copyText("tabBarView.tab.cwdHint"), "tab.open-cwd", () => this.host.openCwd(sid), copyText("tabBarView.tab.cwdHint")) : null;
      case "terminal": {
        // 一直画：做不了灰着、悬停写那台写好的那一句；做得了 ⇒ 悬停是去做什么（拉窗 · 在这里开）＋ 键。
        const way = terminalWay(tab.face, isRemoteOrigin(tab.origin));
        const label = copyText("mobDrawer.item.terminal");
        const off = way === null || way.go === "no";
        const item: MenuItem = { label, detail: chordOf("session.terminal"), enabled: !off, why: way?.go === "no" ? way.why : undefined, onClick: () => this.host.terminal(sid) };
        return this.fromItem(id, item, label, "session.terminal", way && way.go !== "no" ? way.said : label);
      }
      case "find":
        return plain(copyText("sessionHead.act.find"), "session.find", () => this.host.find());
      default: {
        // 按状态那一颗已经是［恢复 ▾］⇒ 不出第二颗。
        if (id === "resume" && extra === "resume") return null;
        const it = this.host.menuItem(sid, id as HeadMenuId);
        if (!it) return null;
        const label = id === "popOut" ? copyText("history.menu.openWindow") : id === "account" ? copyText("beProfile.slot.account") : it.label;
        return this.fromItem(id, it, label, id === "popOut" ? "tab.pop-out" : null);
      }
    }
  }

  /** 当前 tab 变了 / 它的状态变了 ⇒ 重画（与上次画出去的一样就一个 DOM 都不写）。 */
  render(now: number = Date.now()): void {
    const tab = this.host.active();
    if (!tab) {
      if (this.drawn !== "") {
        this.drawn = "";
        this.el.style.display = "none";
      }
      return;
    }
    this.chips.render(tab);
    const d = dotOf(tab);
    const st = stateLine(tab, now);
    const remote = isRemoteOrigin(tab.origin);
    const way = JSON.stringify(terminalWay(tab.face, remote));
    // 那台看不见 ⇒ 不另给［重新连接］：栏底机器条那一行说、那一行修（A2-17-15-2）。去终端只有「终端」那一颗，按状态那一颗不另放。
    const extra = canResume(tab.face) ? "resume" : "";
    const mark = this.host.planMark(tab);
    const kin = this.kinKey(tab);
    const drawn = [tab.sessionId, d, tab.title, machineName(tab.origin), tab.cwdShown ?? "", st.text, st.needs ? 1 : 0, remote ? 1 : 0, way, extra, mark?.label ?? "", tab.pinned ? 1 : 0, kin].join("\u0000");
    if (drawn === this.drawn) return;
    this.drawn = drawn;
    this.el.style.display = "";
    setDot(this.dot, d, stateWord(tab));
    this.title.textContent = tab.title;
    this.where.textContent = machineName(tab.origin);
    this.dir.textContent = tab.cwdShown ?? "";
    this.dir.style.display = tab.cwdShown === null ? "none" : "";
    this.state.textContent = st.text;
    this.state.className = st.needs ? `${s.shState} ${s.shNeed}` : s.shState;
    const sid = tab.sessionId;
    const extraBtn = extra === "resume" ? button({ label: copyText("history.row.resume"), size: "compact", icon: "caretDown", onClick: (e) => this.host.resume(e.currentTarget as HTMLElement, sid) }) : null;
    this.extra.replaceChildren(...(extraBtn ? [extraBtn] : []));
    // 计划那一枚标：这个会话是哪一片哪一块的接手、在什么阶段；点了开计划页并选中那一块。
    if (mark) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = `${s.shPlan} session-plan-mark`;
      b.append(icon("plan", "compact"), document.createTextNode(mark.label));
      attachTooltip(b, () => copyText("plan.head.mark", { slice: mark.slice }));
      b.addEventListener("click", () => mark.open());
      this.plan.replaceChildren(b);
    } else this.plan.replaceChildren();
    this.kin.replaceChildren(...this.kinChips(tab));
    // 右边一排：照用户排的那几颗，本会话用不上的不出；最右「⋯」。和上次画的一样 ⇒ 不换。
    const built = this.buttons().map((id) => this.build(id, tab, extra)).filter((b): b is Built => b !== null);
    const actsDrawn = [sid, way, ...built.map((b) => `${b.id}:${b.el.getAttribute("aria-label")}:${b.el.getAttribute("aria-disabled") ?? ""}`)].join("\u0000");
    if (actsDrawn === this.actsDrawn) return;
    this.actsDrawn = actsDrawn;
    this.built = built;
    for (const b of built) b.el.dataset.head = b.id;
    this.acts.replaceChildren(...built.map((b) => b.el), this.moreBtn);
    this.fit();
  }

  /** 隐藏那一格与血缘两格此刻的样子（变了才重画）；子会话在不在栏上也算（切不切得过去跟着变）。 */
  private kinKey(tab: Tab): string {
    const lin = tab.lineage;
    const p = lin?.startedBy ?? null;
    const kids = (lin?.children ?? []).map((c) => `${c.sid}:${c.title}:${c.face.dot}:${c.hidden?.said ?? ""}:${this.host.tabOf(c.sid) ? 1 : 0}`).join(",");
    return [tab.hidden?.said ?? "", p ? `${p.sid}:${lin?.startedByText ?? ""}:${this.host.tabOf(p.sid) ? 1 : 0}` : "", lin?.childrenText ?? "", kids].join("\u0001");
  }

  /** 已隐藏（悬停那一句）· 由 X 启动（点了切回主会话）· 子会话 N（点开列子会话）：血缘那两句照后端写好的摆（`startedByText` · `childrenText`）。 */
  private kinChips(tab: Tab): HTMLElement[] {
    const out: HTMLElement[] = [];
    if (tab.hidden) {
      const t = tag(copyText("history.row.hidden"));
      t.dataset.head = "hidden";
      attachTooltip(t, () => tab.hidden?.said ?? null);
      out.push(t);
    }
    const chip = (role: string, text: string, onClick: ((b: HTMLButtonElement) => void) | null): HTMLButtonElement => {
      const b = document.createElement("button");
      b.type = "button";
      b.className = s.shPlan;
      b.dataset.head = role;
      b.textContent = text;
      if (onClick) b.addEventListener("click", () => onClick(b));
      else b.setAttribute("aria-disabled", "true");
      return b;
    };
    const lin = tab.lineage;
    const parent = lin?.startedBy ?? null;
    if (parent && lin?.startedByText) {
      const there = this.host.tabOf(parent.sid) !== null;
      out.push(chip("parent", lin.startedByText, there ? () => this.host.switchTo(parent.sid) : null));
    }
    if (lin?.childrenText && lin.children.length > 0) out.push(chip("children", lin.childrenText, (b) => this.openChildren(b, tab)));
    return out;
  }

  /** 「子会话 N」点开：每个子会话一行（状态点 · 名字 · 已隐藏标）；在栏上的点一行切过去，不在的灰着。 */
  private openChildren(anchor: HTMLElement, tab: Tab): void {
    const items: MenuItem[] = (tab.lineage?.children ?? []).map((c) => {
      const t = this.host.tabOf(c.sid);
      return {
        id: c.sid,
        label: c.title,
        avatar: t ? statusDot(dotOf(t), stateWord(t), "compact") : statusDot(c.face.dot, c.face.name, "compact"),
        detail: c.hidden ? copyText("history.row.hidden") : undefined,
        enabled: t !== null,
        onClick: () => this.host.switchTo(c.sid),
      };
    });
    openMenu({ el: anchor, align: "start" }, items, { label: tab.lineage?.childrenText ?? "" });
  }

  /** 「⋯」最上面一组（收进来的）与最后一组（两个流开关 ＋ 调整会话头按钮…）。 */
  private moreExtra(): HeadMoreExtra {
    const top = this.folded.map((b) => b.item);
    return {
      top: top.length > 0 ? [...top, { label: "", divider: true }] : [],
      bottom: [...this.displayItems(), { label: "", divider: true }, { icon: "sliders", label: copyText("sessionHead.edit.open"), onClick: () => this.openEditor() }],
    };
  }

  /**
   * 放不下时收：标题挤到 160 以下（或它本来就短 ⇒ 挤到它的全长以下）⇒ 从排在最后的那颗起收进「⋯」。
   * 先全露出来再量（变宽了收进去的就回来）；不改用户的设置。
   */
  fit(): void {
    const btns = this.built;
    const show = (k: number): number => {
      btns.forEach((b, i) => (b.el.style.display = i < k ? "" : "none"));
      return this.title.clientWidth;
    };
    show(btns.length);
    const k = headButtonsThatFit(btns.length, show, this.title.scrollWidth, TITLE_MIN_PX);
    show(k);
    this.folded = btns.slice(k);
  }
}
