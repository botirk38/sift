#!/usr/bin/env python3

from __future__ import annotations

import argparse
import csv
import re
import statistics
from collections import defaultdict
from pathlib import Path


CATEGORY_ORDER = [
    "Indexed literals",
    "Indexed word",
    "Indexed alternation",
    "Full-scan Unicode",
    "Full-scan no-literal",
]

BENCHMARK_CATEGORY = {
    "linux_literal_default": "Indexed literals",
    "linux_literal": "Indexed literals",
    "linux_literal_casei": "Indexed literals",
    "linux_re_literal_suffix": "Indexed literals",
    "linux_word": "Indexed word",
    "linux_alternates": "Indexed alternation",
    "linux_alternates_casei": "Indexed alternation",
    "linux_unicode_greek": "Full-scan Unicode",
    "linux_unicode_greek_casei": "Full-scan Unicode",
    "linux_unicode_word": "Full-scan Unicode",
    "linux_no_literal": "Full-scan no-literal",
}

DETAILS = {
    "Indexed literals": "Trigram narrowing dominates",
    "Indexed word": "Word-shaped literals stay cheap",
    "Indexed alternation": "Candidate narrowing plus build_many helps",
    "Full-scan Unicode": "Greek classes remain the main holdout",
    "Full-scan no-literal": "Regex-engine full scans are still hardest",
}


def clean_svg(path: Path) -> None:
    svg = path.read_text(encoding="utf-8")
    svg = re.sub(r"<!DOCTYPE[^>]*>\s*", "", svg, count=1)
    svg = re.sub(r"<metadata>.*?</metadata>\s*", "", svg, count=1, flags=re.S)
    path.write_text(svg, encoding="utf-8")


def classify_status(speedup: float) -> str:
    if speedup >= 1.05:
        return "win"
    if speedup >= 0.95:
        return "near"
    return "loss"


def load_results(path: Path) -> tuple[list[dict], dict[str, int]]:
    by_benchmark: dict[str, dict[str, list[float]]] = defaultdict(
        lambda: {"rg": [], "sift": []}
    )

    with path.open("r", encoding="utf-8", newline="") as fh:
        reader = csv.DictReader(fh)
        for row in reader:
            benchmark = row["benchmark"]
            bucket = None
            if row["name"].startswith("rg"):
                bucket = "rg"
            elif row["name"].startswith("sift"):
                bucket = "sift"
            if bucket is None:
                continue
            by_benchmark[benchmark][bucket].append(float(row["duration"]))

    categories: dict[str, list[float]] = defaultdict(list)
    wins = {"sift": 0, "rg": 0, "tie": 0}

    for benchmark, commands in by_benchmark.items():
        category = BENCHMARK_CATEGORY.get(benchmark)
        if category is None:
            continue
        if not commands["rg"] or not commands["sift"]:
            continue
        rg_mean = statistics.mean(commands["rg"])
        sift_mean = statistics.mean(commands["sift"])
        speedup = rg_mean / sift_mean
        categories[category].append(speedup)
        if speedup > 1.0:
            wins["sift"] += 1
        elif speedup < 1.0:
            wins["rg"] += 1
        else:
            wins["tie"] += 1

    rows = []
    for category in CATEGORY_ORDER:
        ratios = categories.get(category, [])
        if not ratios:
            continue
        speedup = statistics.median(ratios)
        rows.append(
            {
                "label": category,
                "speedup": speedup,
                "detail": DETAILS[category],
                "benchmarks": len(ratios),
                "status": classify_status(speedup),
            }
        )
    return rows, wins


def build_chart(rows: list[dict], wins: dict[str, int], output: Path) -> None:
    import matplotlib.pyplot as plt
    from matplotlib import patheffects

    colors = {
        "bg": "#f7f6f2",
        "panel": "#fffdf8",
        "text": "#1f2937",
        "muted": "#6b7280",
        "grid": "#ddd6c8",
        "win": "#17624a",
        "near": "#a56a00",
        "loss": "#9f2d2d",
    }

    labels = [row["label"] for row in rows]
    values = [row["speedup"] for row in rows]
    details = [f"{row['detail']} ({row['benchmarks']} benchmarks)" for row in rows]
    bar_colors = [colors[row["status"]] for row in rows]
    x_max = max(6.0, max(values) + 0.5)

    plt.rcParams.update(
        {
            "font.family": "DejaVu Sans",
            "figure.facecolor": colors["bg"],
            "axes.facecolor": colors["panel"],
            "axes.edgecolor": colors["grid"],
            "axes.labelcolor": colors["muted"],
            "xtick.color": colors["muted"],
            "ytick.color": colors["text"],
            "text.color": colors["text"],
        }
    )

    fig, ax = plt.subplots(figsize=(12.5, 7.5), constrained_layout=True)
    y_pos = list(range(len(labels)))
    bars = ax.barh(y_pos, values, color=bar_colors, height=0.5)
    ax.set_yticks(y_pos, labels=labels)
    ax.invert_yaxis()
    ax.set_xlim(0, x_max)
    ax.set_xlabel(
        "Relative speedup for sift (rg mean time / sift mean time)", labelpad=16
    )
    ax.xaxis.grid(True, color=colors["grid"], linewidth=1)
    ax.set_axisbelow(True)

    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    ax.spines["left"].set_color(colors["grid"])
    ax.spines["bottom"].set_color(colors["grid"])

    title = "sift vs rg by search class (Linux corpus)"
    subtitle = f"Generated from benchsuite raw CSV; sift wins {wins['sift']}, rg wins {wins['rg']}, ties {wins['tie']}"
    fig.suptitle(
        title, x=0.08, y=0.98, ha="left", va="top", fontsize=24, fontweight="bold"
    )
    fig.text(
        0.08, 0.92, subtitle, ha="left", va="top", fontsize=12, color=colors["muted"]
    )

    for bar, value, detail, color in zip(
        bars, values, details, bar_colors, strict=True
    ):
        y = bar.get_y() + bar.get_height() / 2
        ax.text(
            min(value + 0.08, x_max - 0.02),
            y,
            f"{value:.1f}x",
            va="center",
            ha="left",
            fontsize=11,
            fontweight="bold",
            color=color,
            path_effects=[
                patheffects.withStroke(linewidth=3, foreground=colors["panel"])
            ],
        )
        ax.text(
            0.01,
            y + 0.33,
            detail,
            transform=ax.get_yaxis_transform(),
            va="center",
            ha="left",
            fontsize=9.5,
            color=colors["muted"],
        )

    legend_handles = [
        plt.Rectangle((0, 0), 1, 1, color=colors["win"]),
        plt.Rectangle((0, 0), 1, 1, color=colors["near"]),
        plt.Rectangle((0, 0), 1, 1, color=colors["loss"]),
    ]
    ax.legend(
        legend_handles,
        ["sift faster", "near parity", "rg faster"],
        loc="lower right",
        frameon=False,
    )

    output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output, format="svg", dpi=160, bbox_inches="tight")
    plt.close(fig)
    clean_svg(output)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate a performance SVG chart from benchsuite raw CSV."
    )
    parser.add_argument("input", type=Path, help="Path to the benchsuite raw CSV")
    parser.add_argument("output", type=Path, help="Path to the output SVG")
    args = parser.parse_args()

    rows, wins = load_results(args.input)
    build_chart(rows, wins, args.output)


if __name__ == "__main__":
    main()
