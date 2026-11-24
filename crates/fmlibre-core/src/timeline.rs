//! How the players of a career change over time: a record of their abilities, value and attributes, taken from the
//! lists the editor reads anyway, every week or so of the game's time. The game keeps only what a player is now; the
//! records are what a profile draws his development from.
//!
//! The first record holds every player whole. Each later one holds only what changed since the one before, for the
//! players that changed, and the new players whole: most values stay as they are from one week to the next. A record
//! is one line of JSON, compressed on its own (a gzip member), so that a file of them is added to at its end and never
//! written whole (see [`Timeline::record`] and [`Timeline::read`]).
//!
//! A date that is earlier than the last record's means the game has loaded an older save of the career: the record
//! taken then holds every player whole again, and the records dated on or after it are left out from then on, as they
//! belong to days the career is yet to play again.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};

use flate2::Compression;
use flate2::bufread::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};

use crate::date::FmDate;
use crate::details::ATTRIBUTE_GROUPS;
use crate::filter::attribute_keys;
use crate::players::PlayerSummary;

/// The version of a record's line. A line of another version is skipped.
const VERSION: u8 = 1;

/// How many days of the game's time pass between two records, at least.
pub const RECORD_EVERY_DAYS: i64 = 7;

/// The fields a record holds besides the attributes, first in [`timeline_fields`].
pub const CURRENT_ABILITY: &str = "current_ability";
pub const POTENTIAL_ABILITY: &str = "potential_ability";
pub const VALUE: &str = "value";

/// What a record holds of a player, in the order of [`timeline_values`]: his abilities, his transfer value in pounds,
/// and his attributes as FM shows them (1–20).
pub fn timeline_fields() -> Vec<String> {
    [CURRENT_ABILITY, POTENTIAL_ABILITY, VALUE]
        .into_iter()
        .chain(attribute_keys(ATTRIBUTE_GROUPS))
        .map(str::to_owned)
        .collect()
}

/// The values of `player` for [`timeline_fields`]; none for what the layout does not place.
pub fn timeline_values(player: &PlayerSummary) -> Vec<Option<i32>> {
    let abilities = [Some(i32::from(player.current_ability)), Some(i32::from(player.potential_ability))];
    let attributes = player.attributes.iter().map(|&value| (value > 0).then_some(i32::from(value)));
    abilities.into_iter().chain([player.transfer_value]).chain(attributes).collect()
}

/// One record as its line has it.
#[derive(Serialize, Deserialize)]
struct Line {
    v: u8,
    date: FmDate,
    /// When it was taken, in milliseconds since 1970.
    at: u64,
    /// Whether it holds every player whole, rather than what changed since the record before.
    full: bool,
    /// The fields the indices below stand for: a line says so itself, so that a later version of the app, with other
    /// fields or another order, still reads it.
    fields: Vec<String>,
    /// For each player it holds, his UID and pairs of a field's index and its value, one after the other.
    players: Vec<(u32, Vec<i32>)>,
}

/// A value a record holds: of the record `record` on, the field `field` of the player is `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    record: u32,
    field: u8,
    value: i32,
}

/// The records of one career, as read from its file and added to since.
#[derive(Debug, Default)]
pub struct Timeline {
    fields: Vec<String>,
    /// The dates of the records, each later than the one before.
    records: Vec<FmDate>,
    /// What the records hold of each player, in their order.
    players: HashMap<u32, Vec<Point>>,
}

/// How a player's values went, for his profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// The dates of the records since the first that holds him, the oldest first, and the game's date as the last:
    /// what he is now, as the lists have him.
    pub dates: Vec<FmDate>,
    /// For each field (see [`timeline_fields`]) that has a value, its value on each of the dates; none before the
    /// first record that holds one.
    pub series: BTreeMap<String, Vec<Option<i32>>>,
}

/// What a player's current ability was at the record to compare him with (see [`Timeline::earlier`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Earlier {
    pub uid: u32,
    pub date: FmDate,
    pub current_ability: i32,
}

impl Timeline {
    pub fn new() -> Self {
        Self { fields: timeline_fields(), records: Vec::new(), players: HashMap::new() }
    }

    /// The records in `bytes`, the contents of a file, and how many of its bytes they take: what follows is a record
    /// cut short, by a crash say, which the next record is to be written over.
    pub fn read(bytes: &[u8]) -> (Self, usize) {
        let mut timeline = Self::new();
        let mut rest = bytes;
        while !rest.is_empty() {
            let mut decoder = GzDecoder::new(rest);
            let mut text = String::new();
            if decoder.read_to_string(&mut text).is_err() {
                break;
            }
            rest = decoder.into_inner();
            // A line that does not read is skipped; so is one of a version this one does not know.
            if let Ok(line) = serde_json::from_str::<Line>(&text) {
                timeline.take(&line);
            }
        }
        (timeline, bytes.len() - rest.len())
    }

