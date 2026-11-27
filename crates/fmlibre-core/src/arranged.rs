//! Arranging a transfer for the next day, which the game then makes itself.
//!
//! A move the editor makes at once leaves the game's history as it was: the season under way shows one row, for the new
//! club, with what the player did at the old one, and no fee. When the game makes a transfer, it closes the old club's
//! row, opens one for the new club with the fee, writes its own records of the move and pays the fee. The editor gets
//! all of that by agreeing a transfer for the next day as the game keeps one: on that day the game makes it as it makes
//! its own.
//!
//! A move agreed for a later day, as the game keeps one ([`TransfersLayout`], read on Game Pass 0.9.2524.0):
//! - an offer of the transfer manager's list of offers, agreed (state 19): the player, the buying team twice, the
//!   selling team, the fee in pounds, the offer's number, the day it was made, the day it lapses and the day of the
//!   move. It is laid out as one of the game's own agreed transfers, whose bytes it starts from, in a slot of the game's
//!   pool of offers;
//! - the contract he is to have: his own, copied, with the buying club's senior team, from the day of the move (see
//!   `signing.rs`), in a slot of the game's pool of contracts;
//! - an entry naming that contract and the offer's number, agreed and waiting (state 2), in the player's record of
//!   entries, and the offer's number in his record of offers' numbers;
//! - the move due on that day, in the manager's list of them (the player, the day, the offer's number), and the flag the
//!   manager raises while one is due. Without it the game shows the transfer as arranged for 1 January 1900 and never
//!   makes it.
//!
//! A loan is agreed the same way ([`LoanOfferLayout`], read on Game Pass 0.9.2562.0): the offer is one of the game's
//! loan offers, a class of its own laid out as a transfer offer up to its state and kind (1), with the monthly fee where
//! a transfer has its fee and the loan's terms after them; the contract he is to have is a loan contract with the club
//! he joins, until the loan's last day, for the part of his wage that club pays, holding the same terms; its entry is of
//! kind 1. Both start from one of the game's own loans: an offer it made, and the loan contract of that offer's player.
//! On the day the game makes the loan as it makes its own: he plays for the club he joins and stays under contract with
//! his own, which the history shows as a loan. It does so outside a transfer window too, as it makes an arranged
//! transfer (Rodrigo Mendoza to Lyon on 28 October 2026, on Steam). A loan may hold an option to buy: a future fee the
//! club he joins may pay when the loan ends, if it wants to, which the game shows as an "Optional Future Fee"; the
//! offer marks its kind (0xFE), and the game copies it into the loan contract, as it does for its own. The editor never
//! agrees a mandatory one: for those the game keeps the entry until the fee is paid and signs him then on the contract
//! the entry holds (docs/recon/fm27-loans.md).
//!
//! The entry, the move due and any record or list the editor makes or grows are blocks of the game's heap, taken by the
//! game's C runtime (see `heap.rs`): the game gives some of them back to the runtime's `free()`. Until the day, an undo
//! takes it all back and gives those blocks back; once the game has made the move, nothing is left to undo.

use fmlibre_mem::is_user_pointer;

use crate::clubs::ClubSummary;
use crate::date::FmDate;
use crate::edit::Kind;
use crate::players::PlayerSummary;
use crate::profile::{ContractLayout, LoanOfferLayout, LoanTermsLayout, TransfersLayout};
use crate::rtti;
use crate::session::Session;
use crate::signing::NewContract;
use crate::transfer::{
    LISTED, MAX_FEE, MoveError, MoveTerms, NO_SQUAD_NUMBER, PLAYER_JOB, Patch, Patches, PointerList,
};

/// The most offers the editor takes the manager's list to hold.
const MOST_OFFERS: u64 = 200_000;

/// The flags of the agreed transfers the game makes most, and the states of an offer: still to be answered by the club
/// (0) or by the player (8), agreed (19), made (20).
const TRANSFER_FLAGS: u32 = 0x40;
const AGREED: u8 = 19;
const MADE: u8 = 20;
const UNDER_WAY: [u8; 3] = [0, 8, 19];

/// An offer's kind for a transfer with a fee (8 is a free move at the end of a contract), and the kind of every loan
/// offer.
const TRANSFER_OFFER: u8 = 2;
const LOAN_OFFER: u8 = 1;

/// How long the game takes a loan the editor agrees to run: a season, the commonest. It ends on the loan contract's end.
const SEASON_LOAN: u32 = 0x1000;

/// The flags of a loan the editor agrees, as the game's loan contracts keep them: he may play in cup matches (bit 1),
/// and his club may recall him (bit 2).
const LOAN_FLAGS: u16 = 0b110;

/// How long an offer stands, in days, as the game's do.
const OFFER_STANDS: i64 = 30;

/// What the game keeps in an entry where it has no amount, the entry's state while agreed and waiting, and its kind for
/// a transfer with a fee and for a loan.
const NO_AMOUNT: i32 = -9_999_999;
const WAITING: u8 = 2;
const TRANSFER_KIND: u8 = 2;
const LOAN_KIND: u8 = 1;

/// The kinds of future fee a loan offer and its loan contract hold: none, and an option the club he joins may take up
/// when the loan ends, which the game shows as an "Optional Future Fee". A mandatory one, which the editor does not
/// agree, holds others.
const NO_FUTURE_FEE: u8 = 0xFF;
const OPTIONAL_FUTURE_FEE: u8 = 0xFE;

/// The day of the year a loan agreed until the end of the season ends on by default, as most of the game's do: 29 June,
/// the day before contracts run out (day 180, or 181 in a leap year).
const SEASON_LOAN_ENDS: (u8, u8) = (6, 29);

/// The fewest days a loan agreed until the end of the season runs: one that would run shorter runs to the end of the
/// next.
const SHORTEST_SEASON_LOAN: i64 = 30;

/// A date the game has not set: day 1 of 1900.
const UNSET_DATE: [u8; 4] = [0x01, 0x00, 0x6C, 0x07];

/// The room a list the editor makes has, in elements, and how a full one grows: by half again, as the game's do.
const NEW_LIST_ROOM: u64 = 4;

/// Where an object of an arranged transfer goes: the first free slot of the game's pool of them, taken as the game takes
/// one, or, when the pool has none free, a block of the game's heap taken as the transfer is written, which the pool
/// takes as its own once the game lets go of it.
#[derive(Debug, Clone)]
enum Placement {
    Slot { slot: u64, was: Vec<u8>, pool: Patch },
    Heap,
}

/// The vtables of the game's offers of a transfer and of a loan, which the model of an offer the editor makes must have.
#[derive(Debug, Clone, Default)]
pub struct OfferClasses {
    transfer: Vec<u64>,
    loan: Vec<u64>,
}

impl OfferClasses {
    /// Whether the offer whose first word is `vtable` may be the model of a transfer: one of the game's transfer
    /// offers, or any offer where the game's names do not give that class, as before the class was checked (no Mac
    /// save has been read for it).
    fn transfer_model(&self, vtable: u64) -> bool {
        self.transfer.is_empty() || self.transfer.contains(&vtable)
    }
}

