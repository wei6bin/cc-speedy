//! Per-session shared file tailer with broadcast. Stub — full
//! implementation in the next task batch.

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct TailerRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<String, ()>>>,
}
