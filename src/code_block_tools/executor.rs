//! Tool execution engine for running external formatters and linters.
//!
//! This module handles the actual execution of external tools via stdin/stdout,
//! with timeout support and lazy tool availability checking.

use super::config::ToolDefinition;
use super::lookup;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Timeouts of one tool that end further attempts at it.
///
/// A tool that hangs does so for every block it is handed, and each attempt costs the
/// whole timeout. Three is enough to tell a hanging tool from one that is merely slow on
/// an occasional large block.
const TIMEOUT_LIMIT: u32 = 3;

/// Timeout tallies keyed by tool name, shared by every executor in the process.
///
/// The tally has to outlive one executor: a fresh executor is built per file, so
/// per-instance state would forget what the previous file just learned and every file
/// would pay the timeout over again. A tool that exits on its own clears its own tally,
/// so a single slow block never disables anything.
static TIMEOUT_COUNTS: LazyLock<Arc<Mutex<HashMap<String, u32>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Result of executing a tool.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Standard output from the tool.
    pub stdout: String,
    /// Standard error from the tool.
    pub stderr: String,
    /// Exit code (0 typically means success).
    pub exit_code: i32,
    /// Whether the tool executed successfully (exit code 0).
    pub success: bool,
}

/// Error during tool execution.
#[derive(Debug, Clone)]
pub enum ExecutorError {
    /// Tool binary not found in PATH.
    ToolNotFound { tool: String },
    /// Tool execution failed.
    ExecutionFailed { tool: String, message: String },
    /// Tool execution timed out.
    Timeout { tool: String, timeout_ms: u64 },
    /// Tool skipped without being run, having already timed out repeatedly.
    RepeatedTimeouts {
        tool: String,
        timeout_ms: u64,
        timeouts: u32,
    },
    /// I/O error during execution.
    IoError { message: String },
}

impl std::fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotFound { tool } => {
                write!(f, "Tool '{tool}' not found in PATH")
            }
            Self::ExecutionFailed { tool, message } => {
                write!(f, "Tool '{tool}' failed: {message}")
            }
            Self::Timeout { tool, timeout_ms } => {
                write!(f, "Tool '{tool}' timed out after {timeout_ms}ms")
            }
            Self::RepeatedTimeouts {
                tool,
                timeout_ms,
                timeouts,
            } => {
                write!(
                    f,
                    "Tool '{tool}' skipped after timing out {timeouts} times at {timeout_ms}ms; a tool that never exits is usually not reading its stdin"
                )
            }
            Self::IoError { message } => {
                write!(f, "I/O error: {message}")
            }
        }
    }
}

impl std::error::Error for ExecutorError {}

/// Executor for running external tools.
///
/// Caches tool availability checks for efficiency.
pub struct ToolExecutor {
    /// Cache of tool availability checks (tool name -> available).
    tool_cache: Arc<Mutex<HashMap<String, bool>>>,
    /// Timeouts recorded per tool since it last exited on its own.
    timeout_counts: Arc<Mutex<HashMap<String, u32>>>,
    /// Default timeout in milliseconds.
    default_timeout_ms: u64,
}

impl ToolExecutor {
    /// Create a new executor with the given default timeout.
    ///
    /// Timeouts are tallied process-wide, so a tool that hangs is attempted a bounded
    /// number of times across every file of a run rather than once per file.
    pub fn new(default_timeout_ms: u64) -> Self {
        Self {
            tool_cache: Arc::new(Mutex::new(HashMap::new())),
            timeout_counts: Arc::clone(&TIMEOUT_COUNTS),
            default_timeout_ms,
        }
    }

    /// Create an executor that tallies timeouts only for itself.
    ///
    /// For callers that must not inherit or contribute to the process-wide tally, such
    /// as tests, where one test's hanging tool would otherwise decide whether another
    /// test's tool is run at all.
    pub fn isolated(default_timeout_ms: u64) -> Self {
        Self {
            tool_cache: Arc::new(Mutex::new(HashMap::new())),
            timeout_counts: Arc::new(Mutex::new(HashMap::new())),
            default_timeout_ms,
        }
    }

    /// Timeouts recorded for a tool since it last exited on its own.
    fn timeout_count(&self, tool_name: &str) -> u32 {
        self.timeout_counts.lock().unwrap().get(tool_name).copied().unwrap_or(0)
    }

    /// Record that a tool had to be killed at its timeout.
    fn record_timeout(&self, tool_name: &str) {
        *self
            .timeout_counts
            .lock()
            .unwrap()
            .entry(tool_name.to_string())
            .or_insert(0) += 1;
    }

