// A player's development, from the records the app keeps of a career (see `player_progress`): the note under the
// abilities in the profile's header, and the Development tab with his abilities and value over time and the
// attributes that changed. Nothing here writes to the game, or reads it: the records are the app's own.

import { useEffect, useState } from "react";
import { call } from "./api";
import { formatDate, formatMoney } from "./format";
import { ATTRIBUTE_LABELS, profileText, progressText as text, type AttributeKey } from "./labels";
import { LOWER_IS_BETTER, type Loaded, type PanelIds } from "./ProfileParts";
import {
  attributeChanges,
  changeOf,
  CURRENT_ABILITY,
  dayOf,
  DEFAULT_RANGE,
  POTENTIAL_ABILITY,
  rangesOf,
  referenceOf,
  signed,
  VALUE,
  type Change,
  type ProgressRange,
} from "./progress";
import type { EarlierAbility, FmDate, PlayerProgress } from "./types";

/** "▲" for a value that rose, "▼" for one that fell. */
const mark = (delta: number) => (delta > 0 ? "▲" : "▼");

/** "120 → 126 since 1 Jul 2026": a change in words, for hover texts and screen readers. */
export const changeTitle = (change: Change, shown: (value: number) => string = String) =>
  text.change(shown(change.from), shown(change.to), formatDate(change.since));

/**
 * Under the abilities in the header: how far the current ability moved over the last year of the game's time, or
 * since the first record of him. A button, which opens the Development tab. Nothing while it has not moved.
 */
export function AbilityTrend({ progress, onOpen }: { progress: PlayerProgress; onOpen: () => void }) {
  const change = changeOf(progress, CURRENT_ABILITY, referenceOf(progress, DEFAULT_RANGE));
  if (!change) {
    return null;
  }
  const delta = change.to - change.from;
  return (
    <button
      type="button"
      className={`ability-trend ${delta > 0 ? "good" : "bad"}`}
      title={`${profileText.currentAbility}: ${changeTitle(change)}\n${text.openTab}`}
      onClick={onOpen}
    >
      <span className="trend-mark" aria-hidden="true">
        {mark(delta)}
      </span>
      <span className="ability-trend-figure">{signed(delta)}</span>
      <span className="ability-trend-since">{text.since(formatDate(change.since))}</span>
    </button>
  );
}

/**
 * What the current ability of each of the players `uids` was at the record they are compared with (see
 * `squad_progress`): by UID, read again whenever the players or the game data are. Empty until read, where the app
 * keeps no records of the career, and before its second one.
 */
export function useEarlierAbilities(uids: readonly number[], version: number): ReadonlyMap<number, EarlierAbility> {
  const [earlier, setEarlier] = useState<ReadonlyMap<number, EarlierAbility>>(new Map());
  const key = uids.join(",");

  useEffect(() => {
    let cancelled = false;
    if (key === "") {
      setEarlier(new Map());
      return;
    }
    call<EarlierAbility[]>("squad_progress", { uids: key.split(",").map(Number) }).then(
      (rows) => {
        if (!cancelled) {
          setEarlier(new Map(rows.map((row) => [row.uid, row])));
        }
      },
      // The table is no worse without them.
      () => undefined,
    );
    return () => {
      cancelled = true;
    };
  }, [key, version]);

  return earlier;
}

/**
 * Before a current ability in a squad's table: how far it moved since the record he is compared with, as a small
 * signed number whose hover text says from what and since when. Nothing while it has not moved.
 */
export function AbilityChange({ now, earlier }: { now: number; earlier: EarlierAbility | undefined }) {
  if (!earlier || earlier.currentAbility === now) {
    return null;
  }
  const delta = now - earlier.currentAbility;
  const title = `${profileText.currentAbility}: ${changeTitle({ from: earlier.currentAbility, to: now, since: earlier.date })}`;
  return (
    <span className={`ability-change ${delta > 0 ? "good" : "bad"}`} title={title} aria-label={title}>
      {signed(delta)}
    </span>
  );
}

interface ProgressTabProps {
  loaded: Loaded<PlayerProgress> | null;
  panel: PanelIds;
}

/**
 * The Development tab of a player's profile: a period to look back over, where the records reach back far enough for
 * a choice, his abilities and his value as lines over the game's time, and the attributes that changed, the most
 * first. With no record before today's, it says how the records come about instead.
 */
