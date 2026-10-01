//! The queue in front of the workers: one waiting job per show and kind,
//! taken in turn.
//!
//! A new keyframe for a show whose last one is still waiting replaces it, so
//! the newest picture is the one decoded and a show never has more than one
//! picture and one burst of sound waiting. Shows are served in the order they
//! became ready. When there are more keyframes than the workers can decode,
//! every show is still served, each a little less often: two hundred shows on
//! two workers that can each decode a hundred pictures a second get one a
//! second; on one such worker, one every two seconds. Nothing waits on a
//! worker and nothing grows.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::show::lock;
use super::work::Job;

#[derive(Default)]
struct Waiting {
    /// Jobs by (show, kind), the newest only.
    jobs: HashMap<(String, u8), Job>,
    /// The order they became ready in.
    order: VecDeque<(String, u8)>,
    closed: bool,
}

#[derive(Default)]
pub struct Queue {
    waiting: Mutex<Waiting>,
    ready: Condvar,
    /// Jobs a newer one replaced before a worker got to them.
    pub replaced: AtomicU64,
    pub done: AtomicU64,
}

impl Queue {
    pub fn offer(&self, job: Job) {
        let key = job.key();
        let mut w = lock(&self.waiting);
        if w.jobs.insert(key.clone(), job).is_some() {
            self.replaced.fetch_add(1, Ordering::Relaxed);
        } else {
            w.order.push_back(key);
        }
        drop(w);
        self.ready.notify_one();
    }

    /// The next job, waiting for one. `None` once closed.
    pub fn next(&self) -> Option<Job> {
        let mut w = lock(&self.waiting);
        loop {
            if w.closed {
                return None;
            }
            while let Some(key) = w.order.pop_front() {
                if let Some(job) = w.jobs.remove(&key) {
                    return Some(job);
                }
            }
            w = self.ready.wait(w).unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn close(&self) {
        lock(&self.waiting).closed = true;
        self.ready.notify_all();
    }
}

/// The workers and their queue. Dropping it stops them.
pub struct Pool {
    pub queue: Arc<Queue>,
}

impl Pool {
    pub fn start(workers: usize) -> Pool {
        let queue = Arc::new(Queue::default());
        for n in 0..workers.max(1) {
            let q = queue.clone();
            std::thread::Builder::new()
                .name(format!("vitals-{n}"))
                .spawn(move || super::work::run(&q))
                .expect("a vitals worker thread");
        }
        Pool { queue }
    }

    pub fn offer(&self, job: Job) {
        self.queue.offer(job);
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.queue.close();
    }
}
