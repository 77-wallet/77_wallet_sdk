use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct FakeUploader {
    reject: AtomicBool,
    calls: std::sync::Mutex<Vec<(String, String, Vec<u8>)>>,
}
#[async_trait::async_trait]
impl LogUploader for FakeUploader {
    async fn upload(
        &self,
        sn: &str,
        filename: &str,
        data: Vec<u8>,
    ) -> Result<(), crate::error::system::SystemError> {
        self.calls.lock().unwrap().push((sn.into(), filename.into(), data));
        if self.reject.load(Ordering::SeqCst) {
            Err(crate::error::system::SystemError::Service("synthetic rejection".into()))
        } else {
            Ok(())
        }
    }
}

fn consumer(path: LogBasePath, uploader: Arc<FakeUploader>) -> UploadLogConsumer {
    let (_, rx) = broadcast::channel(1);
    UploadLogConsumer::new(rx, path, 300, uploader, "device-sn".into())
}

#[tokio::test]
async fn failed_upload_preserves_checkpoint_and_retry_sends_same_pending_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let base = LogBasePath(dir.path().into());
    let pending = vec![b'x'; 2048];
    let mut content = b"file-uid\nalready-sent\n".to_vec();
    let start = content.len() as u64;
    content.extend_from_slice(&pending);
    tokio::fs::write(base.log_path(), &content).await.unwrap();
    let mut tracker = OffsetTracker::new(base.offset_path()).await;
    tracker.set_uid("file-uid".into());
    tracker.set_offset(start);
    tracker.save().await;
    let before = tokio::fs::read(base.offset_path()).await.unwrap();
    let fake = Arc::new(FakeUploader::default());
    fake.reject.store(true, Ordering::SeqCst);
    let worker = consumer(base.clone(), fake.clone());
    assert!(worker.upload_pending(&mut tracker).await.is_err());
    assert_eq!(tracker.get_offset(), start);
    assert_eq!(tracker.get_uid(), "file-uid");
    assert_eq!(tokio::fs::read(base.offset_path()).await.unwrap(), before);
    fake.reject.store(false, Ordering::SeqCst);
    worker.upload_pending(&mut tracker).await.unwrap();
    let loaded = OffsetTracker::new(base.offset_path()).await;
    assert_eq!(loaded.get_offset(), content.len() as u64);
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    for (sn, filename, bytes) in calls.iter() {
        assert_eq!(sn, "device-sn");
        assert!(filename.starts_with("sdk:"));
        assert_eq!(bytes, &pending);
    }
}

#[tokio::test]
async fn rotated_file_commits_new_identity_only_after_acknowledgement() {
    let dir = tempfile::tempdir().unwrap();
    let base = LogBasePath(dir.path().into());
    let pending = vec![b'y'; 1024];
    let mut content = b"new-uid\n".to_vec();
    content.extend_from_slice(&pending);
    tokio::fs::write(base.log_path(), &content).await.unwrap();
    let mut tracker = OffsetTracker::new(base.offset_path()).await;
    tracker.set_uid("old-uid".into());
    tracker.set_offset(9000);
    tracker.save().await;
    let fake = Arc::new(FakeUploader::default());
    fake.reject.store(true, Ordering::SeqCst);
    let worker = consumer(base.clone(), fake.clone());
    assert!(worker.upload_pending(&mut tracker).await.is_err());
    assert_eq!(tracker.get_uid(), "old-uid");
    assert_eq!(tracker.get_offset(), 9000);
    fake.reject.store(false, Ordering::SeqCst);
    worker.upload_pending(&mut tracker).await.unwrap();
    let loaded = OffsetTracker::new(base.offset_path()).await;
    assert_eq!(loaded.get_uid(), "new-uid");
    assert_eq!(loaded.get_offset(), content.len() as u64);
    assert_eq!(fake.calls.lock().unwrap()[1].2, pending);
}

#[tokio::test]
async fn less_than_one_kib_waits_without_upload_or_checkpoint_change() {
    let dir = tempfile::tempdir().unwrap();
    let base = LogBasePath(dir.path().into());
    tokio::fs::write(base.log_path(), b"uid\nsmall log").await.unwrap();
    let mut tracker = OffsetTracker::new(base.offset_path()).await;
    let fake = Arc::new(FakeUploader::default());
    consumer(base.clone(), fake.clone()).upload_pending(&mut tracker).await.unwrap();
    assert!(fake.calls.lock().unwrap().is_empty());
    assert_eq!(tracker.get_offset(), 0);
    assert_eq!(tracker.get_uid(), "");
    assert!(!base.offset_path().exists());
}

#[tokio::test]
async fn missing_config_starts_no_background_upload_task() {
    let dir = tempfile::tempdir().unwrap();
    let handle =
        UploadLogHandle::new(LogBasePath(dir.path().into()), 300, None, "device-sn".into()).await;
    assert!(handle.handle.lock().await.is_none());
    handle.close().await.unwrap();
}
