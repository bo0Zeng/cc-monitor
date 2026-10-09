/**
 * 复制详情：一条报错旁边那颗［复制详情］。全产品只这一处；toast · 错误条 · 行内 · 浮层 · 对话框 都用它。
 *
 * - 详情（`detail`）由出错的那一端写好随帧回（后端 · 壳 · 文件窗口进程），这里只排版：复制出去的首行是屏上那一句，下面原样接详情。
 * - 详情是空的 ⇒ 不出按钮（本地校验这类）。
 * - 点了：写剪贴板 ⇒「已复制」＋ 对勾 1.5 s 回默认（连点重计，不叠）；读屏念「已复制详情」；不收起所在的 toast / 浮层 / 对话框。
 * - 写不进：「复制失败」，就地展开一块只读原文、全选、焦点进去；Esc 或点别处收起（与日志页「复制诊断信息」同一做法）。
 * - 窄：只剩图标，悬停与读屏名仍是「复制详情」。
 */
import { button } from "./button";
import { icon } from "./icon";
import { copyText } from "../copy-table";
import s from "./detail.module.css";

/** 「已复制」停多久。 */
export const COPIED_MS = 1500;

/** 一次失败带的复制详情：只认 `Error` 上的字符串 `detail`（通道失败 · 壳命令失败 · 控制动作失败都这样带）；别的一律空。 */
export function detailOf(e: unknown): string {
  if (!(e instanceof Error)) return "";
  const d = (e as { detail?: unknown }).detail;
  return typeof d === "string" ? d : "";
}

/**
 * 一次失败给人看的那一句。出错那一端写好的（`Error` 上带字符串 `detail` 的那几形：通道 · 壳命令 · 控制动作）⇒ 它的 `message`；
 * 别的（JS 自己抛的 · 形状不认）⇒ 界面那句 `title`（哪件事没成），原文进控制台、不上屏。toast 与行内都经这里定那一句。
 */
export function failSaid(title: string, e: unknown): string {
  const said = writtenSaid(e);
  if (said !== null) return said;
  console.warn(title, e);
  return title;
}

/** 出错那一端写好的那一句（[`failSaid`] 的判法）；JS 自己抛的 · 形状不认 ⇒ `null`。主语只有界面知道时（「哪一项设置」），界面把它放句首、这一句跟在后面。 */
export function writtenSaid(e: unknown): string | null {
  return e instanceof Error && typeof (e as { detail?: unknown }).detail === "string" && e.message.trim() !== "" ? e.message : null;
}

/** 行内那一句报错（一块里的状态行 · 一行下面那句红字）：[`failSaid`] 定那一句，有详情 ⇒ 后面跟［复制详情］。 */
export function sayFailure(el: HTMLElement, title: string, e: unknown): void {
  sayWithDetail(el, failSaid(title, e), detailOf(e));
}

/**
 * 复制出去的那一段：一条 ⇒「那句 ＋ 详情」；同一句合流 ×N ⇒ 首行带 ×N，下面每段一份详情，段间空一行。
 * 空详情那段只剩那句（不出空行）。
 */
export function detailBody(segments: readonly (readonly [said: string, detail: string])[]): string {
  if (segments.length === 0) return "";
  const joinOne = ([said, detail]: readonly [string, string]): string => (detail.trim() === "" ? said : `${said}\n${detail}`);
  if (segments.length === 1) return joinOne(segments[0]);
  const head = `${segments[0][0]} ${copyText("kit.toast.count", { n: segments.length })}`;
  return [head, ...segments.map(([, d]) => d).filter((d) => d.trim() !== "")].join("\n\n");
}

/** 平台的复制键位（说明那一行用）。 */
function copyKey(): string {
  return /Mac/i.test(navigator.platform) ? "⌘C" : "Ctrl+C";
}

/**
 * 行内那一句（一行下面那句红字 · 一块里的状态行）：写进那一句，有详情 ⇒ 句子后面直接跟［复制详情］。
 * 换掉 `el` 里原有的全部内容；复制不了时那块原文展开在 `el` 里。
 */
export function sayWithDetail(el: HTMLElement, said: string, detail: string): void {
  el.textContent = said;
  const copy = copyDetailButton(said, detail);
  if (copy) {
    el.dataset.detailHost = "";
    el.append(" ", copy);
  } else delete el.dataset.detailHost;
}

export interface CopyDetailOptions {
  /** 窄：只剩图标。 */
  iconOnly?: boolean;
}

/** 离 `el` 最近的那个能竖着滚的祖先。 */
function scrollerOf(el: HTMLElement): HTMLElement | null {
  for (let e = el.parentElement; e; e = e.parentElement) {
    const y = getComputedStyle(e).overflowY;
    if (y === "auto" || y === "scroll") return e;
  }
  return null;
}

/**
 * 就地展开之后滚那一块所在的滚动区：出错那一块（条 / 行）的顶留在上沿之内，展开的原文框在此前提下尽量露全。
 * 只动最近那一个滚动区，不连带外层。
 */
