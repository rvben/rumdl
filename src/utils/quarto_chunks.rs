//! Parser for Quarto / RMarkdown executable code chunk metadata.
//!
//! Two label sources, both supported:
//! 1. Inline info string: ` ```{r, label="setup", echo=FALSE} `
//! 2. Hashpipe chunk options inside the block body: `#| label: setup`
//!
//! The inline form supports three shapes:
//! - Bare label as the first positional argument: `{r setup}` or `{r several words}`
//!   (multiple bare words before any `key=value` are treated as a whitespace-
//!   separated label; this is also how the linter detects spaces in labels).
//! - Explicit `label=value`: `{r, label=setup}` or `{r, label="my label"}`.
//! - Mixed forms like `{r setup, echo=FALSE}`.
//!
//! The grammar reflects how knitr/Quarto themselves parse chunk headers. We do
//! not aim for full knitr fidelity; the goal is to recognise the patterns that
//! drive the two lint rules using this helper (MD078, MD079).

/// Origin of a parsed label, mirrored from panache's `ChunkLabelSource` so
/// rules can distinguish inline-positional spaces (which are the strongest
/// signal of a typo) from quoted-string spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkLabelSource {
    /// A bare positional argument before any `key=value`, e.g. `{r setup}`.
    InlinePositional,
    /// An explicit `label=` argument, e.g. `{r, label=setup}` or `{r, label="my label"}`.
    InlineKey,
    /// A `#| label: setup` hashpipe option inside the block body.
    Hashpipe,
}

/// One label found while parsing a chunk header or body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkLabel {
    pub value: String,
    pub source: ChunkLabelSource,
}

/// Parsed inline chunk header — the part inside `{...}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineChunkHeader {
    /// Engine name, e.g. `r`, `python`. Empty if absent (malformed header).
    pub engine: String,
    /// Labels in declaration order. `InlinePositional` entries come first; if
    /// multiple bare positionals appear before the first `key=value`, they are
    /// all returned so MD079 can flag the implicit-spaces case.
    pub labels: Vec<ChunkLabel>,
}

/// Try to parse the info string of a fenced code block as a Quarto inline
/// chunk header. Accepts both `{r}` and `{r, label=foo}` shapes; returns
/// `None` for plain display blocks like ` ```r `.
pub fn parse_inline_chunk_header(info_string: &str) -> Option<InlineChunkHeader> {
    let trimmed = info_string.trim();
    let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?;

    let mut tokens = tokenize_chunk_args(inner);

    let engine = tokens.next().map(|t| t.value).unwrap_or_default();

    let mut labels: Vec<ChunkLabel> = Vec::new();
    let mut seen_kv = false;
    for tok in tokens {
        match tok.kind {
            TokenKind::Bare => {
                // Bare words before any key=value act as positional labels.
                // Bare words AFTER the first key=value are not labels (knitr
                // ignores stray bareword options).
                if !seen_kv {
                    labels.push(ChunkLabel {
                        value: tok.value,
                        source: ChunkLabelSource::InlinePositional,
                    });
                }
            }
            TokenKind::KeyValue { key } => {
                seen_kv = true;
                if key.eq_ignore_ascii_case("label") {
                    labels.push(ChunkLabel {
                        value: tok.value,
                        source: ChunkLabelSource::InlineKey,
                    });
                }
            }
        }
    }

    Some(InlineChunkHeader { engine, labels })
}

/// Scan the body of a fenced code block for hashpipe label options
/// (`#| label: setup`).
///
/// Only the contiguous run of hashpipe lines at the top of the block is
/// inspected, matching Quarto's own behaviour: chunk options must appear
/// before any code.
pub fn parse_hashpipe_labels(body: &str) -> Vec<ChunkLabel> {
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(after) = line.trim_start().strip_prefix("#|") else {
            // First non-hashpipe, non-blank line ends the option block.
            if line.trim().is_empty() {
                continue;
            }
            break;
        };
        let Some((key, value)) = after.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("label") {
            continue;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if value.is_empty() {
            continue;
        }
        out.push(ChunkLabel {
            value: value.to_string(),
            source: ChunkLabelSource::Hashpipe,
        });
    }
    out
}

