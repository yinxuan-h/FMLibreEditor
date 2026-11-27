// The Competitions tab: the squad rules of the competitions the game is playing, this season's and next season's, as
// the app reads them in one pass over the game's memory, against the probe's `rules`, which reads them on its own; the
// tab's list (every competition being played, those with no squad rule too) and its search, the panel of a competition
// with an area for each season, its Change rules button turning the counts into inputs (Cancel: nothing is saved), the
// panel of a competition with no squad rule, and Read again. With --writes, one rule is set through the
// command and put back with Undo, and one is typed in the panel, saved and undone: the probe reads each back, in the
// list and in the nation's rules the save keeps. On a build whose profile does not turn the squad rules on, the tab
// saying why.
import { SHOWN, VIS, sleep } from "../lib/app.mjs";
import { humanClubs } from "../lib/witness.mjs";

export const name = "rules";
export const writes = false;

const PANEL = `${VIS}(".profile:not(.popup)")`;
const GROUP = `${PANEL}?.querySelector("section.squad-rules")`;
const ROWS = `[...document.querySelectorAll("table.competition-grid tbody tr")].filter(${SHOWN})`;

const limit = (compare) => (compare < 0 ? "at most" : "at least");
const ruleKey = (rule) => `${limit(rule.compare)}|${rule.value}|${rule.name ?? "kind"}`;
const editable = (rule) => (rule.source !== null || rule.unsaved) && rule.value >= 0 && rule.value <= 255;
const hasRules = (competition) => competition.current.length + (competition.next?.length ?? 0) > 0;

/** How many competitions `fmlibre-probe rules` finds being played, and how many of them have squad rules. */
function probePlayed(probe) {
  // UID 0 is no competition's: the probe prints its counts and no competition.
  const m = /(\d+) competitions being played, (\d+) with squad rules/.exec(probe.run(["rules", "0"]));
  return m ? { total: Number(m[1]), ruled: Number(m[2]) } : null;
}

/**
 * What `fmlibre-probe rules UID` prints for one competition: the rules of each season as "limit|value|name" keys, as
 * `ruleKey` makes them of the app's, and for each whether the nation's rules hold the same value ("saved"), another
 * ("saved as N") or no such rule; `next` is null where the game holds no list for the next season.
 */
export function probeRules(probe, uid) {
  const seasons = { current: [], next: null };
  let season = null;
  for (const line of probe.run(["rules", uid]).split(/\r?\n/)) {
    if (/^\s+This season/.test(line)) season = "current";
    else if (/^\s+Next season: the game holds no rules/.test(line)) season = null;
    else if (/^\s+Next season/.test(line)) {
      season = "next";
      seasons.next = [];
    } else if (season) {
      const m = line.match(/^\s+\d+\s+(at most|at least|compare\?)\s+(-?\d+)\s+(\S+)(.*)$/);
      if (m) {
        const saved = / · saved as (-?\d+)/.exec(m[4]);
        seasons[season].push({
          key: `${m[1]}|${m[2]}|${m[3]}`,
          value: Number(m[2]),
          saved: saved ? Number(saved[1]) : / · saved/.test(m[4]) ? Number(m[2]) : null,
        });
      }
    }
  }
  return seasons;
}

/** The league of a human manager's club where it has a rule the editor changes, or else the competition with the most. */
async function witness(app, competitions) {
  const count = (c) => [...c.current, ...(c.next ?? [])].filter(editable).length;
  const human = await humanClubs(app);
  for (const uid of human) {
    const club = (await app.must("search_clubs", { query: String(uid), limit: 5 })).rows.find((c) => c.uid === uid);
    const league = competitions.find((c) => c.uid === club?.leagueUid);
    if (league && count(league) > 0) return league;
  }
  return [...competitions].sort((a, b) => count(b) - count(a))[0];
}

