# FM27: the squad planner in the save file

How a save file keeps the squad planner, the depth columns per position on the squad screen (#134). javiercostoya
found it in their own career saves on the Mac Steam build (27.0.0+2486695 and 27.0.0+2493426, single player). They
paused the game and compared saves that differed only in the planner, and matched the planner's screens (the
position views and the position picker) against the bytes.

These notes are about the **save file**. The editor works on the game's memory, and the planner has not been found
there yet. The editor does not read or change the planner. The layout below is likely a serialization of the
planner as the game holds it, so the season runs and the 27-slot grid are where a search in memory would start.

## Runs and slots

Each human manager has four planner runs in the save's `humans` section. A run is:

- a header `03 1b`;
- 27 slot records back to back, each `[count u8][item_id u32 × count]`;
- a terminator `1b`.

The items of a slot are in depth order. `item_id` is the person's compact id plus one (below). An empty slot is a
single `00` byte, a record with a count of 0, not padding: emptying a slot in the game kept the run at 27 records and
wrote `00` at the emptied slot.

## Runs are seasons

| Run | What it holds |
| --- | --- |
| 1 | the current season, as the manager set it: every change to the current season landed here |
| 2 | the next season: one change there changed exactly one slot of run 2 |
| 3 | the season after: clearing that view emptied seven slots of run 3 |
| 4 | all empty in every save; meaning unknown |

The game fills the future seasons with depth lists of its own (loaned-in and youth players included), so a run the
manager never touched is not empty.

## The 27 slots

The game's position grid, from the keeper up, right to left within each zone:

| Slots | Positions |
| --- | --- |
| 1 | GK |
| 3, 4–6, 7 | DR, DC ×3, DL |
| 8–12 | DM; 9–11 the central three, the flanks hold backup full-backs |
| 13–17 | MC; 14–16 the central three |
| 18–22 | AMR, AMC ×3, AML |
| 24–26 | ST; 25 is the planner's drag target |
| 2, 23, 27 | always empty |

The planner's position dropdown lists the positions of the current tactic from the bottom up, and the depth cards of
each position view match one slot's record, in the same order.

## Person ids

In the save's `game_db`, a person record starts `[compact u32][uid u32][uid u32]`, the two uids equal; an item is
the compact id plus one. Go from the item ids only. Looking up the uid pair and stepping back 4 bytes also finds
copies of the pair that are not records, and the same compact id can start both a staff and a player record, so
take the one with a player uid.

## Not known yet

- Where the planner lives in memory, and whether it keeps the same four runs and 27 slots there.
- What run 4 is for.
- Windows: no save from a Windows build was looked at.
