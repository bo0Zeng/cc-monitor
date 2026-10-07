/**
 * API 报错 / 重试的原因一词：后端给的种类（记录成品的 `apiReason`）⇒ 一个词。界面不认状态码、不读报错对象。
 */
import { copyText } from "../copy-table";
import type { ApiReason } from "../generated/ApiReason";

/** 原因一词（后端没给 ⇒ 原因不明）。 */
export function reasonWord(reason: ApiReason | undefined): string {
  switch (reason) {
    case "overloaded":
      return copyText("apiError.reason.overloaded");
    case "quota":
      return copyText("apiError.reason.quota");
    case "network":
      return copyText("apiError.reason.network");
    case "auth":
      return copyText("apiError.reason.auth");
    case "context":
      return copyText("apiError.reason.context");
    default:
      return copyText("apiError.reason.unknown");
  }
}
