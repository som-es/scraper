use somes_scraper::{volksbg::VolksbgScraper, AustriaScraper};

#[tokio::main]
async fn main() {
    let scraper = AustriaScraper::default();

    let url = std::env::args().nth(1).unwrap_or_else(|| {
        "https://web.archive.org/web/20260614003610/https://www.bmi.gv.at/411/start.html"
            .to_string()
    });

    let Some(eintragungswoche) = scraper.eintragungswoche_of(&url).await.unwrap() else {
        println!("keine Eintragungswoche angekuendigt auf {url}");
        return;
    };

    let eintragungswoche = scraper
        .eintragungswoche_with_details(&eintragungswoche)
        .await
        .unwrap();

    println!(
        "Eintragungszeitraum {} bis {}, Stichtag {:?}, {} Volksbegehren",
        eintragungswoche.start,
        eintragungswoche.end,
        eintragungswoche.cut_off_date,
        eintragungswoche.volksbegehren.len()
    );

    for volksbg in eintragungswoche.volksbegehren {
        println!(
            "\n{} ({}): {} Dokumente\n{}\n{}",
            volksbg.title,
            volksbg.id,
            volksbg.documents.len(),
            volksbg.overview_url,
            volksbg
                .description
                .unwrap_or_default()
                .chars()
                .take(240)
                .collect::<String>()
        );

        for document in volksbg.documents {
            println!("  - {} | {}", document.title, document.link);
        }
    }

    println!(
        "\nUnterschreiben: {}",
        somes_scraper::volksbg::VOLKSBG_SUPPORT_URL
    );
}
