// How close the masthead's parts stand. Its tabs, the Search button, the game's date, Reload and the settings take more
// room in some languages than in others, a count on the Changes tab adds to it, and the Mac sets the scripts Archivo
// has no letters for in a wider font, so the width of the window alone does not tell whether they fit. The masthead is
// measured instead: it stands closer a step at a time (the classes below, see App.css) until its right side, which
// ends with the settings and their languages, no longer runs past the window's edge.

import { useEffect, useLayoutEffect, type RefObject } from "react";

/** The steps, in the order they are taken: each keeps those before it. */
const STEPS = ["tight", "tighter", "markless", "keyless"] as const;

/** Whether the right side of `masthead` runs past its end: the tabs keep their width, and the side gives up its room. */
function overflows(masthead: HTMLElement): boolean {
  const side = masthead.querySelector(".masthead-side");
  return side !== null && side.scrollWidth > side.clientWidth;
}

/** Gives `masthead` the fewest steps with which it fits; all of them when it does not fit even so. */
function fit(masthead: HTMLElement): void {
  masthead.classList.remove(...STEPS);
  for (const step of STEPS) {
    if (!overflows(masthead)) {
      return;
    }
    masthead.classList.add(step);
  }
}

/**
 * Keeps the masthead in `ref` fitted: when what it shows changes (`content`, a list of the same length every time),
 * when the window is resized, and when a font has loaded, as the text is set in another font until then.
 */
export function useMastheadFit(ref: RefObject<HTMLElement | null>, content: readonly unknown[]): void {
  useLayoutEffect(() => {
    if (ref.current) {
      fit(ref.current);
    }
  }, content);

  useEffect(() => {
    const refit = () => {
      if (ref.current) {
        fit(ref.current);
      }
    };
    window.addEventListener("resize", refit);
    document.fonts.addEventListener("loadingdone", refit);
    void document.fonts.ready.then(refit);
    return () => {
      window.removeEventListener("resize", refit);
      document.fonts.removeEventListener("loadingdone", refit);
    };
  }, [ref]);
}
