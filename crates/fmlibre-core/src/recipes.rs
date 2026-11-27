//! Recipes: named sets of edits to one player or one club, such as "Make a wonderkid", worked out as one plan from
//! the values the game holds now and written as one batch.
//!
//! A recipe is data: a list of [`Step`]s over the fields the editor already writes (see [`crate::edit`]), plus the two
//! changes to the game's own lists that have a plan of their own, healing ([`Step::Heal`]) and keeping a player from
//! retiring ([`Step::KeepPlaying`]). A new recipe is a new entry of [`RECIPES`], and its name in the app's
//! dictionaries. Values are written here as the game shows them: an attribute from 1 to 20, money in pounds.
//!
//! No step makes a value worse than it is: every one raises a value to a floor or lowers one to a ceiling where lower
//! is better, and leaves a value that is past it already as it is. A plan says which values change, which stay
//! because they are as good already, which are left alone and why, and which of the steps on the game's lists have
//! nothing to do.

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::clubs::ClubSummary;
use crate::coded::Coded;
use crate::date::FmDate;
use crate::details::ATTRIBUTE_GROUPS;
use crate::edit::{EditError, Entity, Field, Scale, Target};
use crate::health::Lock;
use crate::mass::{Operation, Planned, Skipped, Verdict};
use crate::medical::HealPlan;
use crate::players::{PLAYABLE_RATING, PlayerSummary};
use crate::retirement::RetirementPlan;
use crate::session::Session;
use crate::transfer::MoveError;

/// A named set of edits to one player or one club.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    /// What the app calls it by, and names it from: `wonderkid`.
    pub key: &'static str,
    pub entity: Entity,
    /// The oldest player it is for, if age matters to it: an older one is refused, and so is one whose age is not
    /// known.
    pub oldest: Option<u16>,
    #[serde(skip)]
    pub steps: &'static [Step],
}

/// One thing a recipe does.
#[derive(Debug, Clone, Copy)]
pub enum Step {
    /// Changes one field, or every attribute of a group, for those it is for.
    Fields { fields: Fields, who: Who, change: Change },
    /// Heals every current injury, as Heal does.
    Heal,
    /// Takes the retirement the game plans for him out of its plans, and gives him the trait "Will play football as
    /// long as possible" in place of "Will retire at top", as Prevent retirement does.
    KeepPlaying,
}

