// Recipes, as far as they only read: the ready-made sets of changes the app offers, each one's preview for a witness
// picked from the save, and the interface up to Apply. A young player of one of the best known clubs as a wonderkid
// (and an older one refused), an injured player as injury-proof (or one who is not, with healing named as left out),
// a player the game plans to retire for a long career (or one it does not), and the best known club no human manages
// as rich and with the best facilities. For each preview: its old values are the ones the game holds, no value is
// made worse, what it says stays is what the game holds, and its count is its values and its steps. Then the Recipes
// button at the end of the row of Max attributes in a player's profile, its menu, the dialog with its preview and
// Close; the recipes the command palette finds once something is typed; and Recipes in the header of a club's
// profile, gone while the club is edited. Nothing is applied: `recipesapply` does that.
import { SHOWN, VIS, sleep } from "../lib/app.mjs";
import { clubPlayers, findPlayer, retiringPlayers, topClubs } from "../lib/witness.mjs";

export const name = "recipes";
export const writes = false;

/** The recipes, in the order the app offers them: whose each is, and the oldest player it is for. */
export const RECIPES = [
  ["wonderkid", "player", 21],
  ["injuryProof", "player", null],
  ["longCareer", "player", null],
  ["richClub", "club", null],
  ["topFacilities", "club", null],
];

/** The fields where a lower value is the better one. */
const LOWER_IS_BETTER = new Set(["attribute.injury_proneness"]);

/** In the page: the dialog of a recipe, and the items of the menu that is open. */
export const DIALOG = `document.querySelector(".recipe-dialog")`;
export const MENU = `[...(document.querySelector(".squad-actions-menu")?.querySelectorAll("[role=menuitem]") ?? [])]`;
export const menuItem = (label) => `${MENU}.find((i) => i.textContent === ${JSON.stringify(label)})`;

/** The values of `a` that `b` holds otherwise, as "field: a -> b"; the fields in `except` are not looked at. */
export function differences(a, b, except = new Set()) {
  return Object.keys({ ...a, ...b })
    .filter((field) => !except.has(field) && a[field] !== b[field])
    .map((field) => `${field}: ${a[field]} -> ${b[field]}`);
}

/**
 * Whom the recipes are tried on, picked from the save: a player of 21 or younger and one older, an injured player
 * and one the game plans to retire where there are such, and the best known club no human manages. `runs` lists each
 * recipe with its witness.
 */
export async function witnesses(app, probe) {
  const club = (await topClubs(app, 30))[0];
  const young = (await findPlayer(app, (details) => details.age != null && details.age <= 21))?.player ?? null;
  const old = (await clubPlayers(app, club)).find((player) => player.age != null && player.age > 21) ?? null;
  const injured = (await findPlayer(app, (details) => details.injuries.length > 0))?.player ?? null;
  let retiring = null;
  for (const uid of retiringPlayers(probe).slice(0, 20)) {
    const details = await app.must("player_details", { uid });
    if (details.retires != null) {
      retiring = details;
      break;
    }
  }
  const anyone = old ?? young;
  const runs = [
    young && ["wonderkid", "player", young],
    (injured ?? anyone) && ["injuryProof", "player", injured ?? anyone],
    (retiring ?? anyone) && ["longCareer", "player", retiring ?? anyone],
    ["richClub", "club", club],
    ["topFacilities", "club", club],
  ].filter(Boolean);
  return { club, young, old, injured, retiring, runs };
}

/**
 * Works `recipe` out for `subject` and checks the preview against what the game holds. Returns the preview and the
 * values it was worked out from. Nothing is written.
 */
