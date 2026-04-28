//! Per-session shared file tailer with broadcast. Multiple SSE
//! subscribers to the same session share one tailer; the tailer task
//! self-cleans when the last subscriber disconnects.

use crate::unified::SessionSource;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TailEvent {
    TurnAdded { idx: u32 },
    TurnUpdated { idx: u32 },
    LivenessChanged { state: crate::liveness::Liveness },
}

const BROADCAST_CAPACITY: usize = 64;
const POLL_INTERVAL_MS: u64 = 1000;

#[derive(Default, Debug)]
pub struct TailerState {
    /// Number of complete user/assistant pairs observed so far. The
    /// "current" turn index is `pair_count - 1`; the next added turn
    /// index would be `pair_count`.
    pair_count: u32,
    /// True if the most recent assistant message had an unclosed
    /// `tool_use`. A subsequent `tool_result` turns this off and emits
    /// `TurnUpdated` for the same idx.
    open_turn: bool,
    /// Last reported open-turn idx (if any), so we can emit
    /// `TurnUpdated` against the right index.
    last_open_idx: Option<u32>,
}

/// Pure classifier: given the previous state and the new text appended
/// to the JSONL since last call, return the new state and the list of
/// events to broadcast. Source-specific (CC vs Copilot) parsing is
/// dispatched on `source`.
pub fn classify_new_lines(
    source: SessionSource,
    prev: TailerState,
    new_text: &str,
) -> (TailerState, Vec<TailEvent>) {
    let mut state = prev;
    let mut events = Vec::new();
    for line in new_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match source {
            SessionSource::ClaudeCode => classify_cc_line(&v, &mut state, &mut events),
            SessionSource::Copilot => classify_copilot_line(&v, &mut state, &mut events),
            SessionSource::OpenCode => { /* no-op; OC has no JSONL */ }
        }
    }
    (state, events)
}

fn classify_cc_line(v: &serde_json::Value, state: &mut TailerState, events: &mut Vec<TailEvent>) {
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let blocks: Vec<serde_json::Value> = match content {
        serde_json::Value::Array(a) => a,
        _ => Vec::new(),
    };
    match ty {
        "assistant" => {
            let idx = state.pair_count;
            state.pair_count = state.pair_count.saturating_add(1);
            let opens = blocks
                .iter()
                .any(|b| b.get("type").and_then(|x| x.as_str()) == Some("tool_use"));
            state.open_turn = opens;
            state.last_open_idx = if opens { Some(idx) } else { None };
            events.push(TailEvent::TurnAdded { idx });
        }
        "user" => {
            let closes = blocks
                .iter()
                .any(|b| b.get("type").and_then(|x| x.as_str()) == Some("tool_result"));
            if closes && state.open_turn {
                state.open_turn = false;
                if let Some(idx) = state.last_open_idx.take() {
                    events.push(TailEvent::TurnUpdated { idx });
                }
            }
        }
        _ => {}
    }
}

fn classify_copilot_line(
    v: &serde_json::Value,
    state: &mut TailerState,
    events: &mut Vec<TailEvent>,
) {
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match ty {
        "assistant.message" => {
            let idx = state.pair_count;
            state.pair_count = state.pair_count.saturating_add(1);
            state.open_turn = true;
            state.last_open_idx = Some(idx);
            events.push(TailEvent::TurnAdded { idx });
        }
        "tool.execution_complete" => {
            if state.open_turn {
                state.open_turn = false;
                if let Some(idx) = state.last_open_idx.take() {
                    events.push(TailEvent::TurnUpdated { idx });
                }
            }
        }
        _ => {}
    }
}

/// Per-session tailer — one task, multiple subscribers via broadcast.
pub struct Tailer {
    pub broadcast: broadcast::Sender<TailEvent>,
    pub refcount: Arc<AtomicU32>,
}

impl Tailer {
    pub fn subscribe(&self) -> broadcast::Receiver<TailEvent> {
        self.refcount.fetch_add(1, Ordering::SeqCst);
        self.broadcast.subscribe()
    }

    pub fn release(&self) {
        self.refcount.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Default)]
pub struct TailerRegistry {
    inner: Arc<tokio::sync::Mutex<HashMap<String, Arc<Tailer>>>>,
}

