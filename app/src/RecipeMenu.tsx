// Recipes: ready-made sets of changes to one player or one club, such as "Make a wonderkid". The Recipes button sits
// beside Max attributes in a player's profile and in the header of a club's, and opens a menu of them; the command
// palette lists the recipes of the profile on show too. Picking one opens a dialog that says exactly what it would
// write, like the preview of a mass edit, applies it as one batch, and can undo it.

import { useContext, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type RefObject } from "react";
import { createPortal } from "react-dom";
import { valueText } from "./editDraft";
import { applyRecipe, changeLog, isStoredFiner, planRecipe, recipes, shownValue, undoBatch } from "./editing";
import { fieldTable } from "./fieldTables";
import { describeError, formatDate, formatNumber } from "./format";
import { changesText, fieldLabel, massEditText, profileText, recipeText as text, valueChange } from "./labels";
import { undoFailureLines } from "./massDraft";
import { TeamActionMenu, type MenuAction } from "./TeamActionMenu";
import type { EditField, LoggedChange, MassReport, RecipeInfo, RecipePreview } from "./types";
import { useModal } from "./useModal";
import { EditorContext } from "./useProfileEditing";

/** The name of a recipe by its key, and the sentence that says what it does; a key the dictionary lacks is shown. */
const recipeName = (key: string) => (text.names as Partial<Record<string, string>>)[key] ?? key;
const recipeNote = (key: string) => (text.notes as Partial<Record<string, string>>)[key] ?? "";

/** What the command palette reaches of the Recipes button on show (see `App`). */
export interface RecipeHost {
  /** The Recipes button; null while it is not on the page. */
  button: () => HTMLButtonElement | null;
  /** The recipes it offers now, by key; one that is not for this player is left out. */
  keys: readonly string[];
  /** Opens the dialog of the recipe `key`. */
  open: (key: string) => void;
}

/** The Recipes buttons on the page, the one mounted last at the end. */
const hosts: RefObject<RecipeHost | null>[] = [];

/**
 * The Recipes button the command palette speaks for: the last one mounted that can be pressed and is in the view on
 * show (a profile left open in another tab stays on the page, hidden). Null when there is none.
 */
export function recipesOnShow(): RecipeHost | null {
  for (let i = hosts.length - 1; i >= 0; i--) {
    const host = hosts[i].current;
    const button = host?.button();
    if (host && button && !button.disabled && button.closest(".workspace.inactive") === null) {
      return host;
    }
  }
  return null;
}

/** The palette's name for a recipe: "Recipe: Make a wonderkid". */
export const recipeCommand = (key: string) => text.command(recipeName(key));

interface RecipeMenuProps {
  entity: "player" | "club";
  uid: number;
  name: string;
  /** A player's age, which some recipes go by; null when it is not known, and for a club. */
  age?: number | null;
}

/**
 * The Recipes button and its menu, and the dialog of the recipe picked. Nothing at all until the app has said which
 * recipes there are for this kind of thing. A recipe that is not for this player (a wonderkid older than 21) is
 * dimmed, and its hover text says whom it is for.
 */
