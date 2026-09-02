use std::path::Display;

use chrono::{DateTime, NaiveDate, Utc};
use serde_derive::{Deserialize, Serialize};

use crate::Document;

/// State of a Volksbegehren that is currently on the ballot
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VolksbgState {
    /// Eintragungswoche is announced, but has not started yet
    Scheduled,
    /// Eintragungswoche is running, signatures can be handed in
    Open,
}

impl std::fmt::Display for VolksbgState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = self.as_str();
        write!(f, "{str}")
    }
}

impl VolksbgState {
    pub fn as_str(&self) -> &str {
        match self {
            VolksbgState::Scheduled => "Scheduled",
            VolksbgState::Open => "Open",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Volksbg {
    /// slug of the BMI page, e.g. `gratis_verhuetung`
    pub id: String,
    /// Kurzbezeichnung, e.g. `GRATIS Verhütung`
    pub title: String,
    /// Wortlaut of the Volksbegehren, once the detail page was fetched
    pub description: Option<String>,
    pub documents: Vec<Document>,
    pub overview_url: String,
    pub state: Option<VolksbgState>,
    /// Tag der Entscheidung des BMI über den Einleitungsantrag
    pub ruling_date: Option<NaiveDate>,
    /// Stichtag der Wählerevidenz
    pub cut_off_date: Option<NaiveDate>,
}

/// One week in which listed Volksbegehren may be signed
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VolksbgEintragungswoche {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub cut_off_date: Option<NaiveDate>,
    /// Online signatures end at 20:00 (Europe/Vienna) on the last day
    pub online_deadline_utc: Option<DateTime<Utc>>,
    /// Liste der Eintragungslokale der Gemeinden (xlsx)
    pub polling_stations_url: Option<String>,
    pub volksbegehren: Vec<Volksbg>,
}

impl VolksbgEintragungswoche {
    pub fn state_on(&self, today: NaiveDate) -> VolksbgState {
        if today < self.start {
            VolksbgState::Scheduled
        } else {
            VolksbgState::Open
        }
    }

    pub fn is_open_on(&self, today: NaiveDate) -> bool {
        self.state_on(today) == VolksbgState::Open && today <= self.end
    }

    pub fn apply_state(&mut self, today: NaiveDate) -> &mut Self {
        let state = self.state_on(today);
        for volksbg in self.volksbegehren.iter_mut() {
            volksbg.state = Some(state);
        }
        self
    }
}
