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
        .map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to execute rumdl: {}",
                e
            ))
        })?;

    Ok(status.code().unwrap_or(1))
}
