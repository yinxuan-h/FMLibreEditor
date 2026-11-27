//! Changing fields of many players, staff or clubs at once. A plan is worked out from
//! the values the game holds now, shown as a preview, and then written change by
//! change, each only while the game still holds the value it was planned from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::coded::Coded;
use crate::date::FmDate;
use crate::edit::{EditError, Field, Kind, Target};
use crate::session::Session;

/// The most years an Add years moves a date, either way; also the most a contract is extended by at once.
pub const MOST_YEARS: i64 = 50;

/// What to do with a field's value. Values are as stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Operation {
    /// Set it to `value`.
    Set { value: i64 },
    /// Add `amount`, which may be negative; the result stays within the field's range.
    Add { amount: i64 },
    /// Multiply it by `percent` / 100, rounded; the result stays within the field's range.
    Scale { percent: i64 },
    /// Move a date by `years` whole years (back, for a negative count), keeping its day: 29 February becomes 28
    /// February in a year that has none. Only for dates, at most [`MOST_YEARS`] either way; a date it would take out
    /// of the field's range is left as it is.
    AddYears { years: i64 },
    /// Lower it to `value`, and leave one that is lower already as it is.
    AtMost { value: i64 },
    /// Raise it to `value`, and leave one that is higher already as it is.
    AtLeast { value: i64 },
    /// Move a date on by whole years, keeping its day as Add years does, until it is the day `value` or a later one;
    /// a date that is that late already stays as it is. Only for dates: a contract made to run until a day at least
    /// still ends on the day of the year it ended on. A date that [`MOST_YEARS`] do not bring there is left as it is.
    AtLeastByYears { value: i64 },
}

impl Operation {
    /// Whether the operation suits `field`: dates can only be set or moved by years, only dates are moved by years, and
    /// a value set must be one the field takes.
    pub fn check(self, field: &Field) -> Result<(), EditError> {
        match self {
            _ if !field.mass => {
                Err(EditError::Unsupported { field: field.key.clone(), reason: "it is changed one person at a time" })
            }
            Operation::Set { value }
            | Operation::AtMost { value }
            | Operation::AtLeast { value }
            | Operation::AtLeastByYears { value }
                if !field.takes(value) =>
            {
                Err(EditError::OutOfRange { field: field.key.clone(), value, min: field.min, max: field.max })
            }
            Operation::Add { .. } | Operation::Scale { .. } if field.kind == Kind::Date => {
                Err(EditError::Unsupported {
                    field: field.key.clone(),
                    reason: "a date can only be set or moved by years",
                })
            }
            Operation::AddYears { .. } | Operation::AtLeastByYears { .. } if field.kind != Kind::Date => {
                Err(EditError::Unsupported { field: field.key.clone(), reason: "only a date can be moved by years" })
            }
            Operation::AddYears { years } if !(-MOST_YEARS..=MOST_YEARS).contains(&years) => {
                Err(EditError::OutOfRange { field: field.key.clone(), value: years, min: -MOST_YEARS, max: MOST_YEARS })
            }
            _ => Ok(()),
        }
    }

    /// The value the operation turns `old` into. A date moved by years may leave the field's range, which the plan
    /// checks; a value that is not a date stays as it is.
    pub fn apply(self, field: &Field, old: i64) -> i64 {
        match self {
            Operation::Set { value } => value,
            Operation::Add { amount } => old.saturating_add(amount).clamp(field.min, field.max),
            Operation::Scale { percent } => {
                let scaled = (old as f64 * percent as f64 / 100.0).round() as i64;
                scaled.clamp(field.min, field.max)
            }
            Operation::AddYears { years } => {
                FmDate::from_number(old).and_then(|date| date.plus_years(years)).map_or(old, FmDate::to_number)
            }
            Operation::AtMost { value } => old.min(value),
            Operation::AtLeast { value } => old.max(value),
            Operation::AtLeastByYears { value } => FmDate::from_number(old)
                .and_then(|date| {
                    (0..=MOST_YEARS).filter_map(|years| date.plus_years(years)).find(|late| late.to_number() >= value)
                })
                .map_or(old, FmDate::to_number),
        }
    }

    /// What the operation makes of `value`, read from `field` of someone: the change it plans, nothing where the value
    /// already is what it would make it, or why it leaves the field alone. No value is a field whose record he lacks
    /// (no contract, a club without full finances).
    pub(crate) fn judge(self, field: &Field, value: Option<i64>) -> Verdict {
        let Some(old) = value else {
            return Verdict::Skip(EditError::NoRecord { field: field.key.clone() });
        };
        if !field.is_plausible(old) {
            return Verdict::Skip(EditError::Implausible { field: field.key.clone(), value: old });
        }
        if field.kind == Kind::Date && old == 0 {
            return Verdict::Skip(EditError::Unset { field: field.key.clone() });
        }
        let new = self.apply(field, old);
        if new == old {
            return Verdict::Same { value: old };
        }
        // A date moved by years can leave the days the field takes, such as a contract's end past 2100.
        if !field.takes(new) {
            let (min, max) = (field.min, field.max);
            return Verdict::Skip(EditError::OutOfRange { field: field.key.clone(), value: new, min, max });
        }
        Verdict::Change { old, new }
    }
}

