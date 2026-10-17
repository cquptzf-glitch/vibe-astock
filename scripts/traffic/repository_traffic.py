#!/usr/bin/env python3
"""Archive GitHub's aggregate traffic and render a dependency-free README SVG."""
import argparse
from datetime import date, datetime, timedelta, timezone
from html import escape
import json
import os
from pathlib import Path
import re
import sys
import tempfile
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

API_VERSION = "2026-03-10"


def nonnegative(value):
    if type(value) is not int or value < 0:
        raise ValueError("Traffic counts must be nonnegative integers")
    return value


def validate_snapshot(payload, metric):
    if not isinstance(payload, dict) or not isinstance(payload.get(metric), list):
        raise ValueError("Traffic API response is missing its daily series")
    total = {key: nonnegative(payload.get(key)) for key in ("count", "uniques")}
    rows = {}
    for row in payload[metric]:
        stamp = row.get("timestamp", "")
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T00:00:00Z", stamp):
            raise ValueError("Traffic timestamps must be UTC midnight")
        day = date.fromisoformat(stamp[:10]).isoformat()
        if day in rows:
            raise ValueError("Traffic API returned a duplicate day")
        rows[day] = {key: nonnegative(row.get(key)) for key in ("count", "uniques")}
    return total, rows


def merge_history(previous, repository, snapshots, now):
    if previous.get("schema_version") != 1 or previous.get("repository") != repository:
        raise ValueError("Traffic archive schema or repository does not match")
    # Never add overlapping rolling windows: newer samples replace that day.
    history = json.loads(json.dumps(previous.get("days", {})))
    if not isinstance(history, dict):
        raise ValueError("Invalid traffic archive")
    latest = {}
    for metric in ("views", "clones"):
        total, rows = validate_snapshot(snapshots[metric], metric)
        latest[metric] = total
        for day, counts in rows.items():
            if day > now.date().isoformat():
                raise ValueError("Traffic API returned a future day")
            history.setdefault(day, {})[metric] = counts
    return {"schema_version": 1, "repository": repository,
            "updated_at": now.isoformat().replace("+00:00", "Z"),
            "latest_14_days": latest, "days": dict(sorted(history.items()))}


def empty_history(repository):
    return {"schema_version": 1, "repository": repository, "updated_at": None,
            "latest_14_days": {}, "days": {}}


def fetch_metric(repository, metric, token):
    request = Request(
        "https://api.github.com/repos/" + repository + "/traffic/" + metric + "?per=day",
        headers={"Accept": "application/vnd.github+json",
                 "Authorization": "Bearer " + token, "X-GitHub-Api-Version": API_VERSION,
                 "User-Agent": "repository-traffic-chart"})
    try:
        with urlopen(request, timeout=25) as response:
            body = response.read(1_000_001)
    except HTTPError as error:
        raise RuntimeError("Traffic API HTTP " + str(error.code)
                           + "; check TRAFFIC_TOKEN repository access and Administration: read") from None
    except (URLError, TimeoutError, OSError):
        raise RuntimeError("Traffic API could not be reached; previous archive is preserved") from None
    if len(body) > 1_000_000:
        raise ValueError("Traffic API response exceeds size limit")
    payload = json.loads(body)
    validate_snapshot(payload, metric)
    return payload


