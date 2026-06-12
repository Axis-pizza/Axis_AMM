mod commands;
mod setup;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tfmm-cli")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Setup {
        #[arg(short='y', long)]
        yes: bool,
        #[arg(long)]
        skip_build: bool,
        #[arg(long, default_value = ".")]
        project_dir: PathBuf,
    },
    Sim,
    Real {
        #[arg(long)]
        pool: String,
    },
    Live {
        #[arg(long)]
        pool: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup { yes, skip_build, project_dir } => {
            let opts = setup::SetupOptions { yes, skip_build, project_dir };
            setup::run(opts)?;
        }
        Commands::Sim => commands::run_sim()?,
        Commands::Real { pool } => commands::run_real(&pool)?,
        Commands::Live { pool } => commands::run_live(&pool)?,
    }

    Ok(())
}