/** Opens the Competitions tab and the panel of the competition `uid`, by a search for its UID. */
async function openCompetition(app, competition) {
  await app.openTab("Competitions");
  await app.waitFor(`${VIS}(".search input")`, "the competitions' search");
  await app.setInput(`${VIS}(".search input")`, String(competition.uid));
  // By its UID cell: a short UID (12) is also part of other competitions' UIDs, which the search then lists too.
  const row = `${ROWS}.find((tr) => tr.querySelector("td.uid")?.textContent.trim() === ${JSON.stringify(String(competition.uid))})`;
  await app.waitFor(`!!${row}`, `the row of ${competition.name}`, 60000);
  await sleep(300);
  await app.click(row, `the row of ${competition.name}`);
  await app.waitFor(`!!${GROUP}`, `${competition.name}'s squad rules`);
}

export async function run({ app, report, probe, writes: mayWrite }) {
  const started = Date.now();
  const view = await app.must("competition_rules", { again: false });
  if (view.unavailable) {
    await app.openTab("Competitions");
    await app.waitFor(`${VIS}("section.national-unavailable p")`, "the note that says why");
    const note = await app.js(`${VIS}("section.national-unavailable p").textContent`);
    report.check("the tab says why it shows no squad rules", /editor/.test(note ?? ""), `${view.unavailable}: ${note}`);
    report.skip("the squad rules", `this build shows none (${view.unavailable})`);
    return;
  }
  const competitions = view.competitions;
  const ruled = competitions.filter(hasRules);
  const rules = competitions.flatMap((c) => [...c.current, ...(c.next ?? [])]);
  report.note(`${competitions.length} competitions, ${ruled.length} with squad rules, ${rules.length} rules, ` +
    `${rules.filter(editable).length} the editor changes; read in ${((Date.now() - started) / 1000).toFixed(1)} s`);
  report.check("competitions have squad rules", ruled.length > 0);
  // Those with none are listed too, so that a search finds them (#274: Slovakia's divisions seemed unknown).
  const played = probePlayed(probe);
  report.check("the app lists every competition the probe finds being played, with squad rules or none",
    competitions.length === played?.total && ruled.length === played?.ruled,
    `${competitions.length} listed, ${ruled.length} with rules; the probe: ${JSON.stringify(played)}`);
  report.check("nearly every rule is of a kind the game names",
    rules.filter((rule) => rule.name !== null).length >= 0.9 * rules.length);
  const shared = competitions.filter((c) => c.sharedWith.length > 0);
  report.check("competitions played under the same lists name each other",
    shared.every((c) => c.sharedWith.every((uid) => competitions.find((o) => o.uid === uid)?.sharedWith.includes(c.uid) ?? true)),
    `${shared.length} share their lists`);

  const chosen = await witness(app, competitions);
  report.note(`witness: ${chosen.name} (${chosen.uid}), ${chosen.nation ?? "no nation"}: ${chosen.current.length} rules this ` +
    `season, ${chosen.next?.length ?? "no"} next`);
  const seen = probeRules(probe, chosen.uid);
  report.check(`the app reads what the probe reads of ${chosen.name} this season`,
    JSON.stringify(seen.current.map((r) => r.key)) === JSON.stringify(chosen.current.map(ruleKey)),
    seen.current.map((r) => r.key).join(" / "));
  report.check("and next season",
    JSON.stringify(seen.next?.map((r) => r.key) ?? null) === JSON.stringify(chosen.next?.map(ruleKey) ?? null),
    seen.next?.map((r) => r.key).join(" / ") ?? "no list");
  report.check("and what the nation's rules hold for each rule",
    JSON.stringify(seen.current.map((r) => r.saved)) === JSON.stringify(chosen.current.map((r) => r.source?.value ?? null)));

  // The tab: every competition in the list, and the witness found by its UID.
  await app.openTab("Competitions");
  await app.waitFor(`${ROWS}.length > 0`, "the competitions' rows", 120000);
  await app.setInput(`${VIS}(".search input")`, "");
  await app.waitFor(`${ROWS}.length === ${competitions.length}`, "every competition listed");
  report.check("the tab lists every competition, those with no squad rule too", true, `${competitions.length} rows`);
  await openCompetition(app, chosen);
  const areas = await app.js(`[...${GROUP}.querySelectorAll(".rule-season")].map((area) => ({
    heading: area.querySelector("h4").childNodes[0].textContent,
    values: [...area.querySelectorAll("table.rule-table tbody tr td.num")].map((td) => td.textContent.replace(/\\D/g, "")),
  }))`);
  report.check("its panel has an area for this season and one for the next",
    JSON.stringify(areas.map((a) => a.heading)) === JSON.stringify(["This season", "Next season"]), JSON.stringify(areas));
  report.check("each with its rules and their counts",
    JSON.stringify(areas.map((a) => a.values)) ===
      JSON.stringify([chosen.current.map((r) => String(r.value)), (chosen.next ?? []).map((r) => String(r.value))]),
    JSON.stringify(areas.map((a) => a.values)));
  const notices = await app.js(`[...${GROUP}.querySelectorAll(".rule-notice")].map((p) => p.textContent).join(" | ")`);
  if (chosen.sharedWith.length > 0) {
    report.check("and the note that names the competitions that share them", /a change is theirs too/.test(notices), notices);
  }
  await app.shot("rules-panel");

  // Change rules turns the counts the editor changes into inputs; Cancel leaves them.
  const change = `[...${GROUP}.querySelectorAll(".prize-actions button")].find((b) => b.textContent === "Change rules")`;
  const changeable = [...chosen.current, ...(chosen.next ?? [])].filter(editable).length;
  const canWrite = await app.js(`!!${change} && !${change}.disabled`);
  if (canWrite) {
    await app.click(change, "Change rules");
    await app.waitFor(`${GROUP}.querySelector("input.rule-input")`, "the counts' inputs");
    const inputs = await app.js(`${GROUP}.querySelectorAll("input.rule-input").length`);
    report.check("Change rules offers every count the editor changes", inputs === changeable, `${inputs} inputs / ${changeable}`);
    await app.shot("rules-inputs");
    await app.click(`[...${GROUP}.querySelectorAll(".prize-edit button")].find((b) => b.textContent === "Cancel")`, "Cancel");
    await app.waitFor(`!${GROUP}.querySelector("input.rule-input")`, "the counts shown again");
  } else {
    report.skip("Change rules", changeable === 0 ? "the witness has no rule the editor changes" : "the game cannot be written to");
  }

  // Read again reads them anew.
  const again = `[...document.querySelectorAll(".toolbar button")].filter(${SHOWN}).find((b) => b.textContent === "Read again")`;
  await app.click(again, "Read again");
  await sleep(300);
  await app.waitFor(`${again} && !${again}.disabled && !!${GROUP}`, "the rules read again", 120000);
  report.check("Read again shows them again", true);

  // A competition the game gives no squad rule: found, and its panel says so, with nothing to change.
  const bare = competitions.find((c) => !hasRules(c));
  if (bare) {
    await openCompetition(app, bare);
    const none = `/gives this competition no squad rules/.test(${GROUP}?.textContent ?? "")`;
    await app.waitFor(none, `${bare.name}'s panel saying it has no squad rule`);
    const left = await app.js(`${GROUP}.querySelectorAll(".rule-season, .prize-actions button, input").length`);
    report.check(`the panel of ${bare.name}, which has no squad rule, says the game gives it none, with nothing to change`,
      left === 0, `${left} areas, buttons or inputs`);
    await app.shot("rules-none");
  } else {
    report.skip("a competition with no squad rule", "every competition the game plays has some");
  }

  if (!mayWrite) {
    report.skip("changing a squad rule", "writes to the game: run with --writes, on a save you can lose");
  } else if (!canWrite) {
    report.skip("changing a squad rule", "the game cannot be written to, or the witness has no rule the editor changes");
  } else {
    await changeRule({ app, report, probe, competition: chosen });
    await holdRule({ app, report, probe, competitions });
  }
  await app.press("Escape");
}

