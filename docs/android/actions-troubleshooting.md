# Android Actions 无法触发：fork 禁用状态

2026-10-31 已查明并恢复。本次并非 Android 工作流 YAML、签名、构建步骤或仓库普通 Actions 权限配置错误。

## 本次原因与恢复

权限 API 显示 `enabled=true`、工作流状态为 `active`，但请求运行仍返回 HTTP 422 `Actions has been disabled for this repository.`。这两个 API 状态不足以证明 fork 已允许实际运行。

打开仓库 [Actions 页面](https://github.com/cquptzf-glitch/vibe-astock/actions)，页面明确显示 **Workflows on this fork have been disabled**，并提供 **I understand my workflows, go ahead and re-enable them** 按钮。核对继承工作流及触发条件后重新启用，页面确认 **Actions Enabled**。不需要改签名、扩大 token 权限、切换仓库可见性或重建仓库。

随后以 `android-mvp` 和 `publish_preview=true` 重新请求 Android 工作流，GitHub 接受请求，创建 [Android CI #41184117989](https://github.com/cquptzf-glitch/vibe-astock/actions/runs/41184117989)，运行提交为 `28dd20d88863691e323633088cec94c02cfee290`（0.1.47）。恢复触发不等于构建或发布成功；最终结果应以该 run、签名回执和 Release 为准。

## 后续诊断顺序

1. 分清本机网络 EOF/超时与 GitHub 返回的 HTTP 错误；网络失败不能写成远端提交不符，也不能写成发布成功。
2. 核对远端分支 SHA、工作流文件和 `workflow_dispatch` 输入。
3. 同时检查普通权限 API、工作流状态与仓库 Actions 页面。出现本次 fork 提示时按页面恢复入口处理，不能只反复写入 `enabled=true`。
4. 页面若显示账号或平台单独限制、没有恢复入口，再按 GitHub 官方指引联系 Support；不要将这些状态与本次 fork 开关混为一谈。
5. 触发后记录 run ID、真实 head SHA、各 job 结论；核对 APK 的版本、包名、固定证书和 Release 下载文件，才能写成已经交付。

参考：[GitHub Actions 仓库设置](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository)、[禁用与启用工作流](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/disable-and-enable-workflows)。

## 恢复触发后发现的签名缓存缺失

0.1.47 后续确实通过 Rust、前端及全部手机页面检查，但停在固定签名检查：`android-preview-debug-keystore-v1` 查询结果为零，维护者也确认没有旧私钥备份。不能由 APK 或公开证书恢复私钥，也不能给原包名静默更换证书。

维护者已明确选择新包名并行安装。0.1.48 起 Android 预览使用 `src-tauri/tauri.preview.conf.json`，包名为 `com.vibeastock.preview`；原基础配置和旧公开签名记录保留。初始化与构建都必须传入 `--config src-tauri/tauri.preview.conf.json`，避免生成包名与编译包名不同。已有旧 `gen/android` 工程时需先移出生成目录并保留备份，再重新初始化。

新证书固定在 `docs/android/preview-signing-parallel.json`，私钥通过仓库加密 Secret `VIBE_PREVIEW_KEYSTORE_BASE64` 恢复。`prepare_preview_signing.py --parallel` 会先核对候选私钥的证书再替换 CI 临时文件；错包名、缺失或损坏 pin、错误证书、错误编码均拒绝，原有文件保持。私钥不再放入 Actions cache，不得提交 Git、打印日志或作为公开 artifact 上传。本机私钥备份在 ignored 的 `.local/signing/`，应另保留离线副本。

并行预览不是旧应用的覆盖升级。数据迁移采用旧应用导出业务备份、新应用导入的显式流程，不能宣称自动读取旧应用私有目录或已经完成真机两包并存验收。