function keepInView(host: HTMLElement, box: HTMLElement): void {
  const sc = scrollerOf(host);
  if (!sc) return;
  const view = sc.getBoundingClientRect();
  const top = host.getBoundingClientRect().top;
  const bottom = box.getBoundingClientRect().bottom;
  if (top < view.top) sc.scrollTop -= view.top - top;
  else if (bottom > view.bottom) sc.scrollTop += Math.min(bottom - view.bottom, top - view.top);
}

/**
 * ［复制详情］。`detail` 空 ⇒ `null`（调用方不放）。`said` 是屏上那一句；也可以给一个函数（合流 ×N 那种，点的那一刻才定）。
 * 返回的是一个外壳（按钮 ＋ 读屏那一格 ＋ 复制不了时的那块原文）。
 */
export function copyDetailButton(said: string, detail: string, opts?: CopyDetailOptions): HTMLElement | null;
export function copyDetailButton(body: () => string, detail: string, opts?: CopyDetailOptions): HTMLElement | null;
export function copyDetailButton(said: string | (() => string), detail: string, opts: CopyDetailOptions = {}): HTMLElement | null {
  if (detail.trim() === "") return null;
  const text = typeof said === "function" ? said : (): string => detailBody([[said, detail]]);
  const wrap = document.createElement("span");
  wrap.className = s.detailCopy;
  wrap.dataset.part = "copy-detail";
  const live = document.createElement("span");
  live.className = s.detailLive;
  live.setAttribute("aria-live", "polite");
  let timer: ReturnType<typeof setTimeout> | null = null;
  let fallback: HTMLElement | null = null;

  const name = copyText("detail.act.copy");
  const b = button({
    label: name,
    kind: opts.iconOnly ? "icon" : "ghost",
    size: "compact",
    icon: "copy",
    hint: opts.iconOnly ? name : undefined,
    onClick: (ev) => {
      ev.stopPropagation();
      void copy();
    },
  });

  const paint = (state: "idle" | "copied" | "failed"): void => {
    b.dataset.state = state;
    const ic = icon(state === "copied" ? "check" : state === "failed" ? "error" : "copy", "compact");
    const word =
      state === "copied" ? copyText("detail.act.copied") : state === "failed" ? copyText("detail.act.failed") : name;
    if (opts.iconOnly) {
      b.replaceChildren(ic);
      return;
    }
    const span = document.createElement("span");
    span.textContent = word;
    b.replaceChildren(ic, span);
  };

  const closeFallback = (): void => {
    fallback?.remove();
    fallback = null;
  };

  const showFallback = (body: string): void => {
    closeFallback();
    const box = document.createElement("div");
    box.className = s.detailFallback;
    const hint = document.createElement("div");
    hint.className = s.detailHint;
    hint.textContent = copyText("detail.fallback.hint", { copyKey: copyKey() });
    const area = document.createElement("textarea");
    area.className = s.detailText;
    area.readOnly = true;
    area.value = body;
    // 高按行数给（限高 160 px 由样式管，超了可滚）。
    area.rows = Math.min(body.split("\n").length, 8);
    area.addEventListener("keydown", (ev) => {
      if (ev.key === "Escape") {
        ev.stopPropagation();
        closeFallback();
        b.focus();
      }
    });
    // 调度：一次性 —— 焦点离开那块原文后下一拍收起（让点到别处的那一下先落地）
    area.addEventListener("blur", () => setTimeout(closeFallback, 0));
    box.append(hint, area);
    // 原文放在「宿主」（调用方标的那一块，没标 ⇒ 按钮所在那一行）的末尾，不挤在按钮行里。
    const host = wrap.closest<HTMLElement>("[data-detail-host]") ?? wrap.parentElement ?? wrap;
    host.appendChild(box);
    fallback = box;
    // 焦点不自己滚（浏览器会把框底对齐、把出错那一块滚出视野）：由 `keepInView` 对准那一块。
    area.focus({ preventScroll: true });
    area.select();
    // 全选会把框滚到底：滚回顶，首行看得全。
    area.scrollTop = 0;
    keepInView(host, box);
  };

  const copy = async (): Promise<void> => {
    if (b.style.minWidth === "" && b.offsetWidth > 0) b.style.minWidth = `${b.offsetWidth}px`;
    const body = text();
    if (timer !== null) clearTimeout(timer);
    try {
      if (!navigator.clipboard) throw new Error("no clipboard");
      await navigator.clipboard.writeText(body);
      closeFallback();
      paint("copied");
      live.textContent = copyText("detail.aria.copied");
    } catch {
      paint("failed");
      live.textContent = "";
      showFallback(body);
    }
    // 调度：一次性 —— 「已复制 / 复制失败」停 1.5 s 回默认，再点一次重计
    timer = setTimeout(() => {
      timer = null;
      paint("idle");
      live.textContent = "";
    }, COPIED_MS);
  };

  paint("idle");
  wrap.append(b, live);
  return wrap;
}
