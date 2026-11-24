// What a squad rule counts, worked out from the game's own key for its kind ("all_foreign_players_in_match_squad"):
// whom it counts ("Foreign players") and where ("Match squad"). The game names some 450 kinds; the editor words the
// commonest, by pattern where a kind carries ages, and shows the game's key, spaced out, for the rest.

import { strings } from "./i18n";
import type { RuleSeason, SquadRule } from "./types";

const text = strings.competitions;

/** The most the editor writes as a rule's count, and the most a count may be for it to change it (rules.rs). */
export const MOST_RULE_VALUE = 255;

/** Where a rule counts players. */
export type RuleScope = keyof typeof text.scopes;

/** What a kind of rule counts. */
export interface RuleName {
  /** Whom it counts: "Foreign players"; the game's key spaced out where the editor has no words for it. */
  who: string;
  /** Where it counts them; null for a kind that says none (the squad's size). */
  scope: RuleScope | null;
}

/** The endings of the game's keys that say where a rule counts, the longer ones first. */
const SCOPES: readonly (readonly [string, RuleScope])[] = [
  ["_in_match_squad_maximum", "matchSquad"],
  ["_in_match_squad", "matchSquad"],
  ["_in_starting_11", "startingEleven"],
  ["_in_playing_11", "eleven"],
  ["_in_11", "eleven"],
  ["_in_squad", "squad"],
  ["_on_bench", "bench"],
];

type Who = typeof text.who;
type FixedWho = { [K in keyof Who]: Who[K] extends string ? K : never }[keyof Who];

/** The kinds the editor has words for as they are, by the game's key without its ending. */
const FIXED: Readonly<Record<string, FixedWho>> = {
  max_squad_size: "squadSize",
  min_squad_size: "squadSize",
  all_foreign_players: "foreign",
  all_foreign_players_except_asian: "foreignExceptAsian",
  all_foreign_gks: "foreignGoalkeepers",
  non_eec_players: "nonEu",
  same_nation_players: "domestic",
  domestic_goalkeepers: "domesticGoalkeepers",
  goalkeepers: "goalkeepers",
  keepers: "goalkeepers",
  outfield_players: "outfield",
  non_homegrown_players: "nonHomegrown",
  players_on_youth_contract: "youthContract",
  players_on_ft_contract: "fullTime",
  amateur_players: "amateur",
  youth_or_amateur_players: "youthOrAmateur",
  players_on_trial: "trialists",
  loan_players: "loans",
  international_players: "internationals",
  players_registered_for_different_team: "otherTeam",
  players_registered_for_first_team: "firstTeam",
};

const FROM = { club: "club", nation: "nation", club_or_nation: "clubOrNation" } as const;

/** The kinds that carry ages, by the pattern of the game's key without its ending. */
const PATTERNS: readonly (readonly [RegExp, (found: RegExpExecArray) => string])[] = [
  [/^over_(\d+)_players$/, (m) => text.who.over(Number(m[1]))],
  [/^under_(\d+)_players$/, (m) => text.who.under(Number(m[1]))],
  [/^under_(\d+)_national_players$/, (m) => text.who.domesticUnder(Number(m[1]))],
  [/^foreign_over_(\d+)_players$/, (m) => text.who.foreignOver(Number(m[1]))],
  [/^over_(\d+)_(?:keepers|goalkeepers)$/, (m) => text.who.goalkeepersOver(Number(m[1]))],
  [/^over_(\d+)_outfield_players$/, (m) => text.who.outfieldOver(Number(m[1]))],
  [
    /^homegrown_players_(\d+)_(\d+)_from_(club|nation|club_or_nation)$/,
    (m) => text.who.homegrown(FROM[m[3] as keyof typeof FROM], Number(m[1]), Number(m[2])),
  ],
  [
    /^u(\d+)_homegrown_players_(\d+)_(\d+)_from_nation$/,
    (m) => text.who.homegrownUnder(Number(m[1]), Number(m[2]), Number(m[3])),
  ],
];

/** The game's key with spaces for its underscores and a capital: what is shown for a kind the editor has no words for. */
function spacedOut(key: string): string {
  const words = key.replace(/_/g, " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** What the kind of rule the game calls `name` counts, and where; `kind` is its number, for one the game does not name. */
export function ruleName(name: string | null, kind: number): RuleName {
  if (name === null) {
    return { who: text.unnamedKind(kind), scope: null };
  }
  const ending = SCOPES.find(([suffix]) => name.endsWith(suffix));
  const base = ending ? name.slice(0, -ending[0].length) : name;
  const scope = ending ? ending[1] : null;
  const fixed = Object.prototype.hasOwnProperty.call(FIXED, base) ? FIXED[base] : undefined;
  if (fixed !== undefined) {
    return { who: text.who[fixed], scope };
  }
  for (const [pattern, words] of PATTERNS) {
    const found = pattern.exec(base);
    if (found) {
      return { who: words(found), scope };
    }
  }
  // Left whole: its ending may not mean what the endings above do.
  return { who: spacedOut(name), scope: null };
}

/** A rule's name on one line: "Foreign players · Match squad". */
export function ruleTitle(name: string | null, kind: number): string {
  const { who, scope } = ruleName(name, kind);
  return scope === null ? who : `${who} · ${text.scopes[scope]}`;
}

/** "At most" or "At least", as a rule's value limits. */
export const limitWord = (compare: number) => (compare < 0 ? text.atMost : text.atLeast);

/**
 * Whether the editor changes the rule: a count of players, with its rule in the nation's rules a save keeps, or of a
 * competition whose rules no record keeps (the editor then holds the change).
 */
export const ruleEditable = (rule: SquadRule) =>
  (rule.source !== null || rule.unsaved) && rule.value >= 0 && rule.value <= MOST_RULE_VALUE;

/**
 * The name of the field a change of a squad rule is logged under, `rule.current.all_foreign_players_in_squad` or
 * `rule.next.kind21`: the season, and the rule.
 */
export function ruleFieldLabel(key: string): string {
  const [, season, kind = ""] = key.split(".");
  const numbered = /^kind(\d+)$/.exec(kind);
  const title = numbered ? ruleTitle(null, Number(numbered[1])) : ruleTitle(kind, 0);
  const seasonText = text.seasons[(season === "next" ? "next" : "current") satisfies RuleSeason];
  return text.ruleField(seasonText, title);
}
