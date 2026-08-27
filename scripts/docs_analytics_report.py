#!/usr/bin/env python3
"""Build a private, aggregate rumdl.dev adoption report from Analytics Engine."""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import sys
import urllib.error
import urllib.request
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path


DATASET_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
EVENT_LABELS = {
    "cta_select": "CTA selections",
    "command_copy": "Command copies",
    "playground_ready": "Playground ready",
    "playground_example": "Examples loaded",
    "playground_fix": "Fixes applied",
    "playground_config": "Configuration changes",
    "playground_share": "Share attempts",
    "playground_error": "Playground errors",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("rumdl-adoption-report.html"))
    parser.add_argument("--summary", type=Path)
    parser.add_argument("--dataset", default="rumdl_web_events")
    parser.add_argument("--fixture", action="store_true", help="Render deterministic synthetic data")
    return parser.parse_args()


def query(account_id: str, token: str, sql: str) -> list[dict[str, object]]:
    endpoint = f"https://api.cloudflare.com/client/v4/accounts/{account_id}/analytics_engine/sql"
    request = urllib.request.Request(
        endpoint,
        data=sql.encode("utf-8"),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "text/plain; charset=utf-8",
            "User-Agent": "rumdl-report/1.0",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")[:1000]
        raise RuntimeError(f"Analytics Engine query failed ({error.code}): {detail}") from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"Analytics Engine request failed: {error.reason}") from error

    rows = payload.get("data") if isinstance(payload, dict) else None
    if not isinstance(rows, list):
        raise RuntimeError("Analytics Engine returned an unexpected response")
    return rows


def aggregate_sql(dataset: str, start_days: int, end_days: int = 0) -> str:
    end = f" AND timestamp < now() - INTERVAL '{end_days}' DAY" if end_days else ""
    return f"""
SELECT
  index1 AS event,
  blob2 AS dimension1,
  blob3 AS dimension2,
  blob4 AS dimension3,
  sum(_sample_interval) AS events
FROM {dataset}
WHERE timestamp >= now() - INTERVAL '{start_days}' DAY{end}
GROUP BY event, dimension1, dimension2, dimension3
ORDER BY events DESC
""".strip()


def daily_sql(dataset: str) -> str:
    return f"""
SELECT
  formatDateTime(toStartOfInterval(timestamp, INTERVAL '1' DAY), '%Y-%m-%d') AS day,
  index1 AS event,
  sum(_sample_interval) AS events
FROM {dataset}
WHERE timestamp >= now() - INTERVAL '28' DAY
GROUP BY day, event
ORDER BY day ASC, event ASC
""".strip()


def fixture_rows(now: datetime) -> tuple[list[dict[str, object]], list[dict[str, object]], list[dict[str, object]]]:
    current = [
        {"event": "cta_select", "dimension1": "repository_trial", "dimension2": "hero", "dimension3": "", "events": 184},
        {"event": "cta_select", "dimension1": "open_playground", "dimension2": "hero", "dimension3": "", "events": 112},
        {"event": "cta_select", "dimension1": "compare_markdownlint", "dimension2": "next", "dimension3": "", "events": 68},
        {"event": "command_copy", "dimension1": "npx_check", "dimension2": "success", "dimension3": "", "events": 97},
        {"event": "playground_ready", "dimension1": "default", "dimension2": "", "dimension3": "", "events": 241},
        {"event": "playground_fix", "dimension1": "all", "dimension2": "clean", "dimension3": "", "events": 76},
        {"event": "playground_fix", "dimension1": "single", "dimension2": "remaining", "dimension3": "", "events": 44},
        {"event": "playground_config", "dimension1": "mkdocs", "dimension2": "2_4", "dimension3": "81_120", "events": 35},
        {"event": "playground_config", "dimension1": "standard", "dimension2": "0", "dimension3": "80", "events": 28},
        {"event": "playground_share", "dimension1": "success", "dimension2": "", "dimension3": "", "events": 19},
        {"event": "playground_error", "dimension1": "load", "dimension2": "", "dimension3": "", "events": 3},
    ]
    previous = [dict(row, events=max(0, int(row["events"] * 0.82))) for row in current]
    daily: list[dict[str, object]] = []
    for offset in range(27, -1, -1):
        day = (now - timedelta(days=offset)).strftime("%Y-%m-%d")
        daily.append({"day": day, "event": "cta_select", "events": 32 + ((27 - offset) * 7) % 31})
        daily.append({"day": day, "event": "playground_fix", "events": 13 + ((27 - offset) * 5) % 19})
    return current, previous, daily


