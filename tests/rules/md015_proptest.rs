use proptest::prelude::*;
use super::*;

proptest! {
    #[test]
    fn fixes_any_missing_space(s in r"^[ \t]*[-*+\d][.)]?\S+") {
        let rule = MD015NoMissingSpaceAfterListMarker::new();
        let fixed = rule.fix(&s);
        assert!(
            fixed.contains(" ") || !s.trim_start().is_empty(),
            "Failed to fix: {} -> {}",
            s,
            fixed
        );
    }

    #[test]
    fn preserves_valid_items(s in r"^[ \t]*[-*+\d][.)] [^\s].*$") {
        let rule = MD015NoMissingSpaceAfterListMarker::new();
        let fixed = rule.fix(&s);
        assert_eq!(fixed, s, "Valid item was altered: {} -> {}", s, fixed);
    }
}
