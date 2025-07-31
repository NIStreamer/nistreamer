use std::sync::Arc;

pub fn new_drop_alarm(n_handles: usize) -> Vec<DropAlarmHandle> {
    let seed_arc = Arc::new(());
    let mut handles = Vec::new();
    for _ in 0..n_handles {
        handles.push(DropAlarmHandle::new(seed_arc.clone(), n_handles))
    }
    handles
}

pub struct DropAlarmHandle {
    arc: Arc<()>,
    n_handles: usize,
}

impl DropAlarmHandle {
    pub fn new(arc: Arc<()>, n_handles: usize) -> Self {
        Self { arc, n_handles }
    }

    pub fn drop_detected(&self) -> bool {
        Arc::strong_count(&self.arc) < self.n_handles
    }
}