/// A loan's terms, as the editor agrees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoanDeal {
    /// The loan's last day, as a number like 20280629: he is back at his club the next day.
    pub until: i64,
    /// The part of his wage the club he joins pays, in percent.
    pub wage_share: u8,
    /// Pounds a month the club he joins pays his own, whether he plays or not.
    pub monthly_fee: i64,
    /// An option to buy: pounds for which the club he joins may sign him for good when the loan ends, if it wants to.
    /// None for no option.
    pub option_fee: Option<i64>,
}

impl LoanDeal {
    /// The kind of future fee the offer and the loan contract hold.
    fn fee_kind(&self) -> u8 {
        if self.option_fee.is_some() { OPTIONAL_FUTURE_FEE } else { NO_FUTURE_FEE }
    }
}

/// What the game is to make: a transfer for a fee, on a contract with these terms, or a loan.
#[derive(Debug, Clone, Copy)]
enum Deal {
    Transfer { terms: MoveTerms, fee: i64 },
    Loan(LoanDeal),
}

/// The last day of a loan agreed on `today` until the end of the season, as a number like 20280629: 29 June, the one
/// after the next 30 days; never after `contract_ends`, the day his contract runs out.
pub fn season_loan_end(today: FmDate, contract_ends: Option<i64>) -> i64 {
    let (month, day) = SEASON_LOAN_ENDS;
    let on = |year: u16| i64::from(year) * 10_000 + i64::from(month) * 100 + i64::from(day);
    let soonest = today.plus_days(SHORTEST_SEASON_LOAN).to_number();
    let mut end = on(today.year);
    if end < soonest {
        end = on(today.year + 1);
    }
    contract_ends.filter(|&ends| ends > today.to_number()).map_or(end, |ends| end.min(ends))
}

/// What arranging a transfer or a loan does, worked out from the game as it is now. Nothing of the game's is taken for
/// it: the slots and blocks it needs are taken as it is written.
#[derive(Debug, Clone)]
pub struct ArrangedPlan {
    pub player: u32,
    pub from_club: u32,
    pub to_club: u32,
    /// Pounds; none for a loan.
    pub fee: i64,
    /// The day the game makes the move.
    pub move_day: FmDate,
    pub offer_number: u32,
    /// What the contract he is to have says: for a loan, the loan contract (the part of his wage the club he joins
    /// pays, and the loan's last day).
    pub terms: NewContract,
    /// For a loan, its terms.
    pub loan: Option<LoanDeal>,
    /// The offer and the contract, and where each goes.
    offer: (Vec<u8>, Placement),
    contract: (Vec<u8>, Placement),
    /// The player's object and person block, and the transfer manager.
    object: u64,
    person: u64,
    manager: u64,
}

/// A transfer arranged: what undoing it takes, until the game makes it.
#[derive(Debug, Clone)]
pub struct Arranged {
    pub player: u32,
    pub move_day: FmDate,
    /// Where the offer and the contract went.
    pub offer: u64,
    pub contract: u64,
    patches: Vec<Patch>,
    /// The blocks of the game's heap the editor took, given back by an undo.
    blocks: Vec<u64>,
}

/// The parties of an agreed offer and its days: the player's person block, the teams that buy and sell, the offer's
/// number and the count of offers made, the day it is made and the day of the move.
#[derive(Debug, Clone, Copy)]
struct Agreed {
    person: u64,
    buyer: u64,
    seller: u64,
    number: u32,
    count: u32,
    today: FmDate,
    move_day: FmDate,
}

/// The fields every agreed offer has, of a transfer or of a loan, written over `template`, one of the game's own offers
/// of that class: `fee` where a transfer keeps its fee, and the offer's `kind`.
fn agreed_offer(t: &TransfersLayout, template: &[u8], agreed: Agreed, fee: u32, kind: u8) -> Vec<u8> {
    let date = |day: FmDate| Kind::Date.encode(day.to_number(), &[0; 4]);
    let mut offer = template.to_vec();
    let mut put = |at: u64, value: &[u8]| offer[at as usize..at as usize + value.len()].copy_from_slice(value);
    put(t.offer_one, &1u64.to_le_bytes());
    put(t.offer_person, &agreed.person.to_le_bytes());
    put(t.offer_buyer, &agreed.buyer.to_le_bytes());
    put(t.offer_buyer_again, &agreed.buyer.to_le_bytes());
    put(t.offer_seller, &agreed.seller.to_le_bytes());
    put(t.offer_fee, &fee.to_le_bytes());
    put(t.offer_number, &agreed.number.to_le_bytes());
    put(t.offer_count, &agreed.count.to_le_bytes());
    for &at in &t.offer_unset {
        put(at, &(-1i32).to_le_bytes());
    }
    for at in [t.offer_accepted, t.offer_moved, t.offer_later] {
        put(at, &UNSET_DATE);
    }
    put(t.offer_made, &date(agreed.today));
    put(t.offer_lapses, &date(agreed.today.plus_days(OFFER_STANDS)));
    put(t.offer_move_day, &date(agreed.move_day));
    put(t.offer_state, &[AGREED]);
    put(t.offer_kind, &[kind]);
    offer
}

/// An agreed transfer offer's bytes, made from `template`, one of the game's own: the fee paid at once.
fn offer_bytes(t: &TransfersLayout, template: &[u8], agreed: Agreed, fee: u32) -> Vec<u8> {
    let mut offer = agreed_offer(t, template, agreed, fee, TRANSFER_OFFER);
    offer[t.offer_flags as usize..t.offer_flags as usize + 8].copy_from_slice(&u64::from(TRANSFER_FLAGS).to_le_bytes());
    offer[t.offer_payment as usize..t.offer_payment as usize + 8].copy_from_slice(&0u64.to_le_bytes());
    offer
}

/// An agreed loan offer's bytes, made from `template`, one of the game's own loan offers: the monthly fee where a
/// transfer keeps its fee, then the loan's terms, `wage` the pounds a week of his wage the club he joins pays; no
/// add-ons.
fn loan_offer_bytes(
    t: &TransfersLayout,
    l: &LoanOfferLayout,
    template: &[u8],
    agreed: Agreed,
    loan: &LoanDeal,
    wage: i32,
) -> Vec<u8> {
    let fees = |pounds: i64| i32::try_from(pounds).expect("checked against MAX_FEE");
    let monthly = fees(loan.monthly_fee);
    let fee = u32::try_from(loan.monthly_fee).expect("checked against MAX_FEE");
    let mut offer = agreed_offer(t, template, agreed, fee, LOAN_OFFER);
    let mut put = |at: u64, value: &[u8]| offer[at as usize..at as usize + value.len()].copy_from_slice(value);
    put(t.offer_flags, &TRANSFER_FLAGS.to_le_bytes());
    for &at in &l.offer_cleared {
        put(at, &0u64.to_le_bytes());
    }
    put(l.offer_flags, &u32::from(LOAN_FLAGS).to_le_bytes());
    for &at in &l.offer_wage {
        put(at, &wage.to_le_bytes());
    }
    put(l.offer_length, &SEASON_LOAN.to_le_bytes());
    put(l.offer_unused_fee, &monthly.to_le_bytes());
    put(l.offer_option_fee, &loan.option_fee.map_or(-1, fees).to_le_bytes());
    put(l.offer_fee_kind, &[loan.fee_kind()]);
    for &at in &l.offer_unset {
        put(at, &(-1i32).to_le_bytes());
    }
    for &at in &l.offer_share {
        put(at, &u16::from(loan.wage_share).to_le_bytes());
    }
    offer
}

