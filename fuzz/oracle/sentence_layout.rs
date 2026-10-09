//! Sentence-layout expectations built from a grammar, without consulting the
//! production sentence splitter. Shared by property tests and libFuzzer.

use super::reflow_semantics::{Mode, ReflowSettings, check, reflow};

pub struct SentenceLayoutCase {
    pub input: String,
    pub expected: String,
    pub settings: ReflowSettings,
}

const SPANS: [(&str, &str); 7] = [
    ("*", "*"),
    ("**", "**"),
    ("_", "_"),
    ("__", "__"),
    ("***", "***"),
    ("_**", "**_"),
    ("~~", "~~"),
];

/// The bytes choose grammatical alternatives, never arbitrary prose that the
/// oracle would need a sentence detector to interpret.
pub fn from_bytes(data: [u8; 8]) -> SentenceLayoutCase {
    let [kind, container, span, number, choice, width, count, breaks] = data;
    let (open, close) = SPANS[usize::from(span) % SPANS.len()];
    let number = u32::from(number) + 1;
    let source_gap = if breaks & 1 == 0 { " " } else { "\n" };
    let mut settings = ReflowSettings {
        mode: Mode::SentencePerLine,
        line_length: if width & 1 == 0 { 0 } else { 80 },
        // Quote and abbreviation expectations specify strict detection. The
        // remaining flags vary independently without changing the grammar.
        require_sentence_capital: true,
        ..ReflowSettings::from_bytes(span, choice)
    };
    let mut definitions = "";
    let (input, expected) = match kind % 7 {
        0 => {
            let step = format!("{open}{number}. Verify the user name first.{close}");
            (
                format!("{step}{source_gap}Then sign in.\n"),
                format!("{step}\nThen sign in.\n"),
            )
        }
        1 => {
            let token = ["release-please", "npm", "überprüfen", "iOS"][usize::from(choice) % 4];
            let first = format!("{open}The process completed phase {number}.{close}");
            let text = format!("{first}\n{token} starts the next operation.\n");
            (text.clone(), text)
        }
        2 => {
            let link = match choice % 4 {
                0 => {
                    definitions = "\n[#752]: https://example.com/reference\n";
                    "[#752]"
                }
                1 => "[lowercase](https://example.com/reference)",
                2 => "[lowercase][ref]",
                _ => "![lowercase](image.png)",
            };
            if choice % 4 == 2 {
                definitions = "\n[ref]: https://example.com/reference\n";
            }
            let text = format!("First sentence.\n{link} identifies the next operation.\n");
            (text.clone(), text)
        }
        3 => {
            let title = ["Dr.", "Prof.", "Mr.", "Ms."][usize::from(choice) % 4];
            (
                format!("Ask {title}{source_gap}Smith for help. Then finish.\n"),
                format!("Ask {title} Smith for help.\nThen finish.\n"),
            )
        }
        4 => (
            format!("Read a {open}\"quoted sentence.\"{close}{source_gap}result carefully. Then finish.\n"),
            format!("Read a {open}\"quoted sentence.\"{close} result carefully.\nThen finish.\n"),
        ),
        5 => (
            format!("Use pencils, pens, etc.{source_gap}and paper. Then finish.\n"),
            "Use pencils, pens, etc. and paper.\nThen finish.\n".to_string(),
        ),
        _ => {
            // Packing expectations come from known complete sentences and a
            // separate greedy packer. Production splitting is never used.
            let sentences: Vec<String> = (0..usize::from(count % 8 + 2))
                .map(|i| format!("Sentence {i} has {}tokens.", "many ".repeat(usize::from(choice % 5))))
                .collect();
            let width = [0, 10, 22, 40, 80][usize::from(width) % 5];
            settings.mode = Mode::SentencePack;
            settings.line_length = width;
            let input = format!("{}\n", sentences.join(source_gap));
            let expected = pack_known_sentences(&sentences, width as usize);
            return SentenceLayoutCase {
                input,
                expected,
                settings,
            };
        }
    };
    SentenceLayoutCase {
        input: format!("{}{definitions}", in_container(&input, container)),
        expected: format!("{}{definitions}", in_container(&expected, container)),
        settings,
    }
}

fn in_container(body: &str, container: u8) -> String {
    let (first, rest) = match container % 6 {
        0 => ("", ""),
        1 => ("> ", "> "),
        2 => ("> > ", "> > "),
        3 => ("- ", "  "),
        4 => ("1. ", "   "),
        _ => ("[^note]: ", "    "),
    };
    body.lines()
        .enumerate()
        .map(|(i, line)| format!("{}{line}\n", if i == 0 { first } else { rest }))
        .collect()
}

fn pack_known_sentences(sentences: &[String], width: usize) -> String {
    let mut lines = Vec::<String>::new();
    for sentence in sentences {
        if let Some(last) = lines.last_mut()
            && (width == 0 || last.len() + 1 + sentence.len() <= width)
        {
            last.push(' ');
            last.push_str(sentence);
        } else {
            lines.push(sentence.clone());
        }
    }
    format!("{}\n", lines.join("\n"))
}

impl SentenceLayoutCase {
    pub fn verify_output(&self, actual: &str) -> Result<(), String> {
        if actual == self.expected {
            Ok(())
        } else {
            Err(format!(
                "incorrect sentence layout\nreproduce: rumdl fmt {}\ninput: {:?}\nexpected: {:?}\nactual: {:?}",
                self.settings.shell_args(),
                self.input,
                self.expected,
                actual
            ))
        }
    }

    pub fn check(&self) -> Result<(), String> {
        self.verify_output(&reflow(&self.input, &self.settings)?)?;
        check(&self.input, &self.settings).map_err(|violation| format!("{}: {violation:?}", violation.label()))?;
        Ok(())
    }
}
