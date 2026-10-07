/**
 * 本机「这台上的 cc-monitor」里那一格恢复命令（只管本机；留空 ＝ 通用页那一格）。远端那一格在机器卡里（存进机器表那台）。
 * 失焦 / 回车存；存失败 ⇒ 退回存过的值、行下一句。
 */
import { ccRow } from "./cc-row";
import { copyText } from "../copy-table";
import { getLocalResumeCommand, setLocalResumeCommand } from "../local-machine-prefs";

export function localResumeRow(): HTMLElement {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "settings-input machine-cc-input";
  input.placeholder = copyText("machineCard.field.resumeCmdHint");
  input.spellcheck = false;
  input.autocomplete = "off";
  input.disabled = true;
  input.dataset.role = "local-resume";
  const help = document.createElement("div");
  help.textContent = copyText("machineCard.field.resumeCmdScope", { machine: copyText("remote.cards.local") });
  const err = document.createElement("div");
  err.className = "settings-row-error";
  err.hidden = true;
  help.appendChild(err);
  let saved = "";
  void getLocalResumeCommand().then((v) => {
    saved = v;
    input.value = saved;
    input.disabled = false;
  });
  input.addEventListener("change", () => {
    const next = input.value.trim();
    err.hidden = true;
    setLocalResumeCommand(next).then(
      () => {
        saved = next;
        input.value = next;
      },
      (e: unknown) => {
        input.value = saved;
        err.textContent = copyText("settings.behavior.saveFailedLine", { why: String(e) });
        err.hidden = false;
      },
    );
  });
  const row = ccRow(copyText("machineCard.field.resumeCmd"), help, [input]);
  row.dataset.col = "resume";
  return row;
}