/// Return `true` if the chunk header denotes an *executable* Quarto chunk.
///
/// Quarto treats braced info strings with a non-empty engine as executable.
/// Plain display blocks like ` ```r ` are not executable.
pub fn is_executable_chunk(info_string: &str) -> bool {
    parse_inline_chunk_header(info_string).is_some_and(|h| !h.engine.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Bare,
    KeyValue { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    value: String,
    kind: TokenKind,
}

/// Tokenize the body of a chunk header. Arguments are separated by commas or
/// whitespace; quoted strings preserve their interior (including spaces).
///
/// Returns an iterator over tokens. Each token is either a bare word or a
/// `key=value` pair (with the value unquoted).
fn tokenize_chunk_args(input: &str) -> impl Iterator<Item = Token> + '_ {
    ChunkArgIter {
        input,
        bytes: input.as_bytes(),
        pos: 0,
    }
}

struct ChunkArgIter<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Iterator for ChunkArgIter<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        // Skip separators: whitespace and commas.
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b',' || b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos >= self.bytes.len() {
            return None;
        }

        // Read a key or bare word: run of non-separator, non-`=`, non-quote chars.
        let key_start = self.pos;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b',' || b == b'=' || b.is_ascii_whitespace() {
                break;
            }
            if b == b'"' || b == b'\'' {
                break;
            }
            self.pos += 1;
        }
        let key = &self.input[key_start..self.pos];

        // No `=` follows -> bare token.
        if self.pos >= self.bytes.len() || self.bytes[self.pos] != b'=' {
            return Some(Token {
                value: key.to_string(),
                kind: TokenKind::Bare,
            });
        }

        // Consume `=` and read the value (quoted or unquoted).
        self.pos += 1;
        if self.pos >= self.bytes.len() {
            return Some(Token {
                value: String::new(),
                kind: TokenKind::KeyValue { key: key.to_string() },
            });
        }

        let value = match self.bytes[self.pos] {
            q @ (b'"' | b'\'') => {
                self.pos += 1;
                let val_start = self.pos;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != q {
                    self.pos += 1;
                }
                let val = self.input[val_start..self.pos].to_string();
                if self.pos < self.bytes.len() {
                    self.pos += 1; // consume closing quote
                }
                val
            }
            _ => {
                let val_start = self.pos;
                while self.pos < self.bytes.len() {
                    let b = self.bytes[self.pos];
                    if b == b',' || b.is_ascii_whitespace() {
                        break;
                    }
                    self.pos += 1;
                }
                self.input[val_start..self.pos].to_string()
            }
        };

        Some(Token {
            value,
            kind: TokenKind::KeyValue { key: key.to_string() },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(info: &str) -> InlineChunkHeader {
        parse_inline_chunk_header(info).expect("should parse")
    }

    #[test]
    fn plain_display_block_is_not_a_chunk_header() {
        assert!(parse_inline_chunk_header("r").is_none());
        assert!(parse_inline_chunk_header("python").is_none());
        assert!(parse_inline_chunk_header("").is_none());
    }

    #[test]
    fn bare_engine_has_no_label() {
        let h = header("{r}");
        assert_eq!(h.engine, "r");
        assert!(h.labels.is_empty());
    }

    #[test]
    fn inline_positional_label() {
        let h = header("{r setup}");
        assert_eq!(h.engine, "r");
        assert_eq!(h.labels.len(), 1);
        assert_eq!(h.labels[0].value, "setup");
        assert_eq!(h.labels[0].source, ChunkLabelSource::InlinePositional);
    }

    #[test]
    fn multiple_bare_words_are_all_positional() {
        let h = header("{r several words}");
        assert_eq!(h.engine, "r");
        let vals: Vec<&str> = h.labels.iter().map(|l| l.value.as_str()).collect();
        assert_eq!(vals, vec!["several", "words"]);
        assert!(h.labels.iter().all(|l| l.source == ChunkLabelSource::InlinePositional));
    }

    #[test]
    fn explicit_label_key() {
        let h = header("{r, label=setup}");
        assert_eq!(h.engine, "r");
        assert_eq!(h.labels.len(), 1);
        assert_eq!(h.labels[0].value, "setup");
        assert_eq!(h.labels[0].source, ChunkLabelSource::InlineKey);
    }

    #[test]
    fn quoted_label_with_spaces() {
        let h = header(r#"{r, label="my label"}"#);
        assert_eq!(h.labels.len(), 1);
        assert_eq!(h.labels[0].value, "my label");
        assert_eq!(h.labels[0].source, ChunkLabelSource::InlineKey);
    }

    #[test]
    fn positional_then_options_only_collects_first_as_label() {
        let h = header("{r setup, echo=FALSE}");
        assert_eq!(h.labels.len(), 1);
        assert_eq!(h.labels[0].value, "setup");
        assert_eq!(h.labels[0].source, ChunkLabelSource::InlinePositional);
    }

    #[test]
    fn bareword_after_kv_is_not_a_label() {
        // knitr ignores stray barewords after the first kv; we must not treat
        // them as labels or MD079 would falsely flag them.
        let h = header("{r, echo=FALSE stray}");
        assert!(h.labels.is_empty());
    }

    #[test]
    fn hashpipe_label_is_picked_up() {
        let labels = parse_hashpipe_labels("#| label: setup\n#| echo: false\n1 + 1\n");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].value, "setup");
        assert_eq!(labels[0].source, ChunkLabelSource::Hashpipe);
    }

    #[test]
    fn hashpipe_label_with_quotes() {
        let labels = parse_hashpipe_labels("#| label: \"setup\"\n");
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].value, "setup");
    }

    #[test]
    fn hashpipe_options_must_be_at_top_of_block() {
        // Once real code appears, later #| comments are not options.
        let labels = parse_hashpipe_labels("1 + 1\n#| label: too-late\n");
        assert!(labels.is_empty());
    }

    #[test]
    fn hashpipe_blank_lines_at_top_are_skipped() {
        let labels = parse_hashpipe_labels("\n#| label: setup\n");
        assert_eq!(labels.len(), 1);
    }

    #[test]
    fn hashpipe_value_without_colon_is_ignored() {
        let labels = parse_hashpipe_labels("#| label\n");
        assert!(labels.is_empty());
    }

    #[test]
    fn hashpipe_empty_value_is_ignored() {
        let labels = parse_hashpipe_labels("#| label:\n");
        assert!(labels.is_empty());
    }

    #[test]
    fn is_executable_chunk_recognises_braced_engines() {
        assert!(is_executable_chunk("{r}"));
        assert!(is_executable_chunk("{python}"));
        assert!(is_executable_chunk("{r, label=foo}"));
        assert!(!is_executable_chunk("r"));
        assert!(!is_executable_chunk("python"));
        assert!(!is_executable_chunk(""));
    }

    #[test]
    fn is_executable_chunk_rejects_empty_engine() {
        // `{}` and `{ , label=foo}` have no engine.
        assert!(!is_executable_chunk("{}"));
        assert!(!is_executable_chunk("{ }"));
    }
}
