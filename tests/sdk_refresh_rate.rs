#[path = "../sys/build_support.rs"]
mod build_support;
#[test]
fn pinned_sdk_patch_only_replaces_refresh_rate_query_and_accepts_crlf() {
    let original =
        std::fs::read_to_string("sys/vendor/Spout2/SPOUTSDK/SpoutGL/SpoutUtils.cpp").unwrap();
    let patched = build_support::current_refresh_rate(&original);
    assert_eq!(
        patched,
        build_support::current_refresh_rate(&original.replace("\r\n", "\n").replace("\n", "\r\n"))
    );
    let original = original.replace("\r\n", "\n");
    let start = original.find("\tdouble GetRefreshRate()").unwrap();
    let end = start + original[start..].find("\n\t}").unwrap() + 3;
    assert!(patched.starts_with(&original[..start]));
    assert!(patched.ends_with(&original[end..]));
    assert!(patched.contains("ENUM_CURRENT_SETTINGS"));
    assert!(!patched.contains("dwCurrentSettings++"));
}
#[test]
#[should_panic(expected = "pinned Spout2 GetRefreshRate changed")]
fn changed_sdk_is_rejected_instead_of_silently_skipping_the_fix() {
    build_support::current_refresh_rate("double GetRefreshRate() { return 0; }");
}
