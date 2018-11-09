pub fn shorten_string_to(mut val: &str, target: usize) -> &str {
    while !val.is_empty() && wcwidth::str_width(val).unwrap() > target {
        val = pop_char(val);
    }

    val
}

fn pop_char(val: &str) -> &str {
    if val.is_empty() {
        return val;
    }

    for trim in 1..=4 {
        let new_end = val.len() - trim;
        if val.is_char_boundary(new_end) {
            return &val[..new_end];
        }
    }

    unreachable!("invalid utf-8 in string: {:?}", val)
}

#[test]
fn shrink() {
    assert_eq!("hell", shorten_string_to("hello", 4));
    let tokyo = "東京";
    assert_eq!(6, tokyo.len());
    assert_eq!(tokyo, shorten_string_to(tokyo, 4));
    assert_eq!("東", shorten_string_to(tokyo, 3));
    assert_eq!("東", shorten_string_to(tokyo, 2));
    assert_eq!("", shorten_string_to(tokyo, 1));
    assert_eq!("", shorten_string_to(tokyo, 0));

    let unicode_11 = "\u{1F975}";
    assert_eq!(unicode_11, shorten_string_to(unicode_11, 2));
    // TODO: needs new tables: https://github.com/lucy/wcwidth.rs/pull/2
    // TODO: assert_eq!("", shorten_string_to(unicode_11, 1));
    assert_eq!("", shorten_string_to(unicode_11, 0));

}
