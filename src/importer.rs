use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde_json::{json, Value};
use std::error::Error;
use std::fs;
use walkdir::WalkDir;

const BATCH_SIZE: usize = 100;

pub struct Importer {
    output_dir: String,
    client: Client,
    endpoint: String,
    username: String,
    password: String,
    database: String,
    collection: String,
}

#[derive(Debug)]
struct Document {
    name: String,
    v: Vec<f32>,
    src: String,
}

impl Importer {
    pub fn new(
        output_dir: String,
        endpoint: String,
        username: String,
        password: String,
        database: String,
        collection: String,
    ) -> Self {
        Self {
            output_dir,
            client: Client::new(),
            endpoint,
            username,
            password,
            database,
            collection,
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        // Read all JSON files from the output directory recursively
        let entries: Vec<_> = WalkDir::new(&self.output_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".cpp"))
            .collect();

        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%)")
                .unwrap()
                .progress_chars("#>-"),
        );

        let mut current_batch = Vec::with_capacity(BATCH_SIZE);

        for entry in entries {
            let file_path = entry.path();
            let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();
            let json_file_path = file_path.with_extension("embedding.json");

            let content = fs::read_to_string(&file_path)?;
            let json_content = fs::read_to_string(&json_file_path)?;
            let json: Value = serde_json::from_str(&json_content)?;

            let document = Document {
                name: file_name.to_string(),
                v: json["v"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect(),
                src: content,
            };

            current_batch.push(document);

            if current_batch.len() >= BATCH_SIZE {
                self.import_batch(&current_batch).await?;
                current_batch.clear();
            }

            pb.inc(1);
        }

        // Import any remaining documents
        if !current_batch.is_empty() {
            self.import_batch(&current_batch).await?;
        }

        pb.finish_with_message("Import completed");
        Ok(())
    }

    async fn import_batch(&self, documents: &[Document]) -> Result<(), Box<dyn Error>> {
        let url = format!(
            "{}/_db/{}/_api/document/{}",
            self.endpoint, self.database, self.collection
        );

        let documents_json: Vec<Value> = documents
            .iter()
            .map(|doc| {
                json!({
                    "name": doc.name,
                    "v": doc.v,
                    "src": doc.src,
                })
            })
            .collect();

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.username, Some(&self.password))
            .json(&documents_json)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Failed to import batch: {}", error_text).into());
        }

        Ok(())
    }
}

