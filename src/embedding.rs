use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::error::Error;
use std::fs;
use walkdir::WalkDir;

pub struct Embedder {
    output_dir: String,
    model: TextEmbedding,
}

impl Embedder {
    pub fn new(output_dir: String, model_name: &str) -> Result<Self, Box<dyn Error>> {
        // Parse the model name into an EmbeddingModel enum
        let model = match model_name {
            "BAAI/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "BAAI/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "BAAI/bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,
            "sentence-transformers/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
            "sentence-transformers/all-MiniLM-L12-v2" => EmbeddingModel::AllMiniLML12V2,
            "sentence-transformers/paraphrase-MiniLM-L6-v2" => {
                EmbeddingModel::ParaphraseMLMiniLML12V2
            }
            "sentence-transformers/paraphrase-mpnet-base-v2" => {
                EmbeddingModel::ParaphraseMLMpnetBaseV2
            }
            "nomic-ai/nomic-embed-text-v1" => EmbeddingModel::NomicEmbedTextV1,
            "nomic-ai/nomic-embed-text-v1.5" => EmbeddingModel::NomicEmbedTextV15,
            "intfloat/multilingual-e5-small" => EmbeddingModel::MultilingualE5Small,
            "intfloat/multilingual-e5-base" => EmbeddingModel::MultilingualE5Base,
            "intfloat/multilingual-e5-large" => EmbeddingModel::MultilingualE5Large,
            "mixedbread-ai/mxbai-embed-large-v1" => EmbeddingModel::MxbaiEmbedLargeV1,
            "Alibaba-NLP/gte-base-en-v1.5" => EmbeddingModel::GTEBaseENV15,
            "Alibaba-NLP/gte-large-en-v1.5" => EmbeddingModel::GTELargeENV15,
            "Qdrant/clip-ViT-B-32-text" => EmbeddingModel::ClipVitB32,
            "jinaai/jina-embeddings-v2-base-code" => EmbeddingModel::JinaEmbeddingsV2BaseCode,
            _ => return Err(format!("Unsupported embedding model: {}", model_name).into()),
        };

        let options = InitOptions::new(model).with_show_download_progress(true);
        let text_embedding = TextEmbedding::try_new(options)?;
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
            let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();

            // Skip _index.txt files
            if file_name.contains("_index.txt") {
                continue;
            }
            if file_name.ends_with(".embedding.json") {
                continue;
            }

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
