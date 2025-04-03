mod md001_test;
mod md002_test;
mod md003_test;
mod md004_test;
mod md005_test;
mod md006_test;
mod md007_test;
mod md008_test;
mod md009_test;
mod md010_test;
mod md011_test;
mod md012_test;
mod md013_test;
mod md014_test;
mod md015_test;
mod md016_test;
mod md017_test;
mod md018_test;
mod md019_test;
mod md020_test;
mod md021_test;
mod md022_test;
mod md023_extended_test;
mod md023_test;
mod md024_test;
mod md025_test;
mod md026_test;
mod md027_test;
mod md028_test;
mod md029_test;
mod md030_test;
mod md031_test;
mod md032_test;
mod md033_test;
mod md034_test;
mod md035_test;
mod md036_test;
mod md037_test;
mod md038_test;
mod md039_test;
mod md040_test;
mod md041_test;
mod md042_test;
mod md043_test;
mod md044_test;
mod md045_test;
mod md046_test;
mod md047_test;
mod md048_test;
mod md049_test;
mod md050_test;
mod md051_test;
mod md052_test;
mod md053_additional_test;
mod md053_test;
mod md054_test;
mod md055_test;
mod md056_test;
mod md057_test;
mod md058_test;

// Unicode-specific test modules
mod md001_unicode_test;
mod md006_unicode_test;
mod md054_unicode_test;

#[cfg(test)]
mod performance_tests {
    use rumdl::rule::Rule;
    use rumdl::rules::{MD033NoInlineHtml, MD037SpacesAroundEmphasis};
    use std::time::Instant;

    #[test]
    #[ignore] // Ignore by default as it's mainly for manual performance checking
    fn test_performance_sanity() {
        eprintln!("Running performance sanity test...");

        // Generate a large markdown file with many tags/emphasis markers
        let mut content = String::with_capacity(100_000);
        for i in 0..1000 {
            content.push_str(&format!(
                "Line {} with <span>HTML</span> and *emphasis*\n",
                i
            ));
        }

        eprintln!("Generated test content of {} bytes", content.len());

        // Test MD033 performance
        let html_rule = MD033NoInlineHtml::default();
        let start = Instant::now();
        let html_result = html_rule.check(&content).unwrap();
        let html_duration = start.elapsed();
        eprintln!(
            "MD033 check duration: {:?}, {} warnings",
            html_duration,
            html_result.len()
        );

        // Test MD037 performance
        let emphasis_rule = MD037SpacesAroundEmphasis;
        let start = Instant::now();
        let emphasis_result = emphasis_rule.check(&content).unwrap();
        let emphasis_duration = start.elapsed();
        eprintln!(
            "MD037 check duration: {:?}, {} warnings",
            emphasis_duration,
            emphasis_result.len()
        );

        // Add a basic assertion to ensure the test is meaningful
        assert!(!html_result.is_empty(), "Should have detected HTML tags");
        assert_eq!(
            emphasis_result.len(),
            0,
            "Should not have detected emphasis issues"
        );

        eprintln!("Performance test completed successfully");
    }
}