/// The loan contract he is to have, made from `model`, one of the game's own: with the club whose senior team is
/// `senior`, from the day of the move to the loan's last day, `wage` pounds a week, his squad status `status` there and
/// no squad number, and the loan's terms; nothing it points at is kept, and it starts with no day he joined, as the
/// contracts of the game's agreed loans do.
#[allow(clippy::too_many_arguments)]
fn loan_contract_bytes(
    c: &ContractLayout,
    l: &LoanOfferLayout,
    terms: &LoanTermsLayout,
    model: &[u8],
    person: u64,
    senior: u64,
    loan: &LoanDeal,
    wage: i32,
    status: u8,
    move_day: FmDate,
) -> Vec<u8> {
    let at = |offset: u64, len: usize| offset as usize..offset as usize + len;
    let fees = |pounds: i64| i32::try_from(pounds).expect("checked against MAX_FEE");
    let signed = Kind::Date.encode(move_day.to_number(), &model[at(c.signed, 4)]);
    let until = Kind::Date.encode(loan.until, &model[at(c.expires, 4)]);
    let listing = model[c.listing as usize] & !LISTED;
    let mut bytes = model.to_vec();
    let mut put = |offset: u64, value: &[u8]| bytes[at(offset, value.len())].copy_from_slice(value);
    put(c.owner, &person.to_le_bytes());
    put(c.team, &senior.to_le_bytes());
    put(c.senior_team, &senior.to_le_bytes());
    put(c.job, &[PLAYER_JOB]);
    put(c.signed, &signed);
    put(c.expires, &until);
    put(c.joined, &UNSET_DATE);
    put(c.wage, &wage.to_le_bytes());
    put(l.contract_wage_again, &wage.to_le_bytes());
    put(c.loyalty_bonus, &0i32.to_le_bytes());
    put(c.squad_status, &[status]);
    put(l.contract_status_again, &[status]);
    put(c.squad_number, &[NO_SQUAD_NUMBER]);
    put(c.listing, &[listing]);
    for &offset in &l.contract_cleared {
        put(offset, &[0; 8]);
    }
    for &offset in &l.contract_unset {
        put(offset, &(-1i32).to_le_bytes());
    }
    for &offset in &l.contract_zero {
        put(offset, &0u32.to_le_bytes());
    }
    put(l.contract_length, &SEASON_LOAN.to_le_bytes());
    put(terms.flags, &LOAN_FLAGS.to_le_bytes());
    put(terms.option_fee, &loan.option_fee.map_or(-1, fees).to_le_bytes());
    put(l.contract_fee_kind, &[loan.fee_kind()]);
    put(terms.monthly_fee, &fees(loan.monthly_fee).to_le_bytes());
    put(terms.unused_fee, &fees(loan.monthly_fee).to_le_bytes());
    bytes
}

/// How good a model one of the game's offers is for an arranged transfer, the higher the better, or None for one
/// neither agreed nor made. What the editor does not write is the model's, the bytes from +0x84 on among it, and they
/// go with the kind of the transfer: so first a transfer with a fee, not a free move at the end of a contract or a
/// loan (on a Mac save of July 2027 the game's agreed transfers with the flags it gives most were all free ones), then
/// one agreed rather than made, then one with those flags, then one paid at once.
fn model_rank(t: &TransfersLayout, offer: &[u8]) -> Option<u8> {
    let state = offer[t.offer_state as usize];
    if state != AGREED && state != MADE {
        return None;
    }
    let half = |at: u64| u32::from_le_bytes(offer[at as usize..at as usize + 4].try_into().expect("4 bytes"));
    let paid_at_once = offer[t.offer_payment as usize..t.offer_payment as usize + 8].iter().all(|&b| b == 0);
    Some(
        8 * u8::from(offer[t.offer_kind as usize] == TRANSFER_OFFER)
            + 4 * u8::from(state == AGREED)
            + 2 * u8::from(half(t.offer_flags) == TRANSFER_FLAGS)
            + u8::from(paid_at_once),
    )
}

/// How good a model one of the game's loan offers is for an agreed loan, the higher the better, or None for one neither
/// agreed nor made. What the editor does not write is the model's, so first one with the flags the game gives its
/// loans between clubs' first teams most, then one agreed rather than made, then one whose club pays a part of his
/// wage, then one that holds what most of those hold where the editor does not know what it is.
fn loan_model_rank(t: &TransfersLayout, l: &LoanOfferLayout, offer: &[u8]) -> Option<u8> {
    let state = offer[t.offer_state as usize];
    if state != AGREED && state != MADE {
        return None;
    }
    let half = |at: u64| u32::from_le_bytes(offer[at as usize..at as usize + 4].try_into().expect("4 bytes"));
    let short = |at: u64| u16::from_le_bytes([offer[at as usize], offer[at as usize + 1]]);
    let zero = |at: u64| offer[at as usize..at as usize + 8].iter().all(|&b| b == 0);
    Some(
        16 * u8::from(half(t.offer_flags) == TRANSFER_FLAGS)
            + 8 * u8::from(state == AGREED)
            + 4 * u8::from(l.offer_share.iter().all(|&at| short(at) > 0))
            + 2 * u8::from(l.offer_usual.iter().all(|&(at, value)| offer[at as usize] == value))
            + u8::from(l.offer_usual_zero.iter().all(|&at| zero(at))),
    )
}

/// An entry's bytes: the contract, the offer's number, agreed and waiting, and its `kind`.
fn entry_bytes(t: &TransfersLayout, contract: u64, number: u32, kind: u8) -> Vec<u8> {
    let mut entry = vec![0u8; t.entry_size as usize];
    let mut put = |at: u64, value: &[u8]| entry[at as usize..at as usize + value.len()].copy_from_slice(value);
    put(t.entry_one, &1u32.to_le_bytes());
    put(t.entry_contract, &contract.to_le_bytes());
    put(t.entry_number, &number.to_le_bytes());
    put(t.entry_no_amount, &NO_AMOUNT.to_le_bytes());
    put(t.entry_date, &UNSET_DATE);
    put(t.entry_state, &[WAITING]);
    put(t.entry_one_again, &[1]);
    put(t.entry_agreed, &[1]);
    put(t.entry_kind, &[kind]);
    entry
}

/// A move due's bytes: the player's person block, the day (without the flags other dates carry) and the offer's number.
fn due_bytes(t: &TransfersLayout, person: u64, day: FmDate, number: u32) -> Vec<u8> {
    let mut due = vec![0u8; t.due_size as usize];
    let mut put = |at: u64, value: &[u8]| due[at as usize..at as usize + value.len()].copy_from_slice(value);
    put(t.due_person, &person.to_le_bytes());
    put(t.due_day, &Kind::Date.encode(day.to_number(), &[0; 4]));
    put(t.due_number, &number.to_le_bytes());
    due
}

