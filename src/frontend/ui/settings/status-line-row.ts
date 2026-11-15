/**
 * 每台「这台上的 cc-monitor」里「状态行」那一格：托管终端（`ccm --attach`）最底那一行照的模板。值住**那台**后端（`prefs.json` 的
 * `statusLine`），界面不存副本；字段表 · 写成那一行 · 缺省模板都住后端（`host/managed/status.rs`），这里只照抄。
 *
 * - 输入框是此刻照用的那一条（没设 ⇒ 随包的缺省）；敲字时问那台 `status-line-preview` 现算预览（那一行由后端写好）、
 *   认不出的键那一句照回包；离开那一格（`change`）存（`prefs-set {statusLine}`，空串 ⇒ 不留那一行）。
 * - 「恢复缺省」发 `prefs-set {statusLine: null}`。
 * - 读不懂（那台 `state: "unreadable"`）⇒ 那一格禁用、说那台那一句；问不到 ⇒ 说通道那一句。
 */
import { budgetWithin, callTyped } from "../ipc/chan-caller";
import { copyText } from "../copy-table";
import { readPrefs, setPrefs } from "../machine-prefs";
import { detailOf, sayWithDetail, saidOf } from "../kit/detail";
import { button } from "../kit/button";
import { ccRow } from "./cc-row";
import type { Machine } from "../ipc/origin";
import type { StatusLinePreview } from "../generated/StatusLinePreview";

/** 预览那一问的期限：那台读几份小文件 ＋ 回程。 */
const PREVIEW_BUDGET_MS = 10_000;
/** 敲字停多久才问预览。 */
const PREVIEW_DEBOUNCE_MS = 250;

/** 问那台照 `template`（`null` ⇒ 随包的缺省）预览。通道不通 ⇒ 抛。 */
export async function previewStatusLine(origin: Machine, template: string | null): Promise<StatusLinePreview> {
  return callTyped(origin, "status-line-preview", { template }, budgetWithin(PREVIEW_BUDGET_MS));
}

/** 那台那一格。建好不问；`load()` 才问那台（机器页第一次可见时宿主调）。 */
export function statusLineRow(origin: Machine): { row: HTMLElement; load: () => void } {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "settings-input settings-input-mono";
  input.spellcheck = false;
  input.setAttribute("aria-label", copyText("settingsPanel.statusLine.label"));
  input.disabled = true;
  const reset = button({
    label: copyText("settingsPanel.statusLine.reset"),
    size: "compact",
    onClick: () => void save(null),
  });
  reset.disabled = true;

  const help = document.createElement("div");
  const intro = document.createElement("div");
  intro.textContent = copyText("settingsPanel.statusLine.help");
  // 那一行由后端按列写好（空格对齐左右两段）⇒ 原样等宽摆，不折行。
  const preview = document.createElement("pre");
  preview.className = "settings-hint";
  preview.hidden = true;
  const from = document.createElement("div");
  from.className = "settings-hint";
  const unknown = document.createElement("div");
  unknown.className = "settings-row-error";
  unknown.hidden = true;
  const fields = document.createElement("details");
  const summary = document.createElement("summary");
  summary.textContent = copyText("settingsPanel.statusLine.fields");
  const list = document.createElement("dl");
  fields.append(summary, list);
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;
  help.append(intro, preview, from, unknown, fields, err);
  const row = ccRow(copyText("settingsPanel.statusLine.label"), help, [input, reset]);

  /** 存着的那一格（`null` ⇒ 没设，照缺省）。 */
  let saved: string | null = null;
  /** 那台回的缺省模板（没设时输入框照它）。 */
  let fallback = "";
  let asked = 0;

  const fail = (e: unknown): void => {
    sayWithDetail(err, saidOf(e), detailOf(e));
    err.hidden = false;
  };

  /** 照回包摆：那一行 · 照哪一个算的 · 认不出的那一句 · 字段表（只摆一次）。 */
  const show = (p: StatusLinePreview): void => {
    preview.hidden = p.line === null;
    preview.textContent = p.line ?? "";
    from.textContent = p.from;
    unknown.hidden = p.unknownSaid === null;
    unknown.textContent = p.unknownSaid ?? "";
    fallback = p.defaultTemplate;
    if (list.childElementCount === 0) {
      for (const f of p.fields) {
        const dt = document.createElement("dt");
        dt.className = "settings-input-mono";
        dt.textContent = f.key;
        const dd = document.createElement("dd");
        dd.textContent = f.says;
        list.append(dt, dd);
      }
    }
  };

  /** 问预览（只认最后一问的回包）。 */
  const ask = async (template: string | null): Promise<boolean> => {
    const n = ++asked;
    try {
      const p = await previewStatusLine(origin, template);
      if (n === asked) show(p);
      return true;
    } catch (e) {
      if (n === asked) fail(e);
      return false;
    }
  };

  let timer: ReturnType<typeof setTimeout> | undefined;
  input.addEventListener("input", () => {
    clearTimeout(timer);
    // 调度：合批 —— 敲字停下才问那台一次预览，连着敲只算最后一次
    timer = setTimeout(() => void ask(input.value), PREVIEW_DEBOUNCE_MS);
  });

  /** 存一回（`null` ⇒ 回到缺省），照读回的那一份重摆。 */
  const save = async (next: string | null): Promise<void> => {
    if (next !== null && next === (saved ?? fallback)) return;
    input.disabled = true;
    try {
      const r = await setPrefs(origin, { statusLine: next });
      saved = r.prefs.statusLine ?? null;
      err.hidden = true;
      await ask(saved);
      input.value = saved ?? fallback;
    } catch (e) {
      fail(e);
    } finally {
      input.disabled = false;
    }
  };
  input.addEventListener("change", () => {
    clearTimeout(timer);
    void save(input.value);
  });

  const load = (): void => {
    void readPrefs(origin).then(
      async (r) => {
        saved = r.prefs.statusLine ?? null;
        if (r.state === "unreadable") {
          sayWithDetail(err, r.reason ?? "", r.detail ?? "");
          err.hidden = false;
          return;
        }
        await ask(saved);
        input.value = saved ?? fallback;
        input.disabled = false;
        reset.disabled = false;
      },
      (e: unknown) => fail(e),
    );
  };
  return { row, load };
}
