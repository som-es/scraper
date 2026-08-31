use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum StatementItyp {
    #[default]
    Snme,
    Sn,
    Spet,
    SpetBr,
    Sbi,
}

impl StatementItyp {
    pub fn as_str(&self) -> &'static str {
        match self {
            StatementItyp::Snme => "SNME",
            StatementItyp::Sn => "SN",
            StatementItyp::Spet => "SPET",
            StatementItyp::SpetBr => "SPET-BR",
            StatementItyp::Sbi => "SBI",
        }
    }

    pub fn from_ityp(ityp: &str) -> Option<Self> {
        match ityp.trim().to_uppercase().as_str() {
            "SNME" => Some(StatementItyp::Snme),
            "SN" => Some(StatementItyp::Sn),
            "SPET" => Some(StatementItyp::Spet),
            "SPET-BR" => Some(StatementItyp::SpetBr),
            "SBI" => Some(StatementItyp::Sbi),
            _ => None,
        }
    }

    pub fn reference_ityp(&self) -> &'static str {
        match self {
            StatementItyp::Snme => "ME",
            StatementItyp::Spet | StatementItyp::SpetBr => "PET",
            StatementItyp::Sbi => "BI",
            StatementItyp::Sn => "I",
        }
    }

    pub fn from_reference_ityp(ityp: &str) -> Option<Self> {
        match ityp.trim().to_uppercase().as_str() {
            "ME" => Some(StatementItyp::Snme),
            "PET" => Some(StatementItyp::Spet),
            "BI" => Some(StatementItyp::Sbi),
            "I" | "A" | "AUA" => Some(StatementItyp::Sn),
            _ => None,
        }
    }
}
