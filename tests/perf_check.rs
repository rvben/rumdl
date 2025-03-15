use rumdl::rules::{MD033NoInlineHtml, MD037SpacesAroundEmphasis};
use rumdl::rule::Rule;
use std::time::Instant;

#[test]
fn test_optimized_rules_performance() {
    // Generate a large markdown file with many tags/emphasis markers
    let mut content = String::with_capacity(100_000);
    for i in 0..1000 {
        content.push_str(&format!("Line {} with <span>HTML</span> and *emphasis*\n", i));
    }
    
    println!("Generated test content of {} bytes", content.len());
    
    // Test MD033 performance
    let html_rule = MD033NoInlineHtml::default();
    let start = Instant::now();
    let html_result = html_rule.check(&content).unwrap();
    let html_duration = start.elapsed();
    println!("MD033 check duration: {:?}, {} warnings", html_duration, html_result.len());
    
    // Test MD037 performance
    let emphasis_rule = MD037SpacesAroundEmphasis::default();
    let start = Instant::now();
    let emphasis_result = emphasis_rule.check(&content).unwrap();
    let emphasis_duration = start.elapsed();
    println!("MD037 check duration: {:?}, {} warnings", emphasis_duration, emphasis_result.len());
    
    // Add a basic assertion to ensure the test is meaningful
    assert!(html_result.len() > 0, "Should have detected HTML tags");
    assert_eq!(emphasis_result.len(), 0, "Should not have detected emphasis issues");
} 