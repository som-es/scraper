use common_scrapes::AcceptedState;

use crate::legis_init::{is_accepted_based_on_votes, votes::extract_vote_count};

#[test]
fn vote_counts_support_nationalrat_and_bundesrat_wording() {
    for (text, expected) in [
        ("Namentliche Abstimmung (abgegebene Stimmen: 177; davon Ja-Stimmen: 123, Nein-Stimmen: 54)", (177, 123, 54, 0)),
        ("Namentliche Abstimmung (60 abgegebene Stimmen, 37 Ja-Stimmen, 23 Nein-Stimmen)", (60, 37, 23, 0)),
        ("Namentliche Abstimmung (abgegebene Stimmen: 100, davon Ja-Stimmen: 60, Nein-Stimmen: 38, ungültig 2)", (100, 60, 38, 2)),
        ("Namentliche Abstimmung (100 abgegebene Stimmen, 60 Ja-Stimmen, 38 Nein-Stimmen, 2 ungültig)", (100, 60, 38, 2)),
    ] {
        assert_eq!(extract_vote_count(text), Some(expected), "{text}");
    }
}

#[test]
fn incomplete_vote_counts_are_not_reported_as_zero_votes() {
    for text in [
        "",
        "Antrag einstimmig angenommen",
        "Namentliche Abstimmung",
        "abgegebene Stimmen: 177, davon Ja-Stimmen: 123",
        "Ja-Stimmen: 123, Nein-Stimmen: 54",
    ] {
        assert_eq!(extract_vote_count(text), None, "{text}");
    }
}

#[test]
fn two_thirds_majority_handles_exact_and_rounded_thresholds() {
    for (total, yes, expected) in [
        (120, 79, AcceptedState::Declined),
        (120, 80, AcceptedState::Accepted),
        (120, 81, AcceptedState::Accepted),
        (121, 80, AcceptedState::Declined),
        (121, 81, AcceptedState::Accepted),
    ] {
        assert_eq!(
            is_accepted_based_on_votes(total, yes, false),
            expected,
            "{yes} of {total}"
        );
    }
}

#[test]
fn simple_majority_requires_more_than_half_the_votes() {
    // Parliament glossary: more than half, so an exact tie is insufficient.
    // https://www.parlament.gv.at/verstehen/glossare/allgemein
    for (total, yes, expected) in [
        (100, 49, AcceptedState::Declined),
        (100, 50, AcceptedState::Declined),
        (100, 51, AcceptedState::Accepted),
        (101, 50, AcceptedState::Declined),
        (101, 51, AcceptedState::Accepted),
    ] {
        assert_eq!(
            is_accepted_based_on_votes(total, yes, true),
            expected,
            "{yes} of {total}"
        );
    }
}

#[test]
fn named_votes_reject_reversed_or_truncated_sections_without_panicking() {
    use crate::legis_init::votes::extract_votes_by_name;
    for text in [
        "Mit Nein stimmten die Abgeordneten: Beispiel.\nMit Ja stimmten die Abgeordneten: Muster.",
        "Mit Ja stimmten die Abgeordneten: Muster.\nMit Nein stimmten die Abgeordneten:",
    ] {
        assert!(extract_votes_by_name(text).is_none());
    }
}

#[test]
fn named_votes_preserve_unicode_names_and_section_boundaries() {
    use crate::legis_init::votes::extract_votes_by_name;
    let v=extract_votes_by_name("Mit Ja stimmten die Abgeordneten: Müller, Huber.\nMit Nein stimmten die Abgeordneten: Öztürk, Maier.").unwrap();
    assert_eq!(
        v.pro.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["Müller", "Huber"]
    );
    assert_eq!(
        v.contra.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["Öztürk", "Maier"]
    );
}
