"""
Command-line interface for rumdl.
"""

from __future__ import annotations

import os
import sys
import subprocess
from pathlib import Path

def find_native_binary() -> str:
    """Find the native Rust binary, not the Python entry point script."""
    # In development mode, use the target directory binary
    project_root = Path(__file__).resolve().parent.parent.parent
    target_binary = project_root / "target" / "release" / "rumdl"
    if target_binary.exists() and not target_binary.is_dir():
        return str(target_binary)
    
    # For Windows, check for .exe extension
    if sys.platform == "win32":
        target_binary = project_root / "target" / "release" / "rumdl.exe"
        if target_binary.exists() and not target_binary.is_dir():
            return str(target_binary)
    
    # If we can't find the binary, raise an error
    raise FileNotFoundError(
        "Could not find the native rumdl binary. "
        "Please ensure it was built with 'cargo build --release'."
    )

def main() -> int:
    """Run the rumdl command line tool."""
    try:
        # Find the native binary
        native_binary = find_native_binary()
        
        # Simply forward all arguments to the Rust binary
        args = [native_binary] + sys.argv[1:]
        
        # Run the binary
        if sys.platform == "win32":
            completed_process = subprocess.run(args)
            return completed_process.returncode
        else:
            # On Unix-like systems, directly execute the binary for better signal handling
            os.execv(native_binary, args)
            return 0  # This line will never be reached on non-Windows platforms
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1

if __name__ == "__main__":
    sys.exit(main()) 