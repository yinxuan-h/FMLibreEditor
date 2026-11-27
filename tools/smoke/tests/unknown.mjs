// The day of an update, rehearsed: the app started with FMLIBRE_IGNORE_PROFILES=1, so that the build is one it has no
// profile for. It must find the in-game date and the planned retirements by itself, read the clubs' finances and the
// tactic in use as the newest other build of its kind lays its objects out (where that is how the build's own profile
// says it lays them out: on the first build of an update that moved them it cannot be), say that the build is new to
// it, and write nothing until Edit anyway. The edits it tries write the value already there: refused while the build is
// locked, and a change of nothing once it is not.
import { sleep } from "../lib/app.mjs";
import { humanClubs, retiringPlayers } from "../lib/witness.mjs";

/** The fields an "Object layout" line of the probe's `calibrate` names, each with its offset: "tactics.current 0x1030". */
const moved = (calibrated) =>
  [...(calibrated.match(/^Object layout: (.*)$/m)?.[1] ?? "").matchAll(/\b([a-z_]+\.[a-z_]+) (0x[0-9a-f]+)/g)]
    .map(([, field, offset]) => `${field} ${offset}`)
    .join(", ");

/**
 * The clubs' finances and the tactic in use, read with the layout borrowed from the newest other build of its kind:
 * as the probe reads them with the build's profile.
 */
async function borrowedLayout({ app, report, probe }, rehearsed) {
  const lender = rehearsed.match(/^Object layout: as (.+) lays it out/m)?.[1];
  report.check("reads the objects as the newest other build of its kind does", lender != null,
    rehearsed.match(/^Object layout: .*$/m)?.[0] ?? "no Object layout line");
  const [own, borrowed] = [moved(probe.run(["calibrate"])), moved(rehearsed)];
  if (own !== borrowed) {
    const why = `this build moves ${own || "nothing"}, and ${lender} ${borrowed || "nothing"}: what an update moves ` +
      "itself is unread until its profile says where";
    report.skip("reads the clubs' finances with it", why);
    report.skip("finds the tactic in use with it", why);
    return;
  }
  const human = await humanClubs(app);
  const best = (await app.must("search_clubs", { query: "", limit: 40, filter: { women: false } })).rows;
  let kept = null;
  for (const uid of new Set([...human, ...best.map((club) => club.uid)])) {
    const club = await app.must("club_details", { uid });
    if (club.finances !== null) {
      kept = club;
      break;
    }
  }
  if (kept == null) {
    report.check("reads the clubs' finances with it", false, `none of ${human.length + best.length} clubs`);
  } else {
    const fields = ["finance.balance", "finance.transfer_budget", "finance.wage_budget", "facilities.corporate"];
    const seen = probe.get("club", kept.uid, fields);
    const shown = [kept.finances.balance, kept.finances.transferBudget, kept.finances.wageBudget, kept.facilities.corporate];
    report.check("reads the clubs' finances with it, as with the profile",
      fields.every((field, at) => seen[field] === shown[at]), `${kept.name}: ${JSON.stringify(seen)}`);
  }
  const [club] = human;
  if (club == null) {
    report.skip("finds the tactic in use with it", "the human manager has no club on this save");
    return;
  }
  const [senior] = await app.must("club_squads", { uid: club });
  const knowing = senior.players.filter((player) => player.tactics != null);
  // The probe stops with an error where it finds no tactic in use.
  let options = null;
  try {
    options = probe.run(["training-history", club]).match(/options (\[[^\]]*\]) \(of the tactic in use\)/)?.[1] ?? null;
  } catch {
    options = null;
  }
  report.check("finds the tactic in use with it, as with the profile", knowing.length > 0 && options != null,
    `club ${club}: ${knowing.length} players know it; the probe reads options ${options ?? "none"}`);
}

export const name = "unknown";
export const writes = false;
export const ignoreProfiles = true;