export function ProgressTab({ loaded, panel }: ProgressTabProps) {
  const [range, setRange] = useState<ProgressRange>(DEFAULT_RANGE);
  const progress = loaded?.details;

  return (
    <div className="progress-tab" role="tabpanel" id={panel.id} aria-labelledby={panel.labelledBy}>
      {!loaded && (
        <p className="profile-status" role="status">
          {text.loading}
        </p>
      )}
      {loaded?.error !== undefined && (
        <div className="banner" role="alert">
          {text.failed(loaded.error)}
        </div>
      )}
      {progress &&
        (progress.dates.length < 2 ? (
          <div className="progress-empty">
            <h3>{progress.recording ? text.emptyTitle : text.offTitle}</h3>
            <p>{progress.recording ? text.emptyText : text.offText}</p>
          </div>
        ) : (
          <ProgressBody progress={progress} range={range} onRange={setRange} />
        ))}
    </div>
  );
}

interface ProgressBodyProps {
  progress: PlayerProgress;
  range: ProgressRange;
  onRange: (range: ProgressRange) => void;
}

function ProgressBody({ progress, range, onRange }: ProgressBodyProps) {
  const ranges = rangesOf(progress);
  // A period the records do not reach back past shows all of them, as "all" does.
  const shown = ranges.includes(range) ? range : "all";
  const reference = referenceOf(progress, shown) ?? 0;
  const dates = progress.dates.slice(reference);
  const series = (field: string) => progress.series[field]?.slice(reference);
  const current = series(CURRENT_ABILITY);
  // A potential below zero is a range the game has yet to settle, not a number to draw.
  const potential = series(POTENTIAL_ABILITY)?.map((value) => (value !== null && value > 0 ? value : null));
  const value = series(VALUE);
  const changes = [...attributeChanges(progress, reference)].sort(
    ([a, one], [b, other]) =>
      Math.abs(other.to - other.from) - Math.abs(one.to - one.from) || labelOf(a).localeCompare(labelOf(b)),
  );

  return (
    <>
      <div className="progress-head">
        {ranges.length > 1 && (
          <div className="squad-teams" role="group" aria-label={text.rangeLabel}>
            {ranges.map((id) => (
              <button key={id} type="button" aria-pressed={id === shown} onClick={() => onRange(id)}>
                {text.ranges[id]}
              </button>
            ))}
          </div>
        )}
        <p className="progress-recorded">{text.recorded(progress.dates.length - 1, formatDate(progress.dates[0]))}</p>
      </div>

      {current && (
        <section className="profile-group">
          <h3>{text.abilityTitle}</h3>
          <LineChart
            title={text.abilityTitle}
            dates={dates}
            lines={[
              { key: "current", label: profileText.currentAbility, values: current },
              ...(potential?.some((point) => point !== null)
                ? [{ key: "potential", label: profileText.potentialAbility, values: potential }]
                : []),
            ]}
            shown={String}
            whole
            most={200}
          />
        </section>
      )}

      {value?.some((point) => point !== null && point > 0) && (
        <section className="profile-group">
          <h3>{text.valueTitle}</h3>
          <LineChart
            title={text.valueTitle}
            dates={dates}
            lines={[{ key: "value", label: text.valueTitle, values: value }]}
            shown={(pounds) => formatMoney(pounds, { notation: "compact", maximumFractionDigits: 1 })}
          />
        </section>
      )}

      <section className="profile-group">
        <h3>{text.attributesTitle}</h3>
        {changes.length === 0 ? (
          <p className="note">{text.noAttributeChanges(formatDate(dates[0]))}</p>
        ) : (
          <dl className="stats progress-changes">
            {changes.map(([key, change]) => {
              const delta = change.to - change.from;
              const good = delta > 0 !== LOWER_IS_BETTER.has(key);
              return (
                <div key={key} className="stat" title={changeTitle(change)}>
                  <dt>{labelOf(key)}</dt>
                  <dd>
                    <span className="progress-from">{change.from}</span>
                    <span className="progress-arrow" aria-hidden="true">
                      →
                    </span>
                    <span className="progress-to">{change.to}</span>
                    <span className={`progress-delta ${good ? "good" : "bad"}`}>
                      <span className="trend-mark" aria-hidden="true">
                        {mark(delta)}
                      </span>
                      {Math.abs(delta)}
                    </span>
                  </dd>
                </div>
              );
            })}
          </dl>
        )}
      </section>
    </>
  );
}

/** An attribute's name, or its key where the language has none. */
const labelOf = (key: string) => ATTRIBUTE_LABELS[key as AttributeKey] ?? key;

