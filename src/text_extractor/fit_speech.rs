use regex::Regex;

use crate::{
    remove_titles_from_str,
    text_utils::{remove_multiple_of, remove_paran_contents},
    UNREALISTIC_SPEECH_LENGTH,
};

#[derive(Debug, Clone)]
pub struct FitSpeechRegexes {
    pub president_regex: Regex,
    pub president_last_regex: Regex,
    pub president_no_one_left: Regex,
    pub only_president: Regex,
    pub steno_info_regex: Regex,
    pub br_steno_info_regex: Regex,
    pub time_marker_re: Regex,
}

pub fn fit_speech(
    unfitted_speech: String,
    name: &str,
    fit_speech_regexes: &FitSpeechRegexes,
    is_speech_snippet: bool,
) -> Option<String> {
    let FitSpeechRegexes {
        president_regex,
        president_last_regex,
        president_no_one_left,
        only_president: _,
        steno_info_regex: _,
        br_steno_info_regex: _,
        time_marker_re,
    } = fit_speech_regexes;

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

    let start_at_delegate_speech = if is_speech_snippet {
        if let Some(start_idx) = name_regex.find(&unfitted_speech).map(|x| x.start()) {
            &unfitted_speech[start_idx..]
        } else {
            unfitted_speech.as_str()
        }
    } else {
        let time_marked_speech = if let Some(matched) = time_marker_re.find(&unfitted_speech) {
            &unfitted_speech[matched.end()..]
        } else {
            unfitted_speech.as_str()
        };

        if let Some(start_idx) = name_regex.find(time_marked_speech).map(|x| x.start()) {
            &time_marked_speech[start_idx..]
        } else {
            time_marked_speech
        }
    };

    let end_speech_idx = time_marker_re
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

    let fitted_speech = start_at_delegate_speech[..end_speech_idx].to_string();
    Some(fitted_speech)
}
