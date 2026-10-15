/**
 * 每台「这台上的 cc-monitor」里「pb 路径」那一行：planned-build 的插件根目录。值住**那台**后端（`prefs.json` 的 `pbDir`），
 * 界面不存副本。设了 ⇒ 那台的计划那一族只认它；留空 ⇒ 认那台装着的插件。
 *
 * 离开那一格（`change`）就办：空 ⇒ 删掉那一格；非空 ⇒ 先问那台这个路径能不能用（`plan-pb-check`），能用才存，
 * 行下一句是那台写好的话（能用 · 不存在 · 下面没有 · 跑不起来）。读不懂（那台 `state: "unreadable"`）⇒ 那一格禁用、说那台那一句。
 */
import { budgetWithin, callTyped, saidFrom } from "../ipc/chan-caller";
import { copyText } from "../copy-table";
import { readPrefs, setPrefs } from "../machine-prefs";
import { detailOf, sayWithDetail } from "../kit/detail";
import { PB_DIR_ANCHOR } from "../settings-dest";
import { ccRow } from "./cc-row";
import type { Origin } from "../ipc/origin";

/** 校验那一问的期限：那台起一次 pb（后端给 pb 20 秒）＋ 回程。 */
const CHECK_BUDGET_MS = 25_000;

/** 那台那一行。建好不问；`load()` 才问那台（机器页第一次可见时宿主调）。 */
export function pbDirRow(origin: Origin): { row: HTMLElement; load: () => void } {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "settings-input settings-input-mono";
  input.spellcheck = false;
  input.placeholder = copyText("settingsPanel.pbDir.placeholder");
  input.setAttribute("aria-label", copyText("settingsPanel.pbDir.label"));
  input.disabled = true;
  const help = document.createElement("div");
  const intro = document.createElement("div");
  intro.textContent = copyText("settingsPanel.pbDir.help");
  const ok = document.createElement("div");
  ok.className = "settings-hint";
  ok.hidden = true;
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;
  help.append(intro, ok, err);
  const row = ccRow(copyText("settingsPanel.pbDir.label"), help, [input]);
  row.dataset.anchor = PB_DIR_ANCHOR;
  let saved = "";

  const say = (text: string, bad: boolean, detail = ""): void => {
    sayWithDetail(bad ? err : ok, text, detail);
    err.hidden = !bad;
    ok.hidden = bad;
  };

  const save = async (): Promise<void> => {
    const next = input.value.trim();
    if (next === saved) return;
    input.disabled = true;
    try {
      if (next !== "") {
        const c = await callTyped(origin, "plan-pb-check", { dir: next }, budgetWithin(CHECK_BUDGET_MS));
        if (c.state !== "ok") {
          input.setAttribute("aria-invalid", "true");
          say(c.said, true);
          return;
        }
        await setPrefs(origin, { pbDir: next });
        say(c.said, false);
      } else {
        await setPrefs(origin, { pbDir: "" });
        say(copyText("settingsPanel.pbDir.cleared"), false);
      }
      saved = next;
      input.removeAttribute("aria-invalid");
    } catch (e) {
      say(copyText("settingsPanel.pbDir.saveFailed", { why: saidFrom(e, origin) }), true, detailOf(e));
    } finally {
      input.disabled = false;
    }
  };
  input.addEventListener("change", () => void save());

  const load = (): void => {
    void readPrefs(origin).then(
      (r) => {
        saved = r.prefs.pbDir ?? "";
        input.value = saved;
        if (r.state === "unreadable") {
          say(r.reason ?? copyText("machinePrefs.read.failed"), true, r.detail ?? "");
          return;
        }
        input.disabled = false;
      },
      (e: unknown) => say(copyText("machinePrefs.read.failed"), true, detailOf(e)),
    );
  };
  return { row, load };
}