interface Line {
  key: string;
  label: string;
  /** A value for each date; none before the first record that has one. */
  values: (number | null)[];
}

interface LineChartProps {
  title: string;
  dates: FmDate[];
  lines: Line[];
  /** A value as the axis and the legend write it. */
  shown: (value: number) => string;
  /** Whether the values are whole numbers, so that the axis names only whole ones. */
  whole?: boolean;
  /** The most a value can be: the scale does not run past it. */
  most?: number;
}

/** The drawing's size in its own units; it scales with the panel. */
const CHART = { width: 560, height: 168, left: 46, right: 14, top: 12, bottom: 26 };

/** Up to this many points, each is marked with a dot. */
const DOTS_UP_TO = 40;

/**
 * Lines over the game's time: a point for each record and one for now, set apart as the dates are. The scale runs a
 * little past the least and the most, so that a line that does not move sits in the middle. Under it, each line's
 * name with its first and last value, which is also what a screen reader is told.
 */
function LineChart({ title, dates, lines, shown, whole, most: ceiling = Infinity }: LineChartProps) {
  const numbers = lines.flatMap((line) => line.values.filter((value): value is number => value !== null));
  if (numbers.length === 0 || dates.length < 2) {
    return null;
  }
  const [least, most] = [Math.min(...numbers), Math.max(...numbers)];
  const room = Math.max((most - least) * 0.15, whole ? 2 : Math.abs(most) * 0.05, 1);
  const [low, high] = [Math.max(0, least - room), Math.min(ceiling, most + room)];
  const [first, last] = [dayOf(dates[0]), dayOf(dates[dates.length - 1])];
  const innerWidth = CHART.width - CHART.left - CHART.right;
  const innerHeight = CHART.height - CHART.top - CHART.bottom;
  const x = (date: FmDate) => CHART.left + ((dayOf(date) - first) / Math.max(1, last - first)) * innerWidth;
  const y = (value: number) => CHART.top + (1 - (value - low) / (high - low)) * innerHeight;
  const ticks = [low, (low + high) / 2, high].map((tick) => (whole ? Math.round(tick) : tick));
  const ends = lines.map((line) => {
    const known = line.values.filter((value): value is number => value !== null);
    return { ...line, from: known[0], to: known[known.length - 1] };
  });
  const summary = ends.map((line) => `${line.label}: ${shown(line.from)} → ${shown(line.to)}`).join("; ");

  return (
    <figure className="progress-chart">
      <svg
        viewBox={`0 0 ${CHART.width} ${CHART.height}`}
        role="img"
        aria-label={`${title}. ${formatDate(dates[0])} – ${formatDate(dates[dates.length - 1])}. ${summary}`}
      >
        {ticks.map((tick, i) => (
          <g key={i} className="progress-grid">
            <line x1={CHART.left} x2={CHART.width - CHART.right} y1={y(tick)} y2={y(tick)} />
            <text x={CHART.left - 8} y={y(tick)} dy="0.32em" textAnchor="end">
              {shown(tick)}
            </text>
          </g>
        ))}
        <g className="progress-axis">
          <text x={CHART.left} y={CHART.height - 6}>
            {formatDate(dates[0])}
          </text>
          <text x={CHART.width - CHART.right} y={CHART.height - 6} textAnchor="end">
            {formatDate(dates[dates.length - 1])}
          </text>
        </g>
        {lines.map((line) => {
          const points = line.values.flatMap((value, at) => (value === null ? [] : [{ value, date: dates[at] }]));
          return (
            <g key={line.key} className={`progress-line ${line.key}`}>
              <polyline points={points.map((point) => `${x(point.date).toFixed(1)},${y(point.value).toFixed(1)}`).join(" ")} />
              {points.length <= DOTS_UP_TO &&
                points.map((point, at) => (
                  <circle key={at} cx={x(point.date)} cy={y(point.value)} r="3">
                    <title>{`${formatDate(point.date)} · ${line.label}: ${shown(point.value)}`}</title>
                  </circle>
                ))}
            </g>
          );
        })}
      </svg>
      <figcaption>
        {ends.map((line) => (
          <span key={line.key} className={`progress-key ${line.key}`}>
            <span className="progress-swatch" aria-hidden="true" />
            {line.label}
            <span className="progress-key-values">
              {line.from === line.to ? shown(line.to) : `${shown(line.from)} → ${shown(line.to)}`}
            </span>
          </span>
        ))}
      </figcaption>
    </figure>
  );
}
