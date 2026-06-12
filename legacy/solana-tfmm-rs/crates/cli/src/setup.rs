use anyhow::Result;
use std::path::PathBuf;

pub struct SetupOptions {
    pub yes: bool,
    pub skip_build: bool,
    pub project_dir: PathBuf,
}

pub fn run(opts: SetupOptions) -> Result<()> {
    println!(
        "setup placeholder: yes={}, skip_build={}, dir={}",
        opts.yes,
        opts.skip_build,
        opts.project_dir.display()
    );
    Ok(())
}
