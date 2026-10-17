import json
from datetime import datetime, timezone
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from xml.etree import ElementTree
import repository_traffic as traffic

NOW = datetime(2026, 10, 17, 12, tzinfo=timezone.utc)
REPO = "example/project"


def payload(metric, count=12, uniques=3, day="2026-10-16"):
    return {"count": count, "uniques": uniques, metric: [
        {"timestamp": day + "T00:00:00Z", "count": count, "uniques": uniques}
    ]}


def snapshots(count=12, uniques=3):
    return {metric: payload(metric, count, uniques) for metric in ("views", "clones")}


class TrafficTests(unittest.TestCase):
    def test_overlapping_windows_replace_instead_of_double_counting(self):
        first = traffic.merge_history(traffic.empty_history(REPO), REPO, snapshots(12), NOW)
        second = traffic.merge_history(first, REPO, snapshots(14), NOW)
        self.assertEqual(second["days"]["2026-10-16"]["views"]["count"], 14)
        self.assertEqual(len(second["days"]), 1)
        self.assertEqual(first["days"]["2026-10-16"]["views"]["count"], 12)

    def test_archive_keeps_days_outside_api_retention(self):
        previous = traffic.empty_history(REPO)
        previous["days"]["2025-01-01"] = {"views": {"count": 99, "uniques": 8}}
        result = traffic.merge_history(previous, REPO, snapshots(), NOW)
        self.assertEqual(result["days"]["2025-01-01"]["views"]["count"], 99)

    def test_cross_day_uniques_use_api_total_not_daily_sum(self):
        data = snapshots(10, 3)
        for metric in ("views", "clones"):
            data[metric][metric].append(
                {"timestamp": "2026-10-15T00:00:00Z", "count": 10, "uniques": 3})
            data[metric]["count"] = 20
        result = traffic.merge_history(traffic.empty_history(REPO), REPO, data, NOW)
        self.assertEqual(result["latest_14_days"]["views"]["uniques"], 3)
        self.assertEqual(sum(d["views"]["uniques"] for d in result["days"].values()), 6)

    def test_missing_days_are_gaps_and_valid_zero_is_retained(self):
        state = traffic.merge_history(traffic.empty_history(REPO), REPO, snapshots(0, 0), NOW)
        self.assertNotIn("2026-10-15", state["days"])
        svg = traffic.render_svg(state, NOW)
        self.assertIn("缺失 29 天留空", svg)
        self.assertIn("2026-10-16 每日独立访客 0", svg)
        self.assertNotIn("2026-10-15 每日独立访客 0", svg)
        ElementTree.fromstring(svg)

    def test_missing_credentials_placeholder_has_no_fabricated_graph(self):
        svg = traffic.render_svg(traffic.empty_history(REPO), NOW)
        self.assertIn("等待 GitHub 访问数据", svg)
        self.assertIn("TRAFFIC_TOKEN", svg)
        self.assertNotIn("<polyline", svg)
        self.assertNotIn("近 14 天浏览量", svg)
        ElementTree.fromstring(svg)

    def test_svg_escapes_repository_text(self):
        svg = traffic.render_svg(traffic.empty_history('name/<script>&"'), NOW)
        self.assertIn("&lt;script&gt;&amp;&quot;", svg)
        self.assertNotIn("<script>", svg)
        ElementTree.fromstring(svg)

    def test_rejects_invalid_counts_timestamps_and_duplicate_days(self):
        for invalid in (-1, 1.5, True, None, "12"):
            data = payload("views")
            data["views"][0]["count"] = invalid
            with self.assertRaises(ValueError):
                traffic.validate_snapshot(data, "views")
        data = payload("views")
        data["views"].append(data["views"][0])
        with self.assertRaises(ValueError):
            traffic.validate_snapshot(data, "views")
        for stamp in ("2026-10-16T09:00:00Z", "2026-02-30T00:00:00Z", "not-a-date"):
            data = payload("views")
            data["views"][0]["timestamp"] = stamp
            with self.assertRaises(ValueError):
                traffic.validate_snapshot(data, "views")

    def test_future_samples_and_different_repository_are_rejected(self):
        data = snapshots()
        data["views"] = payload("views", day="2026-10-18")
        with self.assertRaises(ValueError):
            traffic.merge_history(traffic.empty_history(REPO), REPO, data, NOW)
        with self.assertRaises(ValueError):
            traffic.merge_history(traffic.empty_history("other/repo"), REPO, snapshots(), NOW)

    def test_second_api_failure_does_not_overwrite_archive(self):
        with tempfile.TemporaryDirectory() as folder:
            archive = Path(folder) / "traffic.json"
            svg = Path(folder) / "traffic.svg"
            original = json.dumps(traffic.empty_history(REPO))
            archive.write_text(original, encoding="utf-8")
            svg.write_text("existing-chart", encoding="utf-8")
            with patch.dict("os.environ", {"TRAFFIC_TOKEN": "unit-test-placeholder"}), \
                 patch("sys.argv", ["repository_traffic.py", "--repository", REPO, "--output", folder]), \
                 patch.object(traffic, "fetch_metric", side_effect=[payload("views"), RuntimeError("API unavailable")]):
                with self.assertRaises(RuntimeError):
                    traffic.main()
            self.assertEqual(archive.read_text("utf-8"), original)
            self.assertEqual(svg.read_text("utf-8"), "existing-chart")

    def test_sanitized_request_failure_does_not_expose_credential(self):
        from urllib.error import HTTPError
        with patch.object(traffic, "urlopen", side_effect=HTTPError(
                "https://api.github.com/example", 403, "Forbidden", {}, None)):
            with self.assertRaises(RuntimeError) as raised:
                traffic.fetch_metric(REPO, "views", "unit-test-placeholder")
        self.assertIn("403", str(raised.exception))
        self.assertNotIn("unit-test-placeholder", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
