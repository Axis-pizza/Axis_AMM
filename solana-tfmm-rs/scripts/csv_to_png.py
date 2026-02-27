#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
Batch figure generator for Solana TFMM / PFDA diagnostics.

Reads CSVs under --in_dir (default: results/)
Writes PNGs under --out_dir (default: figures/)

Expected CSVs (if present, plot them):
- edge_dt_points.csv      : scatter dt_ms vs edge_bps
- edge_buckets.csv        : bar count per edge bucket + mean_edge overlay (optional)
- dt_buckets.csv          : bar count per dt bucket + mean/median edge
- slot_edge_aggs.csv      : time-series like by slot: mean_edge, trade_count, total_notional
- matched_trades.csv      : hist edge_bps, hist fee_usd_proxy, scatter edge vs fee, edge vs notional
- real_summary.csv        : not plotted (single-row summary), but can print quick stats

All plots saved as PNG.
"""

import argparse
import os
from pathlib import Path

import pandas as pd
import numpy as np
import matplotlib.pyplot as plt


def ensure_dir(p: Path) -> None:
    p.mkdir(parents=True, exist_ok=True)


def savefig(path: Path, dpi: int = 200) -> None:
    plt.tight_layout()
    plt.savefig(path, dpi=dpi, bbox_inches="tight")
    plt.close()


def try_read_csv(path: Path) -> pd.DataFrame | None:
    if not path.exists():
        print(f"[skip] missing: {path}")
        return None
    try:
        df = pd.read_csv(path)
        if df.empty:
            print(f"[skip] empty: {path}")
            return None
        print(f"[load] {path} rows={len(df)} cols={list(df.columns)}")
        return df
    except Exception as e:
        print(f"[skip] failed to read {path}: {e}")
        return None


def plot_edge_dt_scatter(df: pd.DataFrame, out: Path) -> None:
    # columns: dt_ms, edge_bps (based on your log wrote edge_dt_points.csv)
    # fallback names if different
    xcol_candidates = ["match_dt_ms", "dt_ms", "abs_time_diff_ms", "time_diff_ms"]
    ycol_candidates = ["edge_bps", "edge_bps_abs", "edge"]

    xcol = next((c for c in xcol_candidates if c in df.columns), None)
    ycol = next((c for c in ycol_candidates if c in df.columns), None)

    if xcol is None or ycol is None:
        print("[skip] edge_dt_scatter: required cols not found")
        return

    plt.figure(figsize=(7.5, 5.2))
    plt.scatter(df[xcol], df[ycol], s=18, alpha=0.70)
    plt.xlabel("Match time difference (ms)")
    plt.ylabel("Edge (bps)")
    plt.title("Edge vs. latency (MEV speed competition proxy)")
    plt.grid(True, alpha=0.25)

    # Optional: trend line (robust-ish)
    try:
        x = df[xcol].to_numpy(dtype=float)
        y = df[ycol].to_numpy(dtype=float)
        if len(x) >= 5 and np.isfinite(x).all() and np.isfinite(y).all():
            coeff = np.polyfit(x, y, deg=1)
            xs = np.linspace(x.min(), x.max(), 200)
            ys = coeff[0] * xs + coeff[1]
            plt.plot(xs, ys, linewidth=2.0)
    except Exception:
        pass

    savefig(out / "edge_dt_scatter.png")


def plot_edge_buckets(df: pd.DataFrame, out: Path) -> None:
    # columns (your export): bucket,count,mean_edge_bps,mean_notional_usdc,mean_fee_usd_proxy,...
    if "bucket" not in df.columns or "count" not in df.columns:
        print("[skip] edge_buckets: required cols not found")
        return

    # preserve bucket order if looks like [0,1), [1,2), ...
    bucket_order = df["bucket"].tolist()

    counts = df["count"].astype(float).to_numpy()
    plt.figure(figsize=(8.0, 4.8))
    plt.bar(bucket_order, counts)
    plt.xlabel("Edge bucket (bps)")
    plt.ylabel("Count")
    plt.title("Trade frequency by edge bucket")
    plt.grid(True, axis="y", alpha=0.25)
    savefig(out / "edge_buckets_count.png")

    # mean_edge overlay (if present)
    if "mean_edge_bps" in df.columns:
        mean_edge = df["mean_edge_bps"].astype(float).to_numpy()
        plt.figure(figsize=(8.0, 4.8))
        plt.bar(bucket_order, counts, alpha=0.70, label="count")
        ax2 = plt.gca().twinx()
        ax2.plot(bucket_order, mean_edge, marker="o", linewidth=2.0, label="mean_edge_bps")
        plt.xlabel("Edge bucket (bps)")
        plt.title("Trade frequency + mean edge by bucket")
        plt.grid(True, axis="y", alpha=0.20)
        savefig(out / "edge_buckets_count_meanedge.png")


def plot_dt_buckets(df: pd.DataFrame, out: Path) -> None:
    # columns from your log: dt_bucket,count,mean_edge,med_edge,mean_notnl,mean_fee,dt_min,dt_max
    if "dt_bucket" not in df.columns or "count" not in df.columns:
        print("[skip] dt_buckets: required cols not found")
        return

    order = df["dt_bucket"].tolist()
    counts = df["count"].astype(float).to_numpy()

    plt.figure(figsize=(8.5, 4.8))
    plt.bar(order, counts)
    plt.xlabel("Latency bucket (ms)")
    plt.ylabel("Count")
    plt.title("Trade frequency by latency bucket")
    plt.grid(True, axis="y", alpha=0.25)
    savefig(out / "dt_buckets_count.png")

    # mean/median edge (if present)
    if "mean_edge" in df.columns and "med_edge" in df.columns:
        mean_edge = df["mean_edge"].astype(float).to_numpy()
        med_edge = df["med_edge"].astype(float).to_numpy()

        plt.figure(figsize=(8.5, 4.8))
        plt.plot(order, mean_edge, marker="o", linewidth=2.0, label="mean_edge_bps")
        plt.plot(order, med_edge, marker="s", linewidth=2.0, label="median_edge_bps")
        plt.xlabel("Latency bucket (ms)")
        plt.ylabel("Edge (bps)")
        plt.title("Edge level by latency bucket")
        plt.grid(True, alpha=0.25)
        plt.legend()
        savefig(out / "dt_buckets_edge.png")


def plot_slot_edge_aggs(df: pd.DataFrame, out: Path) -> None:
    # columns (your export): slot,trade_count,mean_edge_bps,min_edge_bps,max_edge_bps,total_notional_usdc,mean_match_dt_ms
    required = {"slot", "trade_count", "mean_edge_bps"}
    if not required.issubset(set(df.columns)):
        print("[skip] slot_edge_aggs: required cols not found")
        return

    d = df.copy()
    d["slot"] = d["slot"].astype(int)
    d = d.sort_values("slot")

    # mean edge over slot index (not absolute time, but still good story)
    plt.figure(figsize=(10.0, 4.8))
    plt.plot(d["slot"], d["mean_edge_bps"], linewidth=1.8)
    plt.xlabel("Slot")
    plt.ylabel("Mean edge (bps)")
    plt.title("Slot-level mean edge (time series proxy)")
    plt.grid(True, alpha=0.25)
    savefig(out / "slot_mean_edge_timeseries.png")

    # trade_count over slot
    plt.figure(figsize=(10.0, 4.8))
    plt.plot(d["slot"], d["trade_count"], linewidth=1.8)
    plt.xlabel("Slot")
    plt.ylabel("Trades per slot")
    plt.title("Trade intensity per slot")
    plt.grid(True, alpha=0.25)
    savefig(out / "slot_trade_count_timeseries.png")

    # total_notional (if present)
    if "total_notional_usdc" in d.columns:
        plt.figure(figsize=(10.0, 4.8))
        plt.plot(d["slot"], d["total_notional_usdc"], linewidth=1.8)
        plt.xlabel("Slot")
        plt.ylabel("Total notional (USDC)")
        plt.title("Total notional per slot")
        plt.grid(True, alpha=0.25)
        savefig(out / "slot_total_notional_timeseries.png")


def plot_matched_trades(df: pd.DataFrame, out: Path) -> None:
    # columns you wrote:
    # slot,tx_ts_sec,exec_price_usdc_per_sol,ext_price_usd_per_sol,edge_bps,match_time_diff_ms,notional_usdc,fee_lamports,fee_usd_proxy,edge_usd_proxy
    if "edge_bps" in df.columns:
        plt.figure(figsize=(7.5, 4.8))
        plt.hist(df["edge_bps"].astype(float), bins=30)
        plt.xlabel("Edge (bps)")
        plt.ylabel("Count")
        plt.title("Edge distribution (matched trades)")
        plt.grid(True, axis="y", alpha=0.25)
        savefig(out / "edge_hist.png")

    if "fee_usd_proxy" in df.columns:
        plt.figure(figsize=(7.5, 4.8))
        plt.hist(df["fee_usd_proxy"].astype(float), bins=30)
        plt.xlabel("Fee (USD proxy)")
        plt.ylabel("Count")
        plt.title("Fee distribution (USD proxy)")
        plt.grid(True, axis="y", alpha=0.25)
        savefig(out / "fee_usd_proxy_hist.png")

    # scatter edge vs fee
    if "edge_bps" in df.columns and "fee_usd_proxy" in df.columns:
        plt.figure(figsize=(7.5, 5.2))
        plt.scatter(df["fee_usd_proxy"].astype(float), df["edge_bps"].astype(float), s=18, alpha=0.70)
        plt.xlabel("Fee (USD proxy)")
        plt.ylabel("Edge (bps)")
        plt.title("Edge vs fee (proxy)")
        plt.grid(True, alpha=0.25)
        savefig(out / "edge_vs_fee_scatter.png")

    # scatter edge vs notional
    if "edge_bps" in df.columns and "notional_usdc" in df.columns:
        plt.figure(figsize=(7.5, 5.2))
        plt.scatter(df["notional_usdc"].astype(float), df["edge_bps"].astype(float), s=18, alpha=0.70)
        plt.xlabel("Notional (USDC)")
        plt.ylabel("Edge (bps)")
        plt.title("Edge vs notional")
        plt.grid(True, alpha=0.25)
        savefig(out / "edge_vs_notional_scatter.png")

    # scatter dt vs edge (duplicate if edge_dt_points exists, but okay)
    if "match_time_diff_ms" in df.columns and "edge_bps" in df.columns:
        plt.figure(figsize=(7.5, 5.2))
        plt.scatter(df["match_time_diff_ms"].astype(float), df["edge_bps"].astype(float), s=18, alpha=0.70)
        plt.xlabel("Match time difference (ms)")
        plt.ylabel("Edge (bps)")
        plt.title("Edge vs latency (from matched_trades)")
        plt.grid(True, alpha=0.25)
        savefig(out / "edge_vs_dt_scatter_from_matched.png")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--in_dir", default="results", help="input directory containing CSVs")
    ap.add_argument("--out_dir", default="figures", help="output directory for PNGs")
    ap.add_argument("--dpi", type=int, default=220)
    args = ap.parse_args()

    in_dir = Path(args.in_dir)
    out_dir = Path(args.out_dir)
    ensure_dir(out_dir)

    # 1) Primary “pitch/paper” figures
    df_edge_dt = try_read_csv(in_dir / "edge_dt_points.csv")
    if df_edge_dt is not None:
        plot_edge_dt_scatter(df_edge_dt, out_dir)

    df_edge_buckets = try_read_csv(in_dir / "edge_buckets.csv")
    if df_edge_buckets is not None:
        plot_edge_buckets(df_edge_buckets, out_dir)

    df_dt_buckets = try_read_csv(in_dir / "dt_buckets.csv")
    if df_dt_buckets is not None:
        plot_dt_buckets(df_dt_buckets, out_dir)

    # 2) Extra diagnostic figures (handy for appendix/validation)
    df_slot = try_read_csv(in_dir / "slot_edge_aggs.csv")
    if df_slot is not None:
        plot_slot_edge_aggs(df_slot, out_dir)

    df_matched = try_read_csv(in_dir / "matched_trades.csv")
    if df_matched is not None:
        plot_matched_trades(df_matched, out_dir)

    # 3) Print quick stats if summary exists
    df_summary = try_read_csv(in_dir / "real_summary.csv")
    if df_summary is not None and len(df_summary) >= 1:
        row = df_summary.iloc[0].to_dict()
        print("\n[summary] real_summary.csv (first row)")
        for k in sorted(row.keys()):
            print(f"  {k}: {row[k]}")

    print(f"\n[done] wrote figures to: {out_dir.resolve()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())