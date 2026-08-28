use serde_derive::{Deserialize, Serialize};

pub static EU_SEATS_JSON: &str = include_str!("./seat_to_circle.json");

pub fn load_hemicycle() -> HemicycleLayout {
    serde_json::from_str(EU_SEATS_JSON).unwrap()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HemicycleLayout {
    pub source: String,
    pub numbering_rule: String,
    pub total_seats: u32,
    pub center_px: [f64; 2],
    pub frame_deg: FrameDeg,
    pub circles: Vec<Circle>,
    pub seats: Vec<Seat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Seat {
    pub seat: u32,
    pub circle: u32,
    pub index_in_circle: u32,
    pub slot_in_circle_with_gaps: u32,
    pub block_in_circle: u32,
    pub status: String,
    pub color: String,
    pub x_px: f64,
    pub y_px: f64,
    pub angle_deg: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameDeg {
    pub left: f64,
    pub right: f64,
    pub note: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Circle {
    pub circle: u32,
    pub first_seat: u32,
    pub last_seat: u32,
    pub visible_seats: u32,
    pub slots_including_gaps: u32,
    pub pattern: Vec<PatternEntry>,
    pub blocks: Vec<u32>,
    pub block_seat_ranges: Vec<[u32; 2]>,
    pub radius_px: f64,
    pub seat_pitch_deg: f64,
}

/// Pattern entries are either a seat-count (number) or a gap
/// description like "(1 skip)" (string).
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PatternEntry {
    SeatCount(u32),
    GapNote(String),
}
