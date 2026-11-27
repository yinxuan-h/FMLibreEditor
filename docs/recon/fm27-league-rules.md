# FM27: the rules a competition's squads are registered under

Notes behind the probe's `rules`. Everything here was read on the Windows Game Pass build 0.9.2524.0, on a save of 19
October 2026, with the probe and a few scripts around it; the Süper Lig's limit on foreign players was then changed in
the running game and looked at on the game's own screens (see [What the game makes of a change](#what-the-game-makes-of-a-change)).
The Mac keeps them alike, a few objects laying them out further on (see [On the Mac](#on-the-mac)); nothing was read on
Steam for Windows.

## In short

The rules are data, not code. The game reads each nation's rule files into records and keeps them with the nation;
from those it makes, whenever it loads a save, the lists it counts a squad against: one for this season and one for
the next. A rule is a kind (`all_foreign_players_in_squad`), which way it limits, and a value.

```text
db::NATION +0x108 ─> the container of its men's teams (db::NATIONAL_TEAM_CONTAINER)
    container +0x570   the record its rule files make together      ─┐ a save keeps these;
    container +0x588   the list of the records of the files themselves ─┘ the lists are made from them

sicomps::ACTUAL_COMP   a competition as it is played, found by its class
    +0x120 ─> its db::COMP (which points back from +0xB0)
    +0x38     its own rules, a record: `fxri` names one of its nation's sets of fixture rules
    +0x370 ─> its rule group
rule group +0x1E0 ─> its nation's rules as played ─ +0x60 ─> pointers to the sets of fixture rules
fixture rules +0x70 ─> this season's list of squad rules: pointers to rules of 0x20 bytes
              +0x68 ─> next season's fixture rules, with a list of their own
```

## Records

A record is a list `(begin, end, capacity)` of 16-byte entries: a `u32` key of four characters ('sqsr', the first
character in the highest byte), four spare bytes, and a pointer to a typed value. A typed value is 16 bytes too: a
pointer to an object that says what type it is, and 8 bytes of value. There is one such object per type, told apart
by its class:

| Class | The value |
| --- | --- |
| `sicore::RAW_TYPED_TRAITS<int>`, `<unsigned>`, `<bool>` | in the low bytes; the rest of the 8 keep what was there before |
| `sicore::RAW_TYPED_TRAITS<__int64>` | a competition as a pair of numbers, the UID in each half |
| `sicore::INTERNAL_OBJECT_TYPED_TRAITS<sicore::RECORD>` | a pointer to another record's `(begin, end, capacity)` |
| `sicore::INTERNAL_OBJECT_TYPED_TRAITS<sicore::TYPED_LIST>` | a pointer to a list's `(begin, end, capacity)` of pointers to typed values |
| `sicore::STRING_TYPED_TRAITS` | a pointer to an FM string |
| `sicore::NULL_TYPED_TRAITS` | nothing |

The rule files name their keys in words, and the game registers each word with its four characters in one long
function (`lea rdx, name; lea rcx, …; mov r8d, 'symb'; xor r9d, r9d; call`): 2,183 of them on this build. Those used
here:

| Key | Name | Key | Name |
| --- | --- | --- | --- |
| `fxrl` | fixture_rules | `fxri` | fixture_rules_index |
| `sqsr` | squad_selection_rules | `levl` | level |
| `type` | type | `valu` | value |
| `cmty` | compare_type | `info` | (a rule's conditions) |
| `styr` | start_year | `enyr` | end_year |
| `tfrd` | teams_from_division | `comp`, `DBID` | competition, its UID |
| `nati`, `Nnat` | nation, its UID | `updy` | season_update_day |
| `dyom`, `mont` | day_of_month, month | `trwi` | transfer_windows |

## A nation's rules

The container of a nation's men's teams (see the [national teams notes](fm27-national-teams.md)) holds them. At
`+0x570` is one record of all its rule files together, 35 keys for Türkiye: the files and their version, the nation,
the division levels with the competition of each, the teams it needs, its
competitions, its reserve leagues, and then the rules proper: `transfer_windows`, `transfer_rules`, `loan_rules`,
`work_permit_rules`, `contract_rules`, `fixture_rules`, `discipline_rules`, `season_update_day` (20 June),
`season_windows`, `start_date_rules`, `fixture_times` and helpers. Right after it, at `+0x588`, is a list of the
records of the files themselves, three for Türkiye: the structure (division levels, teams, competitions), the reserve
leagues, and the rules (27 keys, from `transfer_windows` on). So the rules are there twice.

The files do not all name their nation alike: `nati` is the UID as a number in Spain's (796), a record that holds it
under `Nnat` as a number in Türkiye's (799), and the same record with the UID as text in Brazil's ("1651"). The
container of a nation's women's teams has records of its own, at the same places, for the women's competitions.

`fixture_rules` is a list of sets, 10 for Türkiye: one for each division's league (the levels it is for in `hidl` and
`lwdl`), then the cups' and the youth competitions'. A set holds `squad_selection_rules`, a list of records:

| Key | What it holds |
| --- | --- |
| `type` | the kind of rule, a number (see [Kinds](#kinds-of-rule)) |
| `cmty` | which way it limits: -1 at most, 1 at least |
| `valu` | the value |
| `styr`, `enyr` | the years it is good for, either or both missing |
| `info` | a list of conditions, each a record whose `type` is a key: `1yol` (players of last season may be a year older), `Tk24` (players of Turkic nations who joined in 2023/24 count as domestic) |
| `tfrd` | in a cup's rules: the division whose teams the rule is for, a record holding its competition |

The Süper Lig's (set 0), as the files give them:

| Kind | Limit | Good for |
| --- | --- | --- |
| 21 `all_foreign_players_in_squad` | at most 14 | every season |
| 368 `foreign_over_24_players_in_squad` | at most 10 | 2026 and 2027, with `1yol` |
| 368 | at most 10 | from 2028 |
| 209 `under_23_national_players_in_squad` | at least 4 | from 2026, with `Tk24` |
| 87 `goalkeepers_in_squad` | at most 3 | every season |
| 230 `domestic_goalkeepers_in_squad` | at least 2 | every season |
| 9 `max_squad_size` | at most 28 | every season |

The 1. Lig allows 8 foreign players (9 in a match squad, 6 on the pitch), the 2. Lig none. The cup's set gives a
limit on the foreign players in a match squad for the teams of each division (14 for the Süper Lig's).

## The lists the game plays by

A competition as it is played is a `sicomps::ACTUAL_COMP`, 0x810 bytes. 987 of them were in memory; the probe takes
550 for played, those whose `db::COMP` points back at them (`+0xB0`) and whose record names a set of fixture rules
their nation has. Its own rules are a record at `+0x38` (the Süper
Lig's has 49 keys: its stages, its fixture dates with the transfer and registration windows among them, its
discipline, its money) and the same record again at `+0x80`. `fxri` in it names the nation's set of fixture rules it
is played under: 0 for the Süper Lig, 1 for the 1. Lig, 5 for the cup.

Its rule group (`+0x370`, 0x240 bytes, with the pointers to its competitions at `+0x00`) points at the nation's rules
as the game plays by them (`+0x1E0`, 0xB0 bytes), which hold the pointers to the sets of fixture rules at `+0x60`, in
the order of the files' `fixture_rules`. A set of fixture rules is 0x138 bytes:

| Offset | What it holds |
| --- | --- |
| `+0x68` | pointer to next season's fixture rules; every one of Türkiye's 10 has one |
| `+0x70` | pointer to its list of squad rules, `(begin, end, capacity, flags)` of pointers; null for a set with none |

A rule in the list is 0x20 bytes:

| Offset | Field |
| --- | --- |
| `+0x00` | `u32` key of its condition (`1yol`, `Tk24`, `tfrd`), 0 for none |
| `+0x04` | `u32` UID of the competition a `tfrd` condition names |
| `+0x18` | `i32` value |
| `+0x1C` | `u16` kind |
| `+0x1E` | `i8`: -1 at most, 1 at least |

A list holds the rules that are good in its season, in the files' order: the Süper Lig's has six, without the rule
from 2028. It can hold one more than the files' list gives: the 2. Lig's ends with a squad size of at most 25, which
its files give with the dates squad numbers are handed out by (`snms`), not as a rule.

Competitions whose records name the same set are played under the same lists: the Nesine 2. Lig and its two groups
name Türkiye's set 2, and eleven under-19 competitions its set 3. A rule changed for one of them is changed for all, as
there is only the one struct; the probe names the others ("Its lists are also those of"), and so does the app.

The game also copies a list for the pages that show it: such copies were in memory with the values the list had when
the page was built.

## Kinds of rule

The game names 465 kinds, in a table of 16-byte entries in the plugin's read-only data (a pointer to the name, then
the number; at `game_plugin.dll+0x4C2D6C0` on this build): 451 different numbers. The probe finds the table by the
entry that points at `all_foreign_players_in_squad` and reads it both ways. Among them:

| Number | Name |
| --- | --- |
| 2, 4, 21 | `all_foreign_players_in_11`, `…_in_match_squad`, `…_in_squad` |
| 9, 165 | `max_squad_size`, `min_squad_size` |
| 87, 230 | `goalkeepers_in_squad`, `domestic_goalkeepers_in_squad` |
| 209, 502 | `under_23_national_players_in_squad`, `under_24_national_players_in_squad` |
| 368 | `foreign_over_24_players_in_squad` |

Most are counts of players. Some are money (`yearly_wage`, a salary cap), which the same lists hold as large values.
Who counts as foreign is not a rule but code (the binary names `is_foreign_for_squad_selection_rule`).

## On the Mac

Read with the probe on Steam build 26620374 for Apple silicon, on a save of 18 July 2027 (Arsenal's, in England). The
game keeps the rules as on Windows, and the records read alike, their traits told by the Itanium names of their
classes: `N6sicore16RAW_TYPED_TRAITSIiEE` for `int` (`IjEE`, `IbEE` and `IxEE` for `unsigned`, `bool` and
`__int64`), `N6sicore28INTERNAL_OBJECT_TYPED_TRAITSINS_6RECORDELh10EEE` for a record,
`…INS_10TYPED_LISTELh11EEE` for a list and `N6sicore19STRING_TYPED_TRAITSE` for text. A few objects lay the rules
out further on:

| Where | Windows | Mac |
| --- | --- | --- |
| `sicomps::ACTUAL_COMP`: its own rules, its `db::COMP`, its rule group | `+0x38`, `+0x120`, `+0x370` | `+0x50`, `+0x138`, `+0x388` |
| A rule group: its nation's rules | `+0x1E0` | `+0x1F0` |
| The container of a nation's men's teams: the record its rule files make together, the list of the files' records | `+0x570`, `+0x588` | `+0x578`, `+0x590` |

A competition as it is played starts with a lock, pthread's (its signature, 'MUTX', at `+0x8`), and its fields after
the lock sit 0x18 further on; why the other two moved was not looked into. The rest is where Windows has it: the
`db::COMP`'s pointer back (`+0xB0`), the nation's sets of fixture rules (`+0x60`), a set's next season (`+0x68`) and
list (`+0x70`), and the rule of 0x20 bytes. The table of kinds has the same form, its entry for
`all_foreign_players_in_squad` at `game_plugin+0x5743A38` (in `__DATA_CONST`).

As on Windows, 550 competitions read as played, 426 with squad rules, 451 kinds named by the game, and 2,498 rules in
the lists of both seasons, 2,072 of them with theirs in the records; the listing takes about two seconds. The Premier
League (England's fixture rules 6) has the same two rules in each season, at least 8
`homegrown_players_0_21_from_nation` and a squad of at most 25. Its squad size, written 24 in both seasons
(`rules 11 --current 2 24 --next 2 24`), read back so from both lists and from the two records the probe writes to,
the files together and the file for the seasons to 2028, while the file from 2029 kept 25. The game's Competition
Stages page showed the 24 under Next Season at once. Saved under another name and loaded, the game made both lists
again with 24, from the two records, which came back as written; then the 25 was put back.

UEFA's set holds the days squads are registered by where Windows has them (`+0x18`): 74 places, 18 in use, six days
each for the Champions, Europa and Conference League, 25 players a day; an entry is laid out as on Windows. The app's
Competitions tab reads on the Mac what the probe reads: `tools/smoke`'s `rules` with `--writes`, run there through a
local test hook, passed its 28 checks (the Premier League's rule on home-grown players set through the command and
through the panel, and the Champions League's squad size held, with its six days, each read back by the probe and
undone).

## What the game makes of a change

The Süper Lig's limit of 14 foreign players was given a different number in each place that holds it, in the running
game, and the game's screens were read afterwards:

- **Competition Stages, Next Season** showed the number written to next season's list, at once.
- **Competition Stages, Current Season** went on showing 14, from a page built before the change, and showed the
  number written to this season's list once a day had passed (Continue).
- **A club's Registration screen** (Beşiktaş, taken over by a second manager) showed "13/22 Max." with a tick for the
  22 then in this season's list; with 5 written there, "13/5" after a day. So the count is held against this season's
  list. The registration window was closed, so no squad was confirmed against a changed limit.
- **A save and a load.** The game was saved under another name with a different number in each place and loaded
  again. Both records came back as written; the lists did not: the game made them again, this season's from the
  record the files make together, next season's from the rule file's own record.

So a change lasts when it is written to the records, and takes effect in the running game when it is written to the
lists. For this season that is the list and the record together; for next season the other list and the rule file's
record.

Each Süper Lig club also holds a rule of its own for the limit, with the condition `dtcm`
(`include_player_domestic_to_competition_nation`): 18 structs, saved with the save. The Registration screen does not
count against them, and what they are for is not known; the editor leaves them alone.

## A turn of the season

The same save was played on from 20 October 2026 to 4 July 2027, with rules of the Süper Lig changed in both seasons
and rules of the Premier League changed in next season's only, each to numbers of its own, through the app. A listing
of every competition's rules was taken before the changes, before the turn, after it and after a save and a load
(`rules " "`; nothing else wrote to the game meanwhile, and no save was loaded before the last one).

- **Through the season** nothing changed them: eight months on, the January window included, the lists and the
  records held the numbers written.
- **A nation's seasons turn on its `season_update_day`**: Türkiye's on 20 June, England's on 25 June, Brazil's on 2
  January, Korea's on 31 December. The rules count a season by the year it begins in, and one that turns in December
  is the next year's: Korea's turn of 31 December 2026 begins its 2027 season.
- **At the turn next season's list becomes this season's.** So for each of the 138 competitions that turned in those
  weeks; the Süper Lig's and the Premier League's then held the numbers written to next season's list. The numbers
  written to this season's list were gone, and the record the files make together held the new season's.
- **Next season's list is made from the rule file that is for that season.** A nation has one rule file (`file_type`
  12) for every season, or more than one: England has two, one with `OvEY` 2027 (its last season) and one with `OvSY`
  2028 (its first); neither key has a name in the game. The record the files make together carries the same key as
  the file it was made from. After the turn the Süper Lig's list for 2028 held the numbers written to next season's
  list again, from Türkiye's one file, but for its rule on foreign players over 24, which that file replaces by another
  from 2028; the Premier League's list for 2028 held the game's own numbers, from England's file for 2028, which nothing
  had written to. A save and a load left both as they were.
- Where a nation's rules are the same in the new season, the list for the season after is not made anew at the turn:
  the old list of this season is put in its place (Romania's SuperLiga had its rule of 2026 there four days after its
  turn), and it is made from the rule file when a save is loaded.

So a change of this season's rule lasts the season, and a change of next season's rule lasts from the next season
on, for as long as the same rule file and the same rule are in force.

## Competitions of no nation

A continent's club competition has no nation: its `db::COMP` points at a `db::CONTINENT` instead (`+0x58`; the
nation's pointer at `+0x60` is null). The continent keeps records of the same form as a nation's container: the list
of its men's club competitions' rule files at `+0x68` (`file_type` 5) and the record they make together at `+0x98`,
and the same for its women's club competitions at `+0x70` and `+0xB0`. The lists of the CAF, AFC and CONMEBOL
Champions Leagues and of UEFA's Women's Champions League read as those records do. The editor does not read them yet.

UEFA's men's club competitions are the exception: UEFA's `+0x68` is null and its `+0x98` record is empty, and none of
the 1,417 lists of squad rules in the records in memory holds the Champions League's four rules together. The game
makes them itself. Their rule group has six sets; the Champions, Europa and Conference League are played under one,
with an empty list for next season.

That set also holds what a nation's league has none of: a table of pointers at `+0x18` (74 places, 18 in use), each
to an entry of 0x40 bytes for a day squads are registered by, six for each of the three competitions. An entry holds
its competition's UID (`+0x14`), the key `SqSl` (`+0x18`) and at `+0x26` the most players a squad may register by that
day. The competition's page reads its line "Maximum of 25 players to be registered on Friday 2nd July 2027" from
there, not from the list's rule of the squad's size, and its lines on goalkeepers and on players trained at the club
or in the nation from the list.

Tried on the same save: the list's rule on players trained in the nation written from 8 to 6 and the Champions
League's six entries from 25 to 30 showed on its page after a day ("Minimum of 6 players...", "Maximum of 30
players..."); a save and a load put 8, 25 and 25 back in every place. So a change of these rules lasts while the game
runs, and no longer.

The editor therefore writes such a rule to the list alone (and, for the squad's size, to the entries of the set that
repeat it), and the app holds it: it remembers the count for the career, as it remembers a renamed stadium, and
writes it again when it connects or reads the data again and finds the game's own count there. Until the continents'
records are read, the other continents' competitions are held the same way.

Not known yet:
- whether a squad is counted against the list's rule or the entries' number when it is registered for a UEFA
  competition (no club of the save was in one);
- whether the game lets a squad be confirmed against a changed limit while the window is open;
- whether an update of the game puts its own rules over a save's records;
- what the clubs' own structs are for;
- why one of two rules of the same years is in a season's list and the other in the next's (Romania's on players
  under 21 and under 22);
- how any of this is laid out on Steam for Windows.

## The probe's `rules`

`fmlibre-probe rules` lists the competitions being played that have squad rules; with words of a name or a UID, the
rules of each season, numbered, with what the nation's records hold for each. `--current N VALUE` and `--next N VALUE`
change rule N of a season: in the list, and in the record that list is made from, so that the change is in the next
save. `--restore` puts the values back, after `--hold` seconds if given.

A rule is changed only where it is a count (0 to 255) and the record holds it for the season: the rule of the same
kind, limit and condition that is good in the season's year, looked for in the files' order. This season's record is
the one the files make together, next season's the rule file that is for that season. A season begins in the year the
nation's `season_update_day` last passed, or in the year after where that day is in October or later. Before
writing, the list must still point at the rule and the rule be of the kind it was.

`--where` adds where the lists and rules are in the game's memory, and `--sources` what each of the nation's records
holds for the competition's set of fixture rules: the record the files make together and each file's own, with the
seasons a file is for. A rule of a competition no record keeps is changed in its list alone; `--list-only` does the
same for any rule, to see what the game makes of it.

```text
Trendyol Süper Lig (UID 130286) · Türkiye · level 0 · the nation's fixture rules 0
  This season (2026)
     1  at most    14  all_foreign_players_in_squad  · saved
     2  at most    10  foreign_over_24_players_in_squad  [1yol]  · saved, 2026 to 2027
     3  at least    4  under_23_national_players_in_squad  [Tk24]  · saved, from 2026
     4  at most     3  goalkeepers_in_squad  · saved
     5  at least    2  domestic_goalkeepers_in_squad  · saved
     6  at most    28  max_squad_size  · saved
```

The records are those of the nation the competition's `db::COMP` names: its men's container's, or its women's for a
competition those list (`cmps`) and the men's do not.

Checked on the save above: 550 competitions read in about five seconds, 426 with squad rules; the Süper Lig's
goalkeepers changed in both seasons and put back with `--restore`; its foreign players set in both seasons and read
back from the lists and from both records; and the refusals: a rule the list does not have, a value over 255, the
2. Lig's squad size (not in the records as a rule), and a name that many competitions match.

Of those 426, every rule has its rule in the records at 345, some at 45 and none at 36. Of the 36, 23 belong to no
nation (the continents' competitions, whose records were not looked for); the others are women's leagues (the
Women's Super League, the NWSL), Major League Soccer and a few cups; why theirs do not match was not looked into. Of
the 2,498 rules in the lists of both seasons, 2,070 have theirs in the records. A quarter of the 428 without are squad
sizes (105), which the files give with the squad numbers; the others are of many kinds.

## The app's Competitions tab

The app reads the same lists (`competition_rules`, the first time its Competitions tab is shown: one pass over the
game's memory, a few seconds) and keeps them, with its own changes, until the data is read again. It lists every
competition being played, and beside the list the rules of the one picked, in an area for this season and one for the
next. About a quarter have no squad rule in either season: Scotland's, Austria's and Slovakia's top divisions, the
Republic of Ireland's, most cups (124 of 550 on the save above; Slovakia's records hold no squad rule either, read
again on Game Pass 0.9.2562.0). The tab listed only those with rules at first, so that a search for Slovakia's First
Division found nothing and seemed not to know it (#274); now it lists them too, and the panel of one says the game
gives it no squad rules.
A rule's count is changed there (`set_competition_rules`) under the conditions the
probe changes it under; each rule changed is logged as a change of the competition, with the rule it set, and Undo in
Changes writes back what it found, in the list and in the record. A change logged before the app last connected is
kept as history: its undo needed the game's memory as it was.

A rule of a competition whose rules no record keeps (see "Competitions of no nation") is written to the list alone,
logged the same way and held: the app's file of what is frozen in each save keeps the rule, the count it was given
and the game's own, and `competition_rules` lists what is held, which the panel tags. Undo writes the game's count
back and lets go of the rule; once the game has made the list anew and the app has written its count there again,
the undo is made as a new change back.

Only on the builds whose profile says `squad_rules = true`, as the prize money and the sponsorship deals have their
flags: Game Pass 0.9.2524.0 and the Apple silicon halves of Steam builds 26620374 and 26797085 so far (the second read
as the first on the same save). The others say why the tab is empty.

The game names 451 kinds of rule by keys such as `all_foreign_players_in_match_squad`; the lists read on the save
above use 174 of them. The app words the commonest from the key (whom a rule counts, and where: the squad, the match
squad, the eleven, the starting eleven, the bench), by pattern where a key carries ages, and shows the key itself,
spaced out, for the rest; a rule's hover text gives the key. A rule under a condition other than a division's teams
shows the condition's key (`dtcm`, `btms`, ...): what those conditions are was not looked into.
