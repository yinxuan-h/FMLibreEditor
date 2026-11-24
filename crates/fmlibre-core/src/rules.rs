//! The rules a competition's squads are registered under: how many foreign players a squad may hold, how many
//! goalkeepers, how many players in all (docs/recon/fm27-league-rules.md).
//!
//! The game reads a nation's rules from its rule files into records (see `records.rs`) and keeps them in the container
//! of the nation's men's teams: the record the files make together, and the records of the files themselves. From
//! those it makes, whenever it loads a save, the lists it plays by: a competition as it is played
//! (`[rules] competition_class`) points at its rule group, the rule group at its nation's rules, and those hold one
//! set of fixture rules for each kind of fixture the nation plays (a division's league, the cup). A competition's own
//! record names its set by its place. A set holds this season's list of squad rules, small structs of a kind, a value
//! and which way the value limits, and points at the set of the next season with a list of its own.
//!
//! So a rule is in four places, and the editor writes a change of it to two of them: this season's value to this
//! season's list and to the record the files make together, from which the game makes that list again when it loads
//! the save; next season's value to next season's list and to the rule file's own record, from which it makes that
//! one. The game's screens count a squad against the lists (its Registration screen, its Competition Stages screen),
//! and show a change of this season's once a day has passed. A save keeps the records, not the lists.
//!
//! A record holds a rule for the years it is good for, so the rule a season's list was made from is the one of its
//! kind, limit and condition that is good in that season, looked for in the order of the list. A rule of the list
//! that the record does not hold as such (the squad's size where the files give it with the dates squad numbers are
//! handed out by) is shown and left alone.
//!
//! Nothing the editor reads otherwise leads to the competitions as they are played: they are found by their class, in
//! one pass over the game's memory. The names of the kinds of rule are the game's own, read from its table of them.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use fmlibre_mem::{Module, Process, is_user_pointer};
use serde::Serialize;

use crate::coded::Coded;
use crate::date::FmDate;
use crate::prize_money::Competition;
use crate::profile::RulesLayout;
use crate::records::{Records, Traits, Typed, key, key_text};
use crate::rtti;
use crate::session::Session;
use crate::transfer::{MoveError, Patches};

/// The most the editor writes as a rule's value, and the most a value may be for the editor to change it: the game's
/// own counts of players stay below 100 (a squad of at most 99 is one with no limit), while the same lists hold
/// wages and fees in their own rules, which are no counts.
pub const MOST_RULE_VALUE: i32 = 255;

/// The most squad rules a list is taken to have: the fullest seen, a cup's, has 14.
const MOST_RULES: u64 = 256;

/// The most sets of fixture rules a nation is taken to have: Türkiye has 10.
const MOST_FIXTURE_RULES: u64 = 256;

/// The most places a set's table of the days squads are registered by is taken to have: UEFA's has 74.
const MOST_REGISTRATIONS: u64 = 1024;

/// The key that marks an entry of that table.
const REGISTRATION: u32 = key(b"SqSl");

/// The game's name for the kind of rule that limits a squad's size, which the days squads are registered by repeat.
const SQUAD_SIZE: &str = "max_squad_size";

/// The most entries the game's table of kinds is read for, each way from the one it is found by: it has 465.
const MOST_KINDS: u64 = 4096;

/// The longest name of a kind that is read: the longest of the game's has 76 characters.
const LONGEST_KIND: usize = 128;

// The keys of the records the editor reads.
const FIXTURE_INDEX: u32 = key(b"fxri");
const LEVEL: u32 = key(b"levl");
const NATION: u32 = key(b"nati");
const NATION_UID: u32 = key(b"Nnat");
const FIXTURE_RULES: u32 = key(b"fxrl");
const SQUAD_RULES: u32 = key(b"sqsr");
const KIND: u32 = key(b"type");
const COMPARE: u32 = key(b"cmty");
const VALUE: u32 = key(b"valu");
const START_YEAR: u32 = key(b"styr");
const END_YEAR: u32 = key(b"enyr");
const INFO: u32 = key(b"info");
const TEAMS_FROM: u32 = key(b"tfrd");
const COMPETITIONS: u32 = key(b"cmps");
const COMPETITION: u32 = key(b"comp");
const DATABASE_ID: u32 = key(b"DBID");
// The first and the last year a rule file is for, where a nation has more than one: the game gives these two keys no
// name.
const FILE_FROM: u32 = key(b"OvSY");
const FILE_TO: u32 = key(b"OvEY");
const UPDATE_DAY: u32 = key(b"updy");
const DAY: u32 = key(b"dyom");
const MONTH: u32 = key(b"mont");

/// Which season's rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Season {
    Current,
    Next,
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Season::Current => "this season",
            Season::Next => "next season",
        })
    }
}

/// A rule as the record a season's list is made from holds it: its value there, and the years it is good for, each
/// open where the record names none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleSource {
    pub value: i32,
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
}

/// One rule of a season's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SquadRule {
    /// The kind of rule, the game's own number for it, and the game's name for that kind
    /// (`all_foreign_players_in_squad`), where its table of kinds was read.
    pub kind: u16,
    pub name: Option<String>,
    /// Which way the value limits: -1 at most, 1 at least.
    pub compare: i8,
    pub value: i32,
    /// The key of the rule's condition, four characters (`1yol`, `tfrd`), and for one about the teams from a division
    /// the UID of that division's competition.
    pub condition: Option<String>,
    pub condition_competition: Option<u32>,
    /// The rule in the record the game makes this list from when it loads a save; none where the record holds no such
    /// rule, which the editor then leaves alone.
    pub source: Option<RuleSource>,
    /// Whether the game keeps the rule in no record at all: its competition is no nation's (UEFA's Champions League),
    /// and the game makes its rules itself whenever it loads a save. The editor changes such a rule in the list
    /// alone, and the change lasts until the game makes the list again.
    pub unsaved: bool,
    #[serde(skip)]
    place: RulePlace,
}

impl SquadRule {
    /// Whether the value is at most what the rule allows, rather than at least.
    pub fn at_most(&self) -> bool {
        self.compare < 0
    }

    /// Whether the editor changes the rule: a count of players, with its rule in the record a save keeps, or of a
    /// competition whose rules no record keeps.
    pub fn editable(&self) -> bool {
        (self.source.is_some() || self.unsaved) && (0..=MOST_RULE_VALUE).contains(&self.value)
    }

    /// Where the rule's struct is in the game's memory, and where the nation's rules hold its value (none where they
    /// hold no such rule): for the probe's `rules --where`.
    pub fn addresses(&self) -> (u64, Option<u64>) {
        (self.place.rule, self.place.source.map(|source| source.at))
    }
}

/// Where a rule is: the place in its list that points at it, its struct, the key of its condition as the struct holds
/// it, and its value in the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RulePlace {
    slot: u64,
    rule: u64,
    condition: u32,
    source: Option<Typed>,
}

/// The days a competition's squads are registered by: how many the game keeps for it, and the most players a squad
/// may register by each (none where they differ). The competition's page says it for each day: "Maximum of 25 players
/// to be registered on Friday 2nd July 2027".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Registration {
    pub days: usize,
    pub most: Option<u8>,
}

/// One of those days, as its entry holds it: where the entry is, its competition's UID and the most players.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegistrationDay {
    at: u64,
    competition: u32,
    most: u8,
}

/// A day of the year: the one a nation's seasons turn on (Türkiye's on 20 June).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SeasonTurn {
    pub month: u8,
    pub day: u8,
}

/// The rules a competition's squads are registered under, this season and the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompetitionRules {
    #[serde(flatten)]
    pub competition: Competition,
    /// The level of the division in its nation, 0 for the top one; none for a competition that says none.
    pub level: Option<i32>,
    /// Which of its nation's sets of fixture rules it is played under.
    pub fixture_rules: usize,
    /// The year this season began in, as the nation's rules count seasons; none where the in-game date is not known.
    pub season: Option<i32>,
    /// The day of the year the nation's seasons turn on, as its rules give it: next season's list is this season's
    /// from then on. None for a competition of no nation, or where the rules give none.
    pub season_turn: Option<SeasonTurn>,
    pub current: Vec<SquadRule>,
    /// None where the game holds no rules for the next season.
    pub next: Option<Vec<SquadRule>>,
    /// The other competitions played under the same set of fixture rules, by UID (the groups of a division, a
    /// nation's youth leagues): the lists are theirs too, and so is a change of a rule.
    pub shared_with: Vec<u32>,
    /// Whether the game keeps its rules in no record (see [`SquadRule::unsaved`]).
    pub unsaved: bool,
    /// The days its squads are registered by this season, where the game keeps such days for it (UEFA's): how many,
    /// and the most players a squad may register by each, where they all say the same.
    pub registration: Option<Registration>,
    /// Where that set of fixture rules is, and the one of the next season it points at.
    #[serde(skip)]
    set: u64,
    #[serde(skip)]
    next_set: Option<u64>,
    /// Where the competition is as it is played, its object in the database and its rule group.
    #[serde(skip)]
    played: [u64; 3],
}

