use clap::Parser;
use reqwest::Client;
use serde_json::{json, Value};
use std::error::Error;

use cppembedder::embedding_common::create_embedder;

/// Program to query the codebase using semantic search
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// The query to search for
    #[clap(short, long)]
    query: String,

    /// Name of the embedding model to use (e.g. "BAAI/bge-small-en-v1.5")
    #[clap(short, long)]
    embedding_model: String,

    /// ArangoDB endpoint URL (e.g. "http://localhost:8529")
    #[clap(long)]
    arango_endpoint: String,

    /// ArangoDB username
    #[clap(long)]
    arango_username: String,

    /// ArangoDB password
    #[clap(long)]
    arango_password: String,

    /// ArangoDB database name
    #[clap(long)]
    arango_database: String,

    /// ArangoDB collection name
    #[clap(long)]
    arango_collection: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create the embedder
    let embedder = create_embedder(&args.embedding_model)?;

    // Generate embedding for the query
    let query_embedding = embedder.embed(vec![args.query], None)?;
    println!(
        "Generated embedding for query with dimension: {}",
        query_embedding[0].len()
    );

    // Create HTTP client
    let client = Client::new();

    // Prepare the AQL query
    let query_body = json!({
        "query": "FOR doc IN @@chunks LET score = APPROX_NEAR_COSINE(doc.v, @query) SORT score DESC LIMIT 10 RETURN {doc, score}",
        "bindVars": {
            "@chunks": args.arango_collection,
            "query": query_embedding[0]
        }
    });

    // Construct the URL for the cursor API
    let url = format!(
        "{}/_db/{}/_api/cursor",
        args.arango_endpoint, args.arango_database
    );

    // Send the query to ArangoDB
    let response = client
        .post(&url)
        .basic_auth(&args.arango_username, Some(&args.arango_password))
        .json(&query_body)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("ArangoDB query failed: {}", response.text().await?).into());
    }

    let result: Value = response.json().await?;

    // Extract and display results
    if let Some(results) = result.get("result") {
        println!("\nSearch Results:");
        println!("---------------");
        for (i, item) in results.as_array().unwrap().iter().enumerate() {
            let doc = &item["doc"];
            let score = item["score"].as_f64().unwrap();
            let name = doc["name"].as_str().unwrap_or("Unknown");
            println!("{}. {} (Score: {:.4})", i + 1, name, score);
        }
    } else {
        println!("No results found");
    }

    Ok(())
}
