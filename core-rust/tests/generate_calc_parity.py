"""Freeze public/synthetic Python calc 0.5.0 outputs as the mobile Rust oracle.

Run from the repository root. --check verifies tracked oracle source hashes only;
regenerate intentionally when changing the Python contract or adding cases.
No provider calls, user documents, credentials or private ledgers are accessed.
"""
import argparse
import hashlib
import importlib
import json
import pathlib
import random
import subprocess
import sys
from datetime import date, timedelta

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
FILES = ["calc/formulas.py", "calc/series.py", "calc/indicators.py", "calc/earnings.py", "calc/display.py"]
TARGET = ROOT / "core-rust/tests/fixtures/calc_parity.json"


def source_hashes():
    return {name: hashlib.sha256(subprocess.check_output(["git", "show", "HEAD:" + name], cwd=ROOT)).hexdigest() for name in FILES}


def generate():
    modules = [importlib.import_module("calc." + name) for name in ["formulas", "series", "indicators", "earnings"]]
    from calc.display import attach_display
    names = ["pe_deducted_annualized", "pe_ttm_from_parts", "forward_pe", "fx_convert", "forward_cagr", "growth_rate", "ratio", "peg", "pe_digestion_years", "pe_digestion_scenarios", "percentile_rank", "consensus_dispersion", "forward_vs_ttm_judgement", "quarterize", "latest_quarter", "ttm_sum", "ttm_yoy", "qoq", "ttm_from_ytd", "ttm_yoy_from_ytd", "technical_indicators", "chip_distribution", "earnings_reaction", "earnings_surprise"]
    functions = {name: next(getattr(m, name) for m in modules if hasattr(m, name)) for name in names}
    cases = []

    def add(name, args, label=""):
        try:
            output = attach_display(functions[name](**args))
        except (OverflowError, ZeroDivisionError, ValueError, TypeError):
            # The Rust boundary returns error for overflow; never freeze non-JSON.
            output = {"status": "error", "value": None, "unit": "", "reason": "oracle arithmetic overflow", "details": {}, "display": None}
        cases.append({"name": label or f"{name}-{len(cases)}", "function": name, "args": args, "output": output})

    hand = json.loads(subprocess.check_output(["git", "show", "HEAD:calc/tests/fixtures/formulas_cases.json"], cwd=ROOT))
    for case in hand["cases"]:
        add(case["function"], case["args"], case["name"])
    rng = random.Random(42)
    for _ in range(12):
        x, y = rng.uniform(1, 100), rng.uniform(0.1, 10)
        for name, args in [
            ("pe_ttm_from_parts", dict(total_market_cap=x, cap_unit="亿元", ttm_profit=y, profit_unit="亿元")),
            ("pe_deducted_annualized", dict(total_market_cap=x, cap_unit="亿元", latest_quarter_deducted_profit=y, profit_unit="亿元")),
            ("forward_pe", dict(price=x, eps_forecast=y)),
            ("forward_cagr", dict(eps_t=y, eps_t_plus_n=x, years=rng.randint(1, 10))),
            ("growth_rate", dict(current=x-50, base=y)),
            ("ratio", dict(numerator=x-50, denominator=y)),
            ("peg", dict(pe=x, cagr=y/10)),
            ("pe_digestion_years", dict(pe=x, cagr=y/10, anchor=25)),
            ("pe_digestion_scenarios", dict(pe=x, cagr=y/10)),
            ("consensus_dispersion", dict(low=y, mean=y+1, high=y+5)),
            ("forward_vs_ttm_judgement", dict(forward_cagr_value=y/10, ttm_yoy_value=x/10)),
            ("percentile_rank", dict(history=list(range(-5, 35))+[None, True, "bad"], current=x)),
        ]:
            add(name, args)
    for ccy, unit in [("CNY", "亿元"), ("USD", "万美元"), ("HKD", "港元")]:
        for other, other_unit in [("CNY", "元"), ("USD", "亿美元"), ("HKD", "万港元")]:
            if ccy != other:
                for fx_unit in [f"{ccy}/{other}", f"{other}/{ccy}"]:
                    add("pe_ttm_from_parts", dict(total_market_cap=123, cap_unit=unit, ttm_profit=5, profit_unit=other_unit, fx_rate=0.87, fx_unit=fx_unit))
                    add("fx_convert", dict(value=-123.4, unit=unit, fx_rate=0.87, fx_unit=fx_unit))
    quarters = [dict(period=f"{year}-{end}", value=20+(year-2022)*5+q*2) for year in range(2022, 2026) for q, end in enumerate(["03-31", "06-30", "09-30", "12-31"])]
    cumulative = [dict(period=q["period"], value=q["value"]*((i%4)+1)) for i, q in enumerate(quarters)]
    for name in ["quarterize", "latest_quarter", "ttm_sum", "ttm_yoy", "qoq"]:
        key = "cumulative" if name == "quarterize" else "single_quarters"
        original = cumulative if name == "quarterize" else quarters
        for variant in [original, list(reversed(original)), original[:-1]+[dict(period=original[-1]["period"], value=None)], original[:-5], original+[original[0]], []]:
            args = {key: variant, "unit": "万元", "money": True}
            if name in ["ttm_sum", "ttm_yoy", "qoq"]:
                args["end_period"] = "2025-12-31"
            add(name, args)
    spans = [dict(period=f"{y}-01-01..{y}-{end}", value=10*(y-2019)*q) for y in range(2021, 2026) for q, end in [(1,"03-31"),(2,"06-30"),(3,"09-30"),(4,"12-31")]]
    for name in ["ttm_from_ytd", "ttm_yoy_from_ytd"]:
        for end in ["2025-01-01..2025-06-30", "2025-01-01..2025-12-31", "2021-01-01..2021-03-31"]:
            add(name, dict(cumulative=spans, end_period=end, unit="万美元", money=True))
        add(name, dict(cumulative=spans+[spans[0]], end_period=spans[-1]["period"]))
        fy = [dict(period="2022-09-25..2023-09-30", value=100), dict(period="2023-10-01..2024-09-28", value=120), dict(period="2023-10-01..2024-03-30", value=55), dict(period="2024-09-29..2025-03-29", value=65)]
        add(name, dict(cumulative=fy, end_period=fy[-1]["period"], unit="亿美元", money=True))
    bars=[]
    for i in range(90):
        close=100+(i%13)-6+i*0.15
        bars.append(dict(date=str(date(2026,1,1)+timedelta(days=i)), open=close-0.2, close=close, high=close+2, low=close-2, turn=(i%17)*0.23))
    for count in [10, 20, 30, 60, 90]:
        add("technical_indicators", dict(klines=bars[:count]))
        add("technical_indicators", dict(klines=list(reversed(bars[:count])), ma=[2,5,7], ema=[3,8], macd=[3,8,4], rsi=[2,6], kdj=[5,3,3], boll=[5,2.5], min_points=5))
        add("chip_distribution", dict(klines=bars[:count], grid_size=50, min_points=2, decay=0.5))
    flat=[dict(date=b["date"],open=10,close=10,high=10,low=10,turn=0) for b in bars[:30]]
    add("chip_distribution",dict(klines=flat))
    add("technical_indicators",dict(klines=flat))
    add("technical_indicators",dict(klines=bars+[bars[0]]))
    add("chip_distribution",dict(klines=bars,grid_size=49))
    events=[dict(id=f"E{i}",date=bars[i*15+2]["date"],timing=timing) for i,timing in enumerate(["before_open","during","after_close","before_open","after_close"])]
    add("earnings_reaction",dict(klines=bars,events=events))
    add("earnings_reaction",dict(klines=bars,events=events+[dict(id="same",date=events[-1]["date"],timing="after_close")],lookback=2))
    add("earnings_reaction",dict(klines=bars,events=events+[dict(id="pending",date="2026-05-01",timing="after_close")]))
    add("earnings_reaction",dict(klines=bars,events=events[:1]))
    add("earnings_reaction",dict(klines=bars,events=events+[events[0]]))
    for args in [dict(actual=12,estimate=10),dict(actual=-8,estimate=-10),dict(actual=12,low=8,high=10),dict(actual=10,low=10,high=10),dict(actual=0,estimate=0),dict(actual=1,low=2),dict(actual=1,low=3,high=2),dict(actual=1),dict(actual=1,estimate=True)]:
        add("earnings_surprise",args)
    seed=list(cases)
    # Invalid data cannot silently become zero/boolean or a finite success.
    first={}
    for case in seed:
        first.setdefault(case["function"], case["args"])
    for name,args in first.items():
        add(name,{})
        for field,value in args.items():
            if isinstance(value,(int,float)) and not isinstance(value,bool):
                for bad in [True,None,"NaN","not a number"]:
                    changed=dict(args);changed[field]=bad;add(name,changed)
                break
    return dict(calc_version="0.5.0", oracles=source_hashes(), cases=cases)


if __name__ == "__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--check", action="store_true");args=parser.parse_args()
    if args.check:
        fixture=json.loads(TARGET.read_text(encoding="utf-8"))
        if fixture["oracles"] != source_hashes():
            raise SystemExit("Python calc oracle changed; review and regenerate calc_parity.json")
        print(f"Calc oracle hashes match: {len(fixture['cases'])} frozen cases")
    else:
        fixture=generate()
        TARGET.write_text(json.dumps(fixture,ensure_ascii=False,allow_nan=False,separators=(",",":"))+"\n",encoding="utf-8")
        print(f"Wrote {len(fixture['cases'])} synthetic/oracle cases")
