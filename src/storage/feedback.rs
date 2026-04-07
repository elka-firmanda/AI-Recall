use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::models::MemoryId;

/// Feedback rating for a memory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackRating {
    /// Memory was helpful
    Useful,
    /// Memory was not relevant
    Irrelevant,
    /// Memory is outdated
    Outdated,
    /// Memory contains errors
    Wrong,
}

impl FeedbackRating {
    pub fn as_str(&self) -> &'static str {
        match self {
            FeedbackRating::Useful => "useful",
            FeedbackRating::Irrelevant => "irrelevant",
            FeedbackRating::Outdated => "outdated",
            FeedbackRating::Wrong => "wrong",
        }
    }

    /// Get numeric score for this rating (-1 to +1)
    pub fn score(&self) -> f32 {
        match self {
            FeedbackRating::Useful => 1.0,
            FeedbackRating::Irrelevant => -0.3,
            FeedbackRating::Outdated => -0.7,
            FeedbackRating::Wrong => -1.0,
        }
    }
}

impl std::str::FromStr for FeedbackRating {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "useful" => Ok(FeedbackRating::Useful),
            "irrelevant" => Ok(FeedbackRating::Irrelevant),
            "outdated" => Ok(FeedbackRating::Outdated),
            "wrong" => Ok(FeedbackRating::Wrong),
            _ => Err(format!("Unknown feedback rating: {}", s)),
        }
    }
}

/// Individual feedback entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub timestamp: DateTime<Utc>,
    pub rating: FeedbackRating,
    pub query: Option<String>,
    pub context: Option<String>,
    pub source: String, // e.g., "claude", "user", "system"
}

/// Aggregated feedback stats for a memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackStats {
    pub memory_id: MemoryId,
    pub total_feedback: usize,
    pub useful_count: usize,
    pub irrelevant_count: usize,
    pub outdated_count: usize,
    pub wrong_count: usize,
    /// Calculated relevance score (-1.0 to 1.0)
    pub relevance_score: f32,
    pub last_feedback_at: Option<DateTime<Utc>>,
}

/// Feedback storage
pub struct FeedbackStore {
    base_path: PathBuf,
}

impl FeedbackStore {
    pub fn new(base_path: PathBuf) -> Self {
        let store = Self {
            base_path: base_path.join(".meta").join("feedback"),
        };
        store
            .initialize()
            .expect("Failed to initialize feedback store");
        store
    }

    pub fn initialize(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path)?;
        Ok(())
    }

    /// Record feedback for a memory
    pub fn record_feedback(
        &self,
        memory_id: &str,
        rating: FeedbackRating,
        query: Option<String>,
        context: Option<String>,
        source: &str,
    ) -> Result<()> {
        let entry = FeedbackEntry {
            timestamp: Utc::now(),
            rating,
            query,
            context,
            source: source.to_string(),
        };

        let file_path = self.get_feedback_path(memory_id);

        // Append to feedback file
        let entry_json = serde_json::to_string(&entry)?;
        let line = format!("{}\n", entry_json);

        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        file.write_all(line.as_bytes())?;

        debug!(
            "Recorded {} feedback for memory {} from {}",
            rating.as_str(),
            memory_id,
            source
        );

        Ok(())
    }

    /// Get all feedback for a memory
    pub fn get_feedback(&self, memory_id: &str) -> Result<Vec<FeedbackEntry>> {
        let file_path = self.get_feedback_path(memory_id);

        if !file_path.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&file_path)?;
        let mut entries = Vec::new();

        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<FeedbackEntry>(line) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Calculate feedback statistics for a memory
    pub fn get_stats(&self, memory_id: &str) -> Result<FeedbackStats> {
        let entries = self.get_feedback(memory_id)?;

        let total = entries.len();
        let useful_count = entries
            .iter()
            .filter(|e| e.rating == FeedbackRating::Useful)
            .count();
        let irrelevant_count = entries
            .iter()
            .filter(|e| e.rating == FeedbackRating::Irrelevant)
            .count();
        let outdated_count = entries
            .iter()
            .filter(|e| e.rating == FeedbackRating::Outdated)
            .count();
        let wrong_count = entries
            .iter()
            .filter(|e| e.rating == FeedbackRating::Wrong)
            .count();

        // Calculate weighted score
        let total_score: f32 = entries.iter().map(|e| e.rating.score()).sum();
        let relevance_score = if total > 0 {
            (total_score / total as f32).clamp(-1.0, 1.0)
        } else {
            0.0 // Neutral if no feedback
        };

        let last_feedback_at = entries.last().map(|e| e.timestamp);

        Ok(FeedbackStats {
            memory_id: memory_id.to_string(),
            total_feedback: total,
            useful_count,
            irrelevant_count,
            outdated_count,
            wrong_count,
            relevance_score,
            last_feedback_at,
        })
    }

    /// Get global feedback statistics
    pub fn get_global_stats(&self) -> Result<GlobalFeedbackStats> {
        let mut total_memories = 0;
        let mut total_feedback = 0;
        let mut total_useful = 0;
        let mut total_irrelevant = 0;
        let mut total_outdated = 0;
        let mut total_wrong = 0;

        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let memory_id = entry
                    .file_name()
                    .to_string_lossy()
                    .trim_end_matches(".jsonl")
                    .to_string();

                if let Ok(stats) = self.get_stats(&memory_id) {
                    total_memories += 1;
                    total_feedback += stats.total_feedback;
                    total_useful += stats.useful_count;
                    total_irrelevant += stats.irrelevant_count;
                    total_outdated += stats.outdated_count;
                    total_wrong += stats.wrong_count;
                }
            }
        }

        let avg_feedback_per_memory = if total_memories > 0 {
            total_feedback as f32 / total_memories as f32
        } else {
            0.0
        };

        Ok(GlobalFeedbackStats {
            total_memories_with_feedback: total_memories,
            total_feedback_entries: total_feedback,
            avg_feedback_per_memory,
            total_useful,
            total_irrelevant,
            total_outdated,
            total_wrong,
        })
    }

    /// Find memories with low relevance scores
    pub fn find_low_quality_memories(&self, threshold: f32) -> Result<Vec<(String, f32)>> {
        let mut low_quality = Vec::new();

        for entry in fs::read_dir(&self.base_path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let memory_id = entry
                    .file_name()
                    .to_string_lossy()
                    .trim_end_matches(".jsonl")
                    .to_string();

                if let Ok(stats) = self.get_stats(&memory_id) {
                    if stats.relevance_score < threshold {
                        low_quality.push((memory_id, stats.relevance_score));
                    }
                }
            }
        }

        // Sort by score (lowest first)
        low_quality.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        Ok(low_quality)
    }

    /// Clear all feedback for a memory
    pub fn clear_feedback(&self, memory_id: &str) -> Result<()> {
        let file_path = self.get_feedback_path(memory_id);
        if file_path.exists() {
            fs::remove_file(&file_path)?;
            info!("Cleared feedback for memory {}", memory_id);
        }
        Ok(())
    }

    /// Get feedback file path for a memory
    fn get_feedback_path(&self, memory_id: &str) -> PathBuf {
        let safe_id = memory_id.replace(['/', '\\', ':'], "_");
        self.base_path.join(format!("{}.jsonl", safe_id))
    }
}

