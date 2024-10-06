use crate::text_utils::remove_multiple_of;

pub fn filter_name_extraction<'a>(
    iter: impl Iterator<Item = &'a str> + 'a,
) -> impl Iterator<Item = String> + 'a {
    iter.map(|name| name.trim_matches([' ', ' ', '\r', '\n', '\u{ad}']))
        .map(|name| name.trim_matches(char::is_whitespace))
        .map(|name| name.replace('\n', " "))
        .map(|name| name.replace('\u{ad}', ""))
        .map(|name| name.replace('\r', " "))
        .map(|name| name.replace('\u{a0}', " "))
        .map(|name| remove_multiple_of(&name, ' '))
    // .map(|name| remove_multiple_of(&name, '\n'))
}