/// The fields a step changes.
#[derive(Debug, Clone, Copy)]
pub enum Fields {
    /// The field with this key, such as `potential_ability` or `attribute.stamina`.
    One(&'static str),
    /// Every attribute of this group of [`ATTRIBUTE_GROUPS`], such as `technical`.
    Group(&'static str),
}

/// The players a step is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Anyone,
    /// Those the game plays in goal: rated [`PLAYABLE_RATING`] or more there.
    Goalkeepers,
    /// Everyone else.
    Outfield,
}

/// What a step does to a value, written as the game shows it. None of them makes a value worse.
#[derive(Debug, Clone, Copy)]
pub enum Change {
    /// Raises it to this, and leaves a higher one as it is.
    AtLeast(i64),
    /// Lowers it to this, and leaves a lower one as it is: for a value where lower is better.
    AtMost(i64),
    /// Raises it to what the player's age asks for: the value beside the first age in the list that he is not older
    /// than, and the last one's for anyone older.
    AtLeastByAge(&'static [(u16, i64)]),
    /// A date made to run until this many years after the game's date at least, by whole years (see
    /// [`Operation::AtLeastByYears`]).
    YearsAhead(i64),
}

const fn one(key: &'static str, change: Change) -> Step {
    Step::Fields { fields: Fields::One(key), who: Who::Anyone, change }
}

const fn group(name: &'static str, who: Who, change: Change) -> Step {
    Step::Fields { fields: Fields::Group(name), who, change }
}

/// The oldest player the game's press calls a wonderkid.
const WONDERKID_AGE: u16 = 21;

/// A wonderkid's current ability by age: good enough to play now, with room to grow into his potential.
const WONDERKID_ABILITY: Change =
    Change::AtLeastByAge(&[(15, 110), (16, 120), (17, 130), (18, 140), (19, 150), (20, 155), (21, 160)]);

/// The least a wonderkid's technical (or goalkeeping), mental and physical attributes are, by age: about what a
/// player of that current ability has across the board. The game grows them on from there.
const WONDERKID_ATTRIBUTES: Change = Change::AtLeastByAge(&[(15, 10), (16, 11), (18, 12), (20, 13), (21, 14)]);

/// Every recipe, in the order the app offers them.
pub const RECIPES: &[Recipe] = &[
    // A player of 21 or younger: world-class potential, an ability and attributes that fit his age, and the hidden
    // attributes and personality that let him get there. His positions, feet, traits, contract and reputation stay.
    Recipe {
        key: "wonderkid",
        entity: Entity::Player,
        oldest: Some(WONDERKID_AGE),
        steps: &[
            one("potential_ability", Change::AtLeast(180)),
            one("current_ability", WONDERKID_ABILITY),
            group("technical", Who::Outfield, WONDERKID_ATTRIBUTES),
            group("goalkeeping", Who::Goalkeepers, WONDERKID_ATTRIBUTES),
            group("mental", Who::Anyone, WONDERKID_ATTRIBUTES),
            group("physical", Who::Anyone, WONDERKID_ATTRIBUTES),
            // After the groups: a later step for a field takes the place of an earlier one.
            one("attribute.determination", Change::AtLeast(16)),
            one("attribute.consistency", Change::AtLeast(15)),
            one("attribute.important_matches", Change::AtLeast(15)),
            one("attribute.injury_proneness", Change::AtMost(6)),
            one("personality.ambition", Change::AtLeast(16)),
            one("personality.professionalism", Change::AtLeast(17)),
            one("personality.pressure", Change::AtLeast(15)),
        ],
    },
    // As hard to injure as the game lets a player be, and over the injuries he has now.
    Recipe {
        key: "injuryProof",
        entity: Entity::Player,
        oldest: None,
        steps: &[
            Step::Heal,
            one("attribute.injury_proneness", Change::AtMost(1)),
            one("attribute.natural_fitness", Change::AtLeast(20)),
            one("attribute.stamina", Change::AtLeast(15)),
        ],
    },
    // He plays on: no retirement planned, a contract that runs three more years at least, and the two values that
    // slow a player's decline. It does not hold his attributes: Freeze attributes does that while the editor is open.
    Recipe {
        key: "longCareer",
        entity: Entity::Player,
        oldest: None,
        steps: &[
            Step::KeepPlaying,
            one(crate::edit::CONTRACT_EXPIRES, Change::YearsAhead(3)),
            one("attribute.natural_fitness", Change::AtLeast(20)),
            one("personality.professionalism", Change::AtLeast(18)),
        ],
    },
    // Money for anything: the balance, the transfer budget and both wage budgets (pounds, a week for the wages).
    Recipe {
        key: "richClub",
        entity: Entity::Club,
        oldest: None,
        steps: &[
            one("finance.balance", Change::AtLeast(500_000_000)),
            one("finance.transfer_budget", Change::AtLeast(200_000_000)),
            one("finance.wage_budget", Change::AtLeast(6_000_000)),
            one("finance.total_wage_budget", Change::AtLeast(6_000_000)),
        ],
    },
    // Every facility at its best.
    Recipe {
        key: "topFacilities",
        entity: Entity::Club,
        oldest: None,
        steps: &[
            one("facilities.training", Change::AtLeast(20)),
            one("facilities.youth_facilities", Change::AtLeast(20)),
            one("facilities.youth_coaching", Change::AtLeast(20)),
            one("facilities.youth_recruitment", Change::AtLeast(20)),
            one("facilities.corporate", Change::AtLeast(20)),
        ],
    },
];

/// The recipe called `key`.
pub fn recipe(key: &str) -> Result<&'static Recipe, RecipeError> {
    RECIPES.iter().find(|recipe| recipe.key == key).ok_or_else(|| RecipeError::Unknown { key: key.to_owned() })
}

/// The player or club a recipe is applied to, as the lists have it.
#[derive(Debug, Clone, Copy)]
pub enum Subject<'a> {
    Player(&'a PlayerSummary),
    Club(&'a ClubSummary),
}

impl Subject<'_> {
    fn target(&self) -> Target {
        match self {
            Subject::Player(player) => player.target(),
            Subject::Club(club) => club.target(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Subject::Player(player) => &player.name,
            Subject::Club(club) => &club.name,
        }
    }
}

/// What a recipe's steps go by: who the player is, and the game's date.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Facts {
    age: Option<u16>,
    goalkeeper: bool,
    today: Option<FmDate>,
}

impl Who {
    fn includes(self, facts: &Facts) -> bool {
        match self {
            Who::Anyone => true,
            Who::Goalkeepers => facts.goalkeeper,
            Who::Outfield => !facts.goalkeeper,
        }
    }
}

impl Change {
    /// The operation the change is for `field` and the one `facts` describe, with its value as the field stores it.
    fn operation(self, field: &Field, facts: &Facts) -> Result<Operation, EditError> {
        let unsupported = |reason| EditError::Unsupported { field: field.key.clone(), reason };
        Ok(match self {
            Change::AtLeast(shown) => Operation::AtLeast { value: field.scale.stored(shown) },
            Change::AtMost(shown) => Operation::AtMost { value: field.scale.stored(shown) },
            Change::AtLeastByAge(by_age) => {
                let age = facts.age.ok_or_else(|| unsupported("the player's age is not known"))?;
                let shown =
                    by_age.iter().find(|&&(oldest, _)| age <= oldest).or(by_age.last()).map(|&(_, value)| value);
                let shown = shown.ok_or_else(|| unsupported("the recipe has no value for it"))?;
                Operation::AtLeast { value: field.scale.stored(shown) }
            }
            Change::YearsAhead(years) => {
                let day = facts.today.and_then(|today| today.plus_years(years));
                let day = day.ok_or_else(|| unsupported("the in-game date is not known"))?;
                Operation::AtLeastByYears { value: day.to_number() }
            }
        })
    }
}

/// A field a step names that cannot be worked out, by its key, and why.
type Unplanned = (String, EditError);

impl Recipe {
    /// The operations the recipe's field steps come to for the one `facts` describe, on `fields`, the fields of its
    /// kind of thing: one for each field, in the order the steps first name them, a later step for a field taking the
    /// place of an earlier one. With them, the fields a step names that cannot be worked out, and why: one this game's
    /// layout does not have, or a change that needs a fact that is not known.
    fn operations(&self, fields: &[Field], facts: &Facts) -> (Vec<(Field, Operation)>, Vec<Unplanned>) {
        let mut operations: Vec<(Field, Operation)> = Vec::new();
        let mut left: Vec<Unplanned> = Vec::new();
        for step in self.steps {
            let Step::Fields { fields: named, who, change } = *step else {
                continue;
            };
            if !who.includes(facts) {
                continue;
            }
            let keys: Vec<String> = match named {
                Fields::One(key) => vec![key.to_owned()],
                Fields::Group(name) => ATTRIBUTE_GROUPS
                    .iter()
                    .filter(|(group, _)| *group == name)
                    .flat_map(|(_, keys)| keys.iter().map(|key| format!("attribute.{key}")))
                    // A group's attribute this layout does not place is not one of the game's here.
                    .filter(|key| fields.iter().any(|field| field.key == *key))
                    .collect(),
            };
            for key in keys {
                operations.retain(|(field, _)| field.key != key);
                left.retain(|(field, _)| *field != key);
                let Some(field) = fields.iter().find(|field| field.key == key) else {
                    left.push((key.clone(), EditError::UnknownField { entity: self.entity, key }));
                    continue;
                };
                match change.operation(field, facts).and_then(|operation| operation.check(field).map(|()| operation)) {
                    Ok(operation) => operations.push((field.clone(), operation)),
                    Err(err) => left.push((key, err)),
                }
            }
        }
        (operations, left)
    }

