//! Built-in 2-minute 스피드 리딩 scripts for when Claude can't write the day's one: two per
//! curriculum week, on its theme and grammar (`assets/speed_scripts.json`).

use crate::claude::SpeedPassage;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Script {
    pub week: u32,
    pub variant: u32,
    pub title: String,
    pub title_ko: String,
    pub script: String,
    pub script_ko: String,
}

pub fn all() -> &'static [Script] {
    static DATA: OnceLock<Vec<Script>> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../assets/speed_scripts.json")).unwrap_or_default())
}

/// Week of a program day (7 days a week, 26 weeks).
pub fn week_of(day: u32) -> u32 {
    ((day.max(1) - 1) / 7 + 1).min(26)
}

/// The day's built-in script: its week's, alternating between the two.
pub fn for_day(day: u32) -> Option<SpeedPassage> {
    let week = week_of(day);
    let mine: Vec<&Script> = all().iter().filter(|s| s.week == week).collect();
    let s = mine.iter().find(|s| s.variant == day % 2).or(mine.first())?;
    Some(SpeedPassage { title: s.title.clone(), passage: s.script.clone(), passage_ko: s.script_ko.clone(), chunks: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_minute_scripts_for_every_week() {
        for week in 1..=26 {
            for variant in 0..2 {
                let s = all().iter().find(|s| s.week == week && s.variant == variant).unwrap_or_else(|| panic!("week {week} variant {variant}"));
                let n = s.script.split_whitespace().count();
                assert!((200..=280).contains(&n), "week {week}/{variant}: {n} words");
                assert!(!s.title.is_empty() && !s.title_ko.is_empty() && !s.script_ko.is_empty());
            }
        }
        assert_eq!(week_of(1), 1);
        assert_eq!(week_of(8), 2);
        assert_eq!(week_of(182), 26);
        assert_ne!(for_day(1).unwrap().title, for_day(2).unwrap().title, "days of a week alternate");
    }
}
