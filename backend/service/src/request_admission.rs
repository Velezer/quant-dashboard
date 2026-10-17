use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// Admission counter for HTTP handlers; overload is rejected instead of queued.
pub(super) struct RequestAdmission {
    active: AtomicUsize,
    maximum: usize,
}

impl RequestAdmission {
    pub(super) fn new(maximum: usize) -> Self {
        assert!(maximum > 0, "request admission limit must be positive");
        Self {
            active: AtomicUsize::new(0),
            maximum,
        }
    }

    pub(super) fn try_acquire(self: Arc<Self>) -> Option<RequestPermit> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= self.maximum {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(RequestPermit { admission: self }),
                Err(updated) => active = updated,
            }
        }
    }
}

pub(super) struct RequestPermit {
    admission: Arc<RequestAdmission>,
}

impl Drop for RequestPermit {
    fn drop(&mut self) {
        self.admission.active.fetch_sub(1, Ordering::Release);
    }
}