/// What an operation makes of one value (see [`Operation::judge`]).
#[derive(Debug)]
pub(crate) enum Verdict {
    Change { old: i64, new: i64 },
    Same { value: i64 },
    Skip(EditError),
}

/// One change of a plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Planned {
    #[serde(skip)]
    pub target: Target,
    pub uid: u32,
    pub name: String,
    pub field: String,
    pub old: i64,
    pub new: i64,
}

/// A field of a target that the plan leaves alone, and why: the reason in English, and
/// its code with the values it names, so the app can say it in another language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub uid: u32,
    pub name: String,
    pub field: String,
    pub reason: String,
    pub code: &'static str,
    pub params: BTreeMap<&'static str, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub planned: Vec<Planned>,
    pub skipped: Vec<Skipped>,
    /// Targets whose values already are what the operations would make them.
    pub unchanged: usize,
}

impl Session {
    /// Works out what `operations` would change for each of `targets` (with their
    /// names), from the values the game holds now. Nothing is written.
    pub fn plan_mass_edit(
        &self,
        targets: &[(Target, String)],
        operations: &[(Field, Operation)],
    ) -> Result<Plan, EditError> {
        for (field, operation) in operations {
            operation.check(field)?;
            // A field the editor would not write is refused once, not once for every target.
            if let Some(lock) = self.field_lock(field) {
                return Err(EditError::Locked { field: field.key.clone(), lock });
            }
        }
        let fields: Vec<Field> = operations.iter().map(|(field, _)| field.clone()).collect();
        let mut plan = Plan::default();
        for (target, name) in targets {
            let skip = |field: &Field, err: &EditError| Skipped {
                uid: target.uid,
                name: name.clone(),
                field: field.key.clone(),
                reason: err.to_string(),
                code: err.code(),
                params: err.params().into_iter().collect(),
            };
            let values = match self.read_fields(target, &fields) {
                Ok(values) => values,
                Err(err) => {
                    plan.skipped.extend(fields.iter().map(|field| skip(field, &err)));
                    continue;
                }
            };
            let mut changed = false;
            for ((field, operation), value) in operations.iter().zip(values) {
                match operation.judge(field, value) {
                    Verdict::Change { old, new } => {
                        changed = true;
                        plan.planned.push(Planned {
                            target: *target,
                            uid: target.uid,
                            name: name.clone(),
                            field: field.key.clone(),
                            old,
                            new,
                        });
                    }
                    Verdict::Same { .. } => {}
                    Verdict::Skip(err) => plan.skipped.push(skip(field, &err)),
                }
            }
            plan.unchanged += usize::from(!changed);
        }
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Entity, fields};

    fn field(entity: Entity, key: &str) -> Field {
        let layout = crate::layout_for(27).expect("FM27 layout");
        fields(&layout, entity).into_iter().find(|f| f.key == key).expect(key)
    }

    #[test]
    fn operations_stay_in_range() {
        let finishing = field(Entity::Player, "attribute.finishing");
        assert_eq!(Operation::Add { amount: 10 }.apply(&finishing, 95), 100);
        assert_eq!(Operation::Add { amount: -10 }.apply(&finishing, 5), 1);
        assert_eq!(Operation::Scale { percent: 110 }.apply(&finishing, 50), 55);
        assert_eq!(Operation::Scale { percent: 150 }.apply(&finishing, 90), 100);
        let budget = field(Entity::Club, "finance.transfer_budget");
        assert_eq!(Operation::Scale { percent: 200 }.apply(&budget, 24_250_000), 48_500_000);
        assert_eq!(Operation::Set { value: 1_000_000 }.apply(&budget, 24_250_000), 1_000_000);
    }