/// Where a competition's rules are in the game's memory: the competition as it is played, its object in the database,
/// its rule group, its set of fixture rules and next season's (none where the game holds none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RulePlaces {
    pub competition: u64,
    pub db: u64,
    pub group: u64,
    pub set: u64,
    pub next_set: Option<u64>,
}

impl CompetitionRules {
    /// Where the competition's rules are in the game's memory: for the probe's `rules --where`.
    pub fn places(&self) -> RulePlaces {
        let [competition, db, group] = self.played;
        RulePlaces { competition, db, group, set: self.set, next_set: self.next_set }
    }

    /// The list of `season`, if the game holds one.
    pub fn rules(&self, season: Season) -> Option<&[SquadRule]> {
        match season {
            Season::Current => Some(&self.current),
            Season::Next => self.next.as_deref(),
        }
    }
}

/// One rule changed, or to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEdit {
    /// The competition's UID.
    pub competition: u32,
    pub season: Season,
    /// The rule's place in the season's list, from 0.
    pub rule: usize,
    pub kind: u16,
    pub from: i32,
    pub to: i32,
    /// What the record held for it; for a change of the list alone, what the list held.
    pub source_from: i32,
    /// Whether the list alone was changed: the rule has no place in the nation's rules, or the change left it be.
    pub list_only: bool,
}

/// Rules changed, or to be: what each changes from and to, and the bytes written, which the undo writes back.
#[derive(Debug, Clone)]
pub struct RuleChange {
    pub edits: Vec<RuleEdit>,
    patches: Vec<crate::transfer::Patch>,
    /// For each rule changed: what its list and its struct held when they were read.
    guards: Vec<Guard>,
}

impl RuleChange {
    /// Brings `found`, the rules as read before, up to date after this change was written, or undone: each rule's
    /// value, what the record holds for it, and the record's word as the next change will expect to find it. In every
    /// competition played under the same fixture rules, as the rule changed is theirs too.
    pub fn note(&self, found: &mut [CompetitionRules], undone: bool) {
        for edit in &self.edits {
            let named = found.iter().find(|rules| rules.competition.uid == edit.competition);
            let rule = named.and_then(|rules| rules.rules(edit.season)).and_then(|list| list.get(edit.rule));
            let Some(place) = rule.filter(|rule| rule.kind == edit.kind).map(|rule| rule.place) else {
                continue;
            };
            // Written, the list and the record both hold the new value; undone, each what it held before.
            let (listed, recorded) = if undone { (edit.from, edit.source_from) } else { (edit.to, edit.to) };
            let lists =
                found.iter_mut().flat_map(|rules| rules.current.iter_mut().chain(rules.next.iter_mut().flatten()));
            for rule in lists {
                if rule.place.rule == place.rule {
                    rule.value = listed;
                }
                if edit.list_only {
                    continue;
                }
                let (Some(source), Some(changed)) = (&mut rule.place.source, place.source) else {
                    continue;
                };
                if source.at == changed.at {
                    source.raw = (source.raw & !0xFFFF_FFFF) | u64::from(recorded as u32);
                    if let Some(shown) = &mut rule.source {
                        shown.value = recorded;
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Guard {
    slot: u64,
    rule: u64,
    kind: u16,
    compare: i8,
    condition: u32,
}

/// Why the app shows no squad rules on this build of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleUnavailable {
    /// Nobody has read the squad rules on this build and matched them to the game's screens yet.
    Build,
    /// The build was checked, but what the editor found does not read as the game's.
    Unreadable,
}

impl fmt::Display for RuleUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RuleUnavailable::Build => "the squad rules have not been checked on this build of the game",
            RuleUnavailable::Unreadable => "the squad rules do not read as the game's on this build",
        })
    }
}

/// Why the squad rules could not be read for the app.
#[derive(Debug)]
pub enum RuleReadError {
    Unavailable(RuleUnavailable),
    Memory(fmlibre_mem::Error),
}

impl fmt::Display for RuleReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleReadError::Unavailable(reason) => reason.fmt(f),
            RuleReadError::Memory(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for RuleReadError {}

/// The least share of the rules read that must be of a kind the game's table names for them to be believed: every one
/// was on the save they were read on. Offsets that read the wrong bytes would find no rules, or kinds of no name.
const LEAST_NAMED: f64 = 0.9;

/// Whether rules read as the game's: some competitions have some, and nearly all of them are of kinds the game names.
fn read_as_the_games(found: &[CompetitionRules]) -> bool {
    let rules = || found.iter().flat_map(|rules| rules.current.iter().chain(rules.next.iter().flatten()));
    let (all, named) = (rules().count(), rules().filter(|rule| rule.name.is_some()).count());
    all > 0 && named as f64 >= LEAST_NAMED * all as f64
}

#[derive(Debug)]
pub enum RuleError {
    /// The layout does not place the competitions' rules.
    NotInLayout,
    /// The game names no class of this name: another build may call it otherwise.
    NoClass {
        class: String,
    },
    /// The game holds no rules for the competition's next season.
    NoSeason,
    /// The season's list has no rule at this place.
    NoRule {
        rule: usize,
        rules: usize,
    },
    /// The record a save keeps holds no such rule, so a change would last only until the game loads a save.
    NoSource {
        kind: u16,
    },
    /// The rule's value is no count of players (a wage, a fee): the editor leaves it alone.
    NotACount {
        value: i32,
    },
    /// Less than nothing, or more than the editor writes as a rule's value.
    OutOfRange {
        value: i32,
    },
    /// The list, or the rule in it, is no longer what it was: the game has moved on.
    Gone {
        rule: u64,
    },
    /// A write did not land, or memory no longer holds what was read.
    Write(MoveError),
    Memory(fmlibre_mem::Error),
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleError::NotInLayout => f.write_str("the layout does not place the competitions' rules"),
            RuleError::NoClass { class } => write!(f, "the game names no class {class}"),
            RuleError::NoSeason => f.write_str("the game holds no rules for the competition's next season"),
            RuleError::NoRule { rule, rules } => {
                write!(f, "no rule {}: the season's list has {rules}", rule + 1)
            }
            RuleError::NoSource { kind } => write!(
                f,
                "the nation's rules as a save keeps them hold no rule of kind {kind} for the season, so a change \
                 would not last"
            ),
            RuleError::NotACount { value } => write!(f, "the rule's value, {value}, is no count of players"),
            RuleError::OutOfRange { value } => {
                write!(f, "{value} is not between 0 and the {MOST_RULE_VALUE} the editor writes")
            }
            RuleError::Gone { rule } => write!(f, "the rule at {rule:#x} is not what it was"),
            RuleError::Write(err) => err.fmt(f),
            RuleError::Memory(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for RuleError {}

impl Coded for RuleError {
    fn code(&self) -> &'static str {
        match self {
            RuleError::NotInLayout => "rule.notInLayout",
            RuleError::NoClass { .. } => "rule.noClass",
            RuleError::NoSeason => "rule.noSeason",
            RuleError::NoRule { .. } => "rule.noRule",
            RuleError::NoSource { .. } => "rule.noSource",
            RuleError::NotACount { .. } => "rule.notACount",
            RuleError::OutOfRange { .. } => "rule.outOfRange",
            RuleError::Gone { .. } => "rule.gone",
            RuleError::Write(err) => err.code(),
            RuleError::Memory(err) => err.code(),
        }
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            RuleError::NoClass { class } => vec![("class", class.clone())],
            RuleError::NoRule { rule, rules } => vec![("rule", (rule + 1).to_string()), ("rules", rules.to_string())],
            RuleError::NoSource { kind } => vec![("kind", kind.to_string())],
            RuleError::NotACount { value } => vec![("value", value.to_string())],
            RuleError::OutOfRange { value } => {
                vec![("value", value.to_string()), ("most", MOST_RULE_VALUE.to_string())]
            }
            RuleError::Gone { rule } => vec![("address", format!("{rule:#x}"))],
            RuleError::Write(err) => err.params(),
            RuleError::Memory(err) => err.params(),
            RuleError::NotInLayout | RuleError::NoSeason => Vec::new(),
        }
    }
}

impl From<fmlibre_mem::Error> for RuleError {
    fn from(err: fmlibre_mem::Error) -> Self {
        RuleError::Memory(err)
    }
}

impl From<MoveError> for RuleError {
    fn from(err: MoveError) -> Self {
        RuleError::Write(err)
    }
}

/// A rule of a season's list, as its struct holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Listed {
    slot: u64,
    rule: u64,
    condition: u32,
    competition: u32,
    value: i32,
    kind: u16,
    compare: i8,
}

/// A rule as a record holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Recorded {
    kind: u16,
    /// None where the record says neither way.
    compare: Option<i8>,
    value: Typed,
    from_year: Option<i32>,
    to_year: Option<i32>,
    /// The key of its condition, 0 for none, and the competition a condition about a division's teams names.
    condition: u32,
    competition: Option<u32>,
}

