/*--- rotation
{
  "format": 5,
  "name": "照列表先后",
  "line": "照号列表的先后，前一个用不了才换下一个",
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
        "zh": "一天里各时段用到多少就换下一个号；没另设的时段用满为止",
        "en": "How far to use each window in each part of the day before moving on"
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
      "choices": [
        {
          "v": "soft",
          "label": {
            "zh": "软上限：所有号都到上限以后还能接着用",
            "en": "Soft: may go past it once every account is at its limit"
          }
        },
        {
          "v": "hard",
          "label": {
            "zh": "硬上限：任何时候都不超过",
            "en": "Hard: never past it"
          }
        }
      ],
      "default": "soft"
    },
    {
      "key": "delay",
      "kind": "minutes",
      "scope": "each",
      "label": {
        "zh": "启用延迟",
        "en": "Start delay"
      },
      "help": {
        "zh": "排在前面的号全部用不了满这么多分钟后，才换到这个号",
        "en": "Minutes every account ahead must stay unusable before this one is used"
      },
      "min": 0,
      "max": 1440,
      "unit": "minutes",
      "default": 0
    },
    {
      "key": "back",
      "kind": "percent",
      "scope": "each",
      "nullable": {
        "zh": "不回切",
        "en": "Never switch back"
      },
      "label": {
        "zh": "回切阈值",
        "en": "Switch-back level"
      },
      "help": {
        "zh": "这个号余量回到这个数以上时，从排在后面的号切回来",
        "en": "Switch back to this account once its headroom is back above this"
      },
      "min": 0,
      "max": 100,
      "unit": "percent",
      "default": null
    }
  ]
}
---*/

// 所有号同分：照号列表的先后。上限 · 软硬 · 启用延迟 · 回切照设置（积木库里那一判）。
fn decide(ctx) {
  lib::decide_with(ctx, #{})
}