/// Global feedback statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalFeedbackStats {
    pub total_memories_with_feedback: usize,
    pub total_feedback_entries: usize,
    pub avg_feedback_per_memory: f32,
    pub total_useful: usize,
    pub total_irrelevant: usize,
    pub total_outdated: usize,
    pub total_wrong: usize,
}

impl Default for FeedbackStore {
    fn default() -> Self {
        Self::new(PathBuf::from("./data"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (FeedbackStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let store = FeedbackStore::new(temp_dir.path().to_path_buf());
        (store, temp_dir)
    }

    #[test]
    fn test_record_and_get_feedback() {
        let (store, _temp) = create_test_store();

        store
            .record_feedback(
                "mem1",
                FeedbackRating::Useful,
                Some("test query".to_string()),
                Some("helped answer".to_string()),
                "user",
            )
            .unwrap();

        let feedback = store.get_feedback("mem1").unwrap();
        assert_eq!(feedback.len(), 1);
        assert_eq!(feedback[0].rating, FeedbackRating::Useful);
    }

    #[test]
    fn test_feedback_stats() {
        let (store, _temp) = create_test_store();

        // Record multiple feedback entries
        store
            .record_feedback("mem1", FeedbackRating::Useful, None, None, "user")
            .unwrap();
        store
            .record_feedback("mem1", FeedbackRating::Useful, None, None, "user")
            .unwrap();
        store
            .record_feedback("mem1", FeedbackRating::Wrong, None, None, "user")
            .unwrap();

        let stats = store.get_stats("mem1").unwrap();
        assert_eq!(stats.total_feedback, 3);
        assert_eq!(stats.useful_count, 2);
        assert_eq!(stats.wrong_count, 1);
        // Score should be between -1 and 1
        assert!(stats.relevance_score >= -1.0 && stats.relevance_score <= 1.0);
    }

    #[test]
    fn test_find_low_quality() {
        let (store, _temp) = create_test_store();

        // High quality memory
        store
            .record_feedback("mem1", FeedbackRating::Useful, None, None, "user")
            .unwrap();
        store
            .record_feedback("mem1", FeedbackRating::Useful, None, None, "user")
            .unwrap();

        // Low quality memory
        store
            .record_feedback("mem2", FeedbackRating::Wrong, None, None, "user")
            .unwrap();
        store
            .record_feedback("mem2", FeedbackRating::Outdated, None, None, "user")
            .unwrap();

        let low_quality = store.find_low_quality_memories(0.0).unwrap();
        assert_eq!(low_quality.len(), 1);
        assert_eq!(low_quality[0].0, "mem2");
    }

    #[test]
    fn test_rating_scores() {
        assert_eq!(FeedbackRating::Useful.score(), 1.0);
        assert_eq!(FeedbackRating::Wrong.score(), -1.0);
        assert!(FeedbackRating::Irrelevant.score() < 0.0);
        assert!(FeedbackRating::Outdated.score() < 0.0);
    }
}