/// Checks a loan's terms: a share of his wage of 100% at most, a monthly fee from nothing to [`MAX_FEE`], and an option
/// to buy, if any, from £1 to [`MAX_FEE`].
fn check_loan(loan: &LoanDeal) -> Result<(), MoveError> {
    if loan.wage_share > 100 {
        let value = i64::from(loan.wage_share);
        return Err(MoveError::OutOfRange { field: "wageShare", value, min: 0, max: 100 });
    }
    if !(0..=MAX_FEE).contains(&loan.monthly_fee) {
        let value = loan.monthly_fee;
        return Err(MoveError::OutOfRange { field: "monthlyFee", value, min: 0, max: MAX_FEE });
    }
    if let Some(value) = loan.option_fee.filter(|fee| !(1..=MAX_FEE).contains(fee)) {
        return Err(MoveError::OutOfRange { field: "optionFee", value, min: 1, max: MAX_FEE });
    }
    Ok(())
}

/// How many elements a full list of `len` grows to: half as many again, as the game's lists grow, and some room at the
/// least.
fn grown_capacity(len: u64) -> u64 {
    (len + len / 2).max(len + 1).max(NEW_LIST_ROOM)
}

impl Session {
    /// The layout of the game's transfers, when this version of the game has one and the editor can take blocks of the
    /// game's heap, which an arranged transfer needs.
    fn transfers_layout(&self) -> Result<&TransfersLayout, MoveError> {
        let layout = self.layout.transfers.as_ref().ok_or(MoveError::NoTransfers)?;
        if !self.takes_heap_blocks() {
            return Err(MoveError::NoGameMemory);
        }
        Ok(layout)
    }

    /// The layouts an agreed loan needs besides a transfer's: its offer, and the terms of the loan contract.
    fn loans_layout(&self) -> Result<(&TransfersLayout, &LoanOfferLayout, &LoanTermsLayout), MoveError> {
        let t = self.transfers_layout()?;
        let l = t.loan.as_ref().ok_or(MoveError::NoTransfers)?;
        let terms = self.layout.loan.as_ref().and_then(|loan| loan.terms.as_ref()).ok_or(MoveError::NoTransfers)?;
        Ok((t, l, terms))
    }

    /// Whether this version of the game, on this system, lets the editor arrange a transfer for the next day.
    pub fn arranges_transfers(&self) -> bool {
        self.transfers_layout().is_ok()
    }

    /// Whether this version of the game, on this system, lets the editor arrange a loan for the next day.
    pub fn arranges_loans(&self) -> bool {
        self.loans_layout().is_ok()
    }

    /// The vtables of the game's transfer offers and loan offers, found by their names the first time they are needed.
    fn offer_classes(&self, t: &TransfersLayout) -> &OfferClasses {
        self.offer_classes.get_or_init(|| OfferClasses {
            transfer: rtti::vtables(&self.process, &self.plugin, &t.offer_class),
            loan: t
                .loan
                .as_ref()
                .map(|l| rtti::vtables(&self.process, &self.plugin, &l.offer_class))
                .unwrap_or_default(),
        })
    }

    /// Works out arranging the move of `player` to the senior team of `club` for the next day, for `fee` pounds, on the
    /// contract `terms` (see the module's notes). `human_clubs` are the UIDs of the clubs human managers run. Nothing
    /// is written, and nothing of the game's is taken.
    pub fn plan_arranged_transfer(
        &self,
        player: &PlayerSummary,
        club: &ClubSummary,
        terms: MoveTerms,
        fee: i64,
        human_clubs: &[u32],
    ) -> Result<ArrangedPlan, MoveError> {
        self.plan_arranged(player, club, Deal::Transfer { terms, fee }, human_clubs)
    }

    /// Works out arranging the loan of `player` to the senior team of `club` from the next day, on the terms `loan` (see
    /// the module's notes). `human_clubs` are the UIDs of the clubs human managers run. Nothing is written, and nothing
    /// of the game's is taken.
    pub fn plan_arranged_loan(
        &self,
        player: &PlayerSummary,
        club: &ClubSummary,
        loan: LoanDeal,
        human_clubs: &[u32],
    ) -> Result<ArrangedPlan, MoveError> {
        self.plan_arranged(player, club, Deal::Loan(loan), human_clubs)
    }

    /// The last day of a loan of `player` agreed now until the end of the season (see [`season_loan_end`]).
    pub fn season_loan_end(&self, player: &PlayerSummary) -> Option<i64> {
        let today = self.game_date()?;
        let c = &self.layout.contract;
        let contract = self
            .process
            .read_u64(player.address + self.layout.player.at(self.layout.player.contract, player.shift))
            .ok()?;
        let ends = is_user_pointer(contract)
            .then(|| self.process.read_bytes(contract + c.expires, 4).ok())
            .flatten()
            .map(|bytes| Kind::Date.decode(&bytes))
            .filter(|&ends| ends > 0);
        Some(season_loan_end(today, ends))
    }

    fn plan_arranged(
        &self,
        player: &PlayerSummary,
        club: &ClubSummary,
        deal: Deal,
        human_clubs: &[u32],
    ) -> Result<ArrangedPlan, MoveError> {
        let t = self.transfers_layout()?;
        let loan_layouts = match deal {
            Deal::Loan(_) => Some(self.loans_layout()?),
            Deal::Transfer { .. } => None,
        };
        let (process, layout) = (&self.process, &self.layout);
        let (Some(contract_size), Some(pools)) = (layout.contract.size, layout.pool.as_ref()) else {
            return Err(MoveError::NoPool);
        };
        let today = self.game_date().ok_or(MoveError::NoDate)?;
        let move_day = today.plus_days(1);
        match deal {
            Deal::Transfer { terms, fee } => {
                if !(0..=MAX_FEE).contains(&fee) {
                    return Err(MoveError::OutOfRange { field: "fee", value: fee, min: 0, max: MAX_FEE });
                }
                self.check_terms(terms, Some(today))?;
            }
            Deal::Loan(loan) => check_loan(&loan)?,
        }
        let leaving = self.departure(player, human_clubs)?;
        if process.read_u32(club.address + layout.club.uid)? != club.uid {
            return Err(MoveError::NotAClub { uid: club.uid });
        }
        if leaving.club == club.address {
            return Err(MoveError::SameClub);
        }
        let women = process.read_bytes(club.address + layout.club.women, 1)?[0] != 0;
        if player.woman.is_some_and(|woman| woman != women) {
            return Err(MoveError::MenAndWomen);
        }
        let joining = self.arrival(club.address, &leaving)?;
        let person = leaving.object + layout.player.identity();
        let c = &layout.contract;
        let field = |bytes: &[u8], offset: u64, kind: Kind| kind.decode(&bytes[offset as usize..offset as usize + 4]);
        let own_end = field(&leaving.record, c.expires, Kind::Date);
        if let Deal::Loan(loan) = deal {
            // A loan runs from the day of the move to a later day, and ends by the day his contract runs out.
            let earliest = move_day.plus_days(1).to_number();
            if FmDate::from_number(loan.until).is_none() || loan.until < earliest || loan.until > own_end {
                return Err(MoveError::LoanEnd { earliest, latest: own_end });
            }
        }

        // The game's offers: none may be under way for him, and the new one takes the next number and count. The
        // model is one of the game's own offers of the class wanted, the best first (see model_rank, loan_model_rank).
        let manager = self.transfer_manager(t)?;
        let offers = self.manager_list(manager + t.offers)?;
        let classes = self.offer_classes(t);
        let (mut number, mut count, mut models) = (0u32, 0u32, Vec::new());
        for offer in offers.elements(process)? {
            let Ok(bytes) = process.read_bytes(offer, t.offer_size as usize) else {
                continue;
            };
            let word = |at: u64| u64::from_le_bytes(bytes[at as usize..at as usize + 8].try_into().expect("8 bytes"));
            let half = |at: u64| u32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().expect("4 bytes"));
            let state = bytes[t.offer_state as usize];
            if word(t.offer_person) == person && UNDER_WAY.contains(&state) {
                return Err(MoveError::DealUnderWay);
            }
            number = number.max(half(t.offer_number));
            count = count.max(half(t.offer_count));
            let rank = match loan_layouts {
                None if classes.transfer_model(word(0)) => model_rank(t, &bytes),
                Some((_, l, _)) if classes.loan.contains(&word(0)) => process
                    .read_bytes(offer, l.offer_size as usize)
                    .ok()
                    .and_then(|whole| loan_model_rank(t, l, &whole)),
                _ => None,
            };
            if let Some(rank) = rank {
                models.push((rank, offer));
            }
        }
        // The best first; of equally good ones, the first in the list.
        models.sort_by_key(|&(rank, _)| std::cmp::Reverse(rank));
        let agreed = Agreed {
            person,
            buyer: joining.senior,
            seller: leaving.employer,
            number: number + 1,
            count: count + 1,
            today,
            move_day,
        };

