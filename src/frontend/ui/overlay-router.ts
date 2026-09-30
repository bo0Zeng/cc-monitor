/**
 * 〔GAP1 · `设计/01 §1.5`「一个 store，一个 router」〕**overlay 视图的路由**：历史 / 全景 / 网格 / 收件箱 / 驾驶舱
 * 开还是关，只经这一处（顶栏按钮 · 快捷键 · 命令面板同一口）。原先这几处判断散在 `main.ts` 里各写一遍。
 *
 * 语义逐字照旧：顶栏按钮与快捷键 = 开着就关、关着就开（`toggle`）；命令面板只开不关（`open`）；
 * 「开到这一个上」（全景高亮那条要等它开好）= 不看开没开、照开一次（`show`）。零 DOM、零 IPC。
 */

/** 一个 overlay 视图要有的三样（这五个视图本来就长这样）。 */
export interface OverlayView {
  isVisible(): boolean;
  open(): unknown;
  close(): void;
}

export type OverlayName = "history" | "panorama" | "grid" | "cc-bus";

export class OverlayRouter {
  private readonly views = new Map<OverlayName, OverlayView>();

  register(name: OverlayName, view: OverlayView): void {
    if (this.views.has(name)) throw new Error(`overlay registered twice: ${name}`); // 我们自己代码的契约错
    this.views.set(name, view);
  }

  private view(name: OverlayName): OverlayView {
    const v = this.views.get(name);
    if (!v) throw new Error(`overlay not registered: ${name}`);
    return v;
  }

  /** 开着 ⇒ 关；关着 ⇒ 开。 */
  toggle(name: OverlayName): void {
    const v = this.view(name);
    if (v.isVisible()) v.close();
    else void v.open();
  }

  /** 只开不关（已经开着 ⇒ 不动）。 */
  open(name: OverlayName): void {
    const v = this.view(name);
    if (!v.isVisible()) void v.open();
  }

  /** 照开一次并等它开好（视图自己决定开着时是复用还是重载）。 */
  async show(name: OverlayName): Promise<void> {
    await this.view(name).open();
  }
}