    /// Whether the recipe has the step `wanted` (one of those without fields).
    fn has(&self, wanted: fn(&Step) -> bool) -> bool {
        self.steps.iter().any(wanted)
    }
}

/// A value a recipe leaves as it is because it is as good already, or better.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kept {
    pub field: String,
    /// As stored.
    pub value: i64,
}

/// A step on the game's lists that a recipe leaves out, and why: the reason in English, and its code with the values
/// it names, so the app can say it in another language.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeftOut {
    /// `heal` or `keepPlaying`.
    pub step: &'static str,
    pub reason: String,
    pub code: &'static str,
    pub params: BTreeMap<&'static str, String>,
}

/// A recipe worked out for one player or club from the values the game holds now, ready to be written.
#[derive(Debug, Clone)]
pub struct RecipePlan {
    pub recipe: &'static Recipe,
    pub uid: u32,
    pub name: String,
    /// The values that change.
    pub planned: Vec<Planned>,
    /// The values that stay, as good already as the recipe would make them.
    pub kept: Vec<Kept>,
    /// The values left alone, and why.
    pub skipped: Vec<Skipped>,
    /// Healing his injuries, where he has any.
    pub heal: Option<HealPlan>,
    /// Keeping him from retiring, where there is something to write for it.
    pub retirement: Option<RetirementPlan>,
    /// The steps on the game's lists that have nothing to do, or cannot run.
    pub left: Vec<LeftOut>,
}

