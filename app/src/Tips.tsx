// The notes of the controls: what a button does, what a number means, why a control waits. An element says its note in
// `data-tip`, and the one `Tips` of the app shows it in a small box while the pointer rests on the element or the keys
// have reached it (`:focus-visible`): narrow, so that it wraps into a few short lines and fits a small window, where
// the browser's own hover text (`title`) would be one long strip. A few words, and what only repeats a name or a
// number, can stay a `title`.
//
// The box hangs under its element (over it at the foot of the window), near the pointer where the element is wide. Inside
// an element marked `data-tip-beside`, a menu, it hangs beside that element instead, level with the item, so that it
// covers none of the other items. While it shows it describes the element (`aria-describedby`): the control the keys are
// on where the note is that of a label around it. It is not in the way: the pointer goes through it, and it takes no
// focus.

import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

/** The room between a note and its element, and between a note and the edge of the window. */
const GAP = 4;

/** How long the pointer rests on an element before its note shows, in milliseconds. */
const REST = 400;

/** How long after a note went the next one shows at once: the pointer is moving along a row of controls. */
const WARM = 300;

/** How far under the pointer a note begins on an element that is taller than a line or two. */
const UNDER_POINTER = 20;

interface Shown {
  anchor: HTMLElement;
  note: string;
  /** Whether the keys brought it, not the pointer. */
  byKeys: boolean;
}

/** The element whose note `target` shows: itself, or the nearest element around it that has one. */
function anchorOf(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? target.closest<HTMLElement>("[data-tip]") : null;
}

/** The note of `anchor` as it is now; none once it has left the page or lost its note. */
function noteOf(anchor: HTMLElement | null): string {
  return anchor?.isConnected ? (anchor.dataset.tip ?? "").trim() : "";
}

