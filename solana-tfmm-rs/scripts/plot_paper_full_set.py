import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import numpy as np

# 学術論文向けのクリーンなスタイル設定
plt.style.use('seaborn-v0_8-paper')
plt.rcParams.update({
    'font.family': 'serif',
    'axes.labelsize': 11,
    'xtick.labelsize': 10,
    'ytick.labelsize': 10,
    'legend.fontsize': 10,
    'legend.frameon': False,
    'figure.dpi': 300,
    'axes.spines.top': False,
    'axes.spines.right': False,
})

def plot_timeseries_figures(df):
    df['vanilla_rel'] = df['vanilla_pool_value'] - df['hodl_value']
    df['pfda_rel'] = df['pfda_pool_value'] - df['hodl_value']
    df['ideal_rel'] = df['ideal_portfolio_value'] - df['hodl_value']
    
    # ---------------------------------------------------
    # Fig 1: Micro-structure (Price Tracking)
    # ---------------------------------------------------
    fig, ax = plt.subplots(figsize=(6, 4))
    df_zoom = df.head(150)
    
    ax.plot(df_zoom['slot'], df_zoom['market_price'], label='Market Price', color='gray', linestyle='--', alpha=0.7)
    ax.plot(df_zoom['slot'], df_zoom['vanilla_price'], label='Vanilla', color='#d62728', alpha=0.6)
    ax.step(df_zoom['slot'], df_zoom['pfda_price'], label='PFDA-TFMM', color='#1f77b4', where='post', linewidth=1.5)
    
    ax.set_xlabel('Slot')
    ax.set_ylabel('Price')
    ax.legend(loc='upper left')
    fig.tight_layout()
    fig.savefig('../figures/paper/fig1_price_tracking.pdf', format='pdf', bbox_inches='tight')
    
    # ---------------------------------------------------
    # Fig 2: Cumulative Performance (vs HODL)
    # ---------------------------------------------------
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.axhline(0, color='gray', linestyle='-', linewidth=0.8)
    ax.plot(df['slot'], df['vanilla_rel'], label='Vanilla', color='#d62728', linewidth=1.2)
    ax.plot(df['slot'], df['pfda_rel'], label='PFDA-TFMM', color='#1f77b4', linewidth=1.5)
    ax.plot(df['slot'], df['ideal_rel'], label='Ideal Rebalanced', color='#2ca02c', linestyle=':', linewidth=1.5)
    
    ax.set_xlabel('Slot')
    ax.set_ylabel('Performance vs HODL (USD)')
    ax.legend(loc='upper left')
    fig.tight_layout()
    fig.savefig('../figures/paper/fig2_performance.pdf', format='pdf', bbox_inches='tight')
    
    # ---------------------------------------------------
    # Fig 3: Inter-trade Delay
    # ---------------------------------------------------
    fig, ax = plt.subplots(figsize=(6, 4))
    v_gaps = df[df['vanilla_arb_gap'] > 0]['vanilla_arb_gap']
    p_gaps = df[df['pfda_arb_gap'] > 0]['pfda_arb_gap']
    
    bins = np.arange(0, 60, 2)
    ax.hist(v_gaps, bins=bins, alpha=0.5, label='Vanilla', color='#d62728', density=True)
    ax.hist(p_gaps, bins=bins, alpha=0.7, label='PFDA-TFMM', color='#1f77b4', density=True)
    
    ax.set_xlabel('Slots Between Trades')
    ax.set_ylabel('Density')
    ax.legend(loc='upper right')
    fig.tight_layout()
    fig.savefig('../figures/paper/fig3_delay.pdf', format='pdf', bbox_inches='tight')

def plot_sweep_and_revenue(df_sweep):
    # ---------------------------------------------------
    # Fig 4: Revenue Breakdown (Stacked Bar)
    # ---------------------------------------------------
    # VanillaとPFDA（代表的な1行）を抽出
    baseline = df_sweep.iloc[0]
    pfda_best = df_sweep[(df_sweep['alpha'] == 0.75) & (df_sweep['window_slots'] == 10)].iloc[0]

    labels = ['Vanilla AMM', 'PFDA-TFMM\n(10 slots, α=0.75)']
    val_rev = [baseline['vanilla_total_validator_searcher_revenue_usd'], pfda_best['pfda_total_validator_searcher_revenue_usd']]
    prot_rev = [baseline['vanilla_total_protocol_revenue_usd'], pfda_best['pfda_total_protocol_revenue_usd']]
    
    fig, ax = plt.subplots(figsize=(5, 5))
    width = 0.5
    ax.bar(labels, val_rev, width, label='MEV Leakage (Validators/Searchers)', color='#d62728', alpha=0.8)
    ax.bar(labels, prot_rev, width, bottom=val_rev, label='Protocol Revenue (Internalized)', color='#2ca02c', alpha=0.8)
    
    ax.set_ylabel('Cumulative Value Extracted (USD)')
    ax.legend(loc='upper right', bbox_to_anchor=(1.0, 1.15))
    fig.tight_layout()
    fig.savefig('../figures/paper/fig4_revenue_split.pdf', format='pdf', bbox_inches='tight')

    # ---------------------------------------------------
    # Fig 5: Parameter Sensitivity Heatmap
    # ---------------------------------------------------
    # 特定の割引率(例:1.25bps)に固定して、window_slots と alpha のマトリクスを作る
    df_heat = df_sweep[df_sweep['fee_discount_bps'] == 1.25].copy()
    if not df_heat.empty:
        # LVR削減額をクリッピング（極端な外れ値を抑えて色を見やすくする）
        df_heat['lvr_reduction_usd_clip'] = df_heat['lvr_reduction_usd'].clip(lower=0)
        pivot = df_heat.pivot(index='alpha', columns='window_slots', values='lvr_reduction_usd_clip')
        
        fig, ax = plt.subplots(figsize=(6, 4))
        sns.heatmap(pivot, annot=True, fmt=".0f", cmap="YlGnBu", ax=ax, cbar_kws={'label': 'LVR Reduction (USD)'})
        ax.set_xlabel('Batch Window (Slots)')
        ax.set_ylabel('Competitiveness (α)')
        ax.invert_yaxis() # y軸を下から上へ
        fig.tight_layout()
        fig.savefig('../figures/paper/fig5_heatmap.pdf', format='pdf', bbox_inches='tight')

if __name__ == "__main__":
    import os
    if not os.path.exists('../figures/paper'):
        os.makedirs('../figures/paper')
        
    try:
        df_ts = pd.read_csv('../results/timeseries_log.csv')
        plot_timeseries_figures(df_ts)
        
        df_sw = pd.read_csv('../results/pfda_sweep_summary.csv')
        plot_sweep_and_revenue(df_sw)
        
        print("✅ All 5 academic figures generated successfully as PDFs.")
    except Exception as e:
        print(f"Error generating figures: {e}")