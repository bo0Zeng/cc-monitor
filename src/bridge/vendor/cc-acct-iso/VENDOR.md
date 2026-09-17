# vendored cc-acct-iso

F5：cc-monitor 一键部署多账号管线（`deploy_remote_acct_iso`）需要把 cc-acct-iso 脚本内嵌进构建，
故 vendor 一份到这里（`acct_iso_deploy.rs` 用 `include_bytes!` 内嵌）。cc-acct-iso 是一套 bash 脚本
skill（非二进制），vendor 成本低。

- 上游仓: `~/.claude/skills/cc-acct-iso`
- vendored 指纹: 见同目录 `.vendor_id`（= **全部 6 个被部署文件**内容 sha256 前 16 位，见下菜谱的固定顺序）

D 审计（F5）：指纹**必须覆盖全部被部署的文件**（不只 3 个脚本）——否则上游只改 run-tests.sh/SKILL.md/config 时 `.vendor_id` 不变，skip-if-current 会漏更新、远端拿到陈旧辅助文件。

## re-vendor 菜谱（上游改了 cc-acct-iso 后同步过来）

```bash
SK=~/.claude/skills/cc-acct-iso
DEST=src-tauri/vendor/cc-acct-iso      # 从 cc-monitor 仓根跑
cp -a "$SK/scripts/cc-acct-iso" "$SK/scripts/lib.sh" "$SK/scripts/cc-acct-iso-install.sh" "$DEST/scripts/"
cp -a "$SK/scripts/test/run-tests.sh" "$DEST/scripts/test/"
cp -a "$SK/SKILL.md" "$DEST/"
cp -a "$SK/examples/config" "$DEST/examples/"
# 指纹顺序固定（build.rs 自洽校验按同一顺序）：3 脚本 + test + SKILL + config
(cd "$DEST" && cat scripts/cc-acct-iso scripts/lib.sh scripts/cc-acct-iso-install.sh \
  scripts/test/run-tests.sh SKILL.md examples/config | sha256sum | cut -c1-16 > .vendor_id)
```

改了 `.vendor_id` 后，远端 marker 比对会判「版本不符」→ 下次点部署会重推（skip-if-current 生效）。

## 过期检测

`build.rs::check_acct_iso_vendor_freshness` 会在上游仓存在时，比对上游三脚本与 vendored 副本，
不一致则 `cargo:warning`（软警告，同 code-picture-core 的 vendor 过期检查）。上游缺席 → no-op。