/** Shows the notes of the app's controls. It is mounted once, by `App`. */
export function Tips() {
  const id = useId();
  const [shown, setShown] = useState<Shown | null>(null);
  const box = useRef<HTMLDivElement>(null);
  // Where the pointer was last, for a note on a wide or tall element.
  const pointer = useRef({ x: 0, y: 0 });
  // Lets go of the note that shows: a scroll that carried its element out of the window calls it.
  const drop = useRef<() => void>(() => {});

  useEffect(() => {
    // The element the pointer has rested on, the one it is resting on, and the one the keys are on.
    let hovered: HTMLElement | null = null;
    let resting: HTMLElement | null = null;
    let focused: HTMLElement | null = null;
    // The element a press or a key has put the pointer's note away for, until the pointer leaves it.
    let quiet: HTMLElement | null = null;
    // Whether the pointer was pressed last, not a key: focus that follows is the press's.
    let pressed = false;
    let latest: "pointer" | "keys" = "pointer";
    let showing: HTMLElement | null = null;
    let wentAt = -WARM;
    let timer: number | undefined;

    // A note follows its element: it goes when the element does, and changes when the element's does.
    const watcher = new MutationObserver(() => sync());

    const sync = () => {
      if (!noteOf(hovered)) {
        hovered = null;
      }
      if (!noteOf(focused)) {
        focused = null;
      }
      // The note is that of whichever moved last, the pointer or the keys, and the other's while that one has none.
      const byKeys = latest === "pointer" ? hovered === null : focused !== null;
      const anchor = byKeys ? focused : hovered;
      if (anchor === null && showing !== null) {
        wentAt = performance.now();
      }
      if ((anchor === null) !== (showing === null)) {
        if (anchor === null) {
          watcher.disconnect();
        } else {
          watcher.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["data-tip"] });
        }
      }
      showing = anchor;
      const note = noteOf(anchor);
      setShown((now) =>
        anchor === null
          ? null
          : now && now.anchor === anchor && now.note === note && now.byKeys === byKeys
            ? now
            : { anchor, note, byKeys },
      );
    };

    const forgetPointer = () => {
      window.clearTimeout(timer);
      resting = null;
      hovered = null;
    };

    const over = (event: PointerEvent) => {
      if (event.pointerType === "touch") {
        return;
      }
      pointer.current = { x: event.clientX, y: event.clientY };
      const anchor = anchorOf(event.target);
      if (anchor !== quiet) {
        quiet = null;
      }
      if (anchor === (resting ?? hovered)) {
        return;
      }
      forgetPointer();
      if (anchor === null || anchor === quiet || !noteOf(anchor)) {
        sync();
        return;
      }
      // A menu's notes show at once, and so does the next note of a row the pointer moves along.
      if (anchor.closest("[data-tip-beside]") || showing !== null || performance.now() - wentAt < WARM) {
        hovered = anchor;
        latest = "pointer";
        sync();
        return;
      }
      resting = anchor;
      sync();
      timer = window.setTimeout(() => {
        hovered = resting;
        resting = null;
        latest = "pointer";
        sync();
      }, REST);
    };

    const move = (event: PointerEvent) => {
      pointer.current = { x: event.clientX, y: event.clientY };
    };

    // The pointer left the window.
    const out = (event: PointerEvent) => {
      if (event.relatedTarget === null) {
        quiet = null;
        forgetPointer();
        sync();
      }
    };

    // A press, a key or a scroll puts the pointer's note away, as the browser does with its own.
    const hush = () => {
      quiet = resting ?? hovered ?? quiet;
      forgetPointer();
    };

    // A press and a key put the keys' note away too: the control is being used. A key that moves focus brings the
    // next one.
    const down = () => {
      pressed = true;
      hush();
      focused = null;
      sync();
    };

    const key = (event: KeyboardEvent) => {
      if (event.key === "Shift" || event.key === "Control" || event.key === "Alt" || event.key === "Meta") {
        return;
      }
      pressed = false;
      hush();
      focused = null;
      sync();
    };

    // Only focus the keys moved shows a note: a click focuses its button too, and a field it is typed in.
    const focusIn = (event: FocusEvent) => {
      const target = event.target;
      focused = !pressed && target instanceof Element && target.matches(":focus-visible") ? anchorOf(target) : null;
      if (focused !== null) {
        latest = "keys";
      }
      sync();
    };

    const focusOut = () => {
      focused = null;
      sync();
    };

    const scrolled = () => {
      if (hovered !== null || resting !== null) {
        hush();
        sync();
      }
    };

    drop.current = () => {
      forgetPointer();
      focused = null;
      sync();
    };

    document.addEventListener("pointerover", over, true);
    document.addEventListener("pointermove", move, { capture: true, passive: true });
    document.addEventListener("pointerout", out, true);
    document.addEventListener("pointerdown", down, true);
    window.addEventListener("keydown", key, true);
    document.addEventListener("focusin", focusIn);
    document.addEventListener("focusout", focusOut);
    window.addEventListener("scroll", scrolled, true);
    return () => {
      window.clearTimeout(timer);
      watcher.disconnect();
      document.removeEventListener("pointerover", over, true);
      document.removeEventListener("pointermove", move, true);
      document.removeEventListener("pointerout", out, true);
      document.removeEventListener("pointerdown", down, true);
      window.removeEventListener("keydown", key, true);
      document.removeEventListener("focusin", focusIn);
      document.removeEventListener("focusout", focusOut);
      window.removeEventListener("scroll", scrolled, true);
    };
  }, []);

  // Where the box goes (see the top of the file). It follows a scroll or a resize while the keys hold it there.
  useLayoutEffect(() => {
    const tip = box.current;
    if (shown === null || tip === null) {
      return;
    }
    const { anchor, byKeys } = shown;
    const place = () => {
      const at = anchor.getBoundingClientRect();
      const { width, height } = tip.getBoundingClientRect();
      const right = document.documentElement.clientWidth - GAP;
      const bottom = window.innerHeight - GAP;
      if (at.bottom < 0 || at.top > window.innerHeight || at.right < 0 || at.left > right) {
        drop.current();
        return;
      }
      const beside = anchor.closest<HTMLElement>("[data-tip-beside]")?.getBoundingClientRect();
      let left: number;
      let top: number;
      if (beside) {
        // Right of the menu, left of it where the window has no room there, and under it (over it at the foot of the
        // window) where it has room on neither side.
        left = beside.right + GAP;
        top = at.top;
        if (left + width > right) {
          left = beside.left - GAP - width;
        }
        if (left < GAP) {
          left = beside.left;
          top = beside.bottom + GAP + height <= bottom ? beside.bottom + GAP : beside.top - GAP - height;
        }
      } else {
        const { x, y } = pointer.current;
        // Under a wide element the note keeps near the pointer, and inside the element's width.
        left = byKeys ? at.left : Math.max(at.left, Math.min(x - width / 2, at.right - width));
        top = byKeys ? at.bottom + GAP : Math.min(at.bottom + GAP, y + UNDER_POINTER);
        if (top + height > bottom) {
          const over = byKeys ? at.top - GAP - height : Math.max(at.top - GAP - height, y - UNDER_POINTER - height);
          top = over >= GAP ? over : top;
        }
      }
      tip.style.left = `${Math.round(Math.max(GAP, Math.min(left, right - width)))}px`;
      tip.style.top = `${Math.round(Math.max(GAP, Math.min(top, bottom - height)))}px`;
    };
    place();
    window.addEventListener("scroll", place, true);
    window.addEventListener("resize", place);
    return () => {
      window.removeEventListener("scroll", place, true);
      window.removeEventListener("resize", place);
    };
  }, [shown]);

  // The note describes its element while it shows: the control the keys are on, where the note is its label's.
  const anchor = shown?.anchor ?? null;
  useLayoutEffect(() => {
    if (anchor === null) {
      return;
    }
    const active = document.activeElement;
    const described = active instanceof HTMLElement && anchor.contains(active) ? active : anchor;
    const others = (described.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
    described.setAttribute("aria-describedby", [...others, id].join(" "));
    return () => {
      const rest = (described.getAttribute("aria-describedby") ?? "").split(/\s+/).filter((other) => other && other !== id);
      if (rest.length > 0) {
        described.setAttribute("aria-describedby", rest.join(" "));
      } else {
        described.removeAttribute("aria-describedby");
      }
    };
  }, [anchor, id]);

  return shown === null
    ? null
    : createPortal(
        <div ref={box} id={id} role="tooltip" className="tip">
          {shown.note}
        </div>,
        document.body,
      );
}
