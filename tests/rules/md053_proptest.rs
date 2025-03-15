use proptest::prelude::*;
use rumdl::rules::MD053LinkImageReferenceDefinitions;

proptest! {
    #[test]
    fn detects_unused_refs(
        refs in prop::collection::vec(r"\[([a-z][a-z0-9_-]*)\]:", 1..10),
        uses in prop::collection::vec(r"\[([a-z][a-z0-9_-]*)\]", 1..10)
    ) {
        let mut content = String::new();
        let mut expected_unused = Vec::new();

        // Generate reference definitions
        for r in &refs {
            content.push_str(&format!("{} https://example.com\n", r));
            if !uses.contains(&r) {
                expected_unused.push(r.clone());
            }
        }

        // Generate content usages
        content.push_str("\n");
        for u in &uses {
            content.push_str(&format!("Text {} ", u));
        }

        let rule = MD053LinkImageReferenceDefinitions::new(vec![]);
        let result = rule.check(&content);
        
        prop_assert_eq!(
            result.unused_refs.iter().map(|(r,_,_)| r).collect::<Vec<_>>(),
            expected_unused
        );
    }
}