export function RecipeMenu({ entity, uid, name, age = null }: RecipeMenuProps) {
  const { editable } = useContext(EditorContext);
  const [offered, setOffered] = useState<RecipeInfo[]>([]);
  const [open, setOpen] = useState<RecipeInfo | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const wasOpen = useRef(false);

  useEffect(() => {
    let current = true;
    recipes()
      .then((all) => current && setOffered(all.filter((recipe) => recipe.entity === entity)))
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [entity]);

  // Once the dialog is gone, focus goes back to the button that opened it.
  useEffect(() => {
    if (wasOpen.current && open === null) {
      button.current?.focus();
    }
    wasOpen.current = open !== null;
  }, [open]);

  const tooOld = (recipe: RecipeInfo) => recipe.oldest !== null && (age === null || age > recipe.oldest);
  const actions: MenuAction<string>[] = offered.map((recipe) => ({
    action: recipe.key,
    label: recipeName(recipe.key),
    title: tooOld(recipe) && recipe.oldest !== null ? text.tooOld(recipe.oldest) : recipeNote(recipe.key),
    waiting: tooOld(recipe),
    dialog: true,
  }));
  const pick = (key: string) => setOpen(offered.find((recipe) => recipe.key === key) ?? null);

  // What the command palette reaches of this button, kept up to date as soon as a render is on screen.
  const host = useRef<RecipeHost | null>(null);
  useLayoutEffect(() => {
    host.current = {
      button: () => button.current,
      keys: open === null ? offered.filter((recipe) => !tooOld(recipe)).map((recipe) => recipe.key) : [],
      open: pick,
    };
  });
  useEffect(() => {
    hosts.push(host);
    return () => {
      hosts.splice(hosts.indexOf(host), 1);
    };
  }, []);

  if (offered.length === 0) {
    return null;
  }
  return (
    <>
      <TeamActionMenu
        actions={actions}
        editable={editable}
        busy={false}
        onPick={pick}
        buttonRef={button}
        label={text.button}
        title={text.buttonTitle}
        className="max-toggle squad-more recipe-toggle"
      />
      {open && <RecipeDialog recipe={open} uid={uid} name={name} onClose={() => setOpen(null)} />}
    </>
  );
}

/** What a call is under way for, while one is. */
type Busy = "preview" | "apply" | "undo";

/** What the dialog says about the undo: how many changes were taken back, and the ones that could not be. */
interface UndoOutcome {
  undone: number;
  problems: string[];
}

interface RecipeDialogProps {
  recipe: RecipeInfo;
  uid: number;
  /** The player or the club the recipe is for. */
  name: string;
  onClose: () => void;
}

/**
 * The dialog of a recipe: it works the recipe out as soon as it opens and lists what it would write, value by value,
 * with what stays because it is as good already and what is left alone and why. Apply writes it as one batch; Undo
 * takes the batch back. Laid out as the preview of a mass edit is.
 */
function RecipeDialog({ recipe, uid, name, onClose }: RecipeDialogProps) {
  const { onEdited } = useContext(EditorContext);
  const { entity, key } = recipe;
  const titleId = useId();
  const subjectId = useId();
  const dialog = useRef<HTMLDivElement>(null);
  const result = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  const running = useRef(false);

  const [fields, setFields] = useState<EditField[]>([]);
  const [preview, setPreview] = useState<RecipePreview | null>(null);
  const [busy, setBusy] = useState<Busy | null>("preview");
  const [report, setReport] = useState<MassReport | null>(null);
  const [undone, setUndone] = useState<UndoOutcome | null>(null);
  const [error, setError] = useState<string | null>(null);

  const byKey = useMemo(() => new Map(fields.map((field) => [field.key, field] as const)), [fields]);
  const applied = report !== null;

  useModal(dialog, busy === null, onClose);

  // The recipe is worked out as soon as the dialog opens: there is nothing to fill in first.
  useEffect(() => {
    alive.current = true;
    dialog.current?.focus();
    Promise.all([fieldTable(entity), planRecipe(key, uid)]).then(
      ([table, planned]) => {
        if (alive.current) {
          setFields(table);
          setPreview(planned);
          setBusy(null);
        }
      },
      (e) => {
        if (alive.current) {
          setError(describeError(e));
          setBusy(null);
        }
      },
    );
    return () => {
      alive.current = false;
    };
  }, [entity, key, uid]);

  // A disabled control cannot keep focus: when a call is over, focus that was lost with it goes to the outcome, or
  // back to the dialog.
  const wasBusy = useRef(false);
  useEffect(() => {
    const was = wasBusy.current;
    wasBusy.current = busy !== null;
    if (!was || busy !== null) {
      return;
    }
    const box = dialog.current;
    const active = document.activeElement;
    if (box && active instanceof HTMLElement && box.contains(active) && !active.matches(":disabled")) {
      return;
    }
    (applied ? result.current : box)?.focus();
  }, [busy, applied]);

  /** Runs one call at a time; what it throws is shown. */
  const run = async (what: Busy, task: () => Promise<void>) => {
    if (running.current) {
      return;
    }
    running.current = true;
    setBusy(what);
    setError(null);
    try {
      await task();
    } catch (e) {
      if (alive.current) {
        setError(describeError(e));
      }
    } finally {
      running.current = false;
      if (alive.current) {
        setBusy(null);
      }
    }
  };

  const apply = () =>
    preview !== null &&
    preview.changes > 0 &&
    run("apply", async () => {
      try {
        const done = await applyRecipe(preview.id);
        // The game has changed: the lists, the profile and the change log read it again.
        if (done.applied > 0) {
          onEdited();
        }
        if (alive.current) {
          setReport(done);
          setUndone(null);
        }
      } catch (e) {
        // The plan is gone or out of date: what is listed is no longer what Apply would write, so it goes, and the
        // recipe is opened again for a new one.
        if (alive.current) {
          setPreview(null);
        }
        throw e;
      }
    });

  const undo = () =>
    report !== null &&
    run("undo", async () => {
      const done = await undoBatch(report.batch);
      if (done.undone > 0) {
        onEdited();
      }
      // The failures only have the numbers of the changes: the log names them.
      let log: LoggedChange[] = [];
      if (done.failed.length > 0) {
        try {
          log = await changeLog();
        } catch {
          // They are then told by number.
        }
      }
      if (alive.current) {
        setUndone({ undone: done.undone, problems: undoFailureLines(done.failed, log) });
      }
    });

  const shown = (field: EditField | undefined, stored: number) =>
    valueText(field, field ? shownValue(field, stored) : stored);
  const left = preview === null ? 0 : preview.skipped.length + preview.left.length;
  const canUndo = report !== null && report.applied > 0 && (undone === null || undone.problems.length > 0);
  const applyTitle =
    preview === null || preview.changes === 0 ? massEditText.applyNothing : massEditText.applyTitle(preview.changes);
  const stepField = { heal: "injuries", keepPlaying: "retirement" } as const;

  return createPortal(
    // A click beside the dialog does nothing, and keeps focus where it is.
    <div
      className="modal-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          event.preventDefault();
        }
      }}
    >
      <div
        ref={dialog}
        className="modal mass-dialog recipe-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={subjectId}
        aria-busy={busy !== null || undefined}
        tabIndex={-1}
      >
        <header className="modal-head">
          <h2 id={titleId}>{recipeName(key)}</h2>
          <p id={subjectId} className="mass-scope">
            {name}
          </p>
          <p className="mass-scope-note">{recipeNote(key)}</p>
        </header>

        <div className="modal-body">
          {busy === "preview" && (
            <p className="muted" role="status">
              {text.working}
            </p>
          )}
          {/* A live region that is always there, so that it is read out when the preview fills it. */}
          <p className="mass-summary" role="status">
            {preview ? text.summary(preview.changes, preview.kept.length, left) : ""}
          </p>
          {preview !== null && preview.changes > 0 && (
            <div className="mass-listing">
              <div className="mass-table-wrap" role="region" aria-label={text.tableLabel} tabIndex={0}>
                <table className="grid mass-grid mass-changes recipe-changes">
                  <thead>
                    <tr>
                      <th>{massEditText.columns.field}</th>
                      <th>{massEditText.columns.change}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {/* The steps on the game's own lists first, as they are written first. */}
                    {preview.heals !== null && (
                      <tr>
                        <td>{fieldLabel("player", "injuries")}</td>
                        <td className="change-value">{valueChange(formatNumber(preview.heals), formatNumber(0))}</td>
                      </tr>
                    )}
                    {preview.keepsPlaying !== null && (
                      <tr>
                        <td>{fieldLabel("player", "retirement")}</td>
                        <td className="change-value">
                          {preview.keepsPlaying.retires === null
                            ? changesText.keptPlaying(null)
                            : valueChange(formatDate(preview.keepsPlaying.retires), profileText.none)}
                        </td>
                      </tr>
                    )}
                    {preview.rows.map((row) => {
                      const field = byKey.get(row.field);
                      return (
                        <tr key={row.field}>
                          <td>{fieldLabel(entity, row.field)}</td>
                          {/* An attribute is stored on a finer scale than it is shown: the hover text has the stored
                              values. */}
                          <td
                            className="change-value"
                            title={isStoredFiner(field) ? changesText.stored(valueChange(row.old, row.new)) : undefined}
                          >
                            {valueChange(shown(field, row.old), shown(field, row.new))}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          )}
          {preview !== null && preview.kept.length > 0 && (
            <details className="mass-skipped">
              <summary>{text.keptHeading(preview.kept.length)}</summary>
              <div className="mass-table-wrap" role="region" aria-label={text.keptLabel} tabIndex={0}>
                <table className="grid mass-grid recipe-changes">
                  <thead>
                    <tr>
                      <th>{massEditText.columns.field}</th>
                      <th>{text.valueColumn}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.kept.map((kept) => (
                      <tr key={kept.field}>
                        <td>{fieldLabel(entity, kept.field)}</td>
                        <td className="change-value">{shown(byKey.get(kept.field), kept.value)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
          )}
          {preview !== null && left > 0 && (
            // Open from the start: there are few of them, and why a step does nothing is what the reader wants to know.
            <details className="mass-skipped" open>
              <summary>{massEditText.skippedHeading(left)}</summary>
              <div className="mass-table-wrap" role="region" aria-label={text.leftLabel} tabIndex={0}>
                <table className="grid mass-grid recipe-changes">
                  <thead>
                    <tr>
                      <th>{massEditText.columns.field}</th>
                      <th>{massEditText.columns.reason}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.left.map((step) => (
                      <tr key={step.step}>
                        <td>{fieldLabel("player", stepField[step.step])}</td>
                        <td className="reason">
                          {describeError({ code: step.code, params: step.params, message: step.reason })}
                        </td>
                      </tr>
                    ))}
                    {preview.skipped.map((skipped) => (
                      <tr key={skipped.field}>
                        <td>{fieldLabel(entity, skipped.field)}</td>
                        <td className="reason">
                          {describeError({ code: skipped.code, params: skipped.params, message: skipped.reason })}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </details>
          )}
        </div>

        {/* What the last Apply or Undo did: outside the scrolling body, so that it is always in view. */}
        <div
          ref={result}
          className="mass-result"
          role="status"
          aria-live="polite"
          tabIndex={applied || undone !== null ? 0 : -1}
        >
          {report !== null && (
            <>
              <p>{massEditText.applied(report.applied)}</p>
              <Problems
                lines={report.failed.map((failed) => `${fieldLabel(entity, failed.field)}: ${describeError(failed.error)}`)}
              />
            </>
          )}
          {undone !== null && (
            <>
              <p>{massEditText.undone(undone.undone)}</p>
              <Problems lines={undone.problems} />
            </>
          )}
        </div>
        {error !== null && (
          <div className="banner mass-error" role="alert">
            {error}
          </div>
        )}

        <footer className="modal-foot">
          <span className="mass-foot-note">{applied || preview === null ? "" : massEditText.hint}</span>
          {canUndo && (
            <button
              type="button"
              className="mass-undo"
              disabled={busy !== null}
              data-tip={text.undoTitle}
              onClick={() => void undo()}
            >
              {busy === "undo" ? massEditText.undoing : text.undo}
            </button>
          )}
          {!applied && preview !== null && (
            <button
              type="button"
              className="primary mass-apply"
              disabled={preview.changes === 0 || busy !== null}
              data-tip={applyTitle}
              onClick={() => void apply()}
            >
              {busy === "apply" ? massEditText.applying : massEditText.apply}
            </button>
          )}
          <button type="button" className="mass-close" disabled={busy !== null} onClick={onClose}>
            {massEditText.close}
          </button>
        </footer>
      </div>
    </div>,
    document.body,
  );
}

/** Lines of what went wrong; nothing when there are none. */
function Problems({ lines }: { lines: string[] }) {
  if (lines.length === 0) {
    return null;
  }
  return (
    <ul className="edit-problems mass-problems">
      {lines.map((line, index) => (
        <li key={index}>{line}</li>
      ))}
    </ul>
  );
}
