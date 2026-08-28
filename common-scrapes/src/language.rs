use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Text {
    pub language: Language,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Es,
    Sl,
    En,
    De,
    Hu,
    Ro,
    Pt,
    Sv,
    Da,
    Fr,
    Pl,
    Et,
    Nl,
    El,
    Hr,
    Lv,
    It,
    Fi,
    Cs,
    Lt,
    Ga,
    Mt,
    Bg,
    Sk,
}

impl Language {
    pub const ALL: [Language; 24] = [
        Language::Es,
        Language::Sl,
        Language::En,
        Language::De,
        Language::Hu,
        Language::Ro,
        Language::Pt,
        Language::Sv,
        Language::Da,
        Language::Fr,
        Language::Pl,
        Language::Et,
        Language::Nl,
        Language::El,
        Language::Hr,
        Language::Lv,
        Language::It,
        Language::Fi,
        Language::Cs,
        Language::Lt,
        Language::Ga,
        Language::Mt,
        Language::Bg,
        Language::Sk,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Es => "es",
            Language::Sl => "sl",
            Language::En => "en",
            Language::De => "de",
            Language::Hu => "hu",
            Language::Ro => "ro",
            Language::Pt => "pt",
            Language::Sv => "sv",
            Language::Da => "da",
            Language::Fr => "fr",
            Language::Pl => "pl",
            Language::Et => "et",
            Language::Nl => "nl",
            Language::El => "el",
            Language::Hr => "hr",
            Language::Lv => "lv",
            Language::It => "it",
            Language::Fi => "fi",
            Language::Cs => "cs",
            Language::Lt => "lt",
            Language::Ga => "ga",
            Language::Mt => "mt",
            Language::Bg => "bg",
            Language::Sk => "sk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLanguageError;

impl std::fmt::Display for ParseLanguageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid language code")
    }
}

impl std::error::Error for ParseLanguageError {}

impl std::str::FromStr for Language {
    type Err = ParseLanguageError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "es" => Ok(Language::Es),
            "sl" => Ok(Language::Sl),
            "en" => Ok(Language::En),
            "de" => Ok(Language::De),
            "hu" => Ok(Language::Hu),
            "ro" => Ok(Language::Ro),
            "pt" => Ok(Language::Pt),
            "sv" => Ok(Language::Sv),
            "da" => Ok(Language::Da),
            "fr" => Ok(Language::Fr),
            "pl" => Ok(Language::Pl),
            "et" => Ok(Language::Et),
            "nl" => Ok(Language::Nl),
            "el" => Ok(Language::El),
            "hr" => Ok(Language::Hr),
            "lv" => Ok(Language::Lv),
            "it" => Ok(Language::It),
            "fi" => Ok(Language::Fi),
            "cs" => Ok(Language::Cs),
            "lt" => Ok(Language::Lt),
            "ga" => Ok(Language::Ga),
            "mt" => Ok(Language::Mt),
            "bg" => Ok(Language::Bg),
            "sk" => Ok(Language::Sk),
            _ => Err(ParseLanguageError),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl AsRef<str> for Language {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
