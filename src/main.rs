use clap::Parser;
use std::error::Error;

mod chunking;
mod embedding;

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

    /// Name of the embedding model to use (e.g. "BAAI/bge-small-en-v1.5")
    #[clap(short, long)]
    embedding_model: String,

    /// Skip the chunking step and assume it has already been done
    #[clap(short, long)]
    skip_chunking: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create and run the chunker only if not skipped
    if !args.skip_chunking {
        let chunker =
            chunking::Chunker::new(args.project_dir, args.output_dir.clone(), args.clangd_path);
        chunker.run()?;
    }

    // Create and run the embedder
    let embedder = embedding::Embedder::new(args.output_dir, &args.embedding_model)?;
    embedder.run()?;

    Ok(())
}
