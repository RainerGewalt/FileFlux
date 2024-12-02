use serde_json::json;
use std::fs;
use std::path::Path;

/// Scans a directory recursively and returns its structure as JSON.
pub fn scan_directory_structure<P: AsRef<Path>>(path: P) -> serde_json::Value {
    let path = path.as_ref();

    match fs::read_dir(path) {
        Ok(entries) => {
            let mut directory_structure = Vec::new();

            for entry in entries.flatten() {
                let file_type = entry.file_type();
                let file_name = entry.file_name().into_string().unwrap_or_default();

                if let Ok(file_type) = file_type {
                    if file_type.is_dir() {
                        directory_structure.push(json!({
                            "type": "directory",
                            "name": file_name,
                            "children": scan_directory_structure(entry.path())
                        }));
                    } else {
                        directory_structure.push(json!({
                            "type": "file",
                            "name": file_name
                        }));
                    }
                }
            }

            serde_json::Value::Array(directory_structure)
        }
        Err(e) => {
            // Return a descriptive error as JSON
            json!({
                "error": format!("Failed to read directory: {}", e)
            })
        }
    }
}
