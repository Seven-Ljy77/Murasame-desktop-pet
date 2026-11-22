"""Remove the baked-in white sticker outline from a Murasame emote PNG.

The source artwork carries a ~10px pure-white outline drawn around the
character. On a dark desktop it reads as a die-cut sticker border, so we strip
it and rebuild a clean antialiased edge.

Pipeline
--------
1. White is "every channel >= 235 with some opacity". The outline plus its
   antialiased fringe matches that; the pale green hair does not.
2. Starting from the white pixels that touch the transparent background, walk
   through white pixels only. Everything reached that way is the stroke and is
   cut out of the alpha channel. White artwork (a shirt, a collar) is simply not
   reachable this way, because the walk would have to cross non-white pixels —
   this is what keeps white clothing intact while still removing the whole
   outline, including the odd chunky corner.
3. Optional (`--repaint-inner`): where hair or clothing chops the stroke into
   enclosed islands, repaint those islands with neighbouring artwork colours
   instead of cutting them, so no see-through slivers appear. It slightly
   thickens hair locks, so it is off by default.
4. The cut silhouette gets a 0.7px alpha blur, and colours are grown outward
   first, keeping the new edge free of white.

Usage:
    python remove_outline.py <input.png> <output.png> [--debug <tint.png>]
                             [--repaint-inner]
"""

from __future__ import annotations

import argparse
from collections import deque

import numpy as np
from PIL import Image, ImageFilter

WHITE_MIN_CHANNEL = 235
# Anything this light is not trusted as a colour source when repainting.
COLOUR_SOURCE_MAX_WHITENESS = 215
INNER_MAX_RADIUS = 9
INNER_MAX_SILHOUETTE_DISTANCE = 60
INNER_MIN_AREA = 150
INNER_MASK_DILATION = 2
EDGE_BLUR_RADIUS = 0.7
COLOUR_PAD_PASSES = 2
MAX_INSCRIBED_RADIUS = 24
MAX_COLOUR_SEARCH = 80


def chamfer_distance(sources: np.ndarray) -> np.ndarray:
    """Chamfer distance from every pixel to the nearest source pixel."""
    height, width = sources.shape
    inf = 10 ** 6
    dist = np.where(sources, 0, inf).astype(np.int32)
    for y in range(1, height):
        dist[y] = np.minimum(dist[y], dist[y - 1] + 1)
    for y in range(height - 2, -1, -1):
        dist[y] = np.minimum(dist[y], dist[y + 1] + 1)
    for x in range(1, width):
        dist[:, x] = np.minimum(dist[:, x], dist[:, x - 1] + 1)
    for x in range(width - 2, -1, -1):
        dist[:, x] = np.minimum(dist[:, x], dist[:, x + 1] + 1)
    return dist


def distance_to_transparent(alpha: np.ndarray) -> np.ndarray:
    """Chamfer distance from every pixel to the nearest fully transparent one."""
    return chamfer_distance(alpha == 0)


def flood_from_border(passable: np.ndarray) -> np.ndarray:
    """Mark every passable pixel reachable from the canvas border."""
    height, width = passable.shape
    visited = np.zeros((height, width), dtype=bool)
    queue: deque[tuple[int, int]] = deque()

    def seed(y: int, x: int) -> None:
        if passable[y, x] and not visited[y, x]:
            visited[y, x] = True
            queue.append((y, x))

    for x in range(width):
        seed(0, x)
        seed(height - 1, x)
    for y in range(height):
        seed(y, 0)
        seed(y, width - 1)

    while queue:
        y, x = queue.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < height and 0 <= nx < width and passable[ny, nx] and not visited[ny, nx]:
                visited[ny, nx] = True
                queue.append((ny, nx))
    return visited


def white_connected_to_silhouette(white: np.ndarray, transparent: np.ndarray) -> np.ndarray:
    """White pixels reachable from the silhouette by walking through white only.

    This is the stroke: the outline band touches the transparent background and
    is itself white, so the walk follows it all the way around. White artwork
    inside the character (a shirt, eye highlights) is not reachable this way
    unless it literally merges into the stroke, and even then only the merged
    part would be affected.
    """
    height, width = white.shape
    padded = np.pad(transparent, 1, constant_values=False)
    seeds = white & (
        padded[:-2, 1:-1] | padded[2:, 1:-1] | padded[1:-1, :-2] | padded[1:-1, 2:]
    )

    visited = seeds.copy()
    queue: deque[tuple[int, int]] = deque(zip(*np.nonzero(seeds)))
    while queue:
        y, x = queue.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < height and 0 <= nx < width and white[ny, nx] and not visited[ny, nx]:
                visited[ny, nx] = True
                queue.append((ny, nx))
    return visited


