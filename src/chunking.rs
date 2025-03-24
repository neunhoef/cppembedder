use serde::Deserialize;
use serde_json::json;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

/// Represents a code chunk extracted from a source file
#[derive(Debug)]
pub struct CodeChunk {
    pub name: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: String,           // "function", "class", "method", etc.
    pub parent: Option<String>, // For methods, this would be the class name
}

/// Represents the LSP document symbol response structure
#[derive(Debug, Deserialize)]
struct Symbol {
    name: String,
    kind: u8,
    range: Range,
    #[serde(default)]
    children: Vec<Symbol>,
}

#[derive(Debug, Deserialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Debug, Deserialize)]
struct Position {
    line: usize,
    #[serde(rename = "character")]
    _character: usize,
}

// LSP SymbolKind values (subset)
const SYMBOL_KIND_NAMESPACE: u8 = 3;
const SYMBOL_KIND_CLASS: u8 = 5;
const SYMBOL_KIND_METHOD: u8 = 6;
const SYMBOL_KIND_FUNCTION: u8 = 12;

pub struct Chunker {
    project_dir: String,
    output_dir: String,
    clangd_path: String,
    lsp_log_file: String,
}

fn sanitize_name(s: &str) -> String {
    let mut r = s
        .replace("::", "_doublecolon_")
        .replace("<", "_less_")
        .replace(">", "_greater_")
        .replace("/", "_slash_");
    r.truncate(200);
    r
}

impl Chunker {
    pub fn new(
        project_dir: String,
        output_dir: String,
        clangd_path: String,
        lsp_log_file: String,
    ) -> Self {
        Self {
            project_dir,
            output_dir,
            clangd_path,
            lsp_log_file,
        }
    }

    fn find_cpp_source_files(&self) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let mut cpp_files = Vec::new();

        for entry in WalkDir::new(&self.project_dir) {
            let entry = entry.map_err(|e| {
                format!(
                    "Failed to read directory entry in '{}': {}",
                    self.project_dir, e
                )
            })?;
            let path = entry.path();

            if path.is_file() {
                if let Some(extension) = path.extension() {
                    let ext = extension.to_string_lossy().to_lowercase();
                    if ext == "cpp"
                        || ext == "cxx"
                        || ext == "cc"
                        || ext == "h"
                        || ext == "hpp"
                        || ext == "hxx"
                    {
                        cpp_files.push(path.to_path_buf());
                    }
                }
            }
        }

