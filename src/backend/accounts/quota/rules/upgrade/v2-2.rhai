/*--- rotation
{
  "format": 5,
  "name": "",
  "line": "同一排里先用快要重置的号",
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
      "key": "row",
      "kind": "row",
      "scope": "each",
      "label": {
        "zh": "排",
        "en": "Row"
      },
      "help": {
        "zh": "同一排里用满一个才换下一个；上面一排的号一有额度就换上去；下面一排只在上面所有排都没额度时才用",
        "en": "Within a row the next account is used only once the one in use runs out; an account in a higher row takes over as soon as it has quota; a lower row is used only when every row above is out"
      }
    },
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
        "zh": "一天里各时段，用到多少就算这个号没额度了；没另设的号跟总体",
        "en": "How far each window may be used in each part of the day; accounts without their own follow the overall one"
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
            "zh": "软上限：所有号都到上限以后，还能接着用到用满",
            "en": "Soft: once every account is at its limit, may go on until it is used up"
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
        "zh": "上面的排全都没额度满这么多分钟，才换到这个号",
        "en": "Minutes every row above must stay out of quota before this account is used"
      },
      "min": 0,
      "max": 1440,
      "unit": "minutes",
      "default": 0
    }
  ]
}
---*/

// 同一排里 5 小时窗口离重置越近越先；窗口没开的、按量号排后面（同分照号牌左右）。
// 先后只在某个号重置时变，后端本来就在重置时来问，不用另约时刻。
fn decide(ctx) {
  let scores = #{};
  for a in ctx.accounts {
    let r = a.resets.w5h;
    if a.kind == "api" || r == () { continue; }
    scores[a.id] = 100000 - (r - ctx.now) / 60;
  }
  lib::by_rows(ctx, scores)
}
