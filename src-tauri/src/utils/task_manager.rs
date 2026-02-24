//! Global task management for write operations
//! Provides cancellation support and progress tracking

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

/// Manages active write tasks
pub struct TaskManager {
    tasks: HashMap<String, TaskState>,
}

/// State of a write task
pub struct TaskState {
    pub task_id: String,
    pub cancel_flag: Arc<AtomicBool>,
}

lazy_static::lazy_static! {
    static ref GLOBAL_TASK_MANAGER: Mutex<TaskManager> = Mutex::new(TaskManager {
        tasks: HashMap::new(),
    });
}

impl TaskManager {
    /// Register a new task
    pub fn register_task(task_id: String) -> Arc<AtomicBool> {
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let state = TaskState {
            task_id: task_id.clone(),
            cancel_flag: cancel_flag.clone(),
        };

        if let Ok(mut mgr) = GLOBAL_TASK_MANAGER.lock() {
            mgr.tasks.insert(task_id, state);
        }

        cancel_flag
    }

    /// Request cancellation of a task
    pub fn cancel_task(task_id: &str) -> bool {
        if let Ok(mgr) = GLOBAL_TASK_MANAGER.lock() {
            if let Some(state) = mgr.tasks.get(task_id) {
                state.cancel_flag.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// Unregister a task (cleanup)
    pub fn unregister_task(task_id: &str) {
        if let Ok(mut mgr) = GLOBAL_TASK_MANAGER.lock() {
            mgr.tasks.remove(task_id);
        }
    }
}

/// Check if current task should be cancelled
pub fn is_cancelled(cancel_flag: &Arc<AtomicBool>) -> bool {
    cancel_flag.load(Ordering::Relaxed)
}
