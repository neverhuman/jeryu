#!/usr/bin/env python3
"""Plot the JeRyu jankurai score history as a polished PNG."""

from __future__ import annotations

import argparse
import csv
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable

import matplotlib.dates as mdates
import matplotlib.pyplot as plt
from matplotlib.lines import Line2D


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT = ROOT / "target/jankurai/jankurai-score-history.png"
DEFAULT_INPUTS = [
    ROOT / "agent/score-history.csv",
    ROOT / "agent/score-history.jsonl",
    ROOT / "agent/repo-score.json",
]


@dataclass(frozen=True)
class ScorePoint:
    generated_at: int
    when: datetime
    score: float
    commit: str
    run_id: str
    source: str


def _short_commit(commit: str) -> str:
    return commit.strip()[:7]


def _normalize_record(record: dict, source: str) -> ScorePoint:
    commit = str(record.get("commit") or record.get("git", {}).get("head") or "").strip()
    if not commit:
        raise ValueError(f"missing commit in {source}")
    generated_at = int(record["generated_at"])
    score = float(record["score"])
    run_id = str(record.get("run_id") or generated_at)
    when = datetime.fromtimestamp(generated_at, tz=timezone.utc)
    return ScorePoint(
        generated_at=generated_at,
        when=when,
        score=score,
        commit=commit,
        run_id=run_id,
        source=source,
    )


