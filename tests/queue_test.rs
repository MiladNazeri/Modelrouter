use modelrouter::{ExecutionQueue, QueueStatus};

#[test]
fn execution_queue_tracks_completed_jobs() {
    let queue = ExecutionQueue::default();

    let id = queue.submit("hello".to_string(), |prompt| Ok(format!("ran {prompt}")));
    let job = queue.get(id).expect("job");

    assert_eq!(job.status, QueueStatus::Completed);
    assert_eq!(job.output.as_deref(), Some("ran hello"));
}

#[test]
fn execution_queue_tracks_failed_jobs() {
    let queue = ExecutionQueue::default();

    let id = queue.submit("hello".to_string(), |_prompt| {
        Err("provider failed".to_string())
    });
    let job = queue.get(id).expect("job");

    assert_eq!(job.status, QueueStatus::Failed);
    assert_eq!(job.error.as_deref(), Some("provider failed"));
}
