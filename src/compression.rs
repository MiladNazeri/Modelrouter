use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompressedPrompt {
    pub prompt: String,
    pub original_chars: usize,
    pub compressed_chars: usize,
    pub omitted_chars: usize,
    pub estimated_saved_tokens: u32,
}

pub fn compress_prompt(prompt: &str, max_chars: usize) -> CompressedPrompt {
    let original_chars = prompt.chars().count();
    if original_chars <= max_chars || max_chars < 80 {
        return CompressedPrompt {
            prompt: prompt.to_string(),
            original_chars,
            compressed_chars: original_chars,
            omitted_chars: 0,
            estimated_saved_tokens: 0,
        };
    }

    let important = important_lines(prompt);
    let marker = format!(
        "\n\n[modelrouter compressed {} chars from the middle]\n\n",
        original_chars.saturating_sub(max_chars)
    );
    let budget = max_chars.saturating_sub(marker.chars().count());
    let head_budget = budget / 2;
    let tail_budget = budget.saturating_sub(head_budget);
    let head = take_chars(prompt, head_budget);
    let tail = take_last_chars(prompt, tail_budget);
    let mut compressed = format!("{head}{marker}{tail}");

    for line in important {
        if compressed.contains(&line) {
            continue;
        }
        let candidate = format!("{compressed}\n{line}");
        if candidate.chars().count() <= max_chars {
            compressed = candidate;
        }
    }

    let compressed = trim_to_max_chars(&compressed, max_chars);
    let compressed_chars = compressed.chars().count();
    let omitted_chars = original_chars.saturating_sub(compressed_chars);

    CompressedPrompt {
        prompt: compressed,
        original_chars,
        compressed_chars,
        omitted_chars,
        estimated_saved_tokens: u32::try_from(omitted_chars / 4).unwrap_or(u32::MAX),
    }
}

fn important_lines(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter(|line| {
            let normalized = line.trim().to_ascii_lowercase();
            normalized.starts_with("action:")
                || normalized.starts_with("tag:")
                || normalized.starts_with("todo:")
                || normalized.starts_with("fix:")
        })
        .map(|line| line.trim().to_string())
        .collect()
}

fn take_chars(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn take_last_chars(value: &str, count: usize) -> String {
    let mut chars = value.chars().rev().take(count).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect()
}

fn trim_to_max_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
