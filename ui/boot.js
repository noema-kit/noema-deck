// Runs before the page is drawn, so Deck opens in the look of its last run
// rather than flashing the default. `theme.js` then puts on what the core keeps.
try {
  const root = document.documentElement;
  const kept = JSON.parse(localStorage.getItem("noema-deck-look"));
  if (kept) {
    document.getElementById("theme").setAttribute("href", `themes/${kept.theme}/theme.css`);
    Object.assign(root.dataset, { theme: kept.theme, mode: kept.mode });
    for (const [name, value] of Object.entries(kept.fonts)) if (value) root.style.setProperty(name, value);
  } else {
    root.dataset.mode = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
} catch {
  document.documentElement.dataset.mode = "light";
}
