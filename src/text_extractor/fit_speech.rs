use regex::Regex;

use crate::{
    remove_titles_from_str,
    text_utils::{remove_multiple_of, remove_paran_contents},
    UNREALISTIC_SPEECH_LENGTH,
};

pub fn fit_speech(
    unfitted_speech: String,
    name: &str,
    president_regex: &Regex,
    president_last_regex: &Regex,
    president_no_one_left: &Regex,
    only_president: &Regex,
    time_marker_re: &Regex,
) -> Option<String> {
    let unfitted_speech = unfitted_speech.replace('\u{ad}', "");
    let unfitted_speech = unfitted_speech.replace('\u{a0}', " ");
    let unfitted_speech = unfitted_speech.replace("\r\n", "\n");
    let unfitted_speech = unfitted_speech.replace('\r', "\n");
    let unfitted_speech = unfitted_speech.replace("\0", "");
    let unfitted_speech = remove_multiple_of(&unfitted_speech, '\n');

    let (_, name) = remove_paran_contents(name, '(', ')');
    let name = name.trim_matches(char::is_whitespace);
    let name = remove_titles_from_str(name);
    // BK Werner Faymann (S) remove the BK, BM etc
    let name_regex = name
        .split_whitespace()
        .filter(|x| !(*x == "BM" || *x == "RM" || *x == "BK"))
        .fold(String::new(), |mut output, split| {
            use std::fmt::Write;
            let _ = write!(output, r"{split}\s*");
            output
        });
    let name_regex = Regex::new(&name_regex).unwrap();

    let start_at_delegate_speech_idx = name_regex.find(&unfitted_speech).map(|x| x.start());

    let speech_text = match start_at_delegate_speech_idx {
        Some(start_at_delegate_speech_idx) => {
            let start_at_delegate_speech = &unfitted_speech[start_at_delegate_speech_idx..];

            let mut end_speech_idx = time_marker_re
                .find(start_at_delegate_speech)
                .map(|x| x.start())
                .unwrap_or(start_at_delegate_speech.len())
                .min(
                    president_regex
                        .find(start_at_delegate_speech)
                        .map(|x| x.start())
                        .unwrap_or(start_at_delegate_speech.len()),
                )
                .min(
                    president_last_regex
                        .find(start_at_delegate_speech)
                        .map(|x| x.start())
                        .unwrap_or(start_at_delegate_speech.len()),
                )
                .min(
                    president_no_one_left
                        .find(start_at_delegate_speech)
                        .map(|x| x.start())
                        .unwrap_or(start_at_delegate_speech.len()),
                );

            if end_speech_idx == start_at_delegate_speech.len() {
                // last "greedy" try - this is not always correct
                end_speech_idx = only_president
                    .find_iter(start_at_delegate_speech)
                    .last()
                    .map(|x| x.start())
                    .unwrap_or(start_at_delegate_speech.len());
                if end_speech_idx <= 150 {
                    // nummer sicher
                    end_speech_idx = start_at_delegate_speech.len();
                }
            }
            let fitted_speech = start_at_delegate_speech[..end_speech_idx].to_string();
            Some(fitted_speech)
        }
        None => Some(unfitted_speech),
    };

    if let Some(speech_text) = &speech_text {
        if speech_text.len() >= UNREALISTIC_SPEECH_LENGTH {
            return None;
        }
    }
    speech_text
}
