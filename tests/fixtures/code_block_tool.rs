// Native fixture used to exercise external processes on every CI platform.
use std::{
    fs,
    io::{self, Read},
};
fn main() {
    let executable = std::env::current_exe().unwrap();
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
        "echo" | "count-echo" => print!("{input}"),
        "finding" => println!("1:1: project-b-finding"),
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