impl RecipePlan {
    /// How many writes applying the plan logs: one for each value, and one for each step on the game's lists.
    pub fn changes(&self) -> usize {
        self.planned.len() + usize::from(self.heal.is_some()) + usize::from(self.retirement.is_some())
    }
}

#[derive(Debug)]
pub enum RecipeError {
    Unknown {
        key: String,
    },
    /// A player's recipe asked for a club, or the other way round.
    WrongEntity {
        recipe: &'static str,
        entity: Entity,
    },
    /// The recipe is for younger players.
    TooOld {
        recipe: &'static str,
        age: u16,
        oldest: u16,
    },
    /// The recipe goes by the player's age, which is not known.
    AgeUnknown {
        recipe: &'static str,
    },
    /// The editor writes nothing to this game now (see [`Lock`]).
    Locked(Lock),
    Edit(EditError),
}

impl fmt::Display for RecipeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecipeError::Unknown { key } => write!(f, "there is no recipe {key}"),
            RecipeError::WrongEntity { recipe, entity } => write!(f, "the recipe {recipe} is not for a {entity}"),
            RecipeError::TooOld { age, oldest, .. } => {
                write!(f, "the player is {age}: this recipe is for players of {oldest} or younger")
            }
            RecipeError::AgeUnknown { .. } => f.write_str("the player's age is not known, and this recipe goes by it"),
            RecipeError::Locked(lock) => write!(f, "{lock}"),
            RecipeError::Edit(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RecipeError {}

impl Coded for RecipeError {
    fn code(&self) -> &'static str {
        match self {
            RecipeError::Unknown { .. } => "recipe.unknown",
            RecipeError::WrongEntity { .. } => "recipe.wrongEntity",
            RecipeError::TooOld { .. } => "recipe.tooOld",
            RecipeError::AgeUnknown { .. } => "recipe.ageUnknown",
            RecipeError::Locked(lock) => lock.code(),
            RecipeError::Edit(err) => err.code(),
        }
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            RecipeError::Unknown { key } => vec![("recipe", key.clone())],
            RecipeError::WrongEntity { recipe, entity } => {
                vec![("recipe", (*recipe).to_owned()), ("entity", entity.to_string())]
            }
            RecipeError::TooOld { recipe, age, oldest } => {
                vec![("recipe", (*recipe).to_owned()), ("age", age.to_string()), ("oldest", oldest.to_string())]
            }
            RecipeError::AgeUnknown { recipe } => vec![("recipe", (*recipe).to_owned())],
            RecipeError::Locked(lock) => lock.params(),
            RecipeError::Edit(err) => err.params(),
        }
    }
}

impl From<EditError> for RecipeError {
    fn from(err: EditError) -> Self {
        RecipeError::Edit(err)
    }
}

