use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use tracing::info;

use crate::models::Memory;
use crate::storage::markdown::MarkdownStorage;

/// Detected contradiction between memories
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// Primary memory ID
    pub memory_a_id: String,
    /// Conflicting memory ID  
    pub memory_b_id: String,
    /// Type of contradiction
    pub contradiction_type: ContradictionType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Explanation of the conflict
    pub explanation: String,
    /// Suggested resolution
    pub suggestion: String,
}

/// Types of contradictions
#[derive(Debug, Clone, PartialEq)]
pub enum ContradictionType {
    /// Direct factual conflict
    FactConflict,
    /// Conflicting dates/times
    TemporalConflict,
    /// Conflicting values/settings
    ValueConflict,
    /// One memory contradicts another's implication
    LogicalConflict,
    /// Similar but different information (possible duplicate)
    NearDuplicate,
}

impl ContradictionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContradictionType::FactConflict => "fact_conflict",
            ContradictionType::TemporalConflict => "temporal_conflict",
            ContradictionType::ValueConflict => "value_conflict",
            ContradictionType::LogicalConflict => "logical_conflict",
            ContradictionType::NearDuplicate => "near_duplicate",
        }
    }
}

/// Configuration for contradiction detection
#[derive(Debug, Clone)]
pub struct ContradictionConfig {
    /// Minimum similarity threshold to consider as potential duplicate
    pub similarity_threshold: f32,
    /// Maximum distance in days for temporal conflicts
    pub temporal_window_days: i64,
    /// Enable near-duplicate detection
    pub detect_duplicates: bool,
    /// Enable fact conflict detection
    pub detect_fact_conflicts: bool,
}

impl Default for ContradictionConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.85,
            temporal_window_days: 30,
            detect_duplicates: true,
            detect_fact_conflicts: true,
        }
    }
}

/// Contradiction detection engine
pub struct ContradictionDetector {
    config: ContradictionConfig,
    storage: MarkdownStorage,
}

impl ContradictionDetector {
    pub fn new(config: ContradictionConfig, storage: MarkdownStorage) -> Self {
        Self { config, storage }
    }

    /// Run full contradiction check on all memories
    pub fn check_all(&self) -> Result<Vec<Contradiction>> {
        let memories = self.storage.list_memories(None)?;
        let mut contradictions = Vec::new();

        info!("Checking {} memories for contradictions", memories.len());

        // Check for near-duplicates and conflicts
        for (i, memory_a) in memories.iter().enumerate() {
            for memory_b in memories.iter().skip(i + 1) {
                // Skip if same ID
                if memory_a.id == memory_b.id {
                    continue;
                }

                // Check for contradictions
                if let Some(contradiction) = self.check_pair(memory_a, memory_b)? {
                    contradictions.push(contradiction);
                }
            }
        }

        // Check for orphan memories (no connections)
        let orphans = self.find_orphans(&memories)?;
        for orphan in orphans {
            contradictions.push(Contradiction {
                memory_a_id: orphan.id.clone(),
                memory_b_id: "none".to_string(),
                contradiction_type: ContradictionType::NearDuplicate,
                confidence: 0.5,
                explanation: format!("Memory '{}' has no connections (orphan)", orphan.title),
                suggestion: "Link to related memories or review if still relevant".to_string(),
            });
        }

        info!("Found {} contradictions/issues", contradictions.len());
        Ok(contradictions)
    }

    /// Check a specific memory for contradictions
    pub fn check_memory(&self, memory_id: &str) -> Result<Vec<Contradiction>> {
        let target = match self.storage.read_memory(&memory_id.to_string())? {
            Some(m) => m,
            None => return Ok(vec![]),
        };

        let all_memories = self.storage.list_memories(None)?;
        let mut contradictions = Vec::new();

        for memory in &all_memories {
            if memory.id == target.id {
                continue;
            }

            if let Some(contradiction) = self.check_pair(&target, memory)? {
                contradictions.push(contradiction);
            }
        }

        Ok(contradictions)
    }

    /// Check two specific memories for contradictions
    fn check_pair(&self, memory_a: &Memory, memory_b: &Memory) -> Result<Option<Contradiction>> {
        // Check for near-duplicates
        if self.config.detect_duplicates {
            if let Some(contradiction) = self.check_near_duplicate(memory_a, memory_b)? {
                return Ok(Some(contradiction));
            }
        }

        // Check for temporal conflicts
        if let Some(contradiction) = self.check_temporal_conflict(memory_a, memory_b)? {
            return Ok(Some(contradiction));
        }

        // Check for fact conflicts
        if self.config.detect_fact_conflicts {
            if let Some(contradiction) = self.check_fact_conflict(memory_a, memory_b)? {
                return Ok(Some(contradiction));
            }
        }

        Ok(None)
    }

