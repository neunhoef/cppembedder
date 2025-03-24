use fastembed::TextEmbedding;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::error::Error;
use std::fs;
use walkdir::WalkDir;

use crate::embedding_common::create_embedder;

pub struct Embedder {
    output_dir: String,
    model: TextEmbedding,
}

impl Embedder {
    pub fn new(output_dir: String, model_name: &str) -> Result<Self, Box<dyn Error>> {
        let text_embedding = create_embedder(model_name)?;
        Ok(Self {
            output_dir,
            model: text_embedding,
        })
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let entries: Vec<_> = WalkDir::new(&self.output_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_name().to_string_lossy().ends_with(".cpp")
                    || entry.file_name().to_string_lossy().ends_with(".hpp")
            })
            .collect();

        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );

        for entry in entries {
            let file_path = entry.path();

            // Read the file content
            let content = fs::read_to_string(&file_path)?;

            // Generate embedding
            let embedding = self.model.embed(vec![content], None)?;

            // Create output path for the embedding JSON
            let embedding_path = file_path.with_extension("embedding.json");

            // Convert embedding to JSON
            let json_data = json!({
                "v": embedding[0]
            });

            // Write the JSON file
            fs::write(&embedding_path, serde_json::to_string_pretty(&json_data)?)?;

            pb.inc(1);
        }

        pb.finish_with_message("Embedding generation complete");
        Ok(())
    }
}
