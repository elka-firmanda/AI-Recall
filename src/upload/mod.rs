use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, error};
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::mcp::handler::MemoryMcpHandler;
use crate::extractors::PdfExtractor;
use crate::models::{AddMemoryRequest, MemoryType};

/// Upload status
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

/// Upload job
#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadJob {
    pub id: String,
    pub filename: String,
    pub size: usize,
    pub status: UploadStatus,
    pub progress: u8,
    pub created_at: chrono::DateTime<Utc>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub error: Option<String>,
    pub memory_id: Option<String>,
}

impl UploadJob {
    pub fn new(filename: String, size: usize) -> Self {
        Self {
            id: format!("upload_{}", Uuid::new_v4()),
            filename,
            size,
            status: UploadStatus::Pending,
            progress: 0,
            created_at: Utc::now(),
            completed_at: None,
            error: None,
            memory_id: None,
        }
    }
}

/// Queue statistics
#[derive(Debug, Default, serde::Serialize)]
pub struct QueueStats {
    pub pending: usize,
    pub processing: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Upload queue manager
pub struct UploadQueue {
    jobs: Arc<RwLock<VecDeque<UploadJob>>>,
    processing: Arc<RwLock<Vec<UploadJob>>>,
    handler: Arc<MemoryMcpHandler>,
    config: Arc<AppConfig>,
    extractor: PdfExtractor,
    tx: mpsc::Sender<String>,
    rx: Arc<Mutex<mpsc::Receiver<String>>>,
}

impl UploadQueue {
    pub fn new(handler: Arc<MemoryMcpHandler>, config: Arc<AppConfig>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let extractor = PdfExtractor::new(&config.storage.data_dir);
        
        Self {
            jobs: Arc::new(RwLock::new(VecDeque::new())),
            processing: Arc::new(RwLock::new(Vec::new())),
            handler,
            config,
            extractor,
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Start the background processor
    pub fn start_processor(&self) -> JoinHandle<()> {
        let jobs = self.jobs.clone();
        let processing = self.processing.clone();
        let handler = self.handler.clone();
        let config = self.config.clone();
        let extractor = self.extractor.clone();
        let rx = self.rx.clone();

        tokio::spawn(async move {
            let mut rx = rx.lock().await;
            
            while let Some(job_id) = rx.recv().await {
                // Get the job from pending
                let job = {
                    let mut queue = jobs.write().await;
                    queue.iter_mut().find(|j| j.id == job_id).cloned()
                };

                if let Some(mut job) = job {
                    // Move to processing
                    {
                        let mut proc = processing.write().await;
                        job.status = UploadStatus::Processing;
                        job.progress = 10;
                        proc.push(job.clone());
                    }

                    // Process the file
                    let result = process_file(&job, &handler, &config, &extractor).await;

                    // Update job status
                    let mut proc = processing.write().await;
                    if let Some(pos) = proc.iter().position(|j| j.id == job_id) {
                        let mut job = proc.remove(pos);
                        
                        match result {
                            Ok(memory_id) => {
                                job.status = UploadStatus::Completed;
                                job.progress = 100;
                                job.memory_id = Some(memory_id);
                                job.completed_at = Some(Utc::now());
                                info!("Upload {} completed: {}", job.id, job.filename);
                            }
                            Err(e) => {
                                job.status = UploadStatus::Failed;
                                job.error = Some(e.to_string());
                                job.completed_at = Some(Utc::now());
                                error!("Upload {} failed: {}", job.id, e);
                            }
                        }

                        // Add to queue with updated status
                        let mut queue = jobs.write().await;
                        if let Some(pos) = queue.iter().position(|j| j.id == job_id) {
                            queue[pos] = job;
                        }
                    }
                }
            }
        })
    }

    /// Add a new upload job
    pub async fn add_job(&self, filename: String, size: usize, data: Vec<u8>) -> Result<String> {
        let job = UploadJob::new(filename.clone(), size);
        let job_id = job.id.clone();
        
        // Save file temporarily
        let temp_dir = self.config.storage.data_dir.join(".temp");
        let safe_filename = filename.replace(['/', '\\', ':'], "_");
        let temp_path = temp_dir.join(format!("{}_{}", job_id, safe_filename));
        tokio::fs::create_dir_all(&temp_dir).await?;
        tokio::fs::write(&temp_path, data).await?;
        
        // Add to queue
        {
            let mut queue = self.jobs.write().await;
            queue.push_back(job);
        }

        // Notify processor
        self.tx.send(job_id.clone()).await?;
        
        info!("Added upload job {}: {} ({} bytes)", job_id, filename, size);
        Ok(job_id)
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> QueueStats {
        let queue = self.jobs.read().await;
        let processing = self.processing.read().await;
        
        QueueStats {
            pending: queue.iter().filter(|j| j.status == UploadStatus::Pending).count(),
            processing: processing.len(),
            completed: queue.iter().filter(|j| j.status == UploadStatus::Completed).count(),
            failed: queue.iter().filter(|j| j.status == UploadStatus::Failed).count(),
        }
    }

    /// Get all jobs (for display)
    pub async fn get_jobs(&self) -> Vec<UploadJob> {
        let queue = self.jobs.read().await;
        queue.iter().cloned().collect()
    }

    /// Get a specific job
    pub async fn get_job(&self, job_id: &str) -> Option<UploadJob> {
        let queue = self.jobs.read().await;
        queue.iter().find(|j| j.id == job_id).cloned()
    }

    /// Clean up old completed jobs (keep last 100)
    pub async fn cleanup(&self) {
        let mut queue = self.jobs.write().await;
        
        // Separate completed/failed from pending/processing
        let mut completed: Vec<_> = queue
            .iter()
            .filter(|j| matches!(j.status, UploadStatus::Completed | UploadStatus::Failed))
            .cloned()
            .collect();
        
        let active: VecDeque<_> = queue
            .iter()
            .filter(|j| !matches!(j.status, UploadStatus::Completed | UploadStatus::Failed))
            .cloned()
            .collect();
        
        // Keep only last 100 completed
        completed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        completed.truncate(100);
        
        // Rebuild queue
        queue.clear();
        queue.extend(active);
        queue.extend(completed);
    }
}

/// Process a single file
async fn process_file(
    job: &UploadJob,
    handler: &MemoryMcpHandler,
    config: &AppConfig,
    extractor: &PdfExtractor,
) -> Result<String> {
    let temp_dir = config.storage.data_dir.join(".temp");
    let safe_filename = job.filename.replace(['/', '\\', ':'], "_");
    let temp_path = temp_dir.join(format!("{}_{}", job.id, safe_filename));
    
    // Read file
    let data = tokio::fs::read(&temp_path).await?;
    
    // Extract content based on file type
    let extracted = if job.filename.ends_with(".pdf") {
        extractor.extract(&data).await?
    } else {
        // Fallback to text
        String::from_utf8_lossy(&data).to_string()
    };
    
    // Clean up temp file
    let _ = tokio::fs::remove_file(&temp_path).await;
    
    // Create memory
    let title = job.filename
        .trim_end_matches(".pdf")
        .trim_end_matches(".PDF")
        .replace("_", " ");
    
    let request = AddMemoryRequest {
        title,
        content: extracted,
        memory_type: MemoryType::Semantic,
        tags: vec!["uploaded".to_string(), "pdf".to_string()],
        source_refs: vec![],
        confidence: Some(0.8),
        auto_link: true,
    };
    
    let result = handler.memory_add(request).await?;
    Ok(result.id)
}