impl Recorded {
    fn good_in(&self, year: i32) -> bool {
        self.from_year.is_none_or(|from| from <= year) && self.to_year.is_none_or(|to| year <= to)
    }

    fn is(&self, listed: &Listed) -> bool {
        self.kind == listed.kind
            && self.compare.is_none_or(|compare| compare == listed.compare)
            && self.condition == listed.condition
            && (listed.condition != TEAMS_FROM || self.competition == Some(listed.competition))
    }
}

/// What one of a nation's records holds for a competition's set of fixture rules (see [`Session::rule_records`]).
#[derive(Debug, Clone)]
pub struct RuleRecord {
    /// Where the record is.
    pub record: u64,
    /// Whether it is in the container of the nation's women's teams, not of its men's.
    pub women: bool,
    /// Whether it is the record the nation's files make together, not one of the files' own.
    pub together: bool,
    /// Whether it is the one of the files' own that the editor takes next season's rules from.
    pub chosen: bool,
    /// The first and the last season it is for, by the year a season begins in, where it says.
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// How many sets of fixture rules it holds; none where it holds no list of them.
    pub sets: Option<usize>,
    /// The squad rules of the competition's set, in the record's order; empty where it has no such set.
    pub rules: Vec<RecordedRule>,
}

/// A squad rule as a record holds it.
#[derive(Debug, Clone)]
pub struct RecordedRule {
    pub kind: u16,
    /// Which way the value limits; none where the record says neither.
    pub compare: Option<i8>,
    /// The value, and where the record keeps it.
    pub value: Option<i64>,
    pub value_at: u64,
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// The key of its condition, where it has one.
    pub condition: Option<String>,
}

impl RecordedRule {
    fn of(rule: Recorded) -> Self {
        Self {
            kind: rule.kind,
            compare: rule.compare,
            value: rule.value.number(),
            value_at: rule.value.at,
            from_year: rule.from_year,
            to_year: rule.to_year,
            condition: (rule.condition != 0)
                .then(|| key_text(rule.condition).unwrap_or_else(|| format!("{:#x}", rule.condition))),
        }
    }
}

/// For each rule of a season's list, the rule of the record it was made from: the next one, in the record's order,
/// that is good in `year` and of the same kind, limit and condition. None for a rule the record does not hold as such.
fn sources_of(listed: &[Listed], recorded: &[Recorded], year: i32) -> Vec<Option<usize>> {
    let mut from = 0;
    listed
        .iter()
        .map(|rule| {
            let found = (from..recorded.len()).find(|&at| recorded[at].good_in(year) && recorded[at].is(rule))?;
            from = found + 1;
            Some(found)
        })
        .collect()
}

/// The year the season `today` falls in began in, for a nation whose seasons turn on `turn` (month, day): rules good
/// from a year are good from the season that begins in it.
fn season_year(today: FmDate, turn: Option<(u8, u8)>) -> i32 {
    let year = i32::from(today.year);
    let Some(turn) = turn else {
        return year;
    };
    let turned = if (today.month, today.day) < turn { year - 1 } else { year };
    // A season that turns at the end of a year is the next year's: Korea's turn on 31 December, Sweden's on the
    // 26th, and the season played from spring to autumn after that is the one the rules count by.
    if turn.0 >= LATE_TURN { turned + 1 } else { turned }
}

/// The first month of the year from which a season that turns in it is counted as the next year's. The nations read
/// turn their seasons from 1 January to 1 August, or from 25 to 31 December.
const LATE_TURN: u8 = 10;

/// Has each competition of `found` name the others played under its set of fixture rules, in the order they were
/// found in.
fn share_sets(found: &mut [CompetitionRules]) {
    let mut sets: HashMap<u64, Vec<u32>> = HashMap::new();
    for rules in found.iter() {
        sets.entry(rules.set).or_default().push(rules.competition.uid);
    }
    for rules in found.iter_mut() {
        let own = rules.competition.uid;
        rules.shared_with = sets[&rules.set].iter().copied().filter(|&uid| uid != own).collect();
    }
}

/// What a nation's records hold for its fixture rules: the competitions they list by UID, the day its seasons turn,
/// and the record of each set of fixture rules, in the record the files make together and in each rule file's own.
#[derive(Debug, Clone, Default)]
struct NationRecords {
    competitions: Vec<u32>,
    turn: Option<(u8, u8)>,
    together: Vec<u64>,
    files: Vec<RuleFile>,
}

impl NationRecords {
    /// The sets of fixture rules of the rule file that is for the season beginning in `year`: the game makes that
    /// season's list from it. The first file whose years hold the season; the first file where no year is known; none
    /// where no file is for the season, and its rules are then left alone.
    fn file_sets(&self, year: Option<i32>) -> &[u64] {
        let file = match year {
            Some(year) => self.files.iter().find(|file| file.good_in(year)),
            None => self.files.first(),
        };
        file.map_or(&[], |file| file.sets.as_slice())
    }
}

/// The record of one of a nation's rule files that holds fixture rules: the seasons it is for, by the year they begin
/// in, each end open where the record names none, and its sets of fixture rules. Most nations have one file for every
/// season; England has one for the seasons to 2027 and another from 2028.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RuleFile {
    record: u64,
    from_year: Option<i32>,
    to_year: Option<i32>,
    sets: Vec<u64>,
}

impl RuleFile {
    fn good_in(&self, year: i32) -> bool {
        self.from_year.is_none_or(|from| from <= year) && self.to_year.is_none_or(|to| year <= to)
    }
}

impl Session {
    /// Every competition the game is playing whose nation's rules the editor can read, with the squad rules of this
    /// season and of the next; those with no squad rule at all among them. One pass over the game's memory, which
    /// takes a few seconds.
    pub fn squad_rules(&self) -> Result<Vec<CompetitionRules>, RuleError> {
        let layout = self.layout.rules.as_ref().ok_or(RuleError::NotInLayout)?;
        let competition = self.layout.competition.as_ref().ok_or(RuleError::NotInLayout)?;
        let national = self.layout.national.as_ref().ok_or(RuleError::NotInLayout)?;
        let nation_at = competition.nation.ok_or(RuleError::NotInLayout)?;
        let vtables = rtti::vtables(&self.process, &self.plugin, &layout.competition_class);
        if vtables.is_empty() {
            return Err(RuleError::NoClass { class: layout.competition_class.clone() });
        }
        let objects = rtti::objects(&self.process, &self.plugin, &vtables)?;
        let mut played: Vec<u64> = objects.into_values().flatten().collect();
        played.sort_unstable();

        let records = Records::of_game(&self.process, &self.plugin);
        let kinds = self.rule_kinds();
        let today = self.game_date();
        let mut fixtures: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut nations: HashMap<u64, Option<NationRecords>> = HashMap::new();
        let mut found = Vec::new();
        for object in played {
            let word = |at: u64| self.process.read_u64(object + at).ok().filter(|&pointer| is_user_pointer(pointer));
            let (Some(db), Some(group)) = (word(layout.competition_db), word(layout.competition_group)) else {
                continue;
            };
            // Its competition points back at it: one left over from another season, or another save, is not played.
            if self.process.read_u64(db + layout.db_competition).ok() != Some(object) {
                continue;
            }
            let own = object + layout.competition_record;
            let Some(index) = records.number(own, FIXTURE_INDEX).and_then(|index| usize::try_from(index).ok()) else {
                continue;
            };
            let sets = fixtures.entry(group).or_insert_with(|| self.fixture_rules_of(layout, group));
            let Some(&set) = sets.get(index) else {
                continue;
            };
            let Some(read) = self.read_competition(db, competition) else {
                continue;
            };
            let Some(current) = self.listed_rules(layout, set) else {
                continue;
            };
            let next_set = self.process.read_u64(set + layout.fixture_next).ok().filter(|&next| is_user_pointer(next));
            let next = next_set.and_then(|next| self.listed_rules(layout, next));

            // The records of its nation: in the container of the nation's men's teams, or of its women's for a
            // competition those records list and the men's do not.
            let nation_object = self.process.read_u64(db + nation_at).ok().filter(|&nation| is_user_pointer(nation));
            let mut records_in = |container_at: u64| -> Option<NationRecords> {
                let container =
                    self.process.read_u64(nation_object? + container_at).ok().filter(|&c| is_user_pointer(c))?;
                nations
                    .entry(container)
                    .or_insert_with(|| read.nation_uid.and_then(|uid| nation_records(&records, layout, container, uid)))
                    .clone()
            };
            let (men, women) = (records_in(national.nation_men), records_in(national.nation_women));
            let lists = |nation: &Option<NationRecords>| {
                nation.as_ref().is_some_and(|nation| nation.competitions.contains(&read.uid))
            };
            let nation = if !lists(&men) && lists(&women) { women } else { men.or(women) };
            let season = today.map(|today| season_year(today, nation.as_ref().and_then(|nation| nation.turn)));
            let recorded = |sets: &[u64]| sets.get(index).map(|&set| recorded_rules(&records, set)).unwrap_or_default();
            // This season's list is made from the record the files make together, next season's from the rule file
            // that is for that season.
            let (together, file) = match &nation {
                Some(nation) => (recorded(&nation.together), recorded(nation.file_sets(season.map(|year| year + 1)))),
                None => (Vec::new(), Vec::new()),
            };
            // A competition of no nation whose rules no record was found for: the game makes them itself.
            let unsaved = nation_object.is_none();
            let rules = |listed: &[Listed], recorded: &[Recorded], year: Option<i32>| -> Vec<SquadRule> {
                let sources = match year {
                    Some(year) => sources_of(listed, recorded, year),
                    None => vec![None; listed.len()],
                };
                listed
                    .iter()
                    .zip(sources)
                    .map(|(rule, source)| {
                        let source = source.map(|at| recorded[at]);
                        SquadRule {
                            kind: rule.kind,
                            name: kinds.get(&rule.kind).cloned(),
                            compare: rule.compare,
                            value: rule.value,
                            condition: (rule.condition != 0)
                                .then(|| key_text(rule.condition).unwrap_or_else(|| format!("{:#x}", rule.condition))),
                            condition_competition: (rule.condition == TEAMS_FROM).then_some(rule.competition),
                            source: source.and_then(|source| {
                                Some(RuleSource {
                                    value: i32::try_from(source.value.number()?).ok()?,
                                    from_year: source.from_year,
                                    to_year: source.to_year,
                                })
                            }),
                            unsaved,
                            place: RulePlace {
                                slot: rule.slot,
                                rule: rule.rule,
                                condition: rule.condition,
                                source: source.map(|source| source.value),
                            },
                        }
                    })
                    .collect()
            };
            let days: Vec<RegistrationDay> =
                (self.registration_days(layout, set).into_iter()).filter(|day| day.competition == read.uid).collect();
            let registration = days.first().map(|first| Registration {
                days: days.len(),
                most: days.iter().all(|day| day.most == first.most).then_some(first.most),
            });
            found.push(CompetitionRules {
                unsaved,
                registration,
                level: records.number(own, LEVEL).and_then(|level| i32::try_from(level).ok()),
                fixture_rules: index,
                season,
                season_turn: (nation.as_ref())
                    .and_then(|nation| nation.turn)
                    .map(|(month, day)| SeasonTurn { month, day }),
                current: rules(&current, &together, season),
                next: next.map(|next| rules(&next, &file, season.map(|year| year + 1))),
                shared_with: Vec::new(),
                set,
                next_set,
                played: [object, db, group],
                competition: read,
            });
        }
        share_sets(&mut found);
        Ok(found)
    }

