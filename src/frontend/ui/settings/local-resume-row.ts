/**
 * 本机「这台上的 cc-monitor」里那一格恢复命令（只管本机；「通用设置」＝ 跟通用页那一格）。远端那一格在机器卡里（存进机器表那台）。
 * 下拉（候选同通用页那一格）；选了就存；存失败 ⇒ 退回存过的值、行下一句。
 */
import { ccRow } from "./cc-row";
import { copyText } from "../copy-table";
import { getLocalResumeCommand, setLocalResumeCommand } from "../local-machine-prefs";
import { getBehavior } from "../behavior";
import { ResumeSelect } from "./resume-select";
import { detailOf, sayWithDetail } from "../kit/detail";

export function localResumeRow(): HTMLElement {
  const help = document.createElement("div");
  help.textContent = copyText("machineCard.field.resumeCmdScope", { machine: copyText("remote.cards.local") });
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;
  help.appendChild(err);
  let saved = "";
  let presets: string[] = [];
  const sel = new ResumeSelect({
    inherit: true,
    onChange: (next) => {
      err.hidden = true;
      setLocalResumeCommand(next).then(
        () => {
          saved = next;
        },
        (e: unknown) => {
          sel.set(saved, presets);
          sayWithDetail(err, copyText("settings.behavior.saveFailedLine", { why: String(e) }), detailOf(e));
          err.hidden = false;
        },
      );
    },
  });
  sel.disabled = true;
  sel.element.dataset.role = "local-resume";
  void Promise.all([getLocalResumeCommand(), getBehavior().catch(() => null)]).then(([v, b]) => {
    saved = v;
    presets = b?.resumeCommandPresets ?? [];
    sel.set(saved, presets);
    sel.disabled = false;
  });
  const row = ccRow(copyText("machineCard.field.resumeCmd"), help, [sel.element]);
  row.dataset.col = "resume";
  return row;
}
