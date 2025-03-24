use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::error::Error;

pub fn create_embedder(model_name: &str) -> Result<TextEmbedding, Box<dyn Error>> {
    // Parse the model name into an EmbeddingModel enum
    let model = match model_name {
        "BAAI/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
        "BAAI/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
        "BAAI/bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,
        "sentence-transformers/all-MiniLM-L6-v2" => EmbeddingModel::AllMiniLML6V2,
        "sentence-transformers/all-MiniLM-L12-v2" => EmbeddingModel::AllMiniLML12V2,
        "sentence-transformers/paraphrase-MiniLM-L6-v2" => EmbeddingModel::ParaphraseMLMiniLML12V2,
        "sentence-transformers/paraphrase-mpnet-base-v2" => EmbeddingModel::ParaphraseMLMpnetBaseV2,
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
    TextEmbedding::try_new(options).map_err(|e| e.into())
} 