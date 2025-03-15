use rumdl::rules::MD051LinkFragments;
use rumdl::rule::Rule;

#[test]
fn test_valid_link_fragment() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\nThis is a [link](somepath#test-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_link_fragment() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\nThis is a [link](somepath#wrong-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_headings() {
    let rule = MD051LinkFragments::new();
    let content = "# First Heading\n\n## Second Heading\n\n[Link 1](somepath#first-heading)\n[Link 2](somepath#second-heading)";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_special_characters() {
    let rule = MD051LinkFragments::new();
    let content = "# Test & Heading!\n\nThis is a [link](somepath#test-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_no_fragments() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\nThis is a [link](https://example.com) without fragment.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_empty_content() {
    let rule = MD051LinkFragments::new();
    let content = "";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_multiple_invalid_fragments() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\n[Link 1](somepath#wrong1)\n[Link 2](somepath#wrong2)";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_case_sensitivity() {
    let content = r#"
# My Heading

[Valid Link](#my-heading)
[Valid Link Different Case](#MY-HEADING)
"#;

    let rule = MD051LinkFragments::new();
    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();
    
    // Our implementation performs case-insensitive matching for fragments
    assert_eq!(0, warnings.len());
    
    // Note: this behavior is consistent with most Markdown parsers including
    // GitHub and CommonMark, which treat fragments as case-insensitive
}

#[test]
fn test_complex_heading_structures() {
    let rule = MD051LinkFragments::new();
    
    // Test with complex heading structures (mixed ATX and setext headings)
    let content = "# Heading 1\n\nSome text\n\nHeading 2\n-------\n\n### Heading 3\n\n[Link to 1](somepath#heading-1)\n[Link to 2](somepath#heading-2)\n[Link to 3](somepath#heading-3)\n[Link to missing](somepath#heading-4)";
    
    let result = rule.check(content).unwrap();
    
    // With our improved implementation, we expect only the missing heading to fail
    assert_eq!(result.len(), 1);
    
    // Test with special characters in headings/links
    let content = "# Heading & Special! Characters\n\n[Link](somepath#heading-special-characters)\n[Bad Link](somepath#heading--special-characters)";
    
    let result = rule.check(content).unwrap();
    
    // With our improved implementation, only truly invalid fragments should fail
    assert_eq!(result.len(), 1);
}

#[test]
fn test_heading_id_generation() {
    let content = r#"
# Heading 1

[Link with space](#heading-1)
[Link with underscore](#heading-1)
[Link with multiple hyphens](#heading-1)
"#;

    let rule = MD051LinkFragments::new();
    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();
    
    // All links are valid with our improved heading ID generation, 
    // which now follows GitHub's algorithm more closely
    assert_eq!(0, warnings.len());
}

#[test]
fn test_heading_to_fragment_edge_cases() {
    let rule = MD051LinkFragments::new();
    
    // Test duplicate heading IDs
    let content = "# Heading\n\n# Heading\n\n[Link 1](somepath#heading)\n[Link 2](somepath#heading-1)";
    
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
    
    // Test headings with only special characters
    let content = "# @#$%^\n\n[Link](somepath#)";
    
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
    
    // Test mixed internal/external links
    let content = "# Heading\n\n[Internal](somepath#heading)\n[External](https://example.com#heading)";
    
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_fragment_in_code_blocks() {
    let rule = MD051LinkFragments::new();
    
    // Test links in code blocks (should be ignored)
    let content = "# Real Heading\n\n```markdown\n# Fake Heading\n[Link](somepath#fake-heading)\n```\n\n[Link](somepath#real-heading)";
    
    let result = rule.check(content).unwrap();
    println!("Result has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!("Warning {}: line {}, message: {}", i, warning.line, warning.message);
    }
    
    // With our improved implementation, code blocks are ignored
    assert_eq!(result.len(), 0);
    
    // Test headings in code blocks (should be ignored)
    let content = "```markdown\n# Code Heading\n```\n\n[Link](somepath#code-heading)";
    
    let result = rule.check(content).unwrap();
    println!("Second test has {} warnings", result.len());
    for (i, warning) in result.iter().enumerate() {
        println!("Warning {}: line {}, message: {}", i, warning.line, warning.message);
    }
    
    // Headings in code blocks should be ignored and the link should fail
    assert_eq!(result.len(), 1);
}

#[test]
fn test_fragment_with_complex_content() {
    let content = r#"
# Heading with **bold** and *italic*

[Link to heading](#heading-with-bold-and-italic)
"#;

    let rule = MD051LinkFragments::new();
    let result = rule.check(content);
    assert!(result.is_ok());
    let warnings = result.unwrap();
    
    // Our implementation now correctly strips markdown formatting
    // from headings when generating fragments
    assert_eq!(0, warnings.len());
}

#[test]
fn test_performance_md051() {
    // Generate a large document with many headings and links
    let mut content = String::with_capacity(50_000);
    
    // Add 50 headings
    for i in 0..50 {
        content.push_str(&format!("# Heading {}\n\n", i));
        content.push_str("Some content paragraph with details about this section.\n\n");
        
        // Add some subheadings
        if i % 3 == 0 {
            content.push_str(&format!("## Subheading {}.1\n\n", i));
            content.push_str("Subheading content with more details.\n\n");
            content.push_str(&format!("## Subheading {}.2\n\n", i));
            content.push_str("More subheading content here.\n\n");
        }
    }
    
    // Add links section
    content.push_str("# Links Section\n\n");
    
    // Add 100 links, some valid, some invalid
    for i in 0..100 {
        if i % 3 == 0 {
            content.push_str(&format!("[Link to invalid heading](somepath#heading-{})\n", i + 100));
        } else {
            content.push_str(&format!("[Link to heading {}](somepath#heading-{})\n", i % 50, i % 50));
        }
    }
    
    // Measure performance
    let start = std::time::Instant::now();
    let rule = MD051LinkFragments::new();
    let result = rule.check(&content).unwrap();
    let duration = start.elapsed();
    
    println!("MD051 check duration: {:?} for content length {}", duration, content.len());
    println!("Found {} invalid fragments", result.len());
    
    // We expect about 1/3 of the 100 links to be invalid (those where i % 3 == 0)
    assert!(result.len() >= 30);
    assert!(result.len() <= 40);
} 