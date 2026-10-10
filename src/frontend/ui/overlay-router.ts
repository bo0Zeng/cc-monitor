/**
 * 〔「一个 store，一个 router」〕**overlay 视图的路由**：历史 / 计划 / 网格 / 收件箱 / 驾驶舱
 * 开还是关，只经这一处（顶栏按钮 · 快捷键 · 命令面板同一口）。原先这几处判断散在 `main.ts` 里各写一遍。
 *
 * 语义逐字照旧：顶栏按钮与快捷键 = 开着就关、关着就开（`toggle`）；命令面板只开不关（`open`）。零 DOM、零 IPC。
 */

/** 一个 overlay 视图要有的三样（这几个视图本来就长这样）。 */
export interface OverlayView {
  isVisible(): boolean;
  open(): unknown;
  close(): void;
}

export type OverlayName = "history" | "plan" | "grid" | "cc-bus";

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
}