def inscribed_radius(component: np.ndarray) -> int:
    """How many 4-neighbour erosions a component survives."""
    mask = component
    radius = 0
    while radius < MAX_INSCRIBED_RADIUS:
        eroded = mask.copy()
        eroded[1:, :] &= mask[:-1, :]
        eroded[:-1, :] &= mask[1:, :]
        eroded[:, 1:] &= mask[:, :-1]
        eroded[:, :-1] &= mask[:, 1:]
        eroded[0, :] = False
        eroded[-1, :] = False
        eroded[:, 0] = False
        eroded[:, -1] = False
        if not eroded.any():
            break
        mask = eroded
        radius += 1
    return radius


def white_islands(white: np.ndarray):
    """Yield (component_mask, inscribed_radius) for each connected white blob."""
    height, width = white.shape
    labelled = np.zeros((height, width), dtype=bool)
    for sy in range(height):
        for sx in range(width):
            if not white[sy, sx] or labelled[sy, sx]:
                continue
            queue = deque([(sy, sx)])
            labelled[sy, sx] = True
            pixels: list[tuple[int, int]] = []
            while queue:
                y, x = queue.popleft()
                pixels.append((y, x))
                for dy in (-1, 0, 1):
                    for dx in (-1, 0, 1):
                        ny, nx = y + dy, x + dx
                        if 0 <= ny < height and 0 <= nx < width:
                            if white[ny, nx] and not labelled[ny, nx]:
                                labelled[ny, nx] = True
                                queue.append((ny, nx))
            component = np.zeros((height, width), dtype=bool)
            ys = [p[0] for p in pixels]
            xs = [p[1] for p in pixels]
            component[np.array(ys), np.array(xs)] = True
            yield component, inscribed_radius(component)


def dilate(mask: np.ndarray, passes: int) -> np.ndarray:
    for _ in range(passes):
        grown = mask.copy()
        grown[1:, :] |= mask[:-1, :]
        grown[:-1, :] |= mask[1:, :]
        grown[:, 1:] |= mask[:, :-1]
        grown[:, :-1] |= mask[:, 1:]
        mask = grown
    return mask


def repaint_from_neighbours(
    rgb: np.ndarray,
    sources: np.ndarray,
    hole: np.ndarray,
    search: int = MAX_COLOUR_SEARCH,
) -> np.ndarray:
    """Fill each hole pixel with the nearest trusted colour, preferring the left."""
    height, width = hole.shape
    filled = rgb.copy()
    ys, xs = np.nonzero(hole)

    for y, x in zip(ys.tolist(), xs.tolist()):
        colour = None
        for xx in range(x - 1, max(x - search, -1), -1):
            if sources[y, xx]:
                colour = rgb[y, xx]
                break
        if colour is None:
            for xx in range(x + 1, min(x + search, width)):
                if sources[y, xx]:
                    colour = rgb[y, xx]
                    break
        if colour is None:
            for yy in range(y - 1, max(y - search, -1), -1):
                if sources[yy, x]:
                    colour = rgb[yy, x]
                    break
        if colour is None:
            for yy in range(y + 1, min(y + search, height)):
                if sources[yy, x]:
                    colour = rgb[yy, x]
                    break
        if colour is not None:
            filled[y, x] = colour

    # Feather the seam so the repainted patch does not show a hard border.
    feathered = filled.astype(np.float64)
    for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
        edge = hole & ~np.roll(hole, (dy, dx), axis=(0, 1))
        neighbour = np.roll(filled, (dy, dx), axis=(0, 1)).astype(np.float64)
        feathered[edge] = 0.75 * feathered[edge] + 0.25 * neighbour[edge]
    return feathered


