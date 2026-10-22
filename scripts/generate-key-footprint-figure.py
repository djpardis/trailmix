"""Generate the website-styled key accuracy footprint figure.

Footprint values come from the ICASSP evidence JSON. Accuracy values match the
paper's FMAKv2 MIREX table and surrounding same-manifest evidence.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt
from matplotlib.ticker import FixedLocator, FuncFormatter

COLORS = {
    "ink": "#171517",
    "muted": "#292629",
    "cranberry": "#a93649",
    "cranberry_dark": "#802938",
    "raisin": "#5d4861",
    "cocoa": "#8a5a3c",
    "olive": "#6f733f",
    "almond": "#f3ede3",
    "paper": "#fffaf2",
    "grid": "#e7ded2",
}

# MIREX percentages from paper/main.tex and same-manifest evidence notes.
MIREX = {
    "kl_dsp": 61.08,
    "kl_fmak": 73.55,
    "stratum_dsp": 55.09,
    "libkeyfinder": 67.46,
    "madmom_keycnn": 74.16,
    "skey": 73.20,
}

LABELS = {
    "kl_dsp": "KL-DSP",
    "kl_fmak": "KL-FMAK",
    "stratum_dsp": "stratum-dsp",
    "libkeyfinder": "libKeyFinder",
    "madmom_keycnn": "madmom local",
    "skey": "S-KEY 1M",
}

MARKERS = {
    "kl_dsp": "s",
    "kl_fmak": "o",
    "stratum_dsp": "P",
    "libkeyfinder": "v",
    "madmom_keycnn": "D",
    "skey": "^",
}

POINT_COLORS = {
    "kl_dsp": COLORS["raisin"],
    "kl_fmak": COLORS["cranberry"],
    "stratum_dsp": COLORS["olive"],
    "libkeyfinder": COLORS["cocoa"],
    "madmom_keycnn": COLORS["muted"],
    "skey": COLORS["ink"],
}

OFFSETS = {
    "kl_dsp": (-26, 14),
    "kl_fmak": (-42, -26),
    "stratum_dsp": (10, -8),
    "libkeyfinder": (12, 0),
    "madmom_keycnn": (-46, 28),
    "skey": (-24, -30),
}


def mb_label(value: float) -> str:
    if value < 1:
        return f"{value:.2g} MB"
    if value < 10:
        return f"{value:.1f} MB"
    return f"{value:.0f} MB"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--footprint", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()

    footprint = json.loads(arguments.footprint.read_text())
    total_mb = footprint["total_footprint_mb"]

    plt.rcParams.update(
        {
            "font.family": "sans-serif",
            "font.sans-serif": ["DM Sans", "Arial", "DejaVu Sans"],
            "axes.linewidth": 1.4,
            "svg.fonttype": "none",
        }
    )

    fig, ax = plt.subplots(figsize=(8.2, 5.4), dpi=160)
    fig.patch.set_facecolor(COLORS["paper"])
    ax.set_facecolor(COLORS["paper"])

    for system, score in MIREX.items():
        x = total_mb[system]
        ax.scatter(
            x,
            score,
            s=150 if system == "kl_fmak" else 118,
            marker=MARKERS[system],
            color=POINT_COLORS[system],
            edgecolor=COLORS["ink"],
            linewidth=1.2,
            zorder=4 if system == "kl_fmak" else 3,
        )
        dx, dy = OFFSETS[system]
        ax.annotate(
            f"{LABELS[system]}\n{mb_label(x)}",
            xy=(x, score),
            xytext=(dx, dy),
            textcoords="offset points",
            ha="left" if dx >= 0 else "right",
            va="center",
            fontsize=10.5,
            fontweight="700" if system == "kl_fmak" else "500",
            color=POINT_COLORS[system],
            linespacing=1.05,
        )

    ax.set_xscale("log")
    ax.set_xlim(0.5, 800)
    ax.set_ylim(52.5, 78.5)
    ax.xaxis.set_major_locator(FixedLocator([1, 10, 100, 1000]))
    ax.xaxis.set_major_formatter(FuncFormatter(lambda value, _pos: f"{value:g}"))
    ax.yaxis.set_major_locator(FixedLocator([55, 60, 65, 70, 75]))

    ax.grid(True, which="major", color=COLORS["grid"], linewidth=1.0)
    ax.tick_params(colors=COLORS["muted"], labelsize=10.5, length=0)
    for spine in ax.spines.values():
        spine.set_color(COLORS["raisin"])

    ax.set_xlabel("Installed footprint (MB, log scale)", color=COLORS["ink"], fontsize=12, fontweight="700", labelpad=10)
    ax.set_ylabel("FMAKv2 MIREX key score (%)", color=COLORS["ink"], fontsize=12, fontweight="700", labelpad=10)
    ax.set_title("Key accuracy versus installed footprint", color=COLORS["ink"], fontsize=16, fontweight="700", pad=14)

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    fig.tight_layout(pad=1.0)
    fig.savefig(arguments.output, format="svg", bbox_inches="tight", facecolor=fig.get_facecolor())
    plt.close(fig)


if __name__ == "__main__":
    main()