/// What a field step's operation comes to for one value (see [`settle`]).
enum Settled {
    Change { old: i64, new: i64 },
    Kept(i64),
    Skip(EditError),
}

/// What `operation` makes of `value`, read from `field`, for a recipe: as a mass edit judges it, but a value the game
/// already shows as the recipe would have it stays. An attribute is stored on a finer scale than the 1 to 20 the game
/// shows, so one stored as 98 is a 20 already, and is not written to 100.
fn settle(field: &Field, operation: Operation, value: Option<i64>) -> Settled {
    match operation.judge(field, value) {
        Verdict::Change { old, new }
            if field.scale != Scale::Plain && field.scale.shown(old) == field.scale.shown(new) =>
        {
            Settled::Kept(old)
        }
        Verdict::Change { old, new } => Settled::Change { old, new },
        Verdict::Same { value } => Settled::Kept(value),
        Verdict::Skip(err) => Settled::Skip(err),
    }
}

impl Session {
    /// Works out `recipe` for `subject` from the values the game holds now. Nothing is written.
    ///
    /// Refused as a whole for someone it is not for (see [`Recipe::oldest`]), and while the editor writes nothing to
    /// this game: a build it does not know, or a save loaded since it read the lists. A field whose own check did not
    /// pass is left alone and said, like one whose record is missing, and the rest is still planned.
    pub fn plan_recipe(&self, recipe: &'static Recipe, subject: Subject<'_>) -> Result<RecipePlan, RecipeError> {
        let target = subject.target();
        if target.entity != recipe.entity {
            return Err(RecipeError::WrongEntity { recipe: recipe.key, entity: target.entity });
        }
        let mut facts = Facts { today: self.game_date(), ..Facts::default() };
        if let Subject::Player(player) = subject {
            facts.age = player.age;
            facts.goalkeeper = player.position_ratings[0] >= PLAYABLE_RATING;
        }
        if let Some(oldest) = recipe.oldest {
            match facts.age {
                None => return Err(RecipeError::AgeUnknown { recipe: recipe.key }),
                Some(age) if age > oldest => return Err(RecipeError::TooOld { recipe: recipe.key, age, oldest }),
                Some(_) => {}
            }
        }
        let name = subject.name().to_owned();
        let skip = |field: &str, err: &EditError| Skipped {
            uid: target.uid,
            name: name.clone(),
            field: field.to_owned(),
            reason: err.to_string(),
            code: err.code(),
            params: err.params().into_iter().collect(),
        };
        let (operations, unknown) = recipe.operations(&self.fields(recipe.entity), &facts);
        let mut skipped: Vec<Skipped> = unknown.iter().map(|(field, err)| skip(field, err)).collect();
        // What keeps the editor from writing anything refuses the recipe; a check of one part of the game that did
        // not pass only leaves that part's fields alone.
        let mut open = Vec::with_capacity(operations.len());
        for (field, operation) in operations {
            match self.field_lock(&field) {
                Some(lock @ (Lock::UnknownBuild | Lock::SaveChanged)) => return Err(RecipeError::Locked(lock)),
                Some(lock) => skipped.push(skip(&field.key, &EditError::Locked { field: field.key.clone(), lock })),
                None => open.push((field, operation)),
            }
        }
        let fields: Vec<Field> = open.iter().map(|(field, _)| field.clone()).collect();
        let values = self.read_fields(&target, &fields)?;
        let (mut planned, mut kept) = (Vec::new(), Vec::new());
        for ((field, operation), value) in open.iter().zip(values) {
            match settle(field, *operation, value) {
                Settled::Change { old, new } => planned.push(Planned {
                    target,
                    uid: target.uid,
                    name: name.clone(),
                    field: field.key.clone(),
                    old,
                    new,
                }),
                Settled::Kept(value) => kept.push(Kept { field: field.key.clone(), value }),
                Settled::Skip(err) => skipped.push(skip(&field.key, &err)),
            }
        }

        let (mut heal, mut retirement, mut left) = (None, None, Vec::new());
        let mut leave = |step: &'static str, err: &MoveError| {
            left.push(LeftOut {
                step,
                reason: err.to_string(),
                code: err.code(),
                params: err.params().into_iter().collect(),
            });
        };
        if let Subject::Player(player) = subject {
            // Both change the game's own lists, which asks more of the game's checks than a field does.
            let locked = || self.structure_lock().map_or(Ok(()), |lock| Err(MoveError::Locked(lock)));
            if recipe.has(|step| matches!(step, Step::Heal)) {
                match locked().and_then(|()| self.plan_heal(player)) {
                    Ok(plan) => heal = Some(plan),
                    Err(err) => leave("heal", &err),
                }
            }
            if recipe.has(|step| matches!(step, Step::KeepPlaying)) {
                match locked().and_then(|()| self.plan_keep_playing(player)) {
                    Ok(plan) => retirement = Some(plan),
                    Err(err) => leave("keepPlaying", &err),
                }
            }
        }
        Ok(RecipePlan { recipe, uid: target.uid, name, planned, kept, skipped, heal, retirement, left })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::fields;

