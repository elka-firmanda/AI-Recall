use std::path::PathBuf;
use anyhow::{Context, Result};
use tracing::info;

/// PDF content extractor using pdf-extract (simpler alternative)
#[derive(Clone)]
pub struct PdfExtractor {
    data_dir: PathBuf,
}

impl PdfExtractor {
    pub fn new(data_dir: &PathBuf) -> Self {
        let images_dir = data_dir.join("raw").join("images");
        
        // Create directory if needed
        let _ = std::fs::create_dir_all(&images_dir);
        
        Self {
            data_dir: images_dir,
        }
    }
    
    /// Extract text from PDF
    pub async fn extract(&self, data: &[u8]) -> Result<String> {
        // Write to temp file since pdf-extract works with files
        let temp_path = std::env::temp_dir().join(format!("extract_{}.pdf", uuid::Uuid::new_v4()));
        tokio::fs::write(&temp_path, data).await?;
        
        // Extract text
        let text = tokio::task::spawn_blocking({
            let path = temp_path.clone();
            move || {
                let result = pdf_extract::extract_text(&path);
                // Clean up temp file
                let _ = std::fs::remove_file(&path);
                result
            }
        }).await?;
        
        let mut extracted_content = text.context("Failed to extract PDF text")?;
        
        // Attempt to detect and format tables
        extracted_content = self.detect_and_format_tables(&extracted_content);
        
        info!("Extracted {} characters from PDF", extracted_content.len());
        
        Ok(extracted_content)
    }
    
    /// Detect tables in text and format as markdown
    fn detect_and_format_tables(&self, content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::new();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i];
            
            // Simple table detection: look for multiple spaces or tabs
            // that could indicate columns
            if line.contains("  ") || line.contains('\t') {
                // Try to find table rows
                let mut table_rows = vec![line];
                let mut j = i + 1;
                
                while j < lines.len() {
                    let next_line = lines[j];
                    if next_line.contains("  ") || next_line.contains('\t') || next_line.trim().is_empty() {
                        if !next_line.trim().is_empty() {
                            table_rows.push(next_line);
                        }
                        j += 1;
                    } else {
                        break;
                    }
                }
                
                // If we found at least 2 rows, format as table
                if table_rows.len() >= 2 {
                    result.push_str(&self.format_as_markdown_table(&table_rows));
                    result.push('\n');
                    i = j;
                    continue;
                }
            }
            
            result.push_str(line);
            result.push('\n');
            i += 1;
        }
        
        result
    }
    
    /// Format detected table rows as markdown table
    fn format_as_markdown_table(&self, rows: &[&str]) -> String {
        if rows.is_empty() {
            return String::new();
        }
        
        let mut table = String::new();
        
        // Parse rows into columns (split by multiple spaces or tabs)
        let parsed_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                // Split by tabs or 2+ spaces
                let delimiter = if row.contains('\t') { '\t' } else { ' ' };
                row.split(delimiter)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .collect();
        
        // Find max column count
        let max_cols = parsed_rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if max_cols < 2 {
            // Not a table, return original
            return rows.join("\n");
        }
        
        // Format as markdown table
        for (i, row) in parsed_rows.iter().enumerate() {
            let cells: Vec<String> = (0..max_cols)
                .map(|j| row.get(j).cloned().unwrap_or_default())
                .collect();
            
            table.push_str("| ");
            table.push_str(&cells.join(" | "));
            table.push_str(" |\n");
            
            // Add separator after header row
            if i == 0 {
                table.push_str("|");
                for _ in 0..max_cols {
                    table.push_str(" --- |");
                }
                table.push('\n');
            }
        }
        
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_table_detection() {
        let extractor = PdfExtractor::new(&PathBuf::from("/tmp"));
        
        let input = "Column1  Column2  Column3\nValue1   Value2   Value3\nValue4   Value5   Value6";
        let output = extractor.detect_and_format_tables(input);
        
        assert!(output.contains("|"));
        assert!(output.contains("---"));
    }
}
