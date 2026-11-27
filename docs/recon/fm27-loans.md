# FM27: loans

Notes behind showing a player's loan and recalling him from it. Read on the Windows Game Pass build (0.9.2409.0), on
a save of 17 September 2026 whose manager runs Manchester United, with the probe (`refs`, `dump`, `loans`).

## The loan contract

A player on loan keeps his contract with his own club (`player+0x330`) and has a second one with the team he is on
loan to, of its own class, `db::LOAN_CONTRACT` (his own is a `db::FULL_CONTRACT`). It is reached through the pointer
right after his contract:

| Where | What |
| --- | --- |
| player `+0x338` | pointer to a record of his other contracts, shifted with the contract for player-coaches |
| record `+0x00` | pointer to the loan contract, null when he is not on loan |
| record `+0x30` | a `db::BASIC_CONTRACT` of a person who is not a player (likely his agent's) |

Only the player points at the record, and only the record points at the loan contract. The loan contract is laid
out as a contract up to the squad number (`[contract]` in `profiles/fm27/layout.toml`):

| Offset | Field | André Onana, Manchester United → Trabzonspor |
| --- | --- | --- |
| `+0x08` | owner: the player's identity block | Onana |
| `+0x10`, `+0x18` | the team he is on loan to | Trabzonspor |
| `+0x20` | `i32` wage, pounds per week | 113,300 (110,000 in his own contract) |
| `+0x44` | from (the contract's `signed`) | 29 June 2026 |
| `+0x48` | until (`expires`) | 30 June 2027 |
| `+0x4C` | joined | 29 June 2026 |
| `+0x54` | squad status there | 2 |
| `+0x5D` | squad number there | 24, as he wears it at Trabzonspor |

The bytes after the squad number (`+0x60` to `+0x100`) hold the loan's own terms; they are not mapped yet. Onana's
read 3,212 twice at `+0xA0` and `+0xA8`, 58,333 twice at `+0xCC`, his wage again at `+0xD4`, and 180 and 270 at
`+0xD8`.

The record is 32 bytes, laid out one after another with the others of its kind, so reading past its end shows the
next player's:

| Offset | What | Records holding one |
| --- | --- | --- |
| `+0x00` | the loan contract (`db::LOAN_CONTRACT`) | 3,598 |
| `+0x08` | a contract with a club's B team (`db::B_CLUB_CONTRACT`) | 2,189 |
| `+0x10` | a basic or full contract (a few player-coaches) | 11 |
| `+0x18` | another contract with a B team | 60 |

When the game ended the loans of Onana and Bayındır, whose records held nothing else, it left them without a record:
null at `+0x338`. Their team (`+0x130`) pointed at Manchester United's again, while `+0x138` kept Trabzonspor's, the
last team they played for.

## Who has one

On the save, 3,596 players had a loan contract, and every one of them passed the editor's checks (class and owner).
5,637 players had the record, so 2,041 have one without a loan. 5,624 players play for a team of another club than
their contract's: 3,594 of them are on loan; nearly all the rest belong to a club's second team, which the game keeps
as a club of its own (such as "VfB Stuttgart II" or "FC Twente/Heracles Academie").

## The terms

Past the fields it shares with a contract, the loan contract keeps the terms the game's Loan Club tab shows. RootTourist
mapped them on the Steam build 26340721 against 17 loans in the English UI (in a comment on PR #101); the layout places
them as `[loan.terms]`:

| Offset | Type | In the game |
| --- | --- | --- |
| `+0xB0` | `u16` flags | bit 0 "Can play against own team", bit 1 "Can play in cup matches", bit 2 "Can be recalled", bit 12 "Loan cannot be terminated"; bits 3, 8, 13 and 15 occur, and the game shows nothing for them |
| `+0xC0` | `i32` pounds | "Optional Future Fee", the option to buy; -1 for none |
| `+0xCC` | `i32` pounds | "Monthly playing fee"; -1 for none |
| `+0xD0` | `i32` pounds | "Unused Monthly Fee"; -1 for none |
| `+0xD4` | `i32` pounds | the wage again, as at `+0x20` (on 351 of 371 loans) |

On the Steam save of 15 August 2027, 113 of 371 loans could not be recalled in the game and 94 could not be
terminated, 71 neither. Anton Kade's, from FC Augsburg to Liverpool, holds flags `0x1002` (cup matches, no recall, not
to be terminated), an option to buy of £23,245,368 and monthly fees of £371,665 (played) and £725,477 (unused). The
profile lists the terms, the recall dialog says what they would not allow in the game, and Make permanent starts its
fee from the option to buy.

## Recalling a player

The editor never ends a loan itself: the game would have to take him out of one team's list and put him in another,
free the loan contract and tell both clubs. It moves the loan's end (`loan.expires`) instead, and lets the game end
the loan, as at the end of any loan.

**The end is the loan's last day.** In the game, André Onana and Altay Bayındır (Manchester United's goalkeepers on
loan at Trabzonspor and Celta) were recalled with their end moved to the day after the game's date: both came back,
two days on. A loan "until 30 June" is one he plays through 30 June, back at his club on 1 July. So a recall makes the
game's date the loan's last day, and the player is back the next day: one day on.

A day the game has finished would never be one again, and a loan whose last day it was might never end, so the editor
writes only the game's date or a later one. While a loan ends on the game's date, the profile says his return is
pending instead of offering the recall; a loan whose end has passed while he is still away gets a warning and the
recall again.

The game's own recall needs "Can be recalled" (see [The terms](#the-terms)). The editor's does not go through it: it
moves the loan's last day, and the game ends the loan when that day is over.

**A loan that ends may become a transfer.** Anton Kade, recalled by the editor on 15 August 2027 (no recall, not to be
terminated, an option to buy of £23,245,368), was not back at FC Augsburg the next day: Liverpool had signed him for his
option. Its balance and transfer budget were both £23,245,368 lower, and he was on a new contract of £105,519 a week
until 30 June 2033, signed that day. So a recall takes the option to buy away too (`loan.option_fee`, written -1),
which its undo writes back with the loan's end, and the recall dialog says so.

## Making a loan permanent

A player on loan is in a list of the team he plays for, at the club he is on loan to, and in a list of his own club:
its senior team's, or for a youngster the youth team's he played for (Mirza Ćatović, on loan from VfB Stuttgart to
Barcelona, is in Stuttgart's under-19s' list and not in its senior team's). Making the loan permanent (`plan_loan_permanent`)
leaves him where he plays, and writes:

1. his own contract: both team pointers to the senior team of the club he stays at, signed and joined on the game's
   date, the wage and end when given, the squad number and squad status his loan contract has there, and off the
   transfer and loan lists;
2. his own club: a captaincy he holds is left empty, and every list that holds him loses him;
3. the loan, let go as the game lets one go: null at `+0x338` when the record holds nothing else, the loan contract's
   slot in the record otherwise. The loan contract and the record are left with nothing pointing at them.

Each write keeps the bytes it found, so one undo puts the loan back. Checked with the probe's `permanent 2000332528
--restore` (9 writes): while made permanent he read as Barcelona's, not on loan, with the squad status of his loan
contract (13), and afterwards as on loan again from Stuttgart until 30 June 2027. Whether the game takes him as
Barcelona's over the next days is to be checked in the game.

## Arranging a loan

The editor arranges a loan as it arranges a transfer (`arranged.rs`, see `fm27-transfer-offers.md`): an agreed offer
for the next day, of the game's loan class, the loan contract the player is to have held by an entry of his, and the
move due on that day. On the day the game makes the loan as it makes its own. Read on the Windows Game Pass build
0.9.2562.0, on a save of 4 August 2027, from the game's 608 loan offers and the 187 loan contracts its entries held,
and tried there and on Steam build 26797085, on a save of October 2026.

The offer starts from one of the game's own loans between first teams (`loan_model_rank`) and holds the loan's terms
([The loan offer](fm27-transfer-offers.md#the-loan-offer)); the loan contract starts from the loan contract of that
offer's player, with the club he joins, from the day of the move to the loan's last day, his own wage times the share
the club pays (80% of £18,336 a week is £14,668, as the game works it out), his squad status, no squad number, and the
same terms. The game keeps `0x1000` in both for a loan of a season, whatever day it ends on (382 of its season loans
end on 29 June 2028, 91 on 31 December 2027): the loan ends on the loan contract's end, as a recall shows.

What the game made of the loans the editor arranged:

| Player | From, to | Day | Terms | What the game did |
| --- | --- | --- | --- | --- |
| Marc Bernal | Barcelona, Real Betis | 6 August 2027, Game Pass | to 31 January 2028 | made it on the loan contract itself, with a squad number; the entry gone |
| Rodrigo Mendoza | Atlético Madrid, Lyon | 28 October 2026, Steam | to 29 June 2027 | made it on the loan contract itself; the entry gone |
| Rio Ngumoha | Liverpool, Beşiktaş | 5 August 2027, Game Pass | 80% of the wage, £50,000 a month, a £25M future fee | made it, on a copy of the loan contract; the entry kept, for the fee |
| Jorrel Hato | Chelsea, Lyon | 7 August 2027, Game Pass | a £1M future fee, the entry holding the contract for it | made it, on a loan contract it made from the offer |
| Rodrigo Mendoza | Atlético Madrid, Lyon | 20 October 2026, Steam | a £12M future fee, the same | closed the offer as made (state 20) and left him where he was |
| Romeo Akachukwu | Southampton, Altrincham | 24 October 2026, Steam | a £500K future fee, the same | the same |
| Romeo Akachukwu | Southampton, Southend United | 22 October 2026, Steam | a £500K future fee, the same | refused it: the offer in state 16, the entry in state 4 |
| Endrick | Real Madrid, Juventus | 31 October 2026, Steam | 80% of the wage, £50,000 a month, an option to buy for £40M (kind 0xFE) | made it on the loan contract itself, shown with an "Optional Future Fee" of €46.5M; the entry gone. With the loan's end moved to that day, he was back at Real Madrid on his own contract, nothing of the loan left |

So the game makes a loan with no future fee on any day, in a transfer window or out of one, as it makes an arranged
transfer: Lyon's window was closed on 28 October 2026. A future fee is of one of two kinds
([below](#optional-and-mandatory-future-fees)): an option to buy, which the club he joins may take up when the loan
ends, and a mandatory fee. For a mandatory one the game keeps the entry until the fee is paid, and then signs him on
the contract the entry holds. The editor's first
tries left the kind as their model's, the one of no fee, which the game took as a mandatory fee: with the loan contract
in the entry (Ngumoha's, his loan ended early with the fee kept), it signed him for good for €29M on the loan's wage,
until the loan's old end, and showed Hato's fee as "Mandatory", as it shows O'Riley's (one of its own). The game's own
mandatory loans hold a full contract there instead, with the club he is on loan at, from the loan's last day, for
three to five seasons (75 of its 98 made ones with a fee on Game Pass; O'Riley's, from Brighton to Lyon, to 30 June
2032); with one there, the game made Hato's loan on Game Pass in August but closed the offers of the two on Steam in
October and moved neither. Southend already had six players on loan, the National League's most (`max_players`, 6 in
its loan rules), which likely refused Akachukwu's. So the editor agrees no mandatory fee, only an option to buy, of
the option's kind: the game made Endrick's loan as it makes its own with an option, and kept no entry.

### Optional and mandatory future fees

On the Steam save of 30 October 2026 (build 26797085), the game had made 388 loans whose players still held their loan
contract. In every one the offer's `+0x124` and the loan contract's `+0xF7` hold the same byte, the kind of the future
fee:

| Kind | Made loans | The future fee | The player's entry for it |
| --- | --- | --- | --- |
| 0xFF | 331, with no fee (2 more with no fee hold 0xFE) | none | gone |
| 0xFE | 24 | an option to buy: the game shows an "Optional Future Fee" (Mathias Kvistgaarden, from Norwich to Brøndby; Jamie Gittens, from Chelsea to Everton; Adam Hložek, from Hoffenheim to Leeds) | gone |
| 0x64, 0x14 or 0xFF | 17, 9 and 4 | mandatory: a "Mandatory Future Fee" (Malo Gusto, from Chelsea to Juventus) | kept, in state 2, holding the full contract he signs when it is paid |

The offer's `+0x88` (0, 1 or 2) does not tell them apart: Hložek's holds 0, as every mandatory one does. What 0x64
(100) and 0x14 (20) say of a mandatory fee, a condition perhaps, is not known; nor `+0x125`, which varies in loans
with no future fee too.

The undo takes an arranged loan back until the day; the app refuses to move, swap or release the player another way
until then.

### Transfer windows

The game's own loans keep to the windows: in October 2026 they went only to clubs whose leagues take loans then, English
non-league clubs and Spanish regional ones. The windows are in a nation's rules record (see `fm27-league-rules.md`), under
`trwi`: each a first and a last day (`stdt`, `endt`: `dyom` the day, `mont` the month, `year` 2000 for the season's
first year and 2001 for its second), a kind (`wnty`), the division levels it is for (`hidl` the highest, `lwdl` the
lowest, 0 the top) and the seasons (`styr`, `enyr`); the levels are `dvlv`, the league of each, the top one first. The
game's own table names the kinds, as pairs of a name and its number: 1 `normal` (a window with no kind is one), 27
`loans_only`, 11 `domestic_loans_only`, 13 `french_joker_rule`, 26 `0_players_allowed`, 29 `2_free_transfers_max`, and
30 more. France's: normal from 15 June to 1 September and from 1 January to 1 February; from 1 September the joker
rule to 31 December and no signings to 5 January. England's: normal from 15 June to 1 September and from 1 January to
1 February, and from 14 June to 29 March for levels 4 and 5, the National League. The loan rules are `lnrl`, of kinds 0
`expiry_date`, 1 `division`, 2 `max_players` and 5 `max_days`. An arranged loan does not need an open window, as the
table above shows, so the editor does not read these.

## On the Mac

The Mac Steam build (26340721, arm64) keeps loans the same way, 0x40 earlier in the player as the person's fields
are: his contract at `+0x2F0` and the record of his other contracts right after it, at `+0x2F8`. The record and the
loan contract are laid out as on Windows, and so are the terms (`[loan]` and `[loan.terms]` in `layout-macos.toml`).
On the Arsenal save of 5 July 2027, a few days after most loans had ended:

- 92 players had a loan contract, every one passing the class (`N2db13LOAN_CONTRACTE`) and owner checks; 1,471 had
  a record, whose other slots held 1,370 contracts with B teams (`db::B_CLUB_CONTRACT`) and a few full or basic
  ones, as on Windows. Two records held a trial contract (`db::TRIAL_CONTRACT`) in the loan's slot, which the editor
  leaves alone.
- 1,458 players played for a team of another club than their contract's: the 92 on loan, and nearly all the rest at
  a club's second team.
- The terms read as the Loan Club tab would show them: Maxim De Cuyper (Brighton & Hove Albion to Olympique
  Lyonnais until 29 June 2028) has flags `0x1006`, so he can be recalled but the loan cannot be terminated, an option
  to buy of £32,394,208 and monthly fees of £52,761; Rob Dickie (Bristol City to FC Cincinnati) `0x1002`, neither.

With the probe, each put back: De Cuyper's loan made permanent (`permanent 18110473 --restore`, 11 writes: he read as
Lyon's, not on loan, with his loan contract's squad status, then on loan again), and a recall's writes, his loan's
end to the game's date and his option to buy to -1, each read back and restored.

### Arranging a loan on the Mac

The Mac Steam build 26797085 (Apple silicon) keeps the loan offer and the loan contract as Windows does
(`[transfers.loan]` in `layout-macos.toml`). On the Arsenal save of 18 July 2027 the probe's `instances` found 406
`LOAN_OFFER`s (`10LOAN_OFFER`), 0x128 bytes apart in a pool of their own, and 317 loan contracts, 0x100 apart in
theirs; both pools were full, so the editor's offer and loan contract went into blocks of the game's heap. The game had
made 165 of the loans, and for the 140 whose players were still on them each loan contract held what its offer held at
the Windows places: the wage, the monthly fee twice, how long the loan runs, the future fee and its kind (0xFF none in
109, 0xFE an option in 15, mandatory 0x64, 0x14 or 0x19 in 16), and `+0x114` at the contract's `+0xC8`. Where an
offer's two wages differ (3 loans), `+0x108` is the loan contract's wage and `+0x100` its `+0xD4`. In 71 of them the
game had set bit 8 (0x100) of the loan contract's flags, which the offer does not hold.

Marc Bernal, from Barcelona to Villarreal, arranged with `arrange-loan 2000295603 1777 --share 80 --monthly-fee 50000
--option-fee 30000000` on 18 July 2027: the game made the loan on 19 July, on the loan contract the editor wrote, until
29 June 2028, at £28,248 a week (80% of £35,310), £50,000 a month and an option to buy for £30,000,000, its kind 0xFE
kept, and set bit 8 of its flags as for its own. The offer was made (state 20) and his entry gone, as for the game's own
loans with an option; all 12 of the probe's `health` checks passed after it.
