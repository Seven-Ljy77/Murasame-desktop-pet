import e01 from "../assets/pet/01.png";
import e02 from "../assets/pet/02.png";
import e03 from "../assets/pet/03.png";
import e04 from "../assets/pet/04.png";
import e05 from "../assets/pet/05.png";
import e06 from "../assets/pet/06.png";
import e07 from "../assets/pet/07.png";
import e08 from "../assets/pet/08.png";
import e09 from "../assets/pet/09.png";
import e10 from "../assets/pet/10.png";
import e11 from "../assets/pet/11.png";
import e12 from "../assets/pet/12.png";
import e13 from "../assets/pet/13.png";
import e14 from "../assets/pet/14.png";
import e15 from "../assets/pet/15.png";
import e16 from "../assets/pet/16.png";

/** Expression index (matches the file name in assets/pet) to image URL. */
export const EXPRESSION_SOURCES: Readonly<Record<number, string>> = {
  1: e01,
  2: e02,
  3: e03,
  4: e04,
  5: e05,
  6: e06,
  7: e07,
  8: e08,
  9: e09,
  10: e10,
  11: e11,
  12: e12,
  13: e13,
  14: e14,
  15: e15,
  16: e16,
};

let preloaded = false;

/** Warms the browser cache so switching expressions never flashes an empty frame. */
export function preloadExpressions(): void {
  if (preloaded || typeof Image === "undefined") return;
  preloaded = true;
  for (const source of Object.values(EXPRESSION_SOURCES)) {
    const image = new Image();
    image.src = source;
  }
}