    /// How many records there are, and the dates of the first and the last.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn first_date(&self) -> Option<FmDate> {
        self.records.first().copied()
    }

    pub fn last_date(&self) -> Option<FmDate> {
        self.records.last().copied()
    }

    /// Takes the record of the game's date `date`, when one is due, from the players as the lists have them (their
    /// UIDs and [`timeline_values`]), and gives the bytes to add to the end of the file. One is due when there is
    /// none yet, when [`RECORD_EVERY_DAYS`] have passed since the last, and when `date` is before the last: the game
    /// has gone back to an older save, and the records of the days after it are dropped. None without players.
    pub fn record(
        &mut self,
        date: FmDate,
        at: u64,
        players: impl IntoIterator<Item = (u32, Vec<Option<i32>>)>,
    ) -> Option<Vec<u8>> {
        let full = match self.last_date() {
            None => true,
            Some(last) if date < last => true,
            Some(last) if date >= last.plus_days(RECORD_EVERY_DAYS) => false,
            Some(_) => return None,
        };
        let mut known = vec![None; self.fields.len()];
        let mut changed = Vec::new();
        for (uid, values) in players {
            known.fill(None);
            if !full && let Some(points) = self.players.get(&uid) {
                for point in points {
                    known[usize::from(point.field)] = Some(point.value);
                }
            }
            let mut pairs = Vec::new();
            for (field, value) in values.into_iter().enumerate().take(known.len()) {
                if let Some(value) = value
                    && known[field] != Some(value)
                {
                    pairs.extend([field as i32, value]);
                }
            }
            if !pairs.is_empty() {
                changed.push((uid, pairs));
            }
        }
        if full && changed.is_empty() {
            return None;
        }
        let line = Line { v: VERSION, date, at, full, fields: self.fields.clone(), players: changed };
        self.take(&line);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        // The line ends as a line does, so that the file, taken out of its compression, is one of lines.
        let mut text = serde_json::to_string(&line).ok()?;
        text.push('\n');
        encoder.write_all(text.as_bytes()).ok()?;
        encoder.finish().ok()
    }

    /// Adds what `line` holds. A whole record drops the records dated on or after it first. One of what changed counts
    /// when it is later than the last record, or of the same date (two copies of the app took it at once); an earlier
    /// one has nothing to be read against, and is skipped.
    fn take(&mut self, line: &Line) {
        if line.v != VERSION {
            return;
        }
        if line.full {
            self.drop_from(line.date);
        }
        let record = match self.records.last() {
            Some(&last) if line.date == last => self.records.len() - 1,
            Some(&last) if line.date < last => return,
            None if !line.full => return,
            _ => {
                self.records.push(line.date);
                self.records.len() - 1
            }
        };
        let fields: Vec<Option<u8>> = line
            .fields
            .iter()
            .map(|name| self.fields.iter().position(|field| field == name).and_then(|at| u8::try_from(at).ok()))
            .collect();
        for (uid, pairs) in &line.players {
            let points = self.players.entry(*uid).or_default();
            for &[at, value] in pairs.as_chunks::<2>().0 {
                let field = usize::try_from(at).ok().and_then(|at| fields.get(at).copied().flatten());
                if let Some(field) = field {
                    points.push(Point { record: record as u32, field, value });
                }
            }
        }
    }

    /// Drops the records dated `date` or later, and what they hold.
    fn drop_from(&mut self, date: FmDate) {
        let keep = self.records.partition_point(|&record| record < date);
        if keep == self.records.len() {
            return;
        }
        self.records.truncate(keep);
        self.players.retain(|_, points| {
            points.retain(|point| (point.record as usize) < keep);
            !points.is_empty()
        });
    }

    /// The value of the field `field` of the points `points` at the record `record`: the last one given up to it.
    fn value_at(points: &[Point], field: usize, record: usize) -> Option<i32> {
        points
            .iter()
            .take_while(|point| point.record as usize <= record)
            .filter(|point| usize::from(point.field) == field)
            .last()
            .map(|point| point.value)
    }

    /// How the values of the player `uid` went: the records that hold him, and what he is on `today`, the game's date,
    /// `now` (see [`timeline_values`]). A record of today's date gives way to what he is now: he may have been edited
    /// since it was taken.
    pub fn progress(&self, uid: u32, today: FmDate, now: &[Option<i32>]) -> Progress {
        let points = self.players.get(&uid).map_or(&[][..], Vec::as_slice);
        let records = match points.first() {
            Some(first) => {
                let end = self.records.partition_point(|&record| record < today);
                first.record as usize..end.max(first.record as usize)
            }
            None => 0..0,
        };
        let mut dates: Vec<FmDate> = self.records[records.clone()].to_vec();
        dates.push(today);
        let mut series = BTreeMap::new();
        for (field, name) in self.fields.iter().enumerate() {
            let mut values = Vec::with_capacity(dates.len());
            let mut value = None;
            let mut next = points.iter().filter(|point| usize::from(point.field) == field).peekable();
            for record in records.clone() {
                while let Some(point) = next.next_if(|point| point.record as usize <= record) {
                    value = Some(point.value);
                }
                values.push(value);
            }
            values.push(now.get(field).copied().flatten());
            if values.iter().any(Option::is_some) {
                series.insert(name.clone(), values);
            }
        }
        Progress { dates, series }
    }

    /// For each of the players `uids` that a record before `today` holds, his current ability at the record to compare
    /// him with: the oldest of the last `days` days, or the last one where all are older.
    pub fn earlier(&self, uids: &[u32], today: FmDate, days: i64) -> Vec<Earlier> {
        let before = self.records.partition_point(|&record| record < today);
        if before == 0 {
            return Vec::new();
        }
        let from = today.plus_days(-days);
        let oldest = self.records.partition_point(|&record| record < from).min(before - 1);
        let field = self.fields.iter().position(|field| field == CURRENT_ABILITY).unwrap_or(0);
        uids.iter()
            .filter_map(|&uid| {
                let points = self.players.get(&uid)?;
                let record = oldest.max(points.first()?.record as usize);
                (record < before).then_some(())?;
                let current_ability = Self::value_at(points, field, record)?;
                Some(Earlier { uid, date: self.records[record], current_ability })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: u16, month: u8, day: u8) -> FmDate {
        FmDate { year, month, day }
    }

    /// A player's values: his abilities, his value, and two attributes.
    fn values(ca: i32, pa: i32, value: i32, corners: i32, crossing: i32) -> Vec<Option<i32>> {
        vec![Some(ca), Some(pa), Some(value), Some(corners), Some(crossing)]
    }

    fn series(progress: &Progress, field: &str) -> Vec<Option<i32>> {
        progress.series.get(field).cloned().unwrap_or_default()
    }

    #[test]
    fn fields_follow_the_summaries_attributes() {
        let fields = timeline_fields();
        assert_eq!(&fields[..4], ["current_ability", "potential_ability", "value", "corners"]);
        assert!(fields.len() < usize::from(u8::MAX));
    }

    #[test]
    fn records_what_changed_and_reads_it_back() {
        let mut timeline = Timeline::new();
        let mut file = Vec::new();
        let first = timeline.record(
            date(2026, 7, 1),
            1,
            [(7, values(120, 160, 5_000, 10, 12)), (9, values(90, 91, 100, 8, 8))],
        );
        file.extend(first.expect("the first record"));
        // Within a week of the last record none is due.
        assert!(timeline.record(date(2026, 7, 7), 2, [(7, values(121, 160, 5_000, 10, 12))]).is_none());
        // A week on: only what changed, and a new player whole. Player 9 is the same, and is left out.
        let second = timeline.record(
            date(2026, 7, 8),
            3,
            [(7, values(122, 160, 6_000, 10, 13)), (9, values(90, 91, 100, 8, 8)), (11, values(60, 150, 0, 5, 5))],
        );
        file.extend(second.expect("the second record"));
        assert_eq!(timeline.len(), 2);

        let (read, taken) = Timeline::read(&file);
        assert_eq!(taken, file.len());
        assert_eq!(read.len(), 2);
        assert_eq!((read.first_date(), read.last_date()), (Some(date(2026, 7, 1)), Some(date(2026, 7, 8))));
        for timeline in [&timeline, &read] {
            let progress = timeline.progress(7, date(2026, 7, 10), &values(123, 160, 6_000, 10, 13));
            assert_eq!(progress.dates, [date(2026, 7, 1), date(2026, 7, 8), date(2026, 7, 10)]);
            assert_eq!(series(&progress, "current_ability"), [Some(120), Some(122), Some(123)]);
            assert_eq!(series(&progress, "value"), [Some(5_000), Some(6_000), Some(6_000)]);
            assert_eq!(series(&progress, "corners"), [Some(10), Some(10), Some(10)]);
            assert_eq!(series(&progress, "crossing"), [Some(12), Some(13), Some(13)]);
            // An attribute no record holds has no series.
            assert!(!progress.series.contains_key("dribbling"));
            // The new player's begins with the record that first holds him.
            let newcomer = timeline.progress(11, date(2026, 7, 10), &values(60, 150, 0, 5, 5));
            assert_eq!(newcomer.dates, [date(2026, 7, 8), date(2026, 7, 10)]);
            // One no record holds has only what he is now.
            let unknown = timeline.progress(99, date(2026, 7, 10), &values(1, 2, 3, 4, 5));
            assert_eq!(unknown.dates, [date(2026, 7, 10)]);
            assert_eq!(series(&unknown, "current_ability"), [Some(1)]);
        }
    }

    #[test]
    fn todays_record_gives_way_to_what_he_is_now() {
        let mut timeline = Timeline::new();
        timeline.record(date(2026, 7, 1), 1, [(7, values(120, 160, 5_000, 10, 12))]);
        let progress = timeline.progress(7, date(2026, 7, 1), &values(200, 200, 5_000, 20, 20));
        assert_eq!(progress.dates, [date(2026, 7, 1)]);
        assert_eq!(series(&progress, "current_ability"), [Some(200)]);
    }

    #[test]
    fn an_older_save_drops_the_days_after_it() {
        let mut timeline = Timeline::new();
        let mut file = Vec::new();
        for (day, ca) in [(1, 100), (8, 101), (15, 102)] {
            file.extend(timeline.record(date(2026, 7, day), 0, [(7, values(ca, 160, 0, 10, 10))]).expect("due"));
        }
        // The game went back to 10 July: the record is whole, and the one of 15 July is gone.
        file.extend(timeline.record(date(2026, 7, 10), 0, [(7, values(105, 160, 0, 10, 10))]).expect("due"));
        let (read, _) = Timeline::read(&file);
        for timeline in [&timeline, &read] {
            assert_eq!(timeline.len(), 3);
            let progress = timeline.progress(7, date(2026, 7, 12), &values(105, 160, 0, 10, 10));
            assert_eq!(progress.dates, [date(2026, 7, 1), date(2026, 7, 8), date(2026, 7, 10), date(2026, 7, 12)]);
            assert_eq!(series(&progress, "current_ability"), [Some(100), Some(101), Some(105), Some(105)]);
        }
        // The same day again: nothing is due.
        assert!(timeline.record(date(2026, 7, 10), 0, [(7, values(105, 160, 0, 10, 10))]).is_none());
    }

    #[test]
    fn a_record_cut_short_is_left_out() {
        let mut timeline = Timeline::new();
        let mut file = timeline.record(date(2026, 7, 1), 0, [(7, values(100, 160, 0, 10, 10))]).expect("due");
        let whole = file.len();
        let second = timeline.record(date(2026, 7, 8), 0, [(7, values(101, 160, 0, 10, 10))]).expect("due");
        file.extend(&second[..second.len() / 2]);
        let (read, taken) = Timeline::read(&file);
        assert_eq!((read.len(), taken), (1, whole));
        assert!(Timeline::read(b"not a record").0.is_empty());
    }

    #[test]
    fn compares_with_the_oldest_record_of_the_last_days() {
        let mut timeline = Timeline::new();
        timeline.record(date(2025, 1, 1), 0, [(7, values(90, 160, 0, 10, 10))]);
        timeline.record(date(2026, 3, 1), 0, [(7, values(100, 160, 0, 10, 10)), (9, values(50, 60, 0, 1, 1))]);
        timeline.record(date(2026, 6, 1), 0, [(7, values(110, 160, 0, 10, 10)), (9, values(50, 60, 0, 1, 1))]);
        // Within the last year the oldest record is the one of March; player 11 is in none.
        let earlier = timeline.earlier(&[7, 9, 11], date(2026, 7, 1), 365);
        assert_eq!(
            earlier,
            [
                Earlier { uid: 7, date: date(2026, 3, 1), current_ability: 100 },
                Earlier { uid: 9, date: date(2026, 3, 1), current_ability: 50 },
            ]
        );
        // All older than the days asked for: the last one.
        assert_eq!(timeline.earlier(&[7], date(2026, 7, 1), 10)[0].date, date(2026, 6, 1));
        // A record of today is not one to compare with.
        assert_eq!(timeline.earlier(&[7], date(2026, 6, 1), 365)[0].date, date(2026, 3, 1));
        assert!(timeline.earlier(&[7], date(2025, 1, 1), 365).is_empty());
    }
}
