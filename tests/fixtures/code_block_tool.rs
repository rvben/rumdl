// Native fixture used to exercise external processes on every CI platform.
use std::{
    fs,
    io::{self, Read},
};
fn main() {
    let executable = std::env::current_exe().unwrap();
    // Record entry before arguments or stdin can hide an unintended invocation.
    if executable.with_extension("record").exists() {
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(executable.with_extension("calls"))
                .unwrap(),
            "ran"
        )
        .unwrap();
    }
    let root = executable.parent().unwrap().parent().unwrap().parent().unwrap();
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| fs::read_to_string(executable.with_extension("mode")).unwrap());
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    if mode.starts_with("count") {
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("calls"))
                .unwrap(),
            "ran"
        )
        .unwrap();
    }
    match mode.trim() {
        // Make an accidental Bun invocation capable of corrupting the block.
        "fmt" => println!("incorrect output"),
        "echo" | "count-echo" => print!("{input}"),
        "finding" | "count-finding" => println!("1:1: project-b-finding"),
        "lost" => println!("lost"),
        "format" | "count-format" => print!("{}", input.replace("bad", "good")),
        "uppercase" => print!("{}", input.to_uppercase()),
        "fail" => std::process::exit(2),
        "edit-input" => {
            fs::write(root.join("b.md"), "external edit\n").unwrap();
            print!("{input}");
        }
        "invalid-utf8" => {
            use std::io::Write;
            io::stdout().write_all(&[0xff]).unwrap();
        }
        _ => (),
    }
}
