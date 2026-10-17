use super::request_admission::RequestAdmission;
use std::sync::Arc;

#[test]
fn request_admission_rejects_over_capacity_and_recovers_on_drop() {
    let admission = Arc::new(RequestAdmission::new(2));
    let first = admission.clone().try_acquire().expect("first permit");
    let second = admission.clone().try_acquire().expect("second permit");

    assert!(admission.clone().try_acquire().is_none());
    drop(first);

    let third = admission
        .clone()
        .try_acquire()
        .expect("capacity is released");
    assert!(admission.clone().try_acquire().is_none());
    drop((second, third));
    assert!(admission.clone().try_acquire().is_some());
}

#[test]
fn request_admission_is_safe_under_concurrent_claims() {
    let admission = Arc::new(RequestAdmission::new(4));
    let release = Arc::new(std::sync::Barrier::new(5));
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..32 {
        let admission = Arc::clone(&admission);
        let release = Arc::clone(&release);
        let sender = sender.clone();
        workers.push(std::thread::spawn(move || {
            let permit = admission.try_acquire();
            sender
                .send(permit.is_some())
                .expect("send admission result");
            if permit.is_some() {
                release.wait();
            }
            drop(permit);
        }));
    }
    drop(sender);

    let admitted = (0..32)
        .filter(|_| receiver.recv().expect("admission result"))
        .count();
    assert_eq!(admitted, 4);
    release.wait();
    for worker in workers {
        worker.join().expect("admission worker");
    }
}
