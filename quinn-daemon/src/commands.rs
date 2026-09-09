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
        out
    }
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
            vec!["open Discord", "Chrome", "terminal"]
        );
    }

    #[test]
    fn keeps_quoted_text_together() {
        assert_eq!(
            split_utterance("open \"Visual Studio Code\" and terminal"),
            vec!["open \"Visual Studio Code\"", "terminal"]
        );
    }

    #[test]
    fn does_not_split_embedded_words() {
        assert_eq!(split_utterance("open android studio"), vec!["open android studio"]);
    }
}
