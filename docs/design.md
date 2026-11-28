# Design

Deck sorts Claude Code, Codex and Antigravity sessions by domain and runs them in terminal panes. [Architecture](architecture.md) says how it is built.

## Principles

- **Agents own their sessions.** Deck keeps no copy of a session and adds no chat window. It stores only where each session belongs.
- **Opt in.** A session appears once it is assigned to a domain, so the map holds only work worth keeping.
- **One place for each rule.** The core decides everything. The interface sends requests and draws what comes back.
- **Fit in, don't take over.** Panes run the login shell with its own environment and say they are Deck's own terminal. The look uses installed fonts, follows the system's light and dark, and every shortcut can move.

## Model

| Record | Content |
| --- | --- |
| Domain | A stable ID, a name and a place on the map |
| Link | A parent domain and a subdomain |
| Assignment | A domain and a session, by agent and session ID |

- A domain may have several parents, and links never form a loop. Root domains have no parent, and there is no single root.
- A session may belong to several domains. A domain counts the distinct sessions in it and its subdomains.
- Deleting a domain removes its links and assignments, and moves its orphaned subdomains up to its parents. Sessions are never deleted.
- A session the agent no longer lists is marked missing and can be removed.

## Window

The session list runs down the left, and the map or the terminal fills the rest. As in Claude's and Codex's desktop apps there is no top bar: the window's buttons, the list toggle and the map/terminal switch sit together at the top left. The empty parts of the headers drag the window.

| Part | Content |
| --- | --- |
| Session list | New session, new shell and search; the open panes; then every domain as a folder holding its subdomains and its sessions, newest first, each marked with its agent. Both parts fold away. Settings sit at the foot. |
| Map | Domains and links, with pan and zoom. Size follows the session count, colour follows recency over three months. |
| Domain panel | The selected domain's name, parents, subdomains and sessions. |
| Terminal | Panes tiled in rows and columns. Panes out of sight keep running. |

- **Domain tree.** A domain is a folder, empty while it has no session. A session carries its agent's logo, Claude's, OpenAI's or Antigravity's, and its time; a running one shows its state in the logo's corner. A shell carries a terminal window. A domain with two parents appears under both. A search unfolds everything and keeps what matches. Folding is remembered.
- **Sizes.** The list, the panel and the dividers between panes drag by their edges and are remembered.
- **Zoom.** ⌘= and ⌘− draw the whole window larger or smaller, from 67% to 200%, and ⌘0 puts it back, as in Claude's desktop app. The zoom is remembered. In a browser, the browser zooms its own page.

## Map

- **Editing.** Add, rename, move, link, unlink and delete domains. Shift-drag links a parent to a subdomain. A folder path given as a name imports the folder and two levels inside it.
- **Arrange.** Root domains gather at the centre, and each level of subdomains lies on a wider ring, so distance shows depth. Each root keeps its own slice, and linked domains are drawn together. One step undoes it.
- **Assigning.** A session dropped on a domain, on the map or in the list, joins it.

## Sessions

- **New session.** Choose an agent, a domain, a folder and an optional name. The session is assigned at once, and the agent is remembered for next time. Claude Code takes the name as `--name`. Codex and Antigravity choose the session's ID themselves, so Deck follows the first new session of that agent in that folder and names it once the agent has stored it. Each stores a session at its first prompt, so one closed before then is dropped, as is one still waiting when Deck quit.
- **Add existing.** A picker over every agent's whole history, which can show one agent only.
- **Titles.** The agent's own title or name, else the first prompt, else, for Claude Code, the first command, such as `/login`.
- **Renaming.** A double click on a row or a pane's title. Deck writes the name where the agent keeps it, as its own renaming does, so both agree.
- **State.** A pane is active while it writes output and quiet after three seconds. It is ready when it fell quiet with output nobody saw; the terminal button then carries a dot.
- **Reopening.** Open panes come back at the next start, tiled as they were.

## Terminal

- **Panes** run an agent on a session, or the login shell. Splitting a pane opens a shell beside or below it, in the same folder.
- **Carrying.** Drag a session or a pane's title into the terminal. Near a pane's edge it splits that pane; in the middle it replaces it.
- **Closing.** A pane closes when its process exits. One that fails within two seconds stays open, so its output can be read.
- **Colours.** Panes take the theme's sixteen-colour palette for the mode in view. On a light ground, text too faint to read is darkened, as VS Code does; on a dark ground, programs' colours show exactly. Programs that ask, such as Neovim, are told when Deck turns light or dark. Claude Code runs with the light or dark theme that matches.
- **Environment.** A pane gets the login shell's environment, a UTF-8 locale if the profile sets none, and `TERM_PROGRAM=NoemaDeck`. It drops the variables of the terminal Deck was started from, which would make programs speak that terminal's dialect.
- **Fonts.** Deck bundles no typeface. A prompt that draws icons needs a font that has them, such as a Nerd Font; the font menu marks these.

## Settings

Settings open with ⌘, or from the foot of the list, which shows your name and picture as Claude's desktop app does. A change shows at once, and the store keeps it.

| Page | Content |
| --- | --- |
| Profile | Your name and picture, lifetime figures and a year of token activity, as Codex's profile shows them |
| Appearance | Theme, colour mode (light, dark or follow the system) and interface font |
| Terminal | Font, size and line height, with a preview |
| Shortcuts | Every keyboard shortcut. Click a key, then press the new one. A key in use is refused, and Reset puts one back. |
| Usage | Overview: each agent's plan limits. Analytics: the tokens the agents used on this machine |
| About | Version, source and licence |

**Profile.** There are no accounts. The name and picture stay in the store on this machine. The figures cover every day the agents' records hold: lifetime tokens, the peak day, days active, and the longest and current streaks of days in use.

**Usage** is in two tabs, as in Codex's app.

- **Overview.** A card per agent with a bar for each window of its plan, how much is used and when it starts again, as the agent itself reports it. Deck never reads an agent's login. Codex records its limits with every reply. Claude Code and Antigravity are asked through their own `/usage` command, which uses no model, when Deck starts and when the page opens, at most every two minutes. A card shows the last answer until the new one comes.
- **Analytics.** The last 7, 30 or 90 days, or a year a week to a bar, for every agent or one: tokens, replies, the share of input read from the prompt cache, and output; tokens by day stacked by agent, with each agent's share; then the sessions or the models that used the most. A session the agent lists opens on a click.

Each reply counts once. Claude Code writes a reply in parts and copies earlier replies into a resumed session, so a reply counts by its message ID. A forked Codex session begins with a copy of the events before the fork, so an event counts by its time and running total. Deck estimates no cost: prices change, and a plan is not billed by the token.

**Noema**, the default theme, takes its colours from the Noema Kit mark and draws the map as an ink graph: a dot per domain, deeper for recent work, an open ring when empty. **Galaxy** draws domains as stars on a night sky; [its README](../ui/themes/galaxy/README.md) describes it. [Themes](themes.md) says how to make one.

## Later

- More agents, such as OpenCode
- Themes loaded from outside the app
- A prompt per domain, inherited by its subdomains
- Skills, system load and memory per domain
- Links between sessions: fork, reference and handoff
