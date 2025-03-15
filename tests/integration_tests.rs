use rumdl::MD015NoMissingSpaceAfterListMarker;
use rumdl::MD053LinkImageReferenceDefinitions;
use rumdl::rule::Rule;

#[test]
fn cross_rule_md015_md053() {
    let content = "- [Link][ref]\n* [Another][ref2]";
    
    // Apply MD015 fix
    let fixed = MD015NoMissingSpaceAfterListMarker::new().fix(content).unwrap();
    
    // Check MD053 results
    let result = MD053LinkImageReferenceDefinitions::new(vec![]).check(&fixed).unwrap();
    
    // The rule should not generate any warnings because all references are used
    assert!(result.is_empty(), 
        "Should not detect unused refs after MD015 fix: {:?}",
        result
    );
}
