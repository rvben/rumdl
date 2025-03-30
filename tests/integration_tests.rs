use rumdl::rule::Rule;
use rumdl::MD015NoMissingSpaceAfterListMarker;
use rumdl::MD053LinkImageReferenceDefinitions;
use rumdl::rules::{MD003HeadingStyle, MD017NoEmphasisAsHeading, MD036NoEmphasisOnlyFirst};

#[test]
fn cross_rule_md015_md053() {
    let content = "- [Link][ref]\n* [Another][ref2]";

    // Apply MD015 fix
    let fixed = MD015NoMissingSpaceAfterListMarker::new()
        .fix(content)
        .unwrap();

    // Check MD053 results
    let result = MD053LinkImageReferenceDefinitions::new(vec![])
        .check(&fixed)
        .unwrap();

    // The rule should not generate any warnings because all references are used
    assert!(
        result.is_empty(),
        "Should not detect unused refs after MD015 fix: {:?}",
        result
    );
}

#[test]
fn test_heading_duplication_fix() {
    // This test verifies that the MD003 rule now properly fixes duplicated headings
    // in various formats
    
    // Run each test case individually to check the fix for each case
    let md003 = MD003HeadingStyle::default();
    
    // Test case 1: Simple duplication - now properly fixed
    {
        let input = "## Heading## Heading";
        let fixed = md003.fix(input).unwrap();
        println!("Input: '{}'", input);
        println!("Current output: '{}'", fixed.trim());
        
        // Assert expected behavior with the new implementation
        assert_eq!(fixed.trim(), "## Heading",
            "Simple duplication should be fixed");
    }
    
    // Test case 2: Duplication with period separator - already fixed in previous implementation
    {
        let input = "## Heading.## Heading";
        let fixed = md003.fix(input).unwrap();
        println!("Input: '{}'", input);
        println!("Current output: '{}'", fixed.trim());
        
        // Assert expected behavior
        assert_eq!(fixed.trim(), "## Heading",
            "Duplication with period separator should be fixed");
    }
    
    // Test case 3: Trailing emphasis - now properly fixed
    {
        let input = "## Heading**Heading**";
        let fixed = md003.fix(input).unwrap();
        println!("Input: '{}'", input);
        println!("Current output: '{}'", fixed.trim());
        
        // Assert expected behavior with the new implementation
        assert_eq!(fixed.trim(), "## Heading",
            "Duplication with emphasis should be fixed");
    }
    
    // Test case 4: Complex real-world example - now properly fixed
    {
        let input = "## An extremely fast Markdown linter and formatter, written in Rust.## An extremely fast Markdown linter and formatter, written in Rust.**An extremely fast Markdown linter and formatter, written in Rust.**";
        let fixed = md003.fix(input).unwrap();
        println!("Input: '{}'", input);
        println!("Current output: '{}'", fixed.trim());
        
        // Assert expected behavior with the new implementation
        assert_eq!(
            fixed.trim(), 
            "## An extremely fast Markdown linter and formatter, written in Rust",
            "Complex duplication should be fixed"
        );
    }
    
    println!("");
    println!("=== Heading Duplication Fixed ===");
    println!("The linter now properly handles duplicated headings of various forms:");
    println!("1. Simple duplications without punctuation ('## Heading## Heading') are now fixed to '## Heading'");
    println!("2. Duplications with a period separator ('## Heading.## Heading') are fixed to '## Heading'");
    println!("3. Trailing emphasis duplications ('## Heading**Heading**') are fixed to '## Heading'");
    println!("4. Complex cases with multiple duplications are properly cleaned up");
}

#[test]
fn test_rule_application_order() {
    // This test verifies that applying rules in different orders 
    // now consistently produces the correct result for duplicated headings
    
    let content = "## Heading## Heading**Heading**";
    
    // Apply in order: MD017 -> MD036 -> MD003
    let path1 = {
        let step1 = MD017NoEmphasisAsHeading::default().fix(content).unwrap();
        let step2 = MD036NoEmphasisOnlyFirst {}.fix(&step1).unwrap();
        MD003HeadingStyle::default().fix(&step2).unwrap()
    };
    
    // Apply in order: MD036 -> MD017 -> MD003
    let path2 = {
        let step1 = MD036NoEmphasisOnlyFirst {}.fix(content).unwrap();
        let step2 = MD017NoEmphasisAsHeading::default().fix(&step1).unwrap();
        MD003HeadingStyle::default().fix(&step2).unwrap()
    };
    
    // Apply in order: MD003 -> MD017 -> MD036
    let path3 = {
        let step1 = MD003HeadingStyle::default().fix(content).unwrap();
        let step2 = MD017NoEmphasisAsHeading::default().fix(&step1).unwrap();
        MD036NoEmphasisOnlyFirst {}.fix(&step2).unwrap()
    };
    
    // Document current behavior - duplication should now be fixed
    println!("Input: '{}'", content);
    println!("Current path1 output: '{}'", path1.trim());
    println!("Current path2 output: '{}'", path2.trim());
    println!("Current path3 output: '{}'", path3.trim());
    println!("Expected behavior: All paths should result in '## Heading' (no duplication)");
    
    // Verify that all paths lead to the same cleaned result
    assert_eq!(path1.trim(), "## Heading");
    assert_eq!(path2.trim(), "## Heading");
    assert_eq!(path3.trim(), "## Heading");
}

#[test]
fn test_md036_for_emphasis_only_lines() {
    // Test for the proper purpose of MD036 - emphasis-only lines
    let content = "Normal text\n\n**This should be a heading**\n\nMore text";
    
    // Apply MD036 (NoEmphasisOnlyFirst) fix
    let md036 = MD036NoEmphasisOnlyFirst {};
    let fixed_md036 = md036.fix(content).unwrap();
    
    // The emphasis should be converted to a proper heading
    assert!(fixed_md036.contains("## This should be a heading"));
    assert!(!fixed_md036.contains("**This should be a heading**"));
}
