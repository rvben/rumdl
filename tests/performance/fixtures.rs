/// Performance test fixtures - downloads real-world markdown files from GitHub
use std::path::{Path, PathBuf};
use std::fs;

/// Test fixture definition
#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    pub commit: &'static str,
    pub path: &'static str,
}

impl Fixture {
    /// Get the raw GitHub URL for this fixture at the pinned commit
    pub fn raw_url(&self) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            self.url, self.commit, self.path
        )
    }

    /// Get the cache path for this fixture
    pub fn cache_path(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("perf-fixtures")
            .join(format!("{}-{}.md", self.name, &self.commit[..8]))
    }

    /// Download and cache this fixture, returning the file contents
    pub fn download(&self) -> Result<String, Box<dyn std::error::Error>> {
        let cache_path = self.cache_path();

        // If cached, return from cache
        if cache_path.exists() {
            return Ok(fs::read_to_string(cache_path)?);
        }

        // Create cache directory
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download from GitHub
        let content = download_with_retry(&self.raw_url(), 3)?;

        // Cache for future runs
        fs::write(&cache_path, &content)?;

        Ok(content)
    }
}

/// Download with retry logic
fn download_with_retry(url: &str, max_retries: usize) -> Result<String, Box<dyn std::error::Error>> {
    let mut last_error = None;

    for attempt in 0..max_retries {
        match ureq::get(url)
            .timeout(std::time::Duration::from_secs(30))
            .call()
        {
            Ok(response) => {
                return Ok(response.into_string()?);
            }
            Err(e) => {
                eprintln!("Download attempt {} failed: {}", attempt + 1, e);
                last_error = Some(e);
                if attempt < max_retries - 1 {
                    std::thread::sleep(std::time::Duration::from_secs(1 << attempt));
                }
            }
        }
    }

    Err(Box::new(last_error.unwrap()))
}

/// Real-world markdown fixtures for performance testing
pub const FIXTURES: &[Fixture] = &[
    // Rust Book - comprehensive technical documentation
    Fixture {
        name: "rust-book",
        description: "The Rust Programming Language book - chapter on ownership",
        url: "rust-lang/book",
        commit: "c06006157b14b3d47882530fcb94e0b3c304f07d", // Pin to specific commit
        path: "src/ch04-01-what-is-ownership.md",
    },

    // Large GitHub README with complex formatting
    Fixture {
        name: "awesome-rust",
        description: "Awesome Rust - massive README with many links and lists",
        url: "rust-unofficial/awesome-rust",
        commit: "main", // Can use branch or tag
        path: "README.md",
    },

    // RFC document - formal specification style
    Fixture {
        name: "rust-rfc",
        description: "Rust RFC - technical specification document",
        url: "rust-lang/rfcs",
        commit: "master",
        path: "text/0002-rfc-process.md",
    },

    // Documentation with code blocks
    Fixture {
        name: "mdbook-guide",
        description: "mdBook user guide - docs with many code examples",
        url: "rust-lang/mdBook",
        commit: "master",
        path: "guide/src/README.md",
    },

    // Blog post style - narrative writing
    Fixture {
        name: "blog-post",
        description: "Rust blog post - narrative with code examples",
        url: "rust-lang/blog.rust-lang.org",
        commit: "master",
        path: "posts/2024-01-09-Rust-1.75.0.md",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Run with --ignored to test downloads
    fn test_download_fixtures() {
        for fixture in FIXTURES {
            println!("Downloading: {} - {}", fixture.name, fixture.description);

            match fixture.download() {
                Ok(content) => {
                    println!("  ✓ Downloaded {} bytes", content.len());
                    assert!(!content.is_empty(), "Content should not be empty");
                }
                Err(e) => {
                    // Network tests can fail, so warn instead of panic
                    eprintln!("  ⚠ Failed to download: {}", e);
                }
            }
        }
    }

    #[test]
    fn test_cache_path_generation() {
        let fixture = &FIXTURES[0];
        let path = fixture.cache_path();
        assert!(path.to_string_lossy().contains("rust-book"));
        assert!(path.to_string_lossy().contains(&fixture.commit[..8]));
    }

    #[test]
    fn test_raw_url_generation() {
        let fixture = &FIXTURES[0];
        let url = fixture.raw_url();
        assert!(url.starts_with("https://raw.githubusercontent.com/"));
        assert!(url.contains(&fixture.commit));
        assert!(url.contains(&fixture.path));
    }
}
