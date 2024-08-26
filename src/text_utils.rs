use std::collections::HashMap;

use scraper::{html, Selector};

pub fn remove_multiple_of(text: &str, of: char) -> String {
    let mut had_of_before = false;
    text.chars()
        .into_iter()
        .filter(|x| {
            let res = had_of_before;
            if *x == of {
                had_of_before = true
            } else {
                had_of_before = false;
            }
            !res || !had_of_before
        })
        .collect::<String>()
}

pub fn inner_text_to_href_link(html: &str) -> HashMap<String, String> {
    let doc = html::Html::parse_document(html);

    let selector = Selector::parse("a").unwrap();
    todo!()
}

#[cfg(test)]
mod tests {
    use super::remove_multiple_of;

    #[test]
    fn test_remove_multiple_spaces_in_name() {
        let name = "Lukas   Liebt  Rötzer";
        let res = remove_multiple_of(name, ' ');

        assert_eq!(res, "Lukas Liebt Rötzer")
    }

    #[test]
    fn test_remove_multiple_spaces_in_name2() {
        let name = "Lukas ist Rötzer";
        let res = remove_multiple_of(name, ' ');

        assert_eq!(res, "Lukas ist Rötzer")
    }

    #[test]
    fn test_remove_multiple_spaces_in_name3() {
        let name = "Lukas  Rötzer";
        let res = remove_multiple_of(name, ' ');

        assert_eq!(res, "Lukas Rötzer")
    }
}
