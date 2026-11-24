//! The game's records: how it keeps what it reads from its rule files (see docs/recon/fm27-league-rules.md).
//!
//! A record is a list `(begin, end, capacity)` of 16-byte entries: a key of four characters as a `u32` ('sqsr', the
//! first character in the highest byte), four spare bytes, and a pointer to a typed value. A typed value is 16 bytes
//! too: a pointer to the object that says what type it is (its traits, one object per type, told apart by its class)
//! and 8 bytes of value. An integer keeps its value in the low four of them and leaves whatever was there in the
//! rest; a record or a list as a value is a pointer to its own `(begin, end, capacity)`; a list's elements are pointers
//! to typed values.

use std::cell::RefCell;
use std::collections::HashMap;

use fmlibre_mem::{Module, Process, is_user_pointer};

use crate::rtti;

/// The most entries a record is taken to have: the fullest seen, a competition's own, has 49.
const MOST_ENTRIES: u64 = 512;

/// The most elements a list is taken to have: a nation's list of the teams it needs has a few dozen.
const MOST_ELEMENTS: u64 = 4096;

/// The key `name` as the game keeps it: `key(b"sqsr")`.
pub(crate) const fn key(name: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*name)
}

/// The four characters of `key`, when they are ones the game writes keys with.
pub(crate) fn key_text(key: u32) -> Option<String> {
    let bytes = key.to_be_bytes();
    bytes.iter().all(|byte| (0x20..0x7f).contains(byte)).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// What a typed value holds, by the class of its traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Traits {
    Null,
    Int,
    Unsigned,
    Bool,
    Long,
    Text,
    Record,
    List,
    /// A type the editor does not read: a date, a colour, a pointer.
    Other,
}

/// What the traits of the class named `class` hold: `class` as the compiler keeps it (see [`rtti::class_of`]), MSVC's
/// names on Windows (`.?AV?$RAW_TYPED_TRAITS@H@sicore@@`) and the Itanium C++ ABI's on macOS
/// (`N6sicore16RAW_TYPED_TRAITSIiEE`). `None` for a class that is no traits at all.
pub(crate) fn traits_of_class(class: &str) -> Option<Traits> {
    let has = |part: &str| class.contains(part);
    if !has("TYPED_TRAITS") {
        return None;
    }
    Some(if has("NULL_TYPED_TRAITS") {
        Traits::Null
    } else if has("STRING_TYPED_TRAITS") {
        Traits::Text
    } else if has("INTERNAL_OBJECT_TYPED_TRAITS") && (has("@VRECORD@sicore@") || has("6RECORDE")) {
        Traits::Record
    } else if has("INTERNAL_OBJECT_TYPED_TRAITS") && (has("@VTYPED_LIST@sicore@") || has("10TYPED_LISTE")) {
        Traits::List
    } else if has("RAW_TYPED_TRAITS@H@") || has("RAW_TYPED_TRAITSIiE") {
        Traits::Int
    } else if has("RAW_TYPED_TRAITS@I@") || has("RAW_TYPED_TRAITSIjE") {
        Traits::Unsigned
    } else if has("RAW_TYPED_TRAITS@_N@") || has("RAW_TYPED_TRAITSIbE") {
        Traits::Bool
    } else if has("RAW_TYPED_TRAITS@_J@") || has("RAW_TYPED_TRAITSIxE") {
        Traits::Long
    } else {
        Traits::Other
    })
}

/// A typed value as read: where it is, its traits and the 8 bytes of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Typed {
    pub(crate) at: u64,
    pub(crate) traits: u64,
    pub(crate) kind: Traits,
    pub(crate) raw: u64,
}

impl Typed {
    /// The number it holds, for the types that hold one.
    pub(crate) fn number(&self) -> Option<i64> {
        match self.kind {
            Traits::Int => Some(i64::from(self.raw as u32 as i32)),
            Traits::Unsigned => Some(i64::from(self.raw as u32)),
            Traits::Bool => Some(i64::from(self.raw as u8 != 0)),
            Traits::Long => Some(self.raw as i64),
            _ => None,
        }
    }

    /// The 16 bytes it takes in memory.
    pub(crate) fn bytes(&self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.traits.to_le_bytes());
        bytes[8..].copy_from_slice(&self.raw.to_le_bytes());
        bytes
    }

    /// The bytes it takes with the integer `value` in place of its own: the low four bytes of its value, the rest
    /// as it was.
    pub(crate) fn bytes_with(&self, value: i32) -> [u8; 16] {
        let raw = (self.raw & !0xFFFF_FFFF) | u64::from(value as u32);
        Typed { raw, ..*self }.bytes()
    }
}

