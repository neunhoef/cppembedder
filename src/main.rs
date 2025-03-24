use clap::Parser;
use std::error::Error;

mod chunking;
mod embedding;
mod embedding_common;
mod importer;

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

    /// Skip the embeddings computation step
    #[clap(short, long)]
    skip_embeddings: bool,

    /// ArangoDB endpoint URL (e.g. "http://localhost:8529")
    #[clap(short, long)]
    arango_endpoint: String,

    /// ArangoDB username
    #[clap(short, long)]
    arango_username: String,

    /// ArangoDB password
    #[clap(short, long)]
    arango_password: String,

    /// ArangoDB database name
    #[clap(short, long)]
    arango_database: String,

    /// ArangoDB collection name
    #[clap(short, long)]
    arango_collection: String,

    /// Path to the LSP communication log file
    #[clap(long, default_value = "lsp_communication.log")]
    lsp_log_file: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create and run the chunker only if not skipped
    if !args.skip_chunking {
        let chunker =
            chunking::Chunker::new(args.project_dir, args.output_dir.clone(), args.clangd_path, args.lsp_log_file.clone());
        chunker.run()?;
    }

    // Create and run the embedder only if not skipped
    if !args.skip_embeddings {
        let embedder = embedding::Embedder::new(args.output_dir.clone(), &args.embedding_model)?;
        embedder.run()?;
    }

    // Create and run the importer
    let importer = importer::Importer::new(
        args.output_dir,
        args.arango_endpoint,
        args.arango_username,
        args.arango_password,
        args.arango_database,
        args.arango_collection,
    );
    importer.run().await?;

    Ok(())
}