export async function previewOf({ app, report }, recipe, entity, subject) {
  const uid = subject.uid;
  const before = await app.must("read_values", { entity, uid });
  const preview = await app.must("plan_recipe", { recipe, uid });
  const stale = preview.rows.filter((row) => before[row.field] !== row.old).map((row) => row.field);
  const worse = preview.rows
    .filter((row) => (LOWER_IS_BETTER.has(row.field) ? row.new >= row.old : row.new <= row.old))
    .map((row) => `${row.field}: ${row.old} -> ${row.new}`);
  const steps = Number(preview.heals != null) + Number(preview.keepsPlaying != null);
  report.check(
    `${recipe}: the preview for ${subject.name} starts from the values the game holds, and makes none worse`,
    preview.recipe === recipe && preview.uid === uid && stale.length === 0 && worse.length === 0 &&
      preview.changes === preview.rows.length + steps,
    `${preview.changes} changes, ${preview.kept.length} as good already, ${preview.skipped.length} left alone, ` +
      `${preview.left.length} steps left out` +
      (stale.length ? `; not the game's: ${stale.join(", ")}` : "") +
      (worse.length ? `; worse: ${worse.join(", ")}` : ""),
  );
  const notKept = preview.kept.filter((kept) => before[kept.field] !== kept.value).map((kept) => kept.field);
  const twice = [...preview.rows, ...preview.kept, ...preview.skipped].map((row) => row.field);
  report.check(
    `${recipe}: what it says stays is what the game holds, and each value is named once`,
    notKept.length === 0 && new Set(twice).size === twice.length,
    notKept.join(", "),
  );
  if (preview.left.length || preview.skipped.length) {
    const said = [
      ...preview.left.map((left) => `${left.step}: ${left.code}`),
      ...preview.skipped.map((skipped) => `${skipped.field}: ${skipped.code}`),
    ];
    report.note(`${recipe} leaves alone for ${subject.name}: ${said.join("; ")}`);
  }
  return { preview, before };
}