export async function run({ app, report, probe, connection: c, known }) {
  report.check("treats the build as new", !c.knownBuild && c.profile == null, c.profile ?? "no profile");
  report.check("writes nothing until allowed", c.writable && !c.editable);
  const failed = c.health.results.filter((result) => !result.passed).map((result) => result.check);
  report.check("every check passes", failed.length === 0, failed.length ? `failed: ${failed.join(", ")}` : "");
  const date = c.health.results.find((result) => result.check === "gameDate");
  report.check("finds the in-game date by itself", c.gameDate != null, JSON.stringify(c.gameDate));
  if (known?.gameDate) {
    report.check("the same date as with the profile", JSON.stringify(c.gameDate) === JSON.stringify(known.gameDate),
      `${JSON.stringify(c.gameDate)} and ${JSON.stringify(known.gameDate)}`);
  }
  const page = await app.must("search_players", { query: "", limit: 200 });
  const aged = page.rows.filter((player) => player.age != null).length;
  report.check("the players have ages", aged === page.rows.length, `${aged} of ${page.rows.length}`);

  // The game's count of the saves it loaded, which tells the app when the game has loaded a save since it read it: the
  // probe looks for it as the app does, without the profiles too.
  const rehearsed = probe.run(["calibrate"], { FMLIBRE_IGNORE_PROFILES: "1" });
  const saves = rehearsed.match(/^Saves loaded: (.*)$/m)?.[1] ?? "";
  report.check("finds the game's count of saves by itself", saves.includes("loaded since the game started"), saves);
  report.check("the game has the save it was read from", (await app.must("save_changed")) === false);
  await borrowedLayout({ app, report, probe }, rehearsed);

  const [retiring] = retiringPlayers(probe);
  if (retiring == null) {
    report.skip("finds the planned retirements by itself", "the game plans no player's retirement on this save");
  } else {
    const details = await app.must("player_details", { uid: retiring });
    report.check("finds the planned retirements by itself", details.retirementKnown && details.retires != null,
      `${details.name}: ${JSON.stringify(details.retires)}`);
  }

  const notice = `document.querySelector(".compatibility-notice")`;
  await app.waitFor(notice, "the notice about the build");
  const text = await app.js(`${notice}.textContent`);
  report.check("says the build is new, and offers Edit anyway", await app.js(`!!${notice}.querySelector("button.primary")`),
    text.replace(/\s+/g, " ").slice(0, 160));
  await app.shot("unknown-notice");

  // An edit of nothing: the value already there, which the probe reads (the app reads no field it may not write).
  const uid = page.rows[0].uid;
  const value = probe.get("player", uid, ["attribute.finishing"])["attribute.finishing"];
  const same = [{ field: "attribute.finishing", value }];
  const locked = await app.must("apply_edits", { entity: "player", uid, edits: same });
  report.check("refuses edits while locked", locked.applied.length === 0 && locked.failed.length === 1,
    JSON.stringify(locked.failed[0]?.error?.code ?? locked.failed[0]?.error ?? ""));

  await app.click(`${notice}.querySelector("button.primary")`, "Edit anyway");
  await sleep(500);
  const allowed = await app.must("current_connection");
  report.check("Edit anyway allows editing", allowed.editable);
  // Once editing is allowed, the notice is left with nothing but the checks that did not pass to tell.
  if (failed.length === 0) {
    const gone = await app.waitFor(`!${notice}`, "the notice to go", 5000).then(() => true, () => false);
    report.check("and takes the notice away", gone,
      gone ? "" : (await app.js(`${notice}?.textContent ?? ""`)).replace(/\s+/g, " ").slice(0, 160));
  } else {
    report.skip("and takes the notice away", "it still tells the checks that did not pass");
  }
  const read = await app.must("read_values", { entity: "player", uid });
  report.check("then reads the fields", read["attribute.finishing"] === value, `finishing ${read["attribute.finishing"]}`);
  const open = await app.must("apply_edits", { entity: "player", uid, edits: same });
  report.check("then takes edits", open.failed.length === 0 && open.applied.length === 0, JSON.stringify(open.failed));
  report.check("still nothing written", (await app.must("change_log")).length === 0);
  report.note(`gameDate check: ${date?.passed ? "pass" : "FAIL"} ${date?.fit} of ${date?.sampled}`);
}
