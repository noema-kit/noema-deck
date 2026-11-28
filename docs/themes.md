# Themes

A theme sets Deck's colours and typefaces and decides how the map is drawn. Deck ships two: **Noema**, the default, in light and dark, and **Galaxy**, a night sky. [Design](design.md#settings) says how a person chooses one.

## A theme's folder

Each theme is a folder under `ui/themes/`, named by its ID, and listed in `ui/themes/themes.json`.

| File | Holds |
| --- | --- |
| `theme.json` | The manifest: name, modes, swatch and terminal palettes. |
| `theme.css` | The tokens below, and any typefaces the theme brings. |
| `map.js` | The map's painter. |

### `theme.json`

| Field | Content |
| --- | --- |
| `name`, `description` | Shown in Settings, under Appearance. |
| `modes` | `["light", "dark"]`, or one of them. A theme with one mode keeps to it whatever the person chose. |
| `swatch` | Per mode, four colours for the theme's card: map, plate, a domain and the accent. |
| `terminal` | Per mode, an xterm.js theme: `background`, `foreground`, `cursor`, `selectionBackground` and the sixteen ANSI colours. |

### `theme.css`

Tokens on `:root`, per mode under `:root[data-mode="light"]` and `:root[data-mode="dark"]`. `style.css` names only what each one is for.

| Tokens | For |
| --- | --- |
| `--canvas`, `--surface`, `--raised` | The page behind the map, the docked plates and terminals, and hover and chips. |
| `--ink`, `--muted` | Words, and secondary words. |
| `--accent`, `--on-accent`, `--accent-line`, `--accent-wash` | The one accent, words on it, its borders, and its tint over an area such as a chosen row. |
| `--danger`, `--danger-wash` | Refusals, deleting, and a session that waits for you. |
| `--live`, `--ready`, `--quiet` | A session's state: active, ready, quiet. |
| `--line`, `--field`, `--scrim`, `--shadow` | Hairlines, inputs, the veil behind a sheet, and floating pieces' shadow. |
| `--primary-fill`, `--primary-ink`, `--primary-line` | The main button. Optional: the accent by default. |
| `--font-ui`, `--font-mono` | The interface's and the terminals' typefaces. A person's own choice replaces them. Themes name installed typefaces only; Deck bundles none. |
| `--font-display`, `--display-style`, `--display-size` | Names and headings: typeface, `normal` or `italic`, and a scale. |
| `--map-canvas`, `--map-ink`, `--map-accent`, `--map-link`, `--map-node`, `--map-faded`, `--map-live` | The map's colours, as six-digit hex, so a painter can add an alpha. |
| `--map-name`, `--map-top`, `--map-count` | Canvas fonts for a subdomain's name, a root domain's name, and a session count. |

## The painter

`map.js` exports a painter object by default. `ui/map.js` keeps the view, motion, hit testing, the focus ring's timing, names and counts, and calls the painter for the rest.

| Member | Role |
| --- | --- |
| `shape(body)` | Sets `body.core`, the domain's radius at zoom 1, and anything else the painter keeps on the body. Called when the domains change. |
| `backdrop(scene)`, `links(scene)`, `domains(scene)` | Paint each frame, in that order. |
| `ring(ctx, body, radius, scene)` | Optional: the focus ring. A plain circle by default. |
| `resize(ratio)` | Optional: called when the pixel ratio changes, to paint cached pieces again. |
| `reach` | How many times its size a domain's drawing reaches, so one just off screen is still painted. |
| `below` | `{ top, name }`: how far below a domain its name sits. |
| `dims` | Whether names dim with the domains a hover dims. |

A **body** is one domain as drawn: `domain`, `top` (a root domain), `count`, `warmth` (1 for today's work, falling to 0 over three months), `live` (a session runs there now), and on each frame `x`, `y`, `size` and `dim`.

A **scene** is one frame: `ctx`, `width`, `height`, `ratio`, `view`, `ink` (the map tokens), the `bodies` in view, the `links` in view as `{ from, to, second }` where `second` marks a subdomain's link to a further parent, `focus` and `hover`.

## Adding a theme

1. Copy `ui/themes/noema` to a new folder and add its name to `themes.json`.
2. Change the manifest, the tokens and the painter.
3. Choose it in Settings, under Appearance (⌘,).

Themes are part of the interface and run with its rights. Loading themes from outside the app is planned.
