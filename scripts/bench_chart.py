#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def load_snapshot(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def build_chart(snapshot: dict, output: Path) -> None:
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

    categories = snapshot["categories"]
    labels = [item["label"] for item in categories]
    values = [float(item["speedup"]) for item in categories]
    bar_colors = [colors[item["status"]] for item in categories]
    details = [item["detail"] for item in categories]
    x_max = float(snapshot.get("x_max", max(values)))

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
    fig.patch.set_facecolor(colors["bg"])
    ax.set_facecolor(colors["panel"])

    y_pos = list(range(len(labels)))
    bars = ax.barh(y_pos, values, color=bar_colors, height=0.5)

    ax.set_yticks(y_pos, labels=labels)
    ax.invert_yaxis()
    ax.set_xlim(0, x_max)
    ax.set_xlabel("Relative speedup for sift (rg time / sift time)", labelpad=16)
    ax.xaxis.grid(True, color=colors["grid"], linewidth=1)
    ax.set_axisbelow(True)

    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    ax.spines["left"].set_color(colors["grid"])
    ax.spines["bottom"].set_color(colors["grid"])

    title = snapshot["title"]
    subtitle = snapshot["subtitle"]
    fig.suptitle(
        title, x=0.08, y=0.98, ha="left", va="top", fontsize=24, fontweight="bold"
    )
    fig.text(
        0.08, 0.92, subtitle, ha="left", va="top", fontsize=12, color=colors["muted"]
    )

    for idx, (bar, value, detail, color) in enumerate(
        zip(bars, values, details, bar_colors, strict=True)
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


def clean_svg(path: Path) -> None:
    svg = path.read_text(encoding="utf-8")
    svg = re.sub(r"<!DOCTYPE[^>]*>\s*", "", svg, count=1)
    svg = re.sub(r"<metadata>.*?</metadata>\s*", "", svg, count=1, flags=re.S)
    path.write_text(svg, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate a benchmark SVG chart from a snapshot JSON file."
    )
    parser.add_argument("input", type=Path, help="Path to the benchmark snapshot JSON")
    parser.add_argument("output", type=Path, help="Path to the output SVG")
    args = parser.parse_args()

    snapshot = load_snapshot(args.input)
    build_chart(snapshot, args.output)


if __name__ == "__main__":
    main()
