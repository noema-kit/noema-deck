# Galaxy

A theme that draws the map as a night sky. Dark only. [Theme format](../../../docs/themes.md)

- **Domains** are stars of light: a white core, a bloom and four thin spikes. Size follows the session count, and root domains have the longest spikes.
- **Colour** follows recency through blackbody colours, blue-white for today's work cooling to amber over three months. A star with no sessions is dim blue.
- **Links** taper from the parent to the subdomain. Those touching the focused star turn gold.
- **Backdrop.** A faint band of glowing gas, from the lower left to the upper right, and three layers of far stars that drift at different speeds as the map pans.
- **Words** are warm starlight, names in the system's serif italic, with gold as the one accent.

## Drawing

- **Painted once, drawn many times.** Each kind and colour of star is painted once at a few sizes and then copied. The gas is worked out once, on first use, and the far stars are three repeating tiles.
- **Light adds up** where two stars overlap.
- **The gas** is summed as fractions on a coarse grid and only then turned into pixels with a little grain, because a glow this dark painted straight in a canvas's 256 levels shows as bands.