    /// Forget a tool's timeouts, after it exited without needing to be killed.
    fn clear_timeouts(&self, tool_name: &str) {
        self.timeout_counts.lock().unwrap().remove(tool_name);
    }

    /// Check if a tool is available (lazy, cached).
    pub fn is_tool_available(&self, tool_name: &str) -> bool {
        // Check cache first
        {
            let cache = self.tool_cache.lock().unwrap();
            if let Some(&available) = cache.get(tool_name) {
                return available;
            }
        }

        // Resolved in-process the way the spawn itself would resolve it, so the
        // answer does not depend on a `which`/`where` binary being installed.
        let available = self.check_tool_exists(tool_name);

        // Cache the result
        {
            let mut cache = self.tool_cache.lock().unwrap();
            cache.insert(tool_name.to_string(), available);
        }

        available
    }

    /// Check if a tool binary exists where `Command::new` would look for it.
    fn check_tool_exists(&self, tool_name: &str) -> bool {
        lookup::resolve_program(OsStr::new(tool_name), std::env::var_os("PATH").as_deref()).is_some()
    }

    /// Execute a tool with the given input.
    ///
    /// # Arguments
    /// * `tool_def` - Tool definition with command and arguments
    /// * `input` - Content to pass via stdin
    /// * `is_format_mode` - Whether to use format_args (true) or lint_args (false)
    /// * `timeout_ms` - Optional timeout override
    ///
    /// # Returns
    /// Tool output on success, or an error.
    pub fn execute(
        &self,
        tool_def: &ToolDefinition,
        input: &str,
        is_format_mode: bool,
        timeout_ms: Option<u64>,
    ) -> Result<ToolOutput, ExecutorError> {
        if tool_def.command.is_empty() {
            return Err(ExecutorError::ExecutionFailed {
                tool: "unknown".to_string(),
                message: "Empty command".to_string(),
            });
        }

        let tool_name = &tool_def.command[0];

        // Check tool availability (lazy, cached)
        if !self.is_tool_available(tool_name) {
            return Err(ExecutorError::ToolNotFound {
                tool: tool_name.clone(),
            });
        }

        // A tool that has hung this many times will hang again, and every further
        // attempt costs the full timeout. Report it per block, but stop paying for it.
        // Files are processed in parallel, so attempts already in flight when the limit
        // is reached still run: the ceiling is the limit plus the worker count, which is
        // a constant, rather than one timeout per code block in the run.
        let effective_timeout_ms = timeout_ms.unwrap_or(self.default_timeout_ms);
        let timeouts = self.timeout_count(tool_name);
        if timeouts >= TIMEOUT_LIMIT {
            return Err(ExecutorError::RepeatedTimeouts {
                tool: tool_name.clone(),
                timeout_ms: effective_timeout_ms,
                timeouts,
            });
        }

        // Build command
        let mut cmd = Command::new(tool_name);

        // Add base arguments
        if tool_def.command.len() > 1 {
            cmd.args(&tool_def.command[1..]);
        }

        // Add mode-specific arguments
        let extra_args = if is_format_mode {
            &tool_def.format_args
        } else {
            &tool_def.lint_args
        };
        if !extra_args.is_empty() {
            cmd.args(extra_args);
        }

        // Configure stdin/stdout
        if tool_def.stdin {
            cmd.stdin(Stdio::piped());
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn process
        let mut child = cmd.spawn().map_err(|e| ExecutorError::IoError {
            message: format!("Failed to spawn '{tool_name}': {e}"),
        })?;

        let mut stdout_handle = child
            .stdout
            .take()
            .map(|stdout| thread::spawn(move || read_pipe_to_string(stdout)));
        let mut stderr_handle = child
            .stderr
            .take()
            .map(|stderr| thread::spawn(move || read_pipe_to_string(stderr)));

        // Write stdin if required.
        // BrokenPipe is ignored: the tool may exit before consuming all input
        // (e.g., `true` or a linter that validates without reading fully).
        if tool_def.stdin
            && let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(input.as_bytes())
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(ExecutorError::IoError {
                message: format!("Failed to write to stdin: {e}"),
            });
        }

        // Wait for completion with timeout
        let timeout = Duration::from_millis(effective_timeout_ms);
        let status = if timeout.is_zero() {
            child.wait().map_err(|e| ExecutorError::IoError {
                message: format!("Failed to wait for '{tool_name}': {e}"),
            })?
        } else {
            let start = Instant::now();
            loop {
                if let Some(status) = child.try_wait().map_err(|e| ExecutorError::IoError {
                    message: format!("Failed to poll '{tool_name}': {e}"),
                })? {
                    break status;
                }
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    // The reader threads are deliberately abandoned rather than joined.
                    // `read_to_end` returns only once every write end of the pipe is
                    // closed, and a killed tool can leave a descendant holding one, so
                    // joining here waits on exactly the process the timeout exists to
                    // bound. Each thread ends by itself once the pipe finally closes.
                    drop(stdout_handle.take());
                    drop(stderr_handle.take());
                    self.record_timeout(tool_name);
                    return Err(ExecutorError::Timeout {
                        tool: tool_name.clone(),
                        timeout_ms: timeout.as_millis() as u64,
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
        };

        // The tool exited on its own, so whatever made earlier runs hang is over.
        self.clear_timeouts(tool_name);

        let stdout = join_reader(stdout_handle.take()).map_err(|e| ExecutorError::IoError { message: e })?;
        let stderr = join_reader(stderr_handle.take()).map_err(|e| ExecutorError::IoError { message: e })?;
        let exit_code = status.code().unwrap_or(-1);

        Ok(ToolOutput {
            stdout,
            stderr,
            exit_code,
            success: status.success(),
        })
    }

    /// Execute a tool for formatting (returns formatted content).
    pub fn format(
        &self,
        tool_def: &ToolDefinition,
        input: &str,
        timeout_ms: Option<u64>,
    ) -> Result<String, ExecutorError> {
        let output = self.execute(tool_def, input, true, timeout_ms)?;

        if output.success && tool_def.stdout {
            Ok(output.stdout)
        } else if !output.success {
            let exit_code = output.exit_code;
            let stderr = &output.stderr;
            Err(ExecutorError::ExecutionFailed {
                tool: tool_def.command.first().cloned().unwrap_or_default(),
                message: format!("Exit code {exit_code}: {stderr}"),
            })
        } else {
            // Tool doesn't output to stdout, which is unusual for a formatter
            Err(ExecutorError::ExecutionFailed {
                tool: tool_def.command.first().cloned().unwrap_or_default(),
                message: "Formatter doesn't output to stdout".to_string(),
            })
        }
    }

    /// Execute a tool for linting (returns diagnostics).
    pub fn lint(
        &self,
        tool_def: &ToolDefinition,
        input: &str,
        timeout_ms: Option<u64>,
    ) -> Result<ToolOutput, ExecutorError> {
        self.execute(tool_def, input, false, timeout_ms)
    }
}

fn read_pipe_to_string<R: Read>(mut pipe: R) -> std::io::Result<String> {
    let mut buf = Vec::new();
    pipe.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

fn join_reader(handle: Option<thread::JoinHandle<std::io::Result<String>>>) -> Result<String, String> {
    match handle {
        Some(handle) => match handle.join() {
            Ok(res) => res.map_err(|e| format!("Failed to read output: {e}")),
            Err(_) => Err("Output reader thread panicked".to_string()),
        },
        None => Ok(String::new()),
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new(30_000) // 30 seconds default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executor_creation() {
        let executor = ToolExecutor::new(10_000);
        // Just verify it creates successfully
        assert_eq!(executor.default_timeout_ms, 10_000);
    }

    #[test]
    fn test_tool_not_found() {
        let executor = ToolExecutor::default();
        let tool_def = ToolDefinition {
            command: vec!["nonexistent-tool-xyz123".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        let result = executor.execute(&tool_def, "test", false, None);
        assert!(matches!(result, Err(ExecutorError::ToolNotFound { .. })));
    }

    #[test]
    fn test_empty_command() {
        let executor = ToolExecutor::default();
        let tool_def = ToolDefinition {
            command: vec![],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        let result = executor.execute(&tool_def, "test", false, None);
        assert!(matches!(result, Err(ExecutorError::ExecutionFailed { .. })));
    }

    #[test]
    #[cfg(unix)]
    fn test_execute_cat() {
        let executor = ToolExecutor::isolated(30_000);
        let tool_def = ToolDefinition {
            command: vec!["cat".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        let result = executor.execute(&tool_def, "hello world", false, None);
        let output = result.expect("cat should succeed");
        assert!(output.success);
        assert_eq!(output.stdout.trim(), "hello world");
    }

    #[test]
    #[cfg(unix)]
    fn test_timeout() {
        let executor = ToolExecutor::isolated(5);
        let tool_def = ToolDefinition {
            command: vec!["sleep".to_string(), "1".to_string()],
            stdin: false,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        let result = executor.execute(&tool_def, "", false, Some(5));
        assert!(matches!(result, Err(ExecutorError::Timeout { .. })));
    }

    /// A tool definition whose process outlives its own timeout, and leaves a child
    /// holding the stdout pipe open after the tool itself is killed.
    #[cfg(unix)]
    fn descendant_holds_stdout_tool() -> ToolDefinition {
        ToolDefinition {
            command: vec![
                "sh".to_string(),
                "-c".to_string(),
                "sleep 30 & exec sleep 30".to_string(),
            ],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        }
    }

    /// The configured timeout has to bound the call even when the killed tool leaves a
    /// descendant holding the write end of the stdout pipe. Reading that pipe to EOF
    /// waits for the descendant, which is precisely the process the timeout is for.
    #[test]
    #[cfg(unix)]
    fn test_timeout_bounds_execution_when_a_descendant_holds_stdout() {
        let executor = ToolExecutor::isolated(200);
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let started = Instant::now();
            let result = executor.execute(&descendant_holds_stdout_tool(), "input", true, Some(200));
            let _ = tx.send((started.elapsed(), result));
        });

        // Generous next to the 200ms timeout, and far below the 30s the descendant
        // lives for, so this only fires if the call waited on the descendant.
        let (elapsed, result) = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("execute() did not return: the timeout bounded nothing");
        assert!(
            matches!(result, Err(ExecutorError::Timeout { .. })),
            "expected a timeout, got {result:?}"
        );
        assert!(elapsed < Duration::from_secs(10), "execute() took {elapsed:?}");
    }

    /// A hanging tool costs its whole timeout every time it is invoked, so a run over
    /// many code blocks must stop invoking it rather than pay that repeatedly.
    #[test]
    #[cfg(unix)]
    fn test_a_hanging_tool_is_skipped_after_repeated_timeouts() {
        let executor = ToolExecutor::isolated(50);
        let tool_def = ToolDefinition {
            command: vec!["sh".to_string(), "-c".to_string(), "exec sleep 30".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        for attempt in 1..=TIMEOUT_LIMIT {
            let result = executor.execute(&tool_def, "input", true, Some(50));
            assert!(
                matches!(result, Err(ExecutorError::Timeout { .. })),
                "attempt {attempt} should time out, got {result:?}"
            );
        }

        let started = Instant::now();
        let result = executor.execute(&tool_def, "input", true, Some(50));
        match result {
            Err(ExecutorError::RepeatedTimeouts {
                timeouts, timeout_ms, ..
            }) => {
                assert_eq!(timeouts, TIMEOUT_LIMIT);
                assert_eq!(timeout_ms, 50);
            }
            other => panic!("expected the tool to be skipped, got {other:?}"),
        }
        // Skipping means not spawning it, so this must not cost another timeout.
        assert!(
            started.elapsed() < Duration::from_millis(50),
            "skipping still took {:?}",
            started.elapsed()
        );
    }

    /// One slow block must not disable a tool for the rest of the run, so exiting on its
    /// own clears whatever a tool accumulated before.
    #[test]
    #[cfg(unix)]
    fn test_exiting_normally_clears_earlier_timeouts() {
        let executor = ToolExecutor::isolated(50);
        // Both definitions run through `sh`, which is what the tally is keyed on.
        let hangs = ToolDefinition {
            command: vec!["sh".to_string(), "-c".to_string(), "exec sleep 30".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };
        let exits = ToolDefinition {
            command: vec!["sh".to_string(), "-c".to_string(), "cat".to_string()],
            stdin: true,
            stdout: true,
            lint_args: vec![],
            format_args: vec![],
        };

        for _ in 0..TIMEOUT_LIMIT - 1 {
            assert!(matches!(
                executor.execute(&hangs, "input", true, Some(50)),
                Err(ExecutorError::Timeout { .. })
            ));
        }
        assert_eq!(executor.timeout_count("sh"), TIMEOUT_LIMIT - 1);

        let output = executor.execute(&exits, "hello", true, None).expect("cat should exit");
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(executor.timeout_count("sh"), 0, "a clean exit must clear the tally");
    }

    /// The tally outlives one executor, since a fresh executor is built per file and a
    /// hanging tool would otherwise be retried from scratch for every file in the run.
    #[test]
    fn test_the_shared_tally_carries_across_executors() {
        // A name no real tool answers to, so this neither reads nor disturbs the tally
        // of any tool another test in this process may be running.
        let key = "rumdl-test-only-shared-tally-probe";
        let first = ToolExecutor::new(50);
        let second = ToolExecutor::new(50);
        let alone = ToolExecutor::isolated(50);

        let before = second.timeout_count(key);
        first.record_timeout(key);

        assert_eq!(
            second.timeout_count(key),
            before + 1,
            "executors built for different files must share one tally"
        );
        assert_eq!(alone.timeout_count(key), 0, "an isolated executor keeps its own tally");

        first.clear_timeouts(key);
        assert_eq!(second.timeout_count(key), 0);
    }
}