def render_svg(state, now):
    e = lambda value: escape(str(value), quote=True)
    updated = state.get("updated_at")
    days = state.get("days", {})
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="960" height="370" viewBox="0 0 960 370" role="img" aria-labelledby="title description">',
           '<title id="title">GitHub 仓库访问量变化趋势</title>',
           '<desc id="description">最近 30 日每日浏览量与每日独立访客。缺失日期不补零，今日数据尚未结束。</desc>',
           '<style>:root{--bg:#fff;--fg:#1f2937;--muted:#596579;--border:#d8e0eb;--soft:#f6f8fc}@media(prefers-color-scheme:dark){:root{--bg:#0d1117;--fg:#e6edf3;--muted:#9daabd;--border:#30363d;--soft:#161b22}}text{font-family:system-ui,-apple-system,"Segoe UI","Noto Sans SC",sans-serif;fill:var(--fg)}.muted{fill:var(--muted)}.grid{stroke:var(--border);stroke-width:1}</style>',
           '<rect x=".5" y=".5" width="959" height="369" rx="14" fill="var(--bg)" stroke="var(--border)"/>',
           '<text x="24" y="33" font-size="20" font-weight="700">仓库访问趋势</text>',
           '<text x="24" y="54" class="muted" font-size="11">' + e(state["repository"]) + ' · GitHub Traffic · UTC 每日口径</text>']
    if not updated:
        out += ['<text x="480" y="167" text-anchor="middle" font-size="20" font-weight="600">等待 GitHub 访问数据</text>',
                '<text x="480" y="201" text-anchor="middle" class="muted" font-size="14">配置仓库 Secret：TRAFFIC_TOKEN，然后运行 Repository traffic</text>',
                '<text x="480" y="232" text-anchor="middle" class="muted" font-size="12">启用后每 6 小时自动更新，并持续归档历史趋势；此处不会显示虚构数据。</text>',
                '<text x="24" y="347" class="muted" font-size="11">详情：docs/repository-traffic.md · GitHub 官方统计需要 Administration: read 权限</text>', '</svg>']
        return "\n".join(out) + "\n"
    latest = state["latest_14_days"]
    cards = [("近 14 天浏览量", latest["views"]["count"]),
             ("近 14 天独立访客", latest["views"]["uniques"]),
             ("近 14 天克隆次数", latest["clones"]["count"]),
             ("近 14 天独立克隆者", latest["clones"]["uniques"])]
    for index, (label, value) in enumerate(cards):
        x = 24 + index * 234
        out += ['<rect x="' + str(x) + '" y="68" width="210" height="60" rx="8" fill="var(--soft)" stroke="var(--border)"/>',
                '<text x="' + str(x + 12) + '" y="89" class="muted" font-size="11">' + e(label) + '</text>',
                '<text x="' + str(x + 12) + '" y="115" font-size="22" font-weight="700">' + format(value, ",") + '</text>']
    dates = [(now.date() - timedelta(days=29-i)).isoformat() for i in range(30)]
    samples = [days.get(day, {}).get("views") for day in dates]
    maximum = max([1] + [sample["count"] for sample in samples if sample is not None])
    maximum = ((maximum + 3) // 4) * 4
    out += ['<text x="60" y="151" font-size="11" class="muted">每日趋势 · 最近 30 日</text>',
            '<rect x="710" y="141" width="10" height="10" fill="#3b82f6"/><text x="726" y="151" font-size="11">浏览量</text>',
            '<line x1="805" y1="146" x2="820" y2="146" stroke="#f59e0b" stroke-width="3"/><text x="826" y="151" font-size="11">每日访客</text>']
    left, top, width, height = 60, 168, 870, 126
    for tick in range(5):
        y = top + height * tick / 4
        label = maximum * (4 - tick) / 4
        out += ['<line class="grid" x1="60" y1="' + str(y) + '" x2="930" y2="' + str(y) + '"/>',
                '<text x="52" y="' + str(y + 4) + '" text-anchor="end" class="muted" font-size="10">' + e(format(label, ".0f")) + '</text>']
    segments, points = [], []
    for index, (day, sample) in enumerate(zip(dates, samples)):
        x = left + (index + .5) * width / 30
        if sample is None:
            if points:
                segments.append(points)
                points = []
            continue
        count, uniques = sample["count"], sample["uniques"]
        h = count / maximum * height
        y = top + height - uniques / maximum * height
        out += ['<rect x="' + str(round(x - 9, 2)) + '" y="' + str(round(top + height - h, 2))
                + '" width="18" height="' + str(round(h, 2)) + '" rx="2" fill="#3b82f6" opacity=".8"><title>'
                + e(day + " 浏览量 " + str(count)) + '</title></rect>']
        points.append((x, y, day, uniques))
    if points:
        segments.append(points)
    for segment in segments:
        coords = " ".join(str(round(p[0], 2)) + "," + str(round(p[1], 2)) for p in segment)
        out.append('<polyline fill="none" stroke="#f59e0b" stroke-width="2.5" points="' + coords + '"/>')
        for x, y, day, uniques in segment:
            out.append('<circle cx="' + str(round(x, 2)) + '" cy="' + str(round(y, 2))
                       + '" r="3" fill="#f59e0b"><title>' + e(day + " 每日独立访客 " + str(uniques)) + '</title></circle>')
    for index in (0, 6, 12, 18, 24, 29):
        x = left + (index + .5) * width / 30
        out.append('<text x="' + str(x) + '" y="312" text-anchor="middle" font-size="10" class="muted">' + dates[index][5:] + '</text>')
    missing = sum(sample is None for sample in samples)
    out += ['<text x="24" y="339" class="muted" font-size="10">缺失 ' + str(missing)
            + ' 天留空，不视为 0；每日访客不可相加为跨日独立访客。今日尚未结束。</text>',
            '<text x="24" y="356" class="muted" font-size="10">更新：' + e(updated)
            + ' · 来源：GitHub 官方 Traffic API · 近 14 天汇总使用 API 的跨日去重口径</text>', '</svg>']
    return "\n".join(out) + "\n"


def atomic_write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as tmp:
        tmp.write(content)
        name = tmp.name
    os.replace(name, path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repository", default=os.environ.get("GITHUB_REPOSITORY", ""))
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        raise ValueError("Set a valid owner/repository")
    token = os.environ.get("TRAFFIC_TOKEN", "").strip()
    if not token:
        raise ValueError("Set the TRAFFIC_TOKEN Actions secret with Administration: read")
    archive = args.output / "traffic.json"
    previous = json.loads(archive.read_text("utf-8")) if archive.exists() else empty_history(args.repository)
    # Finish both requests and all validation before touching the existing archive.
    snapshots = {metric: fetch_metric(args.repository, metric, token) for metric in ("views", "clones")}
    now = datetime.now(timezone.utc)
    state = merge_history(previous, args.repository, snapshots, now)
    chart = render_svg(state, now)
    atomic_write(archive, json.dumps(state, ensure_ascii=False, indent=2) + "\n")
    atomic_write(args.output / "traffic.svg", chart)
    print("Traffic archive and SVG updated from GitHub aggregate statistics.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, RuntimeError, OSError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
