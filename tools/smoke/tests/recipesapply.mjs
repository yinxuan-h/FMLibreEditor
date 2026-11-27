// Recipes applied: each of the five worked out for its witness (see `recipes`, which picks them and checks the
// previews), written as one batch and undone as one. For each: Apply writes every change the preview showed, the game
// then holds the new values (as the app and the probe read them) and no other value of the player or club changed,
// his injuries are healed and his retirement is off the game's plans where those were part of it, the changes are one
// batch in the change log, a second preview has nothing left to write, the preview that was applied cannot be applied
// again, and one undo of the batch puts every value back, the injuries and the planned retirement with them. Then the
// dialog: Apply with a preview that another one replaced writes nothing and says so, Apply writes the recipe, and Undo
// there takes it back.
import { SHOWN, VIS } from "../lib/app.mjs";
import { DIALOG, differences, menuItem, previewOf, witnesses } from "./recipes.mjs";

export const name = "recipesapply";
export const writes = true;

/** Applies the recipe previewed for `subject`, checks what was written, and undoes it. */
async function roundTrip({ app, report, probe }, recipe, entity, subject) {
  const uid = subject.uid;
  const values = () => app.must("read_values", { entity, uid });
  const details = () => (entity === "player" ? app.must("player_details", { uid }) : null);
  const person = await details();
  const { preview, before } = await previewOf({ app, report }, recipe, entity, subject);
  if (preview.changes === 0) {
    report.skip(`${recipe}: applies and undoes`, `${subject.name} already has everything it gives`);
    return;
  }

  const logBefore = (await app.must("change_log")).length;
  const done = await app.must("apply_recipe", { id: preview.id });
  report.check(
    `${recipe}: Apply writes every change of the preview`,
    done.applied === preview.changes && done.failed.length === 0,
    `${done.applied} of ${preview.changes}` + (done.failed.length ? `; ${JSON.stringify(done.failed[0])}` : ""),
  );
  const after = await values();
  const written = new Set(preview.rows.map((row) => row.field));
  const missed = preview.rows.filter((row) => after[row.field] !== row.new).map((row) => `${row.field}: ${after[row.field]}`);
  // Keeping a player from retiring gives him one trait in place of another: the two are its own.
  const own = preview.keepsPlaying != null ? new Set([...written, "trait.48", "trait.49"]) : written;
  const others = differences(before, after, own);
  report.check(
    `${recipe}: the game holds the new values, and no other value of ${subject.name} changed`,
    missed.length === 0 && others.length === 0,
    [...missed, ...others].slice(0, 5).join("; "),
  );
  // The probe reads the game on its own: the first values, as stored.
  const sample = preview.rows.slice(0, 6);
  const probed = (side) => {
    const read = probe.get(entity, uid, sample.map((row) => row.field));
    return sample.filter((row) => read[row.field] !== row[side]).map((row) => `${row.field}: ${read[row.field]}`);
  };
  if (sample.length) {
    const off = probed("new");
    report.check(`${recipe}: the probe reads them too`, off.length === 0, off.join("; ") || sample.map((row) => row.field).join(", "));
  }
  if (preview.heals != null) report.check(`${recipe}: his injuries are healed`, (await details()).injuries.length === 0);
  if (preview.keepsPlaying != null) report.check(`${recipe}: the game plans no retirement for him`, (await details()).retires == null);
  const log = await app.must("change_log");
  const logged = log.filter((change) => change.batch === done.batch);
  report.check(
    `${recipe}: the changes are one batch in the log`,
    logged.length === done.applied && log.length === logBefore + done.applied,
    `${logged.length} changes in batch ${done.batch}`,
  );
  const again = await app.must("plan_recipe", { recipe, uid });
  report.check(`${recipe}: a second preview has nothing left to write`, again.changes === 0, `${again.changes} changes`);
  const twice = await app.invoke("apply_recipe", { id: preview.id });
  report.check(
    `${recipe}: the preview that was applied cannot be applied again`,
    !twice.ok && twice.error.code === "recipe.outOfDate",
    JSON.stringify(twice.error ?? twice.value),
  );

  const undone = await app.must("undo_batch", { batch: done.batch });
  const back = differences(before, await values());
  report.check(
    `${recipe}: one undo of the batch puts every value back`,
    undone.undone === done.applied && undone.failed.length === 0 && back.length === 0,
    `${undone.undone} undone` + (back.length ? `; ${back.slice(0, 5).join("; ")}` : "") +
      (undone.failed.length ? `; ${JSON.stringify(undone.failed[0])}` : ""),
  );
  if (sample.length) {
    const off = probed("old");
    report.check(`${recipe}: and the probe reads the old values again`, off.length === 0, off.join("; "));
  }
  if (person) {
    const now = await details();
    report.check(
      `${recipe}: his injuries and the retirement the game plans are as they were`,
      now.injuries.length === person.injuries.length && JSON.stringify(now.retires) === JSON.stringify(person.retires),
      `${now.injuries.length} injuries, retires ${JSON.stringify(now.retires)}`,
    );
  }
}

