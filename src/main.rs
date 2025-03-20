use clap::Parser;
use std::error::Error;

mod chunking;

/// Program to chunk C++ source files based on function/class/method boundaries using clangd
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Root directory of the C++ project
    #[clap(short, long)]
    project_dir: String,

    /// Output directory for the chunked files
    #[clap(short, long, default_value = "chunked_output")]
    output_dir: String,

    /// Path to clangd executable
    #[clap(short, long, default_value = "clangd")]
    clangd_path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create and run the chunker
    let chunker = chunking::Chunker::new(
        args.project_dir,
        args.output_dir,
        args.clangd_path,
    );
    chunker.run()?;

    Ok(())
}
