use rumdl::rules::MD051LinkFragments;
use rumdl::rule::Rule;

#[test]
fn test_valid_link_fragment() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\nThis is a [link](#test-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_invalid_link_fragment() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\nThis is a [link](#wrong-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_multiple_headings() {
    let rule = MD051LinkFragments::new();
    let content = "# First Heading\n\n## Second Heading\n\n[Link 1](#first-heading)\n[Link 2](#second-heading)";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_special_characters() {
    let rule = MD051LinkFragments::new();
    let content = "# Test & Heading!\n\nThis is a [link](#test-heading) to the heading.";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
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
    let content = "# Test Heading\n\n[Link 1](#wrong1)\n[Link 2](#wrong2)";
    let result = rule.check(content).unwrap();
    assert_eq!(result.len(), 2);
}

#[test]
fn test_case_sensitivity() {
    let rule = MD051LinkFragments::new();
    let content = "# Test Heading\n\n[Link](#TEST-HEADING)";
    let result = rule.check(content).unwrap();
    assert!(result.is_empty());
} 