export async function run({ app, report, probe }) {
  const { young, old, runs } = await witnesses(app, probe);
  for (const [recipe, entity, subject] of runs) {
    await roundTrip({ app, report, probe }, recipe, entity, subject);
  }

  // The dialog, from the Recipes button of a player's profile.
  const player = old ?? young;
  if (!player) return;
  const values = () => app.must("read_values", { entity: "player", uid: player.uid });
  const before = await values();
  await app.openTab("Players");
  await app.setInput(`${VIS}(".search input")`, String(player.uid));
  const rows = `[...document.querySelectorAll("table.grid tbody tr")].filter(${SHOWN})`;
  await app.waitFor(`${rows}.length === 1 && ${rows}[0].textContent.includes(${JSON.stringify(player.name)})`, "his row");
  await app.click(`${rows}[0]`, "his row");
  const profile = `${VIS}(".profile:not(.popup)")`;
  await app.waitFor(`${profile} && ${profile}.getAttribute("aria-busy") === "false"`, "his profile", 30000);
  const toggle = `${profile}.querySelector(".attribute-buttons .recipe-toggle")`;
  const open = async () => {
    await app.waitFor(`${toggle} && !${toggle}.disabled`, "the Recipes button");
    await app.click(toggle, "Recipes");
    await app.waitFor(menuItem("Injury-proof"), "the Recipes menu");
    await app.click(menuItem("Injury-proof"), "Injury-proof");
    await app.waitFor(`${DIALOG}?.querySelector(".mass-close:not([disabled])") && ${DIALOG}.querySelector(".mass-summary").textContent`, "the preview");
  };
  await open();
  const planned = await app.must("plan_recipe", { recipe: "injuryProof", uid: player.uid });
  if (planned.changes === 0) {
    report.skip("Apply and Undo in the dialog", `${player.name} is injury-proof already`);
    await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
    await app.press("Escape");
    return;
  }
  // The preview the dialog holds was replaced by the one planned just now: Apply says so, and nothing is written.
  await app.click(`${DIALOG}.querySelector(".mass-apply")`, "Apply");
  await app.waitFor(`${DIALOG}.querySelector(".mass-error")`, "the dialog saying its preview is out of date");
  const untouched = differences(before, await values());
  report.check(
    "Apply with a preview that another one replaced writes nothing, and says so",
    untouched.length === 0 && (await app.js(`!${DIALOG}.querySelector(".mass-apply")`)),
    untouched.slice(0, 3).join("; "),
  );
  await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
  await app.waitFor(`!${DIALOG}`, "the dialog closed");

  await open();
  await app.click(`${DIALOG}.querySelector(".mass-apply")`, "Apply");
  await app.waitFor(`/Applied/.test(${DIALOG}.querySelector(".mass-result")?.textContent ?? "")`, "the outcome");
  const after = await values();
  const missed = planned.rows.filter((row) => after[row.field] !== row.new).map((row) => row.field);
  report.check("Apply in the dialog writes the recipe", missed.length === 0, missed.join(", "));
  await app.shot("recipes-applied");
  await app.click(`${DIALOG}.querySelector(".mass-undo")`, "Undo this recipe");
  await app.waitFor(`/Undid/.test(${DIALOG}.querySelector(".mass-result")?.textContent ?? "")`, "the undo's outcome");
  const back = differences(before, await values());
  report.check("and Undo there puts every value back", back.length === 0, back.slice(0, 5).join("; "));
  await app.click(`${DIALOG}.querySelector(".mass-close")`, "Close");
  await app.waitFor(`!${DIALOG}`, "the dialog closed");
  await app.press("Escape");
}
