# Design

Deck organises Claude Code sessions by domain and runs them in terminal panes. [Architecture](architecture.md) says how it is built.

## Model

| Record | Content |
| --- | --- |
| Domain | A stable ID, a name and a position on the map. |
| Link | A parent domain and a subdomain. |
| Assignment | A domain and a session key: the agent's name and the session's own ID. |

- **Sessions** stay with the agent. Deck keeps no copy, and shows a session only once it is assigned.
- **Root domains** have no parent. There is no single root.
- **Links** never form a loop. A domain may have several parents.
- **Assignments.** A session may belong to several domains.
- **Counts.** A domain counts the distinct sessions in it and in its subdomains.
- **Deleting a domain** removes its links and assignments. A subdomain left without a parent moves up to the deleted domain's parents. Sessions are never deleted.
- **Missing sessions.** An assigned session the agent no longer lists is marked missing and can be removed.

## Window

The map or the terminal fills the window below one top bar. The bar holds the window's own buttons, the session list toggle, the switch between map and terminal, the name of what is in view, and the map's tools.

| Part | Content |
| --- | --- |
| Map | Domains drawn as stars on a night sky, with pan and zoom. Size follows the session count. Colour follows recency, from blue-white to amber over three months. |
| Domain panel | The selected domain: name, parents, subdomains and sessions. |
| Session list | Open panes with their state, then recent assigned sessions. Search covers every assigned session. |
| Terminal | Panes tiled in nested rows and columns. Other open panes keep running out of sight. |

- **Resizing.** The session list, the domain panel and the dividers between panes drag by their edges. The list can be hidden. Sizes are remembered.
- **Top bar.** On macOS the window's three buttons sit at its left, as in other desktop apps. Its empty stretches drag and zoom the window.

## Map

- **Editing.** Add, rename, move, link, unlink and delete domains.
- **Import.** A folder path given as a name adds the folder and two levels inside it as domains.
- **Arrange.** Lays the map out as one galaxy. Root domains gather on a small ring at the core, and each level of subdomains lies on a wider ring, so distance from the middle shows depth. Each root's branch keeps its own slice. Siblings sharing a subdomain sit side by side, and links draw related domains together while each keeps room for its name. The map glides to the new places and fits them on screen. One step undoes it.
- **Fit.** Shows every domain at once.
- **Reassigning.** A session dragged from the session list or the panel onto a domain is assigned there too.

## Sessions

- **New session.** Choose a domain, a folder and, optionally, a name. The name goes to Claude Code as `--name` and is written to the session once stored. The session is assigned at once. Claude Code stores a session at its first prompt, so one closed before that is dropped.
- **Add existing.** A picker over the agent's whole history assigns past sessions to the selected domain.
- **Titles.** Claude Code's own title, else the first prompt typed, else the first command run, such as `/login`.
- **Renaming.** A double click on a session's row or its pane's title renames it. Deck writes the record Claude Code's `/rename` writes, so both show the name. A session not yet stored keeps the name until it is.
- **State.** An open pane is active when it wrote output in the last three seconds, and quiet otherwise. It is ready when it fell quiet after output nobody saw: off screen, or with the window in the background. The terminal button in the top bar then carries a dot. Output in a pane's first five seconds is its own redraw and does not count.
- **Reopening.** The panes open when Deck stops reopen at the next start, behind the map, tiled as they were. A session no longer listed, or a folder that is gone, is skipped.

## Terminal

- **Panes.** A pane runs Claude Code on a session, or the user's login shell. Deck has no chat interface.
- **Split.** Splitting a pane right or down opens a shell beside or below that pane alone, in the same folder, so panes tile in nested rows and columns. A double click on a divider shares the room evenly.
- **Carrying.** A session from the list, or a pane by its header, drags into the terminal. Near a pane's edge it splits that pane on that side, and in the middle it takes the pane's place. An outline shows where it will land. Dropped on the map's empty sky, a session opens in the terminal.
- **Shell names.** A double click on a shell's title names it. The name is kept with the open panes.
- **Closing.** A pane closes when its process exits, and closing a pane stops its process. One that fails within two seconds stays, so its output can be read. The session remains and can be reopened.
- **Theme.** Deck's terminal is dark, on the panels' colour, with a matching sixteen-colour palette. A light Claude Code theme is swapped for its dark counterpart in a pane, for that run only.
- **Icons.** A bundled Nerd Fonts symbol font draws prompt icons the terminal font lacks.
- **Other clients.** Deck does not guard against a session being open elsewhere. The agent handles that.

## Later

- **More agents.** Codex and OpenCode.
- **Settings.** Terminal font and theme.
- **Domain prompts.** A prompt per domain, inherited by sessions started in its subdomains.
- **Skills, system load and memory.**
- **Session links.** Fork, reference and handoff relations.
