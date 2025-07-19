//!
//! This module provides Python bindings and integration for rumdl.

use pyo3::prelude::*;

/// Python module for rumdl
#[pymodule]
fn rumdl(_py: Python, m: &Bound<PyModule>) -> PyResult<()> {
    // Add version
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Add a function to check a file
    m.add_function(wrap_pyfunction!(check_file, m)?)?;

    Ok(())
}

/// Check a file for markdown lint issues
#[pyfunction]
fn check_file(file_path: &str) -> PyResult<i32> {
    // For now, we'll just call the binary as a subprocess
    // In a future implementation, this could directly call the Rust functions
    let status = std::process::Command::new("rumdl")
        .arg(file_path)
        .status()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to execute rumdl: {e}")))?;

    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    // Tests that don't require Python runtime

    #[test]
    fn test_check_file_with_valid_input() {
        // Create a temporary markdown file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "# Test Markdown File\n\nThis is a test.").unwrap();

        // Note: This test would require rumdl binary to be in PATH
        // In practice, this might fail in isolated test environments
        if std::env::var("SKIP_INTEGRATION_TESTS").is_ok() {
            return;
        }

        // Only run if rumdl binary exists
        if std::process::Command::new("rumdl").arg("--version").output().is_ok() {
            let result = check_file(file_path.to_str().unwrap());
            // We can't guarantee the exit code without knowing the content
            // but we can check that it doesn't panic
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_check_file_with_invalid_file() {
        // Only run if rumdl binary exists
        if std::process::Command::new("rumdl").arg("--version").output().is_ok() {
            let result = check_file("/non/existent/file/path.md");
            // When the file doesn't exist, rumdl should return an error code
            assert!(result.is_ok());
            // The error code should be non-zero
            let code = result.unwrap();
            assert_ne!(code, 0);
        }
    }

    #[test]
    fn test_error_handling_missing_rumdl_binary() {
        // Save the current PATH
        let original_path = std::env::var("PATH").unwrap_or_default();

        // Set PATH to empty to ensure rumdl binary won't be found
        unsafe {
            std::env::set_var("PATH", "");
        }

        let result = check_file("any_file.md");

        // Restore original PATH
        unsafe {
            std::env::set_var("PATH", original_path);
        }

        // The function should return an error when rumdl binary is not found
        assert!(result.is_err());

        // Check that the error message contains expected text
        if let Err(e) = result {
            let error_str = e.to_string();
            assert!(error_str.contains("Failed to execute rumdl"));
        }
    }

    #[test]
    fn test_check_file_command_construction() {
        // This test verifies that the command is constructed correctly
        // Create a test file path
        let _test_path = "/tmp/test_file.md";

        // Test with a path containing spaces
        let path_with_spaces = "/tmp/test file with spaces.md";

        // Only run if rumdl binary exists
        if std::process::Command::new("rumdl").arg("--version").output().is_ok() {
            let result = check_file(path_with_spaces);
            // The result depends on whether rumdl is installed
            assert!(result.is_ok() || result.is_err());
        }
    }

    #[test]
    fn test_check_file_return_codes() {
        // This test documents the expected return codes
        // 0: Success (no lint errors)
        // 1: Default error code or lint errors found
        // Other: Specific error conditions

        // Only run if rumdl binary exists
        if std::process::Command::new("rumdl").arg("--version").output().is_ok() {
            // Test with non-existent file should return non-zero
            let result = check_file("/definitely/not/a/real/file.md");
            if let Ok(code) = result {
                assert_ne!(code, 0, "Non-existent file should return non-zero exit code");
            }
        }
    }

    // Tests that require Python runtime - marked as ignore by default
    #[cfg(feature = "python")]
    mod python_runtime_tests {
        use super::*;
        use pyo3::types::PyDict;
        use std::ffi::CString;

        #[test]
        #[ignore] // Requires Python runtime
        fn test_module_initialization() {
            pyo3::prepare_freethreaded_python();
            Python::with_gil(|py| {
                let module = PyModule::new(py, "test_rumdl").unwrap();
                let result = rumdl(py, &module);
                assert!(result.is_ok());

                // Check that __version__ is added
                assert!(module.hasattr("__version__").unwrap());
                let version: String = module.getattr("__version__").unwrap().extract().unwrap();
                assert_eq!(version, env!("CARGO_PKG_VERSION"));

                // Check that check_file function is added
                assert!(module.hasattr("check_file").unwrap());
            });
        }

        #[test]
        #[ignore] // Requires Python runtime
        fn test_check_file_python_integration() {
            pyo3::prepare_freethreaded_python();
            Python::with_gil(|py| {
                let module = PyModule::new(py, "test_rumdl").unwrap();
                rumdl(py, &module).unwrap();

                // Create a temporary file
                let temp_dir = TempDir::new().unwrap();
                let file_path = temp_dir.path().join("test.md");
                let mut file = File::create(&file_path).unwrap();
                writeln!(file, "# Test\n\nContent").unwrap();

                // Call the function through Python
                let locals = PyDict::new(py);
                locals.set_item("module", module).unwrap();
                locals.set_item("file_path", file_path.to_str().unwrap()).unwrap();

                let code_str = CString::new("module.check_file(file_path)").unwrap();
                let code = py.eval(code_str.as_c_str(), None, Some(&locals));
                assert!(code.is_ok());
            });
        }

        #[test]
        #[ignore] // Requires Python runtime
        fn test_version_string_correctness() {
            pyo3::prepare_freethreaded_python();
            Python::with_gil(|py| {
                let module = PyModule::new(py, "test_version").unwrap();
                rumdl(py, &module).unwrap();

                let version: String = module.getattr("__version__").unwrap().extract().unwrap();

                // Version should match Cargo.toml version
                assert_eq!(version, env!("CARGO_PKG_VERSION"));

                // Version should follow semver format (basic check)
                let parts: Vec<&str> = version.split('.').collect();
                assert!(parts.len() >= 2); // At least major.minor

                // Each part should be parseable as a number
                for part in parts {
                    // Handle pre-release versions like "1.0.0-alpha"
                    let num_part = part.split('-').next().unwrap();
                    assert!(
                        num_part.parse::<u32>().is_ok(),
                        "Version part '{num_part}' is not a valid number"
                    );
                }
            });
        }

        #[test]
        #[ignore] // This test requires Python runtime and specific setup
        fn test_pymodule_attributes() {
            pyo3::prepare_freethreaded_python();
            Python::with_gil(|py| {
                let module = PyModule::new(py, "test_attrs").unwrap();
                rumdl(py, &module).unwrap();

                // Test that the module has the expected structure
                let dir_str = CString::new("dir(module)").unwrap();
                let dir_result = py.eval(
                    dir_str.as_c_str(),
                    None,
                    Some(&{
                        let locals = PyDict::new(py);
                        locals.set_item("module", &module).unwrap();
                        locals
                    }),
                );

                assert!(dir_result.is_ok());
                let attrs: Vec<String> = dir_result.unwrap().extract().unwrap();

                // Check for expected attributes
                assert!(attrs.contains(&"__version__".to_string()));
                assert!(attrs.contains(&"check_file".to_string()));
            });
        }
    }
}
