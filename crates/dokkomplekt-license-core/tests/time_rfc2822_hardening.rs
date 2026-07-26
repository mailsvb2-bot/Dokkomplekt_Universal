use time::{format_description::well_known::Rfc2822, OffsetDateTime};

fn rfc2822_with_nested_comments(depth: usize) -> String {
    format!(
        "Sat, {}x{} 02 Jan 2021 03:04:05 +0000",
        "(".repeat(depth),
        ")".repeat(depth)
    )
}

#[test]
fn ordinary_nested_rfc2822_comment_still_parses() {
    let value = rfc2822_with_nested_comments(4);
    assert!(OffsetDateTime::parse(&value, &Rfc2822).is_ok());
}

#[test]
fn adversarial_rfc2822_comment_depth_is_rejected() {
    let value = rfc2822_with_nested_comments(40);
    assert!(OffsetDateTime::parse(&value, &Rfc2822).is_err());
}
