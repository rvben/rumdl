use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

pub(super) fn install(root: &Path, mode: &str) -> PathBuf {
    static TOOL: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    let (_, binary) = TOOL.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("tool.rs");
        fs::write(&source, include_str!("../fixtures/code_block_tool.rs")).unwrap();
        let binary = dir.path().join(format!("tool{}", std::env::consts::EXE_SUFFIX));
        let output = Command::new("rustc")
            .args(["--edition=2024", "--crate-name=rumdl_test_tool"])
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(output.status.success(), "native fixture compilation failed: {output:?}");
        (dir, binary)
    });
    let tool = root
        .join(if cfg!(windows) { ".venv/Scripts" } else { ".venv/bin" })
        .join(format!("rumdl-policy-test{}", std::env::consts::EXE_SUFFIX));
    fs::create_dir_all(tool.parent().unwrap()).unwrap();
    fs::copy(binary, &tool).unwrap();
    fs::write(tool.with_extension("mode"), mode).unwrap();
    tool
}
