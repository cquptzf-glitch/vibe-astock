# 仓库访问量趋势

README 中的组件读取 GitHub 官方 Traffic API，展示最近 30 日的每日浏览量（PV）和每日独立访客，以及 API 返回的近 14 天浏览、独立访客、克隆和独立克隆者汇总。

## 启用

1. 在 GitHub 创建 fine-grained personal access token，只选择本仓库，授予 **Repository permissions → Administration: Read-only**。此令牌只用于读取访问统计，无需 Contents 写权限。
2. 打开本仓库 **Settings → Secrets and variables → Actions → New repository secret**，名称填 `TRAFFIC_TOKEN`，值填该令牌。不要把令牌放进 README、Issue、代码、日志或聊天。
3. 打开 **Actions → Repository traffic → Run workflow**，选择 `main` 运行。
4. 成功后 `traffic-data` 分支的 `traffic.svg` 和 `traffic.json` 自动更新，README 展示真实曲线。GitHub 图片缓存可能使展示稍有延迟。

未配置令牌时，组件明确显示“等待 GitHub 访问数据”，工作流报告未采集，不显示模拟访问量。普通 `GITHUB_TOKEN` 不具有 Traffic API 所需的 Administration 权限，不能替代此令牌。

## 统计口径

- 数据源为 GitHub 官方仓库访问/克隆统计，不是第三方图片计数器，也不统计 App 使用量。
- 每 6 小时采集一次；日期按 UTC 划分，今日数值尚未完整。
- GitHub API 只提供最近 14 天。启用时只能补回 API 当时提供的记录，此后通过 `traffic-data` 分支持续归档；不声称能恢复更早的未采集历史。
- 重叠日期用新值替换，绝不把每次抓到的 14 天总数重复累加。
- 近 14 天独立访客直接使用 API 的跨日去重汇总；不可将每日独立访客相加称为跨日独立人数。
- 未返回的日期留空，曲线断开；真实返回的 0 会保留。采集中断超过 14 天会留下无法补齐的历史缺口。
- 只公开日期和聚合计数，不收集 IP、账号、来源网站或访问路径。令牌仅通过 Secret 注入采集步骤。
- 获取失败时工作流失败并保留已发布图表和历史，不用空数据覆盖。
- 更新仅写入 `traffic-data` 分支，不向 `main` 或安卓开发分支持续提交自动统计。

## 文件

- `.github/workflows/repository-traffic.yml`：校验、定时/手动采集和发布。
- `scripts/traffic/repository_traffic.py`：标准库采集、历史合并、明暗自适应 SVG，无第三方统计服务。
- `scripts/traffic/test_repository_traffic.py`：重叠窗口、历史保留、缺值、去重口径和失败保护测试。
- `traffic-data` 分支：`traffic.json` 和 `traffic.svg`，README 共用同一张图。

测试在 GitHub Actions 执行，本地没有运行 Python/前端构建；本次不修改 App 功能。

官方接口与权限说明：[GitHub repository traffic API](https://docs.github.com/en/rest/metrics/traffic)。
