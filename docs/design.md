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

The map or the terminal fills the window. A rail on the left switches between them.

| Part | Content |
| --- | --- |
| Map | Domains drawn as stars on a night sky, with pan and zoom. Size follows the session count. Colour follows recency, from blue-white to amber over three months. |
| Domain panel | The selected domain: name, parents, subdomains and sessions. |
| Session list | Open panes with their state, then recent assigned sessions. Search covers every assigned session. |
| Terminal | One pane, or two side by side or stacked. Other open panes keep running out of sight. |

- **Resizing.** The session list, the domain panel and the divider between two panes drag by their edges. The list can be hidden. Sizes are remembered.
- **Title bar.** On macOS the interface draws under the title bar, and its top edge drags and zooms the window.

## Map

- **Editing.** Add, rename, move, link, unlink and delete domains.
- **Import.** A folder path given as a name adds the folder and two levels inside it as domains.
- **Reassigning.** A session dragged from the panel onto another domain is assigned there too.

## Sessions

- **New session.** Choose a domain and a folder. The session is assigned at once. Claude Code stores a session at its first prompt, so one closed before that is dropped.
- **Add existing.** A picker over the agent's whole history assigns past sessions to the selected domain.
- **State.** An open pane is active when it wrote output in the last three seconds, and quiet otherwise.

## Terminal

- **Panes.** A pane runs Claude Code on a session, or the user's login shell. Deck has no chat interface.
- **Split.** Splitting right or down opens a shell beside the current pane, in the same folder. A session can also open beside another.
- **Closing.** A pane closes when its process exits, and closing a pane stops its process. One that fails within two seconds stays, so its output can be read. The session remains and can be reopened.
- **Theme.** Deck's terminal is dark. A light Claude Code theme is swapped for its dark counterpart in a pane, for that run only.
- **Icons.** A bundled Nerd Fonts symbol font draws prompt icons the terminal font lacks.
- **Other clients.** Deck does not guard against a session being open elsewhere. The agent handles that.

## Later

- **More agents.** Codex and OpenCode.
- **Settings.** Terminal font and theme.
- **Domain prompts.** A prompt per domain, inherited by sessions started in its subdomains.
- **Skills, system load and memory.**
- **Session links.** Fork, reference and handoff relations.
