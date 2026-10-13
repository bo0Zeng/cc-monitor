/*--- rotation
{
  "format": 5,
  "name": "",
  "line": {
    "zh": "余量多的账号权重高",
    "en": "Accounts with more headroom weigh more"
  },
  "settings": {
    "caps": {},
    "capKind": "soft",
    "close": 0,
    "force": null,
    "accounts": [
      {
        "start": true
      }
    ]
  },
  "form": [
    {
      "key": "caps",
      "kind": "daySegments",
      "scope": "each",
      "inherit": true,
      "label": {
        "zh": "用量上限",
        "en": "Usage limit"
      },
      "help": {
        "zh": "账号各窗口用量的上限，可按时段分段；未单独设置的账号取总体值；0% 表示该时段停用",
        "en": "Upper bound on each window's usage, by time of day; accounts without their own follow the overall value; 0% means unused in that period"
      },
      "windows": [
        "5h",
        "7d"
      ],
      "min": 0,
      "max": 100,
      "unit": "percent",
      "default": 100
    },
    {
      "key": "capKind",
      "kind": "choice",
      "scope": "each",
      "inherit": true,
      "label": {
        "zh": "上限类型",
        "en": "Limit type"
      },
      "help": {
        "zh": "软上限：所有账号均达用量上限后，可继续使用至额度用尽。硬上限：任何时候不超过用量上限",
        "en": "Soft: once every account is at its usage limit, use may continue until quota runs out. Hard: never past the usage limit"
      },
      "choices": [
        {
          "v": "soft",
          "label": {
            "zh": "软上限",
            "en": "Soft"
          }
        },
        {
          "v": "hard",
          "label": {
            "zh": "硬上限",
            "en": "Hard"
          }
        }
      ],
      "default": "soft"
    },
    {
      "key": "close",
      "kind": "percent",
      "scope": "all",
      "min": 0,
      "max": 100,
      "default": 0,
      "label": {
        "zh": "权重容差",
        "en": "Weight tolerance"
      },
      "help": {
        "zh": "权重 ＝ 余量（两窗口用量上限减用量取小）；按量账号为 0。较高权重超出较低权重不到这一比例时，两账号视为同权，按账号列表顺序排序",
        "en": "Weight = headroom (the smaller of usage limit minus usage across the two windows); 0 for API-key accounts. When the higher weight exceeds the lower by less than this ratio, the two accounts count as equal and follow the account list order"
      }
    },
    {
      "key": "force",
      "kind": "percent",
      "scope": "all",
      "min": 0,
      "max": 100,
      "default": null,
      "nullable": {
        "zh": "不强制切换",
        "en": "Never"
      },
      "label": {
        "zh": "强制切换阈值",
        "en": "Forced switch threshold"
      },
      "help": {
        "zh": "在用账号未达用量上限时，重置时间早于它、且权重超出它这一比例的账号立即切入",
        "en": "While the account in use is under its usage limit, an account that resets earlier and whose weight exceeds it by this ratio takes over at once"
      }
    },
    {
      "key": "fallback",
      "kind": "toggle",
      "scope": "each",
      "default": false,
      "label": {
        "zh": "耗尽后启用",
        "en": "Use when others run out"
      },
      "help": {
        "zh": "其余账号均无可用额度时才启用；其余账号恢复额度即切出",
        "en": "Used only when every other account is out of quota; left as soon as another recovers"
      }
    },
    {
      "key": "delay",
      "kind": "minutes",
      "scope": "each",
      "min": 0,
      "max": 1440,
      "unit": "minutes",
      "default": 0,
      "label": {
        "zh": "启用延迟",
        "en": "Start delay"
      },
      "help": {
        "zh": "其余账号均无可用额度持续这一时长后才启用；仅对耗尽后启用的账号生效",
        "en": "Used only after every other account has been out of quota this long; applies only to accounts used when others run out"
      }
    },
    {
      "key": "stint",
      "kind": "percent",
      "scope": "each",
      "min": 0,
      "max": 100,
      "default": null,
      "nullable": {
        "zh": "不限",
        "en": "No limit"
      },
      "label": {
        "zh": "单次用量上限",
        "en": "Per-use limit"
      },
      "help": {
        "zh": "每次启用后，5 小时用量增幅达到这一比例即停用，直到其余账号恢复额度；仅对耗尽后启用的账号生效",
        "en": "After each switch to it, stops once its 5-hour usage has risen by this much, until another account recovers; applies only to accounts used when others run out"
      }
    }
  ]
}
---*/

// 权重：余量（两窗取小）；按量号 ⇒ 0。
fn weight(a, left, mins) {
  if a.kind == "api" { 0.0 } else { left }
}
