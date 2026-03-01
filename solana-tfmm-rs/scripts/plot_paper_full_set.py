import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

# 学術論文向けのスタイル設定
plt.style.use('seaborn-v0_8-whitegrid')
plt.rcParams.update({
    'font.family': 'serif',
    'axes.labelsize': 12,
    'font.size': 10,
    'legend.fontsize': 10,
    'figure.dpi': 300
})

def plot_all_figures(df):
    # ====================================================
    # Figure A: Cumulative LVR (損失の比較)
    # ====================================================
    fig, ax = plt.subplots(figsize=(8, 5))
    ax.plot(df['slot'], df['vanilla_cum_lvr'], label='Vanilla AMM (Continuous)', color='crimson', linewidth=2)
    ax.plot(df['slot'], df['pfda_cum_lvr'], label='PFDA-TFMM (Ours)', color='royalblue', linewidth=2)
    ax.set_xlabel('Slot Time (400ms)')
    ax.set_ylabel('Cumulative LVR (USD)')
    ax.set_title('Cumulative Loss-Versus-Rebalancing')
    ax.legend()
    fig.tight_layout()
    fig.savefig('../figures/paper/figA_cumulative_lvr.png')
    
    # ====================================================
    # Figure B: Clearing Dynamics (ミクロ構造と価格のサヤ寄せ)
    # ====================================================
    df_zoom = df.head(150)
    fig, ax = plt.subplots(figsize=(8, 5))
    ax.plot(df_zoom['slot'], df_zoom['market_price'], label='Market Price (External)', color='gray', linestyle='--', alpha=0.7)
    ax.step(df_zoom['slot'], df_zoom['vanilla_price'], label='Vanilla Price', color='crimson', alpha=0.4)
    ax.step(df_zoom['slot'], df_zoom['pfda_price'], label='PFDA Implied Price', color='royalblue', linewidth=2, where='post')
    ax.set_xlabel('Slot')
    ax.set_ylabel('Price (B/A)')
    ax.set_title('Micro-structure: Batch Clearing Dynamics')
    ax.legend()
    fig.tight_layout()
    fig.savefig('../figures/paper/figB_clearing_dynamics.png')

    # ====================================================
    # Figure C: Extracted Value Distribution (1取引あたりの抜かれた額)
    # ====================================================
    v_arbs = df[df['vanilla_arb_profit'] > 1e-6]['vanilla_arb_profit']
    p_arbs = df[df['pfda_arb_profit'] > 1e-6]['pfda_arb_profit']
    fig, ax = plt.subplots(figsize=(8, 5))
    sns.kdeplot(v_arbs, fill=True, label='Vanilla AMM', color='crimson', ax=ax, log_scale=True)
    sns.kdeplot(p_arbs, fill=True, label='PFDA-TFMM', color='royalblue', ax=ax, log_scale=True)
    ax.set_xlabel('Value Extracted per Trade (USD, Log Scale)')
    ax.set_ylabel('Density')
    ax.set_title('Distribution of Extracted Value (MEV)')
    ax.legend()
    fig.tight_layout()
    fig.savefig('../figures/paper/figC_extracted_value_dist.png')

    # ====================================================
    # Figure D: Portfolio Value vs HODL (ETFとしての運用成績) ★大本命★
    # ====================================================
    fig, ax = plt.subplots(figsize=(8, 5))
    # 理想ポートフォリオ（手数料ゼロでリバランス）からの乖離を見せる
    ax.plot(df['slot'], df['ideal_portfolio_value'], label='Ideal Rebalanced Portfolio (0 fee)', color='green', linestyle=':')
    ax.plot(df['slot'], df['hodl_value'], label='HODL (50/50 Initial)', color='gray', linestyle='--')
    ax.plot(df['slot'], df['vanilla_pool_value'], label='Vanilla AMM TVL', color='crimson', linewidth=1.5)
    ax.plot(df['slot'], df['pfda_pool_value'], label='PFDA-TFMM TVL', color='royalblue', linewidth=2)
    
    ax.set_xlabel('Slot Time (400ms)')
    ax.set_ylabel('Portfolio Value (USD)')
    ax.set_title('Cumulative Performance vs Benchmarks')
    ax.legend()
    fig.tight_layout()
    fig.savefig('../figures/paper/figD_portfolio_value.png')

    # ====================================================
    # Figure E: Arbitrage Inter-trade Delay (アービトラージ間隔)
    # ====================================================
    v_gaps = df[df['vanilla_arb_gap'] > 0]['vanilla_arb_gap']
    p_gaps = df[df['pfda_arb_gap'] > 0]['pfda_arb_gap']
    
    fig, ax = plt.subplots(figsize=(8, 5))
    sns.histplot(v_gaps, bins=30, color='crimson', alpha=0.5, label='Vanilla AMM (Latency driven)')
    sns.histplot(p_gaps, bins=30, color='royalblue', alpha=0.7, label='PFDA-TFMM (Batch driven)')
    ax.set_xlabel('Slots Between Arbitrage Trades')
    ax.set_ylabel('Frequency')
    ax.set_title('Inter-trade Delay Distribution')
    ax.legend()
    fig.tight_layout()
    fig.savefig('../figures/paper/figE_arb_gap_dist.png')

if __name__ == "__main__":
    try:
        df = pd.read_csv('../results/timeseries_log.csv')
        plot_all_figures(df)
        print("✅ 5 Academic Figures generated in figures/paper/")
    except FileNotFoundError:
        print("Error: Please run 'cargo run -- sim' first.")