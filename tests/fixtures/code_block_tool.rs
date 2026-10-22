// Native fixture used to exercise external processes on every CI platform.
use std::{
    fs,
    io::{self, Read},
};
// Real child-process concurrency accounting, shared by every fixture copy in a project.
fn update_processes(root: &std::path::Path, entering: bool) -> usize {
    // Keep the lock file in place: deleting a directory lock on Windows can
    // leave it pending deletion and make the next acquisition fail with access denied.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("process-lock"))
        .expect("open fixture lock");
    lock.lock().expect("acquire fixture lock");
    let path = root.join("process-state");
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => "0 0 0".into(),
        Err(e) => panic!("read fixture process state: {e}"),
    };
    let mut state: Vec<usize> = contents.split_whitespace().map(|n| n.parse().unwrap()).collect();
    if entering {
        state[0] += 1;
        state[1] = state[1].max(state[0]);
        state[2] += 1;
        if state[0] >= 2 {
            fs::write(root.join("overlap-observed"), "yes").unwrap();
        }
    } else {
        state[0] -= 1;
    }
    fs::write(path, format!("{} {} {}", state[0], state[1], state[2])).unwrap();
    state[1]
}

fn concurrent_tool(root: &std::path::Path, mode: &str, input: &str) {
    use std::io::Write;
    use std::{
        thread,
        time::{Duration, Instant},
    };
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("process-inputs"))
            .unwrap(),
        "{}",
        input.lines().next().unwrap_or("")
    )
    .unwrap();
    update_processes(root, true);
    let deadline = Instant::now() + Duration::from_secs(10);
    while mode.starts_with("parallel-") && !root.join("overlap-observed").exists() {
        if Instant::now() >= deadline {
            update_processes(root, false);
            eprintln!("fixture did not overlap another child");
            std::process::exit(2);
        }
        thread::sleep(Duration::from_millis(2));
    }
    // Deliberately finish the first block after the second.
    thread::sleep(Duration::from_millis(if input.starts_with("first") { 100 } else { 30 }));
    if mode.ends_with("finding") {
        println!("1:1: finding-{}", input.trim());
    } else if mode.ends_with("expand") {
        print!("{}", input.replace("bad", "good\nextra"));
    } else if mode.ends_with("pipes") {
        eprint!("{}", " ".repeat(128 * 1024));
        print!("{input}");
    } else if mode.ends_with("format") {
        print!("{}", input.replace("bad", "good and longer"));
    } else {
        print!("{input}");
    }
    update_processes(root, false);
}

fn main() {
    let executable = std::env::current_exe().unwrap();
    if std::env::args().nth(1).as_deref() == Some("--version") && executable.with_extension("version").exists() {
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(executable.with_extension("version-calls"))
                .unwrap(),
            "probe"
        )
        .unwrap();
        if let Ok(delay) = fs::read_to_string(executable.with_extension("version-delay-ms")) {
            std::thread::sleep(std::time::Duration::from_millis(delay.trim().parse().unwrap()));
        }
        if let Ok(stderr) = fs::read(executable.with_extension("version-stderr")) {
            std::io::stderr().write_all(&stderr).unwrap();
        }
        if executable.with_extension("version-invalid-utf8").exists() {
            std::io::stdout().write_all(&[0xff]).unwrap();
            return;
        }
        print!("{}", fs::read_to_string(executable.with_extension("version")).unwrap());
        if let Ok(code) = fs::read_to_string(executable.with_extension("version-exit")) {
            std::process::exit(code.trim().parse().unwrap());
        }
        return;
    }
    // The parent can hold this lock to prevent observable entry regardless of
    // how either process is scheduled. A broken timeout fails within 10 seconds.
    if executable.with_extension("entry-lock").exists() {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(executable.with_extension("entry-lock"))
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        eprintln!("fixture entry lock was not released");
                        std::process::exit(2);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Err(error) => panic!("acquire fixture entry lock: {error}"),
            }
        }
    }
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
        fs::write(
            executable.with_extension("arguments"),
            std::env::args().skip(1).collect::<Vec<_>>().join("\n"),
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
    if mode.starts_with("parallel-") || mode.starts_with("serial-") {
        concurrent_tool(root, mode.trim(), &input);
        return;
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
        "fail-message" => {
            eprintln!("fixture format failure");
            std::process::exit(2);
        }
        "slow" => std::thread::sleep(std::time::Duration::from_secs(2)),
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
