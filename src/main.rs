use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::WalkDir;

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

/// Represents a code chunk extracted from a source file
#[derive(Debug)]
struct CodeChunk {
    name: String,
    content: String,
    start_line: usize,
    end_line: usize,
    kind: String,           // "function", "class", "method", etc.
    parent: Option<String>, // For methods, this would be the class name
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
    _character: usize,
}

// LSP SymbolKind values (subset)
//const SYMBOL_KIND_FILE: u8 = 1;
//const SYMBOL_KIND_MODULE: u8 = 2;
const SYMBOL_KIND_NAMESPACE: u8 = 3;
const SYMBOL_KIND_CLASS: u8 = 5;
const SYMBOL_KIND_METHOD: u8 = 6;
const SYMBOL_KIND_FUNCTION: u8 = 12;
//const SYMBOL_KIND_VARIABLE: u8 = 13;

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create output directory if it doesn't exist
    fs::create_dir_all(&args.output_dir)?;

    // Find all C++ source files in the project
    let source_files = find_cpp_source_files(&args.project_dir)?;
    println!("Found {} C++ source files", source_files.len());

    // Start clangd process
    let mut clangd = Command::new(&args.clangd_path)
        .arg("--compile-commands-dir=build")
        .arg("--log=verbose")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

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
            "rootUri": format!("file://{}", fs::canonicalize(&args.project_dir)?.to_string_lossy()),
            "capabilities": {
                "textDocument": {
                    "documentSymbol": {
                        "hierarchicalDocumentSymbolSupport": true
                    }
                }
            }
        }
    });

    send_lsp_request(&mut clangd_stdin, initialize_request)?;

    // Process all source files
    for source_file in source_files {
        println!("Processing file: {}", source_file.display());
        process_file(
            &source_file,
            &args.output_dir,
            &mut clangd_stdin,
            &mut clangd_stdout,
        )?;
    }

    // Shutdown clangd
    let shutdown_request = json!({
        "jsonrpc": "2.0",
        "id": 9999,
        "method": "shutdown",
        "params": null
    });
    send_lsp_request(&mut clangd_stdin, shutdown_request)?;

    // Exit clangd
    let exit_notification = json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": null
    });
    send_lsp_request(&mut clangd_stdin, exit_notification)?;

    Ok(())
}

fn find_cpp_source_files(project_dir: &str) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut cpp_files = Vec::new();

    for entry in WalkDir::new(project_dir) {
        let entry = entry?;
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

fn process_file(
    file_path: &Path,
    output_dir: &str,
    clangd_stdin: &mut std::process::ChildStdin,
    clangd_stdout: &mut BufReader<std::process::ChildStdout>,
) -> Result<(), Box<dyn Error>> {
    let file_content = fs::read_to_string(file_path)?;
    let file_uri = format!("file://{}", fs::canonicalize(file_path)?.to_string_lossy());

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
    send_lsp_request(clangd_stdin, did_open_notification)?;

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
    send_lsp_request(clangd_stdin, document_symbol_request)?;

    // Read and process clangd's response to extract symbols
    let symbols = read_document_symbols(clangd_stdout)?;

    // Extract chunks from the file based on the symbols
    let chunks = extract_chunks(&file_path, &file_content, &symbols)?;

    // Write chunks to output files
    write_chunks(file_path, output_dir, &chunks)?;

    Ok(())
}

fn send_lsp_request(
    stdin: &mut std::process::ChildStdin,
    request: serde_json::Value,
) -> Result<(), Box<dyn Error>> {
    use std::io::Write;

    let request_str = serde_json::to_string(&request)?;
    let content_length = request_str.len();

    writeln!(stdin, "Content-Length: {}", content_length)?;
    writeln!(stdin)?;
    write!(stdin, "{}", request_str)?;
    stdin.flush()?;

    Ok(())
}

fn read_document_symbols(
    stdout: &mut BufReader<std::process::ChildStdout>,
) -> Result<Vec<Symbol>, Box<dyn Error>> {
    let mut headers = HashMap::new();

    let mut in_header = true;
    let mut content: Vec<u8> = Vec::with_capacity(1000000);

    for line in stdout.lines() {
        let line = line?;

        if in_header {
            if line.is_empty() {
                in_header = false;
                continue;
            }
            if line.contains(':') {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                let header = parts[0].trim();
                let value = parts[1].trim();

                headers.insert(header.to_string(), value.to_string());
            } else {
                println!("Ignoring line: {}", line);
            }
        } else {
            // !in_header
            content.extend_from_slice(line.as_bytes());
        }
    }

    let response: Value = serde_json::from_slice(&content)?;

    // Check if this is a document symbol response
    if let Some(id) = response.get("id") {
        if id == 2 && response.get("result").is_some() {
            // Parse the symbol information
            let symbols = response["result"].clone();
            return Ok(serde_json::from_value(symbols)?);
        }
    }

    // Reset for next message
    headers.clear();

    // If we got here, we didn't find the document symbol response
    Err("Failed to get document symbols response".into())
}

fn extract_chunks(
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

fn write_chunks(
    source_file: &Path,
    output_dir: &str,
    chunks: &[CodeChunk],
) -> Result<(), Box<dyn Error>> {
    // Create a directory for this file's chunks
    let file_stem = source_file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let file_chunks_dir = PathBuf::from(output_dir).join(file_stem.to_string());
    fs::create_dir_all(&file_chunks_dir)?;

    // Write index file with metadata about all chunks
    let mut index = File::create(file_chunks_dir.join("_index.txt"))?;
    use std::io::Write;

    writeln!(index, "Source file: {}", source_file.display())?;
    writeln!(index, "Number of chunks: {}", chunks.len())?;
    writeln!(index, "---")?;

    // Write each chunk to a separate file
    for (i, chunk) in chunks.iter().enumerate() {
        let sanitized_name = chunk
            .name
            .replace("::", "_")
            .replace("<", "_")
            .replace(">", "_");
        let chunk_filename = format!(
            "{:03}_{}_{}_{}.cpp",
            i + 1,
            sanitized_name,
            chunk.kind,
            chunk.start_line + 1
        );

        let chunk_path = file_chunks_dir.join(chunk_filename.clone());
        fs::write(&chunk_path, &chunk.content)?;

        // Add to index
        writeln!(index, "Chunk: {}", chunk_filename)?;
        writeln!(index, "  Name: {}", chunk.name)?;
        writeln!(index, "  Kind: {}", chunk.kind)?;
        writeln!(
            index,
            "  Lines: {}-{}",
            chunk.start_line + 1,
            chunk.end_line + 1
        )?;
        if let Some(parent) = &chunk.parent {
            writeln!(index, "  Parent: {}", parent)?;
        }
        writeln!(index, "---")?;
    }

    println!(
        "Wrote {} chunks for {}",
        chunks.len(),
        source_file.display()
    );
    Ok(())
}
