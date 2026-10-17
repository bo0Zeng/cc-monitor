/**
 * 设置窗 → 机器 → 「轮换」栏：那台的策略目录 · 新会话用哪份 · 那一份的设置与模拟。
 * 画的是会话面板同一处（`rot-mount.ts` 目标 `{strategy}`）：第一行选中的那份 ＝ 新会话用的，右边写「设置 · 那台」；
 * 改一格当场存成那份的新版（在用旧版的会话各自「载入新版」）。跟着「当前在看哪台机器」换；那台推来策略目录 / 会话变了 ⇒ 重读。
 * 判据：`tests/frontend/ui/settings/rot-section.vitest.ts`。
 */
import { machineName } from "../control-said";
import { bindEvents } from "../events";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "../ipc/origin";
import { copyText } from "../copy-table";
import { readRemoteConfig } from "../remote-config";
import { mountRotation, parseRotAnchor, type RotMount } from "../rot-mount";
import { getCurrentMachine, subscribeMachine } from "../machine-context";

/** 设置窗目的地带来的「看哪份 · 开哪个号」：栏还没读（或读的不是那台）⇒ 留着，下一次挂上时用。 */
let pending: { strategy: string; account: string | null } | null = null;

/** 目的地的锚点是轮换那一格（[`parseRotAnchor`]）⇒ 交给「轮换」栏，回 `true`。 */
export function focusRotation(anchor: string | undefined): boolean {
  const f = parseRotAnchor(anchor);
  if (!f) return false;
  pending = f;
  RotationSection.live?.takePending();
  return true;
}

export class RotationSection {
  /** 设置窗里那一栏（一扇设置窗一份）。 */
  static live: RotationSection | null = null;
  readonly element: HTMLElement;
  private mount: RotMount | null = null;
  private origin: Origin = LOCAL_ORIGIN;
  private loaded = false;
  private subscribed = false;

  constructor() {
    this.element = document.createElement("div");
    this.element.dataset.rotSection = "true";
    RotationSection.live = this;
    subscribeMachine((origin) => {
      if (!this.loaded || origin === this.origin) return;
      this.origin = origin;
      this.remount();
    });
  }

  /** 宿主在这一栏第一次可见时调（重开设置再调一次）。 */
  loadNow(): void {
    this.loaded = true;
    this.origin = getCurrentMachine();
    this.remount();
    void this.subscribe();
  }

  private remount(): void {
    const machine = isLocalOrigin(this.origin)
      ? copyText("remote.cards.local")
      : machineName(this.origin);
    this.mount = mountRotation({ origin: this.origin, sid: null, machine });
    this.element.replaceChildren(this.mount.root);
    if (!this.takePending()) void this.mount.refresh();
  }

  /** 有留着的「看哪份 · 开哪个号」就照它看（栏已挂上才用）。 */
  takePending(): boolean {
    if (!pending || !this.mount) return false;
    this.mount.focus(pending.strategy, pending.account);
    pending = null;
    return true;
  }

  private async subscribe(): Promise<void> {
    if (this.subscribed) return;
    this.subscribed = true;
    const hosts = await readRemoteConfig().then(
      (c) => c.hosts.map((h) => h.label),
      () => [] as string[],
    );
    const origins: Origin[] = [LOCAL_ORIGIN, ...hosts];
    try {
      await bindEvents(
        {
          onLine: () => {},
          onSessionFace: () => {},
          onChanged: (origin) => {
            if (origin === this.origin) void this.mount?.refresh();
          },
        },
        {
          changed: origins.flatMap((origin) =>
            (["rotation_strategies", "rotation"] as const).map((topic) => ({
              origin,
              topic,
            })),
          ),
        },
      );
    } catch (e) {
      console.warn("[rot] 订不上轮换推送：", e);
    }
  }
}
