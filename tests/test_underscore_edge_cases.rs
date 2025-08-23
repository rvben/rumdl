use rumdl_lib::utils::anchor_styles::kramdown_gfm;

#[test]
fn test_jekyll_underscore_edge_cases() {
    // Test cases from Issue #39 with expected Jekyll/kramdown GFM behavior
    let test_cases = vec![
        ("PHP $_REQUEST", "php-_request"),
        ("sched_debug", "sched_debug"),
        ("Update login_type", "update-login_type"),
        ("Add ldap_monitor to delegator$", "add-ldap_monitor-to-delegator"),
    ];

    for (heading, expected) in test_cases {
        let actual = kramdown_gfm::heading_to_fragment(heading);
        assert_eq!(
            actual, expected,
            "Mismatch for '{heading}': expected '{expected}', got '{actual}'"
        );
    }
}
