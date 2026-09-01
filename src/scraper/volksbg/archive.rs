//! Links of an archived page point to the archive (`web.archive.org` rewrites
//! every `href` of the snapshot it serves). For extraction they are rewritten to
//! the page they were taken from, so that `overview_url`, document links and the
//! detail pages that are fetched next point at the live BMI pages.
//! Only the URL that was fetched keeps its archive wrapper.

/// Hosts the Wayback Machine serves snapshots from (`archive.org` is an alias)
const ARCHIVE_HOSTS: [&str; 2] = ["web.archive.org", "archive.org"];

/// `https://web.archive.org/web/20260614003610/https://www.bmi.gv.at/411/start.html`
/// => `https://www.bmi.gv.at/411/start.html`
///
/// The timestamp may carry a replay modifier (`20260614003610if_`) or be short
/// (`20260614`). Snapshots that wrap a path without a scheme — as the old CMS
/// layout of the BMI links them — stay as they are, their origin host is
/// unknowable: `web.archive.org/web/20260216/Volksbegehren_der_XX_Gesetzgebungsperiode/x/start.aspx`
pub fn unwrap_archive(url: &str) -> Option<&str> {
    let (host, path) = url
        .split_once("://")
        .map(|(_, without_scheme)| without_scheme)?
        .split_once('/')?;

    if !ARCHIVE_HOSTS.contains(&host) {
        return None;
    }

    // `20260614003610if_/https://www.bmi.gv.at/411/start.html`
    let (_, original) = path.strip_prefix("web/")?.split_once('/')?;

    let is_url = original.starts_with("http://") || original.starts_with("https://");
    is_url.then_some(original)
}

/// URL of the page as it is served live, `url` unchanged when it isn't archived
pub fn live_url(url: &str) -> String {
    unwrap_archive(url).unwrap_or(url).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayback_links_fuehren_auf_die_live_seite() {
        let archived = [
            "https://web.archive.org/web/20260614003610/https://www.bmi.gv.at/411/start.html",
            "http://web.archive.org/web/20260614003610if_/https://www.bmi.gv.at/411/start.html",
            "https://archive.org/web/20260614id_/https://www.bmi.gv.at/411/start.html",
        ];

        for url in archived {
            assert_eq!(
                live_url(url),
                "https://www.bmi.gv.at/411/start.html",
                "{url}"
            );
        }

        assert_eq!(
            unwrap_archive(
                "https://web.archive.org/web/20260614/https://www.bmi.gv.at/411/files/x.pdf"
            ),
            Some("https://www.bmi.gv.at/411/files/x.pdf")
        );

        assert_eq!(
            unwrap_archive("https://web.archive.org/web/20260614/https://bmi.vergabeportal.at/"),
            Some("https://bmi.vergabeportal.at/")
        );
    }

    #[test]
    fn nicht_archivierte_urls_bleiben_unchanged() {
        assert_eq!(unwrap_archive("https://www.bmi.gv.at/411/start.html"), None);

        // snapshots of the old CMS wrap a path without a scheme
        assert_eq!(
            unwrap_archive(
                "https://web.archive.org/web/20260216/Volksbegehren_der_XX_Gesetzgebungsperiode/GRATIS_Verhuetung/start.aspx"
            ),
            None
        );

        // a BMI path that merely looks like a snapshot
        assert_eq!(
            unwrap_archive("https://www.bmi.gv.at/web/20260614/https://www.bmi.gv.at/411/x.html"),
            None
        );
    }
}
