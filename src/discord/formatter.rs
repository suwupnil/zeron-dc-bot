pub struct MessageChunker;

impl MessageChunker {
    /// Split a long text into chunks <= max_len (default 1950 to be safely within Discord's 2000 limit)
    /// Preserves code blocks across boundaries so syntax highlighting and formatting remain intact.
    pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
        if text.len() <= max_len {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current_chunk = String::new();
        let mut active_code_block: Option<String> = None;

        for line in text.split_inclusive('\n') {
            // Check if this line toggles a code block
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                if active_code_block.is_some() {
                    active_code_block = None;
                } else {
                    let lang = trimmed.trim_start_matches('`').trim();
                    active_code_block = Some(lang.to_string());
                }
            }

            // If adding this line exceeds max_len
            if current_chunk.len() + line.len() > max_len && !current_chunk.is_empty() {
                // If we're inside a code block, close it for this chunk
                if let Some(ref _lang) = active_code_block {
                    current_chunk.push_str("\n```\n");
                }

                chunks.push(current_chunk);
                current_chunk = String::new();

                // Reopen the code block in the new chunk
                if let Some(ref lang) = active_code_block {
                    current_chunk.push_str(&format!("```{}\n", lang));
                }
            }

            // If a single line itself is longer than max_len (e.g. long URL or minified code)
            if line.len() > max_len {
                let mut start = 0;
                while start < line.len() {
                    let end = std::cmp::min(start + max_len, line.len());
                    chunks.push(line[start..end].to_string());
                    start = end;
                }
            } else {
                current_chunk.push_str(line);
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_message_not_split() {
        let text = "Hello world!";
        let chunks = MessageChunker::split_message(text, 100);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world!");
    }

    #[test]
    fn test_split_with_code_block() {
        let text = "First line\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\nLast line";
        let chunks = MessageChunker::split_message(text, 40);
        assert!(chunks.len() >= 2);
    }
}
