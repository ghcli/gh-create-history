mod cli;
mod content;
mod engine;
mod git_ops;
mod merge;
mod progress;
mod timestamps;
mod topology;

use anyhow::Result;
use clap::Parser;
use cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    engine::run(args)
}

