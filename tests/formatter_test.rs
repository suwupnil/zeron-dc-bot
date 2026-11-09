use zeron_dc_bot::discord::formatter::MessageChunker;

#[test]
fn test_short_message_remains_single_chunk() {
    let msg = "Hello from Zeron bot!";
    let chunks = MessageChunker::split_message(msg, 2000);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], msg);
}

#[test]
fn test_empty_message() {
    let chunks = MessageChunker::split_message("", 100);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], "");
}

#[test]
fn test_split_preserves_code_blocks() {
    let code_msg = "Here is the code:\n```rust\nfn main() {\n    let answer = 42;\n    println!(\"{}\", answer);\n}\n```\nDone.";
    // Split with small limit to force chunking across code block
    let chunks = MessageChunker::split_message(code_msg, 60);
    assert!(
        chunks.len() >= 2,
        "Expected multiple chunks, got {}",
        chunks.len()
    );

    // First chunk should have closed the block
    let first = &chunks[0];
    assert!(first.contains("```rust"));
    assert!(first.ends_with("```\n"));

    // Second chunk should have reopened the block
    let second = &chunks[1];
    assert!(second.starts_with("```rust\n"));
}

#[test]
fn test_long_single_line() {
    let long_line = "A".repeat(500);
    let chunks = MessageChunker::split_message(&long_line, 100);
    assert_eq!(chunks.len(), 5);
    for chunk in chunks {
        assert_eq!(chunk.len(), 100);
    }
}