    /// The competitions' squad rules, as [`Session::squad_rules`] reads them, for the app: on the builds whose profile
    /// says the rules were checked there, and when what was found reads as the game's. One pass over the game's memory,
    /// which takes a few seconds.
    pub fn checked_squad_rules(&self) -> Result<Vec<CompetitionRules>, RuleReadError> {
        let unavailable = |reason| Err(RuleReadError::Unavailable(reason));
        if !self.profile.as_ref().is_some_and(|profile| profile.squad_rules) {
            return unavailable(RuleUnavailable::Build);
        }
        let found = match self.squad_rules() {
            Ok(found) => found,
            Err(RuleError::NotInLayout) => return unavailable(RuleUnavailable::Build),
            Err(RuleError::Memory(err)) => return Err(RuleReadError::Memory(err)),
            Err(_) => return unavailable(RuleUnavailable::Unreadable),
        };
        if !read_as_the_games(&found) {
            return unavailable(RuleUnavailable::Unreadable);
        }
        Ok(found)
    }

    /// Works out changing rules of `rules`, as read just now: each the rule at a place (from 0) of a season's list, to
    /// a new value, in the list and in the nation's rules a save keeps. Nothing is written.
    pub fn plan_rule_change(
        &self,
        rules: &CompetitionRules,
        changes: &[(Season, usize, i32)],
    ) -> Result<RuleChange, RuleError> {
        self.plan_rules(rules, changes, false)
    }

    /// The same for the list alone: the rule's value in the season's list and nowhere else, whether or not the
    /// nation's rules hold the rule. Where they do, the game makes the list from them again when it loads a save, and
    /// the change is gone.
    pub fn plan_list_change(
        &self,
        rules: &CompetitionRules,
        changes: &[(Season, usize, i32)],
    ) -> Result<RuleChange, RuleError> {
        self.plan_rules(rules, changes, true)
    }

    fn plan_rules(
        &self,
        rules: &CompetitionRules,
        changes: &[(Season, usize, i32)],
        list_only: bool,
    ) -> Result<RuleChange, RuleError> {
        let layout = self.layout.rules.as_ref().ok_or(RuleError::NotInLayout)?;
        let mut change = RuleChange { edits: Vec::new(), patches: Vec::new(), guards: Vec::new() };
        let mut patches = Patches::default();
        for &(season, at, to) in changes {
            let list = rules.rules(season).ok_or(RuleError::NoSeason)?;
            let rule = list.get(at).ok_or(RuleError::NoRule { rule: at, rules: list.len() })?;
            if !(0..=MOST_RULE_VALUE).contains(&rule.value) {
                return Err(RuleError::NotACount { value: rule.value });
            }
            if !(0..=MOST_RULE_VALUE).contains(&to) {
                return Err(RuleError::OutOfRange { value: to });
            }
            // The rule's value in the nation's rules, and what it is there: none for a change of the list alone, which
            // a change of a rule no record keeps always is.
            let list_only = list_only || (rule.unsaved && rule.place.source.is_none());
            let source = match (list_only, rule.place.source, rule.source) {
                (true, _, _) => None,
                (false, Some(source), Some(recorded)) => Some((source, recorded.value)),
                (false, _, _) => return Err(RuleError::NoSource { kind: rule.kind }),
            };
            let place = rule.place;
            change.guards.push(Guard {
                slot: place.slot,
                rule: place.rule,
                kind: rule.kind,
                compare: rule.compare,
                condition: place.condition,
            });
            change.edits.push(RuleEdit {
                competition: rules.competition.uid,
                season,
                rule: at,
                kind: rule.kind,
                from: rule.value,
                to,
                source_from: source.map_or(rule.value, |(_, recorded)| recorded),
                list_only,
            });
            patches.push(place.rule + layout.rule_value, rule.value.to_le_bytes(), to.to_le_bytes());
            if let Some((source, _)) = source {
                patches.push(source.at, source.bytes(), source.bytes_with(to));
            }
            // The days squads are registered by repeat the squad's size, and the competition's page reads it from
            // them: those of the season's set that say what the rule said are given what it says now, for every
            // competition played under the set, as the rule is theirs too.
            if rule.name.as_deref() == Some(SQUAD_SIZE) && rule.at_most() {
                let set = match season {
                    Season::Current => Some(rules.set),
                    Season::Next => rules.next_set,
                };
                let days = set.map(|set| self.registration_days(layout, set)).unwrap_or_default();
                // From the competition's UID to the most players: the entry must still be the one read.
                let (first, last) = (layout.registration_competition, layout.registration_most);
                for day in days.into_iter().filter(|day| i32::from(day.most) == rule.value) {
                    let before = self.process.read_bytes(day.at + first, (last - first + 1) as usize)?;
                    let mut after = before.clone();
                    if let Some(most) = after.last_mut() {
                        *most = to as u8;
                    }
                    patches.push(day.at + first, before, after);
                }
            }
        }
        change.patches = patches.0;
        Ok(change)
    }

    /// Writes a planned change and returns it, for its undo. Each list must still hold its rule, each rule be of the
    /// kind it was, and each value still be what the plan read; when a write fails, the ones before it are put back.
    pub fn apply_rule_change(&self, plan: &RuleChange) -> Result<RuleChange, RuleError> {
        self.check_rules(&plan.guards)?;
        self.apply_patches(&plan.patches)?;
        Ok(plan.clone())
    }

    /// Puts back what a change found, provided the lists still hold their rules and memory still holds what the change
    /// wrote.
    pub fn undo_rule_change(&self, done: &RuleChange) -> Result<(), RuleError> {
        self.check_rules(&done.guards)?;
        self.undo_patches(&done.patches)?;
        Ok(())
    }

