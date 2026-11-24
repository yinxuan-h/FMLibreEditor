// The records of how a career's players develop: a copy of the app on an empty data folder takes the first record as it
// connects, and a player's Development tab says that there is nothing to compare with yet. The record is then dated
// five weeks back, with a witness's current ability and one of his attributes lower, as if the app had read the game
// then; a copy started on the same folder, as after a restart, takes a second record, and shows what he gained: under
// his abilities, beside the attribute, in the Development tab and in his club's squad. Nothing is written to the game.
import { readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { gunzipSync, gzipSync } from "node:zlib";

import { App, SHOWN, VIS, sleep } from "../lib/app.mjs";
import { clubPlayers, topClubs } from "../lib/witness.mjs";

export const name = "progress";
export const writes = false;

/** How much lower the dated-back record has the witness's current ability, and how many days back it is dated. */
const GAINED = 5;
const DAYS_BACK = 35;

const PROFILE = `${VIS}(".profile:not(.popup)")`;
const TABS = `[...(${PROFILE}?.querySelectorAll(".profile-tabs [role=tab]") ?? [])]`;

/** Opens the profile of the player with `uid` from the Players tab, and waits for its tabs. */
async function openPlayer(copy, uid) {
  await copy.openTab("Players");
  await copy.waitFor(`${VIS}(".search input")`, "the players' search");
  await copy.setInput(`${VIS}(".search input")`, String(uid));
  const rows = `[...document.querySelectorAll("table.grid tbody tr")].filter(${SHOWN})`;
  await copy.waitFor(`${rows}.length === 1`, "the player's row");
  await sleep(400);
  await copy.click(`${rows}[0]`, "the player's row");
  await copy.waitFor(`${TABS}.length === 2`, "the profile's tabs", 30000);
}

export async function run({ app, report }) {
  const connection = await app.must("current_connection");
  const human = (await app.must("search_staff", { query: "", limit: 20 })).rows.some((member) => member.isHuman);
  if (!human || connection.gameDate == null) {
    report.skip("the records", "the save has no manager of the user's, or no date: the app keeps no records of it");
    return;
  }
  const [club] = await topClubs(app, 1);
  const [player] = await clubPlayers(app, club, 1);
  report.note(`witness: ${player.name} (${player.uid}) of ${club.name}`);
  const data = join(app.folder, "progress-data");
  rmSync(data, { recursive: true, force: true });
  const folder = join(data, "timeline");
  const files = () => {
    try {
      return readdirSync(folder).filter((file) => file.endsWith(".jsonl.gz"));
    } catch {
      return [];
    }
  };

  let first;
  let second;
  try {
    // The first copy: one record, and nothing to compare with.
    first = await App.start({ exe: app.exe, port: app.port + 2, folder: join(app.folder, "progress-first"), env: { FMLIBRE_DATA: data } });
    await first.connect();
    const [file] = files();
    report.check("connecting takes the career's first record", files().length === 1, file ?? "no file");
    if (!file) return;
    const path = join(folder, file);
    const record = JSON.parse(gunzipSync(readFileSync(path)).toString("utf8"));
    const players = (await first.must("search_players", { query: "", limit: 1 })).total;
    report.check("it holds every player whole", record.full === true && record.players.length === players,
      `${record.players.length} of ${players} players, ${statSync(path).size} bytes`);
    const today = connection.gameDate;
    report.check("under the game's date", JSON.stringify(record.date) === JSON.stringify(today), JSON.stringify(record.date));

    const alone = await first.must("player_progress", { uid: player.uid });
    report.check("a player has only what he is now", alone.recording === true && alone.dates.length === 1 &&
      alone.series.current_ability?.[0] === player.currentAbility, JSON.stringify({ dates: alone.dates.length, recording: alone.recording }));
    await openPlayer(first, player.uid);
    report.check("his profile has no note under the abilities", await first.js(`!${PROFILE}.querySelector(".ability-trend")`));
    await first.click(`${TABS}[1]`, "the Development tab");
    await first.waitFor(`${PROFILE}.querySelector(".progress-empty")`, "the empty Development tab");
    report.check("the Development tab says there is nothing to compare with yet",
      (await first.js(`${PROFILE}.querySelector(".progress-empty h3").textContent`)) === "Nothing to compare with yet");
    await first.shot("progress-empty");
    // Reading the data again on the same day takes no second record.
    const size = statSync(path).size;
    await first.must("reload");
    report.check("reading again the same day adds nothing", statSync(path).size === size, `${statSync(path).size} bytes, was ${size}`);
    report.check("no errors in the first copy's console", first.consoleProblems.length === 0, first.consoleProblems.slice(0, 3).join(" || "));
    first.close();
    first = null;

    // The record, dated back, with the witness lower: his current ability, and the first attribute he has above 1.
    const fields = record.fields;
    const pairs = record.players.find(([uid]) => uid === player.uid)?.[1];
    const ability = fields.indexOf("current_ability");
    const at = (field) => pairs.findIndex((value, index) => index % 2 === 0 && value === field) + 1;
    const attribute = fields.findIndex((field, index) => index > fields.indexOf("value") && pairs[at(index)] > 1 && at(index) > 0);
    const was = { ability: pairs[at(ability)], attribute: pairs[at(attribute)] };
    pairs[at(ability)] = was.ability - GAINED;
    pairs[at(attribute)] = was.attribute - 1;
    const then = new Date(Date.UTC(today.year, today.month - 1, today.day - DAYS_BACK));
    record.date = { year: then.getUTCFullYear(), month: then.getUTCMonth() + 1, day: then.getUTCDate() };
    writeFileSync(path, gzipSync(`${JSON.stringify(record)}\n`));
    const dated = statSync(path).size;

    // The second copy, as after a restart: a second record, of what changed, and the witness's gain.
    second = await App.start({ exe: app.exe, port: app.port + 4, folder: join(app.folder, "progress-second"), env: { FMLIBRE_DATA: data } });
    await second.connect();
    const added = statSync(path).size - dated;
    report.check("five weeks on, a second record follows the first", added > 0 && added < dated / 10, `${added} bytes after ${dated}`);
    const progress = await second.must("player_progress", { uid: player.uid });
    const abilities = progress.series.current_ability ?? [];
    report.check("the player's progress has both dates", progress.dates.length === 2 &&
      JSON.stringify(progress.dates[0]) === JSON.stringify(record.date) && JSON.stringify(progress.dates[1]) === JSON.stringify(today),
      JSON.stringify(progress.dates));
    report.check("and what he gained", abilities[0] === was.ability - GAINED && abilities[1] === was.ability &&
      JSON.stringify(progress.series[fields[attribute]]) === JSON.stringify([was.attribute - 1, was.attribute]),
      `${abilities.join(" → ")}; ${fields[attribute]} ${progress.series[fields[attribute]]?.join(" → ")}`);
    const [earlier] = await second.must("squad_progress", { uids: [player.uid] });
    report.check("his squad compares him with the first record", earlier?.currentAbility === was.ability - GAINED &&
      JSON.stringify(earlier.date) === JSON.stringify(record.date), JSON.stringify(earlier));

    await openPlayer(second, player.uid);
    const note = `${PROFILE}.querySelector(".ability-trend")`;
    await second.waitFor(note, "the note under the abilities");
    report.check("the note under his abilities says what he gained",
      (await second.js(`${note}.querySelector(".ability-trend-figure").textContent`)) === `+${GAINED}`, await second.js(`${note}.textContent`));
    report.check("the attribute that rose has an arrow", (await second.js(`${PROFILE}.querySelectorAll(".stat .value .trend").length`)) === 1);
    await second.shot("progress-profile");
    await second.click(note, "the note");
    await second.waitFor(`${PROFILE}.querySelector(".progress-tab svg")`, "the Development tab's chart");
    report.check("the note opens the Development tab", (await second.js(`${TABS}[1].getAttribute("aria-selected")`)) === "true");
    const changed = await second.js(`[...${PROFILE}.querySelectorAll(".progress-changes .stat")].map((row) => row.textContent)`);
    report.check("which lists the attribute that changed", changed.length === 1 && changed[0].includes(`${was.attribute - 1}→${was.attribute}`), changed.join(" | "));
    await second.shot("progress-tab");
    // The tab is out of reach while he is edited.
    await second.click(`${TABS}[0]`, "the Profile tab");
    await second.click(`[...${PROFILE}.querySelectorAll(".profile-actions button")].find((b) => b.textContent.trim() === "Edit")`, "Edit");
    await second.waitFor(`${TABS}[1].disabled`, "the Development tab locked");
    report.check("the Development tab is locked while he is edited", await second.js(`!${note} && ${TABS}[1].disabled`));
    await second.press("Escape");
    await sleep(300);
    await second.press("Escape");
    await sleep(300);

    // His club's squad: the change before his current ability.
    await second.openClub(club.name, club.uid);
    await second.click(`[...document.querySelectorAll(".profile-tabs [role=tab]")].filter(${SHOWN}).find((t) => t.textContent.trim() === "Squad")`, "the Squad tab");
    const row = `[...document.querySelectorAll(".squad-tab tbody tr")].filter(${SHOWN}).find((tr) => tr.querySelector(".name")?.textContent === ${JSON.stringify(player.name)})`;
    await second.waitFor(`${row}?.querySelector(".ability-change")`, "the change in the squad's table");
    report.check("the squad's table shows his gain", (await second.js(`${row}.querySelector(".ability-change").textContent`)) === `+${GAINED}`);
    report.check("and nobody else's", (await second.js(`document.querySelectorAll(".squad-tab .ability-change").length`)) === 1);
    await second.shot("progress-squad");
    report.check("no errors in the second copy's console", second.consoleProblems.length === 0, second.consoleProblems.slice(0, 3).join(" || "));
  } finally {
    first?.close();
    second?.close();
  }
}
