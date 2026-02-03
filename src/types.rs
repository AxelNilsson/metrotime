//! Data types for the Stockholm metro API

use serde::Deserialize;

/// API response containing departures
#[derive(Debug, Deserialize)]
pub struct ApiResponse<'a> {
    #[serde(borrow)]
    pub departures: heapless::Vec<Departure<'a>, 10>,
}

/// Individual metro departure information
#[derive(Debug, Deserialize)]
pub struct Departure<'a> {
    pub destination: &'a str,
    pub display: &'a str,
    pub state: &'a str,
    pub scheduled: &'a str,
    pub expected: &'a str,
    #[serde(borrow)]
    pub stop_point: StopPoint<'a>,
    #[serde(borrow)]
    pub line: Line<'a>,
}

/// Stop point information
#[derive(Debug, Deserialize)]
pub struct StopPoint<'a> {
    pub name: &'a str,
    pub designation: &'a str,
}

/// Line information
#[derive(Debug, Deserialize)]
pub struct Line<'a> {
    pub designation: &'a str,
    pub group_of_lines: &'a str,
}