export async function run({ app, report, probe }) {
  const offered = await app.must("recipes");
  report.check(
    "the app offers the five recipes, three for a player and two for a club",
    JSON.stringify(offered.map((recipe) => [recipe.key, recipe.entity, recipe.oldest])) === JSON.stringify(RECIPES),
    offered.map((recipe) => recipe.key).join(", "),
  );

  const { club, young, old, injured, retiring, runs } = await witnesses(app, probe);
  if (!young) report.skip("wonderkid: the preview", "no player of 21 or younger among the best players of the best known clubs");
  for (const [recipe, entity, subject] of runs) {
    const { preview, before } = await previewOf({ app, report }, recipe, entity, subject);
    const after = await app.must("read_values", { entity, uid: subject.uid });
    const moved = differences(before, after);
    report.check(`${recipe}: a preview writes nothing`, moved.length === 0, moved.slice(0, 5).join("; "));
    if (recipe === "wonderkid") {
      const floors = preview.rows.filter((row) => row.field === "potential_ability" && row.new !== 180);
      report.check("wonderkid: a potential ability below 180 goes to 180", floors.length === 0, JSON.stringify(floors));
    }
    if (recipe === "injuryProof" && injured) {
      report.check("injuryProof: healing is part of it for an injured player", preview.heals === injured.injuries.length, `${preview.heals}`);
    }
    if (recipe === "injuryProof" && !injured) {
      report.check(
        "injuryProof: healing is left out, and named, for a player who is not injured",
        preview.heals == null && preview.left.some((left) => left.step === "heal" && left.code === "heal.notInjured"),
        JSON.stringify(preview.left),
      );
    }
    if (recipe === "longCareer" && retiring) {
      report.check(
        "longCareer: the retirement the game plans is part of it",
        JSON.stringify(preview.keepsPlaying?.retires) === JSON.stringify(retiring.retires),
        JSON.stringify(preview.keepsPlaying ?? preview.left),
      );
    }
    if (recipe === "richClub") {
      const fields = preview.rows.concat(preview.kept, preview.skipped).map((row) => row.field).sort();
      report.check(
        "richClub: it is about the balance and the three budgets, and nothing else",
        JSON.stringify(fields) ===
          JSON.stringify(["finance.balance", "finance.total_wage_budget", "finance.transfer_budget", "finance.wage_budget"]),
        fields.join(", "),
      );
    }
  }
  if (!injured) report.note("no injured player among the best players of the best known clubs: healing was not previewed");
  if (!retiring) report.note("the game plans no player's retirement on this save: keeping one from it was not previewed");

  // What is refused: a wonderkid older than 21, a recipe that does not exist, a club's recipe for no club.
  if (old) {
    const refused = await app.invoke("plan_recipe", { recipe: "wonderkid", uid: old.uid });
    report.check(
      "wonderkid: refused for an older player, saying how old he is",
      !refused.ok && refused.error.code === "recipe.tooOld" && refused.error.params.age === String(old.age) &&
        refused.error.params.oldest === "21",
      `${old.name} (${old.age}): ${JSON.stringify(refused.error ?? refused.value)}`,
    );
  }
  const noClub = await app.invoke("plan_recipe", { recipe: "richClub", uid: 4294967295 });
  const unknown = await app.invoke("plan_recipe", { recipe: "nothing", uid: club.uid });
  const stale = await app.invoke("apply_recipe", { id: 4294967295 });
  report.check(
    "a recipe that does not exist, a club's for no club, and a preview that is not the last one are refused",
    !noClub.ok && !unknown.ok && unknown.error.code === "recipe.unknown" && !stale.ok && stale.error.code === "recipe.outOfDate",
    `${JSON.stringify(noClub.error)} / ${JSON.stringify(unknown.error)} / ${JSON.stringify(stale.error)}`,
  );

  // The Recipes button of a player's profile: at the end of the row of Max attributes, with the player's three.
  const player = young ?? old;
  if (!player) return;
  await app.openTab("Players");
  await app.setInput(`${VIS}(".search input")`, String(player.uid));
  const rows = `[...document.querySelectorAll("table.grid tbody tr")].filter(${SHOWN})`;
  await app.waitFor(`${rows}.length === 1 && ${rows}[0].textContent.includes(${JSON.stringify(player.name)})`, "his row");
  await app.click(`${rows}[0]`, "his row");
  const profile = `${VIS}(".profile:not(.popup)")`;
  await app.waitFor(`${profile} && ${profile}.getAttribute("aria-busy") === "false"`, "his profile", 30000);
  const toggle = `${profile}.querySelector(".attribute-buttons .recipe-toggle")`;
  await app.waitFor(`${toggle} && !${toggle}.disabled`, "the Recipes button");
  const row = await app.js(`[...${profile}.querySelector(".attribute-buttons").children].map((c) => c.textContent.trim())`);
  const oneLine = await app.js(`${profile}.querySelector(".attribute-buttons").getBoundingClientRect().height < 48`);
  report.check(
    "a player's profile has Recipes at the end of the row of Max attributes, which stays one line",
    row[0] === "Max attributes" && row[row.length - 1] === "Recipes" && oneLine,
    row.join(" | "),
  );
  await app.click(toggle, "Recipes");
  await app.waitFor(`${MENU}.length > 0`, "the Recipes menu");
  const items = await app.js(`${MENU}.map((i) => [i.textContent, i.getAttribute("aria-disabled")])`);
  // Opened by a click, the menu shows no note until the pointer or the keys reach an item, though its first item has
  // the focus.
  report.check(
    "opened by a click, the menu shows no note yet",
    await app.js(`!document.querySelector(".tip") && document.activeElement === ${MENU}[0]`),
  );
  // What an item does is in a small box beside the menu once the keys (or the pointer) are on it: down and up again
  // is the first item's.
  await app.press("ArrowDown");
  await app.press("ArrowUp");
  await app.waitFor(`document.querySelector(".tip")`, "the note of the first recipe");
  const note = await app.js(`(() => {
    const box = document.querySelector(".tip").getBoundingClientRect();
    const menu = document.querySelector(".squad-actions-menu").getBoundingClientRect();
    return {
      text: document.querySelector(".tip").textContent,
      described: ${MENU}[0].getAttribute("aria-describedby") === document.querySelector(".tip").id,
      width: Math.round(box.width),
      inside: box.left >= 0 && box.right <= innerWidth && box.top >= 0 && box.bottom <= innerHeight,
      beside: box.left >= menu.right || box.right <= menu.left || box.top >= menu.bottom || box.bottom <= menu.top,
    };
  })()`);
  const tooOld = player.age == null || player.age > 21;
  report.check(
    "its menu lists the player's three recipes, the wonderkid dimmed and saying why for a player older than 21",
    JSON.stringify(items.map(([label]) => label)) === JSON.stringify(["Make a wonderkid", "Injury-proof", "Extend his career"]) &&
      (items[0][1] === "true") === tooOld && /21 or younger/.test(note.text),
    JSON.stringify(items),
  );
  report.check(
    "what a recipe does is in a narrow box beside the menu, within the window",
    note.described && note.width <= 248 && note.inside && note.beside && note.text.length < 120,
    JSON.stringify(note),
  );
  await app.shot("recipes-menu");

  // Injury-proof from the menu: the dialog works it out at once and lists it; Close gives focus back.
  await app.click(menuItem("Injury-proof"), "Injury-proof");
  await app.waitFor(`${DIALOG}?.querySelector(".mass-close:not([disabled])") && ${DIALOG}.querySelector(".mass-summary").textContent`, "the preview");
  const planned = await app.must("plan_recipe", { recipe: "injuryProof", uid: player.uid });
  const shown = await app.js(`({
    title: ${DIALOG}.querySelector("h2").textContent,
    subject: ${DIALOG}.querySelector(".mass-scope").textContent,
    note: ${DIALOG}.querySelector(".mass-scope-note").textContent.length > 20,
    summary: ${DIALOG}.querySelector(".mass-summary").textContent,
    rows: ${DIALOG}.querySelectorAll(".mass-changes tbody tr").length,
    applyOff: ${DIALOG}.querySelector(".mass-apply")?.disabled,
    undo: !!${DIALOG}.querySelector(".mass-undo"),
  })`);
  report.check(
    "the dialog names the recipe and the player, says what it does, and lists every change of the preview",
    shown.title === "Injury-proof" && shown.subject === player.name && shown.note && shown.rows === planned.changes &&
      shown.applyOff === (planned.changes === 0) && !shown.undo,
    JSON.stringify(shown),
  );
  await app.shot("recipes-dialog");
  await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
  await app.waitFor(`!${DIALOG}`, "the dialog closed");
  report.check("closing the dialog gives focus back to Recipes", await app.js(`document.activeElement === ${toggle}`));

  // The command palette finds the recipes of the profile on show once something is typed, and lists none before.
  const options = `[...document.querySelectorAll(".palette-option")].map((o) => o.querySelector(".palette-name")?.textContent ?? "")`;
  await app.chord("k");
  await app.waitFor(`document.querySelector(".palette input")`, "the palette");
  const listed = await app.js(options);
  await app.setInput(`document.querySelector(".palette input")`, "recipe");
  await sleep(600);
  const found = (await app.js(options)).filter((label) => label.startsWith("Recipe: "));
  report.check(
    "the palette lists no recipe until something is typed, then the ones this player can take",
    listed.length > 0 && listed.every((label) => !label.startsWith("Recipe: ")) && found.length === (tooOld ? 2 : 3),
    found.join(" | "),
  );
  await app.click(
    `[...document.querySelectorAll(".palette-option")].find((o) => o.textContent.includes("Extend his career"))`,
    "the recipe in the palette",
  );
  await app.waitFor(`!document.querySelector(".palette") && ${DIALOG}?.querySelector("h2")?.textContent === "Extend his career"`, "the recipe's dialog");
  report.check("picking one there opens its dialog", true);
  await app.waitFor(`${DIALOG}.querySelector(".mass-close:not([disabled])")`, "the preview");
  await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
  await app.waitFor(`!${DIALOG}`, "the dialog closed");
  await app.press("Escape");

  // A club's profile: Recipes in its header, before Edit, with the club's two.
  await app.openClub(club.name, club.uid);
  const head = `${VIS}(".profile:not(.popup) .profile-actions")`;
  await app.waitFor(`${head}?.querySelector(".recipe-toggle:not([disabled])")`, "the club's Recipes button");
  const actions = await app.js(`[...${head}.querySelectorAll("button")].map((b) => b.textContent.trim() || b.getAttribute("aria-label"))`);
  report.check("a club's profile has Recipes in its header, before Edit", actions[0] === "Recipes" && actions[1] === "Edit", actions.join(" | "));
  await app.click(`${head}.querySelector(".recipe-toggle")`, "Recipes");
  await app.waitFor(`${MENU}.length > 0`, "the club's Recipes menu");
  const clubItems = await app.js(`${MENU}.map((i) => i.textContent)`);
  report.check(
    "its menu lists the club's two recipes",
    JSON.stringify(clubItems) === JSON.stringify(["Make the club rich", "Top facilities"]),
    clubItems.join(" | "),
  );
  await app.click(menuItem("Make the club rich"), "Make the club rich");
  await app.waitFor(`${DIALOG}?.querySelector(".mass-close:not([disabled])") && ${DIALOG}.querySelector(".mass-summary").textContent`, "the club's preview");
  const rich = await app.js(`({
    subject: ${DIALOG}.querySelector(".mass-scope").textContent,
    rows: [...${DIALOG}.querySelectorAll(".mass-changes tbody tr")].map((tr) => tr.cells[0].textContent),
  })`);
  report.check(
    "the club's dialog names it, and lists only its money",
    rich.subject === club.name && rich.rows.every((label) => /budget|Balance/i.test(label)),
    JSON.stringify(rich),
  );
  await app.shot("recipes-club");
  await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
  await app.waitFor(`!${DIALOG}`, "the dialog closed");
  // While the club is edited its recipes are out of reach, like its Squad tab.
  await app.click(`${head}.querySelector("button.edit-toggle")`, "Edit");
  await app.waitFor(`${head} && !${head}.querySelector(".recipe-toggle")`, "Recipes gone while the club is edited");
  report.check("while the club is edited, Recipes is not offered", true);
  await app.click(`${head}.querySelector("button.edit-toggle")`, "Done");
  await app.waitFor(`${head}?.querySelector(".recipe-toggle")`, "Recipes back");
  await app.press("Escape");
}