    fn all(entity: Entity) -> Vec<Field> {
        fields(&crate::layout_for(27).expect("FM27 layout"), entity)
    }

    fn named(key: &str) -> &'static Recipe {
        recipe(key).expect(key)
    }

    fn facts(age: u16, goalkeeper: bool) -> Facts {
        Facts { age: Some(age), goalkeeper, today: FmDate::from_number(20260718) }
    }

    fn operation(operations: &[(Field, Operation)], key: &str) -> Option<Operation> {
        operations.iter().find(|(field, _)| field.key == key).map(|&(_, operation)| operation)
    }

    #[test]
    fn every_recipe_is_made_of_fields_the_editor_writes() {
        let mut keys = Vec::new();
        for recipe in RECIPES {
            assert!(!keys.contains(&recipe.key), "{} is there twice", recipe.key);
            keys.push(recipe.key);
            assert!(!recipe.steps.is_empty(), "{}", recipe.key);
            let table = all(recipe.entity);
            // For a goalkeeper and for an outfield player, old and young: every step names fields the layout has, and
            // every operation is one its field takes.
            for goalkeeper in [false, true] {
                for age in [15, recipe.oldest.unwrap_or(40)] {
                    let (operations, left) = recipe.operations(&table, &facts(age, goalkeeper));
                    assert!(left.is_empty(), "{}: {left:?}", recipe.key);
                    assert!(!operations.is_empty(), "{}", recipe.key);
                    for (field, operation) in &operations {
                        assert_eq!(field.entity, recipe.entity);
                        assert!(field.mass, "{}: {} is changed one at a time", recipe.key, field.key);
                        assert!(operation.check(field).is_ok(), "{}: {}", recipe.key, field.key);
                    }
                    let mut named: Vec<&str> = operations.iter().map(|(field, _)| field.key.as_str()).collect();
                    named.sort_unstable();
                    named.dedup();
                    assert_eq!(named.len(), operations.len(), "{}: a field is there twice", recipe.key);
                }
            }
            // The steps on the game's lists are a player's.
            let lists = recipe.has(|step| matches!(step, Step::Heal | Step::KeepPlaying));
            assert!(!lists || recipe.entity == Entity::Player, "{}", recipe.key);
        }
    }

    #[test]
    fn no_recipe_makes_a_value_worse() {
        // A ceiling only where lower is better; everything else is a floor.
        const LOWER_IS_BETTER: [&str; 1] = ["attribute.injury_proneness"];
        for recipe in RECIPES {
            let (operations, _) = recipe.operations(&all(recipe.entity), &facts(18, false));
            for (field, operation) in operations {
                let lower = LOWER_IS_BETTER.contains(&field.key.as_str());
                match operation {
                    Operation::AtLeast { .. } | Operation::AtLeastByYears { .. } => {
                        assert!(!lower, "{}: {}", recipe.key, field.key)
                    }
                    Operation::AtMost { .. } => assert!(lower, "{}: {}", recipe.key, field.key),
                    other => panic!("{}: {} is {other:?}", recipe.key, field.key),
                }
                // Whatever the value is, it only ever moves the good way.
                for old in [field.min, (field.min + field.max) / 2, field.max] {
                    let new = operation.apply(&field, old);
                    assert!(
                        if lower { new <= old } else { new >= old },
                        "{}: {} {old} -> {new}",
                        recipe.key,
                        field.key
                    );
                }
            }
        }
    }

    #[test]
    fn a_wonderkid_gets_what_his_age_asks_for() {
        let wonderkid = named("wonderkid");
        let table = all(Entity::Player);
        let (young, _) = wonderkid.operations(&table, &facts(16, false));
        let (older, _) = wonderkid.operations(&table, &facts(21, false));
        assert_eq!(operation(&young, "potential_ability"), Some(Operation::AtLeast { value: 180 }));
        assert_eq!(operation(&young, "current_ability"), Some(Operation::AtLeast { value: 120 }));
        assert_eq!(operation(&older, "current_ability"), Some(Operation::AtLeast { value: 160 }));
        // Attributes are stored five to a point: 11 is 55, 14 is 70.
        assert_eq!(operation(&young, "attribute.passing"), Some(Operation::AtLeast { value: 55 }));
        assert_eq!(operation(&older, "attribute.passing"), Some(Operation::AtLeast { value: 70 }));
        // The step for determination comes after its group's, and takes its place.
        assert_eq!(operation(&young, "attribute.determination"), Some(Operation::AtLeast { value: 80 }));
        assert_eq!(operation(&young, "attribute.injury_proneness"), Some(Operation::AtMost { value: 30 }));
        assert_eq!(operation(&young, "personality.professionalism"), Some(Operation::AtLeast { value: 17 }));
        // An outfield player keeps his goalkeeping, a goalkeeper his technical attributes; feet, positions, traits and
        // the rest of the hidden attributes are nobody's to change.
        assert_eq!(operation(&young, "attribute.handling"), None);
        let (keeper, _) = wonderkid.operations(&table, &facts(16, true));
        assert_eq!(operation(&keeper, "attribute.handling"), Some(Operation::AtLeast { value: 55 }));
        assert_eq!(operation(&keeper, "attribute.passing"), None);
        assert!(operation(&keeper, "attribute.pace").is_some());
        for untouched in
            ["attribute.left_foot", "attribute.dirtiness", "attribute.versatility", "position.st", "trait.3"]
        {
            assert_eq!(operation(&young, untouched), None, "{untouched}");
        }
        assert_eq!(wonderkid.oldest, Some(21));
    }

    #[test]
    fn a_value_by_age_takes_the_last_one_past_the_list() {
        let ability = all(Entity::Player).into_iter().find(|field| field.key == "current_ability").expect("ability");
        let by_age = Change::AtLeastByAge(&[(15, 110), (17, 130)]);
        let at = |age| by_age.operation(&ability, &facts(age, false)).expect("an operation");
        assert_eq!(at(14), Operation::AtLeast { value: 110 });
        assert_eq!(at(15), Operation::AtLeast { value: 110 });
        assert_eq!(at(16), Operation::AtLeast { value: 130 });
        assert_eq!(at(30), Operation::AtLeast { value: 130 });
        // Without an age there is nothing to go by.
        let unknown = Facts { age: None, ..facts(0, false) };
        assert!(matches!(by_age.operation(&ability, &unknown), Err(EditError::Unsupported { .. })));
    }

    #[test]
    fn a_long_career_runs_three_years_on_from_the_game_s_date() {
        let career = named("longCareer");
        let table = all(Entity::Player);
        let (operations, left) = career.operations(&table, &facts(34, false));
        assert!(left.is_empty());
        let expires = operation(&operations, "contract.expires").expect("the contract's end");
        assert_eq!(expires, Operation::AtLeastByYears { value: 20290718 });
        let field = table.iter().find(|field| field.key == "contract.expires").expect("the field");
        // A contract to 30 June 2027 runs to 30 June 2030; one that runs longer already stays.
        assert!(matches!(settle(field, expires, Some(20270630)), Settled::Change { old: 20270630, new: 20300630 }));
        assert!(matches!(settle(field, expires, Some(20310630)), Settled::Kept(20310630)));
        // A free agent has no contract, and a rolling one no end.
        assert!(matches!(settle(field, expires, None), Settled::Skip(EditError::NoRecord { .. })));
        assert!(matches!(settle(field, expires, Some(0)), Settled::Skip(EditError::Unset { .. })));
        assert!(career.has(|step| matches!(step, Step::KeepPlaying)));
        // Without the game's date the contract is left alone, and the rest still planned.
        let dateless = Facts { today: None, ..facts(34, false) };
        let (operations, left) = career.operations(&table, &dateless);
        assert_eq!(operation(&operations, "contract.expires"), None);
        assert!(operation(&operations, "attribute.natural_fitness").is_some());
        assert!(matches!(left.as_slice(), [(key, EditError::Unsupported { .. })] if key == "contract.expires"));
    }

    #[test]
    fn a_value_the_game_shows_as_asked_already_stays() {
        let table = all(Entity::Player);
        let fitness = table.iter().find(|field| field.key == "attribute.natural_fitness").expect("natural fitness");
        let best = Operation::AtLeast { value: fitness.scale.stored(20) };
        // 98 shows as 20 already; 97 shows as 19.
        assert!(matches!(settle(fitness, best, Some(98)), Settled::Kept(98)));
        assert!(matches!(settle(fitness, best, Some(97)), Settled::Change { old: 97, new: 100 }));
        assert!(matches!(settle(fitness, best, Some(100)), Settled::Kept(100)));
        let proneness = table.iter().find(|field| field.key == "attribute.injury_proneness").expect("proneness");
        let least = Operation::AtMost { value: proneness.scale.stored(1) };
        assert!(matches!(settle(proneness, least, Some(7)), Settled::Kept(7)));
        assert!(matches!(settle(proneness, least, Some(8)), Settled::Change { old: 8, new: 5 }));
        assert!(matches!(settle(proneness, least, Some(3)), Settled::Kept(3)));
        // Money is shown as stored: a pound more is a change.
        let clubs = all(Entity::Club);
        let balance = clubs.iter().find(|field| field.key == "finance.balance").expect("balance");
        let rich = Operation::AtLeast { value: 500_000_000 };
        assert!(matches!(settle(balance, rich, Some(499_999_999)), Settled::Change { new: 500_000_000, .. }));
        assert!(matches!(settle(balance, rich, Some(-20_000_000)), Settled::Change { new: 500_000_000, .. }));
        assert!(matches!(settle(balance, rich, Some(900_000_000)), Settled::Kept(900_000_000)));
        assert!(matches!(settle(balance, rich, None), Settled::Skip(EditError::NoRecord { .. })));
    }

    #[test]
    fn a_field_the_layout_lacks_is_left_alone_and_said() {
        const ODD: Recipe = Recipe {
            key: "odd",
            entity: Entity::Club,
            oldest: None,
            steps: &[one("finance.balance", Change::AtLeast(1)), one("finance.nothing", Change::AtLeast(1))],
        };
        let (operations, left) = ODD.operations(&all(Entity::Club), &Facts::default());
        assert_eq!(operations.len(), 1);
        assert!(matches!(left.as_slice(), [(key, EditError::UnknownField { .. })] if key == "finance.nothing"));
    }

    #[test]
    fn recipes_are_found_by_key_and_tell_what_went_wrong() {
        assert_eq!(named("richClub").entity, Entity::Club);
        let unknown = recipe("nothing").expect_err("no such recipe");
        assert_eq!(unknown.code(), "recipe.unknown");
        let old = RecipeError::TooOld { recipe: "wonderkid", age: 29, oldest: 21 };
        assert_eq!(old.code(), "recipe.tooOld");
        assert_eq!(
            old.params(),
            vec![("recipe", "wonderkid".to_owned()), ("age", "29".into()), ("oldest", "21".into())]
        );
        assert_eq!(RecipeError::Locked(Lock::SaveChanged).code(), "guard.saveChanged");
        // The app lists them by kind of thing.
        let players = RECIPES.iter().filter(|recipe| recipe.entity == Entity::Player).count();
        assert_eq!((players, RECIPES.len() - players), (3, 2));
    }
}