def number(value: object) -> int:
    try:
        return round(float(value))
    except (TypeError, ValueError):
        return 0


def total(rows: list[dict[str, object]], event: str | None = None) -> int:
    return sum(number(row.get("events")) for row in rows if event is None or row.get("event") == event)


def grouped(rows: list[dict[str, object]], event: str, dimensions: tuple[str, ...]) -> list[tuple[tuple[str, ...], int]]:
    result: dict[tuple[str, ...], int] = defaultdict(int)
    for row in rows:
        if row.get("event") != event:
            continue
        key = tuple(str(row.get(dimension, "") or "") for dimension in dimensions)
        result[key] += number(row.get("events"))
    return sorted(result.items(), key=lambda item: (-item[1], item[0]))


def change(current: int, previous: int) -> tuple[str, str]:
    if previous == 0:
        return ("New", "up") if current else ("—", "flat")
    percent = round(((current - previous) / previous) * 100)
    if percent > 0:
        return f"+{percent}%", "up"
    if percent < 0:
        return f"{percent}%", "down"
    return "0%", "flat"


def label(value: str) -> str:
    return (
        value.replace("_", " ")
        .replace("2 4", "2–4")
        .replace("5 plus", "5+")
        .replace("81 120", "81–120")
        .title()
    )


def metric(title: str, value: int, previous: int, note: str) -> str:
    delta, tone = change(value, previous)
    return f"""
    <article class="metric">
      <div class="metric__line"><span>{html.escape(title)}</span><strong class="delta delta--{tone}">{delta}</strong></div>
      <div class="metric__value">{value:,}</div>
      <p>{html.escape(note)}</p>
    </article>"""


def table(title: str, description: str, headings: tuple[str, ...], rows: list[tuple[tuple[str, ...], int]], empty: str) -> str:
    if rows:
        body = "".join(
            "<tr>" + "".join(f"<td>{html.escape(label(value) if value else '—')}</td>" for value in values)
            + f'<td class="numeric">{count:,}</td></tr>'
            for values, count in rows
        )
    else:
        body = f'<tr><td class="empty" colspan="{len(headings) + 1}">{html.escape(empty)}</td></tr>'
    header = "".join(f"<th scope=\"col\">{html.escape(value)}</th>" for value in headings)
    return f"""
    <section class="detail">
      <div class="section-heading"><h2>{html.escape(title)}</h2><p>{html.escape(description)}</p></div>
      <div class="table-wrap"><table class="{'table--wide' if len(headings) >= 3 else ''}"><thead><tr>{header}<th class="numeric" scope="col">Events</th></tr></thead><tbody>{body}</tbody></table></div>
    </section>"""


def trend_markup(rows: list[dict[str, object]], now: datetime) -> str:
    by_day: dict[str, int] = defaultdict(int)
    for row in rows:
        by_day[str(row.get("day", ""))[:10]] += number(row.get("events"))
    days = [(now - timedelta(days=offset)).strftime("%Y-%m-%d") for offset in range(27, -1, -1)]
    maximum = max((by_day[day] for day in days), default=0)
    bars = []
    for index, day in enumerate(days):
        count = by_day[day]
        height = 4 if maximum == 0 else max(4, round((count / maximum) * 100))
        date = datetime.strptime(day, "%Y-%m-%d").strftime("%b %-d")
        tick = date if index in {0, 7, 14, 21, 27} else ""
        bars.append(
            f'<div class="bar-column" title="{html.escape(date)}: {count:,} events" aria-label="{html.escape(date)}, {count:,} events">'
            f'<span class="bar" style="--height:{height}%"></span><span class="tick">{html.escape(tick)}</span></div>'
        )
    return "".join(bars)


