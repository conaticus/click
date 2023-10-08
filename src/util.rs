use std::{
    future::Future,
    sync::{
        atomic::{self, AtomicUsize},
        Arc,
    },
    thread::{self},
    time::Duration,
};

use atomic::Ordering::SeqCst;
use bytes::Bytes;
use flate2::bufread::GzDecoder;
use tar::Archive;
use tokio::task::JoinHandle;

use crate::errors::CommandError;

pub fn extract_tarball(bytes: Bytes, dest: String) -> Result<(), CommandError> {
    let bytes = &bytes.to_vec()[..];
    let gz = GzDecoder::new(bytes);
    let mut archive = Archive::new(gz);

    // All tarballs contain a /package directory to the module source, this should be removed later to keep things as clean as possible
    archive
        .unpack(&dest)
        .map_err(CommandError::ExtractionFailed)
}

#[derive(Default)]
pub struct TaskAllocator {
    active_tasks: Arc<AtomicUsize>,
}

impl TaskAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_task<T>(&self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let active_tasks = Arc::clone(&self.active_tasks);

        tokio::spawn(async move {
            active_tasks.fetch_add(1, SeqCst);
            let future_result = future.await;
            active_tasks.fetch_sub(1, SeqCst);

            future_result
        })
    }

    pub fn add_blocking<F, R>(&self, f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let active_tasks = Arc::clone(&self.active_tasks);

        tokio::task::spawn_blocking(move || {
            active_tasks.fetch_add(1, SeqCst);
            let task_result = f();
            active_tasks.fetch_sub(1, SeqCst);

            task_result
        })
    }

    pub fn block_until_done(&self) {
        while self.task_count() != 0 {
            thread::sleep(Duration::from_millis(1));
        }
    }

    fn task_count(&self) -> usize {
        self.active_tasks.load(SeqCst)
    }
}
