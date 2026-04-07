pub mod storage;

use regex::Regex;
use std::collections::{HashMap, HashSet};

pub use storage::{Direction, Edge, EdgeType, GraphStats, GraphStorage, PathResult, RelatedMemory};

/// Extract wiki links from markdown content
pub struct WikiLinkExtractor {
    link_pattern: Regex,
}

impl WikiLinkExtractor {
    pub fn new() -> Self {
        Self {
            link_pattern: Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]*))?\]\]").unwrap(),
        }
    }

    /// Extract all wiki links from content
    pub fn extract(&self, content: &str) -> Vec<WikiLink> {
        self.link_pattern
            .captures_iter(content)
            .filter_map(|cap| {
                let target = cap.get(1)?.as_str().trim().to_string();
                let display = cap
                    .get(2)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_else(|| target.clone());

                Some(WikiLink {
                    target,
                    display_text: display,
                })
            })
            .collect()
    }

    /// Extract just the target names (for simple usage)
    pub fn extract_targets(&self, content: &str) -> Vec<String> {
        self.extract(content)
            .into_iter()
            .map(|link| link.target)
            .collect()
    }

    /// Count links in content
    pub fn count_links(&self, content: &str) -> usize {
        self.link_pattern.find_iter(content).count()
    }

    /// Check if content contains wiki links
    pub fn has_links(&self, content: &str) -> bool {
        self.link_pattern.is_match(content)
    }

    /// Replace wiki links with markdown links
    pub fn convert_to_markdown_links(&self, content: &str) -> String {
        self.link_pattern
            .replace_all(content, |caps: &regex::Captures| {
                let target = caps.get(1).unwrap().as_str().trim();
                let display = caps.get(2).map(|m| m.as_str().trim()).unwrap_or(target);

                // Convert to standard markdown link
                // Note: The actual URL would need to be resolved
                format!("[{}]({}.md)", display, Self::slugify(target))
            })
            .to_string()
    }

    /// Create a slug from a title
    fn slugify(title: &str) -> String {
        title
            .to_lowercase()
            .replace(" ", "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "")
            .trim_matches('-')
            .to_string()
    }

    /// Build a link map from multiple documents
    pub fn build_link_map(&self, documents: &[(String, String)]) -> HashMap<String, Vec<String>> {
        let mut link_map: HashMap<String, Vec<String>> = HashMap::new();

        for (doc_id, content) in documents {
            let links = self.extract_targets(content);
            link_map.insert(doc_id.clone(), links);
        }

        link_map
    }

    /// Find backlinks (documents that link to a target)
    pub fn find_backlinks(
        &self,
        target: &str,
        documents: &[(String, String)],
    ) -> Vec<(String, String)> {
        let target_lower = target.to_lowercase();

        documents
            .iter()
            .filter(|(_, content)| {
                let links = self.extract_targets(content);
                links.iter().any(|link| link.to_lowercase() == target_lower)
            })
            .cloned()
            .collect()
    }

    /// Detect orphan pages (no inbound or outbound links)
    pub fn find_orphans(
        &self,
        all_titles: &[String],
        documents: &[(String, String)],
    ) -> Vec<String> {
        let mut linked: HashSet<String> = HashSet::new();
        let mut has_outbound: HashSet<String> = HashSet::new();

        for (doc_id, content) in documents {
            let links = self.extract_targets(content);

            if !links.is_empty() {
                has_outbound.insert(doc_id.clone());
            }

            for link in links {
                linked.insert(link.to_lowercase());
            }
        }

        all_titles
            .iter()
            .filter(|title| {
                let title_lower = title.to_lowercase();
                !linked.contains(&title_lower) && !has_outbound.contains(&title_lower)
            })
            .cloned()
            .collect()
    }
}

/// Represents a wiki link
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLink {
    pub target: String,
    pub display_text: String,
}

impl Default for WikiLinkExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_links() {
        let extractor = WikiLinkExtractor::new();
        let content = "See [[Rust Ownership]] for details.";

        let links = extractor.extract(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Rust Ownership");
        assert_eq!(links[0].display_text, "Rust Ownership");
    }

    #[test]
    fn test_extract_links_with_display_text() {
        let extractor = WikiLinkExtractor::new();
        let content = "See [[Rust Ownership|ownership system]] for details.";

        let links = extractor.extract(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Rust Ownership");
        assert_eq!(links[0].display_text, "ownership system");
    }

    #[test]
    fn test_extract_multiple_links() {
        let extractor = WikiLinkExtractor::new();
        let content = "Read about [[Rust]] and [[Ownership]] and [[Borrowing]].";

        let links = extractor.extract_targets(content);
        assert_eq!(links, vec!["Rust", "Ownership", "Borrowing"]);
    }

    #[test]
    fn test_count_links() {
        let extractor = WikiLinkExtractor::new();
        let content = "[[A]] and [[B]] and [[C]]";

        assert_eq!(extractor.count_links(content), 3);
    }

    #[test]
    fn test_has_links() {
        let extractor = WikiLinkExtractor::new();

        assert!(extractor.has_links("See [[Link]] here"));
        assert!(!extractor.has_links("No links here"));
    }

    #[test]
    fn test_find_backlinks() {
        let extractor = WikiLinkExtractor::new();

        let documents = vec![
            ("doc1".to_string(), "See [[Target]] here".to_string()),
            ("doc2".to_string(), "No links".to_string()),
            ("doc3".to_string(), "Also [[Target]]".to_string()),
        ];

        let backlinks = extractor.find_backlinks("Target", &documents);
        assert_eq!(backlinks.len(), 2);
        assert!(backlinks.iter().any(|(id, _)| id == "doc1"));
        assert!(backlinks.iter().any(|(id, _)| id == "doc3"));
    }

    #[test]
    fn test_find_orphans() {
        let extractor = WikiLinkExtractor::new();

        let all_titles = vec![
            "Linked".to_string(),
            "Orphan".to_string(),
            "Connector".to_string(),
        ];

        let documents = vec![
            ("linked".to_string(), "Links to [[Connector]]".to_string()),
            ("connector".to_string(), "Links to [[Linked]]".to_string()),
            // "orphan" has no links and is not linked to
        ];

        let orphans = extractor.find_orphans(&all_titles, &documents);
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0], "Orphan");
    }
}