        Ok(cpp_files)
    }

    fn extract_chunks(
        &self,
        _file_path: &Path,
        file_content: &str,
        symbols: &[Symbol],
    ) -> Result<Vec<CodeChunk>, Box<dyn Error>> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = file_content.lines().collect();

        // Helper function to process symbols recursively
        fn process_symbols(
            symbols: &[Symbol],
            lines: &[&str],
            chunks: &mut Vec<CodeChunk>,
            parent: Option<&str>,
        ) {
            for symbol in symbols {
                let kind = match symbol.kind {
                    SYMBOL_KIND_FUNCTION => "function",
                    SYMBOL_KIND_METHOD => "method",
                    SYMBOL_KIND_CLASS => "class",
                    SYMBOL_KIND_NAMESPACE => "namespace",
                    _ => continue, // Skip other symbols like variables
                };

                let start_line = symbol.range.start.line;
                let end_line = symbol.range.end.line;

                // Skip if the range is invalid or too small
                if start_line >= end_line || end_line >= lines.len() {
                    continue;
                }

                // Extract the content of the chunk
                let content = lines[start_line..=end_line].join("\n");

                // Create a unique name for the chunk
                let chunk_name = if let Some(parent_name) = parent {
                    format!("{}::{}", parent_name, symbol.name)
                } else {
                    symbol.name.clone()
                };

                chunks.push(CodeChunk {
                    name: chunk_name.clone(),
                    content,
                    start_line,
                    end_line,
                    kind: kind.to_string(),
                    parent: parent.map(|s| s.to_string()),
                });

                // Process child symbols (like methods within a class)
                process_symbols(&symbol.children, lines, chunks, Some(&chunk_name));
            }
        }

        process_symbols(symbols, &lines, &mut chunks, None);
        Ok(chunks)
    }

    fn write_chunks(&self, source_file: &Path, chunks: &[CodeChunk]) -> Result<(), Box<dyn Error>> {
        // Create a directory for this file's chunks
        let file_stem = source_file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let file_chunks_dir = PathBuf::from(&self.output_dir).join(file_stem.to_string());
        fs::create_dir_all(&file_chunks_dir).map_err(|e| {
            format!(
                "Failed to create chunks directory '{}': {}",
                file_chunks_dir.display(),
                e
            )
        })?;

        // Write index file with metadata about all chunks
        let mut index = File::create(file_chunks_dir.join("_index.txt")).map_err(|e| {
            format!(
                "Failed to create index file in '{}': {}",
                file_chunks_dir.display(),
                e
            )
        })?;

        writeln!(index, "Source file: {}", source_file.display())
            .map_err(|e| format!("Failed to write to index file: {}", e))?;
        writeln!(index, "Number of chunks: {}", chunks.len())
            .map_err(|e| format!("Failed to write to index file: {}", e))?;
        writeln!(index, "---").map_err(|e| format!("Failed to write to index file: {}", e))?;

        // Write each chunk to a separate file
        for (i, chunk) in chunks.iter().enumerate() {
            let sanitized_name = sanitize_name(&chunk.name);
            let chunk_filename = format!(
                "{:03}_{}_{}_{}.cpp",
                i + 1,
                sanitized_name,
                chunk.kind,
                chunk.start_line + 1
            );

            let chunk_path = file_chunks_dir.join(chunk_filename.clone());
            fs::write(&chunk_path, &chunk.content).map_err(|e| {
                format!(
                    "Failed to write chunk file '{}': {}",
                    chunk_path.display(),
                    e
                )
            })?;

            // Add to index
            writeln!(index, "Chunk: {}", chunk_filename)
                .map_err(|e| format!("Failed to write to index file: {}", e))?;
            writeln!(index, "  Name: {}", chunk.name)
                .map_err(|e| format!("Failed to write to index file: {}", e))?;
            writeln!(index, "  Kind: {}", chunk.kind)
                .map_err(|e| format!("Failed to write to index file: {}", e))?;
            writeln!(
                index,
                "  Lines: {}-{}",
                chunk.start_line + 1,
                chunk.end_line + 1
            )
            .map_err(|e| format!("Failed to write to index file: {}", e))?;
            if let Some(parent) = &chunk.parent {
                writeln!(index, "  Parent: {}", parent)
                    .map_err(|e| format!("Failed to write to index file: {}", e))?;
            }
            writeln!(index, "---").map_err(|e| format!("Failed to write to index file: {}", e))?;
        }

        println!(
            "Wrote {} chunks for {}",
            chunks.len(),
            source_file.display()
        );
        Ok(())
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(&self.output_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {}",
                self.output_dir, e
            )
        })?;

        // Open LSP log file
        let mut _lsp_log = File::create(&self.lsp_log_file).map_err(|e| {
            format!(
                "Failed to create LSP log file '{}': {}",
                self.lsp_log_file, e
            )
        })?;

        // Find all C++ source files in the project
        let source_files = self.find_cpp_source_files().map_err(|e| {
            format!(
                "Failed to scan project directory '{}': {}",
                self.project_dir, e
            )
        })?;
        println!("Found {} C++ source files", source_files.len());

        // Start clangd process
        let mut clangd = Command::new(&self.clangd_path)
            .arg("--compile-commands-dir=build")
            .arg("--log=verbose")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to start clangd process at '{}': {}",
                    self.clangd_path, e
                )
            })?;

        let mut clangd_stdin = clangd.stdin.take().expect("Failed to open clangd stdin");
        let mut clangd_stdout =
            BufReader::new(clangd.stdout.take().expect("Failed to open clangd stdout"));

        // Send LSP initialization request
        let initialize_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": format!("file://{}", fs::canonicalize(&self.project_dir)
                    .map_err(|e| format!("Failed to canonicalize project path '{}': {}", self.project_dir, e))?
                    .to_string_lossy()),
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": {
                            "hierarchicalDocumentSymbolSupport": true
                        }
                    }
                }
            }
        });

        self.send_lsp_request(&mut clangd_stdin, initialize_request)
            .map_err(|e| format!("Failed to send LSP initialization request: {}", e))?;

        // Process all source files
        let total_nr = source_files.len();
        for (i, source_file) in source_files.into_iter().enumerate() {
            println!(
                "Processing file ({i} / {total_nr}): {}",
                source_file.display()
            );
            self.process_file(&source_file, &mut clangd_stdin, &mut clangd_stdout)
                .map_err(|e| {
                    format!("Failed to process file '{}': {}", source_file.display(), e)
                })?;
        }

        // Shutdown clangd
        let shutdown_request = json!({
            "jsonrpc": "2.0",
            "id": 9999,
            "method": "shutdown",
            "params": null
        });
        self.send_lsp_request(&mut clangd_stdin, shutdown_request)
            .map_err(|e| format!("Failed to send LSP shutdown request: {}", e))?;

        // Exit clangd
        let exit_notification = json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        });
        self.send_lsp_request(&mut clangd_stdin, exit_notification)
            .map_err(|e| format!("Failed to send LSP exit notification: {}", e))?;

        Ok(())
    }

    fn send_lsp_request(
        &self,
        stdin: &mut std::process::ChildStdin,
        request: serde_json::Value,
    ) -> Result<(), Box<dyn Error>> {
        let request_str = serde_json::to_string(&request)
            .map_err(|e| format!("Failed to serialize LSP request: {}", e))?;
        let content_length = request_str.len();

        // Create log entry for request
        let log_entry = format!(
            ">>> Request:\nContent-Length: {}\n\n{}\n",
            content_length, request_str
        );
        if let Ok(mut lsp_log) = File::options().append(true).open(&self.lsp_log_file) {
            write!(lsp_log, "{}", log_entry)
                .map_err(|e| format!("Failed to write to LSP log file: {}", e))?;
        }

        writeln!(stdin, "Content-Length: {}", content_length)
            .map_err(|e| format!("Failed to write Content-Length header: {}", e))?;
        writeln!(stdin).map_err(|e| format!("Failed to write header separator: {}", e))?;
        write!(stdin, "{}", request_str)
            .map_err(|e| format!("Failed to write request body: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Failed to flush request: {}", e))?;

        Ok(())
    }

    fn read_lsp_response(
        &self,
        reader: &mut BufReader<std::process::ChildStdout>,
    ) -> Result<serde_json::Value, Box<dyn Error>> {
        // Read headers
        let mut content_length: Option<usize> = None;
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("Failed to read LSP response header: {}", e))?;
            let line = line.trim();

            headers.push_str(&line);
            headers.push('\n');

            if line.is_empty() {
                break; // Headers are done
            }

            if line.starts_with("Content-Length:") {
                let len_str = line
                    .split(':')
                    .nth(1)
                    .ok_or("Invalid Content-Length header")?;
                content_length = Some(len_str.trim().parse().map_err(|e| {
                    format!(
                        "Failed to parse Content-Length value '{}': {}",
                        len_str.trim(),
                        e
                    )
                })?);
            }
        }

        // Read content
        if let Some(length) = content_length {
            let mut buffer = vec![0; length];
            reader.read_exact(&mut buffer).map_err(|e| {
                format!(
                    "Failed to read LSP response body of length {}: {}",
                    length, e
                )
            })?;

            let response_str = String::from_utf8_lossy(&buffer);
            let json_value: serde_json::Value = serde_json::from_slice(&buffer)
                .map_err(|e| format!("Failed to parse LSP response JSON: {}", e))?;

            // Log the response
            let log_entry = format!("<<< Response:\n{}{}\n", headers, response_str);
            if let Ok(mut lsp_log) = File::options().append(true).open(&self.lsp_log_file) {
                write!(lsp_log, "{}", log_entry)
                    .map_err(|e| format!("Failed to write to LSP log file: {}", e))?;
            }

            Ok(json_value)
        } else {
            Err("No Content-Length header found".into())
        }
    }

    fn process_file(
        &self,
        file_path: &Path,
        clangd_stdin: &mut std::process::ChildStdin,
        clangd_stdout: &mut BufReader<std::process::ChildStdout>,
    ) -> Result<(), Box<dyn Error>> {
        let file_content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path.display(), e))?;
        let file_uri = format!(
            "file://{}",
            fs::canonicalize(file_path)
                .map_err(|e| format!(
                    "Failed to canonicalize path '{}': {}",
                    file_path.display(),
                    e
                ))?
                .to_string_lossy()
        );

        // Send didOpen notification to tell clangd about the file
        let did_open_notification = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": file_uri,
                    "languageId": "cpp",
                    "version": 1,
                    "text": file_content
                }
            }
        });
        self.send_lsp_request(clangd_stdin, did_open_notification)
            .map_err(|e| {
                format!(
                    "Failed to send didOpen notification for '{}': {}",
                    file_path.display(),
                    e
                )
            })?;

        // Send document symbol request to get the symbols in the file
        let document_symbol_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "textDocument/documentSymbol",
            "params": {
                "textDocument": {
                    "uri": file_uri
                }
            }
        });
        self.send_lsp_request(clangd_stdin, document_symbol_request)
            .map_err(|e| {
                format!(
                    "Failed to send document symbol request for '{}': {}",
                    file_path.display(),
                    e
                )
            })?;

        // Read and process clangd's response to extract symbols
        let symbols = self.read_document_symbols(clangd_stdout).map_err(|e| {
            format!(
                "Failed to read document symbols for '{}': {}",
                file_path.display(),
                e
            )
        })?;

        // Extract chunks from the file based on the symbols
        let chunks = self
            .extract_chunks(file_path, &file_content, &symbols)
            .map_err(|e| {
                format!(
                    "Failed to extract chunks from '{}': {}",
                    file_path.display(),
                    e
                )
            })?;

        // Write chunks to output files
        self.write_chunks(file_path, &chunks).map_err(|e| {
            format!(
                "Failed to write chunks for '{}': {}",
                file_path.display(),
                e
            )
        })?;

        Ok(())
    }

    fn read_document_symbols(
        &self,
        stdout: &mut BufReader<std::process::ChildStdout>,
    ) -> Result<Vec<Symbol>, Box<dyn Error>> {
        // Keep reading responses until we get the document symbol response
        loop {
            let response = self.read_lsp_response(stdout).map_err(|e| {
                format!(
                    "Failed to read LSP response while waiting for document symbols: {}",
                    e
                )
            })?;

            // Check if this is the document symbol response (id: 2)
            if let Some(id) = response.get("id") {
                if id.as_u64() == Some(2) && response.get("result").is_some() {
                    return serde_json::from_value(response["result"].clone()).map_err(|e| {
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Failed to parse document symbols from response: {}", e),
                        )) as Box<dyn Error>
                    });
                }
            }

            // For debugging
            let log_entry = format!(
                "Got response with id: {:?}, method: {:?}",
                response.get("id"),
                response.get("method")
            );
            if let Ok(mut lsp_log) = File::options().append(true).open(&self.lsp_log_file) {
                write!(lsp_log, "{}", log_entry)
                    .map_err(|e| format!("Failed to write to LSP log file: {}", e))?;
            }
        }
    }
}