def pad_colours(colour: np.ndarray, known: np.ndarray, passes: int) -> np.ndarray:
    """Grow artwork colours into the transparent margin, one ring per pass."""
    height, width = known.shape
    colour = colour.copy()
    known = known.copy()
    for _ in range(passes):
        accumulator = np.zeros_like(colour, dtype=np.int32)
        counts = np.zeros((height, width), dtype=np.int16)
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            shifted_colour = np.roll(colour, (dy, dx), axis=(0, 1)).astype(np.int32)
            shifted_known = np.roll(known, (dy, dx), axis=(0, 1))
            accumulator += shifted_colour * shifted_known[..., None]
            counts += shifted_known.astype(np.int16)
        fill = (~known) & (counts > 0)
        colour[fill] = (accumulator[fill] / counts[fill][..., None]).astype(np.int16)
        known = known | fill
    return colour


def remove_outline(
    source: Image.Image,
    debug_path: str | None = None,
    fill_inner: bool = False,
) -> tuple[Image.Image, dict]:
    array = np.array(source.convert("RGBA"))
    rgb = array[:, :, :3].astype(np.int16)
    alpha = array[:, :, 3].astype(np.int16)

    white = (rgb.min(axis=2) >= WHITE_MIN_CHANNEL) & (alpha > 0)
    transparent = alpha == 0

    silhouette_distance = distance_to_transparent(alpha)

    # Cut every white pixel the silhouette can reach by walking through white.
    # The stroke qualifies because it touches the background; white artwork
    # inside the character does not, because reaching it would mean walking
    # through the character first.
    reachable = white_connected_to_silhouette(white, transparent)
    outer_ring = reachable

    shape = (alpha > 0) & ~outer_ring

    outside = flood_from_border(~shape)
    sealed = outer_ring & ~outside

    inner = np.zeros(alpha.shape, dtype=bool)
    islands_found = 0
    islands_selected = 0
    if fill_inner:
        # Optional pass: keep the separators opaque and colour them with the
        # surrounding artwork. It avoids see-through slivers, but it also makes
        # hair locks look a little thicker, so it is off by default.
        shape |= sealed
        inner = sealed.copy()
        for component, radius in white_islands(white & ~outer_ring):
            islands_found += 1
            if component.sum() < INNER_MIN_AREA:
                continue
            nearest = int(silhouette_distance[component].min())
            if radius <= INNER_MAX_RADIUS and nearest <= INNER_MAX_SILHOUETTE_DISTANCE:
                inner |= component
                islands_selected += 1

    colour = rgb.copy()
    if fill_inner and inner.any():
        # Stay inside the character so the silhouette itself is left alone.
        hole = (dilate(inner, INNER_MASK_DILATION) | sealed) & shape
        sources = (alpha > 0) & (rgb.min(axis=2) < COLOUR_SOURCE_MAX_WHITENESS) & ~hole
        colour = repaint_from_neighbours(rgb, sources, hole)

    mask = Image.fromarray((shape * 255).astype(np.uint8))
    alpha_new = np.array(mask.filter(ImageFilter.GaussianBlur(EDGE_BLUR_RADIUS))).astype(np.int16)

    colour = pad_colours(colour, shape, COLOUR_PAD_PASSES)
    result = np.dstack(
        [
            np.clip(colour, 0, 255).astype(np.uint8),
            np.clip(alpha_new, 0, 255).astype(np.uint8),
        ]
    )

    if debug_path:
        tinted = array.copy()
        tinted[outer_ring] = (0, 220, 255, 255)
        tinted[inner] = (255, 0, 255, 255)
        Image.fromarray(tinted, "RGBA").save(debug_path)

    stats = {
        "outer_ring_px": int(outer_ring.sum()),
        "inner_islands_found": islands_found,
        "inner_islands_repainted": islands_selected if fill_inner else 0,
    }
    return Image.fromarray(result, "RGBA"), stats


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input")
    parser.add_argument("output")
    parser.add_argument("--debug", default=None)
    parser.add_argument(
        "--repaint-inner",
        action="store_true",
        help="also repaint enclosed separators (makes hair locks look thicker)",
    )
    args = parser.parse_args()

    source = Image.open(args.input)
    result, stats = remove_outline(source, args.debug, args.repaint_inner)
    result.save(args.output)
    print(f"{args.input} -> {args.output} {result.size[0]}x{result.size[1]}")
    for key, value in stats.items():
        print(f"  {key}: {value}")


if __name__ == "__main__":
    main()