    #[test]
    fn at_most_and_at_least_never_go_the_other_way() {
        let sharpness = field(Entity::Player, "match_sharpness");
        assert_eq!(Operation::AtMost { value: 3_500 }.apply(&sharpness, 9_200), 3_500);
        // A youth intake below what the editor writes keeps his.
        assert_eq!(Operation::AtMost { value: 3_500 }.apply(&sharpness, 2_100), 2_100);
        let jadedness = field(Entity::Player, "jadedness");
        assert_eq!(Operation::AtLeast { value: 1_000 }.apply(&jadedness, -500), 1_000);
        assert_eq!(Operation::AtLeast { value: 1_000 }.apply(&jadedness, 1_400), 1_400);
        // The value must be one the field takes, as for Set.
        assert!(Operation::AtMost { value: 3_500 }.check(&sharpness).is_ok());
        assert!(Operation::AtMost { value: 0 }.check(&sharpness).is_err());
        assert!(Operation::AtLeast { value: 1_000 }.check(&jadedness).is_ok());
        assert!(Operation::AtLeast { value: 2_000 }.check(&jadedness).is_err());
    }

    #[test]
    fn operations_are_checked_against_the_field() {
        let ca = field(Entity::Player, "current_ability");
        assert!(Operation::Set { value: 150 }.check(&ca).is_ok());
        assert!(Operation::Set { value: 250 }.check(&ca).is_err());
        let expires = field(Entity::Player, "contract.expires");
        assert!(Operation::Set { value: 20300630 }.check(&expires).is_ok());
        assert!(Operation::Add { amount: 10_000 }.check(&expires).is_err());
        assert!(Operation::Scale { percent: 110 }.check(&expires).is_err());
        assert!(Operation::AddYears { years: 5 }.check(&expires).is_ok());
        assert!(Operation::AddYears { years: -MOST_YEARS }.check(&expires).is_ok());
        assert!(Operation::AddYears { years: MOST_YEARS + 1 }.check(&expires).is_err());
        assert!(Operation::AddYears { years: 1 }.check(&ca).is_err());
        let staff_expires = field(Entity::Staff, "contract.expires");
        assert!(Operation::AddYears { years: 5 }.check(&staff_expires).is_ok());
        assert!(Operation::AtLeastByYears { value: 20290718 }.check(&expires).is_ok());
        assert!(Operation::AtLeastByYears { value: 20290718 }.check(&ca).is_err());
        assert!(Operation::AtLeastByYears { value: 21010101 }.check(&expires).is_err());
    }

    #[test]
    fn a_date_runs_on_by_whole_years_until_a_day() {
        let expires = field(Entity::Player, "contract.expires");
        let until = Operation::AtLeastByYears { value: 20290718 };
        // 30 June 2029 is before 18 July 2029: one year more.
        assert_eq!(until.apply(&expires, 20270630), 20300630);
        assert_eq!(until.apply(&expires, 20271231), 20291231);
        // On the day or after it, it stays; and it never comes back.
        assert_eq!(until.apply(&expires, 20290718), 20290718);
        assert_eq!(until.apply(&expires, 20330630), 20330630);
        assert_eq!(until.apply(&expires, 20280229), 20300228);
        // Not a date, or one the most years do not bring there: as it is.
        assert_eq!(until.apply(&expires, 0), 0);
        assert_eq!(until.apply(&expires, 19500630), 19500630);
    }

    #[test]
    fn a_value_is_changed_kept_or_left_alone() {
        let finishing = field(Entity::Player, "attribute.finishing");
        let least = Operation::AtLeast { value: 75 };
        assert!(matches!(least.judge(&finishing, Some(60)), Verdict::Change { old: 60, new: 75 }));
        assert!(matches!(least.judge(&finishing, Some(80)), Verdict::Same { value: 80 }));
        assert!(matches!(least.judge(&finishing, None), Verdict::Skip(EditError::NoRecord { .. })));
        assert!(matches!(least.judge(&finishing, Some(140)), Verdict::Skip(EditError::Implausible { .. })));
        let expires = field(Entity::Player, "contract.expires");
        let years = Operation::AddYears { years: 5 };
        assert!(matches!(years.judge(&expires, Some(0)), Verdict::Skip(EditError::Unset { .. })));
        assert!(matches!(years.judge(&expires, Some(20990630)), Verdict::Skip(EditError::OutOfRange { .. })));
    }

    #[test]
    fn years_move_a_date_and_keep_its_day() {
        let expires = field(Entity::Player, "contract.expires");
        assert_eq!(Operation::AddYears { years: 5 }.apply(&expires, 20280630), 20330630);
        assert_eq!(Operation::AddYears { years: -1 }.apply(&expires, 20280630), 20270630);
        assert_eq!(Operation::AddYears { years: 1 }.apply(&expires, 20280229), 20290228);
        // A date past the field's range is the plan's to leave alone; one that is no date stays as it is.
        let past = Operation::AddYears { years: 5 }.apply(&expires, 20990630);
        assert_eq!(past, 21040630);
        assert!(!expires.takes(past));
        assert_eq!(Operation::AddYears { years: 5 }.apply(&expires, 0), 0);
    }
}