    /// What each record of the nation of the competition `uid` holds for the competition's set of fixture rules: in
    /// the container of its men's teams and then of its women's, the record the files make together and after it each
    /// of the files' own records, in the container's order. Empty where the game is not playing the competition, or it
    /// has no nation. For the probe's `rules --sources`: which of them the game makes a list from is what a turn of the
    /// season and a load show.
    pub fn rule_records(&self, uid: u32) -> Result<Vec<RuleRecord>, RuleError> {
        let layout = self.layout.rules.as_ref().ok_or(RuleError::NotInLayout)?;
        let competition = self.layout.competition.as_ref().ok_or(RuleError::NotInLayout)?;
        let national = self.layout.national.as_ref().ok_or(RuleError::NotInLayout)?;
        let nation_at = competition.nation.ok_or(RuleError::NotInLayout)?;
        let vtables = rtti::vtables(&self.process, &self.plugin, &layout.competition_class);
        let objects = rtti::objects(&self.process, &self.plugin, &vtables)?;
        let records = Records::of_game(&self.process, &self.plugin);
        for object in objects.into_values().flatten() {
            let word = |at: u64| self.process.read_u64(object + at).ok().filter(|&pointer| is_user_pointer(pointer));
            let Some(db) = word(layout.competition_db) else {
                continue;
            };
            if self.process.read_u64(db + layout.db_competition).ok() != Some(object) {
                continue;
            }
            let Some(read) = self.read_competition(db, competition).filter(|read| read.uid == uid) else {
                continue;
            };
            let own = object + layout.competition_record;
            let Some(index) = records.number(own, FIXTURE_INDEX).and_then(|index| usize::try_from(index).ok()) else {
                continue;
            };
            let nation = self.process.read_u64(db + nation_at).ok().filter(|&nation| is_user_pointer(nation));
            let today = self.game_date();
            let year = |record: u64, key: u32| records.number(record, key).and_then(|year| i32::try_from(year).ok());
            let mut found = Vec::new();
            for (women, container_at) in [(false, national.nation_men), (true, national.nation_women)] {
                let container = nation.and_then(|nation| self.process.read_u64(nation + container_at).ok());
                let Some(container) = container.filter(|&container| is_user_pointer(container)) else {
                    continue;
                };
                // The file the editor takes next season's rules from: the one that is for that season.
                let held = read.nation_uid.and_then(|nation| nation_records(&records, layout, container, nation));
                let next_year = today.map(|today| season_year(today, held.as_ref().and_then(|held| held.turn)) + 1);
                let chosen = held.as_ref().and_then(|held| match next_year {
                    Some(year) => held.files.iter().find(|file| file.good_in(year)),
                    None => held.files.first(),
                });
                let chosen = chosen.map(|file| file.record);
                let describe = |record: u64, together: bool| {
                    let sets = records.list(record, FIXTURE_RULES).map(|sets| records.records_of(&sets));
                    let set = sets.as_ref().and_then(|sets| sets.get(index).copied());
                    let rules = set.map(|set| recorded_rules(&records, set)).unwrap_or_default();
                    RuleRecord {
                        record,
                        women,
                        together,
                        chosen: !together && chosen == Some(record),
                        from_year: year(record, FILE_FROM),
                        to_year: year(record, FILE_TO),
                        sets: sets.map(|sets| sets.len()),
                        rules: rules.into_iter().map(RecordedRule::of).collect(),
                    }
                };
                found.push(describe(container + layout.container_rules, true));
                let files = records.elements(container + layout.container_files);
                for file in files.map(|files| records.records_of(&files)).unwrap_or_default() {
                    found.push(describe(file, false));
                }
            }
            return Ok(found);
        }
        Ok(Vec::new())
    }

    /// The game's names for the kinds of squad rule, by their number: read from its table of them the first time they
    /// are asked for. Empty where the layout does not place the rules, or the table is not found in this build.
    pub fn rule_kinds(&self) -> Arc<HashMap<u16, String>> {
        self.rule_kinds
            .get_or_init(|| {
                let kinds = self.layout.rules.as_ref().map(|layout| {
                    read_rule_kinds(&self.process, &self.plugin, &layout.kind_anchor).unwrap_or_default()
                });
                Arc::new(kinds.unwrap_or_default())
            })
            .clone()
    }

    fn check_rules(&self, guards: &[Guard]) -> Result<(), RuleError> {
        let layout = self.layout.rules.as_ref().ok_or(RuleError::NotInLayout)?;
        for guard in guards {
            let held = self.process.read_u64(guard.slot).ok() == Some(guard.rule)
                && self.read_rule(layout, guard.slot, guard.rule).is_some_and(|rule| {
                    (rule.kind, rule.compare, rule.condition) == (guard.kind, guard.compare, guard.condition)
                });
            if !held {
                return Err(RuleError::Gone { rule: guard.rule });
            }
        }
        Ok(())
    }

    /// The sets of fixture rules of the nation the rule group at `group` belongs to; none where it leads to none.
    fn fixture_rules_of(&self, layout: &RulesLayout, group: u64) -> Vec<u64> {
        let Some(nation) =
            self.process.read_u64(group + layout.group_nation_rules).ok().filter(|&nation| is_user_pointer(nation))
        else {
            return Vec::new();
        };
        self.pointers_at(nation + layout.nation_fixture_rules, MOST_FIXTURE_RULES).unwrap_or_default()
    }

    /// The squad rules of the fixture rules at `set`, in the order of their list; `None` when what is there is no
    /// list of rules.
    fn listed_rules(&self, layout: &RulesLayout, set: u64) -> Option<Vec<Listed>> {
        let list = self.process.read_u64(set + layout.fixture_squad_rules).ok()?;
        if list == 0 {
            return Some(Vec::new());
        }
        if !is_user_pointer(list) {
            return None;
        }
        let begin = self.process.read_u64(list).ok()?;
        let pointers = self.pointers_at(list, MOST_RULES)?;
        pointers.iter().enumerate().map(|(at, &rule)| self.read_rule(layout, begin + 8 * at as u64, rule)).collect()
    }

    /// The rule at `rule`, which the place at `slot` of its list points at.
    fn read_rule(&self, layout: &RulesLayout, slot: u64, rule: u64) -> Option<Listed> {
        if !is_user_pointer(rule) {
            return None;
        }
        let bytes = self.process.read_bytes(rule, layout.rule_size as usize).ok()?;
        let u32_at = |at: u64| u32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().expect("4 bytes"));
        Some(Listed {
            slot,
            rule,
            condition: u32_at(layout.rule_condition),
            competition: u32_at(layout.rule_competition),
            value: u32_at(layout.rule_value) as i32,
            kind: u32_at(layout.rule_kind) as u16,
            compare: bytes[layout.rule_compare as usize] as i8,
        })
    }

    /// The days squads are registered by that the fixture rules at `set` hold, for every competition played under
    /// them: the entries its table points at that carry the key of one. None for a set with no such table.
    fn registration_days(&self, layout: &RulesLayout, set: u64) -> Vec<RegistrationDay> {
        let entries = self.pointers_at(set + layout.fixture_registration, MOST_REGISTRATIONS).unwrap_or_default();
        let read = |at: u64| -> Option<RegistrationDay> {
            let bytes = self.process.read_bytes(at, layout.registration_size as usize).ok()?;
            let u32_at = |at: u64| u32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().expect("4 bytes"));
            (u32_at(layout.registration_key) == REGISTRATION).then(|| RegistrationDay {
                at,
                competition: u32_at(layout.registration_competition),
                most: bytes[layout.registration_most as usize],
            })
        };
        entries.into_iter().filter(|&at| is_user_pointer(at)).filter_map(read).collect()
    }

    /// The pointers of the list whose `(begin, end)` is at `header`, no more than `most`; empty for a list with no
    /// buffer, `None` for what is no list.
    fn pointers_at(&self, header: u64, most: u64) -> Option<Vec<u64>> {
        let pair = self.process.read_bytes(header, 16).ok()?;
        let begin = u64::from_le_bytes(pair[..8].try_into().expect("8 bytes"));
        let end = u64::from_le_bytes(pair[8..].try_into().expect("8 bytes"));
        if begin == 0 && end == 0 {
            return Some(Vec::new());
        }
        let length = end.checked_sub(begin)?;
        if !is_user_pointer(begin) || !length.is_multiple_of(8) || length / 8 > most {
            return None;
        }
        let bytes = self.process.read_bytes(begin, length as usize).ok()?;
        Some(bytes.as_chunks::<8>().0.iter().map(|pointer| u64::from_le_bytes(*pointer)).collect())
    }
}

