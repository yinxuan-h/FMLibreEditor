// The Competitions tab: the competitions that have squad rules, and beside the list the rules of the one picked, this
// season's and next season's in an area each: how many foreign players a squad may register, how many goalkeepers, how
// many players in all, and what a team may pick for a match (docs/recon/fm27-league-rules.md). While the game can be
// written to, the counts can be changed, season by season. The editor finds the rules in one pass over the game's
// memory, which takes a few seconds, the first time the tab is shown; after that it keeps them, with its own changes.
// Competitions played under the same lists (a division and its groups) share their rules, and say so.

import { useContext, useEffect, useId, useMemo, useRef, useState, type CSSProperties } from "react";
import { competitionRules, setCompetitionRules } from "./editing";
import { FilterBar } from "./FilterBar";
import { describeError, formatNumber } from "./format";
import { intlLocale } from "./i18n";
import { InfoTip } from "./InfoTip";
import { competitionsText as text, profileText } from "./labels";
import { NationFlag, PICTURE_SIZE, WithPicture } from "./Picture";
import { ProfileFrame } from "./ProfileParts";
import { isRecord, restored, useSnapshot } from "./restore";
import { limitWord, MOST_RULE_VALUE, ruleEditable, ruleName } from "./squadRules";
import type { CompetitionRules, CompetitionRulesRead, HeldRule, RuleInput, RuleSeason, SquadRule } from "./types";
import { EditorContext } from "./useProfileEditing";

/** The nation filter's value for the competitions of no nation: a continent's. */
const INTERNATIONAL = "international";

/** What the tab held before the language was switched: its search text, its nation and the competition picked. */
function readTab(value: unknown): { query: string; nation: string; selected: number | null } {
  const given = isRecord(value) ? value : {};
  return {
    query: typeof given.query === "string" ? given.query : "",
    nation: typeof given.nation === "string" ? given.nation : "",
    selected: typeof given.selected === "number" && Number.isInteger(given.selected) ? given.selected : null,
  };
}

/** `value` without capitals and accents, as the search compares names. */
const folded = (value: string) =>
  value
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/ı/g, "i")
    .toLowerCase();

type Read = { view: CompetitionRulesRead } | { error: string };

interface CompetitionBrowserProps {
  /** Changes when the game data is read again, or changed: the rules are asked for again. */
  version: number;
  active: boolean;
}