/// Reads records out of a process. What a typed value's traits hold is asked once per traits object.
pub(crate) struct Records<'a> {
    process: &'a Process,
    classify: Box<dyn Fn(u64) -> Option<Traits> + 'a>,
    known: RefCell<HashMap<u64, Option<Traits>>>,
}

impl<'a> Records<'a> {
    /// The records of the game: traits are told by the name of their class.
    pub(crate) fn of_game(process: &'a Process, plugin: &'a Module) -> Self {
        Self::with(process, move |traits| {
            rtti::class_of(process, plugin, traits).and_then(|class| traits_of_class(&class))
        })
    }

    /// Records whose traits `classify` tells.
    pub(crate) fn with(process: &'a Process, classify: impl Fn(u64) -> Option<Traits> + 'a) -> Self {
        Self { process, classify: Box::new(classify), known: RefCell::new(HashMap::new()) }
    }

    fn traits(&self, object: u64) -> Option<Traits> {
        if !is_user_pointer(object) {
            return None;
        }
        if let Some(known) = self.known.borrow().get(&object) {
            return *known;
        }
        let kind = (self.classify)(object);
        self.known.borrow_mut().insert(object, kind);
        kind
    }

    /// The typed value at `at`; `None` when what is there is not one.
    pub(crate) fn typed(&self, at: u64) -> Option<Typed> {
        if !is_user_pointer(at) {
            return None;
        }
        let bytes = self.process.read_bytes(at, 16).ok()?;
        let traits = u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"));
        let raw = u64::from_le_bytes(bytes[8..].try_into().expect("8 bytes"));
        Some(Typed { at, traits, kind: self.traits(traits)?, raw })
    }

    /// The entries of the record at `record`, each key with the pointer to its typed value, in the order the record
    /// keeps them; `None` when what is there is not a record's list.
    pub(crate) fn entries(&self, record: u64) -> Option<Vec<(u32, u64)>> {
        let bytes = self.span(record, 16, MOST_ENTRIES)?;
        Some(
            bytes
                .as_chunks::<16>()
                .0
                .iter()
                .map(|entry| {
                    let key = u32::from_le_bytes(entry[..4].try_into().expect("4 bytes"));
                    (key, u64::from_le_bytes(entry[8..].try_into().expect("8 bytes")))
                })
                .collect(),
        )
    }

    /// The value the record at `record` holds under `key`: the first, should it hold several.
    pub(crate) fn get(&self, record: u64, key: u32) -> Option<Typed> {
        let entries = self.entries(record)?;
        entries.iter().find(|entry| entry.0 == key).and_then(|entry| self.typed(entry.1))
    }

    /// The number under `key`.
    pub(crate) fn number(&self, record: u64, key: u32) -> Option<i64> {
        self.get(record, key)?.number()
    }

    /// The text `value` holds, for a value of text.
    pub(crate) fn text(&self, value: &Typed) -> Option<String> {
        (value.kind == Traits::Text).then(|| crate::text::read_fm_string(self.process, value.raw))?
    }

    /// The record under `key`: where its own list is.
    pub(crate) fn record(&self, record: u64, key: u32) -> Option<u64> {
        let value = self.get(record, key)?;
        (value.kind == Traits::Record && is_user_pointer(value.raw)).then_some(value.raw)
    }

    /// The elements of the list under `key`: pointers to typed values.
    pub(crate) fn list(&self, record: u64, key: u32) -> Option<Vec<u64>> {
        let value = self.get(record, key)?;
        if value.kind != Traits::List {
            return None;
        }
        self.elements(value.raw)
    }

    /// The elements of the list whose `(begin, end, capacity)` is at `list`: pointers to typed values. A list that
    /// was never filled has no buffer, and no elements.
    pub(crate) fn elements(&self, list: u64) -> Option<Vec<u64>> {
        let bytes = self.span(list, 8, MOST_ELEMENTS)?;
        Some(bytes.as_chunks::<8>().0.iter().map(|element| u64::from_le_bytes(*element)).collect())
    }

    /// The records a list of records holds, in its order; an element that is not a record ends it.
    pub(crate) fn records_of(&self, elements: &[u64]) -> Vec<u64> {
        elements
            .iter()
            .map_while(|&element| {
                let value = self.typed(element)?;
                (value.kind == Traits::Record && is_user_pointer(value.raw)).then_some(value.raw)
            })
            .collect()
    }

