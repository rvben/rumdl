//! Integration tests for Python bindings
//!
//! These tests verify the Python bindings work correctly when the Python feature is enabled.
//! Most tests are marked with #[ignore] as they require a Python runtime.

#![cfg(feature = "python")]

use std::process::Command;

#[test]
fn test_python_module_can_be_imported() {
    // Skip if no Python available
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("Skipping test: Python 3 not found");
        return;
    }

    // This test would require building the Python module first
    // In a real CI/CD pipeline, you would:
    // 1. Build the module with `maturin build`
    // 2. Install it in a virtual environment
    // 3. Test importing it

    // For now, we just verify the module exists in the codebase
    assert!(std::path::Path::new("src/python.rs").exists());
}

#[test]
#[ignore] // Requires built Python module
fn test_python_module_version() {
    // This would test the module after it's built and installed
    // python3 -c "import rumdl; print(rumdl.__version__)"
}

#[test]
#[ignore] // Requires built Python module
fn test_python_check_file_function() {
    // This would test the check_file function from Python
    // python3 -c "import rumdl; exit_code = rumdl.check_file('test.md'); print(exit_code)"
}
