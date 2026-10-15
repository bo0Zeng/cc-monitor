#!/usr/bin/env python3
"""手机图标字体子集化：从 Phosphor regular 全量字体里只留 `core-ui` 的 `Ph` 表用到的码位。

用法（src/mobile 下）：
    python3 scripts/phosphor-subset.py <Phosphor.ttf 全量字体>
全量字体取文件窗口那份依赖 egui-phosphor 自带的 `res/Phosphor.ttf`（cargo 注册表缓存里，版本照 src/frontend/shell/Cargo.lock）。
写 `core-ui/src/main/res/font/phosphor_regular.ttf`。要 fontTools（`pip install fonttools`）。
改了 `Ph` 表就重跑一次；表与字体两向相等由 app 的 `PhosphorGlyphsTest` 判。
"""

import re
import sys
from pathlib import Path

from fontTools import subset

HERE = Path(__file__).resolve().parents[1]
TABLE = HERE / "core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/icon/Ph.kt"
OUT = HERE / "core-ui/src/main/res/font/phosphor_regular.ttf"


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    cps = sorted({int(m, 16) for m in re.findall(r"\(0x([0-9A-Fa-f]{4,5})\)", TABLE.read_text(encoding="utf-8"))})
    if not cps:
        sys.exit("Ph.kt 里一个码位都没读出来")
    opts = subset.Options()
    opts.layout_features = []
    opts.name_IDs = ["*"]
    opts.notdef_outline = True
    opts.drop_tables += ["GSUB", "GPOS", "FFTM"]
    font = subset.load_font(sys.argv[1], opts)
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=cps)
    sub.subset(font)
    subset.save_font(font, str(OUT), opts)
    print("%d 个码位 ⇒ %s（%d 字节）" % (len(cps), OUT.relative_to(HERE), OUT.stat().st_size))


if __name__ == "__main__":
    main()