/** The competitions with squad rules, and the rules of the one picked beside them; or why there are none to show. */
export function CompetitionBrowser({ version, active }: CompetitionBrowserProps) {
  const [start] = useState(() => restored("competitions", readTab));
  const [read, setRead] = useState<Read | null>(null);
  const [reading, setReading] = useState(false);
  const [query, setQuery] = useState(start.query);
  const [nation, setNation] = useState(start.nation);
  const [selected, setSelected] = useState<number | null>(start.selected);
  useSnapshot("competitions", () => ({ query, nation, selected }));
  const searchInput = useRef<HTMLInputElement>(null);
  const selectedRow = useRef<HTMLTableRowElement>(null);

  const readNow = (again: boolean, alive: () => boolean = () => true) => {
    setReading(true);
    competitionRules(again).then(
      (view) => alive() && (setRead({ view }), setReading(false)),
      (e) => alive() && (setRead({ error: describeError(e) }), setReading(false)),
    );
  };

  // Read when the tab is first shown, not before: the first read is a pass over all of the game's memory. After that,
  // whenever the tab is shown or the game data changes while it is: the editor keeps what it read, so this is quick.
  useEffect(() => {
    if (!active) {
      return;
    }
    let alive = true;
    readNow(false, () => alive);
    return () => {
      alive = false;
    };
    // `readNow` is asked only on these changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, version]);

  useEffect(() => {
    if (active) {
      searchInput.current?.focus();
    }
  }, [active]);

  const view = read && "view" in read ? read.view : null;
  const all = useMemo(() => view?.competitions ?? [], [view]);
  const names = useMemo(() => new Map(all.map((competition) => [competition.uid, competition.name])), [all]);

  // The nations that have a competition here, by name; the continents' competitions come under one entry.
  const nations = useMemo(() => {
    const found = new Set(all.map((competition) => competition.nation).filter((name): name is string => name !== null));
    return [...found].sort((a, b) => a.localeCompare(b, intlLocale));
  }, [all]);

  // A nation's competitions together, its divisions from the top, the continents' competitions after the nations'.
  const rows = useMemo(() => {
    const words = folded(query).split(/\s+/).filter(Boolean);
    const matching = all.filter((competition) => {
      if (nation !== "" && (competition.nation ?? INTERNATIONAL) !== nation) {
        return false;
      }
      // A UID is matched whole, as in the other lists: "12" is one competition's, and part of many others'.
      if (String(competition.uid) === query.trim()) {
        return true;
      }
      const hay = folded(`${competition.name} ${competition.shortName ?? ""} ${competition.nation ?? text.international}`);
      return words.every((word) => hay.includes(word));
    });
    return matching.sort(
      (a, b) =>
        Number(a.nation === null) - Number(b.nation === null) ||
        (a.nation ?? "").localeCompare(b.nation ?? "", intlLocale) ||
        (a.level ?? Infinity) - (b.level ?? Infinity) ||
        a.name.localeCompare(b.name, intlLocale),
    );
  }, [all, query, nation]);

  if (view?.unavailable) {
    return (
      <section className="national-unavailable" hidden={!active} aria-live="polite">
        <h2>{text.unavailableTitle}</h2>
        <p>{view.unavailable === "unreadable" ? text.unavailableUnreadable : text.unavailableBuild}</p>
      </section>
    );
  }

  const picked = selected === null ? undefined : all.find((competition) => competition.uid === selected);
  const showPanel = active && picked !== undefined;
  const className = ["workspace", showPanel && "with-profile", !active && "inactive"].filter(Boolean).join(" ");
  const setRules = (competitions: CompetitionRules[], held: HeldRule[]) =>
    setRead({ view: { unavailable: null, competitions, held } });

  const closePanel = () => {
    // The panel unmounts with the focused control inside it; keep keyboard focus in the table.
    if (document.activeElement?.closest(".profile")) {
      selectedRow.current?.focus();
    }
    setSelected(null);
  };

  return (
    <div className={className}>
      <section className="players" aria-busy={reading}>
        <div className="toolbar">
          <label className="search">
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <circle cx="8.5" cy="8.5" r="5.5" />
              <path d="m13 13 4.5 4.5" />
            </svg>
            <input
              ref={searchInput}
              value={query}
              onChange={(e) => setQuery(e.currentTarget.value)}
              placeholder={text.search}
              spellCheck={false}
            />
          </label>
          <span className="count">{view ? text.summary(all.length, rows.length) : ""}</span>
          {view && (
            <button type="button" disabled={reading} title={text.readAgainTitle} onClick={() => readNow(true)}>
              {text.readAgain}
            </button>
          )}
        </div>
        <FilterBar active={nation === "" ? 0 : 1} onClear={() => setNation("")}>
          <label className="filter-field">
            <span>{text.filters.nation}</span>
            <select value={nation} onChange={(e) => setNation(e.currentTarget.value)}>
              <option value="">{text.filters.allNations}</option>
              {nations.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
              <option value={INTERNATIONAL}>{text.international}</option>
            </select>
          </label>
        </FilterBar>
        {read && "error" in read && (
          <div className="banner" role="alert">
            {read.error}
          </div>
        )}

        <div className="table-wrap">
          <table className="grid club-grid competition-grid">
            <thead>
              <tr>
                <th className="col-name">{text.columns.name}</th>
                <th className="col-league">{text.columns.nation}</th>
                <th className="num col-founded">{text.columns.level}</th>
                <th className="num col-rep" title={text.rulesCountTitle}>
                  {text.columns.rules}
                </th>
                <th className="num col-uid">{text.columns.uid}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((competition) => {
                const isSelected = competition.uid === selected;
                return (
                  <tr
                    key={competition.uid}
                    ref={isSelected ? selectedRow : undefined}
                    className={isSelected ? "selected" : undefined}
                    aria-current={isSelected ? "true" : undefined}
                    tabIndex={0}
                    onClick={() => setSelected(competition.uid)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        setSelected(competition.uid);
                      }
                    }}
                  >
                    <td className="col-name" title={competition.name}>
                      <WithPicture picture={<NationFlag uid={competition.nationUid} />}>
                        <div className="name">
                          <span className="name-text">{competition.name}</span>
                        </div>
                      </WithPicture>
                    </td>
                    <td className="col-league">
                      {competition.nation ?? <span className="muted">{text.international}</span>}
                    </td>
                    <td className="num col-founded">
                      {competition.level === null ? profileText.none : text.level(competition.level + 1)}
                    </td>
                    <td className="num col-rep">
                      {text.rulesCount(competition.current.length, competition.next?.length ?? null)}
                    </td>
                    <td className="num uid col-uid">{competition.uid}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {view === null && read === null && <p className="empty">{text.reading}</p>}
          {view !== null && rows.length === 0 && <p className="empty">{text.empty(query, nation !== "")}</p>}
          {view !== null && rows.length > 0 && <p className="note competition-list-note">{text.listNote}</p>}
        </div>
      </section>

      {showPanel && (
        <CompetitionPanel
          key={picked.uid}
          competition={picked}
          names={names}
          held={view?.held ?? []}
          onSaved={setRules}
          onFailed={() => readNow(false)}
          onClose={closePanel}
        />
      )}
    </div>
  );
}

interface CompetitionPanelProps {
  competition: CompetitionRules;
  /** The names of the competitions read, by UID: for the ones that share these rules, and for a rule's division. */
  names: ReadonlyMap<number, string>;
  /** The rules the editor holds in this save: of competitions whose rules the game keeps in no record. */
  held: readonly HeldRule[];
  /** The rules of every competition after a change was saved, and the rules held then. */
  onSaved: (competitions: CompetitionRules[], held: HeldRule[]) => void;
  /** A change failed, after others of it may have been written: the rules are asked for again. */
  onFailed: () => void;
  onClose: () => void;
}

/** The count typed, as a whole number the editor writes; null for what is none. */
function typedCount(typed: string): number | null {
  const digits = typed.trim();
  if (!/^\d{1,3}$/.test(digits)) {
    return null;
  }
  const count = Number(digits);
  return count <= MOST_RULE_VALUE ? count : null;
}

const SEASONS: readonly RuleSeason[] = ["current", "next"];

/** A day of the year in the window's language, without a year: "20 June". */
function dayOfYear(month: number, day: number): string {
  // In a year that is no leap year: no nation's seasons turn on 29 February.
  return new Intl.DateTimeFormat(intlLocale, { day: "numeric", month: "long" }).format(new Date(2001, month - 1, day));
}

/**
 * The squad rules of one competition: an area for this season and one for the next, each with its rules and their
 * counts. With Change rules, while the game can be written to, the counts become inputs; Save writes the ones changed.
 */
function CompetitionPanel({ competition, names, held, onSaved, onFailed, onClose }: CompetitionPanelProps) {
  const { editable, readOnlyNote, onEdited } = useContext(EditorContext);
  // What was typed, by season and place ("next:0"); null while the counts are only shown.
  const [draft, setDraft] = useState<Record<string, string> | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const noteId = useId();

  const lists: Record<RuleSeason, SquadRule[] | null> = { current: competition.current, next: competition.next };
  // The counts the inputs hold, by season and place: the game's where nothing was typed, null where what was typed is
  // no count.
  const after = (season: RuleSeason, at: number, rule: SquadRule) => {
    const typed = draft?.[`${season}:${at}`];
    return typed === undefined ? rule.value : typedCount(typed);
  };
  const inputs: RuleInput[] = [];
  let invalid = false;
  for (const season of SEASONS) {
    (lists[season] ?? []).forEach((rule, at) => {
      const count = after(season, at, rule);
      if (count === null) {
        invalid = true;
      } else if (count !== rule.value) {
        inputs.push({ season, rule: at, kind: rule.kind, from: rule.value, value: count });
      }
    });
  }
  const anyEditable = SEASONS.some((season) => (lists[season] ?? []).some(ruleEditable));

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const done = await setCompetitionRules(competition.uid, inputs);
      setDraft(null);
      onSaved(done.competitions, done.held);
      onEdited();
    } catch (e) {
      setError(describeError(e));
      onFailed();
    } finally {
      setSaving(false);
    }
  };

  const shared = competition.sharedWith.map((uid) => names.get(uid)).filter((name): name is string => name !== undefined);
  // The day the nation's seasons turn on, as the notes under the areas say it: "20 June".
  const turn = competition.seasonTurn === null ? null : dayOfYear(competition.seasonTurn.month, competition.seasonTurn.day);
  // The rules the editor holds are kept under the lowest UID of the competitions played under the same lists.
  const holder = Math.min(competition.uid, ...competition.sharedWith);
  const heldOf = (season: RuleSeason, rule: SquadRule) =>
    held.find(
      (h) =>
        h.competition === holder &&
        h.season === season &&
        h.kind === rule.kind &&
        h.compare === rule.compare &&
        h.condition === rule.condition &&
        h.conditionCompetition === rule.conditionCompetition,
    );
  // How a change lasts: in the save, or, where the game keeps the rules in none, for as long as the editor holds it.
  const lasts = competition.unsaved ? text.unsavedNote : text.lasts;
  const level = competition.level === null ? null : `${text.columns.level} ${text.level(competition.level + 1)}`;

  return (
    <ProfileFrame
      picture={<NationFlag uid={competition.nationUid} size={PICTURE_SIZE.clubLogo} label={competition.nation ?? ""} />}
      name={competition.name}
      fullName={competition.shortName}
      vitals={[competition.nation ?? text.international, level, `${profileText.uid} ${competition.uid}`]}
      wide
      loaded={{ details: competition }}
      onClose={onClose}
    >
      <section className="profile-group squad-rules">
        <h3>{text.title}</h3>
        <p className="note">{text.intro}</p>
        {shared.length > 0 && <p className="note rule-notice">{text.shared(shared.join(", "))}</p>}
        {!anyEditable && <p className="note rule-notice">{text.noneChangeable}</p>}
        <div className="rule-seasons">
          {SEASONS.map((season) => (
            <SeasonRules
              key={season}
              season={season}
              year={competition.season === null ? null : competition.season + (season === "next" ? 1 : 0)}
              rules={lists[season]}
              names={names}
              turn={turn}
              heldOf={(rule) => heldOf(season, rule)}
              tagFixed={anyEditable}
              typed={draft === null ? null : (at) => draft[`${season}:${at}`]}
              after={(at, rule) => after(season, at, rule)}
              disabled={saving}
              noteId={noteId}
              onType={(at, value) => setDraft((d) => ({ ...d, [`${season}:${at}`]: value }))}
              onEscape={() => setDraft(null)}
            />
          ))}
        </div>
        {draft === null ? (
          <div className="prize-actions">
            <button
              type="button"
              className="heal-toggle"
              disabled={!editable || !anyEditable}
              title={!anyEditable ? text.noneChangeable : editable ? text.editTitle : readOnlyNote}
              onClick={() => {
                setError(null);
                setDraft({});
              }}
            >
              {text.edit}
            </button>
            <InfoTip label={text.infoLabel} note={lasts} />
          </div>
        ) : (
          <div className="prize-edit">
            <p id={noteId} className="move-note">
              {lasts}
            </p>
            {invalid && <p className="move-note invalid">{text.invalid(MOST_RULE_VALUE)}</p>}
            <div className="prize-actions">
              <button
                type="button"
                className="primary"
                disabled={saving || invalid || inputs.length === 0}
                onClick={() => void save()}
              >
                {saving ? text.saving : text.save}
              </button>
              <button type="button" disabled={saving} onClick={() => setDraft(null)}>
                {text.cancel}
              </button>
            </div>
          </div>
        )}
        {error !== null && (
          <p className="move-note invalid" role="alert">
            {error}
          </p>
        )}
      </section>
    </ProfileFrame>
  );
}

interface SeasonRulesProps {
  season: RuleSeason;
  /** The year the season begins in; null where the in-game date is not known. */
  year: number | null;
  /** Null where the game holds no list for the season. */
  rules: SquadRule[] | null;
  names: ReadonlyMap<number, string>;
  /** The day the nation's seasons turn on, worded ("20 June"); null where its rules give none. */
  turn: string | null;
  /** The count the editor holds for a rule of this list, where it holds one. */
  heldOf: (rule: SquadRule) => HeldRule | undefined;
  /**
   * Whether a rule the editor leaves alone is tagged so: where it changes others of the competition's. Where it
   * changes none, the note over the areas says so for all of them.
   */
  tagFixed: boolean;
  /** What was typed for the rule at a place; null while the counts are only shown. */
  typed: ((at: number) => string | undefined) | null;
  /** The count the rule at a place would have: null where what was typed is none. */
  after: (at: number, rule: SquadRule) => number | null;
  disabled: boolean;
  /** The note the inputs are described by. */
  noteId: string;
  onType: (at: number, value: string) => void;
  onEscape: () => void;
}

/** One season's area: its heading, its rules with their counts (inputs while they are changed), and when a change shows. */
function SeasonRules(props: SeasonRulesProps) {
  const { season, year, rules, names, turn, heldOf, tagFixed, typed, after, disabled, noteId, onType, onEscape } = props;
  const headingId = useId();
  const began = year === null ? null : season === "current" ? text.began(year) : text.begins(year);

  return (
    <section className={`rule-season rule-season-${season}`} aria-labelledby={headingId}>
      <h4 id={headingId}>
        {text.seasons[season]}
        {began !== null && <small>{began}</small>}
      </h4>
      {rules === null ? (
        <p className="note">{text.noNext}</p>
      ) : rules.length === 0 ? (
        <p className="note">{text.noRules}</p>
      ) : (
        <table className="rule-table">
          <thead>
            <tr>
              <th>{text.ruleColumns.rule}</th>
              <th>{text.ruleColumns.where}</th>
              <th>{text.ruleColumns.limit}</th>
              <th className="num">{text.ruleColumns.value}</th>
            </tr>
          </thead>
          <tbody>
            {rules.map((rule, at) => (
              <RuleRow
                key={at}
                rule={rule}
                names={names}
                season={text.seasons[season]}
                held={heldOf(rule)}
                tagFixed={tagFixed}
                typed={typed === null ? null : (typed(at) ?? String(rule.value))}
                after={after(at, rule)}
                disabled={disabled}
                noteId={noteId}
                onType={(value) => onType(at, value)}
                onEscape={onEscape}
              />
            ))}
          </tbody>
        </table>
      )}
      {rules !== null && rules.length > 0 && (
        <p className="note rule-season-note">{season === "current" ? text.currentNote(turn) : text.nextNote(turn)}</p>
      )}
    </section>
  );
}

interface RuleRowProps {
  rule: SquadRule;
  /** The heading of the rule's area, which names its input: the two seasons list the same rules. */
  season: string;
  names: ReadonlyMap<number, string>;
  /** The count the editor holds for the rule, where it holds one: the rule is tagged so. */
  held: HeldRule | undefined;
  /** Whether the rule is tagged as left alone when the editor does not change it (see `SeasonRulesProps`). */
  tagFixed: boolean;
  /** What the input holds; null while the counts are only shown. */
  typed: string | null;
  after: number | null;
  disabled: boolean;
  noteId: string;
  onType: (value: string) => void;
  onEscape: () => void;
}

/** One rule: whom it counts and for which teams, where, which way it limits, and its count. */
function RuleRow(props: RuleRowProps) {
  const { rule, season, names, held, tagFixed, typed, after, disabled, noteId, onType, onEscape } = props;
  const { who, scope } = ruleName(rule.name, rule.kind);
  const canEdit = ruleEditable(rule);
  const source = rule.source;
  // Why the editor leaves the rule alone, where it does.
  const fixedTitle = canEdit ? undefined : source === null ? text.notSavedTitle : text.notACountTitle;
  const tags: { key: string; label: string; title?: string }[] = [];
  if (!canEdit && tagFixed) {
    tags.push({ key: "fixed", label: text.leftAlone, title: fixedTitle });
  }
  if (held !== undefined) {
    tags.push({ key: "held", label: text.heldTag, title: text.heldTitle(held.original) });
  }
  if (rule.condition === "tfrd") {
    const division = rule.conditionCompetition === null ? undefined : names.get(rule.conditionCompetition);
    tags.push({ key: "teams", label: division ? text.teamsFrom(division) : text.teamsFromUnknown });
  } else if (rule.condition !== null) {
    tags.push({ key: "condition", label: text.condition(rule.condition), title: text.conditionTitle });
  }
  if (source && (source.fromYear !== null || source.toYear !== null)) {
    tags.push({ key: "years", label: text.years(source.fromYear, source.toYear), title: text.yearsTitle });
  }
  if (source && source.value !== rule.value) {
    tags.push({ key: "saved", label: text.savedDiffers(source.value), title: text.savedDiffersTitle });
  }
  const label = scope === null ? who : `${who} · ${text.scopes[scope]}`;
  const status = after === null ? " invalid" : after !== rule.value ? " changed" : "";

  return (
    <tr className={canEdit ? undefined : "rule-fixed"}>
      <th scope="row" title={rule.name === null ? undefined : text.gameKey(rule.name)}>
        <span className="rule-who">{who}</span>
        {tags.length > 0 && (
          <span className="rule-tags">
            {tags.map((tag) => (
              <span key={tag.key} className="tag" title={tag.title}>
                {tag.label}
              </span>
            ))}
          </span>
        )}
      </th>
      <td>{scope === null ? <span className="muted">{profileText.none}</span> : text.scopes[scope]}</td>
      <td>{limitWord(rule.compare)}</td>
      <td className="num">
        {typed !== null && canEdit ? (
          <input
            type="text"
            inputMode="numeric"
            className={`edit-input rule-input${status}`}
            style={{ "--digits": 3 } as CSSProperties}
            aria-label={`${season}: ${label}, ${limitWord(rule.compare)}`}
            aria-describedby={noteId}
            aria-invalid={status === " invalid" || undefined}
            value={typed}
            disabled={disabled}
            onChange={(e) => onType(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.stopPropagation();
                onEscape();
              }
            }}
          />
        ) : (
          <span className={canEdit ? "rule-value" : "rule-value muted"} title={fixedTitle}>
            {formatNumber(rule.value)}
          </span>
        )}
      </td>
    </tr>
  );
}