    /// The bytes between the `begin` and `end` that `header` holds, for a list of elements of `size` bytes, no more
    /// than `most` of them: empty for a list with no buffer.
    fn span(&self, header: u64, size: u64, most: u64) -> Option<Vec<u8>> {
        if !is_user_pointer(header) {
            return None;
        }
        let pair = self.process.read_bytes(header, 16).ok()?;
        let begin = u64::from_le_bytes(pair[..8].try_into().expect("8 bytes"));
        let end = u64::from_le_bytes(pair[8..].try_into().expect("8 bytes"));
        if begin == 0 && end == 0 {
            return Some(Vec::new());
        }
        let length = end.checked_sub(begin)?;
        if !is_user_pointer(begin) || !length.is_multiple_of(size) || length / size > most {
            return None;
        }
        self.process.read_bytes(begin, length as usize).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_its_four_characters_with_the_first_in_the_highest_byte() {
        // As the game's memory holds 'sqsr': 72 73 71 73.
        assert_eq!(key(b"sqsr").to_le_bytes(), [0x72, 0x73, 0x71, 0x73]);
        assert_eq!(key_text(key(b"1yol")).as_deref(), Some("1yol"));
        assert_eq!(key_text(key(b"id  ")).as_deref(), Some("id  "));
        assert_eq!(key_text(0), None);
        assert_eq!(key_text(0x1FCEE), None);
    }

    #[test]
    fn traits_are_told_by_the_name_of_their_class() {
        let of = traits_of_class;
        assert_eq!(of(".?AV?$RAW_TYPED_TRAITS@H@sicore@@"), Some(Traits::Int));
        assert_eq!(of(".?AV?$RAW_TYPED_TRAITS@I@sicore@@"), Some(Traits::Unsigned));
        assert_eq!(of(".?AV?$RAW_TYPED_TRAITS@_N@sicore@@"), Some(Traits::Bool));
        assert_eq!(of(".?AV?$RAW_TYPED_TRAITS@_J@sicore@@"), Some(Traits::Long));
        assert_eq!(of(".?AVNULL_TYPED_TRAITS@sicore@@"), Some(Traits::Null));
        assert_eq!(of(".?AVSTRING_TYPED_TRAITS@sicore@@"), Some(Traits::Text));
        assert_eq!(of(".?AV?$INTERNAL_OBJECT_TYPED_TRAITS@VRECORD@sicore@@$09@sicore@@"), Some(Traits::Record));
        assert_eq!(of(".?AV?$INTERNAL_OBJECT_TYPED_TRAITS@VTYPED_LIST@sicore@@$0L@@sicore@@"), Some(Traits::List));
        assert_eq!(of(".?AVSI_DATE_TYPED_TRAITS@fmdate@@"), Some(Traits::Other));
        // An object of another class is no traits: what points at it is no typed value.
        assert_eq!(of(".?AVACTUAL_COMP@sicomps@@"), None);
    }

    /// The same classes as the Mac build names them (Steam build 26620374, Apple silicon).
    #[test]
    fn traits_are_told_by_their_itanium_names_too() {
        let of = traits_of_class;
        assert_eq!(of("N6sicore16RAW_TYPED_TRAITSIiEE"), Some(Traits::Int));
        assert_eq!(of("N6sicore16RAW_TYPED_TRAITSIjEE"), Some(Traits::Unsigned));
        assert_eq!(of("N6sicore16RAW_TYPED_TRAITSIbEE"), Some(Traits::Bool));
        assert_eq!(of("N6sicore16RAW_TYPED_TRAITSIxEE"), Some(Traits::Long));
        assert_eq!(of("N6sicore16RAW_TYPED_TRAITSIdEE"), Some(Traits::Other));
        assert_eq!(of("N6sicore19STRING_TYPED_TRAITSE"), Some(Traits::Text));
        assert_eq!(of("N6sicore28INTERNAL_OBJECT_TYPED_TRAITSINS_6RECORDELh10EEE"), Some(Traits::Record));
        assert_eq!(of("N6sicore28INTERNAL_OBJECT_TYPED_TRAITSINS_10TYPED_LISTELh11EEE"), Some(Traits::List));
        // Another class of traits for a record, not seen in the records the editor reads.
        assert_eq!(of("N6sicore19OBJECT_TYPED_TRAITSINS_6RECORDEEE"), Some(Traits::Other));
        assert_eq!(of("N6fmdate20SI_DATE_TYPED_TRAITSE"), Some(Traits::Other));
        assert_eq!(of("N7sicomps11ACTUAL_COMPE"), None);
    }

    #[test]
    fn an_integer_keeps_the_upper_half_of_its_word() {
        // The Süper Lig's limit as read: 14 below what was left there.
        let value = Typed { at: 0x1000, traits: 0x1C5_518C_DA28, kind: Traits::Int, raw: 0x7FFA_0000_000E };
        assert_eq!(value.number(), Some(14));
        let written = value.bytes_with(22);
        assert_eq!(written[..8], value.bytes()[..8]);
        assert_eq!(u64::from_le_bytes(written[8..].try_into().expect("8 bytes")), 0x7FFA_0000_0016);
        let negative = Typed { raw: 0x7FFA_FFFF_FFFF, ..value };
        assert_eq!(negative.number(), Some(-1));
        assert_eq!(Typed { kind: Traits::Bool, raw: 0x7FFA_1300_3B01, ..value }.number(), Some(1));
        assert_eq!(Typed { kind: Traits::Record, ..value }.number(), None);
    }

    /// A record laid out in memory of the test's own as the game lays one out: Windows only (see `Session::over`).
    #[cfg(windows)]
    #[test]
    fn a_record_reads_through_its_entries_values_and_lists() {
        use crate::session::{Blocks, Session};

        let memory = Blocks::new(4);
        let process = Process::open_read_write(std::process::id()).expect("the test opens its own process");
        let session = Session::over(process, crate::layout_for(27).expect("FM27 layout"));
        // Block 0: the traits, one object each; block 1: typed values; block 2: entries and list elements; block 3: the
        // records' and the list's own (begin, end, capacity).
        let (traits, values, lists, heads) = (memory.at(0), memory.at(1), memory.at(2), memory.at(3));
        let (int, record_traits, list_traits) = (traits, traits + 0x10, traits + 0x20);
        let classify = move |object: u64| match object {
            o if o == int => Some(Traits::Int),
            o if o == record_traits => Some(Traits::Record),
            o if o == list_traits => Some(Traits::List),
            _ => None,
        };
        let typed = |at: u64, traits: u64, raw: u64| {
            session.put_pointer(at, traits);
            session.put_pointer(at + 8, raw);
        };
        let entry = |at: u64, name: &[u8; 4], value: u64| {
            session.put(at, &key(name).to_le_bytes());
            session.put_pointer(at + 8, value);
        };
        let span = |at: u64, begin: u64, end: u64| {
            session.put_pointer(at, begin);
            session.put_pointer(at + 8, end);
            session.put_pointer(at + 16, end);
        };
        // A rule {type 21, valu 14}, held by a list under 'sqsr' of an outer record that also holds 'fxri' 0.
        let (rule, outer, list) = (heads, heads + 0x20, heads + 0x40);
        typed(values, int, 21);
        typed(values + 0x10, int, 0x7FFA_0000_000E);
        typed(values + 0x20, record_traits, rule);
        typed(values + 0x30, list_traits, list);
        typed(values + 0x40, int, 0);
        entry(lists, b"type", values);
        entry(lists + 0x10, b"valu", values + 0x10);
        span(rule, lists, lists + 0x20);
        session.put_pointer(lists + 0x100, values + 0x20);
        span(list, lists + 0x100, lists + 0x108);
        entry(lists + 0x200, b"fxri", values + 0x40);
        entry(lists + 0x210, b"sqsr", values + 0x30);
        span(outer, lists + 0x200, lists + 0x220);

        let records = Records::with(&session.process, classify);
        assert_eq!(records.number(outer, key(b"fxri")), Some(0));
        let elements = records.list(outer, key(b"sqsr")).expect("the list of rules");
        assert_eq!(records.records_of(&elements), [rule]);
        assert_eq!(records.number(rule, key(b"type")), Some(21));
        let value = records.get(rule, key(b"valu")).expect("the rule's value");
        assert_eq!((value.at, value.number()), (values + 0x10, Some(14)));
        // What the record does not hold, and a number asked for as a record.
        assert_eq!(records.number(rule, key(b"cmty")), None);
        assert_eq!(records.record(rule, key(b"valu")), None);
        // A list never filled has no buffer; a place that holds no list is none.
        assert_eq!(records.elements(heads + 0x100), Some(Vec::new()));
        assert_eq!(records.entries(values), None);
        std::hint::black_box(&memory);
    }
}
