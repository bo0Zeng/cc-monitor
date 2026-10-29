// 换号时按账号列表顺序。
// 在用账号未达用量上限时不换。
// 耗尽后启用的账号：其余账号均无可用额度、满启用延迟才启用，每次用到单次用量上限即停用。

/*--- rotation
{
  "format": 5,
  "settings": {
    "caps": {},
    "capKind": "soft",
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

// 权重恒为 0：换号时照账号列表顺序。
fn weight(a, left, mins) { 0.0 }
