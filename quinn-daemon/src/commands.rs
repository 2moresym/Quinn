/// Split a short command sentence into independent model requests.
///
/// The splitter deliberately handles only command separators, not natural-language
/// parsing. Keeping this deterministic prevents a multi-action request from asking
/// Needle v2 to emit multiple tool calls in one constrained generation.
pub fn split_utterance(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let bytes = input.as_bytes();
    let mut i = 0;

    'scan: while i < bytes.len() {
        let ch = input[i..].chars().next().unwrap();
        let len = ch.len_utf8();

        if let Some(q) = quote {
            current.push(ch);
            if ch == q {
                quote = None;
            }
            i += len;
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            current.push(ch);
            i += len;
            continue;
        }

        if ch == '+' || ch == ',' {
            push_fragment(&mut out, &mut current);
            i += len;
            continue;
        }

        if ch.is_whitespace() {
            let rest = &input[i..];
            let lower = rest.to_ascii_lowercase();
            for word in ["and", "then"] {
                let marker = format!(" {word} ");
                if lower.starts_with(&marker) {
                    push_fragment(&mut out, &mut current);
                    i += marker.len();
                    while i < bytes.len() {
                        let next = input[i..].chars().next().unwrap();
                        if !next.is_whitespace() {
                            break;
                        }
                        i += next.len_utf8();
                    }
                    continue 'scan;
                }
            }
        }

        current.push(ch);
        i += len;
    }

    push_fragment(&mut out, &mut current);
    if out.is_empty() {
        vec![input.trim().to_string()]
    } else {
        // Carry an omitted leading action verb across conjunctions.
        // Example: "open Discord and Chrome" becomes
        // ["open Discord", "open Chrome"].
        if let Some(first) = out.first().cloned() {
            if let Some((verb, _)) = first.split_once(char::is_whitespace) {
                if is_repeatable_action(verb) {
                    for fragment in out.iter_mut().skip(1) {
                        if !starts_with_action(fragment) {
                            *fragment = format!("{verb} {fragment}");
                        }
                    }
                }
            }
        }
        out
    }
}

fn is_repeatable_action(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "open" | "launch" | "start" | "run" | "close" | "kill" | "stop" | "search"
    )
}

fn starts_with_action(fragment: &str) -> bool {
    fragment
        .split_whitespace()
        .next()
        .map(is_repeatable_action)
        .unwrap_or(false)
}

fn push_fragment(out: &mut Vec<String>, current: &mut String) {
    let fragment = current.trim();
    if !fragment.is_empty() {
        out.push(fragment.to_string());
    }
    current.clear();
}

#[cfg(test)]
mod tests {
    use super::split_utterance;

    #[test]
    fn splits_common_multi_commands() {
        assert_eq!(
            split_utterance("open Discord and Chrome + terminal"),
            vec!["open Discord", "open Chrome", "open terminal"]
        );
    }

    #[test]
    fn preserves_new_actions_after_conjunctions() {
        assert_eq!(
            split_utterance("open Discord and close Firefox"),
            vec!["open Discord", "close Firefox"]
        );
    }

    #[test]
    fn keeps_quoted_text_together() {
        assert_eq!(
            split_utterance("open \"Visual Studio Code\" and terminal"),
            vec!["open \"Visual Studio Code\"", "open terminal"]
        );
    }

    #[test]
    fn does_not_split_embedded_words() {
        assert_eq!(
            split_utterance("open android studio"),
            vec!["open android studio"]
        );
    }
}