def _load_csv(path: Path) -> Iterable[ScorePoint]:
    with path.open(newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        for row in reader:
            yield _normalize_record(row, path.name)


def _load_jsonl(path: Path) -> Iterable[ScorePoint]:
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            yield _normalize_record(json.loads(line), path.name)


def _load_json(path: Path) -> Iterable[ScorePoint]:
    with path.open(encoding="utf-8") as handle:
        yield _normalize_record(json.load(handle), path.name)


def load_points(inputs: Iterable[Path]) -> list[ScorePoint]:
    loaders = {
        ".csv": _load_csv,
        ".jsonl": _load_jsonl,
        ".json": _load_json,
    }
    points: list[ScorePoint] = []
    seen: set[str] = set()

    for path in inputs:
        if not path.exists():
            continue
        loader = loaders.get(path.suffix.lower())
        if loader is None:
            continue
        for point in loader(path):
            key = point.run_id or f"{point.generated_at}:{point.commit}:{point.score}"
            if key in seen:
                continue
            seen.add(key)
            points.append(point)

    points.sort(key=lambda p: (p.generated_at, p.run_id, p.commit))
    return points


def build_plot(points: list[ScorePoint], output: Path, dpi: int) -> None:
    if not points:
        raise SystemExit("no score history points found")

    commits: list[str] = []
    for point in points:
        if point.commit not in commits:
            commits.append(point.commit)
    palette = [
        "#64748b",  # slate
        "#0f766e",  # teal
        "#c2410c",  # orange
        "#7c3aed",  # violet
        "#0ea5e9",  # sky
        "#b91c1c",  # red
    ]
    commit_colors = {commit: palette[i % len(palette)] for i, commit in enumerate(commits)}
    scores = [point.score for point in points]
    y_min = max(0, int(min(scores)) - 4)
    y_max = min(100, int(max(scores)) + 4)
    if y_max <= y_min:
        y_max = y_min + 1

    plt.rcParams.update(
        {
            "figure.facecolor": "#f8fafc",
            "axes.facecolor": "#ffffff",
            "axes.edgecolor": "#cbd5e1",
            "axes.labelcolor": "#0f172a",
            "xtick.color": "#334155",
            "ytick.color": "#334155",
            "text.color": "#0f172a",
            "font.size": 10.5,
            "axes.titleweight": "bold",
            "axes.titlesize": 13,
            "axes.labelsize": 12,
            "legend.fontsize": 10,
        }
    )

    fig, ax = plt.subplots(figsize=(14.5, 8.2), dpi=dpi)
    fig.suptitle("JeRyu code base", x=0.08, y=0.955, ha="left", fontsize=21, fontweight="bold")
    ax.set_title("jankurai score history across audit runs", loc="left", pad=12)

    times = [point.when for point in points]
    ax.plot(
        times,
        scores,
        color="#334155",
        linewidth=2.8,
        alpha=0.85,
        zorder=1,
        solid_capstyle="round",
    )

    for commit in commits:
        commit_points = [point for point in points if point.commit == commit]
        ax.scatter(
            [point.when for point in commit_points],
            [point.score for point in commit_points],
            s=64,
            color=commit_colors[commit],
            edgecolors="#ffffff",
            linewidths=1.2,
            zorder=3,
        )

    ax.fill_between(times, scores, y_min, color="#1d4ed8", alpha=0.08, zorder=0)

    threshold = 85
    if y_min < threshold < y_max:
        ax.axhline(
            threshold,
            color="#dc2626",
            linewidth=1.2,
            linestyle=(0, (5, 5)),
            alpha=0.7,
            zorder=0,
        )
        ax.text(
            times[0],
            threshold + 0.6,
            "pass threshold 85",
            color="#dc2626",
            fontsize=10,
            ha="left",
            va="bottom",
            bbox={
                "boxstyle": "round,pad=0.22",
                "facecolor": "#fff1f2",
                "edgecolor": "none",
                "alpha": 0.9,
            },
        )

    seen_commit_label: set[str] = set()
    for point in points:
        if point.commit in seen_commit_label:
            continue
        seen_commit_label.add(point.commit)
        ax.annotate(
            _short_commit(point.commit),
            (point.when, point.score),
            xytext=(0, 12),
            textcoords="offset points",
            ha="center",
            va="bottom",
            fontsize=10,
            fontweight="bold",
            color=commit_colors[point.commit],
            bbox={
                "boxstyle": "round,pad=0.25",
                "facecolor": "#ffffff",
                "edgecolor": "#cbd5e1",
                "alpha": 0.94,
            },
        )

    latest = points[-1]
    ax.scatter(
        [latest.when],
        [latest.score],
        s=180,
        color=commit_colors[latest.commit],
        edgecolors="#ffffff",
        linewidths=1.8,
        zorder=4,
    )
    ax.annotate(
        f"latest {int(latest.score)}",
        (latest.when, latest.score),
        xytext=(14, -18),
        textcoords="offset points",
        ha="left",
        va="top",
        fontsize=10,
        color="#0f172a",
        bbox={
            "boxstyle": "round,pad=0.28",
            "facecolor": "#ffffff",
            "edgecolor": "#cbd5e1",
            "alpha": 0.95,
        },
    )

    ax.set_ylabel("jankurai score")
    ax.set_xlabel("audit time")
    ax.set_ylim(y_min, y_max)

    locator = mdates.AutoDateLocator(minticks=4, maxticks=8)
    ax.xaxis.set_major_locator(locator)
    ax.xaxis.set_major_formatter(mdates.ConciseDateFormatter(locator))

    ax.yaxis.set_major_locator(plt.MaxNLocator(integer=True))
    ax.grid(axis="y", linestyle=(0, (2, 4)), color="#cbd5e1", linewidth=0.8, alpha=0.85)
    ax.grid(axis="x", visible=False)

    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    ax.spines["left"].set_color("#cbd5e1")
    ax.spines["bottom"].set_color("#cbd5e1")

    legend_handles = [
        Line2D(
            [0],
            [0],
            marker="o",
            color="none",
            markerfacecolor=commit_colors[commit],
            markeredgecolor="#ffffff",
            markeredgewidth=1.0,
            markersize=8,
            label=f"{_short_commit(commit)}",
        )
        for commit in commits
    ]
    ax.legend(
        handles=legend_handles,
        title="commit",
        frameon=False,
        loc="upper left",
        bbox_to_anchor=(0.0, 1.02),
        ncol=min(4, len(legend_handles)),
        handletextpad=0.5,
        columnspacing=1.0,
    )

    footer = (
        f"source: {', '.join(str(path.relative_to(ROOT)) for path in DEFAULT_INPUTS if path.exists())}"
        f"  •  points: {len(points)}  •  span: {int(min(scores))} -> {int(max(scores))}"
    )
    fig.text(
        0.08,
        0.03,
        footer,
        ha="left",
        va="bottom",
        fontsize=8.8,
        color="#475569",
    )

    output.parent.mkdir(parents=True, exist_ok=True)
    fig.subplots_adjust(left=0.095, right=0.985, top=0.81, bottom=0.14)
    fig.savefig(output, dpi=dpi, bbox_inches="tight", pad_inches=0.25, facecolor=fig.get_facecolor())
    plt.close(fig)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT,
        help="PNG output path",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=320,
        help="output resolution for the PNG",
    )
    parser.add_argument(
        "--input",
        dest="inputs",
        action="append",
        type=Path,
        help="extra history file to include; may be passed multiple times",
    )
    args = parser.parse_args()
    inputs = list(DEFAULT_INPUTS)
    if args.inputs:
        inputs.extend(args.inputs)

    points = load_points(inputs)
    build_plot(points, args.output, args.dpi)
    print(f"wrote {args.output}")
    print(f"plotted {len(points)} score points")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
