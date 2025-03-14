# rumdl Python Package

This directory contains the Python package for `rumdl`, which is a wrapper around the Rust binary.

## Build Process

The Python package is built using Maturin, which provides a seamless integration between Rust and Python. The package configuration is now in the root `pyproject.toml` file.

### Building and Installing

To build and install the package in development mode:

```bash
# From the project root
make dev-install
```

Or using Maturin directly:

```bash
maturin develop --release
```

### Building for Distribution

To build a wheel for distribution:

```bash
# From the project root
make build-wheel
```

Or using Maturin directly:

```bash
maturin build --release --strip --interpreter python3
```

### Testing the Package

After installation, you can test the package with:

```bash
python -m rumdl --version
```

## Package Structure

- `rumdl/__init__.py`: Provides version information and package imports
- `rumdl/__main__.py`: Command-line interface that finds and executes the Rust binary
- `rumdl/py.typed`: Marker file for type annotations

## Maintenance Notes

- The version is now managed in both Cargo.toml and Python's __init__.py
- When releasing a new version, update both files to match
- Maturin handles the packaging and distribution, including finding the Rust binary 