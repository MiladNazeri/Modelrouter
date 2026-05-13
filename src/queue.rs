use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueueJob {
    pub id: u64,
    pub prompt: String,
    pub status: QueueStatus,
    pub output: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct ExecutionQueue {
    next_id: Arc<AtomicU64>,
    jobs: Arc<Mutex<Vec<QueueJob>>>,
}

impl ExecutionQueue {
    pub fn submit<F>(&self, prompt: String, runner: F) -> u64
    where
        F: FnOnce(&str) -> Result<String, String>,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.insert(QueueJob {
            id,
            prompt: prompt.clone(),
            status: QueueStatus::Running,
            output: None,
            error: None,
        });

        let result = runner(&prompt);
        let mut jobs = self.jobs.lock().expect("queue lock");
        if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            match result {
                Ok(output) => {
                    job.status = QueueStatus::Completed;
                    job.output = Some(output);
                }
                Err(error) => {
                    job.status = QueueStatus::Failed;
                    job.error = Some(error);
                }
            }
        }
        id
    }

    pub fn get(&self, id: u64) -> Option<QueueJob> {
        self.jobs
            .lock()
            .expect("queue lock")
            .iter()
            .find(|job| job.id == id)
            .cloned()
    }

    pub fn list(&self) -> Vec<QueueJob> {
        self.jobs.lock().expect("queue lock").clone()
    }

    fn insert(&self, job: QueueJob) {
        self.jobs.lock().expect("queue lock").push(job);
    }
}