impl TailerRegistry {
    /// Get-or-create the tailer for `session_id`. If created, spawns
    /// the polling task. The caller MUST `subscribe()` immediately to
    /// bump the refcount before dropping the `Arc<Tailer>`.
    pub async fn ensure(
        &self,
        session_id: &str,
        source: SessionSource,
        jsonl: PathBuf,
    ) -> Arc<Tailer> {
        let mut map = self.inner.lock().await;
        if let Some(t) = map.get(session_id) {
            return t.clone();
        }
        let (tx, _) = broadcast::channel::<TailEvent>(BROADCAST_CAPACITY);
        let tailer = Arc::new(Tailer {
            broadcast: tx.clone(),
            refcount: Arc::new(AtomicU32::new(0)),
        });
        let registry = self.clone();
        let session_id_owned = session_id.to_string();
        let refcount = tailer.refcount.clone();
        tokio::spawn(async move {
            run_tailer_task(source, jsonl, tx, refcount, registry, session_id_owned).await
        });
        map.insert(session_id.to_string(), tailer.clone());
        tailer
    }

    pub async fn contains(&self, session_id: &str) -> bool {
        self.inner.lock().await.contains_key(session_id)
    }
}

async fn run_tailer_task(
    source: SessionSource,
    path: PathBuf,
    tx: broadcast::Sender<TailEvent>,
    refcount: Arc<AtomicU32>,
    registry: TailerRegistry,
    session_id: String,
) {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut state = TailerState::default();
    let mut last_size: u64 = 0;

    // Bootstrap: read the entire file once so subscribers who join later
    // see the *current* turn count rather than starting from zero. We
    // do not emit historical events on bootstrap.
    if let Ok(meta) = tokio::fs::metadata(&path).await {
        last_size = meta.len();
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let (next, _events) = classify_new_lines(source.clone(), state, &content);
            state = next;
        }
    }

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(POLL_INTERVAL_MS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        interval.tick().await;

        if refcount.load(Ordering::SeqCst) == 0 {
            let mut map = registry.inner.lock().await;
            map.remove(&session_id);
            return;
        }

        let meta = match tokio::fs::metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size = meta.len();
        if size <= last_size {
            continue;
        }
        let mut f = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(_) => continue,
        };
        if f.seek(std::io::SeekFrom::Start(last_size)).await.is_err() {
            continue;
        }
        let mut buf = Vec::with_capacity((size - last_size) as usize);
        if f.read_to_end(&mut buf).await.is_err() {
            continue;
        }
        last_size = size;
        let new_text = String::from_utf8_lossy(&buf).into_owned();
        let (next_state, events) = classify_new_lines(source.clone(), state, &new_text);
        state = next_state;
        for ev in events {
            let _ = tx.send(ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc_state() -> TailerState {
        TailerState::default()
    }

    #[test]
    fn cc_assistant_text_emits_turn_added() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 1);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
    }

    #[test]
    fn cc_assistant_with_tool_use_marks_open_turn() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"X","input":{}}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(st.open_turn);
        assert_eq!(st.last_open_idx, Some(0));
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
    }

    #[test]
    fn cc_tool_result_closes_previous_turn() {
        let s = cc_state();
        let txt = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"X","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}
"#;
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
        assert!(matches!(ev[1], TailEvent::TurnUpdated { idx: 0 }));
    }

    #[test]
    fn copilot_assistant_then_tool_complete() {
        let s = cc_state();
        let txt = r#"{"type":"assistant.message"}
{"type":"tool.execution_complete"}
"#;
        let (st, ev) = classify_new_lines(SessionSource::Copilot, s, txt);
        assert_eq!(st.pair_count, 1);
        assert!(!st.open_turn);
        assert_eq!(ev.len(), 2);
        assert!(matches!(ev[0], TailEvent::TurnAdded { idx: 0 }));
        assert!(matches!(ev[1], TailEvent::TurnUpdated { idx: 0 }));
    }

    #[test]
    fn opencode_emits_no_events() {
        let s = cc_state();
        let txt = r#"{"type":"whatever"}
"#;
        let (st, ev) = classify_new_lines(SessionSource::OpenCode, s, txt);
        assert_eq!(st.pair_count, 0);
        assert!(ev.is_empty());
    }

    #[test]
    fn malformed_lines_skipped() {
        let s = cc_state();
        let txt = "not json\n\n";
        let (st, ev) = classify_new_lines(SessionSource::ClaudeCode, s, txt);
        assert_eq!(st.pair_count, 0);
        assert!(ev.is_empty());
    }

    #[tokio::test]
    async fn registry_creates_and_caches_tailer() {
        let reg = TailerRegistry::default();
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");
        std::fs::write(&path, "").unwrap();

        let t1 = reg
            .ensure("a", SessionSource::ClaudeCode, path.clone())
            .await;
        let t2 = reg
            .ensure("a", SessionSource::ClaudeCode, path.clone())
            .await;
        assert!(Arc::ptr_eq(&t1, &t2));
    }
}