/// What the container at `container` holds of its nation's fixture rules, provided its record is that of the nation
/// with the UID `nation`.
fn nation_records(records: &Records, layout: &RulesLayout, container: u64, nation: u32) -> Option<NationRecords> {
    let together = container + layout.container_rules;
    // The nation's UID: a number (Spain's files), or in a record of its own (Türkiye's), there as a number or as
    // text ("1651", Brazil's).
    let uid_of = |value: Typed| match value.kind {
        Traits::Text => records.text(&value)?.trim().parse::<i64>().ok(),
        _ => value.number(),
    };
    let named = match records.get(together, NATION)? {
        of if of.kind == Traits::Record => uid_of(records.get(of.raw, NATION_UID)?)?,
        uid => uid_of(uid)?,
    };
    if named != i64::from(nation) {
        return None;
    }
    let sets = |record: u64| records.list(record, FIXTURE_RULES).map(|sets| records.records_of(&sets));
    let turn = records.record(together, UPDATE_DAY).and_then(|day| {
        let part = |key: u32| records.number(day, key).and_then(|part| u8::try_from(part).ok());
        Some((part(MONTH)?, part(DAY)?))
    });
    // A rule file's own record is one of the files' that holds fixture rules: a nation has one, or one for each run
    // of seasons its rules differ in, which then says the first or the last year it is for.
    let files = records.elements(container + layout.container_files).map(|files| records.records_of(&files));
    let year = |record: u64, key: u32| records.number(record, key).and_then(|year| i32::try_from(year).ok());
    let files = files.unwrap_or_default().into_iter().filter_map(|record| {
        Some(RuleFile {
            record,
            from_year: year(record, FILE_FROM),
            to_year: year(record, FILE_TO),
            sets: sets(record)?,
        })
    });
    let listed = records.list(together, COMPETITIONS).map(|listed| records.records_of(&listed)).unwrap_or_default();
    let competitions =
        listed.into_iter().filter_map(|listed| competition_uid(records, records.record(listed, COMPETITION)?));
    Some(NationRecords {
        competitions: competitions.collect(),
        turn,
        together: sets(together)?,
        files: files.collect(),
    })
}

/// The UID of the competition the record at `competition` names: under its own key, or as a pair of numbers whose
/// lower half is the UID.
fn competition_uid(records: &Records, competition: u64) -> Option<u32> {
    let uid = records.number(competition, DATABASE_ID).or_else(|| records.number(competition, COMPETITION))?;
    Some(uid as u32)
}

/// The squad rules the record of a set of fixture rules holds, in its order; what does not read as a rule is left
/// out.
fn recorded_rules(records: &Records, set: u64) -> Vec<Recorded> {
    let Some(rules) = records.list(set, SQUAD_RULES) else {
        return Vec::new();
    };
    let year = |rule: u64, key: u32| records.number(rule, key).and_then(|year| i32::try_from(year).ok());
    records
        .records_of(&rules)
        .into_iter()
        .filter_map(|rule| {
            let kind = records.number(rule, KIND).and_then(|kind| u16::try_from(kind).ok())?;
            let value = records.get(rule, VALUE).filter(|value| value.kind == crate::records::Traits::Int)?;
            let (condition, competition) = match records.record(rule, TEAMS_FROM) {
                Some(teams) => (TEAMS_FROM, competition_uid(records, teams)),
                None => {
                    let info = records.list(rule, INFO).map(|info| records.records_of(&info)).unwrap_or_default();
                    let first = info.first().and_then(|&info| records.number(info, KIND));
                    (first.map_or(0, |key| key as u32), None)
                }
            };
            Some(Recorded {
                kind,
                compare: records.number(rule, COMPARE).and_then(|compare| i8::try_from(compare).ok()),
                value,
                from_year: year(rule, START_YEAR),
                to_year: year(rule, END_YEAR),
                condition,
                competition,
            })
        })
        .collect()
}

