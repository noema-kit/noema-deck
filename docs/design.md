# Design

Deck arranges Claude Code sessions on a universe of topic stars, and opens them in shell panes. [Architecture](architecture.md) says how it is built.

## Model

Deck stores three kinds of record. Sessions stay with the agent that created them, and Deck keeps no copy.

| Record | Content |
| --- | --- |
| Star | A stable ID, a name and a position on the map. |
| Link | A larger star and a smaller star. |
| Placement | A star and a session key: the agent's name and the session's own ID. |

- **Top stars.** A star with no larger star is a top star. There is no centre: a universe has a few, such as System, AI and PL.
- **Links.** Every link has a larger and a smaller end; equal stars are never linked. A star may have several larger stars. A link that would place a star under itself is refused.
- **Placements.** A session may sit on several stars. A session on no star is unsorted.
- **Counts.** A star counts the distinct sessions on it and on every star beneath it.
- **Deleting a star.** Its links and placements go with it. A smaller star left without a larger star moves up to the deleted star's larger stars, or becomes a top star. Sessions are never deleted.
- **Missing sessions.** A placed session the agent no longer lists is marked missing, with an offer to remove the placement.

## Universe

The map is a night sky, with pan and zoom. It starts empty.

- **Stars.** A star is drawn as light: a white core, a bloom and four thin spikes. Its count sets its size. Its latest session sets its colour, from blue-white today to dim amber after three months of quiet.
- **Names.** A name keeps one size at every zoom. Where two would overlap, the top star's or the fuller star's is shown.
- **Focus.** Selecting a star centres it, highlights its links and opens its sessions.
- **Editing.** Add a top star in empty space or a smaller star under a star. Rename, move, link by dragging from a larger star to a smaller one, unlink and delete.
- **Import.** A folder's path given in place of a name adds that folder as a star, with the folders inside it as smaller stars, two levels deep.
- **Motion.** A focus glides, a zoom eases and a pan coasts. With reduced motion set on the system, each is a jump.

## Sessions

- **Star panel.** The focused star's sessions, most recent first, optionally with those of smaller stars. Open a session or remove it from the star.
- **Unsorted tray.** Every session on no star. Search by title or folder, then drag sessions onto a star.
- **New session.** From a star, choose a folder. Deck starts Claude Code with a session ID it chose and places the session on that star. Claude Code stores a session at its first prompt, so one closed before that leaves the star.

## Shell

Deck has no chat interface. Opening a session opens a shell pane inside Deck's window that runs Claude Code in the session's folder, with the environment of the user's login shell.

- Two panes can sit side by side, and the map stays one key away.
- Closing a pane stops the Claude Code process. The session remains and can be reopened.
- Deck does not guard against a session being open in another client. The agent handles that.

## Later

- **More agents.** Codex and OpenCode.
- **Star prompts.** A prompt per star, inherited by sessions started on the stars beneath it.
- **Skills, system load and memory.** Browse agent skills, watch host load, and review Noema memory.
- **Session links.** Fork, reference and handoff relations between sessions.
