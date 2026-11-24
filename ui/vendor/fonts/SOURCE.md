# Typefaces

The interface's two typefaces, from two npm packages, and a font of icons for the terminal.

| File | Package | Path in package |
| --- | --- | --- |
| `newsreader-latin-wght-normal.woff2` | [`@fontsource-variable/newsreader` 5.3.0](https://www.npmjs.com/package/@fontsource-variable/newsreader/v/5.3.0) | `files/` |
| `newsreader-latin-wght-italic.woff2` | `@fontsource-variable/newsreader` 5.3.0 | `files/` |
| `newsreader.LICENSE.txt` | `@fontsource-variable/newsreader` 5.3.0 | `LICENSE` |
| `instrument-sans-latin-wght-normal.woff2` | [`@fontsource-variable/instrument-sans` 5.3.0](https://www.npmjs.com/package/@fontsource-variable/instrument-sans/v/5.3.0) | `files/` |
| `instrument-sans.LICENSE.txt` | `@fontsource-variable/instrument-sans` 5.3.0 | `LICENSE` |
| `symbols-nerd-font-mono.woff2` | [Nerd Fonts 3.4.0](https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.4.0), `NerdFontsSymbolsOnly` | `SymbolsNerdFontMono-Regular.ttf`, repacked as WOFF2 |
| `symbols-nerd-font.LICENSE.txt` | Nerd Fonts 3.4.0, `NerdFontsSymbolsOnly` | `LICENSE` |

- Newsreader names the stars and titles the panels: https://github.com/productiontype/Newsreader
- Instrument Sans sets everything else: https://github.com/Instrument/instrument-sans
- Symbols Nerd Font Mono draws the icons a shell prompt uses, when the terminal's own font lacks them: https://github.com/ryanoasis/nerd-fonts
- Licence: SIL Open Font License 1.1 for the two typefaces and MIT for the icon font, included in the three licence files.

Only the Latin subsets are bundled. Other scripts fall back to the system's fonts.