/// The game's table of the kinds of squad rule: entries of 16 bytes, a pointer to the kind's name and its number, one
/// after another in the plugin. It is found by the entry that points at the name `anchor`, and read both ways from
/// there for as long as entries read.
fn read_rule_kinds(process: &Process, plugin: &Module, anchor: &str) -> Option<HashMap<u16, String>> {
    let needle: Vec<u8> = [0].into_iter().chain(anchor.bytes()).chain([0]).collect();
    let mut names = Vec::new();
    rtti::each_chunk(process, plugin.base, plugin.end(), needle.len(), |base, bytes, limit| {
        let finder = memchr::memmem::Finder::new(&needle);
        names.extend(finder.find_iter(bytes).filter(|&at| at < limit).map(|at| base + at as u64 + 1));
    });
    if names.is_empty() {
        return None;
    }
    let entry = |at: u64| -> Option<(u16, String)> {
        let bytes = process.read_bytes(at, 16).ok()?;
        let name = u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"));
        let number = u64::from_le_bytes(bytes[8..].try_into().expect("8 bytes"));
        if !plugin.contains(name) {
            return None;
        }
        let text = process.read_bytes(name, LONGEST_KIND).ok()?;
        let end = text.iter().position(|&byte| byte == 0).filter(|&end| end > 0)?;
        if !text[..end].iter().all(|byte| byte.is_ascii_graphic() || *byte == b' ') {
            return None;
        }
        Some((u16::try_from(number).ok()?, String::from_utf8_lossy(&text[..end]).into_owned()))
    };
    rtti::words_equal_to(process, plugin.base, plugin.end(), &names).into_iter().find_map(|found| {
        let before = (1..=MOST_KINDS).map_while(|step| entry(found.checked_sub(16 * step)?));
        let after = (0..=MOST_KINDS).map_while(|step| entry(found + 16 * step));
        let mut kinds = HashMap::new();
        for (number, name) in before.chain(after) {
            kinds.entry(number).or_insert(name);
        }
        // A table of one or two entries is another's: the game's own has hundreds.
        (kinds.len() >= 16).then_some(kinds)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Traits;

    fn listed(kind: u16, compare: i8, condition: u32, competition: u32) -> Listed {
        Listed { slot: 0, rule: 0, condition, competition, value: 0, kind, compare }
    }

    fn recorded(kind: u16, compare: i8, years: (Option<i32>, Option<i32>), condition: u32) -> Recorded {
        Recorded {
            kind,
            compare: Some(compare),
            value: Typed { at: 0, traits: 0, kind: Traits::Int, raw: 0 },
            from_year: years.0,
            to_year: years.1,
            condition,
            competition: None,
        }
    }

    const ONE_YEAR_OLDER: u32 = key(b"1yol");
    const TURKIC: u32 = key(b"Tk24");

    /// The Süper Lig's rules as its nation's record holds them: at most 14 foreign players; at most 10 of them over
    /// 24, by one rule for 2026 and 2027 and another from 2028; at least 4 Turkish players under 23 from 2026; at
    /// most 3 goalkeepers; at least 2 of them Turkish; at most 28 players.
    fn super_lig() -> Vec<Recorded> {
        vec![
            recorded(21, -1, (None, None), 0),
            recorded(368, -1, (Some(2026), Some(2027)), ONE_YEAR_OLDER),
            recorded(368, -1, (Some(2028), None), 0),
            recorded(209, 1, (Some(2026), None), TURKIC),
            recorded(87, -1, (None, None), 0),
            recorded(230, 1, (None, None), 0),
            recorded(9, -1, (None, None), 0),
        ]
    }

    #[test]
    fn a_seasons_list_is_matched_to_the_rules_good_in_its_year() {
        // The list of 2026/27, and of 2027/28, as the game makes it: the rule from 2028 is not in it.
        let now = vec![
            listed(21, -1, 0, 0),
            listed(368, -1, ONE_YEAR_OLDER, 0),
            listed(209, 1, TURKIC, 0),
            listed(87, -1, 0, 0),
            listed(230, 1, 0, 0),
            listed(9, -1, 0, 0),
        ];
        let at = |places: [usize; 6]| places.map(Some).to_vec();
        assert_eq!(sources_of(&now, &super_lig(), 2026), at([0, 1, 3, 4, 5, 6]));
        assert_eq!(sources_of(&now, &super_lig(), 2027), at([0, 1, 3, 4, 5, 6]));
        // From 2028 the list holds the other rule about the over-24s, with no condition.
        let later = vec![listed(21, -1, 0, 0), listed(368, -1, 0, 0), listed(209, 1, TURKIC, 0)];
        assert_eq!(sources_of(&later, &super_lig(), 2028), [Some(0), Some(2), Some(3)]);
        // In a year a rule is not good in, or under another condition, the list's rule has none in the record.
        assert_eq!(sources_of(&now[..2], &super_lig(), 2028), [Some(0), None]);
        assert_eq!(sources_of(&now[..3], &super_lig(), 2025), [Some(0), None, None]);
    }

    #[test]
    fn a_rule_the_record_does_not_hold_is_left_without_one() {
        // The 2. Lig's list ends with the squad's size, which its rule file gives elsewhere.
        let record = vec![recorded(21, -1, (None, None), 0), recorded(87, -1, (None, None), 0)];
        let list = vec![listed(21, -1, 0, 0), listed(87, -1, 0, 0), listed(9, -1, 0, 0)];
        assert_eq!(sources_of(&list, &record, 2026), [Some(0), Some(1), None]);
        // One the list holds before the record's does not take the record's away from the one after it.
        let list = vec![listed(9, -1, 0, 0), listed(21, -1, 0, 0)];
        assert_eq!(sources_of(&list, &record, 2026), [None, Some(0)]);
        // The other way to limit is another rule.
        assert_eq!(sources_of(&[listed(87, 1, 0, 0)], &record, 2026), [None]);
    }

    #[test]
    fn a_cups_rules_are_told_apart_by_the_division_they_are_for() {
        let teams_from = |uid: u32| Recorded { competition: Some(uid), ..recorded(4, -1, (None, None), TEAMS_FROM) };
        // At most so many foreign players in the match squad, for the teams of the Süper Lig and of the 1. Lig.
        let record = vec![teams_from(130_286), teams_from(463_479)];
        let list = vec![listed(4, -1, TEAMS_FROM, 463_479), listed(4, -1, TEAMS_FROM, 130_286)];
        // The list's first is the record's second; the record's first comes before it, and is not looked back for.
        assert_eq!(sources_of(&list, &record, 2026), [Some(1), None]);
        let in_order = vec![listed(4, -1, TEAMS_FROM, 130_286), listed(4, -1, TEAMS_FROM, 463_479)];
        assert_eq!(sources_of(&in_order, &record, 2026), [Some(0), Some(1)]);
    }

    #[test]
    fn a_season_begins_in_the_year_its_nation_last_turned() {
        let day = |year, month, day| FmDate { year, month, day };
        // Türkiye's seasons turn on 20 June.
        let turkey = Some((6, 20));
        assert_eq!(season_year(day(2026, 10, 19), turkey), 2026);
        assert_eq!(season_year(day(2027, 1, 16), turkey), 2026);
        assert_eq!(season_year(day(2027, 6, 19), turkey), 2026);
        assert_eq!(season_year(day(2027, 6, 20), turkey), 2027);
        // Where the rules name no day, the calendar year.
        assert_eq!(season_year(day(2027, 3, 1), None), 2027);
        // Korea's seasons turn on 31 December: the one played in 2026 is 2026's, and from that day on it is 2027's.
        let korea = Some((12, 31));
        assert_eq!(season_year(day(2026, 10, 20), korea), 2026);
        assert_eq!(season_year(day(2026, 12, 30), korea), 2026);
        assert_eq!(season_year(day(2026, 12, 31), korea), 2027);
        assert_eq!(season_year(day(2027, 6, 15), korea), 2027);
        // Brazil's turn on 2 January, in the year they are played in.
        let brazil = Some((1, 2));
        assert_eq!(season_year(day(2027, 1, 1), brazil), 2026);
        assert_eq!(season_year(day(2027, 1, 2), brazil), 2027);
    }

    #[test]
    fn next_seasons_rules_come_from_the_file_that_is_for_that_season() {
        let file = |record: u64, years: (Option<i32>, Option<i32>)| RuleFile {
            record,
            from_year: years.0,
            to_year: years.1,
            sets: vec![record],
        };
        // England's two rule files: one for the seasons to 2027, one from 2028.
        let england = NationRecords {
            files: vec![file(3, (None, Some(2027))), file(4, (Some(2028), None))],
            ..NationRecords::default()
        };
        assert_eq!(england.file_sets(Some(2027)), [3]);
        assert_eq!(england.file_sets(Some(2028)), [4]);
        // With no date to tell the season by, the first.
        assert_eq!(england.file_sets(None), [3]);
        // Türkiye's one file is for every season.
        let turkey = NationRecords { files: vec![file(3, (None, None))], ..NationRecords::default() };
        assert_eq!(turkey.file_sets(Some(2031)), [3]);
        // A season no file is for has none, and its rules are left alone.
        let ends = NationRecords { files: vec![file(3, (None, Some(2027)))], ..NationRecords::default() };
        assert!(ends.file_sets(Some(2028)).is_empty());
        assert!(NationRecords::default().file_sets(Some(2028)).is_empty());
    }

    #[test]
    fn a_rule_is_changed_only_where_it_is_a_count_the_record_holds() {
        let rule = |value: i32, source: bool| SquadRule {
            kind: 21,
            name: None,
            compare: -1,
            value,
            condition: None,
            condition_competition: None,
            source: source.then_some(RuleSource { value, from_year: None, to_year: None }),
            unsaved: false,
            place: RulePlace { slot: 0, rule: 0, condition: 0, source: None },
        };
        assert!(rule(14, true).editable() && rule(0, true).editable() && rule(14, true).at_most());
        // A wage is no count; a rule the record does not hold would not last.
        assert!(!rule(71_940_388, true).editable());
        assert!(!rule(25, false).editable());
        assert_eq!(RuleError::NoRule { rule: 6, rules: 6 }.to_string(), "no rule 7: the season's list has 6");
        assert_eq!(RuleError::OutOfRange { value: 300 }.code(), "rule.outOfRange");
    }

    #[test]
    fn a_change_written_or_undone_is_noted_in_the_rules_read() {
        // A rule in its own struct, with its value in a record: the list said 14 while the record held 21, as after
        // a save made once the record alone was changed.
        let rule = |at: u64, value_at: u64| SquadRule {
            kind: 21,
            name: Some("all_foreign_players_in_squad".into()),
            compare: -1,
            value: 14,
            condition: None,
            condition_competition: None,
            source: Some(RuleSource { value: 21, from_year: None, to_year: None }),
            unsaved: false,
            place: RulePlace {
                slot: 0,
                rule: at,
                condition: 0,
                source: Some(Typed { at: value_at, traits: 0x1C5_518C_DA28, kind: Traits::Int, raw: 0x7FFA_0000_0015 }),
            },
        };
        let competition =
            |uid: u32| Competition { uid, name: String::new(), short_name: None, nation_uid: None, nation: None };
        // This season's list and next season's are two lists, made from two records.
        let rules = |uid: u32, set: u64| CompetitionRules {
            competition: competition(uid),
            level: Some(0),
            fixture_rules: 0,
            season: Some(2026),
            season_turn: None,
            current: vec![rule(set + 0x100, set + 0x1000)],
            next: Some(vec![rule(set + 0x200, set + 0x2000)]),
            unsaved: false,
            registration: None,
            shared_with: Vec::new(),
            set,
            next_set: None,
            played: [0; 3],
        };
        // A division and one of its groups, played under the same fixture rules, and another division.
        let (division, other) = (0x10_0000, 0x20_0000);
        let mut found = vec![rules(463_481, division), rules(463_485, division), rules(130_286, other)];
        share_sets(&mut found);
        let shared: Vec<&[u32]> = found.iter().map(|rules| rules.shared_with.as_slice()).collect();
        assert_eq!(shared, [&[463_485][..], &[463_481], &[]]);

        let edit = RuleEdit {
            competition: 463_481,
            season: Season::Next,
            rule: 0,
            kind: 21,
            from: 14,
            to: 20,
            source_from: 21,
            list_only: false,
        };
        let change = RuleChange { edits: vec![edit.clone()], patches: Vec::new(), guards: Vec::new() };
        let read = |found: &[CompetitionRules], at: usize| {
            let rule = &found[at].next.as_ref().expect("next season's list")[0];
            (rule.value, rule.source.expect("its rule in the record").value, rule.place.source.expect("its place").raw)
        };
        change.note(&mut found, false);
        // The division's rule, which is its group's too.
        assert_eq!(read(&found, 0), (20, 20, 0x7FFA_0000_0014));
        assert_eq!(read(&found, 1), (20, 20, 0x7FFA_0000_0014));
        // This season's list, and the other division, are as they were.
        assert_eq!(found[0].current[0], rule(division + 0x100, division + 0x1000));
        assert_eq!(found[2].next, rules(130_286, other).next);
        change.note(&mut found, true);
        assert_eq!(read(&found, 0), (14, 21, 0x7FFA_0000_0015));
        assert_eq!(read(&found, 1), (14, 21, 0x7FFA_0000_0015));
        // A rule of another kind at that place is left alone.
        let other = RuleChange { edits: vec![RuleEdit { kind: 9, ..edit }], patches: Vec::new(), guards: Vec::new() };
        other.note(&mut found, false);
        assert_eq!(read(&found, 0).0, 14);
    }

    #[test]
    fn rules_read_as_the_games_when_the_game_names_their_kinds() {
        let rule = |name: Option<&str>| SquadRule {
            kind: 21,
            name: name.map(str::to_owned),
            compare: -1,
            value: 14,
            condition: None,
            condition_competition: None,
            source: None,
            unsaved: false,
            place: RulePlace { slot: 0, rule: 0, condition: 0, source: None },
        };
        let rules = |current: Vec<SquadRule>| CompetitionRules {
            competition: Competition { uid: 1, name: String::new(), short_name: None, nation_uid: None, nation: None },
            level: None,
            fixture_rules: 0,
            season: None,
            season_turn: None,
            current,
            next: None,
            unsaved: false,
            registration: None,
            shared_with: Vec::new(),
            set: 0,
            next_set: None,
            played: [0; 3],
        };
        let named = rule(Some("all_foreign_players_in_squad"));
        assert!(read_as_the_games(&[rules(vec![named.clone(); 9]), rules(vec![rule(None)])]));
        // No rules at all, or kinds the game's table does not name: the wrong bytes.
        assert!(!read_as_the_games(&[rules(Vec::new())]));
        assert!(!read_as_the_games(&[rules(vec![named, rule(None)])]));
    }

    /// A change and its undo, through a list and a record laid out in memory of the test's own as the game lays them
    /// out: Windows only (see `Session::over`).
    #[cfg(windows)]
    #[test]
    fn a_change_is_written_to_the_list_and_to_the_record_and_put_back() {
        use crate::session::Blocks;

        let memory = Blocks::new(2);
        let process = Process::open_read_write(std::process::id()).expect("the test opens its own process");
        let session = Session::over(process, crate::layout_for(27).expect("FM27 layout"));
        let layout = session.layout.rules.clone().expect("the rules' layout");
        // A list of one rule: at most 14 of kind 21; and its value in a record, 14 below what was left in its word.
        let (slot, rule, value) = (memory.at(0), memory.at(0) + 0x100, memory.at(1));
        session.put_pointer(slot, rule);
        session.put(rule + layout.rule_value, &14i32.to_le_bytes());
        session.put(rule + layout.rule_kind, &21u16.to_le_bytes());
        session.put(rule + layout.rule_compare, &[0xFF]);
        let source = Typed { at: value, traits: 0x1C5_518C_DA28, kind: Traits::Int, raw: 0x7FFA_0000_000E };
        session.put(value, &source.bytes());
        let listed = session.read_rule(&layout, slot, rule).expect("the rule as laid out");
        assert_eq!((listed.kind, listed.compare, listed.value), (21, -1, 14));

        let rules = CompetitionRules {
            competition: Competition {
                uid: 130_286,
                name: "Süper Lig".into(),
                short_name: None,
                nation_uid: None,
                nation: None,
            },
            level: Some(0),
            fixture_rules: 0,
            season: Some(2026),
            season_turn: None,
            current: vec![SquadRule {
                kind: 21,
                name: None,
                compare: -1,
                value: 14,
                condition: None,
                condition_competition: None,
                source: Some(RuleSource { value: 14, from_year: None, to_year: None }),
                unsaved: false,
                place: RulePlace { slot, rule, condition: 0, source: Some(source) },
            }],
            next: None,
            unsaved: false,
            registration: None,
            shared_with: Vec::new(),
            set: 0,
            next_set: None,
            played: [0; 3],
        };
        let refused =
            |changes: &[(Season, usize, i32)]| session.plan_rule_change(&rules, changes).expect_err("refused");
        assert!(matches!(refused(&[(Season::Next, 0, 20)]), RuleError::NoSeason));
        assert!(matches!(refused(&[(Season::Current, 1, 20)]), RuleError::NoRule { rule: 1, rules: 1 }));
        assert!(matches!(refused(&[(Season::Current, 0, 256)]), RuleError::OutOfRange { value: 256 }));
        assert!(matches!(refused(&[(Season::Current, 0, -1)]), RuleError::OutOfRange { value: -1 }));

        let plan = session.plan_rule_change(&rules, &[(Season::Current, 0, 20)]).expect("a plan");
        assert_eq!(plan.edits.len(), 1);
        assert_eq!((plan.edits[0].from, plan.edits[0].to, plan.edits[0].source_from), (14, 20, 14));
        let read = || {
            let listed = session.read_rule(&layout, slot, rule).expect("the rule").value;
            (listed, session.process.read_u64(value + 8).expect("the record's word"))
        };
        // The writes themselves: the checks of the game's health are for a game, and this is none.
        session.check_rules(&plan.guards).expect("the list holds its rule");
        session.write_patches(&plan.patches).expect("written");
        assert_eq!(read(), (20, 0x7FFA_0000_0014), "the record's word keeps its upper half");
        session.undo_rule_change(&plan).expect("put back");
        assert_eq!(read(), (14, 0x7FFA_0000_000E));

        // Once the list points elsewhere, or the rule is of another kind, nothing is written.
        session.put(rule + layout.rule_kind, &9u16.to_le_bytes());
        assert!(matches!(session.check_rules(&plan.guards), Err(RuleError::Gone { .. })));
        session.put(rule + layout.rule_kind, &21u16.to_le_bytes());
        session.put_pointer(slot, rule + 0x40);
        assert!(matches!(session.undo_rule_change(&plan), Err(RuleError::Gone { .. })));
        assert_eq!(read().0, 14);
        std::hint::black_box(&memory);
    }

    /// A squad's size where no record keeps the rules, as UEFA's: changed in the list alone, and in the days squads
    /// are registered by that repeat it. Through a set laid out in memory of the test's own: Windows only.
    #[cfg(windows)]
    #[test]
    fn a_rule_no_record_keeps_is_changed_in_the_list_and_in_the_days_that_repeat_it() {
        use crate::session::Blocks;

        let memory = Blocks::new(3);
        let process = Process::open_read_write(std::process::id()).expect("the test opens its own process");
        let session = Session::over(process, crate::layout_for(27).expect("FM27 layout"));
        let layout = session.layout.rules.clone().expect("the rules' layout");
        // The set's table: an empty place, then three days: two of 25 players, of two competitions played under the
        // set, and a later one that takes three new players.
        let (set, table) = (memory.at(0), memory.at(0) + 0x200);
        let days = [memory.at(1), memory.at(1) + 0x40, memory.at(1) + 0x80];
        session.put_pointer(set + layout.fixture_registration, table);
        session.put_pointer(set + layout.fixture_registration + 8, table + 4 * 8);
        for (place, &day) in days.iter().enumerate() {
            session.put_pointer(table + 8 * (place as u64 + 1), day);
        }
        for (&day, (competition, most)) in days.iter().zip([(1_301_394u32, 25u8), (1_301_396, 25), (1_301_394, 3)]) {
            session.put(day + layout.registration_competition, &competition.to_le_bytes());
            session.put(day + layout.registration_key, &REGISTRATION.to_le_bytes());
            session.put(day + layout.registration_most, &[most]);
        }
        let read_days = || session.registration_days(&layout, set).iter().map(|day| day.most).collect::<Vec<u8>>();
        assert_eq!(read_days(), [25, 25, 3]);

        // The list: at most 25 players in the squad.
        let (slot, rule) = (memory.at(2), memory.at(2) + 0x100);
        session.put_pointer(slot, rule);
        session.put(rule + layout.rule_value, &25i32.to_le_bytes());
        session.put(rule + layout.rule_kind, &9u16.to_le_bytes());
        session.put(rule + layout.rule_compare, &[0xFF]);
        let squad_size = |unsaved: bool| SquadRule {
            kind: 9,
            name: Some(SQUAD_SIZE.into()),
            compare: -1,
            value: 25,
            condition: None,
            condition_competition: None,
            source: None,
            unsaved,
            place: RulePlace { slot, rule, condition: 0, source: None },
        };
        let rules = |unsaved: bool| CompetitionRules {
            competition: Competition {
                uid: 1_301_394,
                name: "UEFA Champions League".into(),
                short_name: None,
                nation_uid: None,
                nation: None,
            },
            level: Some(0),
            fixture_rules: 1,
            season: Some(2027),
            season_turn: None,
            current: vec![squad_size(unsaved)],
            next: None,
            unsaved,
            registration: None,
            shared_with: vec![1_301_396],
            set,
            next_set: None,
            played: [0; 3],
        };
        // A rule a nation's records do not hold is left alone; one no record keeps at all is changed in the list.
        assert!(!squad_size(false).editable() && squad_size(true).editable());
        let refused = session.plan_rule_change(&rules(false), &[(Season::Current, 0, 30)]);
        assert!(matches!(refused, Err(RuleError::NoSource { kind: 9 })));
        let plan = session.plan_rule_change(&rules(true), &[(Season::Current, 0, 30)]).expect("a plan");
        assert!(plan.edits[0].list_only);
        assert_eq!((plan.edits[0].from, plan.edits[0].to), (25, 30));

        let listed = || session.read_rule(&layout, slot, rule).expect("the rule").value;
        session.check_rules(&plan.guards).expect("the list holds its rule");
        session.write_patches(&plan.patches).expect("written");
        // The days that said 25 say 30, for both competitions; the one of three new players is as it was.
        assert_eq!((listed(), read_days()), (30, vec![30, 30, 3]));
        session.undo_rule_change(&plan).expect("put back");
        assert_eq!((listed(), read_days()), (25, vec![25, 25, 3]));

        // An entry that is another competition's day by now is not written to, and neither is anything else.
        session.put(days[1] + layout.registration_competition, &7u32.to_le_bytes());
        assert!(session.write_patches(&plan.patches).is_err());
        assert_eq!((listed(), read_days()), (25, vec![25, 25, 3]));
        std::hint::black_box(&memory);
    }
}
