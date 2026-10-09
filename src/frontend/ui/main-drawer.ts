/**
 * 主窗口底部抽屉：任务 · agent · 终端三页共用一个 kit 底部抽屉（`kit/dock.ts`），从状态栏那几枚 chip / 会话头「看它的终端」/ 各自的键 / 命令面板开。
 *
 * - 开着时点另一枚 ⇒ 换页不关；点当前页那枚 / Esc ⇒ 收起。页签上的字就是对应 chip 的字（没内容时写页名）。
 * - 切标签页：抽屉开着就照开，各页换成新会话那一份；新会话那一页没有内容 ⇒ 页里写空态，不自己藏起来。
 * - 开没开、开哪页、多高记在本机（下次启动照上次）。
 */
import { Dock } from "./kit/dock";
import { LS_KEYS, safeGet, safeSet } from "./local-storage";
import { copyText } from "./copy-table";
import type { TasksPanel } from "./tasks-panel";
import type { AgentsPanel } from "./agents-panel";
import type { TerminalPage } from "./terminal-page";

export type DrawerPage = "tasks" | "agents" | "terminal";

const PAGES: readonly DrawerPage[] = ["tasks", "agents", "terminal"];

/** 缺省高；拖的下限。上限是主区的一半（拖的那一刻现算）。 */
export const DRAWER_DEFAULT_H = 240;
export const DRAWER_MIN_H = 120;

interface Saved {
  page: DrawerPage | null;
  height: number;
}

/** 盘上那一格 ⇒ 开哪页、多高（读不懂的格回到缺省：收着 · 240）。 */
export function readSaved(raw: string | null): Saved {
  const out: Saved = { page: null, height: DRAWER_DEFAULT_H };
  if (raw === null) return out;
  try {
    const v = JSON.parse(raw) as unknown;
    if (v && typeof v === "object") {
      const o = v as Record<string, unknown>;
      if (typeof o.page === "string" && (PAGES as readonly string[]).includes(o.page)) out.page = o.page as DrawerPage;
      if (typeof o.height === "number" && Number.isFinite(o.height) && o.height >= DRAWER_MIN_H) out.height = Math.round(o.height);
    }
  } catch {
    /* 读不懂 ⇒ 缺省 */
  }
  return out;
}

export class MainDrawer {
  readonly dock: Dock<DrawerPage>;
  private saved: Saved;

  constructor(
    private readonly tasks: TasksPanel,
    private readonly agents: AgentsPanel,
    private readonly terminal: TerminalPage,
    /** 主区此刻多高（抽屉最多占一半）。 */
    mainHeight: () => number,
  ) {
    this.saved = readSaved(safeGet(LS_KEYS.bottomDrawer));
    this.dock = new Dock<DrawerPage>({
      label: copyText("mainDrawer.ctor.label"),
      pages: [
        { key: "tasks", icon: "tasks", label: tasks.label, body: tasks.pageElement },
        { key: "agents", icon: "agent", label: agents.label, body: agents.pageElement },
        { key: "terminal", icon: "terminal", label: copyText("terminal.page.title"), body: terminal.el },
      ],
      current: this.saved.page,
      height: this.saved.height,
      defaultHeight: DRAWER_DEFAULT_H,
      min: DRAWER_MIN_H,
      max: () => Math.floor(mainHeight() / 2),
      onChange: (page) => {
        this.saved.page = page;
        this.save();
        this.sync();
      },
      onResize: (h) => {
        this.saved.height = h;
        this.save();
      },
      escInside: () => terminal.escFromInput(),
    });
    tasks.onChip = () => this.toggle("tasks");
    agents.onChip = () => this.toggle("agents");
    tasks.onLabel = (l) => this.dock.setLabel("tasks", l);
    agents.onLabel = (l) => this.dock.setLabel("agents", l);
    this.sync();
  }

  /** chip / 键 / 命令面板：这一页开着 ⇒ 收起；否则开到这一页。 */
  toggle(page: DrawerPage): void {
    this.dock.toggle(page);
  }

  /** 开到这一页（开着就留着，不收起）：↗ 浮层的［在 cc-monitor 里打开］。 */
  open(page: DrawerPage): void {
    this.dock.show(page);
  }

  /** 各页知道自己此刻看不看得见（chip 的「开着」· 远端任务现问一次）。 */
  private sync(): void {
    this.tasks.setVisible(this.dock.current === "tasks");
    this.agents.setVisible(this.dock.current === "agents");
    this.terminal.setVisible(this.dock.current === "terminal");
  }

  private save(): void {
    safeSet(LS_KEYS.bottomDrawer, JSON.stringify(this.saved));
  }
}