        let (offer, offer_at, contract, contract_at, made) = match (deal, loan_layouts) {
            (Deal::Transfer { terms, fee }, _) => {
                let &(_, model_offer) = models.first().ok_or(MoveError::NoTransfers)?;
                let template = process.read_bytes(model_offer, t.offer_size as usize)?;
                let fee = u32::try_from(fee).map_err(|_| MoveError::OutOfRange {
                    field: "fee",
                    value: fee,
                    min: 0,
                    max: MAX_FEE,
                })?;
                let offer = offer_bytes(t, &template, agreed, fee);
                let model = process.read_bytes(leaving.contract, contract_size as usize)?;
                // His contract keeps its end unless the terms give another, as a move made now keeps it; one that ends
                // by the day of the move runs to the end of next season, as a new contract does.
                let own_end = field(&model, c.expires, Kind::Date);
                let kept = (own_end > move_day.to_number()).then_some(own_end);
                let terms = MoveTerms { expires: terms.expires.or(kept), ..terms };
                let contract = self.new_contract(model, leaving.object, joining.senior, terms, today, move_day, None);
                let made = NewContract {
                    wage: field(&contract, c.wage, Kind::I32),
                    expires: field(&contract, c.expires, Kind::Date),
                    squad_status: contract[c.squad_status as usize],
                };
                let offer_at = self.placement(model_offer, t.offer_size, pools)?;
                let contract_at = self.placement(leaving.contract, contract_size, pools)?;
                (offer, offer_at, contract, contract_at, made)
            }
            (Deal::Loan(loan), Some((_, l, terms))) => {
                // The model of the loan contract is the one the player of a model offer is on now: the best offer
                // whose player is still on loan.
                let identity = layout.player.identity();
                let (model_offer, model_contract) = models
                    .iter()
                    .find_map(|&(_, offer)| {
                        let object = process.read_u64(offer + t.offer_person).ok()?.checked_sub(identity)?;
                        Some((offer, self.loan_contract(object, 0).ok()??))
                    })
                    .ok_or(MoveError::NoLoanModel)?;
                let template = process.read_bytes(model_offer, l.offer_size as usize)?;
                let model = process.read_bytes(model_contract, l.contract_size as usize)?;
                // The club he joins pays its share of the wage his own contract gives him, and he keeps his squad
                // status.
                let own_wage = field(&leaving.record, c.wage, Kind::I32).max(0);
                let wage = i32::try_from(own_wage * i64::from(loan.wage_share) / 100).unwrap_or(i32::MAX);
                let status = leaving.record[c.squad_status as usize];
                let offer = loan_offer_bytes(t, l, &template, agreed, &loan, wage);
                let made = NewContract { wage: i64::from(wage), expires: loan.until, squad_status: status };
                let offer_at = self.placement(model_offer, l.offer_size, pools)?;
                let contract =
                    loan_contract_bytes(c, l, terms, &model, person, joining.senior, &loan, wage, status, move_day);
                let contract_at = self.placement(model_contract, l.contract_size, pools)?;
                (offer, offer_at, contract, contract_at, made)
            }
            (Deal::Loan(_), None) => return Err(MoveError::NoTransfers),
        };
        Ok(ArrangedPlan {
            player: player.uid,
            from_club: leaving.club_uid,
            to_club: joining.club_uid,
            fee: match deal {
                Deal::Transfer { fee, .. } => fee,
                Deal::Loan(_) => 0,
            },
            move_day,
            offer_number: agreed.number,
            terms: made,
            loan: match deal {
                Deal::Loan(loan) => Some(loan),
                Deal::Transfer { .. } => None,
            },
            offer: (offer, offer_at),
            contract: (contract, contract_at),
            object: leaving.object,
            person,
            manager,
        })
    }

    /// Writes an arranged transfer: the offer and the contract into their slots, the entry and the move due into blocks
    /// of the game's heap, then the links to them, the offer and the move due last. When a write fails, the ones before
    /// it are put back and the blocks given back.
    pub fn apply_arranged_transfer(&self, plan: &ArrangedPlan) -> Result<Arranged, MoveError> {
        let t = self.transfers_layout()?;
        // Refused before any block of the game's heap is taken, as the writes themselves would be.
        if let Some(lock) = self.structure_lock() {
            return Err(MoveError::Locked(lock));
        }
        let mut blocks = Vec::new();
        let mut placed = (0, 0);
        let result = self
            .arranged_patches(t, plan, &mut blocks, &mut placed)
            .and_then(|all| self.apply_patches(&all).map(|()| all));
        match result {
            Ok(patches) => Ok(Arranged {
                player: plan.player,
                move_day: plan.move_day,
                offer: placed.0,
                contract: placed.1,
                patches,
                blocks,
            }),
            Err(err) => {
                self.give_back(&blocks);
                Err(err)
            }
        }
    }

    /// Takes an arranged transfer back, provided the game still holds everything it wrote: until the day of the move.
    /// The blocks of the game's heap it took are given back.
    pub fn undo_arranged_transfer(&self, done: &Arranged) -> Result<(), MoveError> {
        self.undo_patches(&done.patches)?;
        self.give_back(&done.blocks);
        Ok(())
    }

    /// The writes of an arranged transfer: the offer and the contract placed, the entry and the move due made in blocks
    /// of the game's heap, then the links to them. `placed` gets where the offer and the contract went.
    fn arranged_patches(
        &self,
        t: &TransfersLayout,
        plan: &ArrangedPlan,
        blocks: &mut Vec<u64>,
        placed: &mut (u64, u64),
    ) -> Result<Vec<Patch>, MoveError> {
        let p = &self.layout.player;
        let mut links = Patches::default();
        let offer = self.place(&plan.offer.1, &plan.offer.0, &mut links, blocks)?;
        let contract = self.place(&plan.contract.1, &plan.contract.0, &mut links, blocks)?;
        *placed = (offer, contract);
        let entry = self.heap_block(t.entry_size as usize)?;
        blocks.push(entry);
        let kind = if plan.loan.is_some() { LOAN_KIND } else { TRANSFER_KIND };
        self.process.write_bytes(entry, &entry_bytes(t, contract, plan.offer_number, kind))?;
        let due = self.heap_block(t.due_size as usize)?;
        blocks.push(due);
        self.process.write_bytes(due, &due_bytes(t, plan.person, plan.move_day, plan.offer_number))?;

        let entries_at = plan.object + p.at(t.player_entries, 0);
        self.add_to_record(t, entries_at, &entry.to_le_bytes(), &mut links, blocks)?;
        let numbers_at = plan.object + p.at(t.player_offer_numbers, 0);
        self.add_to_record(t, numbers_at, &plan.offer_number.to_le_bytes(), &mut links, blocks)?;
        // The offer, then the move due, once all they lead to is in place; then the flag that a move is due.
        self.add_to_list(plan.manager + t.offers, &offer.to_le_bytes(), &mut links, blocks)?;
        self.add_to_list(plan.manager + t.due, &due.to_le_bytes(), &mut links, blocks)?;
        let flag = self.process.read_bytes(plan.manager + t.due_flag, 1)?;
        links.push(plan.manager + t.due_flag, flag, [1]);
        Ok(links.0)
    }

    /// The transfer manager: the one object of its class. Found once, in a pass over the game's memory, and kept while
    /// it is still there: the game makes another when it loads a save.
    fn transfer_manager(&self, t: &TransfersLayout) -> Result<u64, MoveError> {
        let mut kept = self.transfer_manager.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((manager, vtable)) = *kept
            && self.process.read_u64(manager).ok() == Some(vtable)
        {
            return Ok(manager);
        }
        let vtables = rtti::vtables(&self.process, &self.plugin, &t.manager_class);
        let objects = rtti::objects(&self.process, &self.plugin, &vtables)?;
        let managers: Vec<(u64, u64)> =
            objects.into_iter().flat_map(|(vtable, at)| at.into_iter().map(move |manager| (manager, vtable))).collect();
        match managers[..] {
            [found] => {
                *kept = Some(found);
                Ok(found.0)
            }
            _ => Err(MoveError::NoTransfers),
        }
    }

    /// One of the manager's lists of pointers, when it checks out.
    fn manager_list(&self, header: u64) -> Result<PointerList, MoveError> {
        PointerList::read_up_to(&self.process, header, MOST_OFFERS)?.ok_or(MoveError::NoTransfers)
    }

    /// Where an object of `size` bytes of the class `model` is one of goes (see [`Placement`]).
    fn placement(&self, model: u64, size: u64, pools: &crate::profile::PoolLayout) -> Result<Placement, MoveError> {
        let pool = self.pool_holding(model, size)?;
        match pool.take(&self.process, pools) {
            Ok(taken) => Ok(Placement::Slot { slot: taken.slot, was: taken.was, pool: taken.pool }),
            Err(MoveError::PoolFull) => Ok(Placement::Heap),
            Err(err) => Err(err),
        }
    }

    /// The writes that place `bytes` where `placement` says, and where that is: a block of the game's heap is taken
    /// now.
    fn place(
        &self,
        placement: &Placement,
        bytes: &[u8],
        patches: &mut Patches,
        blocks: &mut Vec<u64>,
    ) -> Result<u64, MoveError> {
        match placement {
            Placement::Slot { slot, was, pool } => {
                patches.push(pool.address, pool.before.clone(), pool.after.clone());
                patches.push(*slot, was.clone(), bytes.to_vec());
                Ok(*slot)
            }
            Placement::Heap => {
                let block = self.heap_block(bytes.len())?;
                blocks.push(block);
                patches.push(block, vec![0; bytes.len()], bytes.to_vec());
                Ok(block)
            }
        }
    }

    /// Adds `element` (4 or 8 bytes) to the list of the record the pointer at `at` leads to; a record of its own when
    /// the pointer is null, in a block of the game's heap.
    fn add_to_record(
        &self,
        t: &TransfersLayout,
        at: u64,
        element: &[u8],
        patches: &mut Patches,
        blocks: &mut Vec<u64>,
    ) -> Result<(), MoveError> {
        let record = self.process.read_u64(at)?;
        if record != 0 {
            if !is_user_pointer(record) {
                return Err(MoveError::UnusualContract);
            }
            return self.add_to_list(record, element, patches, blocks);
        }
        let size = element.len() as u64;
        let buffer = self.heap_block((NEW_LIST_ROOM * size) as usize)?;
        blocks.push(buffer);
        self.process.write_bytes(buffer, element)?;
        let made = self.heap_block(t.record_size as usize)?;
        blocks.push(made);
        let words: Vec<u8> =
            [buffer, buffer + size, buffer + NEW_LIST_ROOM * size].iter().flat_map(|w| w.to_le_bytes()).collect();
        self.process.write_bytes(made, &words)?;
        patches.push(at, 0u64.to_le_bytes(), made.to_le_bytes());
        Ok(())
    }

    /// Adds `element` (4 or 8 bytes) at the end of the list `(begin, end, capacity)` at `header`: in the room it has, or
    /// in a block of the game's heap half as large again, which the list then points at. The old block stays as it was,
    /// for the undo.
    fn add_to_list(
        &self,
        header: u64,
        element: &[u8],
        patches: &mut Patches,
        blocks: &mut Vec<u64>,
    ) -> Result<(), MoveError> {
        let size = element.len() as u64;
        let words = self.process.read_bytes(header, 24)?;
        let word = |i: usize| u64::from_le_bytes(words[i * 8..i * 8 + 8].try_into().expect("8 bytes"));
        let (begin, end, capacity) = (word(0), word(1), word(2));
        let empty = begin == 0 && end == 0 && capacity == 0;
        let sound = empty
            || (is_user_pointer(begin)
                && begin <= end
                && end <= capacity
                && (end - begin).is_multiple_of(size)
                && (capacity - begin).is_multiple_of(size)
                && (capacity - begin) / size <= MOST_OFFERS);
        if !sound {
            return Err(MoveError::NoTransfers);
        }
        if !empty && end < capacity {
            patches.push(end, self.process.read_bytes(end, element.len())?, element.to_vec());
            patches.push(header + 8, end.to_le_bytes(), (end + size).to_le_bytes());
            return Ok(());
        }
        let len = if empty { 0 } else { (end - begin) / size };
        let room = grown_capacity(len);
        let buffer = self.heap_block((room * size) as usize)?;
        blocks.push(buffer);
        let mut bytes = if empty { Vec::new() } else { self.process.read_bytes(begin, (end - begin) as usize)? };
        bytes.extend_from_slice(element);
        self.process.write_bytes(buffer, &bytes)?;
        let after: Vec<u8> =
            [buffer, buffer + (len + 1) * size, buffer + room * size].iter().flat_map(|w| w.to_le_bytes()).collect();
        patches.push(header, words, after);
        Ok(())
    }

    /// Gives back blocks of the game's heap, as far as it can.
    fn give_back(&self, blocks: &[u64]) {
        for &block in blocks.iter().rev() {
            let _ = self.heap_free(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> TransfersLayout {
        crate::profile::windows_layout().transfers.expect("the Windows layout places the transfers")
    }

    fn agreed(today: FmDate) -> Agreed {
        Agreed {
            person: 0x1_0000_0010,
            buyer: 0x2_0000_0020,
            seller: 0x3_0000_0030,
            number: 5181,
            count: 6403,
            today,
            move_day: today.plus_days(1),
        }
    }

    #[test]
    fn an_offer_is_agreed_for_the_day_of_the_move() {
        let t = layout();
        let template = vec![0xAA; t.offer_size as usize];
        let today = FmDate::from_number(20261016).expect("a date");
        let offer = offer_bytes(&t, &template, agreed(today), 7_777_777);
        let half = |at: u64| u32::from_le_bytes(offer[at as usize..at as usize + 4].try_into().expect("4 bytes"));
        let word = |at: u64| u64::from_le_bytes(offer[at as usize..at as usize + 8].try_into().expect("8 bytes"));
        assert_eq!(word(t.offer_person), 0x1_0000_0010);
        assert_eq!((word(t.offer_buyer), word(t.offer_buyer_again)), (0x2_0000_0020, 0x2_0000_0020));
        assert_eq!(word(t.offer_seller), 0x3_0000_0030);
        assert_eq!((half(t.offer_fee), half(t.offer_number), half(t.offer_count)), (7_777_777, 5181, 6403));
        assert_eq!(offer[t.offer_state as usize], AGREED);
        assert_eq!(offer[t.offer_kind as usize], TRANSFER_OFFER);
        assert_eq!(word(t.offer_payment), 0);
        let day =
            |at: u64| FmDate::from_raw((half(at) & 0xFFFF) as u16, (half(at) >> 16) as u16).map(FmDate::to_number);
        assert_eq!(day(t.offer_made), Some(20261016));
        assert_eq!(day(t.offer_move_day), Some(20261017));
        assert_eq!(day(t.offer_lapses), Some(20261115));
        // What the layout does not name is the model's.
        assert_eq!(offer[t.offer_size as usize - 1], 0xAA);
    }

    #[test]
    fn the_model_of_a_transfer_is_a_transfer_offer_once_the_class_is_known() {
        let known = OfferClasses { transfer: vec![0x10], loan: vec![0x20] };
        assert!(known.transfer_model(0x10));
        assert!(!known.transfer_model(0x20));
        // Where the game's names do not give the class, any offer may be the model, as before.
        assert!(OfferClasses::default().transfer_model(0x20));
    }

    #[test]
    fn a_transfer_with_a_fee_is_the_model_before_an_agreed_free_move() {
        let t = layout();
        let offer = |state: u8, kind: u8, flags: u32, payment: u64| {
            let mut bytes = vec![0u8; t.offer_size as usize];
            bytes[t.offer_state as usize] = state;
            bytes[t.offer_kind as usize] = kind;
            bytes[t.offer_flags as usize..t.offer_flags as usize + 4].copy_from_slice(&flags.to_le_bytes());
            bytes[t.offer_payment as usize..t.offer_payment as usize + 8].copy_from_slice(&payment.to_le_bytes());
            model_rank(&t, &bytes)
        };
        // As on the Mac save of July 2027: the agreed transfers with the flags the game gives most are free moves.
        let free_agreed = offer(AGREED, 8, TRANSFER_FLAGS, 0);
        let fee_made = offer(MADE, TRANSFER_OFFER, TRANSFER_FLAGS, 0);
        let fee_agreed = offer(AGREED, TRANSFER_OFFER, 0x1000_0040, 0);
        assert!(fee_made > free_agreed);
        assert!(fee_agreed > fee_made);
        assert!(offer(AGREED, TRANSFER_OFFER, TRANSFER_FLAGS, 0) > fee_agreed);
        assert!(offer(AGREED, TRANSFER_OFFER, TRANSFER_FLAGS, 0x7b_5eb0_4d08) < offer(AGREED, 2, TRANSFER_FLAGS, 0));
        assert_eq!(offer(8, TRANSFER_OFFER, TRANSFER_FLAGS, 0), None);
    }

    #[test]
    fn a_move_due_names_the_player_the_day_and_the_offer() {
        let t = layout();
        let day = FmDate::from_number(20261018).expect("a date");
        let due = due_bytes(&t, 0x1a1_da30_7ea0, day, 5181);
        assert_eq!(due, [0xa0, 0x7e, 0x30, 0xda, 0xa1, 0x01, 0, 0, 0x23, 0x01, 0xea, 0x07, 0x3d, 0x14, 0, 0]);
    }

    #[test]
    fn an_entry_waits_for_its_day() {
        let t = layout();
        let entry = entry_bytes(&t, 0x1a1_e5c8_43d8, 5181, TRANSFER_KIND);
        assert_eq!(entry.len(), 0x48);
        assert_eq!(&entry[0x10..0x18], &0x1a1_e5c8_43d8u64.to_le_bytes());
        assert_eq!(&entry[0x28..0x30], &[0x3d, 0x14, 0, 0, 0x81, 0x69, 0x67, 0xff]);
        assert_eq!(&entry[0x3E..0x42], &[2, 1, 1, 2]);
    }

    #[test]
    fn a_full_list_grows_by_half_again() {
        assert_eq!(grown_capacity(103), 154);
        assert_eq!(grown_capacity(5178), 7767);
        assert_eq!(grown_capacity(0), NEW_LIST_ROOM);
        assert_eq!(grown_capacity(1), NEW_LIST_ROOM);
    }

    fn loan_layouts() -> (TransfersLayout, LoanOfferLayout, ContractLayout, LoanTermsLayout) {
        let layout = crate::profile::windows_layout();
        let t = layout.transfers.expect("the Windows layout places the transfers");
        let l = t.loan.clone().expect("the Windows layout places the loan offers");
        let terms = layout.loan.and_then(|loan| loan.terms).expect("the Windows layout places the loans' terms");
        (t, l, layout.contract, terms)
    }

    /// The loan of the first live test on Game Pass 0.9.2562.0: 80% of £82,454 a week and £50,000 a month.
    const LOAN: LoanDeal = LoanDeal { until: 20280629, wage_share: 80, monthly_fee: 50_000, option_fee: None };

    #[test]
    fn a_loan_offer_holds_the_monthly_fee_and_the_terms() {
        let (t, l, ..) = loan_layouts();
        let template = vec![0xAA; l.offer_size as usize];
        let today = FmDate::from_number(20270804).expect("a date");
        let offer = loan_offer_bytes(&t, &l, &template, agreed(today), &LOAN, 65_963);
        let half = |at: u64| u32::from_le_bytes(offer[at as usize..at as usize + 4].try_into().expect("4 bytes"));
        let short = |at: u64| u16::from_le_bytes([offer[at as usize], offer[at as usize + 1]]);
        assert_eq!((offer[t.offer_state as usize], offer[t.offer_kind as usize]), (AGREED, LOAN_OFFER));
        assert_eq!(half(t.offer_fee), 50_000);
        assert_eq!(half(t.offer_flags), TRANSFER_FLAGS);
        assert_eq!(half(l.offer_flags), u32::from(LOAN_FLAGS));
        assert!(l.offer_wage.iter().all(|&at| half(at) == 65_963));
        // No future fee: the place holds -1, and its kind none.
        assert_eq!((half(l.offer_unused_fee), half(l.offer_option_fee)), (50_000, u32::MAX));
        assert_eq!(offer[l.offer_fee_kind as usize], NO_FUTURE_FEE);
        assert!(l.offer_unset.iter().all(|&at| half(at) == u32::MAX));
        assert!(l.offer_share.iter().all(|&at| short(at) == 80));
        assert_eq!(half(l.offer_length), SEASON_LOAN);
        assert!(l.offer_cleared.iter().all(|&at| offer[at as usize..at as usize + 8] == [0; 8]));
        // A loan offer keeps no pointer where a transfer offer keeps how its fee is paid, nor what follows its flags.
        assert_eq!(offer[t.offer_payment as usize], 0xAA);
        assert_eq!(offer[t.offer_flags as usize + 4], 0xAA);
    }

    #[test]
    fn a_loan_contract_runs_from_the_move_to_the_last_day() {
        let (_, l, c, terms) = loan_layouts();
        let mut model = vec![0x55; l.contract_size as usize];
        // Dates with flags, which the contract keeps.
        model[c.signed as usize..c.signed as usize + 4].copy_from_slice(&[0x9f, 0x34, 0xeb, 0x07]);
        model[c.expires as usize..c.expires as usize + 4].copy_from_slice(&[0xb5, 0x34, 0xec, 0x07]);
        let move_day = FmDate::from_number(20270805).expect("a date");
        let contract =
            loan_contract_bytes(&c, &l, &terms, &model, 0x1ff5e7deae8, 0x1ff55d9c1d0, &LOAN, 65_963, 3, move_day);
        let half = |at: u64| u32::from_le_bytes(contract[at as usize..at as usize + 4].try_into().expect("4 bytes"));
        let word = |at: u64| u64::from_le_bytes(contract[at as usize..at as usize + 8].try_into().expect("8 bytes"));
        assert_eq!(word(c.owner), 0x1ff5e7deae8);
        assert_eq!((word(c.team), word(c.senior_team)), (0x1ff55d9c1d0, 0x1ff55d9c1d0));
        assert_eq!(contract[c.job as usize], PLAYER_JOB);
        assert_eq!(half(c.signed), 0x07eb_34d9, "5 August 2027, the model's flags kept");
        assert_eq!(half(c.expires), 0x07ec_34b5, "29 June 2028");
        assert_eq!(contract[c.joined as usize..c.joined as usize + 4], UNSET_DATE);
        assert_eq!((half(c.wage), half(l.contract_wage_again)), (65_963, 65_963));
        assert_eq!((contract[c.squad_status as usize], contract[l.contract_status_again as usize]), (3, 3));
        assert_eq!(contract[c.squad_number as usize], NO_SQUAD_NUMBER);
        assert!(l.contract_cleared.iter().all(|&at| word(at) == 0));
        assert!(l.contract_unset.iter().all(|&at| half(at) == u32::MAX));
        assert!(l.contract_zero.iter().all(|&at| half(at) == 0));
        assert_eq!(half(l.contract_length), SEASON_LOAN);
        assert_eq!(contract[terms.flags as usize..terms.flags as usize + 2], LOAN_FLAGS.to_le_bytes());
        assert_eq!(
            (half(terms.option_fee), half(terms.monthly_fee), half(terms.unused_fee)),
            (u32::MAX, 50_000, 50_000)
        );
        assert_eq!(contract[l.contract_fee_kind as usize], NO_FUTURE_FEE);
    }

    #[test]
    fn an_option_to_buy_is_marked_optional_in_the_offer_and_the_loan_contract() {
        let (t, l, c, terms) = loan_layouts();
        let loan = LoanDeal { option_fee: Some(30_000_000), ..LOAN };
        let today = FmDate::from_number(20261030).expect("a date");
        // Models of a mandatory fee's kind, which the editor's loan does not keep.
        let mut template = vec![0xAA; l.offer_size as usize];
        template[l.offer_fee_kind as usize] = 0x64;
        let offer = loan_offer_bytes(&t, &l, &template, agreed(today), &loan, 65_963);
        let half =
            |bytes: &[u8], at: u64| u32::from_le_bytes(bytes[at as usize..at as usize + 4].try_into().expect("4"));
        assert_eq!(half(&offer, l.offer_option_fee), 30_000_000);
        assert_eq!(offer[l.offer_fee_kind as usize], OPTIONAL_FUTURE_FEE);
        let mut model = vec![0x55; l.contract_size as usize];
        model[l.contract_fee_kind as usize] = 0x64;
        let contract =
            loan_contract_bytes(&c, &l, &terms, &model, 0x1ff5e7deae8, 0x1ff55d9c1d0, &loan, 65_963, 3, today);
        assert_eq!(half(&contract, terms.option_fee), 30_000_000);
        assert_eq!(contract[l.contract_fee_kind as usize], OPTIONAL_FUTURE_FEE);
        // An option of nothing, or of more than the editor writes, is refused.
        for fee in [0, MAX_FEE + 1] {
            let refused = check_loan(&LoanDeal { option_fee: Some(fee), ..LOAN });
            assert!(matches!(refused, Err(MoveError::OutOfRange { field: "optionFee", .. })), "{fee}");
        }
        assert!(check_loan(&loan).is_ok());
    }

    #[test]
    fn a_season_loan_ends_on_29_june_a_month_away_at_least() {
        let on =
            |today: i64, contract: Option<i64>| season_loan_end(FmDate::from_number(today).expect("a date"), contract);
        assert_eq!(on(20270804, None), 20280629);
        assert_eq!(on(20280301, None), 20280629);
        assert_eq!(on(20280610, None), 20290629);
        // Never past his contract, one that has not run out yet.
        assert_eq!(on(20270804, Some(20280131)), 20280131);
        assert_eq!(on(20270804, Some(20310630)), 20280629);
        assert_eq!(on(20270804, Some(20270630)), 20280629);
    }

    #[test]
    fn a_loan_between_first_teams_that_shares_the_wage_is_the_model() {
        let (t, l, ..) = loan_layouts();
        let offer = |state: u8, flags: u32, share: u16, usual: bool| {
            let mut bytes = vec![0u8; l.offer_size as usize];
            bytes[t.offer_state as usize] = state;
            bytes[t.offer_flags as usize..t.offer_flags as usize + 4].copy_from_slice(&flags.to_le_bytes());
            for &at in &l.offer_share {
                bytes[at as usize..at as usize + 2].copy_from_slice(&share.to_le_bytes());
            }
            for &(at, value) in &l.offer_usual {
                bytes[at as usize] = if usual { value } else { value.wrapping_add(1) };
            }
            loan_model_rank(&t, &l, &bytes)
        };
        assert_eq!(offer(8, TRANSFER_FLAGS, 100, true), None);
        assert!(offer(MADE, TRANSFER_FLAGS, 0, true) > offer(MADE, 0x1000_0040, 100, true));
        assert!(offer(MADE, TRANSFER_FLAGS, 100, true) > offer(MADE, TRANSFER_FLAGS, 0, true));
        assert!(offer(MADE, TRANSFER_FLAGS, 80, true) > offer(MADE, TRANSFER_FLAGS, 80, false));
        assert!(offer(AGREED, TRANSFER_FLAGS, 0, false) > offer(MADE, TRANSFER_FLAGS, 100, true));
    }
}