def build_report(current: list[dict[str, object]], previous: list[dict[str, object]], daily: list[dict[str, object]], now: datetime, synthetic: bool) -> str:
    all_current = total(current)
    all_previous = total(previous)
    playground_current = sum(number(row.get("events")) for row in current if str(row.get("event", "")).startswith("playground_"))
    playground_previous = sum(number(row.get("events")) for row in previous if str(row.get("event", "")).startswith("playground_"))
    trials = sum(count for (values, count) in grouped(current, "cta_select", ("dimension1",)) if values[0] == "repository_trial")
    previous_trials = sum(count for (values, count) in grouped(previous, "cta_select", ("dimension1",)) if values[0] == "repository_trial")
    errors = total(current, "playground_error")
    previous_errors = total(previous, "playground_error")
    status = "Synthetic preview" if synthetic else "Live aggregate data"
    empty_note = "Collection is active; the first interactions will appear here after they are recorded." if not synthetic else "Synthetic fixture contains no matching events."

    event_totals: dict[str, int] = defaultdict(int)
    for row in current:
        name = EVENT_LABELS.get(str(row.get("event")), label(str(row.get("event"))))
        event_totals[name] += number(row.get("events"))
    event_summary = sorted((((name,), count) for name, count in event_totals.items()), key=lambda item: -item[1])

    generated = now.strftime("%B %-d, %Y at %H:%M UTC")
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light">
<title>rumdl.dev weekly adoption pulse</title>
<style>
:root {{ --paper:#f4f1e9; --paper-soft:#ebe6da; --ink:#111318; --muted:#64615a; --rule:#c8c1b3; --coral:#ff595e; --coral-deep:#9f2430; --night:#121a29; --night-muted:#aeb9ca; --white:#fffdf8; }}
* {{ box-sizing:border-box; }}
html {{ background:var(--paper); color:var(--ink); font-family:Manrope,Inter,ui-sans-serif,system-ui,sans-serif; scroll-behavior:smooth; }}
body {{ margin:0; }}
::selection {{ background:var(--coral); color:var(--ink); }}
a {{ color:inherit; text-underline-offset:.2em; }}
.shell {{ width:min(1360px,calc(100% - 48px)); margin:auto; }}
.topbar {{ min-height:72px; display:flex; align-items:center; justify-content:space-between; border-bottom:1px solid var(--rule); font-size:.78rem; font-weight:750; }}
.brand {{ font-size:1.1rem; letter-spacing:-.03em; }}
.brand::before {{ content:""; display:inline-block; width:.7rem; height:.7rem; margin-right:.55rem; background:var(--coral); clip-path:polygon(0 0,100% 0,100% 35%,65% 35%,65% 100%,35% 100%,35% 35%,0 35%); }}
.status {{ color:var(--muted); }}
.hero {{ display:grid; grid-template-columns:minmax(0,1.35fr) minmax(280px,.65fr); gap:clamp(40px,8vw,120px); padding:clamp(72px,10vw,132px) 0 70px; }}
.hero h1 {{ max-width:11ch; margin:0; font-size:clamp(3.4rem,8vw,6rem); line-height:.94; letter-spacing:-.04em; text-wrap:balance; }}
.hero p {{ max-width:38rem; margin:28px 0 0; color:var(--muted); font-size:clamp(1.05rem,2vw,1.32rem); line-height:1.5; }}
.period {{ align-self:end; padding:26px 0 4px; border-top:1px solid var(--ink); }}
.period strong {{ display:block; font-size:1.35rem; letter-spacing:-.025em; }}
.period span {{ display:block; margin-top:8px; color:var(--muted); font-size:.85rem; line-height:1.5; }}
.metrics {{ display:grid; grid-template-columns:repeat(4,1fr); border-block:1px solid var(--ink); }}
.metric {{ min-width:0; padding:30px 28px 32px 0; }}
.metric + .metric {{ padding-left:28px; border-left:1px solid var(--rule); }}
.metric__line {{ display:flex; gap:12px; justify-content:space-between; align-items:baseline; font-size:.76rem; font-weight:750; }}
.metric__value {{ margin-top:26px; font-size:clamp(2.4rem,4vw,4rem); font-variant-numeric:tabular-nums; font-weight:760; letter-spacing:-.04em; }}
.metric p {{ margin:8px 0 0; color:var(--muted); font-size:.82rem; line-height:1.45; }}
.delta {{ font-size:.72rem; font-variant-numeric:tabular-nums; }}
.delta--up {{ color:var(--ink); }} .delta--down {{ color:var(--coral-deep); }} .delta--flat {{ color:var(--muted); }}
.trend-section {{ margin-top:88px; padding:38px; color:var(--white); background:var(--night); border-radius:14px; }}
.trend-heading {{ display:flex; align-items:end; justify-content:space-between; gap:28px; padding-bottom:32px; border-bottom:1px solid #344155; }}
.trend-heading h2 {{ margin:0; font-size:clamp(2rem,4vw,3.5rem); line-height:1; letter-spacing:-.04em; }}
.trend-heading p {{ max-width:30rem; margin:0; color:var(--night-muted); line-height:1.5; }}
.chart {{ display:grid; grid-template-columns:repeat(28,minmax(5px,1fr)); gap:clamp(3px,.7vw,10px); height:270px; padding-top:38px; }}
.bar-column {{ min-width:0; display:grid; grid-template-rows:1fr 34px; align-items:end; font-size:.63rem; color:var(--night-muted); }}
.bar {{ width:100%; height:var(--height); min-height:4px; display:block; background:var(--coral); border-radius:3px 3px 0 0; }}
.tick {{ padding-top:10px; white-space:nowrap; }}
.details {{ padding:clamp(88px,11vw,140px) 0; }}
.detail {{ display:grid; grid-template-columns:minmax(230px,.58fr) minmax(0,1.42fr); gap:clamp(32px,7vw,96px); padding:54px 0; border-top:1px solid var(--rule); }}
.detail:last-child {{ border-bottom:1px solid var(--rule); }}
.section-heading h2 {{ margin:0; font-size:1.45rem; letter-spacing:-.03em; }}
.section-heading p {{ max-width:32rem; margin:12px 0 0; color:var(--muted); line-height:1.55; }}
.table-wrap {{ overflow-x:auto; scrollbar-color:var(--coral) var(--paper-soft); }}
table {{ width:100%; border-collapse:collapse; font-size:.88rem; }}
.table--wide {{ min-width:520px; }}
th,td {{ padding:13px 14px; text-align:left; border-bottom:1px solid var(--rule); }}
th {{ padding-top:0; color:var(--muted); font-size:.7rem; text-transform:uppercase; letter-spacing:.04em; }}
.numeric {{ text-align:right; font-variant-numeric:tabular-nums; }}
.empty {{ color:var(--muted); text-align:center; padding:42px 14px; }}
.privacy {{ display:grid; grid-template-columns:1fr 1fr; gap:clamp(32px,8vw,120px); padding:62px 0 90px; }}
.privacy h2 {{ margin:0; font-size:clamp(2rem,4vw,3.5rem); line-height:1; letter-spacing:-.04em; }}
.privacy p {{ margin:0; color:var(--muted); line-height:1.65; }}
.privacy strong {{ color:var(--ink); }}
footer {{ padding:24px 0 38px; border-top:1px solid var(--rule); color:var(--muted); font-size:.75rem; display:flex; justify-content:space-between; gap:20px; }}
@media (max-width:900px) {{ .hero,.detail,.privacy {{ grid-template-columns:1fr; }} .metrics {{ grid-template-columns:repeat(2,1fr); }} .metric:nth-child(3) {{ border-left:0; border-top:1px solid var(--rule); padding-left:0; }} .metric:nth-child(4) {{ border-top:1px solid var(--rule); }} }}
@media (max-width:620px) {{ .shell {{ width:calc(100% - 28px); }} .topbar {{ min-height:60px; }} .status {{ display:none; }} .hero {{ padding-top:60px; }} .metrics {{ grid-template-columns:1fr; }} .metric + .metric,.metric:nth-child(3) {{ border-left:0; border-top:1px solid var(--rule); padding-left:0; }} .trend-section {{ margin-inline:-14px; border-radius:0; padding:28px 14px; }} .trend-heading {{ align-items:start; flex-direction:column; }} .chart {{ height:220px; gap:3px; }} .tick {{ font-size:.55rem; transform:rotate(-35deg); transform-origin:top left; }} .bar-column:last-child .tick {{ justify-self:end; transform:none; }} footer {{ flex-direction:column; }} }}
@media print {{ .shell {{ width:100%; }} .trend-section {{ break-inside:avoid; }} .detail {{ break-inside:avoid; }} }}
</style>
</head>
<body>
<div class="shell">
  <header class="topbar"><div class="brand">rumdl.dev</div><div>Weekly adoption pulse</div><div class="status">{html.escape(status)}</div></header>
  <main>
    <section class="hero">
      <div><h1>Signals, not surveillance.</h1><p>A weekly view of the actions that show whether developers are trying rumdl and finding depth in the playground. Counts are aggregate and content-free.</p></div>
      <div class="period"><strong>Rolling seven days</strong><span>Compared with the preceding seven-day period.<br>Generated {html.escape(generated)}.</span></div>
    </section>
    <section class="metrics" aria-label="Key adoption metrics">
      {metric("Recorded actions", all_current, all_previous, "All allowlisted product events")}
      {metric("Playground actions", playground_current, playground_previous, "Ready, fix, config, share, and error events")}
      {metric("Repository trials", trials, previous_trials, "Selections of the read-only repository path")}
      {metric("Playground errors", errors, previous_errors, "Load, lint, config, or share stages")}
    </section>
    <section class="trend-section">
      <div class="trend-heading"><h2>28-day rhythm</h2><p>Daily weighted event totals. Analytics Engine sampling is accounted for with <code>_sample_interval</code>.</p></div>
      <div class="chart" role="img" aria-label="Daily aggregate event totals for the last 28 days">{trend_markup(daily, now)}</div>
    </section>
    <div class="details">
      {table("Event mix", "The product actions recorded in the current period.", ("Event",), event_summary, empty_note)}
      {table("Acquisition paths", "Which call to action was selected, and where it appeared.", ("Action", "Location"), grouped(current, "cta_select", ("dimension1", "dimension2")), empty_note)}
      {table("Fix behavior", "Whether visitors used a focused fix or fix-all, plus the resulting state.", ("Scope", "Outcome"), grouped(current, "playground_fix", ("dimension1", "dimension2")), empty_note)}
      {table("Configuration depth", "Flavor, disabled-rule bucket, and line-length bucket—never raw configuration.", ("Flavor", "Rules disabled", "Line length"), grouped(current, "playground_config", ("dimension1", "dimension2", "dimension3")), empty_note)}
      {table("Sharing outcomes", "Successful, failed, or size-limited share-link attempts.", ("Result",), grouped(current, "playground_share", ("dimension1",)), empty_note)}
      {table("Error stages", "Where playground errors occurred, without messages, Markdown, or stack traces.", ("Stage",), grouped(current, "playground_error", ("dimension1",)), "No playground errors were recorded in this period.")}
    </div>
    <section class="privacy">
      <h2>Deliberately aggregate.</h2>
      <p><strong>This report cannot identify a visitor or reconstruct a journey.</strong> The endpoint accepts only named events and fixed categorical values. It rejects Markdown, URLs, referrers, user agents, account identifiers, and free-form properties. Use the results to prioritize product work—not to profile people.</p>
    </section>
  </main>
  <footer><span>Dataset: rumdl_web_events · Retention: 3 months</span><span>{html.escape(status)} · {html.escape(generated)}</span></footer>
</div>
</body>
</html>
"""


def write_summary(path: Path, current: list[dict[str, object]], previous: list[dict[str, object]], synthetic: bool) -> None:
    metrics = [
        ("Recorded actions", total(current), total(previous)),
        ("Repository trials", sum(count for (values, count) in grouped(current, "cta_select", ("dimension1",)) if values[0] == "repository_trial"), sum(count for (values, count) in grouped(previous, "cta_select", ("dimension1",)) if values[0] == "repository_trial")),
        ("Playground fixes", total(current, "playground_fix"), total(previous, "playground_fix")),
        ("Playground errors", total(current, "playground_error"), total(previous, "playground_error")),
    ]
    lines = ["# rumdl.dev weekly adoption pulse", "", "Synthetic fixture preview." if synthetic else "Rolling seven days compared with the preceding seven days.", "", "| Metric | Events | Change |", "| --- | ---: | ---: |"]
    for title, value, old in metrics:
        lines.append(f"| {title} | {value:,} | {change(value, old)[0]} |")
    lines.extend(["", "Download the `rumdl-adoption-report` artifact for the complete HTML report.", ""])
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    args = parse_args()
    if not DATASET_RE.fullmatch(args.dataset):
        print("Invalid Analytics Engine dataset name", file=sys.stderr)
        return 2

    now = datetime.now(timezone.utc)
    if args.fixture:
        current, previous, daily = fixture_rows(now)
    else:
        account_id = os.environ.get("CLOUDFLARE_ACCOUNT_ID", "")
        token = os.environ.get("CLOUDFLARE_ANALYTICS_TOKEN", "")
        if not account_id or not token:
            print("CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_ANALYTICS_TOKEN are required", file=sys.stderr)
            return 2
        current = query(account_id, token, aggregate_sql(args.dataset, 7))
        previous = query(account_id, token, aggregate_sql(args.dataset, 14, 7))
        daily = query(account_id, token, daily_sql(args.dataset))

    document = build_report(current, previous, daily, now, args.fixture)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(document, encoding="utf-8")
    if args.summary:
        write_summary(args.summary, current, previous, args.fixture)
    print(f"wrote {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
