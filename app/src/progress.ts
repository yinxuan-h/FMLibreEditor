// How a player developed, worked out from the records the app keeps of a career (see `player_progress`): which of
// the dates he is compared with, and what changed since. The header's note, the arrows beside the attributes and the
// Development tab all read the same comparison.

import type { FmDate, PlayerProgress } from "./types";

/** The period a comparison looks back over. */
export type ProgressRange = "quarter" | "year" | "all";

/** How many days of the game's time each period is; all of the records for "all". */
const RANGE_DAYS: Record<ProgressRange, number> = { quarter: 92, year: 365, all: Infinity };

/** The period the header's note and the attributes' arrows look back over: a year, as the card of My club does. */
export const DEFAULT_RANGE: ProgressRange = "year";

/** The fields of a record besides the attributes. */
export const CURRENT_ABILITY = "current_ability";
export const POTENTIAL_ABILITY = "potential_ability";
export const VALUE = "value";

const NOT_ATTRIBUTES = new Set([CURRENT_ABILITY, POTENTIAL_ABILITY, VALUE]);

/** A date as days since 1970, to tell how far apart two are. */
export const dayOf = (date: FmDate) => Math.round(Date.UTC(date.year, date.month - 1, date.day) / 86_400_000);

/** A value then and now. */
export interface Change {
  from: number;
  to: number;
  /** The date `from` is of. */
  since: FmDate;
}

/**
 * Which of the dates of `progress` he is compared with over `range`: the oldest within the period before the last
 * date, which is what he is now. None while there is nothing before now.
 */
export function referenceOf(progress: PlayerProgress, range: ProgressRange): number | null {
  const last = progress.dates.length - 1;
  if (last < 1) {
    return null;
  }
  const oldest = dayOf(progress.dates[last]) - RANGE_DAYS[range];
  const within = progress.dates.findIndex((date) => dayOf(date) >= oldest);
  // All the records are older than the period: the last of them.
  return Math.min(within === -1 ? last - 1 : within, last - 1);
}

/**
 * How the field `field` changed from the date at `reference` to now. A value the records do not have that far back
 * is taken from the first record that has one. None when it has not changed.
 */
export function changeOf(progress: PlayerProgress, field: string, reference: number | null): Change | null {
  const values = progress.series[field];
  if (!values || reference === null) {
    return null;
  }
  const last = values.length - 1;
  const to = values[last];
  let at = reference;
  while (at < last && values[at] === null) {
    at += 1;
  }
  const from = values[at];
  if (from === null || to === null || from === to || at === last) {
    return null;
  }
  return { from, to, since: progress.dates[at] };
}

/** The attributes that changed from the date at `reference` to now, by key. */
export function attributeChanges(progress: PlayerProgress, reference: number | null): Map<string, Change> {
  const changes = new Map<string, Change>();
  for (const field of Object.keys(progress.series)) {
    const change = NOT_ATTRIBUTES.has(field) ? null : changeOf(progress, field, reference);
    if (change) {
      changes.set(field, change);
    }
  }
  return changes;
}

/** How many days of the game's time the dates of `progress` span. */
export function spanDays(progress: PlayerProgress): number {
  const dates = progress.dates;
  return dates.length < 2 ? 0 : dayOf(dates[dates.length - 1]) - dayOf(dates[0]);
}

/** The periods worth offering for `progress`: those that do not reach back past all of it, and "all". */
export function rangesOf(progress: PlayerProgress): ProgressRange[] {
  const span = spanDays(progress);
  return (["quarter", "year"] as const).filter((range) => RANGE_DAYS[range] < span).concat(["all"] as never);
}

/** "+3" or "−2": a change as a signed number, with a real minus sign. */
export const signed = (delta: number) => (delta > 0 ? `+${delta}` : `−${Math.abs(delta)}`);
