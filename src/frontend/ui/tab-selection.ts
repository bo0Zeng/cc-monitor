/**
 * **tab 栏的多选**：选中了哪几个 ＋ 连选的锚点。纯界面状态，只住 monitor，不落盘。
 *
 * - 单击：清掉多选，锚点落在它（切 tab 是宿主的事）。
 * - Ctrl 单击：把它加进 / 移出多选，不切 tab；多选还是空的时候，当前 tab 也算在里面。
 * - Shift 单击：从锚点到它，按条上看得到的顺序连选（顺序由调用方给）。
 * - Esc：清掉多选 —— 有选中时在快捷键表的 overlay 栈上压一层，清空时弹掉（菜单 / 对话框在它上面就先关它们）。
 * - tab 没了就从多选里掉出去（`retain`，整刷那一拍调）。
 */
import { dispatcher, type OverlayHandle } from "./keybindings/registry";
import { tabContextMenuOpen } from "./tab-context-menu";

export class TabSelection {
  private readonly picked = new Set<string>();
  private anchor: string | null = null;
  private readonly overlay: OverlayHandle = {
    passes: "all", // 多选不盖住 tab：数字键 / ] [ 照常
    handleEsc: () => {
      // 菜单开着 ⇒ 这一下 Esc 是关菜单的（菜单自己听着），多选留着。
      if (tabContextMenuOpen()) return false;
      this.clear();
      this.changed();
      return true;
    },
  };
  private stacked = false;

  /** `changed` = 选中集合变了之后要做的事（宿主重画 tab 栏）。 */
  constructor(private readonly changed: () => void) {}

  has(sid: string): boolean {
    return this.picked.has(sid);
  }

  get size(): number {
    return this.picked.size;
  }

  /** 选中的那几个，按 `order` 的先后。 */
  inOrder(order: readonly string[]): string[] {
    return order.filter((sid) => this.picked.has(sid));
  }

  /** 单击：清掉多选，锚点落在它。 */
  plain(sid: string): void {
    this.picked.clear();
    this.anchor = sid;
    this.sync();
  }

  /** Ctrl 单击。`active` = 当前 tab（多选还空着的时候它也算进来）。 */
  toggle(sid: string, active: string | null): void {
    if (this.picked.size === 0 && active !== null && active !== sid) this.picked.add(active);
    if (this.picked.has(sid)) this.picked.delete(sid);
    else this.picked.add(sid);
    this.anchor = sid;
    this.sync();
  }

  /** Shift 单击：锚点到它这一段（`order` = 条上看得到的顺序）。没有锚点 / 锚点不在条上 ⇒ 以当前 tab 为锚。 */
  range(sid: string, order: readonly string[], active: string | null): void {
    const from = this.anchor !== null && order.includes(this.anchor) ? this.anchor : active;
    const a = from === null ? -1 : order.indexOf(from);
    const b = order.indexOf(sid);
    this.picked.clear();
    if (b < 0) {
      this.sync();
      return;
    }
    const [lo, hi] = a < 0 ? [b, b] : [Math.min(a, b), Math.max(a, b)];
    for (const s of order.slice(lo, hi + 1)) this.picked.add(s);
    if (a < 0) this.anchor = sid;
    this.sync();
  }

  clear(): void {
    this.picked.clear();
    this.sync();
  }

  /** 整刷那一拍：不在了的 tab 掉出去（锚点同理）。不回调 `changed`（调用方正在画）。 */
  retain(alive: (sid: string) => boolean): void {
    for (const sid of Array.from(this.picked)) if (!alive(sid)) this.picked.delete(sid);
    if (this.anchor !== null && !alive(this.anchor)) this.anchor = null;
    this.sync();
  }

  /** 有选中 ⇔ Esc 那一层在栈上。 */
  private sync(): void {
    const want = this.picked.size > 0;
    if (want === this.stacked) return;
    this.stacked = want;
    if (want) dispatcher.pushOverlay(this.overlay);
    else dispatcher.popOverlay(this.overlay);
  }
}