/** "Its squads are registered by 6 days, at most 25 players by each", as the probe prints it: the most, or null. */
function probeRegistration(probe, uid) {
  const m = /Its squads are registered by (\d+) days, at most (\d+) players by each/.exec(probe.run(["rules", uid]));
  return m ? { days: Number(m[1]), most: Number(m[2]) } : null;
}

/**
 * A rule of a competition whose rules the game keeps in no record (UEFA's Champions League): set through the command,
 * it is written to the list alone, and to the days squads are registered by where it is the squad's size; the editor
 * holds it from then on, and its undo lets go of it.
 */
async function holdRule({ app, report, probe, competitions }) {
  // One with days its squads are registered by first (UEFA's): a change of its squad's size is written to those too.
  const candidates = competitions.filter((c) => c.unsaved && c.current.some(editable));
  const unsaved = candidates.find((c) => c.registration) ?? candidates[0];
  if (!unsaved) {
    report.skip("a rule no record keeps", "no competition here has rules the game keeps in no record");
    return;
  }
  const size = unsaved.current.findIndex((r) => r.name === "max_squad_size" && r.compare < 0 && editable(r));
  const at = size >= 0 ? size : unsaved.current.findIndex(editable);
  const rule = unsaved.current[at];
  const to = rule.value < 255 ? rule.value + 1 : rule.value - 1;
  const read = () => probeRules(probe, unsaved.uid).current[at];
  const days = probeRegistration(probe, unsaved.uid);
  report.note(`held rule: ${unsaved.name} (${unsaved.uid}), ${rule.name ?? `kind ${rule.kind}`}, ${limit(rule.compare)} ${rule.value}` +
    (days ? `; registered by ${days.days} days, at most ${days.most}` : ""));
  report.check("its rules are in no record, and the editor changes them all the same",
    rule.source === null && rule.unsaved, JSON.stringify({ source: rule.source, unsaved: rule.unsaved }));

  const input = { season: "current", rule: at, kind: rule.kind, from: rule.value, value: to };
  const set = await app.must("set_competition_rules", { competition: unsaved.uid, changes: [input] });
  report.check("the count is written to the list", set.changes.length === 1 && read().value === to, JSON.stringify(read()));
  if (size >= 0 && days?.most === rule.value) {
    const now = probeRegistration(probe, unsaved.uid);
    report.check("and to the days squads are registered by, which repeat the squad's size", now?.most === to, JSON.stringify(now));
  }
  const holder = Math.min(unsaved.uid, ...unsaved.sharedWith);
  const held = set.held.find((h) => h.competition === holder && h.kind === rule.kind && h.season === "current");
  report.check("the editor holds it, with the game's own count", held?.value === to && held?.original === rule.value,
    JSON.stringify(set.held));
  const logged = (await app.must("change_log")).find((c) => c.id === set.changes[0].id);
  report.check("and the log says so", logged?.rule?.held === true && logged?.entity === "competition", JSON.stringify(logged?.rule));
  await app.must("undo_change", { id: set.changes[0].id });
  const back = read();
  const after = await app.must("competition_rules", { again: false });
  report.check("Undo puts the game's own count back and lets go of the rule",
    back.value === rule.value && !after.held.some((h) => h.competition === holder && h.kind === rule.kind),
    `${JSON.stringify(back)}; held ${JSON.stringify(after.held)}`);
  if (size >= 0 && days?.most === rule.value) {
    report.check("and the days squads are registered by say what they said", probeRegistration(probe, unsaved.uid)?.most === rule.value);
  }
}

