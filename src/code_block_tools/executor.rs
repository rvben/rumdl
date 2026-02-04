//! Tool execution engine for running external formatters and linters.
//!
//! This module handles the actual execution of external tools via stdin/stdout,
//! with timeout support and lazy tool availability checking.

use super::config::ToolDefinition;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    /// Default timeout in milliseconds.
    default_timeout_ms: u64,
}

impl ToolExecutor {
    /// Create a new executor with the given default timeout.
    pub fn new(default_timeout_ms: u64) -> Self {
        Self {
            tool_cache: Arc::new(Mutex::new(HashMap::new())),
            default_timeout_ms,
        }
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

        // Check if tool exists using `which` on Unix or `where` on Windows
        let available = self.check_tool_exists(tool_name);

        // Cache the result
        {
            let mut cache = self.tool_cache.lock().unwrap();
            cache.insert(tool_name.to_string(), available);
        }

        available
    }

    /// Check if a tool binary exists.
    fn check_tool_exists(&self, tool_name: &str) -> bool {
        #[cfg(unix)]
        {
            Command::new("which")
                .arg(tool_name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }

        #[cfg(windows)]
        {
            Command::new("where")
                .arg(tool_name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        }
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

        // Write stdin if required
        if tool_def.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(input.as_bytes()).map_err(|e| ExecutorError::IoError {
                message: format!("Failed to write to stdin: {e}"),
            })?;
        }

        // Wait for completion with timeout
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(self.default_timeout_ms));
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
                    let _ = join_reader(stdout_handle.take());
                    let _ = join_reader(stderr_handle.take());
                    return Err(ExecutorError::Timeout {
                        tool: tool_name.clone(),
                        timeout_ms: timeout.as_millis() as u64,
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
        };

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

    // Integration tests with real tools would go here, but are skipped
    // in unit tests since they require the tools to be installed.

    #[test]
    #[ignore = "requires 'cat' to be available"]
    fn test_execute_cat() {
        let executor = ToolExecutor::default();
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
    #[ignore = "requires 'sleep' to be available"]
    fn test_timeout() {
        let executor = ToolExecutor::new(5);
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
}
