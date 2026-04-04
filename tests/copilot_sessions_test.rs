use cc_speedy::copilot_sessions::parse_copilot_messages_from_path;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_parse_messages_user_and_assistant() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"Hello\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"Hi there\"}}\n",
        "{\"type\":\"tool.execution_start\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].text, "Hello");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].text, "Hi there");
}

#[test]
fn test_parse_messages_skips_empty_assistant_content() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"user.message\",\"data\":{\"content\":\"query\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"\"}}\n",
        "{\"type\":\"assistant.message\",\"data\":{\"content\":\"answer\"}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].text, "answer");
}

#[test]
fn test_parse_messages_skips_non_message_events() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("events.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"session.start\",\"data\":{}}\n",
        "{\"type\":\"assistant.turn_start\",\"data\":{}}\n",
        "{\"type\":\"user.message\",\"data\":{\"content\":\"only msg\"}}\n",
        "{\"type\":\"tool.execution_complete\",\"data\":{}}\n",
    )).unwrap();
    let msgs = parse_copilot_messages_from_path(&path).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "only msg");
}
