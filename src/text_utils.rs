use std::collections::HashMap;

use scraper::{html, Selector};

pub fn remove_multiple_of(text: &str, of: char) -> String {
    let mut had_of_before = false;
    text.chars()
        .filter(|x| {
            let res = had_of_before;
            had_of_before = *x == of;
            !res || !had_of_before
        })
        .collect::<String>()
}

pub fn inner_text_to_href_link(html: &str) -> HashMap<String, String> {
    let doc = html::Html::parse_document(html);

    let selector = Selector::parse("a").unwrap();
    todo!()
}

pub fn remove_paran_contents(
    text: &str,
    opening_paran: char,
    closening_paran: char,
) -> (Vec<String>, String) {
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut paran_infos: Vec<(usize, usize)> = Vec::new();
    let paran_contents_string_components: Vec<String> = Vec::new();

    for (idx, char) in text.bytes().enumerate() {
        if char as char == opening_paran {
            stack.push((opening_paran, idx));
        }
        if char as char == closening_paran
            && stack.last().map_or(false, |&(c, _)| c == opening_paran)
        {
            let paran_start_idx = stack.pop().unwrap().1;
            if stack.is_empty() {
                paran_infos.push((paran_start_idx, idx));
            }
        }
    }

    let mut out_text = String::with_capacity(text.len());
    let mut outer_start = 0;
    for &(start, end) in &paran_infos {
        out_text.push_str(&text[outer_start..start]);
        outer_start = end + 1;
    }
    if outer_start < text.len() {
        out_text.push_str(&text[outer_start + 1..]);
    }

    (paran_contents_string_components, out_text)
}

#[cfg(test)]
mod tests {
    use super::{remove_multiple_of, remove_paran_contents};

    #[test]
    fn test_remove_paran_contents() {
        let (_, text) = remove_paran_contents(
            "Tim mit kleinen (sehr kleinen) Geräten (sehr, (sehr) sehr wahr)",
            '(',
            ')',
        );
        println!("text: {text}");
    }

    #[test]
    fn test_remove_paran_contents1() {
        let (_, text) = remove_paran_contents(
            "Tim mit kleinen (sehr kleinen) Geräten(sehr, (sehr) sehr wahr) SUPER duper wahr!!",
            '(',
            ')',
        );
        println!("text: {text}");
    }

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