    /// Check for near-duplicate memories
    fn check_near_duplicate(&self, a: &Memory, b: &Memory) -> Result<Option<Contradiction>> {
        let content_a = a.content.as_deref().unwrap_or("");
        let content_b = b.content.as_deref().unwrap_or("");

        let similarity = calculate_similarity(content_a, content_b);

        if similarity > self.config.similarity_threshold {
            return Ok(Some(Contradiction {
                memory_a_id: a.id.clone(),
                memory_b_id: b.id.clone(),
                contradiction_type: ContradictionType::NearDuplicate,
                confidence: similarity,
                explanation: format!(
                    "Memories '{}' and '{}' have {:.0}% similar content",
                    a.title,
                    b.title,
                    similarity * 100.0
                ),
                suggestion: "Consider merging these memories or removing the duplicate".to_string(),
            }));
        }

        Ok(None)
    }

    /// Check for temporal conflicts
    fn check_temporal_conflict(&self, a: &Memory, b: &Memory) -> Result<Option<Contradiction>> {
        // Check if both have dates in content
        let dates_a = extract_dates(a.content.as_deref().unwrap_or(""));
        let dates_b = extract_dates(b.content.as_deref().unwrap_or(""));

        // If they share tags and have close dates, might be related
        let shared_tags: Vec<String> = a
            .tags
            .iter()
            .filter(|t| b.tags.contains(t))
            .cloned()
            .collect();

        if !shared_tags.is_empty() && !dates_a.is_empty() && !dates_b.is_empty() {
            // Check if dates are very close (possible duplicate events)
            for date_a in &dates_a {
                for date_b in &dates_b {
                    let diff = (*date_a - *date_b).num_days().abs();
                    if diff < 2 {
                        return Ok(Some(Contradiction {
                            memory_a_id: a.id.clone(),
                            memory_b_id: b.id.clone(),
                            contradiction_type: ContradictionType::TemporalConflict,
                            confidence: 0.7,
                            explanation: format!(
                                "Memories '{}' and '{}' share tags {:?} and have dates {} and {}",
                                a.title, b.title, shared_tags, date_a, date_b
                            ),
                            suggestion: "Review if these describe the same event".to_string(),
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Check for fact conflicts (simplified - looks for opposing keywords)
    fn check_fact_conflict(&self, a: &Memory, b: &Memory) -> Result<Option<Contradiction>> {
        let content_a = a.content.as_deref().unwrap_or("").to_lowercase();
        let content_b = b.content.as_deref().unwrap_or("").to_lowercase();

        // Define opposing pairs
        let opposites: Vec<(&str, &str)> = vec![
            ("always", "never"),
            ("enable", "disable"),
            ("true", "false"),
            ("yes", "no"),
            ("all", "none"),
            ("must", "must not"),
            ("required", "optional"),
        ];

        for (word_a, word_b) in opposites {
            if content_a.contains(word_a) && content_b.contains(word_b)
                || content_a.contains(word_b) && content_b.contains(word_a)
            {
                // Check if they share context (tags or title similarity)
                let shared_tags_count: usize = a.tags.iter().filter(|t| b.tags.contains(t)).count();

                if shared_tags_count > 0 || title_similarity(&a.title, &b.title) > 0.5 {
                    return Ok(Some(Contradiction {
                        memory_a_id: a.id.clone(),
                        memory_b_id: b.id.clone(),
                        contradiction_type: ContradictionType::FactConflict,
                        confidence: 0.6,
                        explanation: format!(
                            "Memories '{}' and '{}' contain opposing terms ('{}' vs '{}')",
                            a.title, b.title, word_a, word_b
                        ),
                        suggestion: "Review which statement is correct".to_string(),
                    }));
                }
            }
        }

        Ok(None)
    }

    /// Find orphan memories (no connections)
    fn find_orphans(&self, memories: &[Memory]) -> Result<Vec<Memory>> {
        let mut orphans = Vec::new();

        for memory in memories {
            // Check if memory has any wiki links or is linked to
            let content = memory.content.as_deref().unwrap_or("");
            let has_outgoing = extract_wiki_links(content).len() > 0;

            // Check for incoming links (simplified - would need graph storage)
            let has_incoming = memories.iter().any(|m| {
                let m_content = m.content.as_deref().unwrap_or("");
                m_content.contains(&format!("[[{}]]", memory.title))
                    || m_content.contains(&format!("[[{}|", memory.title))
            });

            if !has_outgoing && !has_incoming {
                orphans.push(memory.clone());
            }
        }

        Ok(orphans)
    }

    /// Generate a contradiction report
    pub fn generate_report(&self, contradictions: &[Contradiction]) -> String {
        let mut report = String::new();
        report.push_str("# Contradiction Report\n\n");
        report.push_str(&format!("Generated: {}\n\n", chrono::Utc::now()));
        report.push_str(&format!("Total issues found: {}\n\n", contradictions.len()));

        // Group by type
        let mut by_type: HashMap<&str, Vec<&Contradiction>> = HashMap::new();
        for c in contradictions {
            by_type
                .entry(c.contradiction_type.as_str())
                .or_default()
                .push(c);
        }

        for (type_name, items) in by_type {
            report.push_str(&format!("## {} ({} items)\n\n", type_name, items.len()));
            for c in items {
                report.push_str(&format!(
                    "- **{}** ↔ **{}**\n  - Confidence: {:.0}%\n  - {}\n  - *Suggestion: {}*\n\n",
                    c.memory_a_id,
                    c.memory_b_id,
                    c.confidence * 100.0,
                    c.explanation,
                    c.suggestion
                ));
            }
        }

        report
    }
}

/// Calculate simple text similarity (Jaccard index on words)
fn calculate_similarity(a: &str, b: &str) -> f32 {
    let words_a: HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    let words_b: HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let intersection: HashSet<_> = words_a.intersection(&words_b).collect();
    let union: HashSet<_> = words_a.union(&words_b).collect();

    if union.is_empty() {
        return 0.0;
    }

    intersection.len() as f32 / union.len() as f32
}

/// Calculate title similarity
fn title_similarity(a: &str, b: &str) -> f32 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Exact match
    if a_lower == b_lower {
        return 1.0;
    }

    // Contains check
    if a_lower.contains(&b_lower) || b_lower.contains(&a_lower) {
        return 0.8;
    }

    // Word similarity
    calculate_similarity(a, b)
}

/// Extract dates from text (simplified)
fn extract_dates(text: &str) -> Vec<chrono::NaiveDate> {
    let mut dates = Vec::new();

    // Look for YYYY-MM-DD patterns
    let re = Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").unwrap();
    for cap in re.captures_iter(text) {
        if let (Ok(year), Ok(month), Ok(day)) = (
            cap[1].parse::<i32>(),
            cap[2].parse::<u32>(),
            cap[3].parse::<u32>(),
        ) {
            if let Some(date) = chrono::NaiveDate::from_ymd_opt(year, month, day) {
                dates.push(date);
            }
        }
    }

    dates
}

/// Extract wiki links from content
fn extract_wiki_links(content: &str) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    re.captures_iter(content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageConfig;
    use tempfile::TempDir;

    fn create_test_detector() -> (ContradictionDetector, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            max_file_size_mb: 10,
        };
        let storage = MarkdownStorage::new(config);
        storage.initialize().unwrap();

        let detector_config = ContradictionConfig::default();
        let detector = ContradictionDetector::new(detector_config, storage);

        (detector, temp_dir)
    }

    #[test]
    fn test_calculate_similarity() {
        let sim = calculate_similarity("hello world", "hello world");
        assert_eq!(sim, 1.0);

        let sim = calculate_similarity("hello world", "hello there");
        assert!(sim > 0.0 && sim < 1.0);

        let sim = calculate_similarity("hello", "world");
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_extract_dates() {
        let dates = extract_dates("Meeting on 2024-01-15 and 2024-02-20");
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0].to_string(), "2024-01-15");
    }

    #[test]
    fn test_extract_wiki_links() {
        let links = extract_wiki_links("See [[Rust]] and [[Ownership]]");
        assert_eq!(links, vec!["Rust", "Ownership"]);
    }

    #[test]
    fn test_title_similarity() {
        assert_eq!(title_similarity("Rust", "rust"), 1.0);
        assert!(title_similarity("Rust Programming", "Rust") > 0.7);
        assert!(title_similarity("Java", "Python") < 0.5);
    }
}
