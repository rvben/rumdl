use rumdl::config::Config;
use rumdl::lint_context::LintContext;
use rumdl::rule::Rule;
use rumdl::rules::all_rules;
use std::env;
use std::time::Instant;

fn benchmark_rule(rule: &dyn Rule, test_cases: &[(&str, &str)], iterations: u32) -> Vec<(String, u64, u64)> {
    let mut results = Vec::new();

    for (name, content) in test_cases {
        // Warm up
        for _ in 0..10 {
            let ctx = LintContext::new(content);
            let _ = rule.check(&ctx);
            let _ = rule.fix(&ctx);
        }

        // Benchmark check
        let start = Instant::now();
        for _ in 0..iterations {
            let ctx = LintContext::new(content);
            let _ = rule.check(&ctx);
        }
        let check_time = start.elapsed().as_micros() as u64 / iterations as u64;

        // Benchmark fix
        let start = Instant::now();
        for _ in 0..iterations {
            let ctx = LintContext::new(content);
            let _ = rule.fix(&ctx);
        }
        let fix_time = start.elapsed().as_micros() as u64 / iterations as u64;

        results.push((name.to_string(), check_time, fix_time));
    }

    results
}

fn generate_test_content(rule_name: &str) -> Vec<(&'static str, String)> {
    match rule_name {
        "MD009" => vec![
            ("No trailing spaces", "Line without trailing spaces\n".repeat(100)),
            ("With trailing spaces", "Line with trailing spaces   \n".repeat(100)),
            ("Mixed content", "Normal line\nLine with spaces  \nAnother normal\n".repeat(50)),
            ("Empty lines", "\n\n\n".repeat(100)),
        ],
        "MD013" => vec![
            ("Short lines", "Short line\n".repeat(1000)),
            ("Long lines", "This is a very long line that exceeds the default line length limit of 80 characters and should trigger MD013\n".repeat(500)),
            ("Mixed lengths", "Short\nThis is a very long line that exceeds the limit\nShort again\n".repeat(300)),
            ("Code blocks", "```\nThis is a very long line in a code block that should be ignored by MD013\n```\n".repeat(100)),
        ],
        "MD047" => vec![
            ("Correct ending", "Content with proper ending\n".to_string()),
            ("Missing newline", "Content without newline at end".to_string()),
            ("Multiple newlines", "Content with multiple newlines\n\n\n".to_string()),
            ("Empty file", "".to_string()),
            ("Large file correct", "Line\n".repeat(1000)),
            ("Large file incorrect", format!("{}\n\n", "Line\n".repeat(1000))),
        ],
        "MD038" => vec![
            ("No code spans", "Regular text without code\n".repeat(100)),
            ("Correct code spans", "Text with `correct code` spans\n".repeat(100)),
            ("Spaces in code", "Text with ` spaced code ` spans\n".repeat(100)),
            ("Multiple backticks", "Text with ``code with ` backtick`` spans\n".repeat(100)),
            ("Mixed content", "Normal text\n`good code`\n` bad code `\n".repeat(100)),
        ],
        "MD044" => vec![
            ("No proper names", "regular text without any proper names\n".repeat(100)),
            ("Correct names", "Using JavaScript and Python correctly\n".repeat(100)),
            ("Incorrect names", "Using javascript and python incorrectly\n".repeat(100)),
            ("Mixed case", "JavaScript is good, but javascript is bad\n".repeat(100)),
            ("Many names", "JavaScript Python TypeScript Ruby Go Rust Java\n".repeat(100)),
        ],
        "MD034" => vec![
            ("No URLs", "Simple text without any URLs or links\n".repeat(100)),
            ("Markdown links", "[link](http://example.com) and [another](https://test.com)\n".repeat(100)),
            ("Bare URLs", "Check out http://example.com and https://test.com for more\n".repeat(100)),
            ("Mixed URLs", "Visit [site](http://a.com) or just go to http://b.com directly\n".repeat(100)),
            ("Complex URLs", "IPv6: http://[2001:db8::1]/path and port: http://example.com:8080/path?query=1\n".repeat(50)),
            ("Many URLs", {
                let mut content = String::new();
                for i in 0..200 {
                    content.push_str(&format!("URL {i}: http://example{i}.com/path/{i}/file?id={i}\n"));
                }
                content
            }),
        ],
        "MD053" => vec![
            ("No references", "Simple text without any links or references\n".repeat(100)),
            ("Used references", "[link1][ref1] and [link2][ref2]\n\n[ref1]: http://example1.com\n[ref2]: http://example2.com\n".repeat(50)),
            ("Unused references", "Some text here\n\n[unused1]: http://unused1.com\n[unused2]: http://unused2.com\n".repeat(50)),
            ("Mixed references", "[used][ref1]\n\n[ref1]: http://used.com\n[unused]: http://unused.com\n".repeat(100)),
            ("Many references", {
                let mut content = String::new();
                for i in 0..100 {
                    content.push_str(&format!("[link{i}][ref{i}]\n"));
                }
                content.push('\n');
                for i in 0..200 {
                    content.push_str(&format!("[ref{i}]: http://example{i}.com\n"));
                }
                content
            }),
        ],
        _ => vec![
            ("Default test", "Default test content\n".repeat(100)),
        ],
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <rule_name>", args[0]);
        std::process::exit(1);
    }

    let rule_name = &args[1];
    let config = Config::default();
    let rules = all_rules(&config);

    let rule = rules.into_iter().find(|r| r.name() == rule_name).unwrap_or_else(|| {
        eprintln!("Rule {rule_name} not found");
        std::process::exit(1);
    });

    let test_cases_vec = generate_test_content(rule_name);
    let test_cases: Vec<(&str, &str)> = test_cases_vec
        .iter()
        .map(|(name, content)| (*name, content.as_str()))
        .collect();

    println!("Benchmarking {rule_name} Rule");
    println!("{}", "=".repeat(50));
    println!();

    let results = benchmark_rule(rule.as_ref(), &test_cases, 100);

    let mut total_check = 0u64;
    let mut total_fix = 0u64;

    for (test_name, check_time, fix_time) in &results {
        println!("{test_name:<30} Check: {check_time:>6} μs  Fix: {fix_time:>6} μs");
        total_check += check_time;
        total_fix += fix_time;
    }

    println!();
    println!("Average times:");
    println!("  Check: {} μs", total_check / results.len() as u64);
    println!("  Fix:   {} μs", total_fix / results.len() as u64);
}