/** Sets one rule of `competition` through the command and undoes it, then one in the panel, each read back by the probe. */
async function changeRule({ app, report, probe, competition }) {
  // Next season's list where the game holds one: the game's own Next Season page shows a change of it at once.
  const season = (competition.next ?? []).some(editable) ? "next" : "current";
  const list = season === "next" ? competition.next : competition.current;
  const at = list.findIndex(editable);
  const rule = list[at];
  const to = rule.value < 255 ? rule.value + 1 : rule.value - 1;
  const read = () => probeRules(probe, competition.uid)[season][at];
  const before = read();
  report.note(`rule: ${season} season, ${rule.name ?? `kind ${rule.kind}`}, ${limit(rule.compare)} ${rule.value}; ` +
    `the nation's rules hold ${before.saved}`);

  const input = { season, rule: at, kind: rule.kind, from: rule.value, value: to };
  const set = await app.must("set_competition_rules", { competition: competition.uid, changes: [input] });
  const after = read();
  report.check("set_competition_rules writes the count to the list and to the nation's rules",
    set.changes.length === 1 && after.value === to && after.saved === to, JSON.stringify(after));
  const now = set.competitions.find((c) => c.uid === competition.uid);
  report.check("and gives the rules as they are now",
    (season === "next" ? now?.next : now?.current)?.[at]?.value === to);
  for (const uid of competition.sharedWith) {
    const other = set.competitions.find((c) => c.uid === uid);
    report.check(`the rule is that of ${other?.name ?? uid} too`, (season === "next" ? other?.next : other?.current)?.[at]?.value === to);
  }
  const logged = (await app.must("change_log")).find((c) => c.id === set.changes[0].id);
  report.check("the change is logged as the competition's, with the rule it set",
    logged?.entity === "competition" && logged?.uid === competition.uid && logged?.old === rule.value && logged?.new === to &&
      logged?.field === `rule.${season}.${rule.name ?? `kind${rule.kind}`}` && logged?.rule?.kind === rule.kind &&
      logged?.rule?.season === season && logged?.rule?.shared === competition.sharedWith.length,
    JSON.stringify({ entity: logged?.entity, field: logged?.field, rule: logged?.rule }));
  const stale = await app.invoke("set_competition_rules", { competition: competition.uid, changes: [input] });
  report.check("a change from a count the list no longer shows is refused", !stale.ok && stale.error?.code === "app.dataChanged",
    JSON.stringify(stale.error ?? stale.value?.changes?.length));
  await app.must("undo_change", { id: set.changes[0].id });
  const undone = read();
  report.check("Undo puts the count back, in the list and in the nation's rules",
    undone.value === before.value && undone.saved === before.saved, JSON.stringify(undone));
  const kept = (await app.must("competition_rules", { again: false })).competitions.find((c) => c.uid === competition.uid);
  report.check("and the app shows it again", (season === "next" ? kept?.next : kept?.current)?.[at]?.value === rule.value);

  // In the panel: Change rules, a count typed, Save.
  await openCompetition(app, competition);
  await app.click(`[...${GROUP}.querySelectorAll(".prize-actions button")].find((b) => b.textContent === "Change rules")`, "Change rules");
  const area = `${GROUP}.querySelector(".rule-season-${season}")`;
  const row = `${area}.querySelectorAll("table.rule-table tbody tr")[${at}]`;
  const box = `${row}.querySelector("input.rule-input")`;
  await app.waitFor(`!!${box}`, "the rule's input");
  await app.setInput(box, to);
  const save = `[...${GROUP}.querySelectorAll(".prize-edit button")].find((b) => b.textContent === "Save rules")`;
  await app.waitFor(`${save} && !${save}.disabled`, "Save rules");
  await app.click(save, "Save rules");
  await app.waitFor(`!${GROUP}.querySelector("input.rule-input")`, "the rules saved", 30000);
  const shown = await app.js(`${row}.querySelector("td.num").textContent.replace(/\\D/g, "")`);
  const typed = read();
  report.check("Save writes the count typed, and the panel shows it",
    shown === String(to) && typed.value === to && typed.saved === to, `${shown}; ${JSON.stringify(typed)}`);
  await app.shot("rules-saved");
  const saved = (await app.must("change_log")).find((c) => c.undoneAt == null && c.entity === "competition" &&
    c.uid === competition.uid);
  if (saved) await app.must("undo_change", { id: saved.id });
  const back = read();
  report.check("and its Undo puts it back", !!saved && back.value === before.value && back.saved === before.saved, JSON.stringify(back));
}
