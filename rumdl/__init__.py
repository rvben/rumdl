"""
rumdl: An extremely fast Markdown linter written in Rust.
"""

try:
    from importlib.metadata import version
    __version__ = version("rumdl")
except ImportError:
    # Python < 3.8
    from importlib_metadata import version
    __version__ = version("rumdl")
